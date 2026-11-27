//! The workbench's one confirmation prompt: what is waiting for a second
//! click, the words for it, and the layer that asks.
//!
//! A surface arms [`PendingConfirm`] instead of swapping its own row into a
//! confirm state; the prompt does the asking, and the same variant runs the
//! action when the user confirms. That keeps one card, one keyboard
//! contract, and one place where "Delete" means something.
//!
//! The layer is painted from the app root (`view.rs`), above Settings, so an
//! action armed from any page asks in the same place.

use super::*;
use crate::theme::tokens::{DynamicSpacing, IconSize, TextSize};
use crate::ui::glyph::{icon_tile, TileLook};
use crate::ui::{confirm::ConfirmPrompt, icon::icon};

/// A destructive action waiting on the user's answer. Each variant owns its
/// wording and the one call that finally runs it.
#[derive(Clone)]
pub(crate) enum PendingConfirm {
    /// Settings → Skills → Delete: the skill's directory on disk.
    DeleteSkill {
        /// The skill's folder — what the prompt removes.
        dir: PathBuf,
        /// The skill's name, so the prompt names it above the path.
        name: String,
    },
    /// Settings → Providers & models → Remove: the models.json entry.
    RemoveProvider { id: String, name: String },
    /// Settings → Plugins → Remove: the installed package source.
    RemovePlugin {
        source: String,
        project: bool,
        name: String,
    },
    /// Settings → MCP → Remove: one configured server.
    RemoveMcpServer { name: String },
    /// Settings → Remote hosts → Remove: one host entry.
    RemoveRemoteHost { name: String },
    /// Settings → Providers & models → Accounts → Remove: a profile and its
    /// credential.
    RemoveAccount {
        id: String,
        label: String,
        alias: String,
    },
    /// Sidebar → session row menu → Delete session…: pi's session file.
    DeleteSession {
        path: PathBuf,
        /// The row's title, so the prompt names the conversation.
        title: String,
        /// The captured working directory, if known.
        cwd: Option<PathBuf>,
    },
    /// Sidebar → project menu → Delete project & files: the folder on disk.
    DeleteWorkspaceFiles { cwd: PathBuf, label: String },
    /// Settings → Privacy → Clean up now, while the policy is set to delete:
    /// every old session file goes in one pass.
    RunSessionRetention,
    /// Settings → Subagents → Delete: the agent file on disk.
    DeleteSubagent {
        path: PathBuf,
        name: String,
        /// The file shadows a built-in of the same name, so deleting it
        /// brings the built-in back.
        overrides_builtin: bool,
    },
}

impl PendingConfirm {
    /// The element id prefix, so each prompt's parts are addressable and two
    /// surfaces never share one.
    fn id(&self) -> &'static str {
        match self {
            Self::DeleteSkill { .. } => "skill-delete-confirm",
            Self::DeleteSession { .. } => "session-delete-confirm",
            Self::DeleteWorkspaceFiles { .. } => "workspace-delete-files-confirm",
            Self::RemoveProvider { .. } => "provider-remove-confirm",
            Self::RemovePlugin { .. } => "plugin-remove-confirm",
            Self::RemoveMcpServer { .. } => "mcp-remove-confirm",
            Self::RemoveRemoteHost { .. } => "remote-host-remove-confirm",
            Self::RemoveAccount { .. } => "provider-account-remove-confirm",
            Self::RunSessionRetention => "session-retention-confirm",
            Self::DeleteSubagent { .. } => "subagent-delete-confirm",
        }
    }

    /// The question, kept a question: the object it names is in the target
    /// block below it, and what happens to it is [`body`](Self::body).
    fn title(&self) -> String {
        match self {
            Self::DeleteSkill { .. } => tr!("skills_ui.delete_confirm"),
            Self::DeleteSession { .. } => tr!("sidebar.delete_this_session"),
            Self::DeleteWorkspaceFiles { label, .. } => {
                tr!("sidebar.delete_project_confirm", name = label)
            }
            Self::RunSessionRetention => tr!("settings.session_retention_confirm"),
            Self::RemoveProvider { .. } => tr!("settings.remove_this_provider"),
            Self::RemovePlugin { .. } => tr!("settings.remove_this_plugin"),
            Self::RemoveMcpServer { .. } => tr!("mcp.remove_confirm"),
            Self::RemoveRemoteHost { .. } => tr!("remote_hosts.remove_confirm"),
            Self::RemoveAccount { .. } => tr!("accounts.remove_confirm"),
            Self::DeleteSubagent { .. } => tr!("subagents_ui.delete_confirm"),
        }
    }

    /// What happens to it, and whether it can be undone.
    fn body(&self) -> String {
        match self {
            Self::DeleteSkill { .. } => tr!("skills_ui.delete_confirm_body"),
            Self::DeleteSession { .. } => tr!("sidebar.removes_the_session_file_from_disk"),
            Self::DeleteWorkspaceFiles { .. } => {
                if crate::platform::TRASH_IS_RECOVERABLE {
                    tr!("sidebar.delete_project_body_trash")
                } else {
                    tr!("sidebar.delete_project_body_permanent")
                }
            }
            Self::RunSessionRetention => tr!("settings.session_retention_confirm_body"),
            Self::RemoveProvider { .. } => tr!("settings.remove_provider_body"),
            Self::RemovePlugin { .. } => tr!("settings.remove_plugin_body"),
            Self::RemoveMcpServer { .. } => tr!("mcp.remove_confirm_body"),
            Self::RemoveRemoteHost { .. } => tr!("remote_hosts.remove_confirm_body"),
            Self::RemoveAccount { .. } => tr!("accounts.remove_confirm_body"),
            Self::DeleteSubagent {
                name,
                overrides_builtin,
                ..
            } => {
                if *overrides_builtin {
                    tr!("subagents_ui.reset_confirm_body", name = name)
                } else {
                    tr!("subagents_ui.delete_confirm_body")
                }
            }
        }
    }

    /// The object the prompt names, on its own, so the question stays a
    /// question and the target is still readable when a path is long.
    fn target(&self, theme: &Theme) -> Option<AnyElement> {
        match self {
            Self::RunSessionRetention => None,
            Self::DeleteSkill { dir, name } => Some(object_line(
                self.id(),
                name.clone(),
                Some(crate::skills::display_path(dir)),
                theme,
            )),
            Self::DeleteSession { title, cwd, path } => {
                Some(session_target(title, cwd.as_deref(), path, theme))
            }
            Self::DeleteWorkspaceFiles { label, cwd } => Some(object_line(
                self.id(),
                label.clone(),
                Some(cwd.display().to_string()),
                theme,
            )),
            Self::RemoveProvider { id, name } => Some(object_line(
                self.id(),
                name.clone(),
                Some(id.clone()),
                theme,
            )),
            Self::RemovePlugin { source, name, .. } => Some(object_line(
                self.id(),
                name.clone(),
                Some(source.clone()),
                theme,
            )),
            Self::RemoveMcpServer { name } | Self::RemoveRemoteHost { name } => {
                Some(object_line(self.id(), name.clone(), None, theme))
            }
            Self::RemoveAccount { label, alias, .. } => Some(object_line(
                self.id(),
                label.clone(),
                Some(alias.clone()),
                theme,
            )),
            Self::DeleteSubagent { path, name, .. } => Some(object_line(
                self.id(),
                name.clone(),
                Some(path.display().to_string()),
                theme,
            )),
        }
    }

    fn confirm_label(&self) -> String {
        match self {
            Self::DeleteSkill { .. }
            | Self::DeleteSession { .. }
            | Self::DeleteWorkspaceFiles { .. } => tr!("sidebar.delete"),
            Self::RunSessionRetention => tr!("settings.session_retention_mode_delete"),
            Self::RemoveProvider { .. } => tr!("settings.remove"),
            Self::RemovePlugin { .. } => tr!("settings.remove"),
            Self::RemoveMcpServer { .. } => tr!("mcp.remove"),
            Self::RemoveRemoteHost { .. } => tr!("remote_hosts.remove"),
            Self::RemoveAccount { .. } => tr!("settings.remove"),
            Self::DeleteSubagent { .. } => tr!("subagents_ui.delete"),
        }
    }

    /// How wide the card is. A prompt naming a path wants more room than one
    /// naming a provider.
    fn width(&self, theme: &Theme) -> Pixels {
        match self {
            Self::DeleteSession { .. } | Self::DeleteWorkspaceFiles { .. } => px(480.),
            _ => theme.rems(20.),
        }
    }

    fn prompt(&self, theme: &Theme, session_scroll: &ScrollHandle) -> ConfirmPrompt {
        let mut prompt = ConfirmPrompt::new(self.id(), self.title())
            .width(self.width(theme))
            .cancel_label(tr!("view.cancel"))
            .confirm_label(self.confirm_label())
            .body(self.body())
            .leading(destructive_mark(theme));
        if let Some(target) = self.target(theme) {
            prompt = prompt.target(target);
        }
        if matches!(self, Self::DeleteSession { .. }) {
            prompt = prompt.scroll_target(session_scroll);
        }
        prompt
    }
}

/// The media every confirmation carries beside its question: the destructive
/// glyph on the shared danger-washed tile, so the consequence reads before the
/// sentence does.
fn destructive_mark(theme: &Theme) -> AnyElement {
    icon_tile(
        icon(
            "icons/trash.svg",
            IconSize::Custom(1.5).px(theme),
            theme.error,
        ),
        TileLook::Destructive,
        theme,
    )
    .debug_selector(|| "confirm-mark".into())
    .into_any_element()
}

fn session_target(title: &str, cwd: Option<&Path>, path: &Path, theme: &Theme) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base12.px(theme))
        .child(
            div()
                .debug_selector(|| "session-delete-confirm-target-title".into())
                .whitespace_normal()
                .text_size(TextSize::Default.px(theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(title.to_owned()),
        )
        .children(cwd.filter(|cwd| !cwd.as_os_str().is_empty()).map(|cwd| {
            session_path_row(
                "session-delete-confirm-workspace",
                "icons/folder.svg",
                tr!("session.detail_workspace"),
                cwd,
                theme,
            )
        }))
        .child(session_path_row(
            "session-delete-confirm-file",
            "icons/file.svg",
            tr!("session.detail_file"),
            path,
            theme,
        ))
        .into_any_element()
}

fn session_path_row(
    id: &'static str,
    glyph: &'static str,
    label: String,
    path: &Path,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(DynamicSpacing::Base08.px(theme))
        .child(
            div()
                .flex_none()
                .child(icon(glyph, IconSize::Small.px(theme), theme.text_muted)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base02.px(theme))
                .whitespace_normal()
                .text_left()
                .text_size(TextSize::Small.px(theme))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(label),
                )
                .child(
                    div()
                        .debug_selector(move || id.into())
                        .font_family(theme::code_font_family())
                        .text_color(theme.text_secondary)
                        .child(path.display().to_string()),
                ),
        )
}

/// The target block: a wrapped name over a quiet detail (a provider id, a
/// package source, an account alias) in the code face. Both take the card's
/// width so long names and paths wrap within it.
fn object_line(id: &str, label: String, detail: Option<String>, theme: &Theme) -> AnyElement {
    let mut block = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base02.px(theme))
        .child(
            div()
                .debug_selector({
                    let id = id.to_string();
                    move || format!("{id}-target-title")
                })
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .text_size(TextSize::Default.px(theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(label),
        );
    if let Some(detail) = detail {
        block = block.child(
            div()
                .debug_selector({
                    let id = id.to_string();
                    move || format!("{id}-target-detail")
                })
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .font_family(theme::code_font_family())
                .text_size(TextSize::XSmall.px(theme))
                .text_color(theme.text_muted)
                .child(detail),
        );
    }
    block.into_any_element()
}

impl OrbitApp {
    /// Ask before running `confirm`. The prompt takes the keyboard, so a
    /// stray Enter lands on Cancel rather than on the destructive button.
    pub(super) fn request_confirm(&mut self, confirm: PendingConfirm, cx: &mut Context<Self>) {
        if matches!(confirm, PendingConfirm::DeleteSession { .. }) {
            self.session_delete_scroll = ScrollHandle::new();
        }
        self.pending_confirm = Some(confirm);
        cx.notify();
    }

    /// Back out without running anything.
    pub(super) fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        if self.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    /// The destructive half: the only path from the prompt to the action.
    pub(super) fn run_pending_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(confirm) = self.pending_confirm.take() else {
            return;
        };
        match confirm {
            PendingConfirm::DeleteSkill { dir, .. } => self.delete_skill_dir(dir, cx),
            PendingConfirm::DeleteSession { path, .. } => {
                self.delete_session_file(path, window, cx)
            }
            PendingConfirm::DeleteWorkspaceFiles { cwd, label } => {
                self.delete_workspace_files(cwd, label, cx)
            }
            PendingConfirm::RunSessionRetention => self.run_session_retention(cx),
            PendingConfirm::RemoveProvider { id, .. } => self.provider_remove(id, cx),
            PendingConfirm::RemovePlugin {
                source, project, ..
            } => self.plugin_remove_confirmed(source, project, cx),
            PendingConfirm::RemoveMcpServer { name } => self.mcp_confirm_remove(name, cx),
            PendingConfirm::RemoveRemoteHost { name } => self.remote_host_confirm_remove(name, cx),
            PendingConfirm::RemoveAccount { id, .. } => self.account_remove(id, cx),
            PendingConfirm::DeleteSubagent { .. } => {
                // The ConfirmDelete action still owns the unsaved-edits
                // guard, exactly as the inline button did.
                self.apply_subagent_action(subagents_ui::SubagentAction::ConfirmDelete, cx)
            }
        }
    }

    /// The prompt, or `None` when nothing is waiting on an answer.
    pub(super) fn confirm_layer(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Opening hands the keyboard to Cancel; closing gives it back to the
        // composer, so it is never left on a button that no longer exists.
        let showing = self.pending_confirm.is_some();
        if showing != self.confirm_showing {
            self.confirm_showing = showing;
            if showing {
                window.focus(&self.modal_cancel_focus);
            } else {
                self.return_focus(None, window, cx);
            }
        }
        let confirm = self.pending_confirm.as_ref()?;
        let this = cx.entity();
        let cancelling = this.clone();
        Some(
            confirm
                .prompt(&theme, &self.session_delete_scroll)
                .focus(&self.modal_cancel_focus, &self.modal_confirm_focus)
                .on_confirm(move |window, cx| {
                    this.update(cx, |app, cx| app.run_pending_confirm(window, cx))
                })
                .on_cancel(move |_, cx| cancelling.update(cx, |app, cx| app.cancel_confirm(cx)))
                .layer(&theme)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_variant() -> Vec<PendingConfirm> {
        vec![
            PendingConfirm::DeleteSkill {
                dir: PathBuf::from("/tmp/skills/notes"),
                name: "notes".into(),
            },
            PendingConfirm::DeleteSession {
                path: PathBuf::from("/tmp/sessions/s1.jsonl"),
                title: "s1".into(),
                cwd: Some(PathBuf::from("/tmp/projects/alpha")),
            },
            PendingConfirm::DeleteWorkspaceFiles {
                cwd: PathBuf::from("/tmp/projects/alpha"),
                label: "alpha".into(),
            },
            PendingConfirm::RemoveProvider {
                id: "acme".into(),
                name: "Acme".into(),
            },
            PendingConfirm::RemovePlugin {
                source: "github:acme/tools".into(),
                project: false,
                name: "tools".into(),
            },
            PendingConfirm::RemoveMcpServer {
                name: "docs".into(),
            },
            PendingConfirm::RemoveRemoteHost {
                name: "alpha".into(),
            },
            PendingConfirm::RunSessionRetention,
            PendingConfirm::RemoveAccount {
                id: "openai~work".into(),
                label: "Work".into(),
                alias: "openai~work".into(),
            },
            PendingConfirm::DeleteSubagent {
                path: PathBuf::from("/tmp/agents/scout.md"),
                name: "scout".into(),
                overrides_builtin: false,
            },
        ]
    }

    struct PromptProbe;

    impl gpui::Render for PromptProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *crate::theme::get(cx);
            let target = object_line(
                "probe-confirm",
                "Review guidelines for code changes and an exceptionally long conversation title that should wrap onto another line".into(),
                Some(
                    "unbroken-session-folder-name-which-is-extremely-long-and-must-wrap-inside-the-confirmation-dialog-without-truncation".into(),
                ),
                &theme,
            );
            div().size_full().relative().child(
                ConfirmPrompt::new("probe-confirm", "Delete this?")
                    .target(target)
                    .on_confirm(|_, _| {})
                    .on_cancel(|_, _| {})
                    .layer(&theme),
            )
        }
    }

    /// Both lines wrap, including a long unbroken detail, within the card.
    #[gpui::test]
    fn target_lines_wrap_within_the_card(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;
        use gpui::{point, px, size};

        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        crate::theme::set_os_reduce_motion_for_test(true);
        let cx = cx.add_empty_window();
        cx.draw(point(px(0.), px(0.)), size(px(900.), px(600.)), |_, cx| {
            cx.new(|_| PromptProbe)
        });
        let card = cx
            .debug_bounds("probe-confirm-card")
            .expect("the card paints");
        let body = cx
            .debug_bounds("probe-confirm-body")
            .expect("the body paints");
        assert!(
            body.size.width > card.size.width - px(4.),
            "body {body:?} inside {card:?}"
        );
        for selector in ["probe-confirm-target-title", "probe-confirm-target-detail"] {
            let line = cx.debug_bounds(selector).expect("the target line paints");
            assert!(
                line.size.width > px(120.),
                "{selector} collapsed to {line:?} inside {body:?}"
            );
            assert!(
                line.size.height > px(25.),
                "{selector} did not wrap: {line:?}"
            );
            assert!(
                line.left() >= body.left() && line.right() <= body.right() + px(1.),
                "{selector} {line:?} runs outside {body:?}"
            );
        }
        crate::theme::set_os_reduce_motion_for_test(false);
    }

    struct SessionModalProbe {
        prompt: PendingConfirm,
        scroll: ScrollHandle,
        cancel: FocusHandle,
        confirm: FocusHandle,
    }

    impl Render for SessionModalProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = theme::get(cx);
            div().relative().size_full().child(
                self.prompt
                    .prompt(theme, &self.scroll)
                    .focus(&self.cancel, &self.confirm)
                    .layer(theme),
            )
        }
    }

    fn open_session_modal(
        cx: &mut gpui::TestAppContext,
        prefs: theme::UiPrefs,
        prompt: PendingConfirm,
    ) -> (Entity<SessionModalProbe>, &mut gpui::VisualTestContext) {
        cx.update(|cx| {
            let mut theme = Theme::for_id(theme::ThemeId::Orbit);
            theme.ui = prefs;
            theme.ui.motion = theme::MotionPreference::Reduced;
            cx.set_global(theme);
            cx.set_global(theme::AppearancePrefs::default());
            crate::bind_keys(cx);
        });
        let (probe, cx) = cx.add_window_view(|window, cx| {
            let cancel = cx.focus_handle();
            window.focus(&cancel);
            SessionModalProbe {
                prompt,
                scroll: ScrollHandle::new(),
                cancel,
                confirm: cx.focus_handle(),
            }
        });
        cx.simulate_resize(gpui::size(px(960.), px(640.)));
        cx.run_until_parked();
        (probe, cx)
    }

    #[gpui::test]
    fn session_delete_lays_out_the_full_title_and_paths_at_minimum_window(
        cx: &mut gpui::TestAppContext,
    ) {
        let (probe, cx) = open_session_modal(
            cx,
            theme::UiPrefs::default(),
            PendingConfirm::DeleteSession {
                title: "Review guidelines for code changes and a long conversation title "
                    .repeat(3),
                cwd: Some(PathBuf::from(format!("/work/{}", "workspace".repeat(15)))),
                path: PathBuf::from(format!("/store/sessions/{}.jsonl", "session".repeat(24))),
            },
        );
        let mut bounds = |id| {
            cx.debug_bounds(id)
                .unwrap_or_else(|| panic!("missing {id}"))
        };
        let card = bounds("session-delete-confirm-card");
        let body = bounds("session-delete-confirm-body");
        let title = bounds("session-delete-confirm-target-title");
        let cwd = bounds("session-delete-confirm-workspace");
        let file = bounds("session-delete-confirm-file");
        let actions = bounds("session-delete-confirm-actions");
        assert_eq!(card.size.width, px(480.));
        for line in [title, cwd, file] {
            assert!(line.size.height > px(25.), "text did not wrap: {line:?}");
            assert!(line.left() >= body.left() && line.right() <= body.right());
        }
        assert!(title.bottom() < cwd.top() && cwd.bottom() < file.top());
        assert!(
            file.bottom() <= body.bottom() && body.bottom() <= actions.top() + px(1.),
            "file {file:?}, body {body:?}, actions {actions:?}"
        );
        assert!(actions.bottom() <= card.bottom() && card.bottom() <= px(640.));
        cx.update(|window, cx| {
            assert!(probe.read(cx).cancel.is_focused(window));
        });
    }

    #[gpui::test]
    fn session_delete_pages_long_content_at_maximum_scale_without_hiding_actions(
        cx: &mut gpui::TestAppContext,
    ) {
        let (probe, cx) = open_session_modal(
            cx,
            theme::UiPrefs {
                ui_font_size: *theme::FONT_SIZES.last().unwrap(),
                spacing_density: *theme::SPACING_DENSITIES.last().unwrap(),
                ..Default::default()
            },
            PendingConfirm::DeleteSession {
                title: "Review guidelines for code changes and a long conversation title "
                    .repeat(30),
                cwd: None,
                path: PathBuf::from(format!("/store/sessions/{}.jsonl", "session".repeat(30))),
            },
        );
        assert!(cx
            .debug_bounds("session-delete-confirm-workspace")
            .is_none());
        let card = cx.debug_bounds("session-delete-confirm-card").unwrap();
        let actions = cx.debug_bounds("session-delete-confirm-actions").unwrap();
        assert!(
            card.top() >= px(0.) && card.bottom() <= px(640.),
            "{card:?}"
        );
        assert!(actions.bottom() <= card.bottom());
        let scroll = cx.update(|_, cx| probe.read(cx).scroll.clone());
        assert!(scroll.max_offset().height > px(0.));
        assert_eq!(scroll.offset().y, px(0.));

        cx.simulate_keystrokes("pagedown");
        cx.run_until_parked();
        let paged = scroll.offset().y;
        assert!(
            paged < px(0.),
            "Page Down must reach the scroll body from Cancel"
        );
        assert_eq!(
            cx.debug_bounds("session-delete-confirm-actions"),
            Some(actions)
        );
        cx.simulate_keystrokes("pageup");
        cx.run_until_parked();
        assert_eq!(scroll.offset().y, px(0.));

        for _ in 0..10 {
            cx.simulate_keystrokes("pagedown");
            cx.run_until_parked();
        }
        assert_eq!(scroll.offset().y, -scroll.max_offset().height);
        let file = cx.debug_bounds("session-delete-confirm-file").unwrap();
        let body = scroll.bounds();
        assert!(file.top() >= body.top() && file.bottom() <= body.bottom() + px(1.));
        assert_eq!(
            cx.debug_bounds("session-delete-confirm-actions"),
            Some(actions)
        );
    }

    /// A prompt that now carries a consequence line, with no scrollable
    /// target, still keeps its footer inside the window at the largest UI
    /// scale — the media and the header grow, the card does not push its
    /// actions off.
    #[gpui::test]
    fn a_described_prompt_keeps_its_actions_at_maximum_scale(cx: &mut gpui::TestAppContext) {
        let (_, cx) = open_session_modal(
            cx,
            theme::UiPrefs {
                ui_font_size: *theme::FONT_SIZES.last().unwrap(),
                spacing_density: *theme::SPACING_DENSITIES.last().unwrap(),
                ..Default::default()
            },
            PendingConfirm::RemoveMcpServer {
                name: "a-very-long-mcp-server-name-that-has-to-wrap-inside-the-card".into(),
            },
        );
        let mut bounds = |id| {
            cx.debug_bounds(id)
                .unwrap_or_else(|| panic!("missing {id}"))
        };
        let card = bounds("mcp-remove-confirm-card");
        let header = bounds("mcp-remove-confirm-title");
        let mark = bounds("confirm-mark");
        let heading = bounds("mcp-remove-confirm-heading");
        let body = bounds("mcp-remove-confirm-body");
        let target = bounds("mcp-remove-confirm-target-title");
        let actions = bounds("mcp-remove-confirm-actions");
        let cancel = bounds("mcp-remove-confirm-cancel-button");
        let confirm = bounds("mcp-remove-confirm-confirm-button");
        // The media is the alert-dialog proportion (40px holding a 24px
        // glyph), not a 16px glyph floating beside the paragraph.
        assert!(
            mark.size.width >= px(36.),
            "mark {mark:?} is the size of a row tile, not a dialog media"
        );
        // Media and text share the card's center axis without narrowing the copy.
        assert!(
            mark.bottom() <= heading.top() + px(1.),
            "mark {mark:?} sits beside the heading {heading:?} instead of above it"
        );
        assert!(
            heading.size.height > mark.size.height,
            "heading {heading:?} collapsed under the media"
        );
        for item in [mark, heading, target] {
            assert!((item.center().x - card.center().x).abs() <= px(1.));
        }
        assert!((cancel.size.width - confirm.size.width).abs() <= px(1.));
        assert!(
            (cancel.left() - actions.left() - (actions.right() - confirm.right())).abs() <= px(1.)
        );
        assert!(cancel.right() < confirm.left());
        assert!(
            mark.left() >= header.left() && mark.right() < header.right(),
            "mark {mark:?} runs outside {header:?}"
        );
        assert!(
            header.bottom() <= body.top() + px(1.),
            "header {header:?} overlaps body {body:?}"
        );
        assert!(
            target.left() >= body.left() && target.right() <= body.right() + px(1.),
            "target {target:?} runs outside body {body:?}"
        );
        assert!(
            body.bottom() <= actions.top() + px(1.),
            "body {body:?} overlaps actions {actions:?}"
        );
        assert!(actions.bottom() <= card.bottom() + px(1.));
        assert!(
            card.top() >= px(0.) && card.bottom() <= px(640.),
            "{card:?}"
        );
    }

    /// Each surface owns its prompt id and its own verb, so two armed asks
    /// never paint the same element ids and none falls back to a bare "OK".
    /// Every card asks a question and says the consequence under it; the
    /// object it acts on rides in the target block.
    #[test]
    fn each_removal_names_its_object_and_its_own_prompt() {
        let theme = Theme::for_id(crate::theme::ThemeId::ALL[0]);
        let mut ids = std::collections::HashSet::new();
        for confirm in every_variant() {
            assert!(ids.insert(confirm.id()), "{} is used twice", confirm.id());
            assert!(
                !confirm.title().is_empty(),
                "{} has no question",
                confirm.id()
            );
            assert!(
                !confirm.body().is_empty(),
                "{} does not say what happens",
                confirm.id()
            );
            assert_ne!(
                confirm.title(),
                confirm.body(),
                "{} asks and answers with one sentence",
                confirm.id()
            );
            assert!(
                !confirm.confirm_label().is_empty(),
                "{} has no action label",
                confirm.id()
            );
            // Session retention acts on every old session at once; every
            // other prompt has one object to name.
            if !matches!(confirm, PendingConfirm::RunSessionRetention) {
                assert!(
                    confirm.target(&theme).is_some(),
                    "{} names no object",
                    confirm.id()
                );
            }
        }

        let provider = PendingConfirm::RemoveProvider {
            id: "acme".into(),
            name: "Acme".into(),
        };
        assert_eq!(
            provider.title(),
            tr!("settings.remove_this_provider").to_string()
        );

        // The account's name is the target's title; the card asks the
        // question and the body carries the sign-out.
        let account = PendingConfirm::RemoveAccount {
            id: "openai~work".into(),
            label: "Work".into(),
            alias: "openai~work".into(),
        };
        assert_eq!(account.title(), tr!("accounts.remove_confirm").to_string());
        assert_eq!(
            account.body(),
            tr!("accounts.remove_confirm_body").to_string()
        );

        // A file that shadows a built-in says the built-in comes back; a
        // plain agent file says Undo can restore it.
        let plain = PendingConfirm::DeleteSubagent {
            path: PathBuf::from("/tmp/agents/scout.md"),
            name: "scout".into(),
            overrides_builtin: false,
        };
        let override_file = PendingConfirm::DeleteSubagent {
            path: PathBuf::from("/tmp/agents/scout.md"),
            name: "scout".into(),
            overrides_builtin: true,
        };
        assert_eq!(plain.title(), override_file.title());
        assert_eq!(
            plain.body(),
            tr!("subagents_ui.delete_confirm_body").to_string()
        );
        assert_eq!(
            override_file.body(),
            tr!("subagents_ui.reset_confirm_body", name = "scout").to_string()
        );
        assert_ne!(plain.body(), override_file.body());
    }
}
