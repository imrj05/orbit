//! Worktrees page and controller.
//!
//! The feature surface for Git worktrees, driven entirely by
//! [`crate::worktree::WorktreeManager`]: discovery for the active repository,
//! creating a worktree from a new or existing branch, opening one as an
//! ordinary Orbit workspace, renaming/moving/locking/removing rows, and the
//! optional setup script. A worktree is a workspace like any other once
//! opened — Explorer, Git, Terminal, Review, and the agent all follow
//! `current_workspace`, so this module never duplicates their plumbing.
//!
//! Mutations are serialized per repository (`worktree_mutation_repo`) and every
//! one of them ends in a fresh [`OrbitApp::refresh_worktrees`], so the list,
//! the active workspace, and the UI reconcile after Git's answer — never
//! against a stale picture.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{AnyElement, MouseDownEvent, MouseUpEvent};

use super::helpers::*;
use super::*;
use crate::theme::tokens::{
    context_menu, input, list_item, modal, panel, picker, popover, ButtonSize, DynamicSpacing,
    IconSize, Radius, RaisedExt, TextSize,
};
use crate::worktree::{self, Worktree, WorktreeError, WorktreeManager};
use crate::worktree_setup;

/// Watcher-driven re-list throttle while the page is open (§30): a burst of
/// file saves must not run `git worktree list` per event.
const WATCH_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
/// Page column width; matches the Settings body.
const PAGE_MAX_W: f32 = 720.;
/// The one-time "How worktrees work" card's key in `hints.json`.
pub(super) const WORKTREE_HOWTO_HINT_KEY: &str = "worktree_howto_dismissed";

impl OrbitApp {
    // ── page lifecycle ─────────────────────────────────────────────────

    /// Open the Worktrees page and load the repository's worktrees.
    pub(super) fn open_worktrees(&mut self, cx: &mut Context<Self>) {
        self.worktrees_open = true;
        // One main-area feature at a time (Git / Usage / Files / Review).
        if self.git_open {
            self.close_git(cx);
        }
        if self.usage_open {
            self.close_usage(cx);
        }
        self.session_details_open = false;
        self.settings_open = false;
        self.close_files(cx);
        self.sidepane.update(cx, |pane, cx| pane.close(cx));
        self.refresh_worktrees(cx);
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// Leave the Worktrees page.
    pub(super) fn close_worktrees(&mut self, cx: &mut Context<Self>) {
        self.worktrees_open = false;
        self.worktree_menu = None;
        self.worktree_advanced_open = false;
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// Palette / context-menu entry: toggle the page.
    pub(super) fn on_open_worktrees(
        &mut self,
        _: &crate::OpenWorktrees,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktrees_open {
            self.close_worktrees(cx);
        } else {
            self.open_worktrees(cx);
        }
    }

    /// A manager for the active workspace, carrying the cached main root so
    /// path resolution never runs Git on the UI thread.
    pub(super) fn worktree_manager(&self) -> Option<WorktreeManager> {
        let repo = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())?;
        let mut manager = WorktreeManager::new(repo).with_config(self.worktree_config.clone());
        if let Some(root) = &self.worktree_repo_root {
            manager = manager.with_root(root.clone());
        }
        Some(manager)
    }

    /// The main worktree root, from the last list when known.
    fn worktree_root_hint(&self) -> Option<PathBuf> {
        self.worktree_repo_root.clone()
    }

    /// Display label for the active repository.
    fn worktree_repo_label(&self) -> String {
        self.worktree_root_hint()
            .or_else(|| self.current_workspace.clone())
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_default()
    }

    /// Re-list the active repository's worktrees. Safe to call at any time;
    /// while a list is in flight this remembers to run another when it
    /// settles, so a workspace switch cannot be served stale rows.
    pub(super) fn refresh_worktrees(&mut self, cx: &mut Context<Self>) {
        if self.worktrees_busy {
            self.worktree_refresh_pending = true;
            return;
        }
        let Some(manager) = self.worktree_manager() else {
            self.worktrees.clear();
            return;
        };
        self.worktrees_busy = true;
        self.worktree_refresh_pending = false;
        self.worktree_fetch = self.worktree_fetch.wrapping_add(1);
        let fetch = self.worktree_fetch;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let root = manager.repository_root().ok();
                    let list = manager.list();
                    (root, list)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktrees_busy = false;
                let stale = app.worktree_fetch != fetch;
                if !stale {
                    match result {
                        (root, Ok(worktrees)) => {
                            if root.is_some() {
                                app.worktree_repo_root = root;
                            }
                            app.worktrees = worktrees;
                            app.worktrees_error = None;
                        }
                        (_, Err(err)) => {
                            app.worktrees.clear();
                            app.worktrees_error = Some(err.to_string());
                        }
                    }
                }
                let rerun = app.worktree_refresh_pending || stale;
                app.worktree_refresh_pending = false;
                if rerun {
                    app.refresh_worktrees(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Watcher-driven re-list, throttled (§30).
    pub(super) fn refresh_worktrees_soon(&mut self, cx: &mut Context<Self>) {
        if !self.worktrees_open || self.worktrees_busy {
            return;
        }
        let now = Instant::now();
        if self.worktree_refresh_due.is_some_and(|due| now < due) {
            return;
        }
        self.worktree_refresh_due = Some(now + WATCH_REFRESH_INTERVAL);
        self.refresh_worktrees(cx);
    }

    /// The worktree row whose path matches `path`, if it is listed.
    fn listed_worktree(&self, path: &Path) -> Option<&Worktree> {
        self.worktrees
            .iter()
            .find(|worktree| worktree::same_path(&worktree.path, path))
    }

    /// Whether `path` is the active workspace.
    fn worktree_is_active(&self, path: &Path) -> bool {
        self.current_workspace
            .as_deref()
            .is_some_and(|active| worktree::same_path(active, path))
    }

    /// Discover the missing worktree's owner only among known workspaces. Git
    /// must identify exactly one main checkout; a path alone cannot do that.
    pub(super) fn find_missing_worktree_local(
        &mut self,
        missing: PathBuf,
        session_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let mut candidates = self.workspaces.clone();
        candidates.extend(self.current_workspace.clone());
        candidates.extend(self.worktree_repo_root.clone());
        cx.spawn(async move |this, cx| {
            let target = missing.clone();
            let local = cx
                .background_executor()
                .spawn(async move { missing_worktree_local(&target, &candidates) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.current_session_path.as_ref() != Some(&session_path) {
                    return;
                }
                if let Some(WorktreeDialog::Unavailable { path, local: found }) =
                    &mut app.worktree_dialog
                {
                    if *path == missing {
                        *found = local;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Recheck Git's ownership before leaving the unavailable session. The
    /// action creates a new task; it never rewrites or resumes the old one.
    fn start_task_in_missing_worktree_local(&mut self, cx: &mut Context<Self>) {
        let Some(WorktreeDialog::Unavailable {
            path,
            local: Some(local),
        }) = self.worktree_dialog.clone()
        else {
            return;
        };
        let Some(session_path) = self.current_session_path.clone() else {
            return;
        };
        let mut candidates = self.workspaces.clone();
        candidates.push(local.clone());
        cx.spawn(async move |this, cx| {
            let target = path.clone();
            let candidate = local.clone();
            let verified = cx.background_executor().spawn(async move {
                missing_worktree_local(&target, &candidates)
                    .is_some_and(|owner| worktree::same_path(&owner, &candidate))
            }).await;
            let _ = this.update(cx, |app, cx| {
                if app.current_session_path.as_ref() != Some(&session_path)
                    || !matches!(&app.worktree_dialog, Some(WorktreeDialog::Unavailable { path: current, local: Some(root) }) if *current == path && *root == local)
                {
                    return;
                }
                if verified && local.is_dir() {
                    app.worktree_dialog = None;
                    app.open_worktree(&local, cx);
                } else {
                    if let Some(WorktreeDialog::Unavailable { local, .. }) = &mut app.worktree_dialog {
                        *local = None;
                    }
                    app.worktree_dialog_error = Some(tr!("worktree.unavailable.local_unavailable"));
                }
                cx.notify();
            });
        }).detach();
    }

    // ── clone flow ─────────────────────────────────────────────────────

    pub(super) fn open_clone_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.worktree_dialog = Some(WorktreeDialog::Clone);
        self.worktree_dialog_error = None;
        self.clone_window = Some(window.window_handle());
        self.clone_parent = None;
        self.clone_url_input.update(cx, |input, cx| input.clear(cx));
        self.clone_name_input
            .update(cx, |input, cx| input.clear(cx));
        self.clone_url_input.read(cx).focus(window);
        cx.notify();
    }

    fn clone_focus_step(&self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.clone_url_input.read(cx).is_composing()
            || self.clone_name_input.read(cx).is_composing()
        {
            return;
        }
        if self.clone_operation.is_some() {
            window.focus(&self.clone_cancel_focus);
            return;
        }
        let fields = [
            self.clone_url_input.read(cx).focus_handle(cx),
            self.clone_name_input.read(cx).focus_handle(cx),
            self.clone_browse_focus.clone(),
            self.clone_confirm_focus.clone(),
            self.clone_cancel_focus.clone(),
        ];
        let current = fields
            .iter()
            .position(|field| field.is_focused(window))
            .unwrap_or(0);
        let next = if backwards {
            (current + fields.len() - 1) % fields.len()
        } else {
            (current + 1) % fields.len()
        };
        window.focus(&fields[next]);
    }

    fn on_clone_next(
        &mut self,
        _: &crate::CloneDialogNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clone_focus_step(false, window, cx);
    }

    fn on_clone_prev(
        &mut self,
        _: &crate::CloneDialogPrev,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clone_focus_step(true, window, cx);
    }

    fn on_clone_activate(
        &mut self,
        _: &crate::CloneDialogActivate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clone_cancel_focus.is_focused(window) {
            self.cancel_worktree_dialog(cx);
            self.input.read(cx).focus(window);
        } else if self.clone_browse_focus.is_focused(window) && self.clone_operation.is_none() {
            self.browse_clone_parent(window, cx);
        } else if self.clone_operation.is_none() {
            if self.clone_url_input.read(cx).is_composing()
                || self.clone_name_input.read(cx).is_composing()
            {
                return;
            }
            self.submit_clone(cx);
            if self.clone_operation.is_some() {
                window.focus(&self.clone_cancel_focus);
            }
        }
    }

    fn browse_clone_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI's test platform does not implement native path prompts.
        #[cfg(test)]
        if let Some(parent) = self.clone_test_parent.take() {
            self.finish_clone_parent(parent, cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            let receiver = cx.update(|_, app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some(tr!("working_folder.clone_parent").into()),
                })
            });
            let Ok(receiver) = receiver else { return };
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(parent) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |app, _, cx| app.finish_clone_parent(parent, cx));
        })
        .detach();
    }

    fn finish_clone_parent(&mut self, parent: PathBuf, cx: &mut Context<Self>) {
        if matches!(self.worktree_dialog, Some(WorktreeDialog::Clone))
            && self.clone_operation.is_none()
        {
            self.clone_parent = Some(parent);
            self.worktree_dialog_error = None;
            cx.notify();
        }
    }

    fn submit_clone(&mut self, cx: &mut Context<Self>) {
        if self.clone_operation.is_some() {
            return;
        }
        let source = self.clone_url_input.read(cx).text().trim().to_owned();
        let name = self.clone_name_input.read(cx).text().trim().to_owned();
        let valid_name = matches!(
            Path::new(&name).components().collect::<Vec<_>>().as_slice(),
            [std::path::Component::Normal(_)]
        );
        let Some(parent) = self.clone_parent.clone().filter(|path| path.is_dir()) else {
            self.worktree_dialog_error = Some(tr!("working_folder.clone_parent_required"));
            cx.notify();
            return;
        };
        if source.is_empty() || !valid_name {
            self.worktree_dialog_error = Some(tr!("working_folder.clone_fields_required"));
            cx.notify();
            return;
        }
        let destination = parent.join(name);
        let clone_window = self.clone_window;
        let composer_focus = self.input.read(cx).focus_handle(cx);
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let progress = Arc::new(std::sync::Mutex::new(String::new()));
        self.clone_operation = Some(CloneOperation {
            cancelled: cancelled.clone(),
            progress: progress.clone(),
        });
        self.worktree_dialog_error = None;
        cx.notify();

        // A separate UI timer displays the latest Git progress while the
        // blocking process runs on the background executor.
        let timer_cancelled = cancelled.clone();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let still_running = this
                .update(cx, |app, cx| {
                    let running = app.clone_operation.as_ref().is_some_and(|operation| {
                        Arc::ptr_eq(&operation.cancelled, &timer_cancelled)
                    });
                    if running {
                        cx.notify();
                    }
                    running
                })
                .unwrap_or(false);
            if !still_running {
                break;
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let worker_cancelled = cancelled.clone();
            let worker_destination = destination.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::repository_clone::clone_repository(
                        &source,
                        &worker_destination,
                        &worker_cancelled,
                        |chunk| {
                            if let Ok(mut latest) = progress.lock() {
                                *latest = chunk.trim().chars().take(512).collect();
                            }
                        },
                    )
                })
                .await;
            let opened = this
                .update(cx, |app, cx| {
                    if !app
                        .clone_operation
                        .as_ref()
                        .is_some_and(|operation| Arc::ptr_eq(&operation.cancelled, &cancelled))
                    {
                        return false;
                    }
                    let should_open = matches!(app.worktree_dialog, Some(WorktreeDialog::Clone))
                        && !cancelled.load(std::sync::atomic::Ordering::Acquire);
                    app.clone_operation = None;
                    let opened = match result {
                        Ok(()) if should_open => {
                            app.worktree_dialog = None;
                            app.open_worktree(&destination, cx);
                            true
                        }
                        Err(crate::repository_clone::CloneError::Git(error)) if should_open => {
                            app.worktree_dialog_error = Some(error);
                            false
                        }
                        _ => false,
                    };
                    cx.notify();
                    opened
                })
                .unwrap_or(false);
            if opened {
                if let Some(window) = clone_window {
                    let _ = cx.update_window(window, |_, window, _| window.focus(&composer_focus));
                }
            }
        })
        .detach();
    }

    // ── create flow ────────────────────────────────────────────────────

    /// Open the Create Worktree dialog and load the branch choices.
    pub(super) fn open_create_worktree_dialog(&mut self, cx: &mut Context<Self>) {
        self.worktree_dialog = Some(WorktreeDialog::Create);
        self.worktree_dialog_error = None;
        self.worktree_branch_mode = WorktreeBranchMode::New;
        self.worktree_branch_choice = None;
        self.worktree_run_setup = self.worktree_config.run_setup_auto;
        // Reset the derivation state *after* clearing the fields, so the
        // branch observer firing on `clear` cannot leave it marked as
        // user-edited before the user types anything.
        self.worktree_name_input
            .update(cx, |input, cx| input.clear(cx));
        self.worktree_branch_input
            .update(cx, |input, cx| input.clear(cx));
        self.worktree_start_input
            .update(cx, |input, cx| input.clear(cx));
        self.worktree_branch_touched = false;
        self.worktree_branch_programmatic = false;

        // Branch choices load off-thread: listing branches and refs are Git
        // calls, and render must never wait on them. The remote names and
        // local branches ride along so the dialog can say what a remote ref
        // will create, and so the picker's remote level is instant afterwards.
        if let Some(manager) = self.worktree_manager() {
            cx.spawn(async move |this, cx| {
                let loaded = cx
                    .background_executor()
                    .spawn(async move {
                        let repo = manager.repository_root().ok()?;
                        let local = crate::git::list_branches(&repo).unwrap_or_default();
                        let remotes = crate::git_ops::list_remotes(&repo);
                        let refs = crate::git_ops::list_refs(&repo);
                        let mut branches = local.clone();
                        for reference in refs
                            .iter()
                            .filter(|reference| reference.kind == crate::git::RefKind::Remote)
                        {
                            branches.push(reference.name.clone());
                        }
                        Some((branches, local, remotes, refs))
                    })
                    .await;
                let _ = this.update(cx, |app, cx| {
                    if let Some((branches, local, remotes, refs)) = loaded {
                        // Prefer a branch that is not checked out in the
                        // active workspace; that one is usually taken.
                        let active = app
                            .current_workspace
                            .as_deref()
                            .and_then(crate::git::current_branch);
                        app.worktree_branch_choice = branches
                            .iter()
                            .find(|branch| active.as_deref() != Some(branch.as_str()))
                            .cloned()
                            .or_else(|| branches.first().cloned());
                        app.worktree_branches = branches;
                        app.worktree_local_branches = local;
                        app.worktree_remote_names = remotes;
                        app.worktree_remotes = refs;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// The local branch that choosing `branch` in the create dialog will
    /// create, when `branch` is a remote-tracking ref. `None` for a local
    /// branch (which is checked out as-is).
    fn tracking_branch_for(&self, branch: &str) -> Option<crate::working_folder::RemoteRow> {
        if !self.worktree_remotes.iter().any(|reference| {
            reference.kind == crate::git::RefKind::Remote && reference.name == branch
        }) {
            return None;
        }
        crate::working_folder::remote_ref_rows(
            &self.worktree_remotes,
            &self.worktree_remote_names,
            &self.worktree_local_branches,
        )
        .into_iter()
        .find(|row| row.remote_ref == branch)
    }

    /// Close the open dialog without acting.
    pub(super) fn cancel_worktree_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(operation) = self.clone_operation.take() {
            operation
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.worktree_dialog = None;
        self.worktree_dialog_error = None;
        cx.notify();
    }

    /// Dismiss when the scrim (outside the card) is pressed.
    pub(super) fn on_worktree_dialog_scrim(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let clone = matches!(self.worktree_dialog, Some(WorktreeDialog::Clone));
        self.cancel_worktree_dialog(cx);
        if clone {
            self.input.read(cx).focus(window);
        }
    }

    /// The dialog's primary button.
    pub(super) fn confirm_worktree_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.worktree_dialog.clone() else {
            return;
        };
        match dialog {
            WorktreeDialog::Clone => self.submit_clone(cx),
            WorktreeDialog::Create => self.submit_create_worktree(cx),
            WorktreeDialog::AllowSetup(pending) => {
                if let Some(root) = self.worktree_root_hint() {
                    self.worktree_config.allow_setup(&root);
                    let _ = self.worktree_config.persist();
                }
                self.worktree_dialog = None;
                self.perform_worktree_create(pending, cx);
            }
            WorktreeDialog::Rename { path } => self.submit_move_worktree(path, true, cx),
            WorktreeDialog::Move { path } => self.submit_move_worktree(path, false, cx),
            WorktreeDialog::Remove { path, dirty } => {
                self.worktree_dialog = None;
                let force = dirty;
                let label = tr!(
                    "worktree.operation.removing",
                    name = self.worktree_display_name(&path)
                );
                self.run_worktree_mutation(
                    label,
                    cx,
                    move |manager| manager.remove(&path, force),
                    |_app, _| {},
                );
            }
            WorktreeDialog::BranchInUse { path, .. } => {
                self.cancel_worktree_dialog(cx);
                self.open_worktree(&path, cx);
            }
            WorktreeDialog::Unavailable { .. } => {
                self.worktree_dialog = None;
                self.open_worktrees(cx);
            }
        }
    }

    fn worktree_display_name(&self, path: &Path) -> String {
        self.listed_worktree(path)
            .map(|worktree| worktree.name.clone())
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    }

    /// Validate the create dialog and dispatch. A repository's first setup
    /// run goes through the explicit permission dialog; everything else runs
    /// straight away.
    fn submit_create_worktree(&mut self, cx: &mut Context<Self>) {
        // The fields hold the preedit while an IME candidate window is open; a
        // click on Create must not name a worktree with half-composed text
        // (issue #100).
        if crate::composer::any_composing(
            cx,
            &[
                &self.worktree_name_input,
                &self.worktree_branch_input,
                &self.worktree_start_input,
            ],
        ) {
            return;
        }
        let Some(manager) = self.worktree_manager() else {
            self.worktree_dialog_error = Some(tr!("worktree.error.not_a_repository"));
            return;
        };
        let name = self.worktree_name_input.read(cx).text().trim().to_string();
        if let Err(err) = worktree::validate_name(&name) {
            self.worktree_dialog_error = Some(err.to_string());
            return;
        }
        let path = match manager.resolve_path(&name) {
            Ok(path) => path,
            Err(err) => {
                self.worktree_dialog_error = Some(err.to_string());
                return;
            }
        };
        let (branch, start_point) = match self.worktree_branch_mode {
            WorktreeBranchMode::New => (
                self.worktree_branch_input
                    .read(cx)
                    .text()
                    .trim()
                    .to_string(),
                {
                    let start = self.worktree_start_input.read(cx).text().trim().to_string();
                    (!start.is_empty()).then_some(start)
                },
            ),
            WorktreeBranchMode::Existing => match self.worktree_branch_choice.clone() {
                Some(branch) => (branch, None),
                None => {
                    self.worktree_dialog_error = Some(tr!("worktree.error.select_branch"));
                    return;
                }
            },
        };
        if branch.is_empty() {
            self.worktree_dialog_error = Some(tr!("git.branch_name_empty"));
            return;
        }
        // The setup script is a file check against the cached root, so no Git
        // runs on the UI thread here.
        let script = self
            .worktree_root_hint()
            .and_then(|root| self.worktree_config.effective_setup_script(&root));
        let run_setup = self.worktree_run_setup && script.is_some();
        let pending = PendingWorktreeCreate {
            name,
            path,
            branch,
            existing_branch: self.worktree_branch_mode == WorktreeBranchMode::Existing,
            start_point,
            run_setup,
        };
        let needs_permission = run_setup
            && !self
                .worktree_root_hint()
                .is_some_and(|root| self.worktree_config.setup_allowed(&root));
        if needs_permission {
            self.worktree_dialog = Some(WorktreeDialog::AllowSetup(pending));
            self.worktree_dialog_error = None;
            cx.notify();
            return;
        }
        self.perform_worktree_create(pending, cx);
    }

    /// Run `git worktree add` on the background executor, then optionally the
    /// setup script and the workspace open.
    fn perform_worktree_create(&mut self, pending: PendingWorktreeCreate, cx: &mut Context<Self>) {
        let Some(manager) = self.worktree_manager() else {
            return;
        };
        self.worktree_dialog = None;
        self.worktree_dialog_error = None;
        self.worktree_mutation_repo = Some(manager.repo().to_path_buf());
        self.worktree_operation = Some(tr!(
            "worktree.operation.creating",
            name = pending.name.clone()
        ));
        let open_after = self.worktree_config.open_after_create;
        let run_setup = pending.run_setup;
        cx.spawn(async move |this, cx| {
            let pending_bg = pending.clone();
            let (created, root, script) = cx
                .background_executor()
                .spawn(async move {
                    let root = manager.repository_root().ok();
                    let script = root
                        .as_deref()
                        .and_then(|root| manager.config().effective_setup_script(root));
                    let created = if pending_bg.existing_branch {
                        manager.create_from_existing_branch(
                            &pending_bg.name,
                            &pending_bg.path,
                            &pending_bg.branch,
                        )
                    } else {
                        manager.create_new_branch(
                            &pending_bg.name,
                            &pending_bg.path,
                            &pending_bg.branch,
                            pending_bg.start_point.as_deref(),
                        )
                    };
                    (created, root, script)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktree_mutation_repo = None;
                app.worktree_operation = None;
                match created {
                    Ok(worktree) => {
                        app.worktrees_error = None;
                        app.toast_success(tr!(
                            "worktree.created_toast",
                            name = pending.name.clone()
                        ));
                        if run_setup {
                            if let (Some(root), Some(script)) = (root, script) {
                                // Setup first, then open: the progress banner
                                // stays on the page until the script settles
                                // (§10), and a failed setup never auto-opens.
                                app.begin_worktree_setup(
                                    root,
                                    script,
                                    worktree.path.clone(),
                                    open_after,
                                    cx,
                                );
                            } else if open_after {
                                app.open_worktree(&worktree.path, cx);
                            }
                        } else if open_after {
                            app.open_worktree(&worktree.path, cx);
                        }
                        app.refresh_worktrees(cx);
                    }
                    Err(err) => {
                        app.finish_worktree_error(err, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Run the setup script asynchronously, capturing its output. A failure
    /// never removes the worktree Git just created.
    fn begin_worktree_setup(
        &mut self,
        repo_root: PathBuf,
        script: PathBuf,
        worktree: PathBuf,
        open_after: bool,
        cx: &mut Context<Self>,
    ) {
        self.worktree_setup = Some(WorktreeSetupState {
            worktree: worktree.clone(),
            script: script.clone(),
            outcome: None,
            expanded: false,
            open_after,
        });
        self.spawn_setup_script(repo_root, script, worktree, open_after, cx);
    }

    fn spawn_setup_script(
        &mut self,
        repo_root: PathBuf,
        script: PathBuf,
        worktree: PathBuf,
        open_after: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let run_path = worktree.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { worktree_setup::run(&script, &repo_root, &run_path) })
                .await;
            let _ = this.update(cx, |app, cx| {
                let success = outcome.success;
                if let Some(state) = app.worktree_setup.as_mut() {
                    state.outcome = Some(outcome.clone());
                }
                if !success {
                    app.toast_warning(outcome.summary());
                } else {
                    app.toast_success(tr!("worktree.setup.succeeded_toast"));
                    if open_after {
                        // The created worktree opens only once its setup is done.
                        app.open_worktree(&worktree, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Retry the setup script from the progress banner.
    pub(super) fn retry_worktree_setup(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.worktree_setup.clone() else {
            return;
        };
        let Some(root) = self.worktree_root_hint() else {
            return;
        };
        self.worktree_setup = Some(WorktreeSetupState {
            worktree: state.worktree.clone(),
            script: state.script.clone(),
            outcome: None,
            expanded: state.expanded,
            open_after: state.open_after,
        });
        self.spawn_setup_script(root, state.script, state.worktree, state.open_after, cx);
        cx.notify();
    }

    /// Dismiss the setup progress banner.
    pub(super) fn dismiss_worktree_setup(&mut self, cx: &mut Context<Self>) {
        self.worktree_setup = None;
        cx.notify();
    }

    /// Expand/collapse the captured setup output.
    pub(super) fn toggle_worktree_setup_output(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.worktree_setup.as_mut() {
            state.expanded = !state.expanded;
        }
        cx.notify();
    }

    // ── row actions ────────────────────────────────────────────────────

    /// Open a worktree as the working folder. Explorer, Git, Terminal, Review,
    /// and the agent all follow that one path, so choosing a folder starts the
    /// task there — the same transition the new-task page's folder field runs.
    /// A running session is parked rather than dropped, and an idle one is
    /// torn down with its transcript already on disk, so nothing is lost.
    ///
    /// This used to move `current_workspace` alone, which left the live pi
    /// process rooted in the previous folder: the chip named one place while
    /// the agent edited another.
    pub(super) fn open_worktree(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !path.is_dir() {
            self.worktrees_error = Some(tr!(
                "worktree.error.missing_directory",
                path = path.to_string_lossy()
            ));
            cx.notify();
            return;
        }
        let path = path.to_path_buf();
        if !self.folder_change(&path) {
            self.close_worktrees(cx);
            return;
        }
        self.start_task_in_folder_quiet(path, cx);
        self.close_worktrees(cx);
    }

    /// Whether choosing `path` is a change of working folder, i.e. whether the
    /// agent has to be moved to it. Picking the folder already in use is a
    /// no-op; anything else is a new task there.
    pub(super) fn folder_change(&self, path: &Path) -> bool {
        !self
            .current_workspace
            .as_deref()
            .is_some_and(|active| worktree::same_path(active, path))
    }

    // ── existing worktrees ─────────────────────────────────────────────

    /// A native picker for a worktree folder that `git worktree list` may not
    /// report: a directory that was moved by hand, or one that was created
    /// outside Orbit and never opened here. The path goes through
    /// [`OrbitApp::adopt_worktree`], so nothing is opened until Git agrees it
    /// is a worktree.
    pub(super) fn browse_for_existing_worktree(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The dialog is asynchronous for the reason documented on
        // `browse_for_folder`: the blocking `rfd::FileDialog` re-enters the
        // main loop while this entity is borrowed.
        cx.spawn_in(window, async move |this, cx| {
            let receiver = cx.update(|_window, app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some(tr!("working_folder.existing").into()),
                })
            });
            let Ok(receiver) = receiver else {
                return;
            };
            let folder = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(folder) = folder else {
                return;
            };
            let _ = this.update_in(cx, |app, window, cx| {
                app.adopt_worktree(folder, window, cx);
            });
        })
        .detach();
    }

    /// Reconnect a worktree directory Git no longer lists at that path
    /// (`git worktree repair`), then open it. A directory whose worktree
    /// metadata is gone is reported with that reason — it is never opened as if
    /// it were healthy, because commits there would sit on no branch.
    pub(super) fn adopt_worktree(
        &mut self,
        path: PathBuf,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(manager) = self.worktree_manager() else {
            return;
        };
        let label = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.worktree_mutation_repo = Some(manager.repo().to_path_buf());
        self.worktree_operation = Some(tr!("working_folder.connecting", name = label.clone()));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let target = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { manager.adopt(&target) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktree_mutation_repo = None;
                app.worktree_operation = None;
                match result {
                    Ok(worktree) => {
                        app.worktrees_error = None;
                        app.toast_success(tr!(
                            "working_folder.repaired",
                            name = worktree.name.clone()
                        ));
                        app.refresh_worktrees(cx);
                        app.open_worktree(&worktree.path, cx);
                    }
                    Err(err) => app.finish_worktree_error(err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Toggle the row-actions popup.
    pub(super) fn toggle_worktree_menu(
        &mut self,
        path: PathBuf,
        at: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        if self
            .worktree_menu
            .as_ref()
            .is_some_and(|open| open.path == path)
        {
            self.worktree_menu = None;
        } else {
            self.worktree_menu = Some(WorktreeMenu { path, at });
        }
        cx.notify();
    }

    pub(super) fn dismiss_worktree_menu(&mut self, cx: &mut Context<Self>) {
        self.worktree_menu = None;
        cx.notify();
    }

    /// Toggle the toolbar "Advanced" menu. The click's mouse-down dismisses an
    /// open menu (`on_mouse_down_out`); without the gesture guard the same
    /// click's mouse-up would toggle it straight back open, so a second click
    /// never closed it.
    pub(super) fn toggle_worktree_advanced(&mut self, cx: &mut Context<Self>) {
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        self.worktree_advanced_open = !self.worktree_advanced_open;
        cx.notify();
    }

    /// Open the rename dialog with the current name prefilled. The Git
    /// branch is never renamed — only the directory.
    pub(super) fn start_rename_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let name = self.worktree_display_name(&path);
        self.worktree_field_input.update(cx, |input, cx| {
            input.set_placeholder_key("worktree.field.new_name_placeholder", cx);
            input.set_text(name, cx);
        });
        self.worktree_dialog = Some(WorktreeDialog::Rename { path });
        self.worktree_dialog_error = None;
        self.worktree_menu = None;
        cx.notify();
    }

    /// Open the move dialog with the current parent prefilled.
    pub(super) fn start_move_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let parent = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.worktree_field_input.update(cx, |input, cx| {
            input.set_placeholder_key("worktree.field.destination_placeholder", cx);
            input.set_text(parent, cx);
        });
        self.worktree_dialog = Some(WorktreeDialog::Move { path });
        self.worktree_dialog_error = None;
        self.worktree_menu = None;
        cx.notify();
    }

    /// Commit a rename (`new_name = true`) or a move. Rename resolves the
    /// field as a name under the worktree root; Move resolves it as a
    /// destination directory.
    fn submit_move_worktree(&mut self, path: PathBuf, new_name: bool, cx: &mut Context<Self>) {
        if crate::composer::any_composing(cx, &[&self.worktree_field_input]) {
            return;
        }
        let Some(manager) = self.worktree_manager() else {
            return;
        };
        let value = self.worktree_field_input.read(cx).text().trim().to_string();
        if value.is_empty() {
            self.worktree_dialog_error = Some(if new_name {
                tr!("worktree.error.invalid_name", name = value)
            } else {
                tr!("worktree.error.move_destination")
            });
            return;
        }
        let destination = if new_name {
            match manager.resolve_path(&value) {
                Ok(destination) => destination,
                Err(err) => {
                    self.worktree_dialog_error = Some(err.to_string());
                    return;
                }
            }
        } else {
            worktree::expand_path(
                self.worktree_root_hint()
                    .as_deref()
                    .unwrap_or(Path::new(".")),
                &value,
            )
        };
        if worktree::same_path(&path, &destination) {
            self.cancel_worktree_dialog(cx);
            return;
        }
        if destination.exists() {
            self.worktree_dialog_error = Some(tr!(
                "worktree.error.path_exists",
                path = destination.to_string_lossy()
            ));
            return;
        }
        self.worktree_dialog = None;
        let label = tr!(
            "worktree.operation.moving",
            name = self.worktree_display_name(&path)
        );
        self.worktree_operation = Some(label);
        self.worktree_mutation_repo = Some(manager.repo().to_path_buf());
        let old_path = path.clone();
        let destination_for_task = destination.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { manager.move_to(&old_path, &destination_for_task) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktree_operation = None;
                app.worktree_mutation_repo = None;
                match result {
                    Ok(()) => {
                        // A moved active workspace follows its directory so a
                        // running session is not stranded on a dead path.
                        if app.worktree_is_active(&path) {
                            app.set_current_workspace(destination.clone(), cx);
                        }
                        app.worktrees_error = None;
                    }
                    Err(err) => app.finish_worktree_error(err, cx),
                }
                app.refresh_worktrees(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Ask before removing: read the worktree's dirty state off-thread first,
    /// so the confirmation can say what is at stake.
    pub(super) fn start_remove_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.worktree_menu = None;
        let Some(manager) = self.worktree_manager() else {
            return;
        };
        let path_for_task = path.clone();
        cx.spawn(async move |this, cx| {
            let dirty = cx
                .background_executor()
                .spawn(async move { manager.is_dirty(&path_for_task).unwrap_or(false) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktree_dialog = Some(WorktreeDialog::Remove { path, dirty });
                app.worktree_dialog_error = None;
                cx.notify();
            });
        })
        .detach();
    }

    /// Lock a worktree (`git worktree lock`).
    pub(super) fn lock_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.worktree_menu = None;
        let label = tr!(
            "worktree.operation.locking",
            name = self.worktree_display_name(&path)
        );
        self.run_worktree_mutation(
            label,
            cx,
            move |manager| manager.lock(&path, None),
            |_app, _| {},
        );
    }

    /// Unlock a worktree (`git worktree unlock`).
    pub(super) fn unlock_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.worktree_menu = None;
        let label = tr!(
            "worktree.operation.unlocking",
            name = self.worktree_display_name(&path)
        );
        self.run_worktree_mutation(
            label,
            cx,
            move |manager| manager.unlock(&path),
            |_app, _| {},
        );
    }

    /// `git worktree prune`: drop administrative entries for worktrees whose
    /// directories are gone.
    pub(super) fn prune_worktrees(&mut self, cx: &mut Context<Self>) {
        self.worktree_advanced_open = false;
        let label = tr!("worktree.operation.pruning");
        self.run_worktree_mutation(label, cx, |manager| manager.prune(), |_app, _| {});
    }

    /// `git worktree repair`: rebuild administrative files after a manual
    /// move.
    pub(super) fn repair_worktrees(&mut self, cx: &mut Context<Self>) {
        self.worktree_advanced_open = false;
        let label = tr!("worktree.operation.repairing");
        self.run_worktree_mutation(label, cx, |manager| manager.repair(), |_app, _| {});
    }

    /// Copy a worktree's path.
    pub(super) fn copy_worktree_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            path.to_string_lossy().into_owned(),
        ));
        self.worktree_menu = None;
        self.set_status(tr!("worktree.path_copied"));
        cx.notify();
    }

    /// Reveal a worktree in the OS file manager.
    pub(super) fn reveal_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.worktree_menu = None;
        crate::platform::reveal_in_file_manager(&path);
        cx.notify();
    }

    /// Serialize one mutating operation, then re-list. A second mutation
    /// while one runs is dropped, not queued: the row state it would target
    /// may already be gone (§31).
    fn run_worktree_mutation<F, D>(
        &mut self,
        label: String,
        cx: &mut Context<Self>,
        operation: F,
        done: D,
    ) where
        F: FnOnce(WorktreeManager) -> Result<(), WorktreeError> + Send + 'static,
        D: FnOnce(&mut OrbitApp, Result<(), WorktreeError>) + Send + 'static,
    {
        if self.worktree_mutation_repo.is_some() {
            return;
        }
        let Some(manager) = self.worktree_manager() else {
            return;
        };
        self.worktree_mutation_repo = Some(manager.repo().to_path_buf());
        self.worktree_operation = Some(label);
        self.worktrees_error = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { operation(manager) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktree_mutation_repo = None;
                app.worktree_operation = None;
                match &result {
                    Ok(()) => {
                        app.worktrees_error = None;
                        done(app, Ok(()));
                    }
                    Err(err) => {
                        let err = err.clone();
                        app.finish_worktree_error(err.clone(), cx);
                        done(app, Err(err));
                    }
                }
                app.refresh_worktrees(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Route a mutation error: an already-checked-out branch gets the
    /// actionable dialog, everything else the page banner.
    fn finish_worktree_error(&mut self, err: WorktreeError, cx: &mut Context<Self>) {
        if let WorktreeError::BranchCheckedOut { branch, path } = err {
            self.worktree_dialog = Some(WorktreeDialog::BranchInUse { branch, path });
            self.worktree_dialog_error = None;
        } else {
            self.worktrees_error = Some(err.to_string());
        }
        cx.notify();
    }

    // ── settings ───────────────────────────────────────────────────────

    /// Settings → Worktrees rows.
    pub(super) fn worktree_settings_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let repo_label = self.worktree_repo_label();
        let directory_hint = if repo_label.is_empty() {
            tr!("worktree.settings.directory_hint")
        } else {
            tr!(
                "worktree.settings.directory_hint_repo",
                repo = repo_label.clone()
            )
        };
        vec![
            self.settings_section(
                theme,
                &tr!("worktree.settings.manage_section"),
                vec![self.setting_row(
                    theme,
                    &tr!("worktree.settings.manage"),
                    Some(&tr!("worktree.settings.manage_hint")),
                    None,
                    Some(worktree_primary_button(
                        "worktree-settings-open",
                        tr!("worktree.settings.manage_open"),
                        theme,
                        this.clone(),
                        |app, cx| app.open_worktrees(cx),
                    )),
                )],
            ),
            self.settings_section(
                theme,
                &tr!("worktree.settings.location_section"),
                vec![
                    self.setting_row(
                        theme,
                        &tr!("worktree.settings.directory"),
                        Some(&directory_hint),
                        None,
                        Some(self.settings_text_field(&self.worktree_dir_input, &theme)),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("worktree.settings.setup_script"),
                        Some(&tr!("worktree.settings.setup_script_hint")),
                        None,
                        Some(self.settings_text_field(&self.worktree_script_input, &theme)),
                    ),
                ],
            ),
            self.settings_section(
                theme,
                &tr!("worktree.settings.creation_section"),
                vec![
                    self.setting_row(
                        theme,
                        &tr!("worktree.settings.run_setup_auto"),
                        Some(&tr!("worktree.settings.run_setup_auto_hint")),
                        None,
                        Some(self.settings_toggle(
                            "worktree-run-setup-auto",
                            self.worktree_config.run_setup_auto,
                            theme,
                            this.clone(),
                            Self::toggle_worktree_run_setup_auto,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("worktree.settings.open_after_create"),
                        Some(&tr!("worktree.settings.open_after_create_hint")),
                        None,
                        Some(self.settings_toggle(
                            "worktree-open-after-create",
                            self.worktree_config.open_after_create,
                            theme,
                            this.clone(),
                            Self::toggle_worktree_open_after_create,
                        )),
                    ),
                ],
            ),
        ]
    }

    /// The Settings field for a worktree text input (fixed width so the two
    /// rows align).
    fn settings_text_field(&self, input: &Entity<ComposerInput>, theme: &Theme) -> AnyElement {
        input_field_frame(div(), theme)
            .w(px(240.))
            .bg(theme.background)
            .child(input.clone())
            .into_any_element()
    }

    pub(super) fn toggle_worktree_run_setup_auto(&mut self, cx: &mut Context<Self>) {
        self.worktree_config.run_setup_auto = !self.worktree_config.run_setup_auto;
        let _ = self.worktree_config.persist();
        cx.notify();
    }

    pub(super) fn toggle_worktree_open_after_create(&mut self, cx: &mut Context<Self>) {
        self.worktree_config.open_after_create = !self.worktree_config.open_after_create;
        let _ = self.worktree_config.persist();
        cx.notify();
    }

    /// The create dialog's per-worktree setup checkbox.
    pub(super) fn toggle_create_run_setup(&mut self, cx: &mut Context<Self>) {
        self.worktree_run_setup = !self.worktree_run_setup;
        cx.notify();
    }

    // ── rendering ──────────────────────────────────────────────────────

    /// Shared top-bar leading controls (Back + title) while the page is open.
    pub(super) fn worktrees_top_bar_leading(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                press(
                    button_frame(div().id("worktrees-top-back"), &theme, ButtonSize::Medium)
                        .group(BUTTON_GROUP)
                        .cursor_pointer()
                        .hover_state(&theme),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| this.close_worktrees(cx)),
                )
                .child(icon(
                    "icons/arrow-left.svg",
                    ButtonSize::Medium.icon_size().px(&theme),
                    theme.text_secondary,
                ))
                .child(
                    div()
                        .text_color(theme.text_secondary)
                        .child(tr!("view.back")),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(icon(
                        "icons/branch.svg",
                        IconSize::Medium.px(&theme),
                        theme.text_secondary,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(tr!("worktree.page.title")),
                    ),
            )
            .into_any_element()
    }

    /// The full-page Worktrees surface.
    pub(super) fn render_worktrees_page(&self, cx: &Context<Self>) -> AnyElement {
        let theme = *theme::get(cx);
        let this = cx.entity();
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text_primary)
            .font_family(theme::ui_font_family())
            .child(self.worktrees_header(theme, this.clone(), cx))
            .child(
                div()
                    .id("worktrees-body")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            if this.worktree_menu.is_some() || this.worktree_advanced_open {
                                this.worktree_advanced_open = false;
                                this.dismiss_worktree_menu(cx);
                            }
                        }),
                    )
                    .child(
                        div()
                            .w_full()
                            .max_w(px(PAGE_MAX_W))
                            .mx_auto()
                            .px(DynamicSpacing::Base20.px(&theme))
                            .py(DynamicSpacing::Base20.px(&theme))
                            .debug_selector(|| "worktree-page-content".to_string())
                            .flex()
                            .flex_col()
                            .gap(DynamicSpacing::Base16.px(&theme))
                            .children(self.worktree_setup_banner(theme, this.clone(), cx))
                            .children(self.worktrees_error_banner(theme, this.clone()))
                            .children(self.worktree_howto_card(theme, this.clone()))
                            .child(
                                div()
                                    .debug_selector(|| "worktree-page-list".to_string())
                                    .child(self.worktrees_list(theme, this.clone(), cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Page header: repository context on the left, actions on the right.
    fn worktrees_header(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let repo_label = self.worktree_repo_label();
        let busy = self.worktree_operation.clone();
        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme));
        if let Some(label) = busy {
            right = right.child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(spinner(
                        "worktree-op-spinner",
                        IconSize::Small.px(&theme),
                        theme.text_muted,
                        theme,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(label),
                    ),
            );
        } else {
            right = right.child(worktree_icon_button(
                "worktrees-refresh",
                "icons/refresh.svg",
                theme,
                this.clone(),
                |app, cx| app.refresh_worktrees(cx),
            ));
        }
        let advanced = self.worktree_advanced_control(theme, this.clone(), cx);
        right = right.child(advanced).child(
            button_frame(div().id("worktrees-new"), &theme, ButtonSize::Medium)
                .border_1()
                .border_color(theme.border)
                .raised(theme.surface_elevated, &theme)
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .hover_state(&theme)
                .child(icon(
                    "icons/plus.svg",
                    ButtonSize::Medium.icon_size().px(&theme),
                    theme.accent_text,
                ))
                .child(
                    div()
                        .text_color(theme.text_primary)
                        .child(tr!("worktree.page.new")),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        this.open_create_worktree_dialog(cx)
                    }),
                ),
        );

        div()
            .h(px(44.))
            .flex_none()
            .pl(DynamicSpacing::Base20.px(&theme))
            .pr(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(repo_label),
            )
            .child(right)
            .into_any_element()
    }

    /// The Advanced popover: prune stale worktrees and repair administrative
    /// files, kept off the main surface (§19).
    fn worktree_advanced_control(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let open = self.worktree_advanced_open;
        let mut control = div().relative();
        if open {
            let popup_this = this.clone();
            control = control.child(
                div()
                    .absolute()
                    .top(px(0.))
                    .right(px(0.))
                    .size(px(0.))
                    .child(
                        // Anchor to the button's top-right (via a zero-size
                        // point) and drop the menu `MENU_OFFSET` below it,
                        // right-aligned — the same pattern as Settings' select
                        // and the Git "…" menu. `Window` mode (not `Local` +
                        // `snap_to_window`) is deliberate: that combo cannot
                        // flip, so in this top toolbar the off-screen "open
                        // above" position made the snap slide the menu down
                        // over its own button.
                        anchored()
                            .position_mode(AnchoredPositionMode::Window)
                            .anchor(Corner::TopRight)
                            .offset(point(
                                px(0.),
                                ButtonSize::Medium.height(&theme) + popover::MENU_OFFSET,
                            ))
                            .child(deferred(
                                picker_surface(div(), &theme)
                                    .debug_selector(|| "worktrees-advanced-popup".to_string())
                                    .w(px(240.))
                                    .py(picker::list_padding_y(&theme))
                                    .occlude()
                                    .on_mouse_down_out({
                                        let this = popup_this.clone();
                                        move |_, _, cx| {
                                            this.update(cx, |app, cx| {
                                                app.worktree_advanced_open = false;
                                                // Stamp the dismissal so the
                                                // button's mouse-up (the same
                                                // click) cannot toggle it back
                                                // open.
                                                app.menu_dismissed_at = Some(Instant::now());
                                                cx.notify();
                                            });
                                        }
                                    })
                                    // Adopting a folder is the one action
                                    // here that needs a window (the native
                                    // picker), so it carries its own
                                    // listener instead of `worktree_menu_row`.
                                    .child(
                                        picker_entry(div().id("wa-adopt"), &theme)
                                            .h(picker::entry_height(&theme))
                                            .flex_none()
                                            .cursor_pointer()
                                            .hover_state(&theme)
                                            .child(icon(
                                                "icons/folder-open.svg",
                                                context_menu::ICON.px(&theme),
                                                theme.text_muted,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_color(theme.text_secondary)
                                                    .child(tr!("working_folder.existing")),
                                            )
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|app, _, window, cx| {
                                                    cx.stop_propagation();
                                                    app.worktree_advanced_open = false;
                                                    app.browse_for_existing_worktree(window, cx);
                                                }),
                                            ),
                                    )
                                    .child(worktree_menu_row(
                                        "wa-prune",
                                        "icons/trash.svg",
                                        tr!("worktree.advanced.prune"),
                                        theme,
                                        this.clone(),
                                        |app, cx| app.prune_worktrees(cx),
                                    ))
                                    .child(worktree_menu_row(
                                        "wa-repair",
                                        "icons/folder-sync.svg",
                                        tr!("worktree.advanced.repair"),
                                        theme,
                                        this,
                                        |app, cx| app.repair_worktrees(cx),
                                    )),
                            )),
                    ),
            );
        }
        control = control.child(
            press(
                button_frame(div().id("worktrees-advanced"), &theme, ButtonSize::Medium)
                    .debug_selector(|| "worktrees-advanced".to_string())
                    .border_1()
                    .border_color(theme.border)
                    .raised(
                        if open {
                            theme.surface_hover
                        } else {
                            theme.surface_elevated
                        },
                        &theme,
                    )
                    .cursor_pointer()
                    .hover_state(&theme)
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!("worktree.page.advanced")),
                    )
                    .child(icon(
                        "icons/chevron-down.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_muted,
                    )),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.toggle_worktree_advanced(cx);
                }),
            ),
        );
        control.into_any_element()
    }

    /// The setup progress / result banner.
    fn worktree_setup_banner(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.worktree_setup.as_ref()?;
        let name = state
            .worktree
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut card = div()
            .w_full()
            .min_w_0()
            .bg(theme.surface_secondary)
            .border_1()
            .border_color(theme.border)
            .rounded(panel::RADIUS.px(&theme))
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme));

        let (glyph, glyph_color, title) = match &state.outcome {
            None => (
                "icons/loader.svg",
                theme.text_muted,
                tr!("worktree.setup.running"),
            ),
            Some(outcome) if outcome.success => (
                "icons/check.svg",
                theme.success,
                tr!("worktree.setup.succeeded"),
            ),
            Some(_) => (
                "icons/circle-x.svg",
                theme.error,
                tr!("worktree.setup.failed"),
            ),
        };
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(if state.outcome.is_none() {
                    spinner(
                        "worktree-setup-spin",
                        IconSize::Medium.px(&theme),
                        glyph_color,
                        theme,
                    )
                } else {
                    icon(glyph, IconSize::Medium.px(&theme), glyph_color).into_any_element()
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base02.px(&theme))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .truncate()
                                .child(tr!(
                                    "worktree.setup.script_label",
                                    name = name.clone(),
                                    script = state.script.to_string_lossy()
                                )),
                        ),
                ),
        );
        if let Some(outcome) = &state.outcome {
            if !outcome.success {
                card = card.child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(outcome.summary()),
                );
            }
            let mut actions = div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(worktree_text_button(
                    "worktree-setup-toggle-output",
                    if state.expanded {
                        tr!("worktree.setup.hide_output")
                    } else {
                        tr!("worktree.setup.show_output")
                    },
                    theme,
                    this.clone(),
                    |app, cx| app.toggle_worktree_setup_output(cx),
                ));
            if !outcome.success {
                actions = actions.child(worktree_text_button(
                    "worktree-setup-retry",
                    tr!("worktree.setup.retry"),
                    theme,
                    this.clone(),
                    |app, cx| app.retry_worktree_setup(cx),
                ));
            }
            let path = state.worktree.clone();
            actions = actions.child(worktree_text_button(
                "worktree-setup-open",
                tr!("worktree.setup.open_worktree"),
                theme,
                this.clone(),
                move |app, cx| app.open_worktree(&path, cx),
            ));
            actions = actions.child(worktree_text_button(
                "worktree-setup-dismiss",
                tr!("common.dismiss"),
                theme,
                this.clone(),
                |app, cx| app.dismiss_worktree_setup(cx),
            ));
            card = card.child(actions);
            if state.expanded {
                let output = outcome.output();
                card = card.child(
                    div()
                        .id("worktree-setup-output")
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.border)
                        .rounded(Radius::Medium.px(&theme))
                        .p(DynamicSpacing::Base08.px(&theme))
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(11.))
                        .text_color(theme.text_secondary)
                        .child(if output.trim().is_empty() {
                            tr!("worktree.setup.no_output")
                        } else {
                            output
                        }),
                );
            }
        }
        Some(card.into_any_element())
    }

    /// The page-level error banner, with the raw Git text preserved.
    fn worktrees_error_banner(&self, theme: Theme, this: Entity<OrbitApp>) -> Option<AnyElement> {
        let error = self.worktrees_error.clone()?;
        Some(
            div()
                .w_full()
                .min_w_0()
                .bg(theme.error.opacity(0.08))
                .border_1()
                .border_color(theme.error.opacity(0.35))
                .rounded(panel::RADIUS.px(&theme))
                .px(DynamicSpacing::Base16.px(&theme))
                .py(DynamicSpacing::Base12.px(&theme))
                .flex()
                .items_start()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(icon(
                    "icons/circle-x.svg",
                    IconSize::Medium.px(&theme),
                    theme.error,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::code_font_family())
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_primary)
                        .child(error),
                )
                .child(worktree_text_button(
                    "worktrees-error-dismiss",
                    tr!("common.dismiss"),
                    theme,
                    this,
                    |app, cx| {
                        app.worktrees_error = None;
                        cx.notify();
                    },
                ))
                .into_any_element(),
        )
    }

    /// A dismissible primer shown above the list: what a worktree is and how
    /// the pieces fit together. Once dismissed it stays gone (persisted with
    /// the other one-time hints), so the page then shows only the list.
    pub(super) fn worktree_howto_card(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Option<AnyElement> {
        if self.worktree_howto_dismissed {
            return None;
        }
        let dismiss = button_frame(
            div().id("worktrees-howto-dismiss"),
            &theme,
            ButtonSize::Default,
        )
        .cursor_pointer()
        .hover_on_glass(&theme)
        .tip(tr!("common.dismiss"))
        .child(icon(
            "icons/x.svg",
            ButtonSize::Default.icon_size().px(&theme),
            theme.text_muted,
        ))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| {
                app.worktree_howto_dismissed = true;
                crate::transcript::persist_hint(WORKTREE_HOWTO_HINT_KEY);
                cx.notify();
            });
        });
        Some(
            div()
                .debug_selector(|| "worktree-howto-card".to_string())
                .w_full()
                .min_w_0()
                .bg(theme.surface_secondary)
                .border_1()
                .border_color(theme.border)
                .rounded(panel::RADIUS.px(&theme))
                .px(DynamicSpacing::Base16.px(&theme))
                .py(DynamicSpacing::Base12.px(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(icon(
                            "icons/info.svg",
                            IconSize::Small.px(&theme),
                            theme.text_muted,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_weight(FontWeight::MEDIUM)
                                .child(tr!("worktree.howto.title")),
                        )
                        .child(dismiss),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(tr!("worktree.howto.intro")),
                )
                .child(worktree_howto_line(theme, tr!("worktree.howto.create")))
                .child(worktree_howto_line(theme, tr!("worktree.howto.open")))
                .child(worktree_howto_line(theme, tr!("worktree.howto.setup")))
                .child(worktree_howto_line(theme, tr!("worktree.howto.rename")))
                .into_any_element(),
        )
    }

    /// The list of worktrees for the active repository.
    fn worktrees_list(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        if self.worktrees.is_empty() {
            rows.push(worktree_empty_state(
                theme,
                self.worktrees_busy,
                self.worktrees_error.is_none(),
            ));
        } else {
            for worktree in &self.worktrees {
                rows.push(self.worktree_row(worktree, theme, this.clone(), cx));
            }
        }
        self.settings_section(theme, &tr!("worktree.page.section"), rows)
    }

    /// One worktree row: a status marker, the name, its branch, its path, and
    /// the row-actions menu.
    fn worktree_row(
        &self,
        worktree: &Worktree,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let active = self.worktree_is_active(&worktree.path);
        let name = worktree.name.clone();
        let path = worktree.path.clone();
        let branch = worktree.branch_label();
        let menu_open = self
            .worktree_menu
            .as_ref()
            .filter(|menu| worktree::same_path(&menu.path, &worktree.path))
            .cloned();

        div()
            .id(ElementId::Name(
                format!("worktree-row-{}", path.to_string_lossy()).into(),
            ))
            .group("worktree-row")
            .relative()
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .cursor_pointer()
            .when(active, |row| row.bg(theme.surface_selected))
            .hover_state(&theme)
            .on_mouse_down(MouseButton::Right, {
                let this = this.clone();
                let path = path.clone();
                move |event: &MouseDownEvent, _, cx| {
                    this.update(cx, |app, cx| {
                        app.toggle_worktree_menu(path.clone(), Some(event.position), cx)
                    });
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let this = this.clone();
                let path = path.clone();
                move |_, _, cx| {
                    this.update(cx, |app, cx| app.open_worktree(&path, cx));
                }
            })
            .child(worktree_markers(worktree, theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(name.clone()),
                            )
                            .when(active, |row| {
                                row.child(
                                    div()
                                        .px(DynamicSpacing::Base04.px(&theme))
                                        .rounded(Radius::Small.px(&theme))
                                        .bg(theme.surface_active)
                                        .text_size(TextSize::XSmall.px(&theme))
                                        .text_color(theme.accent_text)
                                        .child(tr!("worktree.row.active")),
                                )
                            })
                            .when(worktree.bare, |row| {
                                row.child(
                                    div()
                                        .text_size(TextSize::XSmall.px(&theme))
                                        .text_color(theme.text_muted)
                                        .child(tr!("worktree.bare")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(icon(
                                "icons/branch.svg",
                                IconSize::XSmall.px(&theme),
                                theme.text_muted,
                            ))
                            .child(
                                div()
                                    .font_family(theme::code_font_family())
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_secondary)
                                    .child(branch),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .font_family(theme::code_font_family())
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_muted)
                            .child(shorten_home(&path)),
                    ),
            )
            .child({
                let button_this = this.clone();
                let menu_this = this.clone();
                let path_for_menu = path.clone();
                let row_menu = menu_open.clone();
                icon_button_frame(
                    div().id(ElementId::Name(
                        format!("worktree-more-{}", path.to_string_lossy()).into(),
                    )),
                    &theme,
                    ButtonSize::Compact,
                )
                .group(BUTTON_GROUP)
                .relative()
                .tip(tr!("sidebar.more_actions"))
                .cursor_pointer()
                .opacity(if row_menu.is_some() { 1.0 } else { 0.0 })
                .group_hover("worktree-row", |style| style.opacity(1.))
                .pressed_state(&theme, PressedRamp::Wash)
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    button_this.update(cx, |app, cx| {
                        app.toggle_worktree_menu(path_for_menu.clone(), None, cx)
                    });
                })
                .child(icon(
                    "icons/more.svg",
                    ButtonSize::Compact.icon_size().px(&theme),
                    theme.text_muted,
                ))
                .children(row_menu.map(|menu| worktree_row_menu(&menu, worktree, theme, menu_this)))
            })
            .into_any_element()
    }

    /// The Worktrees dialog layer (create / permission / rename / move /
    /// remove / branch-in-use), rendered above the app's other surfaces.
    pub(super) fn worktree_dialog_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.worktree_dialog.as_ref()?;
        let body: AnyElement = match dialog {
            WorktreeDialog::Clone => self.clone_dialog_body(theme, this.clone()),
            WorktreeDialog::Create => self.create_worktree_dialog_body(theme, this.clone(), cx),
            WorktreeDialog::AllowSetup(pending) => {
                self.allow_setup_dialog_body(pending, theme, this.clone())
            }
            WorktreeDialog::Rename { path } => {
                self.rename_worktree_dialog_body(path, theme, this.clone())
            }
            WorktreeDialog::Move { path } => {
                self.move_worktree_dialog_body(path, theme, this.clone())
            }
            WorktreeDialog::Remove { path, dirty } => {
                self.remove_worktree_dialog_body(path, *dirty, theme, this.clone())
            }
            WorktreeDialog::BranchInUse { branch, path } => {
                self.branch_in_use_dialog_body(branch, path, theme, this.clone())
            }
            WorktreeDialog::Unavailable { path, local } => {
                self.worktree_unavailable_dialog_body(path, local.as_deref(), theme, this.clone())
            }
        };
        Some(
            modal_scrim(&theme)
                .id("worktree-dialog-layer")
                .when(matches!(dialog, WorktreeDialog::Clone), |scrim| {
                    scrim
                        .key_context("CloneDialog")
                        .on_action(cx.listener(Self::on_clone_next))
                        .on_action(cx.listener(Self::on_clone_prev))
                        .on_action(cx.listener(Self::on_clone_activate))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::on_worktree_dialog_scrim),
                )
                // `body` is already an `AnyElement`, which is not `Styled`; the
                // plain div is the smallest thing the entrance can fade.
                .child(crate::motion::enter(
                    div().child(body),
                    "worktree-dialog-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }

    /// The shared dialog card frame.
    fn worktree_dialog_card(&self, title: String, theme: Theme, body: AnyElement) -> AnyElement {
        modal_card(&theme)
            .w(px(520.))
            .max_h(px(560.))
            .child(
                div()
                    .px(modal::header_padding_x(&theme))
                    .pt(modal::header_padding_top(&theme))
                    .pb(modal::header_padding_bottom(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(title),
                    ),
            )
            .child(
                div()
                    .id("worktree-dialog-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(modal::header_padding_x(&theme))
                    .pb(modal::header_padding_bottom(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(body),
            )
            .into_any_element()
    }

    fn worktree_dialog_error(&self, theme: Theme) -> Option<AnyElement> {
        let error = self.worktree_dialog_error.clone()?;
        Some(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.error)
                .child(error)
                .into_any_element(),
        )
    }

    fn clone_dialog_body(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let mut body = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme));
        if let Some(operation) = &self.clone_operation {
            let progress = operation
                .progress
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default();
            body = body
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(if progress.is_empty() {
                            tr!("working_folder.clone_running")
                        } else {
                            progress
                        }),
                )
                .child(clone_dialog_button(
                    "clone-cancel",
                    tr!("view.cancel"),
                    theme,
                    self.clone_cancel_focus.clone(),
                    false,
                    this.clone(),
                    |app, window, cx| {
                        app.cancel_worktree_dialog(cx);
                        app.input.read(cx).focus(window);
                    },
                ));
        } else {
            let browse = clone_dialog_button(
                "clone-choose-parent",
                tr!("working_folder.clone_browse"),
                theme,
                self.clone_browse_focus.clone(),
                false,
                this.clone(),
                |app, window, cx| app.browse_clone_parent(window, cx),
            );
            body = body
                .child(worktree_field(
                    &theme,
                    tr!("working_folder.clone_url"),
                    None,
                    input_field_frame(div(), &theme)
                        .w_full()
                        .bg(theme.background)
                        .child(self.clone_url_input.clone()),
                ))
                .child(worktree_field(
                    &theme,
                    tr!("working_folder.clone_name"),
                    None,
                    input_field_frame(div(), &theme)
                        .w_full()
                        .bg(theme.background)
                        .child(self.clone_name_input.clone()),
                ))
                .child(worktree_field(
                    &theme,
                    tr!("working_folder.clone_parent"),
                    None,
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(browse)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(theme.text_muted)
                                .child(
                                    self.clone_parent
                                        .as_deref()
                                        .map(shorten_home)
                                        .unwrap_or_else(|| {
                                            tr!("working_folder.clone_parent_empty")
                                        }),
                                ),
                        ),
                ))
                .children(self.worktree_dialog_error(theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(modal::footer_gap(&theme))
                        .border_t_1()
                        .border_color(theme.border)
                        .pt(modal::footer_padding(&theme))
                        .child(clone_dialog_button(
                            "clone-dialog-cancel",
                            tr!("view.cancel"),
                            theme,
                            self.clone_cancel_focus.clone(),
                            false,
                            this.clone(),
                            |app, window, cx| {
                                app.cancel_worktree_dialog(cx);
                                app.input.read(cx).focus(window);
                            },
                        ))
                        .child(clone_dialog_button(
                            "clone-dialog-confirm",
                            tr!("working_folder.clone_action"),
                            theme,
                            self.clone_confirm_focus.clone(),
                            true,
                            this,
                            |app, window, cx| {
                                app.submit_clone(cx);
                                if app.clone_operation.is_some() {
                                    window.focus(&app.clone_cancel_focus);
                                }
                            },
                        )),
                );
        }
        self.worktree_dialog_card(
            tr!("working_folder.clone_title"),
            theme,
            body.into_any_element(),
        )
    }

    /// Create dialog body.
    fn create_worktree_dialog_body(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let name_field = worktree_field(
            &theme,
            tr!("worktree.field.name"),
            Some(&tr!("worktree.field.name_hint")),
            input_field_frame(div(), &theme)
                .w_full()
                .bg(theme.background)
                .child(self.worktree_name_input.clone()),
        );
        let mode = self.worktree_branch_mode;
        let mut branch_section = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(worktree_field_label(
                &theme,
                tr!("worktree.field.branch"),
                Some(&tr!("worktree.field.branch_hint")),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .child(worktree_mode_chip(
                        "worktree-mode-new",
                        tr!("worktree.field.branch_new"),
                        mode == WorktreeBranchMode::New,
                        theme,
                        this.clone(),
                        |app, cx| {
                            app.worktree_branch_mode = WorktreeBranchMode::New;
                            cx.notify();
                        },
                    ))
                    .child(worktree_mode_chip(
                        "worktree-mode-existing",
                        tr!("worktree.field.branch_existing"),
                        mode == WorktreeBranchMode::Existing,
                        theme,
                        this.clone(),
                        |app, cx| {
                            app.worktree_branch_mode = WorktreeBranchMode::Existing;
                            cx.notify();
                        },
                    )),
            );
        if mode == WorktreeBranchMode::New {
            branch_section = branch_section
                .child(
                    input_field_frame(div(), &theme)
                        .w_full()
                        .bg(theme.background)
                        .child(self.worktree_branch_input.clone()),
                )
                .child(
                    input_field_frame(div(), &theme)
                        .w_full()
                        .bg(theme.background)
                        .child(self.worktree_start_input.clone()),
                )
                .child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("worktree.field.start_hint")),
                );
        } else if self.worktree_branches.is_empty() {
            branch_section = branch_section.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("worktree.field.loading_branches")),
            );
        } else {
            let mut list = div()
                .id("worktree-branch-list")
                .max_h(px(160.))
                .overflow_y_scroll()
                .border_1()
                .border_color(theme.border)
                .rounded(panel::RADIUS.px(&theme))
                .flex()
                .flex_col();
            for branch in &self.worktree_branches {
                let selected = self.worktree_branch_choice.as_deref() == Some(branch.as_str());
                let branch_click = branch.clone();
                let row_this = this.clone();
                list = list.child(
                    div()
                        .id(ElementId::Name(format!("worktree-branch-{branch}").into()))
                        .w_full()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .py(DynamicSpacing::Base06.px(&theme))
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .cursor_pointer()
                        .when(selected, |row| row.bg(theme.surface_selected))
                        .hover_state(&theme)
                        .child(icon(
                            if selected {
                                "icons/check.svg"
                            } else {
                                "icons/branch.svg"
                            },
                            IconSize::Small.px(&theme),
                            if selected {
                                theme.accent_text
                            } else {
                                theme.text_muted
                            },
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(theme::code_font_family())
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(if selected {
                                    theme.text_primary
                                } else {
                                    theme.text_secondary
                                })
                                .child(branch.clone()),
                        )
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            row_this.update(cx, |app, cx| {
                                app.worktree_branch_choice = Some(branch_click.clone());
                                cx.notify();
                            });
                        }),
                );
            }
            branch_section = branch_section.child(list);
            // A remote-tracking ref is not checked out as-is: it becomes a
            // local branch that tracks it. Say which one before Create runs,
            // instead of leaving the user to discover it after.
            if let Some(row) = self
                .worktree_branch_choice
                .as_deref()
                .and_then(|choice| self.tracking_branch_for(choice))
            {
                branch_section = branch_section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(icon(
                            "icons/branch.svg",
                            IconSize::XSmall.px(&theme),
                            theme.text_muted,
                        ))
                        .child(tr!(
                            "working_folder.tracking_hint",
                            branch = row.local_branch,
                            upstream = row.remote_ref
                        )),
                );
            }
        }

        // Location preview: `<root>/<name>`, computed from the cached root so
        // no Git runs while typing.
        let location = match (
            self.worktree_root_hint(),
            self.worktree_name_input.read(cx).text(),
        ) {
            (Some(root), name) if !name.trim().is_empty() => {
                shorten_home(&worktree::expand_path(&root, name.trim()))
            }
            (Some(root), _) => shorten_home(&root),
            _ => String::new(),
        };
        let location_row = worktree_field(
            &theme,
            tr!("worktree.field.location"),
            None,
            div()
                .w_full()
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .font_family(theme::code_font_family())
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(if location.is_empty() {
                    tr!("worktree.field.location_unknown")
                } else {
                    location
                })
                .into_any_element(),
        );

        let script = self
            .worktree_root_hint()
            .and_then(|root| self.worktree_config.effective_setup_script(&root));
        let setup_row = script.map(|script| {
            div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(self.settings_toggle(
                    "worktree-create-run-setup",
                    self.worktree_run_setup,
                    theme,
                    this.clone(),
                    Self::toggle_create_run_setup,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base02.px(&theme))
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_secondary)
                                .child(tr!("worktree.field.run_setup")),
                        )
                        .child(
                            div()
                                .truncate()
                                .font_family(theme::code_font_family())
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(shorten_home(&script)),
                        ),
                )
                .into_any_element()
        });

        let footer = worktree_dialog_footer(
            theme,
            tr!("view.cancel"),
            tr!("worktree.dialog.create"),
            this,
            |app, cx| app.cancel_worktree_dialog(cx),
            |app, cx| app.confirm_worktree_dialog(cx),
        );
        self.worktree_dialog_card(
            tr!("worktree.dialog.create_title"),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(name_field)
                .child(branch_section)
                .child(location_row)
                .children(setup_row)
                .children(self.worktree_dialog_error(theme))
                .child(footer)
                .into_any_element(),
        )
    }

    /// Setup-script permission dialog (§33).
    fn allow_setup_dialog_body(
        &self,
        pending: &PendingWorktreeCreate,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let script = self
            .worktree_root_hint()
            .and_then(|root| self.worktree_config.effective_setup_script(&root));
        let text = tr!(
            "worktree.allow_setup.body",
            name = pending.name.clone(),
            script = script
                .as_deref()
                .map(shorten_home)
                .unwrap_or_else(|| tr!("worktree.allow_setup.unknown_script"))
        );
        let footer = worktree_dialog_footer(
            theme,
            tr!("view.cancel"),
            tr!("worktree.allow_setup.allow"),
            this,
            |app, cx| app.cancel_worktree_dialog(cx),
            |app, cx| app.confirm_worktree_dialog(cx),
        );
        self.worktree_dialog_card(
            tr!("worktree.allow_setup.title"),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(text),
                )
                .child(footer)
                .into_any_element(),
        )
    }

    fn rename_worktree_dialog_body(
        &self,
        path: &Path,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let current = self.worktree_display_name(path);
        let field = worktree_field(
            &theme,
            tr!("worktree.field.new_name"),
            Some(&tr!("worktree.field.rename_hint")),
            input_field_frame(div(), &theme)
                .w_full()
                .bg(theme.background)
                .child(self.worktree_field_input.clone()),
        );
        let footer = worktree_dialog_footer(
            theme,
            tr!("view.cancel"),
            tr!("worktree.dialog.rename"),
            this,
            |app, cx| app.cancel_worktree_dialog(cx),
            |app, cx| app.confirm_worktree_dialog(cx),
        );
        self.worktree_dialog_card(
            tr!("worktree.dialog.rename_title", name = current),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(field)
                .children(self.worktree_dialog_error(theme))
                .child(footer)
                .into_any_element(),
        )
    }

    fn move_worktree_dialog_body(
        &self,
        path: &Path,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let current = self.worktree_display_name(path);
        let field = worktree_field(
            &theme,
            tr!("worktree.field.destination"),
            Some(&tr!("worktree.field.move_hint")),
            input_field_frame(div(), &theme)
                .w_full()
                .bg(theme.background)
                .child(self.worktree_field_input.clone()),
        );
        let footer = worktree_dialog_footer(
            theme,
            tr!("view.cancel"),
            tr!("worktree.dialog.move"),
            this,
            |app, cx| app.cancel_worktree_dialog(cx),
            |app, cx| app.confirm_worktree_dialog(cx),
        );
        self.worktree_dialog_card(
            tr!("worktree.dialog.move_title", name = current),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(field)
                .children(self.worktree_dialog_error(theme))
                .child(footer)
                .into_any_element(),
        )
    }

    fn remove_worktree_dialog_body(
        &self,
        path: &Path,
        dirty: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let worktree = self.listed_worktree(path);
        let name = worktree
            .map(|worktree| worktree.name.clone())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let branch = worktree.map(Worktree::branch_label).unwrap_or_default();
        let body = if dirty {
            tr!("worktree.remove.dirty_body")
        } else {
            tr!("worktree.remove.clean_body")
        };
        let confirm_label = if dirty {
            tr!("worktree.remove.remove_anyway")
        } else {
            tr!("worktree.remove.remove")
        };
        let footer = worktree_dialog_footer(
            theme,
            tr!("view.cancel"),
            confirm_label,
            this,
            |app, cx| app.cancel_worktree_dialog(cx),
            |app, cx| app.confirm_worktree_dialog(cx),
        );
        self.worktree_dialog_card(
            tr!("worktree.remove.title"),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base02.px(&theme))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(name),
                        )
                        .child(
                            div()
                                .font_family(theme::code_font_family())
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(branch),
                        )
                        .child(
                            div()
                                .truncate()
                                .font_family(theme::code_font_family())
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(shorten_home(path)),
                        ),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(if dirty {
                            theme.error
                        } else {
                            theme.text_secondary
                        })
                        .child(body),
                )
                .child(footer)
                .into_any_element(),
        )
    }

    /// The session's workspace is gone; show where it pointed and offer the
    /// Worktrees page. Nothing is re-pointed automatically (§22).
    fn worktree_unavailable_dialog_body(
        &self,
        path: &Path,
        local: Option<&Path>,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(modal::footer_gap(&theme))
            .border_t_1()
            .border_color(theme.border)
            .pt(modal::footer_padding(&theme))
            .child(worktree_text_button(
                "worktree-unavailable-dismiss",
                tr!("common.dismiss"),
                theme,
                this.clone(),
                |app, cx| app.cancel_worktree_dialog(cx),
            ))
            .when(local.is_some(), |footer| {
                footer.child(
                    div()
                        .debug_selector(|| "worktree-unavailable-local".to_string())
                        .child(worktree_text_button(
                            "worktree-unavailable-local",
                            tr!("worktree.unavailable.start_local"),
                            theme,
                            this.clone(),
                            |app, cx| app.start_task_in_missing_worktree_local(cx),
                        )),
                )
            })
            .child(worktree_primary_button(
                "worktree-unavailable-open",
                tr!("worktree.unavailable.locate"),
                theme,
                this,
                |app, cx| {
                    app.worktree_dialog = None;
                    app.open_worktrees(cx);
                },
            ));
        self.worktree_dialog_card(
            tr!("worktree.unavailable.title"),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(tr!("worktree.unavailable.body")),
                )
                .child(
                    div()
                        .truncate()
                        .font_family(theme::code_font_family())
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(shorten_home(path)),
                )
                .children(local.map(|local| {
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(tr!(
                            "worktree.unavailable.local_hint",
                            path = shorten_home(local)
                        ))
                }))
                .children(self.worktree_dialog_error(theme))
                .child(footer)
                .into_any_element(),
        )
    }

    fn branch_in_use_dialog_body(
        &self,
        branch: &str,
        path: &Path,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let path = path.to_path_buf();
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(modal::footer_gap(&theme))
            .border_t_1()
            .border_color(theme.border)
            .pt(modal::footer_padding(&theme))
            .child(worktree_text_button(
                "worktree-bi-cancel",
                tr!("view.cancel"),
                theme,
                this.clone(),
                |app, cx| app.cancel_worktree_dialog(cx),
            ))
            .child(worktree_primary_button(
                "worktree-bi-open",
                tr!("worktree.branch_in_use.open"),
                theme,
                this,
                move |app, cx| {
                    app.worktree_dialog = None;
                    app.open_worktree(&path, cx);
                },
            ));
        self.worktree_dialog_card(
            tr!("worktree.branch_in_use.title"),
            theme,
            div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(
                    div()
                        .font_family(theme::code_font_family())
                        .text_color(theme.text_primary)
                        .child(branch.to_string()),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(tr!("worktree.branch_in_use.body")),
                )
                .child(footer)
                .into_any_element(),
        )
    }
}

/// Identify an owner from Git's worktree lists, never from directory shape.
/// Ambiguous or unlisted paths have no safe Local target.
pub(super) fn missing_worktree_local(missing: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    let mut owner: Option<PathBuf> = None;
    for candidate in candidates.iter().filter(|candidate| candidate.is_dir()) {
        let Ok(worktrees) = WorktreeManager::new(candidate.clone()).list() else {
            continue;
        };
        if !worktrees
            .iter()
            .any(|worktree| !worktree.is_main && worktree::same_path(&worktree.path, missing))
        {
            continue;
        }
        let Some(main) = worktrees
            .iter()
            .find(|worktree| worktree.is_main && !worktree.bare && worktree.path.is_dir())
        else {
            continue;
        };
        match &owner {
            None => owner = Some(main.path.clone()),
            Some(existing) if worktree::same_path(existing, &main.path) => {}
            Some(_) => return None,
        }
    }
    owner
}

// ── "Work in" picker (status bar) ─────────────────────────────────────
//
// One control answers "where does this task run?" for every kind of place
// Orbit supports: the repository's main checkout, a linked worktree, a
// worktree folder that Git no longer lists, a new worktree (from a local or a
// remote branch), or any folder at all. The chip replaces the old
// workspace-chip + work-in-chip pair — a readout that restarted the session and
// a switch that appeared only inside a worktree.
//
// The rows are built from the cached worktree list, so render never runs Git.
// Remote refs are the one exception: they are fetched when their level opens,
// because a repository can have hundreds and the picker is opened far more
// often than that level is.

/// One row of the status bar's "Work in" picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkInRow {
    /// An existing place: the main checkout, a linked worktree.
    Folder(crate::working_folder::FolderRow),
    /// Create a new worktree (opens the create dialog).
    NewWorktree,
    /// Browse to a worktree folder Git may not have listed.
    ExistingWorktree,
    /// Open the remote-branch level.
    RemoteRef,
    /// Clone a repository from a URL.
    Clone,
    /// The native folder dialog, for any directory.
    ChooseFolder,
    /// A folder from a recent session outside the listed locations.
    Recent(PathBuf),
}

impl OrbitApp {
    /// The repository's main checkout, from the cached list. `None` when the
    /// active workspace is not a Git repository, which is what hides the chip.
    pub(super) fn work_in_root(&self) -> Option<PathBuf> {
        self.worktree_root_hint()
            .or_else(|| self.main_worktree_path())
    }

    /// Every row of the picker's first level: the locations Git reports, then
    /// the ways to reach a new one. `None` when there is no repository, so the
    /// control hides and non-worktree projects are unchanged (§39).
    pub(super) fn work_in_rows(&self) -> Option<Vec<WorkInRow>> {
        let root = self.work_in_root()?;
        let mut rows: Vec<WorkInRow> = crate::working_folder::location_rows(
            &self.worktrees,
            &root,
            self.current_workspace.as_deref(),
        )
        .into_iter()
        .map(WorkInRow::Folder)
        .collect();
        rows.push(WorkInRow::NewWorktree);
        rows.push(WorkInRow::ExistingWorktree);
        rows.push(WorkInRow::RemoteRef);
        rows.push(WorkInRow::Clone);
        rows.push(WorkInRow::ChooseFolder);

        // Session cwd values are canonicalized when sessions are loaded. Use
        // the same identity for worktrees so aliases do not produce two rows.
        let mut seen: HashSet<PathBuf> = self
            .worktrees
            .iter()
            .map(|worktree| sessions::canonical_workspace_path(&worktree.path))
            .collect();
        seen.insert(sessions::canonical_workspace_path(&root));
        if let Some(active) = &self.current_workspace {
            seen.insert(sessions::canonical_workspace_path(active));
        }
        let mut recent_count = 0;
        for session in &self.sessions {
            if seen.insert(session.cwd.clone()) {
                rows.push(WorkInRow::Recent(session.cwd.clone()));
                recent_count += 1;
                if recent_count == crate::workspace_picker::MAX_RECENTS {
                    break;
                }
            }
        }
        Some(rows)
    }

    /// The main worktree path, when one is known.
    fn main_worktree_path(&self) -> Option<PathBuf> {
        self.worktrees
            .iter()
            .find(|worktree| worktree.is_main)
            .map(|worktree| worktree.path.clone())
    }

    /// Where the working folder is now. Classified from the filesystem against
    /// the cached root, so the chip label costs one `read` and never a Git
    /// call — and it stays truthful about a broken worktree.
    pub(super) fn working_folder(&self) -> Option<crate::working_folder::WorkingFolder> {
        let active = self.current_workspace.as_deref()?;
        // Outside a repository there is no location to choose between: the
        // plain folder chip names the workspace and changes it.
        let root = self.work_in_root()?;
        let location = crate::working_folder::classify(active, Some(&root));
        Some(crate::working_folder::WorkingFolder::new(
            active.to_path_buf(),
            location,
        ))
    }

    /// The chip label: `Local` at the main checkout, the worktree's name inside
    /// one, the directory name anywhere else.
    pub(super) fn work_in_label(&self) -> Option<String> {
        self.working_folder().map(|folder| folder.label)
    }

    /// Index of the row the working folder is on, for the initial highlight.
    pub(super) fn work_in_selected_ix(&self, rows: &[WorkInRow]) -> usize {
        rows.iter()
            .position(|row| matches!(row, WorkInRow::Folder(folder) if folder.selected))
            .unwrap_or(0)
    }

    /// The remote-branch rows: remote-tracking refs resolved into the local
    /// branch each one would create.
    pub(super) fn work_in_remote_rows(&self) -> Vec<crate::working_folder::RemoteRow> {
        crate::working_folder::remote_ref_rows(
            &self.worktree_remotes,
            &self.worktree_remote_names,
            &self.worktree_local_branches,
        )
    }

    /// Rows the highlighted index walks over, for the level that is showing.
    fn work_in_row_count(&self) -> usize {
        if self.work_in_remote_open {
            self.work_in_remote_rows().len()
        } else {
            self.work_in_rows().map_or(0, |rows| rows.len())
        }
    }

    /// Toggle the "Work in" picker. Mutually exclusive with the composer and
    /// status-bar popovers it would overlap.
    pub(super) fn toggle_work_in_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.work_in_menu_open {
            self.close_work_in_menu(window, cx);
            return;
        }
        self.branch_picker = None;
        self.model_selector = None;
        self.context_popup = ContextPopup::None;
        self.access_menu_open = false;
        self.workflow_menu_open = false;
        self.add_menu_open = false;
        self.open_in_menu_open = false;
        self.workspace_picker = None;
        self.worktree_menu = None;
        let rows = self.work_in_rows().unwrap_or_default();
        self.work_in_menu_highlight = self.work_in_selected_ix(&rows);
        self.work_in_scroll.set_offset(point(px(0.), px(0.)));
        self.work_in_remote_open = false;
        self.work_in_details_open = false;
        self.work_in_remote_highlight = 0;
        self.work_in_menu_open = true;
        // The list a worktree was created in from a terminal may be one
        // revision behind; refresh once on open (a background list, not a
        // blocking call).
        self.refresh_worktrees(cx);
        window.focus(&self.work_in_menu_focus);
        cx.notify();
    }

    /// Close the picker without touching focus — for the other popovers that
    /// must not overlap it. Resets the remote level too, so the next open
    /// starts on the locations.
    pub(super) fn dismiss_work_in_menu(&mut self) {
        self.work_in_menu_open = false;
        self.work_in_remote_open = false;
        self.work_in_details_open = false;
    }

    pub(super) fn close_work_in_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.work_in_menu_open {
            self.work_in_menu_open = false;
            self.work_in_remote_open = false;
            self.work_in_details_open = false;
            self.input.read(cx).focus(window);
            cx.notify();
        }
    }

    pub(super) fn on_work_in_trigger_click(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The click's mouse-down dismisses any open menu; without this guard
        // the same click's mouse-up would toggle it straight back open.
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        self.toggle_work_in_menu(window, cx);
    }

    pub(super) fn on_work_in_next(
        &mut self,
        _: &crate::WorkInMenuNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.work_in_details_open {
            self.scroll_work_in_details(false);
            cx.notify();
            return;
        }
        let count = self.work_in_row_count();
        if count > 0 {
            let highlighted = if self.work_in_remote_open {
                &mut self.work_in_remote_highlight
            } else {
                &mut self.work_in_menu_highlight
            };
            *highlighted = (*highlighted + 1) % count;
            self.reveal_work_in_highlight(cx);
        }
        cx.notify();
    }

    pub(super) fn on_work_in_prev(
        &mut self,
        _: &crate::WorkInMenuPrev,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.work_in_details_open {
            self.scroll_work_in_details(true);
            cx.notify();
            return;
        }
        let count = self.work_in_row_count();
        if count > 0 {
            let highlighted = if self.work_in_remote_open {
                &mut self.work_in_remote_highlight
            } else {
                &mut self.work_in_menu_highlight
            };
            *highlighted = (*highlighted + count - 1) % count;
            self.reveal_work_in_highlight(cx);
        }
        cx.notify();
    }

    fn scroll_work_in_details(&self, backwards: bool) {
        let mut offset = self.work_in_scroll.offset();
        let distance = if backwards { px(80.) } else { px(-80.) };
        offset.y = (offset.y + distance).clamp(-self.work_in_scroll.max_offset().height, px(0.));
        self.work_in_scroll.set_offset(offset);
    }

    fn toggle_work_in_details(&mut self, cx: &mut Context<Self>) {
        if !self.work_in_details_open
            && (!self.work_in_remote_open
                || self
                    .work_in_remote_rows()
                    .get(self.work_in_remote_highlight)
                    .is_none())
        {
            return;
        }
        self.work_in_details_open = !self.work_in_details_open;
        self.work_in_scroll.set_offset(point(px(0.), px(0.)));
        if !self.work_in_details_open {
            self.work_in_scroll
                .scroll_to_item(self.work_in_remote_highlight + 1);
        }
        cx.notify();
    }

    fn on_work_in_details(
        &mut self,
        _: &crate::WorkInMenuDetails,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_work_in_details(cx);
    }

    /// Keep keyboard navigation visible across the mixed-height sections.
    fn reveal_work_in_highlight(&self, cx: &Context<Self>) {
        if self.work_in_remote_open {
            self.work_in_scroll
                .scroll_to_item(self.work_in_remote_highlight + 1);
            return;
        }
        let Some(rows) = self.work_in_rows() else {
            return;
        };
        let folders = rows
            .iter()
            .filter(|row| matches!(row, WorkInRow::Folder(_)))
            .count();
        let actions = rows
            .iter()
            .filter(|row| !matches!(row, WorkInRow::Folder(_) | WorkInRow::Recent(_)))
            .count();
        let index = self.work_in_menu_highlight;
        let theme = theme::get(cx);
        let (group, offset, height) = if index < folders {
            (0, index, picker::two_line_entry_height(theme))
        } else if index < folders + actions {
            (
                if folders > 0 { 2 } else { 1 },
                index - folders,
                picker::entry_height(theme),
            )
        } else {
            (
                if folders > 0 { 5 } else { 4 },
                index - folders - actions,
                picker::two_line_entry_height(theme),
            )
        };
        let Some(bounds) = self.work_in_scroll.bounds_for_item(group) else {
            return;
        };
        let viewport = self.work_in_scroll.bounds();
        let mut scroll = self.work_in_scroll.offset();
        let top = bounds.top() + height * offset as f32 + scroll.y;
        let bottom = top + height;
        if top < viewport.top() {
            scroll.y += viewport.top() - top;
        } else if bottom > viewport.bottom() {
            scroll.y -= bottom - viewport.bottom();
        }
        self.work_in_scroll.set_offset(scroll);
    }

    pub(super) fn on_work_in_confirm(
        &mut self,
        _: &crate::WorkInMenuConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.work_in_remote_open {
            let ix = self.work_in_remote_highlight;
            self.run_work_in_remote_item(ix, window, cx);
            return;
        }
        let ix = self.work_in_menu_highlight;
        self.run_work_in_item(ix, window, cx);
    }

    pub(super) fn on_work_in_close(
        &mut self,
        _: &crate::WorkInMenuClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Escape returns from details to the remote list, then to locations.
        if self.work_in_details_open {
            self.toggle_work_in_details(cx);
            return;
        }
        if self.work_in_remote_open {
            self.work_in_remote_open = false;
            self.work_in_scroll.set_offset(point(px(0.), px(0.)));
            self.work_in_menu_highlight = self
                .work_in_rows()
                .map(|rows| self.work_in_selected_ix(&rows))
                .unwrap_or(0);
            cx.notify();
            return;
        }
        self.close_work_in_menu(window, cx);
    }

    /// Back out of the remote level (the header's back row).
    pub(super) fn back_work_in_remote(&mut self, cx: &mut Context<Self>) {
        if self.work_in_details_open {
            self.toggle_work_in_details(cx);
            return;
        }
        self.work_in_remote_open = false;
        self.work_in_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// Run row `ix` of the picker's first level.
    pub(super) fn run_work_in_item(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rows) = self.work_in_rows() else {
            self.close_work_in_menu(window, cx);
            return;
        };
        let Some(row) = rows.get(ix).cloned() else {
            self.close_work_in_menu(window, cx);
            return;
        };
        match row {
            WorkInRow::Folder(folder) => {
                let target = folder.folder;
                if folder.prunable {
                    // Git lists it but the directory moved or vanished:
                    // reconnect it rather than fail on an empty click.
                    self.close_work_in_menu(window, cx);
                    self.adopt_worktree(target.path, window, cx);
                    return;
                }
                self.work_in_menu_open = false;
                self.work_in_remote_open = false;
                if !target.location.is_usable() {
                    self.close_work_in_menu(window, cx);
                    self.adopt_worktree(target.path, window, cx);
                    return;
                }
                if !self.worktree_is_active(&target.path) {
                    self.open_worktree(&target.path, cx);
                }
            }
            WorkInRow::NewWorktree => {
                self.work_in_menu_open = false;
                self.open_new_worktree_from_work_in(window, cx);
            }
            WorkInRow::ExistingWorktree => {
                self.work_in_menu_open = false;
                self.browse_for_existing_worktree(window, cx);
            }
            WorkInRow::RemoteRef => {
                self.work_in_scroll.set_offset(point(px(0.), px(0.)));
                self.work_in_details_open = false;
                self.work_in_remote_open = true;
                self.work_in_remote_highlight = 0;
                self.load_worktree_remotes(cx);
                cx.notify();
            }
            WorkInRow::Clone => {
                self.work_in_menu_open = false;
                self.open_clone_dialog(window, cx);
            }
            WorkInRow::ChooseFolder => {
                self.work_in_menu_open = false;
                self.browse_for_folder(window, cx);
            }
            WorkInRow::Recent(path) => {
                self.close_work_in_menu(window, cx);
                match crate::working_folder::classify(&path, None) {
                    crate::working_folder::Location::Broken(
                        crate::working_folder::BrokenReason::Missing,
                    ) => {
                        self.toast_error(tr!(
                            "worktree.error.missing_directory",
                            path = path.to_string_lossy()
                        ));
                        cx.notify();
                    }
                    crate::working_folder::Location::Broken(_) => {
                        self.toast_error(tr!("working_folder.broken"));
                        cx.notify();
                    }
                    _ => self.open_worktree(&path, cx),
                }
            }
        }
    }

    /// Run row `ix` of the remote-branch level: create a worktree on a local
    /// branch that tracks the remote ref, then open it.
    fn run_work_in_remote_item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.work_in_remote_rows().get(ix).cloned() else {
            return;
        };
        self.work_in_menu_open = false;
        self.work_in_remote_open = false;
        self.work_in_details_open = false;
        self.open_worktree_from_remote(row, window, cx);
    }

    /// Load remote-tracking refs for the remote level. One background fetch;
    /// a second request while it runs is dropped (the rows refresh when it
    /// lands).
    fn load_worktree_remotes(&mut self, cx: &mut Context<Self>) {
        if self.worktree_remotes_busy {
            return;
        }
        let Some(manager) = self.worktree_manager() else {
            return;
        };
        self.worktree_remotes_busy = true;
        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    let repo = manager.repository_root().ok()?;
                    Some((
                        crate::git_ops::list_refs(&repo),
                        crate::git_ops::list_remotes(&repo),
                        manager.local_branches(),
                    ))
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.worktree_remotes_busy = false;
                if let Some((refs, remotes, local_branches)) = fetched {
                    app.worktree_remotes = refs;
                    app.worktree_remote_names = remotes;
                    app.worktree_local_branches = local_branches;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Open the create dialog pointed at a remote-tracking ref: the "Existing"
    /// branch choice is the ref, and the create path turns it into a local
    /// branch that tracks it (never a detached worktree).
    pub(super) fn open_worktree_from_remote(
        &mut self,
        row: crate::working_folder::RemoteRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_create_worktree_dialog(cx);
        self.worktree_branch_mode = WorktreeBranchMode::Existing;
        self.worktree_branch_choice = Some(row.remote_ref.clone());
        // The branch list loads off-thread; put the ref in it immediately so
        // the row shows as chosen before the fetch lands.
        if !self.worktree_branches.iter().any(|b| b == &row.remote_ref) {
            self.worktree_branches.push(row.remote_ref.clone());
        }
        let suggestion = self.suggest_worktree_name_from(&row.short);
        self.worktree_name_input
            .update(cx, |input, cx| input.set_text(suggestion, cx));
        let handle = self.worktree_name_input.read(cx).focus_handle(cx);
        window.focus(&handle);
        self.set_status(tr!(
            "working_folder.tracking_hint",
            branch = row.local_branch.clone(),
            upstream = row.remote_ref.clone()
        ));
        cx.notify();
    }

    /// Open the create dialog on its "existing branch" list, which is where
    /// remote-tracking refs are listed. Choosing one there creates a local
    /// branch that tracks it (`WorktreeManager::create` routes remote-only
    /// refs through the tracking path), so this is the remote-branch entry
    /// from the new-task page without a second picker screen.
    pub(super) fn open_new_worktree_from_remote_branch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_create_worktree_dialog(cx);
        self.worktree_branch_mode = WorktreeBranchMode::Existing;
        let suggestion = self.suggest_worktree_name();
        self.worktree_name_input
            .update(cx, |input, cx| input.set_text(suggestion, cx));
        let handle = self.worktree_name_input.read(cx).focus_handle(cx);
        window.focus(&handle);
        cx.notify();
    }

    /// The quick "New worktree" flow: open the create dialog with a suggested
    /// name (fully editable — the custom-name option) and a new branch derived
    /// from it, focused so the name can be typed over immediately.
    pub(super) fn open_new_worktree_from_work_in(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_create_worktree_dialog(cx);
        let suggestion = self.suggest_worktree_name();
        self.worktree_name_input
            .update(cx, |input, cx| input.set_text(suggestion, cx));
        let handle = self.worktree_name_input.read(cx).focus_handle(cx);
        window.focus(&handle);
        cx.notify();
    }

    /// A free worktree name: the active branch slug, else `worktree`, with a
    /// numeric suffix until it is unused.
    fn suggest_worktree_name(&self) -> String {
        let base = self
            .branch
            .as_ref()
            .map(|branch| worktree::slugify(&branch.name))
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| "worktree".to_string());
        self.free_worktree_name(base)
    }

    /// A free worktree name derived from `raw` (a branch or ref's short name).
    fn suggest_worktree_name_from(&self, raw: &str) -> String {
        let base = worktree::slugify(raw);
        let base = if base.is_empty() {
            "worktree".to_string()
        } else {
            base
        };
        self.free_worktree_name(base)
    }

    /// `base`, or `base-2`, `base-3`, … until neither the list nor the folder
    /// on disk uses it.
    fn free_worktree_name(&self, base: String) -> String {
        let root = self.worktree_root_hint();
        let mut candidate = base.clone();
        let mut n = 1;
        loop {
            let listed = self
                .worktrees
                .iter()
                .any(|worktree| worktree.name == candidate);
            let on_disk = root
                .as_deref()
                .map(|root| worktree::expand_path(root, &candidate).exists())
                .unwrap_or(false);
            if !listed && !on_disk {
                return candidate;
            }
            n += 1;
            candidate = format!("{base}-{n}");
        }
    }

    /// The status bar's "Work in" chip: the working folder's label, as a
    /// control. Present in a repository in every state — at the main checkout
    /// it reads `Local` and is how a worktree is reached from the composer;
    /// inside a worktree it names that worktree. Absent outside a repository.
    pub(super) fn work_in_chip(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let label = self.work_in_label()?;
        let theme = *theme::get(cx);
        let open = self.work_in_menu_open;
        // The mark follows the place: a monitor for the main checkout, the
        // branch glyph for a worktree, a folder for anything else.
        let icon_path = match self.working_folder().map(|folder| folder.location) {
            Some(crate::working_folder::Location::Local) => "icons/monitor.svg",
            Some(crate::working_folder::Location::Worktree { .. }) => "icons/branch.svg",
            Some(crate::working_folder::Location::PlainFolder { .. }) => "icons/folder.svg",
            Some(crate::working_folder::Location::Broken(_)) => "icons/alert-circle.svg",
            None => "icons/monitor.svg",
        };
        Some(
            div()
                .flex()
                .flex_col()
                .items_start()
                .children(self.work_in_popup(cx))
                .child(
                    button_frame(div().id("status-work-in"), &theme, ButtonSize::Default)
                        .cursor_pointer()
                        .when(!open, |chip| chip.hover_quiet(&theme))
                        .when(open, |chip| {
                            chip.bg(theme.surface_active).text_color(theme.text_primary)
                        })
                        .tip(tr!("working_folder.title"))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(Self::on_work_in_trigger_click),
                        )
                        .child(icon(
                            icon_path,
                            ButtonSize::Default.icon_size().px(&theme),
                            theme.text_muted,
                        ))
                        .child(
                            div()
                                .text_color(if open {
                                    theme.text_primary
                                } else {
                                    theme.text_muted
                                })
                                .child(label),
                        )
                        .child(Self::chip_caret(
                            open,
                            theme.text_primary,
                            "work-in-caret-turn",
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    /// The "Work in" popover, while open. First level: the repository's
    /// locations and the ways to reach a new one. Remote level: the
    /// remote-tracking refs, each with the local branch it would create.
    pub(super) fn work_in_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.work_in_menu_open {
            return None;
        }
        let theme = *theme::get(cx);
        let this = cx.weak_entity();
        let rows = self.work_in_rows().unwrap_or_default();
        let selected_ix = self.work_in_selected_ix(&rows);
        // Worth saying whenever there is a conversation to leave behind, not
        // only while a turn streams: `start_task_in_folder` parks a live run
        // and tears down an idle session either way.
        let leaving = self.is_running() || !self.transcript.is_empty();

        let mut list = div()
            .id("work-in-list")
            .debug_selector(|| "work-in-list".to_string())
            .w_full()
            .max_h(px(320.))
            .overflow_y_scroll()
            .track_scroll(&self.work_in_scroll)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme));
        if self.work_in_details_open {
            list = list.child(self.work_in_back_row(&theme, this.clone()));
            if let Some(row) = self
                .work_in_remote_rows()
                .get(self.work_in_remote_highlight)
            {
                list = list.child(
                    div()
                        .debug_selector(|| "work-in-remote-details".to_string())
                        .flex_none()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .px(picker::search_padding_x(&theme))
                        .child(
                            div()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(tr!("working_folder.remote_detail_label")),
                        )
                        .child(
                            div()
                                .debug_selector(|| "work-in-remote-full-ref".to_string())
                                .w_full()
                                .text_color(theme.text_primary)
                                .child(row.remote_ref.clone()),
                        )
                        .child(
                            div()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(tr!("working_folder.local_detail_label")),
                        )
                        .child(
                            div()
                                .debug_selector(|| "work-in-remote-full-local".to_string())
                                .w_full()
                                .text_color(theme.text_primary)
                                .child(row.local_branch.clone()),
                        ),
                );
            }
        } else if self.work_in_remote_open {
            list = list.child(self.work_in_back_row(&theme, this.clone()));
            let remote_rows = self.work_in_remote_rows();
            if self.worktree_remotes_busy && remote_rows.is_empty() {
                list =
                    list.child(self.work_in_note(&theme, tr!("worktree.field.loading_branches")));
            } else if remote_rows.is_empty() {
                list = list.child(self.work_in_note(&theme, tr!("working_folder.remote_empty")));
            }
            for (ix, row) in remote_rows.iter().enumerate() {
                let highlighted = ix == self.work_in_remote_highlight;
                let entry = div()
                    .id(ElementId::NamedInteger("work-in-remote".into(), ix as u64))
                    .debug_selector(move || format!("work-in-remote-{ix}"));
                let this_row = this.clone();
                list = list.child(
                    picker_entry(entry, &theme)
                        .h(picker_row_height(&theme))
                        .flex_none()
                        .min_w_0()
                        .when(highlighted, |row| row.bg(theme.surface_selected))
                        .when(!highlighted, |row| row.hover_state(&theme))
                        .on_mouse_move({
                            let this_row = this_row.clone();
                            move |_, _, cx| {
                                this_row
                                    .update(cx, |app, cx| {
                                        if app.work_in_remote_highlight != ix {
                                            app.work_in_remote_highlight = ix;
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                            }
                        })
                        .on_mouse_up(MouseButton::Left, {
                            let this_row = this_row.clone();
                            move |_, window, cx| {
                                this_row
                                    .update(cx, |app, cx| {
                                        app.run_work_in_remote_item(ix, window, cx)
                                    })
                                    .ok();
                            }
                        })
                        .child(
                            PickerRow::new(
                                row.remote_ref.clone(),
                                if highlighted {
                                    theme.text_primary
                                } else {
                                    theme.text_secondary
                                },
                                theme.text_muted,
                            )
                            .icon(
                                "icons/branch.svg",
                                if highlighted {
                                    theme.accent_text
                                } else {
                                    theme.text_muted
                                },
                            )
                            .detail(tr!(
                                "working_folder.remote_creates",
                                branch = row.local_branch.clone()
                            ))
                            .weight(FontWeight::MEDIUM)
                            .into_element(&theme),
                        ),
                );
            }
        } else {
            let has_locations = rows.iter().any(|row| matches!(row, WorkInRow::Folder(_)));
            list = list.child(self.work_in_locations(&rows, selected_ix, &theme, this.clone()));
            if has_locations {
                list = list.child(work_in_divider(&theme));
            }
            list = list.child(self.work_in_actions(&rows, &theme, this.clone()));
            if rows.iter().any(|row| matches!(row, WorkInRow::Recent(_))) {
                list = list
                    .child(work_in_divider(&theme))
                    .child(menu_header(tr!("working_folder.recent"), &theme))
                    .child(self.work_in_recents(&rows, &theme, this.clone()));
            }
        }

        let mut popup = picker_surface(div(), &theme)
            .debug_selector(|| "work-in-popup".to_string())
            .w(px(300.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .key_context("WorkInMenu")
            .track_focus(&self.work_in_menu_focus)
            .on_action(cx.listener(Self::on_work_in_next))
            .on_action(cx.listener(Self::on_work_in_prev))
            .on_action(cx.listener(Self::on_work_in_confirm))
            .on_action(cx.listener(Self::on_work_in_close))
            .on_action(cx.listener(Self::on_work_in_details))
            .on_mouse_down_out(cx.listener(|app, _, window, cx| {
                app.menu_dismissed_at = Some(Instant::now());
                app.close_work_in_menu(window, cx);
            }))
            .child(menu_header(
                if self.work_in_remote_open {
                    tr!("working_folder.remote")
                } else {
                    tr!("working_folder.title")
                },
                &theme,
            ))
            .child(list);
        // One honest line: choosing a folder starts a task in it, and the
        // conversation on screen belongs to the folder being left.
        if leaving {
            popup = popup.child(
                div()
                    .px(picker::list_padding_y(&theme))
                    .pb(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("working_folder.new_task_note")),
            );
        }
        if self.work_in_remote_open
            && !self.work_in_details_open
            && !self.work_in_remote_rows().is_empty()
        {
            popup = popup.child(
                button_frame(
                    div().id("work-in-details-trigger"),
                    &theme,
                    ButtonSize::Medium,
                )
                .cursor_pointer()
                .hover_state(&theme)
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|app, _, _, cx| app.toggle_work_in_details(cx)),
                )
                .child(kbd_shortcut("space", &theme))
                .child(tr!("working_folder.remote_details")),
            );
        }
        popup = popup.child(working_folder_keyboard_hints(
            &theme,
            self.work_in_remote_open,
        ));
        Some(
            anchored_popover(
                Corner::BottomLeft,
                point(px(0.), -popover::MENU_OFFSET),
                popup, "anchored_popover-in-9", &theme,
            )
            .into_any_element(),
        )
    }

    /// The location rows: Local first, then each linked worktree.
    fn work_in_locations(
        &self,
        rows: &[WorkInRow],
        selected_ix: usize,
        theme: &Theme,
        this: gpui::WeakEntity<OrbitApp>,
    ) -> AnyElement {
        let mut list = div()
            .debug_selector(|| "work-in-locations".to_string())
            .w_full()
            .flex()
            .flex_col();
        for (ix, row) in rows.iter().enumerate() {
            let WorkInRow::Folder(folder) = row else {
                continue;
            };
            let highlighted = ix == self.work_in_menu_highlight;
            let selected = ix == selected_ix;
            let icon_path = match &folder.folder.location {
                crate::working_folder::Location::Local => "icons/monitor.svg",
                crate::working_folder::Location::Worktree { .. } => "icons/branch.svg",
                crate::working_folder::Location::PlainFolder { .. } => "icons/folder.svg",
                crate::working_folder::Location::Broken(_) => "icons/alert-circle.svg",
            };
            let entry = div().id(ElementId::NamedInteger("work-in-row".into(), ix as u64));
            let this_row = this.clone();
            list =
                list.child(
                    picker_entry(entry, theme)
                        .when(selected || highlighted, |row| {
                            row.bg(theme.surface_selected)
                        })
                        .when(!selected && !highlighted, |row| row.hover_state(theme))
                        // Scrolling rows under a stationary pointer must not
                        // change the keyboard highlight or trigger a rebuild.
                        .on_mouse_move({
                            let this_row = this_row.clone();
                            move |_, _, cx| {
                                this_row
                                    .update(cx, |app, cx| {
                                        if app.work_in_menu_highlight != ix {
                                            app.work_in_menu_highlight = ix;
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                            }
                        })
                        .on_mouse_up(MouseButton::Left, {
                            let this_row = this_row.clone();
                            move |_, window, cx| {
                                this_row
                                    .update(cx, |app, cx| app.run_work_in_item(ix, window, cx))
                                    .ok();
                            }
                        })
                        .child({
                            let mut row = PickerRow::new(
                                folder.label.clone(),
                                if selected || highlighted {
                                    theme.text_primary
                                } else {
                                    theme.text_secondary
                                },
                                if selected || highlighted {
                                    theme.text_secondary
                                } else {
                                    theme.text_muted
                                },
                            )
                            .icon(icon_path, theme.text_muted)
                            .weight(FontWeight::MEDIUM);
                            if let Some(detail) = folder.detail.clone() {
                                row = row.detail(detail).mono();
                            }
                            row.into_element(theme)
                        })
                        .children(
                            folder
                                .prunable
                                .then(|| work_in_tag(tr!("working_folder.prunable"), theme)),
                        )
                        .when(selected, |row| {
                            row.child(div().debug_selector(|| "work-in-check".to_string()).child(
                                icon(
                                    "icons/check.svg",
                                    context_menu::ICON.px(theme),
                                    theme.accent_text,
                                ),
                            ))
                        }),
                );
        }
        list.into_any_element()
    }

    fn work_in_recents(
        &self,
        rows: &[WorkInRow],
        theme: &Theme,
        this: gpui::WeakEntity<OrbitApp>,
    ) -> AnyElement {
        let mut list = div()
            .debug_selector(|| "work-in-recents".to_string())
            .w_full()
            .flex()
            .flex_col();
        for (ix, row) in rows.iter().enumerate() {
            let WorkInRow::Recent(path) = row else {
                continue;
            };
            let highlighted = ix == self.work_in_menu_highlight;
            let entry = div()
                .id(ElementId::NamedInteger("work-in-recent".into(), ix as u64))
                .debug_selector(move || format!("work-in-recent-{ix}"));
            let hover_this = this.clone();
            let click_this = this.clone();
            list = list.child(
                picker_entry(entry, theme)
                    .when(highlighted, |row| row.bg(theme.surface_selected))
                    .when(!highlighted, |row| row.hover_state(theme))
                    .on_mouse_move(move |_, _, cx| {
                        hover_this
                            .update(cx, |app, cx| {
                                if app.work_in_menu_highlight != ix {
                                    app.work_in_menu_highlight = ix;
                                    cx.notify();
                                }
                            })
                            .ok();
                    })
                    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        click_this
                            .update(cx, |app, cx| app.run_work_in_item(ix, window, cx))
                            .ok();
                    })
                    .child(
                        PickerRow::new(
                            sessions::workspace_label(path),
                            theme.text_secondary,
                            theme.text_muted,
                        )
                        .icon("icons/folder.svg", theme.text_muted)
                        .detail(shorten_home(path))
                        .weight(FontWeight::MEDIUM)
                        .into_element(theme),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The action rows: new worktree, an existing worktree folder, a remote
    /// branch, any folder.
    fn work_in_actions(
        &self,
        rows: &[WorkInRow],
        theme: &Theme,
        this: gpui::WeakEntity<OrbitApp>,
    ) -> AnyElement {
        let mut list = div()
            .debug_selector(|| "work-in-actions".to_string())
            .w_full()
            .flex()
            .flex_col();
        for (ix, row) in rows.iter().enumerate() {
            // A menu row is one line. The sentence-length hint lives in the
            // new-task page's picker, which has the width for it; inside a
            // 300px popover it could only ellipsize mid-sentence, and a
            // tooltip for it would be a second floating layer over a list
            // that scrolls.
            let (icon_path, title) = match row {
                WorkInRow::Folder(_) | WorkInRow::Recent(_) => continue,
                WorkInRow::NewWorktree => ("icons/plus.svg", tr!("working_folder.new")),
                WorkInRow::ExistingWorktree => ("icons/branch.svg", tr!("working_folder.existing")),
                WorkInRow::RemoteRef => ("icons/cloud.svg", tr!("working_folder.remote")),
                WorkInRow::Clone => ("icons/cloud.svg", tr!("working_folder.clone_title")),
                WorkInRow::ChooseFolder => ("icons/folder-open.svg", tr!("working_folder.choose")),
            };
            let highlighted = ix == self.work_in_menu_highlight;
            let entry = div().id(ElementId::NamedInteger("work-in-action".into(), ix as u64));
            let this_row = this.clone();
            list = list.child(
                picker_entry(entry, theme)
                    .when(highlighted, |row| row.bg(theme.surface_selected))
                    .when(!highlighted, |row| row.hover_state(theme))
                    .on_mouse_move({
                        let this_row = this_row.clone();
                        move |_, _, cx| {
                            this_row
                                .update(cx, |app, cx| {
                                    if app.work_in_menu_highlight != ix {
                                        app.work_in_menu_highlight = ix;
                                        cx.notify();
                                    }
                                })
                                .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this_row = this_row.clone();
                        move |_, window, cx| {
                            this_row
                                .update(cx, |app, cx| app.run_work_in_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .child(
                        PickerRow::new(
                            title,
                            if highlighted {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            },
                            theme.text_muted,
                        )
                        .icon(
                            icon_path,
                            if highlighted {
                                theme.accent_text
                            } else {
                                theme.text_muted
                            },
                        )
                        .weight(FontWeight::MEDIUM)
                        .into_element(theme),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The remote level's back row. Built on the same [`picker_entry`] frame as
    /// a real row, so its icon and label line up with the refs below it instead
    /// of stepping in by the row inset.
    fn work_in_back_row(&self, theme: &Theme, this: gpui::WeakEntity<OrbitApp>) -> AnyElement {
        div()
            .py(DynamicSpacing::Base02.px(theme))
            .child(
                picker_entry(div().id("work-in-back"), theme)
                    .h(picker::entry_height(theme))
                    .cursor_pointer()
                    .hover_state(theme)
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        this.update(cx, |app, cx| app.back_work_in_remote(cx)).ok();
                    })
                    .child(icon(
                        "icons/chevron-left.svg",
                        context_menu::ICON.px(theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(theme.text_secondary)
                            .child(tr!("common.back")),
                    ),
            )
            .into_any_element()
    }

    fn work_in_note(&self, theme: &Theme, text: String) -> AnyElement {
        div()
            .px(list_item::padding_x(theme))
            .py(DynamicSpacing::Base08.px(theme))
            .text_size(TextSize::XSmall.px(theme))
            .text_color(theme.text_muted)
            .child(text)
            .into_any_element()
    }
}

/// A quiet trailing tag (`prunable`).
fn work_in_tag(text: String, theme: &Theme) -> AnyElement {
    div()
        .flex_none()
        .px(DynamicSpacing::Base06.px(theme))
        .py(px(1.))
        .rounded(Radius::Small.px(theme))
        .bg(theme.warning.opacity(0.16))
        .text_size(TextSize::XSmall.px(theme))
        .text_color(theme.warning)
        .child(text)
        .into_any_element()
}

fn work_in_divider(theme: &Theme) -> AnyElement {
    div()
        .mx(list_item::inset(theme))
        .my(DynamicSpacing::Base04.px(theme))
        .h(px(1.))
        .bg(theme.border)
        .into_any_element()
}

// ── render helpers ─────────────────────────────────────────────────────

/// One row's popup menu: open / reveal / copy, rename / move, lock / unlock,
/// and remove. The main worktree hides every mutating action.
fn worktree_row_menu(
    menu: &WorktreeMenu,
    worktree: &Worktree,
    theme: Theme,
    this: Entity<OrbitApp>,
) -> AnyElement {
    let path = worktree.path.clone();
    let open_path = path.clone();
    let reveal_path = path.clone();
    let copy_path = path.clone();
    let rename_path = path.clone();
    let move_path = path.clone();
    let lock_path = path.clone();
    let remove_path = path.clone();
    let mut body = div()
        .py(picker::list_padding_y(&theme))
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base01.px(&theme))
        .child(worktree_menu_row(
            "wm-open",
            "icons/arrow-up-right.svg",
            tr!("worktree.menu.open"),
            theme,
            this.clone(),
            move |app, cx| app.open_worktree(&open_path, cx),
        ))
        .child(worktree_menu_row(
            "wm-reveal",
            "icons/folder-open.svg",
            tr!("worktree.menu.reveal"),
            theme,
            this.clone(),
            move |app, cx| app.reveal_worktree(reveal_path.clone(), cx),
        ))
        .child(worktree_menu_row(
            "wm-copy",
            "icons/copy.svg",
            tr!("sidebar.copy_path"),
            theme,
            this.clone(),
            move |app, cx| app.copy_worktree_path(copy_path.clone(), cx),
        ));
    if !worktree.is_main {
        body = body
            .child(worktree_menu_row(
                "wm-rename",
                "icons/pencil.svg",
                tr!("worktree.menu.rename"),
                theme,
                this.clone(),
                move |app, cx| app.start_rename_worktree(rename_path.clone(), cx),
            ))
            .child(worktree_menu_row(
                "wm-move",
                "icons/arrow-expand.svg",
                tr!("worktree.menu.move"),
                theme,
                this.clone(),
                move |app, cx| app.start_move_worktree(move_path.clone(), cx),
            ));
        body = if worktree.locked {
            body.child(worktree_menu_row(
                "wm-unlock",
                "icons/lock.svg",
                tr!("worktree.menu.unlock"),
                theme,
                this.clone(),
                move |app, cx| app.unlock_worktree(lock_path.clone(), cx),
            ))
        } else {
            body.child(worktree_menu_row(
                "wm-lock",
                "icons/lock.svg",
                tr!("worktree.menu.lock"),
                theme,
                this.clone(),
                move |app, cx| app.lock_worktree(lock_path.clone(), cx),
            ))
        };
        body = body.child(worktree_menu_row(
            "wm-remove",
            "icons/trash.svg",
            tr!("worktree.menu.remove"),
            theme,
            this.clone(),
            move |app, cx| app.start_remove_worktree(remove_path.clone(), cx),
        ));
    }
    let popup = picker_surface(div(), &theme)
        .w(px(220.))
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.menu_dismissed_at = Some(Instant::now());
                    app.worktree_menu = None;
                    cx.notify();
                })
            }
        })
        .child(body);
    let anchor = if let Some(at) = menu.at {
        anchored().anchor(Corner::TopLeft).position(at)
    } else {
        anchored()
            .position_mode(AnchoredPositionMode::Local)
            .anchor(Corner::TopRight)
            .offset(point(
                px(0.),
                ButtonSize::Compact.height(&theme) + popover::MENU_OFFSET,
            ))
    };
    anchor
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(deferred(popup))
        .into_any_element()
}

/// The worktree state marker: locked / prunable / main / linked.
fn worktree_markers(worktree: &Worktree, theme: Theme) -> AnyElement {
    let mut row = div()
        .w(px(20.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center();
    if worktree.locked {
        row = row.child(icon(
            "icons/lock.svg",
            IconSize::Small.px(&theme),
            theme.text_secondary,
        ));
    } else if worktree.prunable {
        row = row.child(icon(
            "icons/circle-x.svg",
            IconSize::Small.px(&theme),
            theme.error,
        ));
    } else if worktree.is_main {
        row = row.child(div().size(px(8.)).rounded_full().bg(theme.accent));
    } else {
        row = row.child(
            div()
                .size(px(8.))
                .rounded_full()
                .border_1()
                .border_color(theme.text_muted),
        );
    }
    row.into_any_element()
}

/// One bullet in the "How worktrees work" card.
fn worktree_howto_line(theme: Theme, text: String) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(div().flex_none().text_color(theme.text_muted).child("•"))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_secondary)
                .child(text),
        )
        .into_any_element()
}

/// Empty state for the worktrees list.
fn worktree_empty_state(theme: Theme, busy: bool, has_no_error: bool) -> AnyElement {
    div()
        .w_full()
        .py(DynamicSpacing::Base24.px(&theme))
        .flex()
        .flex_col()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .child(if busy {
            spinner(
                "worktrees-empty-spin",
                IconSize::Medium.px(&theme),
                theme.text_muted,
                theme,
            )
        } else {
            icon(
                "icons/branch.svg",
                IconSize::Medium.px(&theme),
                theme.text_muted,
            )
            .into_any_element()
        })
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(if busy {
                    tr!("worktree.empty.loading")
                } else if has_no_error {
                    tr!("worktree.empty.title")
                } else {
                    tr!("worktree.empty.unavailable")
                }),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("worktree.empty.hint")),
        )
        .into_any_element()
}

/// A labelled field in a worktree dialog.
fn worktree_field(
    theme: &Theme,
    label: String,
    hint: Option<&str>,
    control: impl IntoElement,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(input::gap(theme))
        .child(worktree_field_label(theme, label, hint))
        .child(control)
        .into_any_element()
}

fn worktree_field_label(theme: &Theme, label: String, hint: Option<&str>) -> AnyElement {
    let mut column = div()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base02.px(theme))
        .child(
            input_label(label, theme)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary),
        );
    if let Some(hint) = hint {
        column = column.child(
            div()
                .text_size(TextSize::XSmall.px(theme))
                .text_color(theme.text_muted)
                .child(hint.to_string()),
        );
    }
    column.into_any_element()
}

/// A dialog footer: Cancel on the left, the primary action on the right.
fn worktree_dialog_footer<C, P>(
    theme: Theme,
    cancel_label: String,
    confirm_label: String,
    this: Entity<OrbitApp>,
    on_cancel: C,
    on_confirm: P,
) -> AnyElement
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    P: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    div()
        .flex()
        .items_center()
        .justify_end()
        .gap(modal::footer_gap(&theme))
        .border_t_1()
        .border_color(theme.border)
        .pt(modal::footer_padding(&theme))
        .child(worktree_text_button(
            "worktree-dialog-cancel",
            cancel_label,
            theme,
            this.clone(),
            on_cancel,
        ))
        .child(worktree_primary_button(
            "worktree-dialog-confirm",
            confirm_label,
            theme,
            this,
            on_confirm,
        ))
        .into_any_element()
}

fn clone_dialog_button<F>(
    id: &'static str,
    label: String,
    theme: Theme,
    focus: FocusHandle,
    primary: bool,
    this: Entity<OrbitApp>,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Window, &mut Context<OrbitApp>) + 'static,
{
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .key_context("CloneDialog CloneDialogButton")
        .track_focus(&focus)
        .border_1()
        .when(primary, |button| {
            button
                .border_color(theme.accent.opacity(0.5))
                .raised(theme.accent.opacity(0.14), &theme)
                .hover_accent_wash(&theme, AccentWash::Faint)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.accent_text)
        })
        .when(!primary, |button| {
            button
                .border_color(theme.border)
                .raised(theme.surface_elevated, &theme)
                .hover_state(&theme)
                .text_color(theme.text_secondary)
        })
        .focus_state(&theme)
        .cursor_pointer()
        .child(label)
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.focus(&focus);
            this.update(cx, |app, cx| action(app, window, cx));
        })
        .into_any_element()
}

/// A quiet button in a worktree surface.
fn worktree_text_button<F>(
    id: &'static str,
    label: String,
    theme: Theme,
    this: Entity<OrbitApp>,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .border_1()
        .border_color(theme.border)
        .raised(theme.surface_elevated, &theme)
        .cursor_pointer()
        .hover_state(&theme)
        .text_color(theme.text_secondary)
        .child(label)
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| action(app, cx));
        })
        .into_any_element()
}

/// A quiet icon button (page refresh).
fn worktree_icon_button<F>(
    id: &'static str,
    icon_path: &'static str,
    theme: Theme,
    this: Entity<OrbitApp>,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .border_1()
        .border_color(theme.border)
        .raised(theme.surface_elevated, &theme)
        .cursor_pointer()
        .hover_state(&theme)
        .tip(tr!("common.refresh"))
        .child(icon(
            icon_path,
            ButtonSize::Medium.icon_size().px(&theme),
            theme.text_secondary,
        ))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| action(app, cx));
        })
        .into_any_element()
}

/// The dialog's primary action.
fn worktree_primary_button<F>(
    id: &'static str,
    label: String,
    theme: Theme,
    this: Entity<OrbitApp>,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .border_1()
        .border_color(theme.accent.opacity(0.5))
        .raised(theme.accent.opacity(0.14), &theme)
        .cursor_pointer()
        .hover_accent_wash(&theme, AccentWash::Faint)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.accent_text)
        .child(label)
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| action(app, cx));
        })
        .into_any_element()
}

/// One row of the Advanced popover.
fn worktree_menu_row<F>(
    id: &'static str,
    icon_path: &'static str,
    label: String,
    theme: Theme,
    this: Entity<OrbitApp>,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    picker_entry(div().id(id), &theme)
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_pointer()
        .hover_state(&theme)
        .child(icon(
            icon_path,
            context_menu::ICON.px(&theme),
            theme.text_muted,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.text_secondary)
                .child(label),
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| action(app, cx));
        })
        .into_any_element()
}

/// A mode chip in the create dialog (New branch / Existing branch).
fn worktree_mode_chip<F>(
    id: &'static str,
    label: String,
    selected: bool,
    theme: Theme,
    this: Entity<OrbitApp>,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    button_frame(div().id(id), &theme, ButtonSize::Default)
        .border_1()
        .border_color(if selected {
            theme.accent.opacity(0.5)
        } else {
            theme.border
        })
        .raised(
            if selected {
                theme.accent.opacity(0.14)
            } else {
                theme.surface_elevated
            },
            &theme,
        )
        .cursor_pointer()
        .hover_state(&theme)
        .font_weight(FontWeight::MEDIUM)
        .text_color(if selected {
            theme.accent_text
        } else {
            theme.text_secondary
        })
        .child(label)
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| action(app, cx));
        })
        .into_any_element()
}

/// `/Users/user/...` reads `~/...` on every surface that shows a path.
fn shorten_home(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let home = crate::platform::home_dir();
    let home = home.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(home.as_ref()) {
        if rest.is_empty() {
            return "~".to_string();
        }
        if let Some(rest) = rest.strip_prefix(std::path::MAIN_SEPARATOR) {
            return format!("~/{rest}");
        }
    }
    raw.into_owned()
}
