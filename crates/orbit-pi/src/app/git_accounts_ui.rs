//! Settings presentation and background lifecycle for local Git diagnostics,
//! plus the one thing the page edits: the current repository's own commit
//! identity (`git config --local`, see `crate::git_identity`).
use super::*;
use crate::git_identity::{self, Invalid, LocalIdentity};
use crate::ui::button::{Button, ButtonVariant};
use crate::git_accounts::{self, State};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, TextSize};
use crate::ui::prelude::*;

fn state_text<T>(state: &State<T>) -> String {
    match state {
        State::Detected(_) => tr!("git_accounts.detected"),
        State::NotConfigured => tr!("git_accounts.not_configured"),
        State::Unavailable => tr!("git_accounts.unavailable"),
        State::Unknown => tr!("git_accounts.unknown"),
        State::Failed => tr!("git_accounts.failed"),
    }
}
fn scope(s: &str) -> String {
    match s {
        "system" => tr!("git_accounts.scope_system"),
        "global" => tr!("git_accounts.scope_global"),
        "local" => tr!("git_accounts.scope_local"),
        "worktree" => tr!("git_accounts.scope_worktree"),
        "command" => tr!("git_accounts.scope_command"),
        _ => tr!("git_accounts.unknown"),
    }
}
fn invalid_text(invalid: Invalid) -> String {
    match invalid {
        Invalid::EmptyName => tr!("git_accounts.local_name_required"),
        Invalid::EmptyEmail => tr!("git_accounts.local_email_required"),
        Invalid::BadName => tr!("git_accounts.local_name_invalid"),
        Invalid::BadEmail => tr!("git_accounts.local_email_invalid"),
    }
}

/// Save is live only for a complete pair that differs from what the
/// repository already holds.
fn can_save_identity(stored: &LocalIdentity, name: &str, email: &str) -> bool {
    let (name, email) = (name.trim(), email.trim());
    !name.is_empty()
        && !email.is_empty()
        && (stored.name.as_deref() != Some(name) || stored.email.as_deref() != Some(email))
}

fn gh_release_status(status: GhUpdateStatus, installed: &State<String>) -> String {
    match status {
        GhUpdateStatus::NotChecked => tr!("git_accounts.update_not_checked"),
        GhUpdateStatus::Checking => tr!("git_accounts.update_checking"),
        GhUpdateStatus::Failed => tr!("git_accounts.update_failed"),
        GhUpdateStatus::Latest(latest) => {
            let version = format!("{}.{}.{}", latest.0, latest.1, latest.2);
            match installed {
                State::Detected(current)
                    if crate::gh::parse_version(current).is_some_and(|v| v < latest) =>
                {
                    tr!("git_accounts.update_available", version = version)
                }
                State::Detected(current) if crate::gh::parse_version(current) == Some(latest) => {
                    tr!("git_accounts.update_current", version = version)
                }
                _ => tr!("git_accounts.update_latest", version = version),
            }
        }
    }
}

impl OrbitApp {
    fn check_gh_release(&mut self, cx: &mut Context<Self>) {
        if self.gh_latest == GhUpdateStatus::Checking {
            return;
        }
        self.gh_latest = GhUpdateStatus::Checking;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let latest = cx
                .background_executor()
                .spawn(async { crate::gh::latest_cli_version() })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.gh_latest = latest.map_or(GhUpdateStatus::Failed, GhUpdateStatus::Latest);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn refresh_git_accounts_for_workspace(&mut self, cx: &mut Context<Self>) {
        self.git_accounts_generation = self.git_accounts_generation.wrapping_add(1);
        let generation = self.git_accounts_generation;
        let workspace = self.current_workspace.clone();
        self.git_accounts_loading = true;
        self.git_accounts_snapshot = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async move { git_accounts::discover(workspace) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.git_accounts_generation == generation {
                    app.git_accounts_loading = false;
                    app.seed_git_identity_form(snapshot.repository.as_ref(), cx);
                    app.git_accounts_snapshot = Some(snapshot);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Show what the repository holds now, so the form never edits a stale
    /// value: its own override, or empty fields over the global fallback.
    fn seed_git_identity_form(&mut self, repo: Option<&LocalIdentity>, cx: &mut Context<Self>) {
        let name = repo.and_then(|r| r.name.clone()).unwrap_or_default();
        let email = repo.and_then(|r| r.email.clone()).unwrap_or_default();
        self.git_identity_name.update(cx, |input, cx| input.set_text(name, cx));
        self.git_identity_email.update(cx, |input, cx| input.set_text(email, cx));
        self.git_identity_error = None;
    }

    /// Write the typed name and email to the current repository's config.
    fn save_git_identity(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = self.current_workspace.clone() else {
            return;
        };
        if self.git_identity_busy {
            return;
        }
        let name = self.git_identity_name.read(cx).text();
        let email = self.git_identity_email.read(cx).text();
        let (name, email) = match git_identity::validate(&name, &email) {
            Ok(pair) => pair,
            Err(invalid) => {
                self.git_identity_error = Some(invalid_text(invalid));
                cx.notify();
                return;
            }
        };
        self.run_git_identity_change(
            tr!("git_accounts.local_saved", project = self.workspace_label()),
            move || git_identity::save(&workspace, &name, &email),
            cx,
        );
    }

    /// Drop the repository's override so the global identity applies again.
    fn clear_git_identity(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = self.current_workspace.clone() else {
            return;
        };
        if self.git_identity_busy {
            return;
        }
        self.run_git_identity_change(
            tr!("git_accounts.local_cleared", project = self.workspace_label()),
            move || git_identity::clear(&workspace),
            cx,
        );
    }

    fn run_git_identity_change(
        &mut self,
        done: String,
        change: impl FnOnce() -> Result<(), String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.git_identity_busy = true;
        self.git_identity_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { change() }).await;
            let _ = this.update(cx, |app, cx| {
                app.git_identity_busy = false;
                match result {
                    Ok(()) => {
                        app.toast_success(done);
                        // Re-read: the effective rows and the form follow git.
                        app.refresh_git_accounts_for_workspace(cx);
                    }
                    Err(error) => {
                        app.git_identity_error =
                            Some(tr!("git_accounts.local_failed", error = error));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Settings → Git Accounts → the current repository's identity form.
    fn git_identity_section(
        &self,
        theme: Theme,
        repo: Option<&LocalIdentity>,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let project = self.workspace_label();
        let title = tr!("git_accounts.local_title", project = project);
        let Some(repo) = repo else {
            return self.settings_section_desc(
                theme,
                &title,
                None,
                vec![self.setting_row(
                    theme,
                    &tr!("git_accounts.local_not_repo"),
                    Some(&tr!("git_accounts.local_not_repo_hint")),
                    None,
                    None,
                )],
            );
        };
        let name = self.git_identity_name.read(cx).text();
        let email = self.git_identity_email.read(cx).text();
        let busy = self.git_identity_busy;
        let can_save = !busy && can_save_identity(repo, &name, &email);
        let save_this = this.clone();
        let save = Button::new("git-identity-save", ButtonVariant::Primary)
            .label(tr!("git_accounts.local_save"))
            .loading(busy)
            .disabled(!can_save)
            .on_mouse_up(&theme, move |_, _, cx| {
                save_this.update(cx, |app, cx| app.save_git_identity(cx));
            })
            .into_any_element();
        let mut actions = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme));
        if repo.is_set() {
            actions = actions.child(
                Button::new("git-identity-clear", ButtonVariant::Secondary)
                    .label(tr!("git_accounts.local_clear"))
                    .disabled(busy)
                    .on_mouse_up(&theme, move |_, _, cx| {
                        this.update(cx, |app, cx| app.clear_git_identity(cx));
                    }),
            );
        }
        let actions = actions.child(save).into_any_element();
        let (status, status_hint) = if repo.is_set() {
            (
                tr!("git_accounts.local_overridden"),
                tr!("git_accounts.local_overridden_hint"),
            )
        } else {
            (
                tr!("git_accounts.local_inherited"),
                tr!("git_accounts.local_inherited_hint"),
            )
        };
        let mut rows = vec![
            self.setting_row(
                theme,
                &tr!("git_accounts.name"),
                None,
                None,
                Some(self.settings_text_field(&self.git_identity_name, &theme)),
            ),
            self.setting_row(
                theme,
                &tr!("git_accounts.email"),
                None,
                None,
                Some(self.settings_text_field(&self.git_identity_email, &theme)),
            ),
            self.setting_row(theme, &status, Some(&status_hint), None, Some(actions)),
        ];
        if let Some(error) = &self.git_identity_error {
            rows.push(self.setting_row(
                theme,
                &tr!("git_accounts.local_error_title"),
                Some(error),
                None,
                None,
            ));
        }
        self.settings_section_desc(
            theme,
            &title,
            Some(&tr!("git_accounts.local_description")),
            rows,
        )
    }

    pub(super) fn git_accounts_toolbar(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        Button::new("git-accounts-refresh", ButtonVariant::Secondary)
            .size(ButtonSize::Large)
            .leading(refresh_glyph(
                "git-accounts-refresh-spin",
                ButtonSize::Large.icon_size().px(&theme),
                self.git_accounts_loading,
                theme.text_secondary,
                theme,
            ))
            .label(tr!("git_accounts.refresh"))
            .loading(self.git_accounts_loading)
            .on_mouse_up(&theme, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    if !app.git_accounts_loading {
                        app.refresh_git_accounts_for_workspace(cx);
                    }
                });
            })
            .into_any_element()
    }

    pub(super) fn git_accounts_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(snapshot) = &self.git_accounts_snapshot else {
            return vec![div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("git_accounts.loading"))
                .into_any_element()];
        };
        let value = |s: &State<git_accounts::ConfigValue>| match s {
            State::Detected(v) => format!("{} · {}", v.value, scope(&v.scope)),
            _ => state_text(s),
        };
        let row = |title: String, detail: String| {
            self.setting_row(theme, &title, None, Some(&detail), None)
        };
        let summary = if matches!(snapshot.ssh, State::Detected(_))
            || matches!(snapshot.helpers, State::Detected(_))
            || snapshot.gh_token_present
            || snapshot.github_token_present
        {
            tr!("git_accounts.summary_configured")
        } else {
            tr!("git_accounts.summary_unknown")
        };
        let mut sections = Vec::new();
        sections.push(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_secondary)
                .child(summary)
                .into_any_element(),
        );
        sections.push(self.settings_section_desc(
            theme,
            &tr!("git_accounts.identity"),
            Some(&tr!("git_accounts.identity_note")),
            vec![
                row(tr!("git_accounts.name"), value(&snapshot.name)),
                row(tr!("git_accounts.email"), value(&snapshot.email)),
            ],
        ));
        sections.push(self.git_identity_section(
            theme,
            snapshot.repository.as_ref(),
            this.clone(),
            cx,
        ));
        let gh = match &snapshot.gh {
            State::Detected(version)
                if crate::gh::parse_version(version)
                    .is_some_and(|v| v < crate::gh::MIN_GH_VERSION) =>
            {
                tr!("git_accounts.version_old", version = version)
            }
            State::Detected(version) => tr!("git_accounts.version", version = version),
            other => state_text(other),
        };
        let checking = self.gh_latest == GhUpdateStatus::Checking;
        let check_button = Button::new("gh-check-updates", ButtonVariant::Secondary)
            .label(tr!("git_accounts.update_check"))
            .loading(checking)
            .on_mouse_up(&theme, move |_, _, cx| {
                this.update(cx, |app, cx| app.check_gh_release(cx));
            })
            .into_any_element();
        sections.push(self.settings_section_desc(
            theme,
            &tr!("git_accounts.github"),
            Some(&tr!("git_accounts.github_note")),
            vec![
                row(tr!("git_accounts.cli"), gh),
                row(tr!("git_accounts.auth_status"), tr!("git_accounts.unknown")),
                self.setting_row(
                    theme,
                    &tr!("git_accounts.update_title"),
                    Some(&tr!("git_accounts.update_check_note")),
                    Some(&gh_release_status(self.gh_latest, &snapshot.gh)),
                    Some(check_button),
                ),
                self.setting_row(
                    theme,
                    &tr!("git_accounts.update_manual"),
                    Some(&tr!("git_accounts.update_manual_note")),
                    None,
                    None,
                ),
                row(
                    tr!("git_accounts.update_homebrew"),
                    "brew upgrade gh".to_owned(),
                ),
                row(
                    tr!("git_accounts.update_winget"),
                    "winget upgrade --id GitHub.cli --source winget".to_owned(),
                ),
            ],
        ));
        let ssh_state = match &snapshot.ssh {
            State::NotConfigured => tr!("git_accounts.no_identities"),
            other => state_text(other),
        };
        let public_keys = match &snapshot.public_keys {
            State::Detected(count) => tr!("git_accounts.public_keys_count", count = count),
            other => state_text(other),
        };
        let mut ssh_rows = vec![
            row(tr!("git_accounts.agent"), ssh_state),
            row(tr!("git_accounts.public_keys"), public_keys),
        ];
        if let State::Detected(keys) = &snapshot.ssh {
            for key in keys {
                ssh_rows.push(row(key.algorithm.clone(), key.fingerprint.clone()));
            }
        }
        sections.push(self.settings_section_desc(
            theme,
            &tr!("git_accounts.ssh"),
            Some(&tr!("git_accounts.ssh_note")),
            ssh_rows,
        ));
        let mut helper_rows = vec![row(
            tr!("git_accounts.helpers"),
            state_text(&snapshot.helpers),
        )];
        if let State::Detected(helpers) = &snapshot.helpers {
            for helper in helpers {
                let kind = if helper.kind == "custom" {
                    tr!("git_accounts.custom_helper")
                } else {
                    helper.kind.to_string()
                };
                let detail = match &helper.host {
                    Some(host) => format!("{} · {}", host, scope(&helper.scope)),
                    None => scope(&helper.scope),
                };
                helper_rows.push(row(kind, detail));
            }
        }
        sections.push(self.settings_section_desc(
            theme,
            &tr!("git_accounts.https"),
            Some(&tr!("git_accounts.helper_note")),
            helper_rows,
        ));
        let mut remote_rows = vec![row(
            tr!("git_accounts.remotes"),
            state_text(&snapshot.remotes),
        )];
        if let State::Detected(remotes) = &snapshot.remotes {
            for remote in remotes {
                remote_rows.push(row(
                    remote.name.clone(),
                    format!("{} · {}", remote.host, remote.transport),
                ));
            }
        }
        sections.push(self.settings_section_desc(
            theme,
            &tr!("git_accounts.repository"),
            Some(&tr!("git_accounts.repository_note")),
            remote_rows,
        ));
        let present = |v| {
            if v {
                tr!("git_accounts.detected")
            } else {
                tr!("git_accounts.not_detected")
            }
        };
        sections.push(self.settings_section_desc(
            theme,
            &tr!("git_accounts.environment"),
            Some(&tr!("git_accounts.environment_note")),
            vec![
                row("GH_TOKEN".to_string(), present(snapshot.gh_token_present)),
                row(
                    "GITHUB_TOKEN".to_string(),
                    present(snapshot.github_token_present),
                ),
            ],
        ));
        sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_needs_a_complete_pair_that_changes_something() {
        let empty = LocalIdentity::default();
        assert!(can_save_identity(&empty, "Ada", "ada@example.test"));
        assert!(!can_save_identity(&empty, "Ada", "  "));
        assert!(!can_save_identity(&empty, "", "ada@example.test"));
        let stored = LocalIdentity {
            name: Some("Ada".into()),
            email: Some("ada@example.test".into()),
        };
        assert!(!can_save_identity(&stored, " Ada ", "ada@example.test"));
        assert!(can_save_identity(&stored, "Ada", "ada@work.test"));
    }

    #[test]
    fn release_comparison_never_invents_an_installed_version() {
        let latest = GhUpdateStatus::Latest((2, 100, 0));
        assert_eq!(
            gh_release_status(latest, &State::Detected("2.99.0".into())),
            tr!("git_accounts.update_available", version = "2.100.0")
        );
        assert_eq!(
            gh_release_status(latest, &State::Detected("2.100.0".into())),
            tr!("git_accounts.update_current", version = "2.100.0")
        );
        assert_eq!(
            gh_release_status(latest, &State::Unavailable),
            tr!("git_accounts.update_latest", version = "2.100.0")
        );
        assert_eq!(
            gh_release_status(latest, &State::Detected("2.101.0".into())),
            tr!("git_accounts.update_latest", version = "2.100.0")
        );
        assert_eq!(
            gh_release_status(GhUpdateStatus::Failed, &State::Unavailable),
            tr!("git_accounts.update_failed")
        );
    }
}
