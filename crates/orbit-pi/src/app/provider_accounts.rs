//! Extra provider accounts in Settings → Providers: the Accounts panel inside
//! a provider's row, and the Add / Rename modal. Every account is its own
//! provider (`<provider>~<slug>`, see [`crate::provider_accounts`]), so it
//! signs in on its own row and all accounts are live in every session at once.
//! Nothing here reads or copies a credential.
use super::*;
use crate::provider_accounts::{AccountProfile, CredentialKind};
use crate::theme::tokens::{input, modal, ButtonSize, DynamicSpacing, Radius, TextSize};
use crate::ui::button::IconButton;
use crate::ui::dialog::modal_button;
use crate::ui::field::TextField;
use crate::ui::prelude::*;
use crate::ui::banner::{Banner, BannerTone};
use crate::ui::prelude::{
    input_label, modal_footer, modal_header, ButtonVariant, ModalSubtitle, ModalWidthExt,
};

impl OrbitApp {
    /// One pass over the account store for the Settings controller. Rendering
    /// reads [`Self::account_profile_map`]; it must not touch disk.
    pub(super) fn refresh_account_map(&mut self) {
        self.account_profile_map = self.provider_accounts.all_profiles();
        self.account_store_healthy = self.provider_accounts.is_healthy();
    }

    pub(super) fn cached_account_profiles(&self, provider: &str) -> Vec<AccountProfile> {
        self.account_profile_map
            .get(provider)
            .cloned()
            .unwrap_or_default()
    }

    /// Always `None`: a session no longer picks one account per provider.
    /// Kept for the sign-in paths, which branch on it.
    pub(super) fn selected_account_id(&self, provider: &str) -> Option<String> {
        self.provider_accounts.selected(provider)
    }

    /// pi's own `auth.json`: the Default account and every alias live there.
    pub(super) fn account_auth_path(&self, provider: &str) -> anyhow::Result<PathBuf> {
        if self.remote.is_some() {
            anyhow::bail!("Local accounts are unavailable for remote sessions");
        }
        self.provider_accounts.auth_path(provider)
    }

    pub(super) fn account_add(
        &mut self,
        provider: &str,
        label: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.remote.is_some() {
            return false;
        }
        match self.provider_accounts.add(provider, label) {
            Ok(alias) => {
                self.refresh_account_map();
                self.clear_error_from("account_profiles");
                // Open the new account's own row: that is where it signs in.
                self.provider_models_open.insert(alias, true);
                self.account_reload(cx);
                true
            }
            Err(error) => {
                self.set_error_from("account_profiles", error.to_string());
                cx.notify();
                false
            }
        }
    }

    /// Rename an account. Only its display name changes; the alias, and so
    /// its sign-in, stays.
    pub(super) fn account_rename(&mut self, id: &str, label: &str, cx: &mut Context<Self>) -> bool {
        match self.provider_accounts.rename(id, label) {
            Ok(()) => {
                self.refresh_account_map();
                self.clear_error_from("account_profiles");
                self.account_reload(cx);
                true
            }
            Err(error) => {
                self.set_error_from("account_profiles", error.to_string());
                cx.notify();
                false
            }
        }
    }

    /// Remove an account: its metadata and its credential in pi's `auth.json`.
    pub(super) fn account_remove(&mut self, id: String, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            cx.notify();
            return;
        }
        match self.provider_accounts.remove(&id) {
            Ok(alias) => {
                self.provider_models_open.remove(&alias);
                self.refresh_account_map();
                self.clear_error_from("account_profiles");
                self.toast_success(tr!("accounts.removed"));
                self.account_reload(cx);
            }
            Err(error) => {
                self.set_error_from("account_profiles", error.to_string());
                self.refresh_account_map();
            }
        }
        cx.notify();
    }

    /// pi registers the account providers at startup. Restart now when that
    /// interrupts nothing; mid-turn, raise the "Restart pi" banner instead.
    fn account_reload(&mut self, cx: &mut Context<Self>) {
        self.reload_custom_providers_later(cx);
        if self.busy || self.transcript.is_streaming() || self.auth.is_busy() {
            self.provider_auth_dirty = true;
            cx.notify();
            return;
        }
        self.provider_apply_credentials(cx);
    }

    /// Views for accounts pi has not reported yet (no sign-in, or pi not yet
    /// restarted), so every account has a row to sign in on.
    pub(super) fn account_views(&self, views: &[ProviderView]) -> Vec<ProviderView> {
        if self.remote.is_some() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (provider, profiles) in &self.account_profile_map {
            let base = views.iter().find(|view| &view.id == provider);
            for profile in profiles {
                if views.iter().any(|view| view.id == profile.alias) {
                    continue;
                }
                let mut view = self.provider_view_for(
                    &profile.alias,
                    Some(providers::provider_display_name(&profile.alias)),
                    base.is_some_and(|base| base.oauth),
                    base.is_none_or(|base| base.api_key),
                    &[],
                    base.map(|base| base.note).unwrap_or(""),
                );
                if view.catalog_count == 0 {
                    view.catalog_count = base.map(|base| base.catalog_count).unwrap_or(0);
                }
                out.push(view);
            }
        }
        out
    }

    /// The Accounts panel inside one provider row. Local sessions only: a
    /// remote agent's credentials are on the host.
    pub(super) fn provider_accounts_section(
        &self,
        view: &ProviderView,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Option<AnyElement> {
        // An account row has no accounts of its own.
        if self.remote.is_some() || crate::provider_accounts::split_alias(&view.id).is_some() {
            return None;
        }
        let profiles = self.cached_account_profiles(&view.id);

        let add = self.provider_button(
            format!("provider-account-add-{}", view.id),
            &tr!("accounts.add"),
            ButtonVariant::Secondary,
            theme,
            this.clone(),
            ProviderAction::AddAccount {
                id: view.id.clone(),
                name: view.name.clone(),
            },
        );
        let muted = |text: String| {
            div()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_muted)
                .whitespace_normal()
                .child(text)
        };

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary)
                            .child(tr!("accounts.title")),
                    )
                    .child(muted(if profiles.is_empty() {
                        tr!("accounts.empty").to_string()
                    } else {
                        tr!("accounts.live_hint").to_string()
                    })),
            )
            .child(add);

        let mut section = div()
            .debug_selector({
                let id = view.id.clone();
                move || format!("provider-accounts-{id}")
            })
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(header);

        if !profiles.is_empty() {
            section = section.child(
                div()
                    .w_full()
                    .rounded(Radius::Large.px(&theme))
                    .bg(theme.card_fill(theme.surface))
                    .border_1()
                    .border_color(theme.hairline_subtle())
                    .p(DynamicSpacing::Base04.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(
                        profiles
                            .iter()
                            .map(|profile| self.account_row(view, profile, theme, this.clone())),
                    ),
            );
        }
        if !self.account_store_healthy {
            section = section.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.error)
                            .child(tr!("accounts.unavailable")),
                    )
                    .child(muted(tr!("accounts.unavailable_hint").to_string())),
            );
        }
        Some(section.into_any_element())
    }

    /// One account: its name, what it is signed in with, and Rename / Remove.
    /// Clicking it opens the account's own provider row, where it signs in.
    fn account_row(
        &self,
        view: &ProviderView,
        profile: &AccountProfile,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let row_id = format!("provider-account-{}", profile.id);
        let debug_id = row_id.clone();

        let (status, status_color) = match profile.credential {
            Some(CredentialKind::OAuth) => (tr!("accounts.signed_in_oauth"), theme.success),
            Some(CredentialKind::ApiKey) => (tr!("accounts.signed_in_key"), theme.success),
            None => (tr!("accounts.not_signed_in"), theme.text_disabled),
        };

        let trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base02.px(&theme))
            .child(account_icon_button(
                format!("provider-account-rename-{}", profile.id),
                "icons/pencil.svg",
                tr!("accounts.rename").to_string(),
                theme.text_secondary,
                theme,
                this.clone(),
                ProviderAction::RenameAccount {
                    provider: view.id.clone(),
                    name: view.name.clone(),
                    id: profile.id.clone(),
                    label: profile.label.clone(),
                },
            ))
            .child(account_icon_button(
                format!("provider-account-remove-{}", profile.id),
                "icons/trash.svg",
                tr!("settings.remove").to_string(),
                theme.error,
                theme,
                this.clone(),
                ProviderAction::RemoveAccount {
                    id: profile.id.clone(),
                    label: profile.label.clone(),
                    alias: profile.alias.clone(),
                },
            ));

        let alias = profile.alias.clone();
        let open_this = this.clone();
        let main = div()
            .id(ElementId::Name(row_id.into()))
            .debug_selector(move || debug_id.clone())
            .w_full()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .px(DynamicSpacing::Base08.px(&theme))
            .py(DynamicSpacing::Base06.px(&theme))
            .rounded(Radius::Medium.px(&theme))
            .cursor_pointer()
            .hover_state(&theme)
            .tip(tr!("accounts.open_tip"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        // A row, so the name is measured at full width first
                        // (see the facts line in `provider_row`).
                        div()
                            .flex()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Default.px(&theme))
                            .text_color(theme.text_primary)
                            .child(profile.label.clone()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base04.px(&theme))
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(status_color),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(TextSize::XSmall.px(&theme))
                                    .text_color(theme.text_muted)
                                    .child(format!("{status} · {}", profile.alias)),
                            ),
                    ),
            )
            .child(trailing)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let alias = alias.clone();
                open_this.update(cx, |app, cx| {
                    app.provider_models_open.insert(alias, true);
                    cx.notify();
                });
            });

        main.into_any_element()
    }

    /// Open the Add-account modal, or the Rename modal when `rename` names an
    /// existing profile and its current label.
    pub(super) fn account_editor_open(
        &mut self,
        provider: String,
        name: String,
        rename: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.provider_key_editor = None;
        self.provider_usage_open = None;
        let current = rename.as_ref().map(|(_, label)| label.clone());
        let label = cx.new(|cx| {
            let input = ComposerInput::new(cx)
                .with_element_id("account-name-input")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder_key("accounts.name_placeholder");
            match current {
                Some(text) => input.with_text(text),
                None => input,
            }
        });
        let target = label.read(cx).focus_handle(cx);
        self.account_editor_focus.open(&target, window, cx);
        self.account_editor = Some(AccountEditor {
            provider_id: provider,
            provider_name: name,
            rename: rename.map(|(id, _)| id),
            label,
            error: None,
        });
        cx.notify();
    }

    /// Create the profile and make it the session's account. Failure keeps the
    /// modal open; the banner carries the precise reason.
    pub(super) fn account_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.account_editor.as_ref() else {
            return;
        };
        if crate::composer::any_composing(cx, &[&editor.label]) {
            return;
        }
        let label = editor.label.read(cx).text().trim().to_string();
        if label.is_empty() {
            if let Some(editor) = self.account_editor.as_mut() {
                editor.error = Some(tr!("accounts.name_required"));
            }
            cx.notify();
            return;
        }
        if let Some(id) = editor.rename.clone() {
            if self.account_rename(&id, &label, cx) {
                self.account_editor = None;
                self.toast_success(tr!("accounts.renamed"));
            } else if let Some(editor) = self.account_editor.as_mut() {
                editor.error = Some(tr!("accounts.rename_failed"));
            }
            cx.notify();
            return;
        }
        let provider = editor.provider_id.clone();
        if self.account_add(&provider, &label, cx) {
            self.account_editor = None;
            self.toast_info(tr!("accounts.added"));
        } else if let Some(editor) = self.account_editor.as_mut() {
            editor.error = Some(tr!("accounts.create_failed"));
        }
        cx.notify();
    }

    /// The Add-account modal, or `None` when closed.
    pub(super) fn account_editor_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let editor = self.account_editor.as_ref()?;
        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(input::gap(&theme))
                    .child(
                        input_label(tr!("accounts.name"), &theme)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary),
                    )
                    .child(TextField::new(editor.label.clone(), theme.background).render(&theme))
                    .when(editor.rename.is_none(), |field| {
                        field.child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .whitespace_normal()
                                .child(tr!("accounts.name_hint_live")),
                        )
                    }),
            );
        if let Some(error) = &editor.error {
            body = body.child(
                Banner::note(BannerTone::Error, error.clone(), &theme),
            );
        }

        let cancel = modal_button(
            "provider-account-close",
            tr!("settings.cancel"),
            ButtonVariant::Secondary,
            &self.modal_cancel_focus,
        )
        .size(ButtonSize::Large)
        .on_click(&theme, {
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.account_editor = None;
                    cx.notify();
                });
            }
        });
        let create = modal_button(
            "provider-account-create",
            if editor.rename.is_some() {
                tr!("accounts.save_name")
            } else {
                tr!("accounts.create")
            },
            ButtonVariant::Primary,
            &self.modal_confirm_focus,
        )
        .size(ButtonSize::Large)
        .on_click(&theme, {
            let this = this.clone();
            move |_, window, cx| {
                this.update(cx, |app, cx| {
                    app.apply_provider_action(ProviderAction::SaveAccount, window, cx)
                });
            }
        });

        let card = modal_card(&theme)
            .debug_selector(|| "provider-account-card".to_string())
            .modal_width(px(460.))
            .max_h(relative(1.))
            .on_action(cx.listener(Self::provider_editor_cancel))
            .on_action(cx.listener(Self::provider_editor_confirm))
            .child(modal_header(
                &theme,
                if editor.rename.is_some() {
                    tr!("accounts.rename_for", name = editor.provider_name)
                } else {
                    tr!("accounts.add_for", name = editor.provider_name)
                },
                Some(ModalSubtitle::Id(editor.provider_id.clone().into())),
            ))
            .child(
                div()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body),
            )
            .child(
                modal_footer(&theme)
                    .debug_selector(|| "provider-account-footer".to_string())
                    .child(cancel)
                    .child(create),
            );

        // The backdrop every modal shares (`ui::dialog::modal_scrim`'s role).
        let scrim = theme.scrim_modal();
        Some(
            div()
                .id("provider-account-layer")
                .absolute()
                .inset_0()
                .occlude()
                .bg(scrim)
                .p(DynamicSpacing::Base24.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::provider_editor_scrim))
                .child(crate::motion::enter(
                    card,
                    "provider-account-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }
}

/// A quiet square icon button for a row action (Rename, Remove). The press is
/// the button's alone: the row it sits in opens the account on click.
fn account_icon_button(
    id: String,
    glyph: &'static str,
    tip: String,
    color: Hsla,
    theme: Theme,
    this: Entity<OrbitApp>,
    action: ProviderAction,
) -> AnyElement {
    let debug = id.clone();
    IconButton::new(ElementId::Name(id.into()), glyph)
        .ink(color)
        .tip(tip)
        .on_mouse_up(&theme, move |_, window, cx| {
            cx.stop_propagation();
            let action = action.clone();
            this.update(cx, |app, cx| app.apply_provider_action(action, window, cx));
        })
        .debug_selector(move || debug.clone())
        .into_any_element()
}
