//! Settings presentation and background lifecycle for local Git diagnostics.
use super::helpers::*;
use super::*;
use crate::git_accounts::{self, State};
use crate::theme::tokens::{ButtonSize, RaisedExt, TextSize};

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
                    app.git_accounts_snapshot = Some(snapshot);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn git_accounts_toolbar(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        button_frame(div().id("git-accounts-refresh"), &theme, ButtonSize::Large)
            .group(BUTTON_GROUP)
            .border_1()
            .border_color(theme.border)
            .raised(theme.surface_elevated, &theme)
            .when(self.git_accounts_loading, |button| button.opacity(0.6))
            .when(!self.git_accounts_loading, |button| {
                button
                    .cursor_pointer()
                    .hover(|s| s.raised(theme.surface_hover, &theme))
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        this.update(cx, |app, cx| {
                            if !app.git_accounts_loading {
                                app.refresh_git_accounts_for_workspace(cx);
                            }
                        });
                    })
            })
            .child(refresh_glyph(
                "git-accounts-refresh-spin",
                ButtonSize::Large.icon_size().px(&theme),
                self.git_accounts_loading,
                theme.text_secondary,
                theme,
            ))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(tr!("git_accounts.refresh")),
            )
            .into_any_element()
    }

    pub(super) fn git_accounts_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
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
        let check_button = button_frame(div().id("gh-check-updates"), &theme, ButtonSize::Medium)
            .border_1()
            .border_color(theme.border)
            .raised(theme.surface_elevated, &theme)
            .when(checking, |button| button.opacity(0.6))
            .when(!checking, |button| {
                button
                    .cursor_pointer()
                    .hover(|s| s.raised(theme.surface_hover, &theme))
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        this.update(cx, |app, cx| app.check_gh_release(cx));
                    })
            })
            .text_color(theme.text_secondary)
            .child(tr!("git_accounts.update_check"))
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
