//! The AI review store — every reviewer process Orbit owns, across all
//! workspaces, plus the durable history of finished runs.
//!
//! A review is **not** a second turn in the user's session: each run gets its
//! own read-only pi process scoped to Ask mode (see
//! [`BundledExtensions::spawn_reviewer`]), and its events are drained here,
//! mirroring the parked-session pattern. Its final answer is read from a
//! private [`Transcript`] and parsed by [`crate::ai_review::parse_report`].
//!
//! Each run is pinned to the workspace it was started from, so a review of
//! Project A keeps going after the user switches to Project B. At most
//! [`MAX_CONCURRENT_REVIEWS`] review processes run at once; the rest wait in a
//! queue. Finished runs persist to `~/.orbit-pi/reviews.json` (see
//! [`crate::reviews`]).
//!
//! The Review pane mirrors a single run (the focused one, else the newest for
//! the current workspace). The Review page will render the whole store.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::ai_review::{self as model, Report, ReviewKind, ReviewStatus};
use crate::git;
use crate::reviews::{self, ReviewRun, ReviewRunConfig, RunStatus};

/// How many reviewer processes may run at once. Each one is a full pi process,
/// so this is deliberately small; the rest of the queue waits.
pub(super) const MAX_CONCURRENT_REVIEWS: usize = 2;

/// The dedicated reviewer process for one run. Kept out of `lives`: it is not
/// a user session and must never appear in the sidebar or its notifications.
pub(super) struct ReviewTask {
    /// The workspace this run belongs to, so a queued run launches against the
    /// right cwd after the user has moved on.
    pub(super) workspace: PathBuf,
    pub(super) client: PiClient,
    /// Accumulates the reviewer's streamed answer; the final text is read with
    /// `last_response_text` on settle.
    pub(super) transcript: Transcript,
}

/// Every review run the app owns: the live tasks and the finished history.
/// `runs` is newest-first; `tasks` holds only running ones.
pub(super) struct ReviewStore {
    runs: Vec<ReviewRun>,
    tasks: HashMap<u64, ReviewTask>,
    next_id: u64,
    /// Whether the history has unsaved changes.
    dirty: bool,
}

impl Default for ReviewStore {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            tasks: HashMap::new(),
            next_id: 1,
            dirty: false,
        }
    }
}

impl ReviewStore {
    /// Load the persisted history. Live runs never survive a restart (their
    /// processes are gone), so only finished ones come back; ids continue from
    /// the highest restored one.
    pub fn load() -> Self {
        let mut runs = reviews::load();
        let next_id = runs.iter().map(|run| run.id).max().unwrap_or(0) + 1;
        reviews::prune(&mut runs);
        Self {
            runs,
            tasks: HashMap::new(),
            next_id,
            dirty: false,
        }
    }

    /// Every run, newest first — the Review page's history list.
    pub fn runs(&self) -> &[ReviewRun] {
        &self.runs
    }

    /// How many runs are queued or running, for badges and menu rows.
    pub fn active_count(&self) -> usize {
        self.runs.iter().filter(|run| run.is_active()).count()
    }

    /// The newest run for `workspace`, active or finished.
    pub fn latest_for(&self, workspace: &Path) -> Option<&ReviewRun> {
        self.runs.iter().find(|run| run.workspace == workspace)
    }

    /// The run with `id`.
    pub fn get(&self, id: u64) -> Option<&ReviewRun> {
        self.runs.iter().find(|run| run.id == id)
    }

    /// Insert a queued run and return its id.
    pub fn push(
        &mut self,
        workspace: PathBuf,
        head: Option<String>,
        kind: ReviewKind,
        config: ReviewRunConfig,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.runs.insert(
            0,
            ReviewRun {
                id,
                workspace,
                head,
                kind,
                config,
                status: RunStatus::Queued,
                started_at: now_secs(),
                finished_at: None,
                progress: Default::default(),
                report: None,
            },
        );
        self.dirty = true;
        reviews::prune(&mut self.runs);
        id
    }

    /// Attach a spawned process to a run and mark it running.
    pub fn attach(&mut self, id: u64, task: ReviewTask) {
        if let Some(run) = self.run_mut(id) {
            run.status = RunStatus::Running;
        }
        self.tasks.insert(id, task);
        self.dirty = true;
    }

    /// Whether a reviewer process is live for `id`.
    pub fn has_task(&self, id: u64) -> bool {
        self.tasks.contains_key(&id)
    }

    /// The live task for `id`.
    pub fn task(&self, id: u64) -> Option<&ReviewTask> {
        self.tasks.get(&id)
    }

    /// The live task for `id`, mutably.
    pub fn task_mut(&mut self, id: u64) -> Option<&mut ReviewTask> {
        self.tasks.get_mut(&id)
    }

    /// Ids of the queued runs, oldest first — launch order.
    pub fn queued_ids(&self) -> Vec<u64> {
        self.runs
            .iter()
            .rev()
            .filter(|run| run.status == RunStatus::Queued)
            .map(|run| run.id)
            .collect()
    }

    /// How many more reviewer processes may start right now.
    pub fn free_slots(&self) -> usize {
        MAX_CONCURRENT_REVIEWS.saturating_sub(self.tasks.len())
    }

    /// Claim a queued run for launch, marking it running before the async diff
    /// collection, so the queue cannot launch it twice.
    pub fn claim(&mut self, id: u64) {
        if let Some(run) = self.run_mut(id) {
            run.status = RunStatus::Running;
        }
    }

    /// Finish a run: record its report (if any), drop its process, persist.
    pub fn finish(&mut self, id: u64, status: RunStatus, report: Option<Report>) {
        self.tasks.remove(&id);
        if let Some(run) = self.run_mut(id) {
            run.finished_at = Some(now_secs());
            run.status = status;
            if report.is_some() {
                run.report = report;
            }
        }
        self.save();
    }

    /// Update coarse progress for a running run.
    pub fn note_progress(&mut self, id: u64, progress: crate::reviews::RunProgress) {
        if let Some(run) = self.run_mut(id) {
            run.progress = progress;
        }
    }

    /// Cancel a run. A queued run never had a process; a running one is
    /// aborted and its process dropped.
    pub fn cancel(&mut self, id: u64) {
        if let Some(task) = self.tasks.remove(&id) {
            let _ = task.client.send(CommandBody::Abort);
        }
        if let Some(run) = self.run_mut(id) {
            run.finished_at = Some(now_secs());
            run.status = RunStatus::Cancelled;
        }
        self.save();
    }

    /// Drop every run for `workspace` (the folder left the app). Active runs
    /// are aborted first.
    pub fn forget_workspace(&mut self, workspace: &Path) {
        let ids: Vec<u64> = self
            .runs
            .iter()
            .filter(|run| run.workspace == workspace)
            .map(|run| run.id)
            .collect();
        for id in ids {
            self.cancel(id);
        }
        self.runs.retain(|run| run.workspace != workspace);
        self.save();
    }

    /// Write the history when something changed.
    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        reviews::save(&self.runs);
    }

    fn run_mut(&mut self, id: u64) -> Option<&mut ReviewRun> {
        self.runs.iter_mut().find(|run| run.id == id)
    }
}

/// Unix seconds, the store's timestamp unit.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

impl OrbitApp {
    /// Start a review and return its id. The run is queued immediately (so it
    /// appears in run lists at once); the diff, when the target needs one, is
    /// collected off-thread before the process spawns.
    pub(super) fn start_review(&mut self, kind: ReviewKind, cx: &mut Context<Self>) -> Option<u64> {
        let Some(cwd) = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        else {
            self.reviews_error = Some(tr!("ai_review.no_workspace"));
            cx.notify();
            return None;
        };
        let config = self.review_config();
        let head = git::head_revision(&cwd);
        let id = self.reviews.push(cwd, head, kind, config);
        self.focused_review = Some(id);
        self.reviews_error = None;
        cx.notify();
        self.pump_review_queue(cx);
        Some(id)
    }

    /// Launch queued runs while reviewer slots are free. Each launch collects
    /// its diff off-thread first (the diff-backed kinds), then spawns.
    pub(super) fn pump_review_queue(&mut self, cx: &mut Context<Self>) {
        while self.reviews.free_slots() > 0 {
            let Some(id) = self.reviews.queued_ids().into_iter().next() else {
                break;
            };
            let Some(run) = self.reviews.get(id) else {
                break;
            };
            let workspace = run.workspace.clone();
            let kind = run.kind.clone();
            // Claim the slot synchronously: the diff collection below is async,
            // and this loop must not launch the same run twice.
            self.reviews.claim(id);
            if !kind.is_diff() {
                self.launch_review(id, None, cx);
                continue;
            }
            // `Changes` follows the pane's selected Review source; the other
            // diff kinds resolve their range from the kind itself.
            let source = (kind == ReviewKind::Changes)
                .then(|| self.sidepane.read(cx).ai_review_source().0);
            let session_id = self.session_id.clone();
            cx.spawn(async move |this, cx| {
                let collected = cx
                    .background_executor()
                    .spawn(async move {
                        match source {
                            Some(source) => {
                                git::collect_review_diff(&workspace, source, session_id.as_deref())
                            }
                            None => git::collect_kind_diff(&workspace, &kind),
                        }
                    })
                    .await;
                let _ = this.update(cx, |app, cx| {
                    app.launch_review(id, Some(collected.map(|diff| diff.patch)), cx);
                });
            });
        }
    }

    /// Spawn the reviewer process for a run whose diff (if any) is collected,
    /// then send `new_session` + the prompt. A collection failure fails the run
    /// before any process exists.
    pub(super) fn launch_review(
        &mut self,
        id: u64,
        patch: Option<Result<String, String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.reviews.get(id) else {
            return;
        };
        let workspace = run.workspace.clone();
        let kind = run.kind.clone();
        let patch = match patch {
            Some(Err(error)) => {
                self.reviews.finish(id, RunStatus::Failed(error), None);
                cx.notify();
                return;
            }
            Some(Ok(patch)) => Some(patch),
            None => None,
        };
        let prompt = self.review_prompt(&kind, patch.as_deref(), cx);
        let client = match self.extensions.spawn_reviewer(&workspace) {
            Ok(client) => client,
            Err(error) => {
                self.reviews.finish(
                    id,
                    RunStatus::Failed(tr!("ai_review.spawn_failed", error = error.to_string())),
                    None,
                );
                cx.notify();
                return;
            }
        };
        let _ = client.send(CommandBody::NewSession);
        let _ = client.send(CommandBody::Prompt {
            message: prompt,
            images: None,
            streaming_behavior: None,
        });
        self.reviews.attach(
            id,
            ReviewTask {
                workspace,
                client,
                transcript: Transcript::new(),
            },
        );
        cx.notify();
    }

    /// Build a run's prompt. `patch` is the collected diff, absent for the
    /// snapshot kinds.
    fn review_prompt(&self, kind: &ReviewKind, patch: Option<&str>, cx: &Context<Self>) -> String {
        let embedded = |patch: Option<&str>| {
            let (patch, truncated) = model::cap_patch(patch.unwrap_or_default());
            (patch, truncated)
        };
        match kind {
            ReviewKind::Changes => {
                let label = self.sidepane.read(cx).ai_review_source_label();
                let (patch, truncated) = embedded(patch);
                model::build_changes_prompt(&label, &patch, truncated)
            }
            ReviewKind::Uncommitted => {
                let (patch, truncated) = embedded(patch);
                let context = tr!("ai_review.context_uncommitted");
                model::build_revision_prompt(&context, &patch, truncated)
            }
            ReviewKind::Branch { base } => {
                let (patch, truncated) = embedded(patch);
                let context = tr!("ai_review.context_branch", base = base.clone());
                model::build_revision_prompt(&context, &patch, truncated)
            }
            ReviewKind::Commit { sha, title } => {
                let (patch, truncated) = embedded(patch);
                let short: String = sha.chars().take(7).collect();
                let context = if title.is_empty() {
                    tr!("ai_review.context_commit", sha = short)
                } else {
                    tr!(
                        "ai_review.context_commit_titled",
                        sha = short,
                        title = title.clone()
                    )
                };
                model::build_revision_prompt(&context, &patch, truncated)
            }
            ReviewKind::Files { paths } => model::build_files_prompt(paths),
            ReviewKind::Project => model::build_project_prompt(),
        }
    }

    /// Drain every live reviewer each heartbeat. A settled run parses its final
    /// answer and finishes; finishing frees a slot for the queue.
    pub(super) fn tick_reviews(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .reviews
            .runs()
            .iter()
            .filter(|run| self.reviews.has_task(run.id))
            .map(|run| run.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut outcomes: Vec<(u64, RunStatus, Option<Report>)> = Vec::new();
        let mut touched = false;
        for id in ids {
            let Some(task) = self.reviews.task(id) else {
                continue;
            };
            let events = task.client.drain_events();
            if events.is_empty() {
                continue;
            }
            touched = true;
            let mut settled = false;
            let mut failure: Option<String> = None;
            for event in &events {
                match event {
                    Event::AgentSettled => settled = true,
                    Event::ProcessExited => failure = Some(tr!("ai_review.process_exited")),
                    // The reviewer has no UI surface: never leave it blocked on
                    // a dialog. Cancel so the run can settle (or fail fast).
                    Event::ExtensionUiRequest { id, .. } => {
                        let _ = task.client.respond_dialog(
                            id.as_str(),
                            serde_json::json!({
                                "type": "extension_ui_response",
                                "id": id,
                                "cancelled": true
                            }),
                        );
                    }
                    // Persist the read-only scope against the real session id
                    // so a resume stays read-only even without the env var. The
                    // prompt is already scoped by `ORBIT_WORKFLOW_MODE`.
                    Event::Response { command, data, .. }
                        if command.as_str() == "new_session" =>
                    {
                        if let Some(session) = data
                            .as_ref()
                            .and_then(|data| data.get("sessionId"))
                            .and_then(Value::as_str)
                        {
                            crate::workflow::persist_for(session, WorkflowMode::Ask);
                        }
                    }
                    // A settled assistant message carrying an error fails the
                    // run; the reviewer's answer never arrived.
                    Event::MessageEnd { value } => {
                        let message = value.get("message").unwrap_or(value);
                        if let Some(error) = transcript::message_error(message) {
                            failure = Some(error);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(task) = self.reviews.task_mut(id) {
                for event in events {
                    task.transcript.apply_event(&event);
                }
            }
            if let Some(error) = failure {
                outcomes.push((id, RunStatus::Failed(error), None));
                continue;
            }
            if settled {
                match self
                    .reviews
                    .task(id)
                    .and_then(|task| task.transcript.last_response_text())
                    .map(|(_, text)| model::parse_report(&text))
                {
                    Some(report) if !report.is_empty() => {
                        outcomes.push((id, RunStatus::Completed, Some(report)));
                    }
                    _ => outcomes.push((id, RunStatus::Failed(tr!("ai_review.empty_answer")), None)),
                }
            }
        }
        if outcomes.is_empty() {
            if touched {
                cx.notify();
            }
            return;
        }
        for (id, status, report) in outcomes {
            self.reviews.finish(id, status, report);
        }
        self.pump_review_queue(cx);
        cx.notify();
    }

    /// Cancel one run by id.
    pub(super) fn cancel_review(&mut self, id: u64, cx: &mut Context<Self>) {
        self.reviews.cancel(id);
        self.pump_review_queue(cx);
        cx.notify();
    }

    /// Cancel the run the pane is showing (its Stop button).
    pub(super) fn cancel_focused_review(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.focused_review_id() {
            self.cancel_review(id, cx);
        }
    }

    /// The run the pane is showing: the focused one while it exists, else the
    /// newest for the current workspace.
    pub(super) fn focused_review_id(&self) -> Option<u64> {
        if let Some(id) = self
            .focused_review
            .filter(|id| self.reviews.get(*id).is_some())
        {
            return Some(id);
        }
        let workspace = self.current_workspace.clone()?;
        self.reviews.latest_for(&workspace).map(|run| run.id)
    }

    /// Forget the review history for a workspace (the folder left the app).
    pub(super) fn forget_reviews(&mut self, workspace: &Path) {
        self.reviews.forget_workspace(workspace);
        if self
            .focused_review
            .and_then(|id| self.reviews.get(id))
            .is_none()
        {
            self.focused_review = None;
        }
    }

    /// The snapshot the Review pane renders (synced each frame). Mirrors the
    /// focused run; `None` when the workspace has no runs yet.
    pub(super) fn ai_review_snapshot(&self) -> crate::sidepane::AiReviewSnapshot {
        let Some(run) = self.focused_review_id().and_then(|id| self.reviews.get(id)) else {
            return crate::sidepane::AiReviewSnapshot::default();
        };
        crate::sidepane::AiReviewSnapshot {
            kind: Some(run.kind.clone()),
            status: match &run.status {
                RunStatus::Queued | RunStatus::Running => ReviewStatus::Running,
                RunStatus::Completed => ReviewStatus::Done,
                RunStatus::Failed(error) => ReviewStatus::Failed(error.clone()),
                RunStatus::Cancelled => ReviewStatus::Idle,
            },
            report: run.report.clone(),
        }
    }

    /// The pane's opener callback: starts or cancels a review on behalf of the
    /// pane's sparkles menu without the pane reaching back into the app.
    pub(super) fn ai_review_opener(&self, cx: &Context<Self>) -> crate::sidepane::AiReviewAction {
        let this = cx.weak_entity();
        Rc::new(move |request, _window, cx| {
            this.update(cx, |app, cx| match request {
                crate::sidepane::AiReviewRequest::Start(kind) => {
                    app.start_review(kind, cx);
                }
                crate::sidepane::AiReviewRequest::Cancel => app.cancel_focused_review(cx),
            })
            .ok();
        })
    }

    /// The model/thinking the next review starts on: the session default, so
    /// the common case needs no picker interaction.
    fn review_config(&self) -> ReviewRunConfig {
        ReviewRunConfig {
            provider: self.session_default.provider.clone(),
            model: self.session_default.model_id.clone(),
            thinking: self.session_default.thinking.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind() -> ReviewKind {
        ReviewKind::Uncommitted
    }

    #[test]
    fn push_queues_and_assigns_increasing_ids() {
        let mut store = ReviewStore::default();
        let first = store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        let second = store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        assert_eq!((first, second), (1, 2));
        assert_eq!(store.runs().len(), 2);
        // Newest first.
        assert_eq!(store.runs()[0].id, second);
        assert_eq!(store.runs()[1].id, first);
        assert_eq!(store.active_count(), 2);
        assert_eq!(store.queued_ids(), vec![first, second]);
    }

    #[test]
    fn free_slots_start_at_the_cap() {
        let store = ReviewStore::default();
        assert_eq!(store.free_slots(), MAX_CONCURRENT_REVIEWS);
    }

    #[test]
    fn latest_for_finds_only_its_workspace() {
        let mut store = ReviewStore::default();
        let a = store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        let b = store.push(PathBuf::from("/b"), None, kind(), ReviewRunConfig::default());
        assert_eq!(store.latest_for(Path::new("/a")).map(|run| run.id), Some(a));
        assert_eq!(store.latest_for(Path::new("/b")).map(|run| run.id), Some(b));
        assert!(store.latest_for(Path::new("/c")).is_none());
    }

    #[test]
    fn finish_records_the_report_and_frees_the_slot() {
        let mut store = ReviewStore::default();
        let id = store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        let report = Report::default();
        store.finish(id, RunStatus::Completed, Some(report.clone()));
        let run = store.get(id).expect("run exists");
        assert_eq!(run.status, RunStatus::Completed);
        assert!(run.finished_at.is_some());
        assert_eq!(run.report.as_ref(), Some(&report));
        assert_eq!(store.free_slots(), MAX_CONCURRENT_REVIEWS);
    }

    #[test]
    fn cancel_marks_the_run_cancelled() {
        let mut store = ReviewStore::default();
        let id = store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        store.cancel(id);
        assert_eq!(
            store.get(id).map(|run| &run.status),
            Some(&RunStatus::Cancelled)
        );
        assert!(store.queued_ids().is_empty());
    }

    #[test]
    fn forget_workspace_drops_only_that_workspaces_runs() {
        let mut store = ReviewStore::default();
        store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        let b = store.push(PathBuf::from("/b"), None, kind(), ReviewRunConfig::default());
        store.forget_workspace(Path::new("/a"));
        assert_eq!(store.runs().len(), 1);
        assert_eq!(store.runs()[0].id, b);
    }

    #[test]
    fn restored_history_continues_the_id_sequence() {
        // A store whose persisted history already used id 9 must issue 10 next.
        let runs = vec![ReviewRun {
            id: 9,
            workspace: PathBuf::from("/a"),
            head: None,
            kind: ReviewKind::Project,
            config: ReviewRunConfig::default(),
            status: RunStatus::Completed,
            started_at: 1,
            finished_at: Some(2),
            progress: Default::default(),
            report: None,
        }];
        let next_id = runs.iter().map(|run| run.id).max().unwrap_or(0) + 1;
        let mut store = ReviewStore {
            runs,
            tasks: HashMap::new(),
            next_id,
            dirty: false,
        };
        let id = store.push(PathBuf::from("/a"), None, kind(), ReviewRunConfig::default());
        assert_eq!(id, 10);
    }
}
