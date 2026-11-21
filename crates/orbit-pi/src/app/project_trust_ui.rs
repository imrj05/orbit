//! Native confirmation for pi's project-trust boundary.
//!
//! An unknown workspace is already running safely with `--no-approve` when
//! this dialog opens. Trusting persists the decision and restarts the active
//! process so project resources load; continuing untrusted persists the
//! denial without interrupting the restricted session.

use super::*;
use crate::project_trust::{self, TrustDecision};
use crate::remote_hosts::RemoteHost;
use crate::theme::tokens::{modal, DynamicSpacing, TextSize};
use crate::ui::button::ButtonVariant;
use crate::ui::dialog::modal_button;
use crate::ui::prelude::*;

pub(crate) struct ProjectTrustPrompt {
    pub(super) key: String,
    pub(super) workspace: String,
    pub(super) local: bool,
}

pub(crate) fn trust_key(workspace: &Path, remote: Option<&RemoteHost>) -> String {
    remote.map_or_else(
        || project_trust::local_key(workspace),
        project_trust::remote_key,
    )
}

impl OrbitApp {
    pub(super) fn project_trust_for(&self, workspace: &Path) -> TrustDecision {
        let key = trust_key(workspace, self.remote.as_ref());
        self.project_trust.decision(&key)
    }

    /// Local MCP processes always use the local workspace's decision, even
    /// while the chat in front of the user is a trusted remote session.
    pub(super) fn local_project_trust_for(&self, workspace: &Path) -> TrustDecision {
        self.project_trust
            .decision(&project_trust::local_key(workspace))
    }

    pub(super) fn prompt_for_unknown_trust(
        &mut self,
        workspace: &Path,
        decision: TrustDecision,
    ) {
        if decision != TrustDecision::Unknown || self.project_trust_prompt.is_some() {
            return;
        }
        self.project_trust_prompt = Some(ProjectTrustPrompt {
            key: trust_key(workspace, self.remote.as_ref()),
            workspace: self.remote.as_ref().map_or_else(
                || workspace.display().to_string(),
                RemoteHost::location,
            ),
            local: self.remote.is_none(),
        });
        self.project_trust_focus_pending = true;
    }

    /// Palette entry for a workspace previously denied (or one whose prompt
    /// was superseded). Already-trusted workspaces have nothing to re-approve.
    pub(super) fn open_project_trust_prompt(&mut self, cx: &mut Context<Self>) {
        self.command_palette = None;
        let workspace = self.workspace_dir();
        match self.project_trust_for(&workspace) {
            TrustDecision::Trusted => {
                self.toast_info(tr!("trust.already"));
            }
            TrustDecision::Unknown | TrustDecision::Untrusted => {
                self.project_trust_prompt = None;
                self.prompt_for_unknown_trust(&workspace, TrustDecision::Unknown);
            }
        }
        cx.notify();
    }

    pub(super) fn confirm_project_trust(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.project_trust_prompt.take() else {
            return;
        };
        if let Err(error) = self.project_trust.set(&prompt.key, true) {
            self.project_trust_prompt = Some(prompt);
            self.project_trust_focus_pending = true;
            self.toast_error(tr!("trust.error", error = error));
            cx.notify();
            return;
        }
        let current = trust_key(&self.workspace_dir(), self.remote.as_ref());
        if current == prompt.key {
            if self.busy || self.is_compacting {
                self.project_trust_apply_pending = Some(prompt.key.clone());
                self.toast_info(tr!("trust.deferred"));
            } else {
                self.toast_success(tr!("trust.success"));
                self.runtime_restart(cx);
            }
        }
        cx.notify();
    }

    pub(super) fn deny_project_trust(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.project_trust_prompt.take() else {
            return;
        };
        if let Err(error) = self.project_trust.set(&prompt.key, false) {
            // Denial is already the process's enforced state. A broken store
            // must not trap the user behind a modal they cannot dismiss.
            self.toast_error(tr!("trust.error", error = error));
        } else {
            self.toast_info(tr!("trust.denied"));
        }
        self.project_trust_focus_pending = false;
        cx.notify();
    }

    fn project_trust_confirm_action(
        &mut self,
        _: &crate::PickerConfirm,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_project_trust(cx);
    }

    fn project_trust_cancel_action(
        &mut self,
        _: &crate::PickerCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.deny_project_trust(cx);
    }

    /// Apply a trust change only after the active run has settled. If the
    /// user switched workspaces meanwhile, their new workspace's own decision
    /// controls the next spawn instead.
    pub(super) fn tick_project_trust(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.project_trust_apply_pending.clone() else {
            return;
        };
        if self.busy || self.is_compacting {
            return;
        }
        self.project_trust_apply_pending = None;
        if trust_key(&self.workspace_dir(), self.remote.as_ref()) == key {
            self.runtime_restart(cx);
        }
    }

    pub(super) fn project_trust_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = self.project_trust_prompt.as_ref()?;
        let theme = theme.for_floating();
        let explanation = if prompt.local {
            tr!("trust.body_local", workspace = prompt.workspace.clone())
        } else {
            tr!("trust.body_remote", workspace = prompt.workspace.clone())
        };
        let footer = modal_footer(&theme)
            .child({
                let this = this.clone();
                modal_button(
                    "project-trust-cancel",
                    tr!("trust.deny"),
                    ButtonVariant::Secondary,
                    &self.modal_cancel_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.deny_project_trust(cx))
                })
            })
            .child({
                let this = this.clone();
                modal_button(
                    "project-trust-confirm",
                    tr!("trust.confirm"),
                    ButtonVariant::Primary,
                    &self.modal_confirm_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.confirm_project_trust(cx))
                })
            });
        let card = modal_card(&theme)
            .debug_selector(|| "project-trust-dialog".to_string())
            .modal_width(px(500.))
            .child(modal_header(&theme, tr!("trust.title"), None))
            .child(
                div()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(
                        div()
                            .whitespace_normal()
                            .text_size(TextSize::Default.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(explanation),
                    )
                    .child(
                        div()
                            .whitespace_normal()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!("trust.hint")),
                    ),
            )
            .child(footer);
        Some(
            modal_scrim(&theme)
                .id("project-trust-layer")
                .key_context("Picker")
                .track_focus(&self.project_trust_focus)
                .on_action(cx.listener(Self::project_trust_confirm_action))
                .on_action(cx.listener(Self::project_trust_cancel_action))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.deny_project_trust(cx))
                })
                .child(crate::motion::enter(
                    card,
                    "project-trust-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn local_mcp_never_uses_the_remote_sessions_trust(cx: &mut gpui::TestAppContext) {
        let app = cx.update(|cx| cx.new(OrbitApp::new));
        app.update(cx, |app, _| {
            let workspace = PathBuf::from("/tmp/orbit-local-trust-check");
            let host = RemoteHost {
                name: "prod".into(),
                ssh: "example.test".into(),
                user: None,
                port: None,
                dir: "/srv/app".into(),
                enabled: true,
                local_dir: None,
            };
            app.project_trust
                .set(&project_trust::local_key(&workspace), false)
                .unwrap();
            app.project_trust
                .set(&project_trust::remote_key(&host), true)
                .unwrap();
            app.remote = Some(host);
            assert_eq!(
                app.local_project_trust_for(&workspace),
                TrustDecision::Untrusted
            );
            assert_eq!(
                app.project_trust_for(&workspace),
                TrustDecision::Trusted
            );
        });
    }

    #[gpui::test]
    fn a_corrupt_trust_store_does_not_trap_the_denial_dialog(cx: &mut gpui::TestAppContext) {
        let app = cx.update(|cx| cx.new(OrbitApp::new));
        let path = std::env::temp_dir().join(format!(
            "orbit-corrupt-project-trust-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, b"not json").unwrap();
        app.update(cx, |app, cx| {
            app.project_trust = project_trust::ProjectTrust::load_from(path.clone());
            app.project_trust_prompt = Some(ProjectTrustPrompt {
                key: "local:/tmp/example".into(),
                workspace: "/tmp/example".into(),
                local: true,
            });
            app.deny_project_trust(cx);
            assert!(app.project_trust_prompt.is_none());
            assert!(!app.project_trust_focus_pending);
        });
        let _ = std::fs::remove_file(path);
    }
}
