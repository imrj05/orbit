//! Settings → Git: the machine's accounts and the commit identity.
//!
//! The page leads with what the user actually came for — the accounts that can
//! authenticate Git on this machine:
//!
//! - **SSH keys** discovered under `~/.ssh` (name, type, fingerprint, agent
//!   status, and the hosts `~/.ssh/config` binds them to).
//! - **GitHub accounts** from the `gh` CLI, with per-account switch / sign-out
//!   and an add-account flow (a token piped to `gh` and forgotten; a browser
//!   fallback in the terminal). Orbit never stores a token (D7).
//!
//! Below that, **Commit identity** (the global `git config --global` name and
//! email) and **This repository** (an optional `git config --local` override)
//! are kept distinct, because an author identity is not an account.

use gpui::AnyElement;

use super::helpers::*;
use super::*;
use crate::git_account;
use crate::ssh_keys;
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, Radius, RaisedExt, TextSize};

/// The visual weight of a Git action button.
#[derive(Clone, Copy)]
enum GitButtonStyle {
    Primary,
    Ghost,
    Danger,
}

impl OrbitApp {
    /// The rows for Settings → Git, accounts first.
    pub(super) fn git_settings_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = vec![
            self.git_accounts_section(theme, this.clone(), cx),
            self.git_account_section(theme, this.clone(), cx),
            self.git_identity_section(theme, this.clone()),
        ];
        // A repository-local override only makes sense inside a Git repo.
        if self.git_repo_root.is_some() {
            rows.push(self.git_repo_section(theme, this));
        }
        rows
    }

    // ── Saved Git accounts ─────────────────────────────────────────────

    /// The accounts the user saved: each binds a repository to one SSH key and
    /// commit identity. The only place `~/.ssh` keys are shown is the add/edit
    /// form's picker — never as a standing list.
    fn git_accounts_section(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        let in_repo = self.git_repo_root.is_some();
        let accounts = &self.git_accounts_config.accounts;

        if accounts.is_empty() && self.git_account_form.is_none() {
            rows.push(self.setting_row(
                theme,
                &tr!("git_settings.git_accounts_empty"),
                Some(&tr!("git_settings.git_accounts_empty_hint")),
                None,
                None,
            ));
        }

        for (ix, account) in accounts.iter().enumerate() {
            let key_name = self.git_key_name(&account.ssh_key);
            let mut details = Vec::new();
            if !account.host.trim().is_empty() {
                details.push(account.host.clone());
            }
            if !key_name.is_empty() {
                details.push(key_name);
            }
            if !account.email.trim().is_empty() {
                details.push(account.email.clone());
            }
            let desc = details.join(" · ");
            let mut control = div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme));
            if in_repo {
                control = control.child(self.git_button(
                    ElementId::Name(format!("git-account-use-{ix}").into()),
                    tr!("git_settings.git_account_use"),
                    GitButtonStyle::Primary,
                    theme,
                    this.clone(),
                    move |app, cx| app.git_account_use(ix, cx),
                ));
            }
            control = control
                .child(self.git_button(
                    ElementId::Name(format!("git-account-edit-{ix}").into()),
                    tr!("git_settings.git_account_edit"),
                    GitButtonStyle::Ghost,
                    theme,
                    this.clone(),
                    move |app, cx| app.git_account_edit(ix, cx),
                ))
                .child(self.git_button(
                    ElementId::Name(format!("git-account-remove-{ix}").into()),
                    tr!("git_settings.git_account_remove"),
                    GitButtonStyle::Danger,
                    theme,
                    this.clone(),
                    move |app, cx| app.git_account_remove(ix, cx),
                ));
            rows.push(self.setting_row(
                theme,
                &account.label,
                Some(&desc),
                None,
                Some(control.into_any_element()),
            ));
        }

        if let Some(form) = &self.git_account_form {
            rows.extend(self.git_account_form_rows(form, theme, this.clone()));
        } else {
            let hint = if in_repo {
                tr!("git_settings.git_accounts_add_hint")
            } else {
                tr!("git_settings.git_accounts_no_repo")
            };
            rows.push(self.setting_row(
                theme,
                &tr!("git_settings.git_accounts_add"),
                Some(&hint),
                None,
                Some(self.git_button(
                    "git-account-add",
                    tr!("git_settings.git_account_add_button"),
                    GitButtonStyle::Ghost,
                    theme,
                    this,
                    |app, cx| app.git_account_add_start(cx),
                )),
            ));
        }

        self.settings_section_desc(
            theme,
            &tr!("git_settings.git_accounts_section"),
            Some(&tr!("git_settings.git_accounts_section_desc")),
            rows,
        )
    }

    /// The add/edit form rows, including the SSH-key picker (the only place
    /// `~/.ssh` keys appear).
    fn git_account_form_rows(
        &self,
        form: &GitAccountForm,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Vec<AnyElement> {
        let host_placeholder = tr!("git_settings.git_account_host_placeholder");
        let mut rows = vec![
            self.setting_row(
                theme,
                &tr!("git_settings.git_account_label"),
                None,
                None,
                Some(self.git_field(&form.label, &theme)),
            ),
            self.setting_row(
                theme,
                &tr!("git_settings.git_account_host"),
                Some(&host_placeholder),
                None,
                Some(self.git_field(&form.host, &theme)),
            ),
            self.setting_row(
                theme,
                &tr!("git_settings.name"),
                None,
                None,
                Some(self.git_field(&form.name, &theme)),
            ),
            self.setting_row(
                theme,
                &tr!("git_settings.email"),
                None,
                None,
                Some(self.git_field(&form.email, &theme)),
            ),
        ];
        let selected = form.key_ix;
        rows.push(git_key_picker(
            theme,
            this.clone(),
            &self.git_ssh_keys,
            selected,
        ));
        rows.push(
            div()
                .w_full()
                .px(DynamicSpacing::Base16.px(&theme))
                .py(DynamicSpacing::Base12.px(&theme))
                .flex()
                .items_center()
                .justify_end()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(self.git_button(
                    "git-account-cancel",
                    tr!("git_settings.git_account_cancel"),
                    GitButtonStyle::Ghost,
                    theme,
                    this.clone(),
                    |app, cx| app.git_account_cancel(cx),
                ))
                .child(self.git_button(
                    "git-account-save",
                    tr!("git_settings.git_account_save"),
                    GitButtonStyle::Primary,
                    theme,
                    this,
                    |app, cx| app.git_account_save(cx),
                ))
                .into_any_element(),
        );
        rows
    }

    /// The file stem of a stored key path, for the account row's detail line.
    fn git_key_name(&self, ssh_key: &str) -> String {
        if ssh_key.trim().is_empty() {
            return String::new();
        }
        Path::new(ssh_key)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    // ── Commit identity ────────────────────────────────────────────────

    /// The global commit identity: the name and email on new commits.
    fn git_identity_section(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        self.settings_section_desc(
            theme,
            &tr!("git_settings.identity_section"),
            Some(&tr!("git_settings.identity_section_desc")),
            vec![
                self.setting_row(
                    theme,
                    &tr!("git_settings.name"),
                    None,
                    None,
                    Some(self.git_field(&self.git_name_input, &theme)),
                ),
                self.setting_row(
                    theme,
                    &tr!("git_settings.email"),
                    None,
                    None,
                    Some(self.git_field(&self.git_email_input, &theme)),
                ),
                self.setting_row(
                    theme,
                    &tr!("git_settings.save_identity"),
                    Some(&tr!("git_settings.save_identity_hint")),
                    None,
                    Some(self.git_button(
                        "git-save-identity",
                        tr!("git_settings.save"),
                        GitButtonStyle::Primary,
                        theme,
                        this,
                        |app, cx| app.git_save_identity(cx),
                    )),
                ),
            ],
        )
    }

    /// The repository-local identity override (`git config --local`).
    fn git_repo_section(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let state = if self.git_repo_identity_set {
            tr!("git_settings.repo_overridden")
        } else {
            tr!("git_settings.repo_inherited")
        };
        self.settings_section_desc(
            theme,
            &tr!("git_settings.repo_section"),
            Some(&tr!("git_settings.repo_section_desc")),
            vec![
                self.setting_row(
                    theme,
                    &tr!("git_settings.name"),
                    None,
                    None,
                    Some(self.git_field(&self.git_repo_name_input, &theme)),
                ),
                self.setting_row(
                    theme,
                    &tr!("git_settings.email"),
                    None,
                    None,
                    Some(self.git_field(&self.git_repo_email_input, &theme)),
                ),
                self.setting_row(
                    theme,
                    &tr!("git_settings.repo_override"),
                    Some(&state),
                    None,
                    Some(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base08.px(&theme))
                            .child(self.git_button(
                                "git-repo-save",
                                tr!("git_settings.save"),
                                GitButtonStyle::Primary,
                                theme,
                                this.clone(),
                                |app, cx| app.git_save_repo_identity(cx),
                            ))
                            .child(self.git_button(
                                "git-repo-clear",
                                tr!("git_settings.use_global"),
                                GitButtonStyle::Ghost,
                                theme,
                                this,
                                |app, cx| app.git_clear_repo_identity(cx),
                            ))
                            .into_any_element(),
                    ),
                ),
            ],
        )
    }

    // ── GitHub accounts (gh) ───────────────────────────────────────────

    /// The GitHub accounts `gh` is signed into, plus the add-account flow.
    fn git_account_section(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();

        let status = if self.git_settings_busy {
            spinner(
                "git-account-spin",
                IconSize::Small.px(&theme),
                theme.text_3,
                theme,
            )
        } else {
            self.git_status_chip(theme)
        };
        rows.push(self.setting_row(
            theme,
            &tr!("git_settings.current_account"),
            Some(&tr!("git_settings.current_account_hint")),
            None,
            Some(status),
        ));

        for account in &self.git_accounts {
            let login = account.login.clone();
            let host = account.host.clone();
            let mut control = div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme));
            if account.active {
                control = control.child(git_active_badge(theme));
            } else if self.git_switch_supported {
                let switch_login = login.clone();
                let switch_host = host.clone();
                control = control.child(self.git_button(
                    ElementId::Name(format!("git-switch-{login}").into()),
                    tr!("git_settings.use_account"),
                    GitButtonStyle::Ghost,
                    theme,
                    this.clone(),
                    move |app, cx| {
                        app.git_switch_account(switch_login.clone(), switch_host.clone(), cx)
                    },
                ));
            }
            let out_login = login.clone();
            let out_host = host.clone();
            control = control.child(self.git_button(
                ElementId::Name(format!("git-signout-{login}").into()),
                tr!("git_settings.sign_out"),
                GitButtonStyle::Danger,
                theme,
                this.clone(),
                move |app, cx| app.git_sign_out(out_login.clone(), out_host.clone(), cx),
            ));
            rows.push(self.setting_row(
                theme,
                &login,
                Some(&host),
                None,
                Some(control.into_any_element()),
            ));
        }

        rows.push(self.setting_row(
            theme,
            &tr!("git_settings.host"),
            Some(&tr!("git_settings.host_hint")),
            None,
            Some(self.git_field(&self.git_host_input, &theme)),
        ));
        rows.push(
            self.setting_row(
                theme,
                &tr!("git_settings.token"),
                Some(&tr!("git_settings.add_account_hint")),
                None,
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(self.git_field(&self.git_token_input, &theme))
                        .child(self.git_button(
                            "git-sign-in-token",
                            tr!("git_settings.add_account"),
                            GitButtonStyle::Primary,
                            theme,
                            this.clone(),
                            |app, cx| app.git_sign_in_with_token(cx),
                        ))
                        .into_any_element(),
                ),
            ),
        );
        rows.push(self.setting_row(
            theme,
            &tr!("git_settings.browser_sign_in"),
            Some(&tr!("git_settings.browser_sign_in_hint")),
            None,
            Some(self.git_button(
                "git-sign-in-browser",
                tr!("git_settings.copy_command"),
                GitButtonStyle::Ghost,
                theme,
                this.clone(),
                |app, cx| app.git_copy_login_command(cx),
            )),
        ));

        if !self.git_gh_installed {
            rows.push(self.setting_row(
                theme,
                &tr!("git_settings.gh_missing"),
                Some(&tr!("git_settings.gh_missing_hint")),
                None,
                None,
            ));
        }
        if let Some(error) = &self.git_settings_error {
            rows.push(self.setting_row(theme, &tr!("git_settings.error"), Some(error), None, None));
        }

        self.settings_section_desc(
            theme,
            &tr!("git_settings.account_section"),
            Some(&tr!("git_settings.account_section_desc")),
            rows,
        )
    }

    // ── controller ─────────────────────────────────────────────────────

    /// Load the SSH keys, `gh` accounts, and identities when the page opens.
    /// Everything runs off the UI thread; the page paints cached state first.
    pub(super) fn git_settings_section_opened(&mut self, cx: &mut Context<Self>) {
        self.git_settings_error = None;
        self.git_probe_accounts(cx);
        self.git_probe_ssh_keys(cx);
        self.git_probe_identity(cx);
    }

    /// Open the add form with blank fields and the global identity prefilled.
    pub(super) fn git_account_add_start(&mut self, cx: &mut Context<Self>) {
        let identity = self.git_identity.clone();
        let form = GitAccountForm {
            edit_index: None,
            label: git_form_input(
                cx,
                "git-account-label",
                "",
                "git_settings.git_account_label_placeholder",
            ),
            host: git_form_input(
                cx,
                "git-account-host",
                "github.com",
                "git_settings.git_account_host_placeholder",
            ),
            name: git_form_input(
                cx,
                "git-account-name",
                &identity.name,
                "git_settings.name_placeholder",
            ),
            email: git_form_input(
                cx,
                "git-account-email",
                &identity.email,
                "git_settings.email_placeholder",
            ),
            key_ix: None,
        };
        self.git_account_form = Some(form);
        cx.notify();
    }

    /// Open the edit form for a saved account, preselecting its key.
    pub(super) fn git_account_edit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(account) = self.git_accounts_config.accounts.get(index).cloned() else {
            return;
        };
        let key_ix = self
            .git_ssh_keys
            .iter()
            .position(|key| key.private_path.to_string_lossy() == account.ssh_key);
        let form = GitAccountForm {
            edit_index: Some(index),
            label: git_form_input(
                cx,
                "git-account-label",
                &account.label,
                "git_settings.git_account_label_placeholder",
            ),
            host: git_form_input(
                cx,
                "git-account-host",
                &account.host,
                "git_settings.git_account_host_placeholder",
            ),
            name: git_form_input(
                cx,
                "git-account-name",
                &account.name,
                "git_settings.name_placeholder",
            ),
            email: git_form_input(
                cx,
                "git-account-email",
                &account.email,
                "git_settings.email_placeholder",
            ),
            key_ix,
        };
        self.git_account_form = Some(form);
        cx.notify();
    }

    /// Select (or clear) the SSH key in the open form.
    pub(super) fn git_account_pick_key(&mut self, key_ix: Option<usize>, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.git_account_form {
            form.key_ix = key_ix;
        }
        cx.notify();
    }

    /// Close the form without saving.
    pub(super) fn git_account_cancel(&mut self, cx: &mut Context<Self>) {
        self.git_account_form = None;
        cx.notify();
    }

    /// Save the open form as a new account or over the edited one.
    pub(super) fn git_account_save(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &self.git_account_form else {
            return;
        };
        let label = form.label.read(cx).text().trim().to_string();
        if label.is_empty() {
            self.git_settings_error = Some(tr!("git_settings.git_account_label_required"));
            cx.notify();
            return;
        }
        let account = git_account::GitAccount {
            label,
            host: form.host.read(cx).text().trim().to_string(),
            ssh_key: form
                .key_ix
                .and_then(|ix| self.git_ssh_keys.get(ix))
                .map(|key| key.private_path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            name: form.name.read(cx).text().trim().to_string(),
            email: form.email.read(cx).text().trim().to_string(),
        };
        match form.edit_index {
            Some(index) if index < self.git_accounts_config.accounts.len() => {
                self.git_accounts_config.accounts[index] = account.clone();
            }
            _ => self.git_accounts_config.accounts.push(account.clone()),
        }
        let label = account.label.clone();
        if let Err(err) = self.git_accounts_config.persist() {
            self.git_settings_error = Some(err.to_string());
        } else {
            self.git_settings_error = None;
            self.toast_success(tr!("git_settings.git_account_saved", label = label));
        }
        self.git_account_form = None;
        cx.notify();
    }

    /// Delete a saved account.
    pub(super) fn git_account_remove(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.git_accounts_config.accounts.len() {
            return;
        }
        let label = self.git_accounts_config.accounts[index].label.clone();
        self.git_accounts_config.accounts.remove(index);
        let _ = self.git_accounts_config.persist();
        self.toast_success(tr!("git_settings.git_account_removed", label = label));
        cx.notify();
    }

    /// Apply a saved account to the active repository (`git config --local`).
    pub(super) fn git_account_use(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(repo) = self.git_repo_root.clone() else {
            self.git_settings_error = Some(tr!("git_settings.git_accounts_no_repo"));
            cx.notify();
            return;
        };
        let Some(account) = self.git_accounts_config.accounts.get(index).cloned() else {
            return;
        };
        let label = account.label.clone();
        self.git_settings_op(
            cx,
            move || {
                git_account::apply_to_repo(&repo, &account)?;
                Ok(tr!("git_settings.git_account_applied", label = label))
            },
            |app, result, cx| {
                if let Ok(message) = result {
                    app.toast_success(message);
                    app.git_probe_identity(cx);
                }
                cx.notify();
            },
        );
    }

    /// Write the identity fields to `git config --global`.
    pub(super) fn git_save_identity(&mut self, cx: &mut Context<Self>) {
        let identity = git_account::GitIdentity {
            name: self.git_name_input.read(cx).text().trim().to_string(),
            email: self.git_email_input.read(cx).text().trim().to_string(),
        };
        self.git_identity = identity.clone();
        self.git_settings_op(
            cx,
            move || {
                git_account::write_identity(&identity)?;
                Ok(tr!("git_settings.identity_saved"))
            },
            |app, result, cx| {
                if let Ok(message) = result {
                    app.toast_success(message);
                }
                cx.notify();
            },
        );
    }

    /// Pipe the token field to `gh auth login --with-token`, then clear it.
    /// The host field targets GitHub Enterprise; an empty host means
    /// `github.com`.
    pub(super) fn git_sign_in_with_token(&mut self, cx: &mut Context<Self>) {
        let token = self.git_token_input.read(cx).text().trim().to_string();
        if token.is_empty() {
            self.git_settings_error = Some(tr!("git_settings.token_required"));
            cx.notify();
            return;
        }
        let host = self.git_host_input.read(cx).text().trim().to_string();
        let host = if host.is_empty() {
            "github.com".to_string()
        } else {
            host
        };
        self.git_settings_op(
            cx,
            move || git_account::login_with_token(&token, &host),
            |app, result, cx| {
                if let Ok(message) = result {
                    app.git_token_input.update(cx, |input, cx| {
                        input.set_text(String::new(), cx);
                    });
                    app.toast_success(message);
                    app.git_probe_accounts(cx);
                }
                cx.notify();
            },
        );
    }

    /// The browser flow is interactive, so hand the terminal the exact command
    /// (the same honest fallback the Git page's Re-authenticate uses).
    pub(super) fn git_copy_login_command(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string("gh auth login".into()));
        self.toast_success(tr!("git_settings.login_copied"));
        cx.notify();
    }

    /// Switch `gh`'s active account.
    pub(super) fn git_switch_account(
        &mut self,
        login: String,
        host: String,
        cx: &mut Context<Self>,
    ) {
        self.git_settings_op(
            cx,
            move || git_account::switch_account(&login, &host),
            |app, result, cx| {
                if let Ok(message) = result {
                    app.toast_success(message);
                    app.git_probe_accounts(cx);
                }
                cx.notify();
            },
        );
    }

    /// Remove an account from `gh`'s credential store.
    pub(super) fn git_sign_out(&mut self, login: String, host: String, cx: &mut Context<Self>) {
        self.git_settings_op(
            cx,
            move || git_account::sign_out(&login, &host),
            |app, result, cx| {
                if let Ok(message) = result {
                    app.toast_success(message);
                    app.git_probe_accounts(cx);
                }
                cx.notify();
            },
        );
    }

    /// Write the repository-local identity override.
    pub(super) fn git_save_repo_identity(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.git_repo_root.clone() else {
            return;
        };
        let identity = git_account::GitIdentity {
            name: self.git_repo_name_input.read(cx).text().trim().to_string(),
            email: self.git_repo_email_input.read(cx).text().trim().to_string(),
        };
        self.git_settings_op(
            cx,
            move || {
                git_account::write_local_identity(&repo, &identity)?;
                Ok(tr!("git_settings.repo_identity_saved"))
            },
            |app, result, cx| {
                if let Ok(message) = result {
                    app.toast_success(message);
                    app.git_probe_identity(cx);
                }
                cx.notify();
            },
        );
    }

    /// Drop the repository-local override so the global identity applies.
    pub(super) fn git_clear_repo_identity(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.git_repo_root.clone() else {
            return;
        };
        self.git_settings_op(
            cx,
            move || {
                git_account::clear_local_identity(&repo)?;
                Ok(tr!("git_settings.repo_identity_cleared"))
            },
            |app, result, cx| {
                if let Ok(message) = result {
                    app.toast_success(message);
                    app.git_probe_identity(cx);
                }
                cx.notify();
            },
        );
    }

    // ── probes ─────────────────────────────────────────────────────────

    /// Re-read `gh`'s installed state and accounts.
    fn git_probe_accounts(&mut self, cx: &mut Context<Self>) {
        self.spawn_git(
            cx,
            || {
                let status = crate::gh::status(std::path::Path::new("."));
                let accounts = if status.installed {
                    git_account::list_accounts()
                } else {
                    Vec::new()
                };
                let switch = status.installed && git_account::switch_supported();
                Ok((status.installed, accounts, switch))
            },
            |app, result, cx| {
                if let Ok((installed, accounts, switch)) = result {
                    app.git_gh_installed = installed;
                    app.git_accounts = accounts;
                    app.git_switch_supported = switch;
                }
                cx.notify();
            },
        );
    }

    /// Re-scan `~/.ssh` for keys and agent status.
    fn git_probe_ssh_keys(&mut self, cx: &mut Context<Self>) {
        self.spawn_git(
            cx,
            || Ok(ssh_keys::list_keys()),
            |app, result, cx| {
                if let Ok(keys) = result {
                    app.git_ssh_keys = keys;
                }
                cx.notify();
            },
        );
    }

    /// Re-read the global identity, discover whether the active workspace is a
    /// Git repository, and load its local override. The repository fields show
    /// the effective identity (local when set, otherwise the global default).
    fn git_probe_identity(&mut self, cx: &mut Context<Self>) {
        let repo = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        self.spawn_git(
            cx,
            move || {
                let global = git_account::read_identity();
                let (is_repo, local) = match repo.as_deref() {
                    Some(path) if crate::git::is_repo(path) => {
                        (true, git_account::read_local_identity(path))
                    }
                    _ => (false, git_account::GitIdentity::default()),
                };
                Ok((repo, is_repo, global, local))
            },
            |app, result, cx| {
                if let Ok((repo, is_repo, global, local)) = result {
                    app.git_identity = global.clone();
                    app.git_name_input
                        .update(cx, |input, cx| input.set_text(global.name.clone(), cx));
                    app.git_email_input
                        .update(cx, |input, cx| input.set_text(global.email.clone(), cx));

                    app.git_repo_root = if is_repo { repo } else { None };
                    let overridden =
                        !local.name.trim().is_empty() || !local.email.trim().is_empty();
                    app.git_repo_identity_set = overridden;
                    let effective = if overridden { local } else { global };
                    app.git_repo_name_input
                        .update(cx, |input, cx| input.set_text(effective.name.clone(), cx));
                    app.git_repo_email_input
                        .update(cx, |input, cx| input.set_text(effective.email.clone(), cx));
                }
                cx.notify();
            },
        );
    }

    // ── plumbing ───────────────────────────────────────────────────────

    /// Run one Git settings mutation with the busy flag and error capture.
    fn git_settings_op<T, W, A>(&mut self, cx: &mut Context<Self>, work: W, apply: A)
    where
        T: Send + 'static,
        W: FnOnce() -> Result<T, String> + Send + 'static,
        A: FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + Send + 'static,
    {
        self.git_settings_busy = true;
        self.git_settings_error = None;
        cx.notify();
        self.spawn_git(cx, work, move |app, result, cx| {
            app.git_settings_busy = false;
            if let Err(error) = &result {
                app.git_settings_error = Some(error.clone());
            }
            apply(app, result, cx);
            cx.notify();
        });
    }

    /// Spawn a blocking Git settings read/write on the background executor.
    fn spawn_git<T, W, A>(&self, cx: &mut Context<Self>, work: W, apply: A)
    where
        T: Send + 'static,
        W: FnOnce() -> Result<T, String> + Send + 'static,
        A: FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + Send + 'static,
    {
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { work() }).await;
            let _ = this.update(cx, |app, cx| apply(app, result, cx));
        })
        .detach();
    }

    /// The current-account chip: a status dot and the active login.
    fn git_status_chip(&self, theme: Theme) -> AnyElement {
        let active = self
            .git_accounts
            .iter()
            .find(|account| account.active)
            .or_else(|| self.git_accounts.first());
        let (color, label) = match active {
            Some(account) if account.active => (
                theme.ok_green,
                tr!("git_settings.signed_in_as", name = account.login.clone()),
            ),
            Some(account) => (
                theme.text_3,
                tr!("git_settings.signed_in_as", name = account.login.clone()),
            ),
            None => (theme.text_3, tr!("git_settings.not_signed_in")),
        };
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(div().size(px(7.)).rounded_full().bg(color))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_2)
                    .child(label),
            )
            .into_any_element()
    }

    /// A fixed-width field so the Git rows share one right-aligned axis.
    fn git_field(&self, input: &Entity<ComposerInput>, theme: &Theme) -> AnyElement {
        input_field_frame(div(), theme)
            .w(px(220.))
            .bg(theme.bg_main)
            .child(input.clone())
            .into_any_element()
    }

    /// A Git action button, styled like the Worktrees dialog controls.
    fn git_button<F>(
        &self,
        id: impl Into<ElementId>,
        label: String,
        style: GitButtonStyle,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: F,
    ) -> AnyElement
    where
        F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    {
        let mut button = button_frame(div().id(id.into()), &theme, ButtonSize::Medium)
            .group(BUTTON_GROUP)
            .cursor_pointer()
            .font_weight(FontWeight::MEDIUM);
        button = match style {
            GitButtonStyle::Primary => button
                .border_1()
                .border_color(theme.accent.opacity(0.5))
                .raised(theme.accent.opacity(0.14), &theme)
                .text_color(theme.accent)
                .hover(|style| style.raised(theme.accent.opacity(0.2), &theme)),
            GitButtonStyle::Ghost => button
                .border_1()
                .border_color(theme.border)
                .raised(theme.bg_raised, &theme)
                .text_color(theme.text_2)
                .hover(|style| style.raised(theme.bg_hover, &theme)),
            GitButtonStyle::Danger => button
                .border_1()
                .border_color(theme.border)
                .raised(theme.bg_raised, &theme)
                .text_color(theme.crit)
                .hover(|style| {
                    style
                        .border_color(theme.crit.opacity(0.6))
                        .raised(theme.crit.opacity(0.08), &theme)
                }),
        };
        press(button)
            .child(label)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| action(app, cx));
            })
            .into_any_element()
    }
}

/// A text field for the Git-account form (created when the form opens).
fn git_form_input(
    cx: &mut Context<OrbitApp>,
    id: &'static str,
    text: &str,
    placeholder_key: &'static str,
) -> Entity<ComposerInput> {
    cx.new(|cx| {
        let mut input = ComposerInput::new(cx)
            .with_element_id(id)
            .with_placeholder_key(placeholder_key)
            .with_key_context("Composer Picker")
            .with_max_lines(1)
            .with_wrap(false);
        if !text.is_empty() {
            input = input.with_text(text.to_string());
        }
        input
    })
}

/// The SSH-key picker inside the add/edit form. This is the only surface that
/// lists `~/.ssh` keys, and it exists only while the form is open.
fn git_key_picker(
    theme: Theme,
    this: Entity<OrbitApp>,
    keys: &[crate::ssh_keys::SshKey],
    selected: Option<usize>,
) -> AnyElement {
    let mut list = div()
        .w_full()
        .flex()
        .flex_col()
        .border_1()
        .border_color(theme.border)
        .rounded(Radius::Large.px(&theme))
        .overflow_hidden()
        .child(git_key_option(
            theme,
            this.clone(),
            None,
            selected.is_none(),
            tr!("git_settings.git_account_key_none"),
            String::new(),
            false,
        ));
    if keys.is_empty() {
        list = list.child(
            div()
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_3)
                .child(tr!("git_settings.git_account_key_empty")),
        );
    }
    for (ix, key) in keys.iter().enumerate() {
        let title = if key.comment.trim().is_empty() {
            key.name.clone()
        } else {
            key.comment.clone()
        };
        let mut detail = vec![key.name.clone(), key.key_type.clone()];
        if let Some(fingerprint) = &key.fingerprint {
            detail.push(short_fingerprint(fingerprint));
        }
        list = list.child(git_key_option(
            theme,
            this.clone(),
            Some(ix),
            selected == Some(ix),
            title,
            detail.join(" · "),
            key.loaded,
        ));
    }
    div()
        .w_full()
        .px(DynamicSpacing::Base16.px(&theme))
        .py(DynamicSpacing::Base12.px(&theme))
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_2)
                .child(tr!("git_settings.git_account_key")),
        )
        .child(
            div()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_3)
                .child(tr!("git_settings.git_account_key_hint")),
        )
        .child(list)
        .into_any_element()
}

/// One selectable row in the key picker.
fn git_key_option(
    theme: Theme,
    this: Entity<OrbitApp>,
    key_ix: Option<usize>,
    selected: bool,
    title: String,
    subtitle: String,
    loaded: bool,
) -> AnyElement {
    let id = key_ix.map_or_else(|| "none".to_string(), |ix| ix.to_string());
    let indicator = if selected {
        icon(
            "icons/circle-check.svg",
            IconSize::Small.px(&theme),
            theme.accent,
        )
        .into_any_element()
    } else {
        div()
            .size(IconSize::Small.px(&theme))
            .rounded_full()
            .border_1()
            .border_color(theme.text_3)
            .into_any_element()
    };
    let mut row = div()
        .id(ElementId::Name(format!("git-key-option-{id}").into()))
        .w_full()
        .px(DynamicSpacing::Base12.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.overlay))
        .hover(|row| row.bg(theme.bg_hover))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| app.git_account_pick_key(key_ix, cx));
        })
        .child(indicator)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().truncate().text_color(theme.text_2).child(title))
                .children((!subtitle.is_empty()).then(|| {
                    div()
                        .truncate()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_3)
                        .child(subtitle)
                })),
        );
    if loaded {
        row = row.child(git_agent_chip(theme, true));
    }
    row.into_any_element()
}

/// The active-account marker on an account row.
fn git_active_badge(theme: Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme))
        .child(icon(
            "icons/check.svg",
            IconSize::XSmall.px(&theme),
            theme.ok_green,
        ))
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_3)
                .child(tr!("git_settings.active")),
        )
        .into_any_element()
}

/// A fingerprint trimmed to `SHA256:xxxxxxxxxxxx…` so a row stays scannable;
/// the full value is still available from `ssh-keygen -lf`.
fn short_fingerprint(fingerprint: &str) -> String {
    let tail = fingerprint.strip_prefix("SHA256:").unwrap_or(fingerprint);
    let head: String = tail.chars().take(12).collect();
    format!("SHA256:{head}…")
}

/// Whether an SSH key is loaded in the agent: a status dot and label.
fn git_agent_chip(theme: Theme, loaded: bool) -> AnyElement {
    let (color, label) = if loaded {
        (theme.ok_green, tr!("git_settings.ssh_in_agent"))
    } else {
        (theme.text_3, tr!("git_settings.ssh_not_loaded"))
    };
    div()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .child(div().size(px(7.)).rounded_full().bg(color))
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_3)
                .child(label),
        )
        .into_any_element()
}
