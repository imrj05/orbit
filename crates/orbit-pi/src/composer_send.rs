//! Composer delivery policy shared by settings, shortcuts, hints, and RPC sends.

use orbit_rpc::CommandBody;
use serde_json::Value;

use crate::platform::shortcuts as keys;

/// How a message is delivered during a run. Idle sends always start a prompt.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    #[default]
    FollowUp,
    Steer,
}

impl SendMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FollowUp => "follow_up",
            Self::Steer => "steer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "follow_up" => Some(Self::FollowUp),
            "steer" => Some(Self::Steer),
            _ => None,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::FollowUp => Self::Steer,
            Self::Steer => Self::FollowUp,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::FollowUp => tr!("composer.queue_follow_up"),
            Self::Steer => tr!("composer.steer_task"),
        }
    }

    fn hint_label(self) -> String {
        match self {
            Self::FollowUp => tr!("composer.queue"),
            Self::Steer => tr!("composer.steer"),
        }
    }

    /// The default/alternate shortcut that currently invokes this mode.
    pub fn shortcut(self, default: Self) -> &'static str {
        if self == default {
            keys::SEND
        } else {
            keys::SEND_ALTERNATE
        }
    }

    /// [`Self::shortcut`] rendered for display (`↵` / `⌥↵` on macOS).
    pub fn shortcut_label(self, default: Self) -> String {
        keys::label(self.shortcut(default))
    }

    pub fn command(
        self,
        running: bool,
        message: String,
        images: Option<Vec<Value>>,
    ) -> (CommandBody, &'static str) {
        if !running {
            return (
                CommandBody::Prompt {
                    message,
                    images,
                    streaming_behavior: None,
                },
                "prompt",
            );
        }
        let body = match self {
            Self::FollowUp => CommandBody::FollowUp { message, images },
            Self::Steer => CommandBody::Steer { message, images },
        };
        (body, self.as_str())
    }
}

/// Resolve labels at paint time so language and preference changes are live.
pub fn shortcut_hints(default: SendMode, running: bool) -> Vec<(String, String)> {
    if running {
        [default, default.opposite()]
            .into_iter()
            .map(|mode| (mode.shortcut_label(default), mode.hint_label()))
            .collect()
    } else {
        vec![(keys::label(keys::SEND), tr!("composer.send"))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_preserves_follow_up_behavior() {
        assert_eq!(SendMode::default(), SendMode::FollowUp);
    }

    #[test]
    fn alternate_reverses_both_defaults() {
        assert_eq!(SendMode::FollowUp.opposite(), SendMode::Steer);
        assert_eq!(SendMode::Steer.opposite(), SendMode::FollowUp);
    }

    #[test]
    fn idle_sends_are_prompts_for_both_modes() {
        for mode in [SendMode::FollowUp, SendMode::Steer] {
            let (body, label) = mode.command(false, "hello".into(), None);
            assert_eq!(label, "prompt");
            assert!(matches!(
                body,
                CommandBody::Prompt {
                    streaming_behavior: None,
                    ..
                }
            ));
        }
    }

    #[test]
    fn running_sends_use_the_selected_rpc_command_with_images() {
        let images = Some(vec![
            json!({"type": "image", "data": "abc", "mimeType": "image/png"}),
        ]);
        for mode in [SendMode::FollowUp, SendMode::Steer] {
            let (body, label) = mode.command(true, "hello".into(), images.clone());
            let value = serde_json::to_value(body).unwrap();
            assert_eq!(value["type"], mode.as_str());
            assert_eq!(value["message"], "hello");
            assert_eq!(value["images"], json!(images));
            assert_eq!(label, mode.as_str());
        }
    }

    #[test]
    fn running_hints_show_only_short_default_and_alternate_labels() {
        for (default, primary, alternate) in [
            (
                SendMode::FollowUp,
                tr!("composer.queue"),
                tr!("composer.steer"),
            ),
            (
                SendMode::Steer,
                tr!("composer.steer"),
                tr!("composer.queue"),
            ),
        ] {
            assert_eq!(
                shortcut_hints(default, true),
                vec![
                    (keys::label(keys::SEND), primary),
                    (keys::label(keys::SEND_ALTERNATE), alternate)
                ]
            );
        }
    }

    fn bound_action(cx: &gpui::App, key: &str, context: &str) -> Option<Box<dyn gpui::Action>> {
        let (bindings, _) = cx.key_bindings().borrow().bindings_for_input(
            &[gpui::Keystroke::parse(key).unwrap()],
            &[gpui::KeyContext::parse(context).unwrap()],
        );
        bindings
            .first()
            .map(|binding| binding.action().boxed_clone())
    }

    #[gpui::test]
    fn tab_traversal_is_guarded_in_text_and_terminal_contexts(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::bind_keys(cx);
            // Global traversal on every surface that does not own Tab.
            assert!(bound_action(cx, "tab", "Sidebar")
                .unwrap()
                .as_any()
                .is::<crate::FocusNext>());
            assert!(bound_action(cx, "shift-tab", "Sidebar")
                .unwrap()
                .as_any()
                .is::<crate::FocusPrev>());
            // The composer accepts completions with Tab…
            assert!(bound_action(cx, "tab", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::AutocompleteAccept>());
            // …and the terminal keeps Tab for the shell.
            assert!(bound_action(cx, "tab", "Terminal")
                .unwrap()
                .as_any()
                .is::<crate::TerminalTab>());
            assert!(bound_action(cx, "shift-tab", "Terminal")
                .unwrap()
                .as_any()
                .is::<crate::TerminalTab>());
            // Session cycling also yields to the shell's Ctrl+Tab.
            assert!(bound_action(cx, "ctrl-tab", "Terminal")
                .unwrap()
                .as_any()
                .is::<crate::TerminalTab>());
        });
    }

    #[gpui::test]
    fn registry_shortcuts_resolve_on_both_platform_modifiers(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::bind_keys(cx);
            // `secondary` parses as Cmd here and Ctrl elsewhere, so the same
            // assertion covers both platforms.
            assert!(bound_action(cx, "secondary-shift-m", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::ToggleModelMenu>());
            assert!(bound_action(cx, "secondary-shift-t", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::ToggleThinkingMenu>());
            assert!(bound_action(cx, "secondary-shift-r", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::ReviewChanges>());
            assert!(bound_action(cx, "secondary-/", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::OpenShortcutHelp>());
        });
    }

    #[gpui::test]
    fn session_shortcuts_resolve_to_their_slots_and_cycles(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::bind_keys(cx);
            // ⌘1…⌘9 carry the slot in the action payload.
            let action = bound_action(cx, "secondary-3", "Composer").unwrap();
            let slot = action
                .as_any()
                .downcast_ref::<crate::OpenSessionSlot>()
                .expect("⌘3 is a session slot");
            assert_eq!(slot.slot, 3);
            assert_eq!(
                bound_action(cx, "secondary-9", "Composer")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<crate::OpenSessionSlot>()
                    .unwrap()
                    .slot,
                9
            );
            // Ctrl+Tab / Ctrl+Shift+Tab cycle sessions on every platform.
            assert!(bound_action(cx, "ctrl-tab", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::NextSession>());
            assert!(bound_action(cx, "ctrl-shift-tab", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::PrevSession>());
            // Git tabs moved off ⌘1–5 so numbering belongs to sessions.
            assert!(bound_action(cx, "secondary-alt-1", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::GitTabChanges>());
            assert!(!bound_action(cx, "secondary-1", "Composer")
                .unwrap()
                .as_any()
                .is::<crate::GitTabChanges>());
        });
    }

    #[gpui::test]
    fn chat_shortcuts_keep_default_alternate_and_explicit_steer_distinct(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::bind_keys(cx);
            let context = "Composer ChatComposer";
            assert!(bound_action(cx, "enter", context)
                .unwrap()
                .as_any()
                .is::<crate::Submit>());
            assert!(bound_action(cx, "alt-enter", context)
                .unwrap()
                .as_any()
                .is::<crate::SendAlternate>());
            let steer = if cfg!(target_os = "macos") {
                "cmd-shift-enter"
            } else {
                "ctrl-shift-enter"
            };
            assert!(bound_action(cx, steer, context)
                .unwrap()
                .as_any()
                .is::<crate::SteerRun>());
            assert!(bound_action(cx, "shift-enter", context)
                .unwrap()
                .as_any()
                .is::<crate::Newline>());
        });
    }

    #[gpui::test]
    fn turn_navigation_shortcuts_stay_live_from_the_composer(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::bind_keys(cx);
            let (prev, next) = if cfg!(target_os = "macos") {
                ("cmd-up", "cmd-down")
            } else {
                ("ctrl-up", "ctrl-down")
            };
            // Global bindings (no key context): they must resolve while the
            // composer owns focus, which is where the user presses them.
            assert!(bound_action(cx, prev, "Composer")
                .unwrap()
                .as_any()
                .is::<crate::PrevTurn>());
            assert!(bound_action(cx, next, "Composer")
                .unwrap()
                .as_any()
                .is::<crate::NextTurn>());
        });
    }

    #[gpui::test]
    fn other_inputs_keep_their_enter_actions_and_cannot_use_chat_sends(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::bind_keys(cx);
            for context in [
                "Composer",
                "Composer Picker",
                "Composer AskInput",
                "Composer DialogInput",
                "Editor",
            ] {
                assert!(
                    bound_action(cx, "alt-enter", context).is_none(),
                    "{context}"
                );
                let steer = if cfg!(target_os = "macos") {
                    "cmd-shift-enter"
                } else {
                    "ctrl-shift-enter"
                };
                assert!(bound_action(cx, steer, context).is_none(), "{context}");
            }
            assert!(bound_action(cx, "enter", "Composer Picker")
                .unwrap()
                .as_any()
                .is::<crate::PickerConfirm>());
            assert!(bound_action(cx, "enter", "Composer AskInput")
                .unwrap()
                .as_any()
                .is::<crate::AskSubmit>());
            assert!(bound_action(cx, "enter", "Composer DialogInput")
                .unwrap()
                .as_any()
                .is::<crate::DialogConfirm>());
            assert!(bound_action(cx, "enter", "Editor")
                .unwrap()
                .as_any()
                .is::<crate::Newline>());
        });
    }

    #[test]
    fn idle_hints_show_only_send() {
        for default in [SendMode::FollowUp, SendMode::Steer] {
            assert_eq!(
                shortcut_hints(default, false),
                vec![(keys::label(keys::SEND), tr!("composer.send"))]
            );
        }
    }
}
