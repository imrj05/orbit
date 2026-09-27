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
pub fn shortcut_hints(default: SendMode, running: bool) -> Vec<(&'static str, String)> {
    if running {
        [default, default.opposite()]
            .into_iter()
            .map(|mode| (mode.shortcut(default), mode.hint_label()))
            .collect()
    } else {
        vec![(keys::SEND, tr!("composer.send"))]
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
                vec![(keys::SEND, primary), (keys::SEND_ALTERNATE, alternate)]
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
                vec![(keys::SEND, tr!("composer.send"))]
            );
        }
    }
}
