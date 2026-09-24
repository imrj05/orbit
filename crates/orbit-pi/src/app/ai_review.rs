//! AI reviewer lifecycle — a dedicated, read-only pi process whose findings
//! render in the Review pane.
//!
//! The reviewer is **not** a second turn in the user's session: it runs on its
//! own pi process scoped to Ask mode (read-only) and its events are drained
//! here, mirroring the parked-session pattern. Its final answer is read from a
//! private [`Transcript`] and parsed by [`crate::ai_review::parse_report`].
//!
//! The process is launched with `ORBIT_WORKFLOW_MODE=ask` and `ORBIT_REVIEW=1`
//! (see [`BundledExtensions::spawn_reviewer`]), so it is read-only from the
//! first hook and the access guard never raises a dialog nobody is routing.

use super::*;
use crate::git;
use crate::ai_review::{self as model, ReviewKind, ReviewStatus};

/// The dedicated reviewer process. Kept out of `lives`: it is not a user
/// session and must never appear in the sidebar or its notifications.
pub(super) struct ReviewAgent {
    client: PiClient,
    /// Accumulates the reviewer's streamed answer; the final text is read with
    /// `last_response_text` on settle.
    transcript: Transcript,
}

impl OrbitApp {
    /// Start (or restart) a review. For the changes scope the diff is collected
    /// off-thread first, so the UI never blocks on git.
    pub(super) fn start_ai_review(&mut self, kind: ReviewKind, cx: &mut Context<Self>) {
        // Drop any in-flight reviewer; its process dies with the client.
        self.ai_review = None;
        self.ai_review_generation = self.ai_review_generation.wrapping_add(1);
        let generation = self.ai_review_generation;
        // Set the kind first so an early failure still renders a titled panel.
        self.ai_review_kind = Some(kind);
        let Some(cwd) = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        else {
            self.fail_ai_review(tr!("ai_review.no_workspace"));
            cx.notify();
            return;
        };
        self.ai_review_status = ReviewStatus::Running;
        self.ai_report = None;
        cx.notify();

        if kind == ReviewKind::Changes {
            let (source, _) = self.sidepane.read(cx).ai_review_source();
            let session = self.session_id.clone();
            let workspace = cwd.clone();
            cx.spawn(async move |this, cx| {
                let collected = cx
                    .background_executor()
                    .spawn(async move {
                        git::collect_review_diff(&workspace, source, session.as_deref())
                    })
                    .await;
                let _ = this.update(cx, |app, cx| {
                    if app.ai_review_generation != generation {
                        return;
                    }
                    app.launch_review_agent(cwd, kind, Some(collected), cx);
                });
            })
            .detach();
        } else {
            self.launch_review_agent(cwd, kind, None, cx);
        }
    }

    /// Spawn the dedicated process and send `new_session` + the review prompt.
    /// The prompt does not wait for the session id: `ORBIT_WORKFLOW_MODE=ask`
    /// already scopes the process read-only, and pi preserves command order.
    fn launch_review_agent(
        &mut self,
        cwd: PathBuf,
        kind: ReviewKind,
        collected: Option<Result<git::ReviewDiff, String>>,
        cx: &mut Context<Self>,
    ) {
        let diff = match kind {
            ReviewKind::Changes => match collected {
                Some(Ok(diff)) => Some(diff),
                Some(Err(error)) => {
                    self.fail_ai_review(error);
                    cx.notify();
                    return;
                }
                None => {
                    self.fail_ai_review(tr!("ai_review.no_diff"));
                    cx.notify();
                    return;
                }
            },
            ReviewKind::Project => None,
        };
        let client = match self.extensions.spawn_reviewer(&cwd) {
            Ok(client) => client,
            Err(error) => {
                self.fail_ai_review(tr!("ai_review.spawn_failed", error = error.to_string()));
                cx.notify();
                return;
            }
        };
        let prompt = match kind {
            ReviewKind::Changes => {
                let label = self.sidepane.read(cx).ai_review_source_label();
                let patch = diff.map(|diff| diff.patch).unwrap_or_default();
                let (patch, truncated) = model::cap_patch(&patch);
                model::build_changes_prompt(&label, &patch, truncated)
            }
            ReviewKind::Project => model::build_project_prompt(),
        };
        let _ = client.send(CommandBody::NewSession);
        let _ = client.send(CommandBody::Prompt {
            message: prompt,
            images: None,
            streaming_behavior: None,
        });
        self.ai_review = Some(ReviewAgent {
            client,
            transcript: Transcript::new(),
        });
        self.ai_review_status = ReviewStatus::Running;
        cx.notify();
    }

    /// Drain the reviewer's events each heartbeat. Settle reads the final
    /// answer and parses it into the pane's report.
    pub(super) fn tick_ai_review(&mut self, cx: &mut Context<Self>) {
        let Some(agent) = self.ai_review.as_mut() else {
            return;
        };
        let events = agent.client.drain_events();
        if events.is_empty() {
            return;
        }
        let mut settled = false;
        let mut failure: Option<String> = None;
        for event in &events {
            match event {
                Event::AgentSettled => settled = true,
                Event::ProcessExited => failure = Some(tr!("ai_review.process_exited")),
                // The reviewer has no UI surface: never leave it blocked on a
                // dialog. Cancel so the run can settle (or fail fast).
                Event::ExtensionUiRequest { id, .. } => {
                    let _ = agent.client.respond_dialog(
                        id.as_str(),
                        serde_json::json!({
                            "type": "extension_ui_response",
                            "id": id,
                            "cancelled": true
                        }),
                    );
                }
                // Persist the read-only scope against the real session id so a
                // resume stays read-only even without the env var. The prompt is
                // already scoped by `ORBIT_WORKFLOW_MODE`.
                Event::Response { command, data, .. } if command.as_str() == "new_session" => {
                    if let Some(id) = data
                        .as_ref()
                        .and_then(|data| data.get("sessionId"))
                        .and_then(Value::as_str)
                    {
                        crate::workflow::persist_for(id, WorkflowMode::Ask);
                    }
                }
                Event::MessageEnd { value } => {
                    if let Some(error) = transcript::message_error(value) {
                        failure = Some(error);
                    }
                }
                _ => {}
            }
            agent.transcript.apply_event(event);
        }
        if let Some(error) = failure {
            self.fail_ai_review(error);
            cx.notify();
            return;
        }
        if settled {
            let report = self
                .ai_review
                .as_ref()
                .and_then(|agent| agent.transcript.last_response_text())
                .map(|(_, text)| model::parse_report(&text))
                .unwrap_or_default();
            self.ai_review = None;
            if report.is_empty() {
                self.ai_review_status = ReviewStatus::Failed(tr!("ai_review.empty_answer"));
                self.ai_report = None;
            } else {
                self.ai_review_status = ReviewStatus::Done;
                self.ai_report = Some(report);
            }
            cx.notify();
        }
    }

    /// Stop the reviewer (the pane's Stop button). The process is dropped.
    pub(super) fn cancel_ai_review(&mut self, cx: &mut Context<Self>) {
        self.ai_review_generation = self.ai_review_generation.wrapping_add(1);
        if let Some(agent) = self.ai_review.take() {
            let _ = agent.client.send(CommandBody::Abort);
        }
        self.ai_review_kind = None;
        self.ai_review_status = ReviewStatus::Idle;
        self.ai_report = None;
        cx.notify();
    }

    /// Forget the reviewer and its result without notifying. Called when the
    /// workspace or session changes: the old findings no longer apply.
    pub(super) fn discard_ai_review(&mut self) {
        self.ai_review_generation = self.ai_review_generation.wrapping_add(1);
        self.ai_review = None;
        self.ai_review_kind = None;
        self.ai_review_status = ReviewStatus::Idle;
        self.ai_report = None;
    }

    fn fail_ai_review(&mut self, error: String) {
        self.ai_review = None;
        // Keep the kind so the pane still renders the error state (the panel
        // is keyed on the run's kind).
        self.ai_review_status = ReviewStatus::Failed(error);
        self.ai_report = None;
    }

    /// The snapshot the Review pane renders (synced each frame).
    pub(super) fn ai_review_snapshot(&self) -> crate::sidepane::AiReviewSnapshot {
        crate::sidepane::AiReviewSnapshot {
            kind: self.ai_review_kind,
            status: self.ai_review_status.clone(),
            report: self.ai_report.clone(),
        }
    }

    /// The pane's opener callback: starts a review on behalf of the pane's
    /// sparkles menu without the pane needing to reach back into the app.
    pub(super) fn ai_review_opener(&self, cx: &Context<Self>) -> crate::sidepane::AiReviewAction {
        let this = cx.weak_entity();
        Rc::new(move |request, _window, cx| {
            this.update(cx, |app, cx| match request {
                crate::sidepane::AiReviewRequest::Start(kind) => app.start_ai_review(kind, cx),
                crate::sidepane::AiReviewRequest::Cancel => app.cancel_ai_review(cx),
            })
            .ok();
        })
    }
}
