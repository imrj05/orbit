//! Rewind — fork the conversation before an earlier user message.
//!
//! pi's `fork` writes the branch up to the chosen entry into a **new** session
//! file and moves the process onto it, so a rewind is a session replacement,
//! not a truncation: the response handler resets exactly what `new_session`
//! resets, then reloads the transcript from the branch pi kept.
//!
//! Two ways in, one behaviour: the transcript's per-message action and the
//! palette's *Rewind to a message…* both open [`RewindPicker`], and the picker
//! is the only thing that sends `fork`. A transcript row that cannot be matched
//! to a pi entry just opens the list, which is honest about what is on offer
//! instead of forking at the wrong message.

use super::*;
use crate::rewind::{self, ForkPoint};
use crate::rewind_picker::RewindPicker;

impl OrbitApp {
    /// Do not replace the process or submit into it while pi is forking.
    /// The fork reply determines which session owns the process.
    pub(super) fn rewind_transition_pending(&mut self, cx: &mut Context<Self>) -> bool {
        if self.rewind_fork_id.is_some() {
            self.toast_warning(tr!("rewind.in_progress"));
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Palette path: open the list at the newest user message.
    pub(super) fn open_rewind_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_rewind_picker_at(None, window, cx);
    }

    /// Transcript path: `turn` is the displayed message row, not its ordinal
    /// among user messages.
    pub(super) fn rewind_to_turn(
        &mut self,
        turn: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ordinal) = self.transcript.user_turn_ordinal(turn) else {
            return;
        };
        self.open_rewind_picker_at(Some(ordinal), window, cx);
    }

    fn open_rewind_picker_at(
        &mut self,
        turn: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rewind_transition_pending(cx) {
            return;
        }
        // The transcript footer is a pointer path: a click that dismissed the
        // picker (mouse-down on the scrim) must not re-open it through the
        // button that happens to sit under the pointer (mouse-up). The palette
        // path is keyboard and needs no guard.
        if turn.is_some() {
            const GESTURE: Duration = Duration::from_millis(200);
            if let Some(dismissed) = self.menu_dismissed_at.take() {
                if dismissed.elapsed() < GESTURE {
                    return;
                }
            }
        }
        // A fork mid-run would leave a live run writing into a session pi has
        // just replaced, so ask for the stop instead of guessing at it.
        if self.is_running() {
            self.toast_warning(tr!("rewind.stop_first"));
            cx.notify();
            return;
        }
        if self.transcript.user_turn_texts().is_empty() {
            self.toast_info(tr!("rewind.nothing_to_rewind"));
            cx.notify();
            return;
        }
        // Every other floating surface yields to the picker, so two lists can
        // never take the same arrow keys.
        self.command_palette = None;
        self.model_selector = None;
        self.branch_picker = None;
        self.workspace_picker = None;
        self.session_menu = None;
        self.workspace_menu = None;
        self.dismiss_work_in_menu();

        // Always ask pi for the list: it is one cheap command, and a cached
        // copy would be wrong the moment a prompt is sent or a run compacts.
        self.rewind_preselect = turn;
        self.rewind_fetching = true;
        let Some(request_id) = self.send_tracked(CommandBody::GetForkMessages, "get_fork_messages")
        else {
            // Without a live pi there is no list to show; `send` already said
            // so, and a picker stuck on its loading row would not.
            self.rewind_fetching = false;
            self.rewind_preselect = None;
            return;
        };
        self.rewind_fetch_id = Some(request_id);

        let this = cx.weak_entity();
        let on_pick = Box::new(move |entry: String, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| app.request_rewind(entry, cx))
                .ok();
        }) as Box<dyn Fn(String, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_dismiss = Box::new(move |by_mouse: bool, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                app.close_rewind_picker(by_mouse, window, cx);
            })
            .ok();
        }) as Box<dyn Fn(bool, &mut Window, &mut App)>;

        let picker = cx.new(|cx| RewindPicker::new(on_pick, on_dismiss, cx));
        window.focus(&picker.read(cx).focus_handle(cx));
        self.rewind_picker = Some(picker);
        cx.notify();
    }

    /// The entry id the `turn`-th transcript message forks before, if pi's list
    /// names it.
    fn preselect_for(&self, turn: Option<usize>, points: &[ForkPoint]) -> Option<String> {
        let turn = turn?;
        let texts = self.transcript.user_turn_texts();
        rewind::point_for_turn(points, &texts, turn).map(|point| point.entry_id.clone())
    }

    pub(super) fn close_rewind_picker(
        &mut self,
        by_mouse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rewind_picker.take().is_some() {
            if by_mouse {
                // The same click's mouse-up must not re-open it through a
                // footer button that happens to sit under the pointer.
                self.menu_dismissed_at = Some(Instant::now());
            }
            self.rewind_preselect = None;
            self.rewind_fetch_id = None;
            self.rewind_fetching = false;
            self.input.read(cx).focus(window);
            cx.notify();
        }
    }

    /// The transcript's rewind action, handed down so a message footer can
    /// open the picker without owning session state.
    pub(super) fn rewind_opener(&self, cx: &Context<Self>) -> crate::transcript_view::RewindOpener {
        let this = cx.weak_entity();
        Rc::new(move |row, window, cx| {
            this.update(cx, |app, cx| {
                if app.transcript.user_turn_ordinal(row).is_some() {
                    app.rewind_to_turn(row, window, cx);
                } else {
                    // pi's RPC forks before user messages, not assistant
                    // responses. Let the user choose the fork point instead
                    // of pretending the response itself is one.
                    app.open_rewind_picker(window, cx);
                }
            })
            .ok();
        })
    }

    /// Apply a `get_fork_messages` reply: cache the list and refill the open
    /// picker in place.
    pub(super) fn on_fork_messages_response(
        &mut self,
        data: Option<&serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        self.rewind_fetching = false;
        let points = data.map(rewind::fork_points).unwrap_or_default();
        let from_message = self.rewind_preselect.is_some();
        let preselect = self.preselect_for(self.rewind_preselect, &points);
        self.rewind_preselect = None;
        if let Some(picker) = self.rewind_picker.clone() {
            picker.update(cx, |picker, cx| {
                let ambiguous = from_message && preselect.is_none();
                picker.set_points(points, preselect, cx);
                if ambiguous {
                    // Never silently pick the newest prompt for a message
                    // whose entry could not be identified.
                    picker.clear_highlight(cx);
                }
            });
        }
        cx.notify();
    }

    /// Send the fork the picker asked for. The picker closes first: the reply
    /// replaces the session under it.
    pub(super) fn request_rewind(&mut self, entry_id: String, cx: &mut Context<Self>) {
        if self.rewind_transition_pending(cx) {
            return;
        }
        self.rewind_picker = None;
        self.rewind_preselect = None;
        self.rewind_fork_id = self.send_tracked(CommandBody::Fork { entry_id }, "fork");
        cx.notify();
    }

    /// Apply a `fork` reply.
    ///
    /// A cancelled fork (an extension's `session_before_fork` handler said no)
    /// changes nothing. A successful one is a session replacement: pi has
    /// already moved the process onto the branched file, so everything the
    /// previous session owned is dropped here and re-read from pi.
    pub(super) fn on_fork_response(
        &mut self,
        data: Option<&serde_json::Value>,
        refresh_sessions: &mut bool,
        cx: &mut Context<Self>,
    ) {
        let cancelled = data
            .and_then(|data| data.get("cancelled"))
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        if cancelled {
            self.toast_info(tr!("rewind.cancelled"));
            cx.notify();
            return;
        }
        let text = data
            .and_then(|data| data.get("text"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Collect clipboard items that have not reached the attachment tray
        // yet before separating the old session from the fork.
        self.drain_pasted_files(cx);
        self.drain_pasted_images(cx);
        self.drain_pasted_texts(cx);
        let can_prefill = if self.current_session_path.is_some() {
            // Drafts belong to the source session, not the new branch. Keep
            // all attachment types with it, then start the fork's composer
            // clean. Never use the New Task slot for a known session.
            self.save_composer_draft(cx);
            self.input.update(cx, |input, cx| input.clear(cx));
            true
        } else {
            // pi hasn't named the source yet: the New Task slot may already
            // contain a different draft. Keep this draft visible rather than
            // overwrite either one or silently attach its files to the prompt.
            self.input.read(cx).text().trim().is_empty()
                && self.attachments.is_empty()
                && self.files.is_empty()
                && self.pasted_texts.is_empty()
        };

        self.execution = SessionExecution::default();
        self.transcript.clear();
        self.current_title = None;
        self.reset_session_name(cx);
        self.current_session_path = None;
        self.added = 0;
        self.removed = 0;
        self.context = None;
        self.session_usage = None;
        // Widgets and dialogs belonged to the process context pi replaced.
        self.extension_widgets.clear();
        self.reset_turns();
        self.reset_queue();
        self.session_id = None;
        self.reset_quota_entries();
        // The fork inherits its model from the entries it branched from; it is
        // not a fresh session to put on the configured default.
        self.default_model_armed = false;
        self.sync_review_context(cx);
        *refresh_sessions = true;
        // pi hands the forked-from prompt back so it can be sent again with an
        // edit — the whole point of rewinding rather than retyping.
        if can_prefill && !text.trim().is_empty() {
            self.input.update(cx, |input, cx| input.set_text(text, cx));
        }
        self.toast_success(tr!("rewind.done"));
        self.send(CommandBody::GetMessages, "get_messages");
        self.send(CommandBody::GetState, "get_state");
        self.refresh_context_stats();
        cx.notify();
    }
}
