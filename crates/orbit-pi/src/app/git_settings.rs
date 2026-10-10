//! Settings → Git: saved identities and the accounts that back them.
//!
//! The page leads with **Identities** — named commit identities (`user.name` /
//! `user.email`) plus how each authenticates: whatever the machine already
//! uses, a signed-in `gh` account, a specific `~/.ssh` key, or nothing. A
//! read-only **System identity** row shows the global default. The editor modal
//! is the only surface that lists `~/.ssh` keys.
//!
//! Below that, **GitHub accounts (gh)** carries API/token auth (Orbit never
//! stores a token, D7) and **Commit identity** / **This repository** carry the
//! raw global and local overrides.

use gpui::AnyElement;

use super::helpers::*;
use super::*;
use crate::git_account;
use crate::ssh_keys;
use crate::theme::tokens::{
    modal, ButtonSize, DynamicSpacing, IconSize, Radius, RaisedExt, TextSize,
};

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

    // ── Identities ─────────────────────────────────────────────────────

    /// The saved identities, with a read-only System identity first. The only
    /// place `~/.ssh` keys appear is the editor's picker.
    fn git_accounts_section(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        let in_repo = self.git_repo_root.is_some();
        let applied_email = if self.git_repo_identity_set {
            self.git_repo_email_input.read(cx).text().trim().to_string()
        } else {
            String::new()
        };

        rows.push(git_system_identity_row(
            theme,
            &self.git_identity.name,
            &self.git_identity.email,
        ));

        if self.git_accounts_config.accounts.is_empty() {
            rows.push(
                div()
                    .w_full()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base12.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_2)
                            .child(tr!("git_settings.git_accounts_empty")),
                    )
                    .child(
                        div()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_3)
                            .child(tr!("git_settings.git_accounts_empty_hint")),
                    )
                    .into_any_element(),
            );
        }

        for (ix, account) in self.git_accounts_config.accounts.iter().enumerate() {
            let applied =
                in_repo && !applied_email.is_empty() && account.email.trim() == applied_email;
            rows.push(self.git_identity_row(theme, this.clone(), account, ix, applied, in_repo));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .px(DynamicSpacing::Base04.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(DynamicSpacing::Base08.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Default.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(tr!("git_settings.git_accounts_section")),
                            )
                            .child(self.git_button(
                                "git-identity-new",
                                tr!("git_settings.identity_new_button"),
                                GitButtonStyle::Ghost,
                                theme,
                                this,
                                |app, cx| app.git_account_add_start(cx),
                            )),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_3)
                            .child(tr!("git_settings.git_accounts_section_desc")),
                    ),
            )
            .child(self.settings_group(theme, rows))
            .into_any_element()
    }

    /// One saved identity row: color/icon tile, label, email, and actions.
    fn git_identity_row(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        account: &git_account::GitAccount,
        index: usize,
        applied: bool,
        in_repo: bool,
    ) -> AnyElement {
        let color: Hsla = gpui::rgb(git_account::color_value(&account.color)).into();
        let icon_path = git_account::icon_path(&account.icon);
        let mut control = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme));
        if applied {
            control = control.child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .child(icon(
                        "icons/circle-check.svg",
                        IconSize::XSmall.px(&theme),
                        theme.ok_green,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_3)
                            .child(tr!("git_settings.identity_applied")),
                    ),
            );
        }
        control = control
            .child(self.git_button(
                ElementId::Name(format!("git-identity-edit-{index}").into()),
                tr!("git_settings.git_account_edit"),
                GitButtonStyle::Ghost,
                theme,
                this.clone(),
                move |app, cx| app.git_account_edit(index, cx),
            ))
            .child(self.git_button(
                ElementId::Name(format!("git-identity-remove-{index}").into()),
                tr!("git_settings.git_account_remove"),
                GitButtonStyle::Danger,
                theme,
                this.clone(),
                move |app, cx| app.git_account_remove(index, cx),
            ));

        let row = div()
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .when(in_repo, |row| {
                row.cursor_pointer().hover(|row| row.bg(theme.bg_hover))
            })
            .when(in_repo, |row| {
                row.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.git_account_use(index, cx));
                })
            })
            .child(
                div()
                    .flex_none()
                    .size(px(32.))
                    .rounded(Radius::Medium.px(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(color.opacity(0.18))
                    .child(icon(icon_path, IconSize::Small.px(&theme), color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base01.px(&theme))
                    .child(
                        div()
                            .truncate()
                            .text_size(TextSize::Default.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(account.label.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_3)
                            .child(if account.email.trim().is_empty() {
                                account.name.clone()
                            } else {
                                account.email.clone()
                            }),
                    ),
            )
            .child(control);
        row.into_any_element()
    }

    /// The body of the New/Edit Identity modal.
    fn git_identity_modal_body(
        &self,
        form: &GitAccountForm,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        use crate::git_account::AuthMethod;

        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base16.px(&theme))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_profile_name"),
                false,
                git_modal_input(&form.label, &theme),
            ))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_color"),
                false,
                git_color_swatches(theme, this.clone(), &form.color),
            ))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_icon"),
                false,
                git_icon_picker(theme, this.clone(), &form.icon),
            ))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_user_name"),
                true,
                git_modal_input(&form.name, &theme),
            ))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_email"),
                true,
                git_modal_input(&form.email, &theme),
            ))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_source_account"),
                false,
                git_source_account(
                    theme,
                    this.clone(),
                    &self.git_accounts,
                    &form.source_account,
                    self.git_account_source_open,
                ),
            ))
            .child(git_modal_field(
                theme,
                &tr!("git_settings.identity_auth_method"),
                false,
                git_auth_methods(theme, this.clone(), form.auth_method),
            ))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(git_auth_description(form.auth_method)),
            );

        if form.auth_method == AuthMethod::Ssh {
            body = body.child(git_modal_field(
                theme,
                &tr!("git_settings.git_account_key"),
                false,
                git_key_picker(theme, this.clone(), &self.git_ssh_keys, form.key_ix),
            ));
        }

        body = body.child(git_checkbox(
            theme,
            this.clone(),
            form.sign_commits,
            tr!("git_settings.identity_sign_commits"),
            |app, cx| app.git_account_toggle_sign(cx),
        ));
        if form.sign_commits {
            body = body.child(git_modal_field(
                theme,
                &tr!("git_settings.identity_signing_key"),
                false,
                git_modal_input(&form.signing_key, &theme),
            ));
        }

        if let Some(error) = &self.git_settings_error {
            body = body.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.crit)
                    .child(error.clone()),
            );
        }
        body.into_any_element()
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

    /// The New/Edit Identity modal, when the form is open.
    pub(super) fn git_identity_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.git_account_form.as_ref()?;
        let editing = form.edit_index.is_some();
        let title = if editing {
            tr!("git_settings.identity_edit_title")
        } else {
            tr!("git_settings.identity_new_title")
        };
        let confirm = if editing {
            tr!("git_settings.identity_save")
        } else {
            tr!("git_settings.identity_create")
        };

        let header = div()
            .px(modal::header_padding_x(&theme))
            .pt(modal::header_padding_top(&theme))
            .pb(modal::header_padding_bottom(&theme))
            .flex()
            .items_start()
            .justify_between()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_3)
                            .child(tr!("git_settings.identity_subtitle")),
                    ),
            )
            .child(
                div()
                    .id("git-identity-close")
                    .flex_none()
                    .size(px(24.))
                    .rounded(Radius::Medium.px(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_mouse_down(MouseButton::Left, {
                        let this = this.clone();
                        move |_, _, cx| this.update(cx, |app, cx| app.git_account_close(cx))
                    })
                    .child(icon(
                        "icons/x.svg",
                        IconSize::Small.px(&theme),
                        theme.text_3,
                    )),
            );

        let footer = div()
            .px(modal::footer_padding(&theme))
            .py(modal::footer_padding(&theme))
            .flex()
            .items_center()
            .justify_end()
            .gap(modal::footer_gap(&theme))
            .border_t_1()
            .border_color(theme.border)
            .child(self.git_button(
                "git-identity-cancel",
                tr!("git_settings.git_account_cancel"),
                GitButtonStyle::Ghost,
                theme,
                this.clone(),
                |app, cx| app.git_account_close(cx),
            ))
            .child(self.git_button(
                "git-identity-confirm",
                confirm,
                GitButtonStyle::Primary,
                theme,
                this.clone(),
                |app, cx| app.git_account_save(cx),
            ));

        let card = div()
            .w(px(560.))
            .max_h(px(660.))
            .bg(theme.bg_composer)
            .border_1()
            .border_color(theme.border)
            .rounded(Radius::XLarge.px(&theme))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .child(header)
            .child(
                div()
                    .id("git-identity-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(modal::header_padding_x(&theme))
                    .pb(modal::header_padding_bottom(&theme))
                    .child(self.git_identity_modal_body(form, theme, this.clone())),
            )
            .child(footer);

        let scrim = match theme.mode {
            ThemeMode::Dark => Hsla {
                h: 0.,
                s: 0.,
                l: 0.,
                a: 0.42,
            },
            ThemeMode::Light => Hsla {
                h: 0.,
                s: 0.,
                l: 0.,
                a: 0.22,
            },
        };
        Some(
            div()
                .id("git-identity-layer")
                .absolute()
                .inset_0()
                .occlude()
                .bg(scrim)
                .p(DynamicSpacing::Base24.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let this = this.clone();
                    this.update(cx, |app, cx| app.git_account_close(cx));
                })
                .child(card)
                .into_any_element(),
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

    /// Open the New Identity modal with blank fields and the global identity
    /// prefilled.
    pub(super) fn git_account_add_start(&mut self, cx: &mut Context<Self>) {
        let identity = self.git_identity.clone();
        let defaults = git_account::GitAccount::default();
        let form = GitAccountForm {
            edit_index: None,
            label: git_form_input(
                cx,
                "git-account-label",
                "",
                "git_settings.identity_profile_name_placeholder",
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
            signing_key: git_form_input(
                cx,
                "git-account-signing-key",
                "",
                "git_settings.identity_signing_key_placeholder",
            ),
            color: defaults.color,
            icon: defaults.icon,
            source_account: String::new(),
            auth_method: git_account::AuthMethod::Machine,
            sign_commits: false,
            key_ix: None,
        };
        self.git_settings_error = None;
        self.git_account_source_open = false;
        self.git_account_form = Some(form);
        cx.notify();
    }

    /// Open the edit modal for a saved identity, preselecting every field.
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
                "git_settings.identity_profile_name_placeholder",
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
            signing_key: git_form_input(
                cx,
                "git-account-signing-key",
                &account.signing_key,
                "git_settings.identity_signing_key_placeholder",
            ),
            color: account.color,
            icon: account.icon,
            source_account: account.source_account,
            auth_method: account.auth_method,
            sign_commits: account.sign_commits,
            key_ix,
        };
        self.git_settings_error = None;
        self.git_account_source_open = false;
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

    /// Pick the identity's color swatch.
    pub(super) fn git_account_pick_color(&mut self, color: String, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.git_account_form {
            form.color = color;
        }
        cx.notify();
    }

    /// Pick the identity's icon.
    pub(super) fn git_account_pick_icon(&mut self, icon: String, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.git_account_form {
            form.icon = icon;
        }
        cx.notify();
    }

    /// Pick the auth method.
    pub(super) fn git_account_pick_auth(
        &mut self,
        method: git_account::AuthMethod,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = &mut self.git_account_form {
            form.auth_method = method;
        }
        cx.notify();
    }

    /// Toggle commit signing in the open form.
    pub(super) fn git_account_toggle_sign(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.git_account_form {
            form.sign_commits = !form.sign_commits;
        }
        cx.notify();
    }

    /// Toggle the source-account dropdown.
    pub(super) fn git_account_toggle_source(&mut self, cx: &mut Context<Self>) {
        self.git_account_source_open = !self.git_account_source_open;
        cx.notify();
    }

    /// Pick (or clear) the source control account.
    pub(super) fn git_account_pick_source(&mut self, login: String, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.git_account_form {
            form.source_account = login;
        }
        self.git_account_source_open = false;
        cx.notify();
    }

    /// Close the form without saving.
    pub(super) fn git_account_close(&mut self, cx: &mut Context<Self>) {
        self.git_account_form = None;
        self.git_account_source_open = false;
        self.git_settings_error = None;
        cx.notify();
    }

    /// Save the open form as a new identity or over the edited one.
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
            color: form.color.clone(),
            icon: form.icon.clone(),
            name: form.name.read(cx).text().trim().to_string(),
            email: form.email.read(cx).text().trim().to_string(),
            source_account: form.source_account.clone(),
            auth_method: form.auth_method,
            ssh_key: form
                .key_ix
                .and_then(|ix| self.git_ssh_keys.get(ix))
                .map(|key| key.private_path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            sign_commits: form.sign_commits,
            signing_key: form.signing_key.read(cx).text().trim().to_string(),
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
        self.git_account_source_open = false;
        cx.notify();
    }

    /// Delete a saved identity.
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

    /// Apply a saved identity to the active repository (`git config --local`).
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
                // A row's click-to-apply must not also fire when its action
                // button is pressed.
                cx.stop_propagation();
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

/// The SSH-key picker inside the identity editor. It is rendered as the
/// control of a normal [`git_modal_field`], so the list lines up with the
/// text inputs instead of carrying its own inset.
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
        .min_w_0()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
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

/// The read-only System identity row (the global git config default).
fn git_system_identity_row(theme: Theme, name: &str, email: &str) -> AnyElement {
    let subtitle = if !email.trim().is_empty() {
        email.to_string()
    } else if !name.trim().is_empty() {
        name.to_string()
    } else {
        tr!("git_settings.identity_system_unset")
    };
    div()
        .w_full()
        .px(DynamicSpacing::Base16.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base12.px(&theme))
        .child(
            div()
                .flex_none()
                .size(px(32.))
                .rounded(Radius::Medium.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.overlay)
                .child(icon(
                    "icons/git-merge.svg",
                    IconSize::Small.px(&theme),
                    theme.text_2,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base01.px(&theme))
                .child(
                    div()
                        .truncate()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(tr!("git_settings.identity_system")),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_3)
                        .child(subtitle),
                ),
        )
        .into_any_element()
}

/// A labelled field in the identity modal.
fn git_modal_field(theme: Theme, label: &str, required: bool, control: AnyElement) -> AnyElement {
    let mut label_row = div()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme))
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_2)
                .child(label.to_string()),
        );
    if required {
        label_row = label_row.child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.crit)
                .child("*"),
        );
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base06.px(&theme))
        .child(label_row)
        .child(control)
        .into_any_element()
}

/// A full-width input frame for the modal.
fn git_modal_input(input: &Entity<ComposerInput>, theme: &Theme) -> AnyElement {
    input_field_frame(div(), theme)
        .w_full()
        .bg(theme.bg_main)
        .child(input.clone())
        .into_any_element()
}

/// The identity color swatch row.
fn git_color_swatches(theme: Theme, this: Entity<OrbitApp>, selected: &str) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme));
    for (name, hex) in git_account::ACCOUNT_COLORS {
        let color: Hsla = gpui::rgb(*hex).into();
        let is_selected = *name == selected;
        let name = name.to_string();
        let this = this.clone();
        row = row.child(
            div()
                .id(ElementId::Name(format!("git-color-{name}").into()))
                .size(px(22.))
                .rounded_full()
                .bg(color)
                .cursor_pointer()
                .when(is_selected, |swatch| {
                    swatch.border_2().border_color(theme.text)
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let name = name.clone();
                    this.update(cx, |app, cx| app.git_account_pick_color(name, cx));
                }),
        );
    }
    row.into_any_element()
}

/// The identity icon choices.
fn git_icon_picker(theme: Theme, this: Entity<OrbitApp>, selected: &str) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme));
    for (id, path) in git_account::ACCOUNT_ICONS {
        let is_selected = *id == selected;
        let id = id.to_string();
        let this = this.clone();
        row = row.child(
            div()
                .id(ElementId::Name(format!("git-icon-{id}").into()))
                .size(px(30.))
                .rounded(Radius::Medium.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(is_selected, |cell| cell.bg(theme.overlay))
                .when(!is_selected, |cell| {
                    cell.hover(|cell| cell.bg(theme.bg_hover))
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let id = id.clone();
                    this.update(cx, |app, cx| app.git_account_pick_icon(id, cx));
                })
                .child(icon(
                    path,
                    IconSize::Small.px(&theme),
                    if is_selected {
                        theme.accent
                    } else {
                        theme.text_3
                    },
                )),
        );
    }
    row.into_any_element()
}

/// The auth-method segmented control.
fn git_auth_methods(
    theme: Theme,
    this: Entity<OrbitApp>,
    selected: git_account::AuthMethod,
) -> AnyElement {
    use git_account::AuthMethod;
    let options = [
        (
            AuthMethod::Machine,
            "git_settings.identity_auth_machine",
            "icons/monitor.svg",
        ),
        (
            AuthMethod::Account,
            "git_settings.identity_auth_account",
            "icons/at-sign.svg",
        ),
        (
            AuthMethod::Ssh,
            "git_settings.identity_auth_ssh",
            "icons/lock.svg",
        ),
        (
            AuthMethod::Anonymous,
            "git_settings.identity_auth_anonymous",
            "icons/eye-off.svg",
        ),
    ];
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme));
    for (method, key, icon_path) in options {
        let is_selected = method == selected;
        let this = this.clone();
        row = row.child(
            div()
                .id(ElementId::Name(format!("git-auth-{key}").into()))
                .px(DynamicSpacing::Base08.px(&theme))
                .py(DynamicSpacing::Base04.px(&theme))
                .rounded(Radius::Medium.px(&theme))
                .border_1()
                .border_color(if is_selected {
                    theme.border_strong
                } else {
                    theme.border
                })
                .when(is_selected, |chip| chip.bg(theme.overlay))
                .when(!is_selected, |chip| {
                    chip.hover(|chip| chip.bg(theme.bg_hover))
                })
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .cursor_pointer()
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.git_account_pick_auth(method, cx));
                })
                .child(icon(
                    icon_path,
                    IconSize::XSmall.px(&theme),
                    if is_selected {
                        theme.accent
                    } else {
                        theme.text_3
                    },
                ))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(if is_selected {
                            theme.text
                        } else {
                            theme.text_2
                        })
                        .child(tr!(key)),
                ),
        );
    }
    row.into_any_element()
}

/// The helper line under the auth-method control.
fn git_auth_description(method: git_account::AuthMethod) -> String {
    use git_account::AuthMethod;
    match method {
        AuthMethod::Machine => tr!("git_settings.identity_auth_machine_hint"),
        AuthMethod::Account => tr!("git_settings.identity_auth_account_hint"),
        AuthMethod::Ssh => tr!("git_settings.identity_auth_ssh_hint"),
        AuthMethod::Anonymous => tr!("git_settings.identity_auth_anonymous_hint"),
    }
}

/// The source-control-account dropdown (a compact inline list).
fn git_source_account(
    theme: Theme,
    this: Entity<OrbitApp>,
    accounts: &[git_account::GhAccount],
    selected: &str,
    open: bool,
) -> AnyElement {
    let label = if selected.trim().is_empty() {
        tr!("git_settings.identity_source_none")
    } else {
        selected.to_string()
    };
    let chip = button_frame(div().id("git-source-toggle"), &theme, ButtonSize::Medium)
        .w_full()
        .border_1()
        .border_color(theme.border)
        .raised(theme.bg_raised, &theme)
        .cursor_pointer()
        .hover(|chip| chip.raised(theme.bg_hover, &theme))
        .flex()
        .items_center()
        .justify_between()
        .on_mouse_up(MouseButton::Left, {
            let this = this.clone();
            move |_, _, cx| this.update(cx, |app, cx| app.git_account_toggle_source(cx))
        })
        .child(div().text_color(theme.text_2).child(label))
        .child(icon(
            "icons/chevron-down.svg",
            IconSize::XSmall.px(&theme),
            theme.text_3,
        ));

    let mut column = div().w_full().flex().flex_col().child(chip);
    if open {
        let mut list = div()
            .mt(DynamicSpacing::Base04.px(&theme))
            .w_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(theme.border)
            .rounded(Radius::Large.px(&theme))
            .overflow_hidden()
            .child(git_source_option(
                theme,
                this.clone(),
                "",
                selected.trim().is_empty(),
                tr!("git_settings.identity_source_none"),
            ));
        for account in accounts {
            list = list.child(git_source_option(
                theme,
                this.clone(),
                &account.login,
                account.login == selected,
                account.login.clone(),
            ));
        }
        column = column.child(list);
    }
    column.into_any_element()
}

/// One row of the source-account list.
fn git_source_option(
    theme: Theme,
    this: Entity<OrbitApp>,
    login: &str,
    selected: bool,
    label: String,
) -> AnyElement {
    let login = login.to_string();
    div()
        .id(ElementId::Name(format!("git-source-option-{login}").into()))
        .w_full()
        .px(DynamicSpacing::Base12.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .flex()
        .items_center()
        .justify_between()
        .gap(DynamicSpacing::Base08.px(&theme))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.overlay))
        .hover(|row| row.bg(theme.bg_hover))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            let login = login.clone();
            this.update(cx, |app, cx| app.git_account_pick_source(login, cx));
        })
        .child(div().truncate().text_color(theme.text_2).child(label))
        .when(selected, |row| {
            row.child(icon(
                "icons/check.svg",
                IconSize::XSmall.px(&theme),
                theme.accent,
            ))
        })
        .into_any_element()
}

/// A labelled checkbox for the modal.
fn git_checkbox<F>(
    theme: Theme,
    this: Entity<OrbitApp>,
    on: bool,
    label: String,
    action: F,
) -> AnyElement
where
    F: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    div()
        .id("git-identity-sign")
        .w_full()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .cursor_pointer()
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| action(app, cx));
        })
        .child(
            div()
                .flex_none()
                .size(px(16.))
                .rounded(Radius::Small.px(&theme))
                .border_1()
                .border_color(if on {
                    theme.accent
                } else {
                    theme.border_strong
                })
                .when(on, |box_| box_.bg(theme.accent))
                .flex()
                .items_center()
                .justify_center()
                .children(on.then(|| {
                    icon(
                        "icons/check.svg",
                        IconSize::XSmall.px(&theme),
                        theme.send_fg,
                    )
                })),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_2)
                .child(label),
        )
        .into_any_element()
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
