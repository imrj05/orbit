//! Settings → Providers & models, per machine.
//!
//! A machine selector at the top of the page (after t3code's environment
//! picker) chooses whose providers the page shows: **This Mac** keeps the page
//! as it always was (this machine's `auth.json`, `models.json` and catalog);
//! a remote host shows *that host's* providers, read from its own pi with
//! `pi --list-models` over ssh. That command reads the host's credentials and
//! prints only the models they unlock, so the providers listed are exactly the
//! ones signed in there and no credential ever leaves the host.
//!
//! On a host, **Sign in** runs pi's own `/login` there in Terminal (OAuth and
//! API keys alike; see `orbit_rpc::remote::login_script`), and **Refresh**
//! reads the list again. The page opens on the machine the current session
//! runs on.

use std::collections::BTreeMap;

use super::mcp_ui::mcp_text_button;
use super::ssh_auth_ui::AuthThen;
use super::*;
use crate::ui::popover::anchored_menu;
use crate::ui::chip::{ChipButton, ChipStyle};
use crate::remote_hosts::{self, ListedModel, ProbeFailure, RemoteHost};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, TextSize};
use crate::ui::button::ButtonVariant;
use crate::ui::dialog::modal_button;
use crate::ui::field::TextField;
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::prelude::*;
use crate::ui::banner::{Banner, BannerTone};

/// One host's provider listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteModels {
    Loading,
    Loaded(Vec<ListedModel>),
    Failed(String),
    /// Connected, but pi is not installed for the account.
    PiMissing,
}

/// The Add API key dialog for a host.
pub(crate) struct RemoteKeyEditor {
    pub(super) host: RemoteHost,
    /// A built-in provider that accepts an API key.
    pub(super) provider: Option<&'static str>,
    /// Masked; never copied, cleared with the dialog.
    pub(super) input: Entity<ComposerInput>,
    pub(super) error: Option<String>,
    pub(super) saving: bool,
}

/// A listing grouped by provider, in the order pi printed them.
pub(crate) fn group_by_provider(models: &[ListedModel]) -> Vec<(String, Vec<&ListedModel>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Vec<&ListedModel>> = BTreeMap::new();
    for model in models {
        if !groups.contains_key(&model.provider) {
            order.push(model.provider.clone());
        }
        groups
            .entry(model.provider.clone())
            .or_default()
            .push(model);
    }
    order
        .into_iter()
        .map(|provider| {
            let models = groups.remove(&provider).unwrap_or_default();
            (provider, models)
        })
        .collect()
}

impl OrbitApp {
    /// The host the Providers page is showing, when it is a remote one that
    /// still exists and is switched on.
    pub(super) fn providers_remote_host(&self) -> Option<RemoteHost> {
        let name = self.providers_machine.as_ref()?;
        self.remote_hosts
            .iter()
            .find(|host| &host.name == name && host.enabled)
            .cloned()
    }

    /// Opening Providers & models: pick the machine the current session runs
    /// on, and read its providers when it is a host.
    pub(super) fn providers_section_opened(&mut self, cx: &mut Context<Self>) {
        self.reload_remote_hosts();
        self.providers_machine_menu = false;
        // Coming back (after signing in in the terminal, say) keeps the host
        // that was chosen; otherwise the page opens on the session's machine.
        if self.providers_remote_host().is_none() {
            self.providers_machine = self.remote.as_ref().map(|host| host.name.clone());
        }
        if let Some(host) = self.providers_remote_host() {
            self.fetch_remote_models(host, 0, cx);
        } else {
            self.providers_machine = None;
        }
    }

    pub(super) fn select_providers_machine(
        &mut self,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.providers_machine_menu = false;
        self.providers_machine = name;
        if let Some(host) = self.providers_remote_host() {
            self.fetch_remote_models(host, 0, cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_providers_machine_menu(&mut self, cx: &mut Context<Self>) {
        self.providers_machine_menu = !self.providers_machine_menu;
        cx.notify();
    }

    /// Read `host`'s providers in the background (`pi --list-models` over
    /// ssh, with the remembered password). A host that wants a password gets
    /// the password dialog, which comes back here.
    pub(super) fn fetch_remote_models(
        &mut self,
        host: RemoteHost,
        attempt: u8,
        cx: &mut Context<Self>,
    ) {
        self.remote_model_lists
            .insert(host.name.clone(), RemoteModels::Loading);
        cx.notify();
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let list_host = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { remote_hosts::list_models(&list_host, secret.as_deref()) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_model_list(host, attempt, outcome, cx);
            });
        })
        .detach();
    }

    pub(super) fn finish_remote_model_list(
        &mut self,
        host: RemoteHost,
        attempt: u8,
        outcome: Result<Vec<ListedModel>, ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(models) => {
                self.remote_model_lists
                    .insert(host.name.clone(), RemoteModels::Loaded(models));
                cx.notify();
            }
            // Every failure (password, cloud root guard, host key, pi
            // missing) is explained the way the Remote hosts page explains it.
            Err(failure) => {
                let name = host.name.clone();
                self.finish_remote_probe(
                    host,
                    AuthThen::ProviderList(name),
                    attempt,
                    Err(failure),
                    cx,
                );
            }
        }
    }

    /// Sign in on `host` with pi's own `/login` (its provider picker, OAuth
    /// and API keys) over `ssh -t`, in a tab of Orbit's own terminal panel:
    /// Settings steps aside so the tab is in view. ssh and pi ask their own
    /// questions there (password, host key, the browser code). On Windows,
    /// where the command's POSIX quoting does not suit `cmd`, the system
    /// terminal takes it instead.
    pub(super) fn remote_provider_sign_in(
        &mut self,
        host: RemoteHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match host.target().and_then(|target| {
            let script = orbit_rpc::remote::login_script(None)?;
            orbit_rpc::remote::interactive_ssh_command(&target, &script)
        }) {
            Ok(command) => command,
            Err(err) => {
                self.toast_error(format!("{err:#}"));
                cx.notify();
                return;
            }
        };
        // A session already running there loads new credentials only on a
        // restart; the page offers it.
        let session_host = self.remote_host_name() == Some(host.name.as_str());
        if cfg!(windows) {
            match crate::platform::open_terminal_command(&command) {
                Ok(()) => {
                    self.provider_auth_dirty |= session_host;
                    self.toast_info(tr!("providers_remote.sign_in_hint", host = host.name));
                }
                Err(err) => self.toast_error(err),
            }
            cx.notify();
            return;
        }
        if !self.close_settings(cx) {
            return;
        }
        self.terminal_panel
            .update(cx, |panel, cx| panel.new_command_tab(command, window, cx));
        self.provider_auth_dirty |= session_host;
        self.toast_info(tr!(
            "providers_remote.sign_in_terminal_hint",
            host = host.name
        ));
        cx.notify();
    }

    /// Open the Add API key dialog for `host`.
    pub(super) fn open_remote_key_editor(
        &mut self,
        host: RemoteHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("remote-key-input")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_masked(true)
                .with_placeholder_key("providers_remote.key_placeholder")
        });
        let target = input.read(cx).focus_handle(cx);
        self.remote_key_editor_focus.open(&target, window, cx);
        self.remote_key_editor = Some(RemoteKeyEditor {
            host,
            provider: None,
            input,
            error: None,
            saving: false,
        });
        cx.notify();
    }

    pub(super) fn close_remote_key_editor(&mut self, cx: &mut Context<Self>) {
        if self.remote_key_editor.take().is_some() {
            cx.notify();
        }
    }

    fn pick_remote_key_provider(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if let Some(editor) = self.remote_key_editor.as_mut() {
            editor.provider = Some(id);
            editor.error = None;
        }
        cx.notify();
    }

    /// Save the key typed in the dialog (`from_this_mac` reads this Mac's own
    /// key for the provider instead) on the host, in the background.
    pub(super) fn save_remote_key(&mut self, from_this_mac: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.remote_key_editor.as_mut() else {
            return;
        };
        if editor.saving || editor.input.read(cx).is_composing() {
            return;
        }
        let Some(provider) = editor.provider else {
            editor.error = Some(tr!("providers_remote.key_pick_provider"));
            cx.notify();
            return;
        };
        let key = if from_this_mac {
            crate::providers::local_api_key(provider)
        } else {
            Some(editor.input.read(cx).text().trim().to_string()).filter(|key| !key.is_empty())
        };
        let Some(key) = key else {
            editor.error = Some(tr!("providers.api_key_required"));
            cx.notify();
            return;
        };
        editor.saving = true;
        editor.error = None;
        let host = editor.host.clone();
        let secret = self.remote_secret(&host);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let save_host = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    remote_hosts::set_api_key(&save_host, secret.as_deref(), provider, &key)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_key(host, provider, outcome, cx)
            });
        })
        .detach();
    }

    pub(super) fn finish_remote_key(
        &mut self,
        host: RemoteHost,
        provider: &'static str,
        outcome: Result<(), ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(()) => {
                self.remote_key_editor = None;
                if self.remote_host_name() == Some(host.name.as_str()) {
                    self.provider_auth_dirty = true;
                }
                self.toast_success(tr!(
                    "providers_remote.key_saved",
                    provider = crate::providers::provider_display_name(provider),
                    host = host.name.clone()
                ));
                self.fetch_remote_models(host, 0, cx);
            }
            Err(failure) => {
                let message = failure_text(failure, &host);
                if let Some(editor) = self.remote_key_editor.as_mut() {
                    editor.saving = false;
                    editor.error = Some(message);
                }
            }
        }
        cx.notify();
    }

    fn toggle_remote_provider(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.remote_provider_expanded.remove(&key) {
            self.remote_provider_expanded.insert(key);
        }
        cx.notify();
    }

    // ── rendering ─────────────────────────────────────────────────────────

    /// The machine selector: hidden when there is no host to choose.
    pub(super) fn providers_machine_selector(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Option<AnyElement> {
        let hosts: Vec<&RemoteHost> = self
            .remote_hosts
            .iter()
            .filter(|host| host.enabled)
            .collect();
        if hosts.is_empty() {
            return None;
        }
        let selected = self.providers_remote_host();
        let (glyph, label) = match &selected {
            Some(host) => ("icons/cloud.svg", host.name.clone()),
            None => ("icons/monitor.svg", tr!("providers_remote.this_mac")),
        };
        let open = self.providers_machine_menu;
        let toggle_this = this.clone();
        let button = div()
            .relative()
            .child(
                ChipButton::new("providers-machine", ChipStyle::Outlined { inset: false })
                    .size(ButtonSize::Large)
                    .selector("providers-machine")
                    .icon(glyph)
                    .label(label)
                    .open(open)
                    .on_mouse_up(&theme, move |_, _, cx| {
                cx.stop_propagation();
                toggle_this.update(cx, |app, cx| app.toggle_providers_machine_menu(cx));
            }),
            )
            .children(open.then(|| machine_menu(&hosts, selected.as_ref(), theme, this.clone())));
        Some(
            div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(tr!("providers_remote.machine_label")),
                )
                .child(button)
                .into_any_element(),
        )
    }

    /// The toolbar while a host is selected: the selector, Sign in, Refresh.
    pub(super) fn remote_provider_toolbar(
        &self,
        host: &RemoteHost,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let loading = self.remote_model_lists.get(&host.name) == Some(&RemoteModels::Loading);
        let (sign_in_host, refresh_host, key_host) = (host.clone(), host.clone(), host.clone());
        div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .children(self.providers_machine_selector(theme, this.clone()))
            .child(div().flex_1())
            .child(mcp_text_button(
                "providers-remote-refresh".into(),
                if loading {
                    tr!("remote_hosts.checking")
                } else {
                    tr!("providers_remote.refresh")
                },
                theme,
                false,
                this.clone(),
                move |app, _, cx| {
                    if app.remote_model_lists.get(&refresh_host.name)
                        != Some(&RemoteModels::Loading)
                    {
                        app.fetch_remote_models(refresh_host.clone(), 0, cx);
                    }
                },
            ))
            .child(mcp_text_button(
                "providers-remote-add-key".into(),
                tr!("providers_remote.add_key"),
                theme,
                false,
                this.clone(),
                move |app, window, cx| app.open_remote_key_editor(key_host.clone(), window, cx),
            ))
            .child(mcp_text_button(
                "providers-remote-sign-in".into(),
                tr!("providers_remote.sign_in"),
                theme,
                true,
                this,
                move |app, window, cx| {
                    app.remote_provider_sign_in(sign_in_host.clone(), window, cx)
                },
            ))
            .into_any_element()
    }

    /// The page body while a host is selected: what is signed in there.
    pub(super) fn remote_provider_rows(
        &self,
        host: &RemoteHost,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        rows.push(
            div()
                .px(DynamicSpacing::Base04.px(&theme))
                .text_size(TextSize::Default.px(&theme))
                .text_color(theme.text_secondary)
                .child(tr!(
                    "providers_remote.intro",
                    host = host.name.clone(),
                    target = host.connection_label()
                ))
                .into_any_element(),
        );
        // New sign-ins reach a session already running there on a restart.
        if self.provider_auth_dirty && self.remote_host_name() == Some(host.name.as_str()) {
            rows.push(self.settings_group(
                theme,
                vec![self.setting_row(
                    theme,
                    &tr!("settings.credentials_changed"),
                    Some(&tr!("providers_remote.restart_hint")),
                    None,
                    Some(mcp_text_button(
                        "providers-remote-restart".into(),
                        tr!("settings.restart_pi"),
                        theme,
                        true,
                        this.clone(),
                        |app, _, cx| app.provider_apply_credentials(cx),
                    )),
                )],
            ));
        }
        match self.remote_model_lists.get(&host.name) {
            None | Some(RemoteModels::Loading) => rows.push(self.settings_group(
                theme,
                vec![div()
                    .w_full()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(spinner(
                        "providers-remote-loading",
                        IconSize::Small.px(&theme),
                        theme.text_muted,
                        theme,
                    ))
                    .child(tr!("providers_remote.loading", host = host.name.clone()))
                    .into_any_element()],
            )),
            Some(RemoteModels::Failed(message)) => rows.push(self.settings_group(
                theme,
                vec![self.setting_row(
                    theme,
                    &tr!("providers_remote.failed_title"),
                    Some(message),
                    None,
                    None,
                )],
            )),
            Some(RemoteModels::PiMissing) => rows.push(self.settings_group(
                theme,
                vec![self.setting_row(
                    theme,
                    &tr!("remote_hosts.pi_missing"),
                    Some(&tr!("providers_remote.pi_missing_hint")),
                    None,
                    Some(mcp_text_button(
                        "providers-remote-open-hosts".into(),
                        tr!("providers_remote.open_remote_hosts"),
                        theme,
                        false,
                        this.clone(),
                        |app, _, cx| app.set_settings_section(SettingsSection::RemoteHosts, cx),
                    )),
                )],
            )),
            Some(RemoteModels::Loaded(models)) if models.is_empty() => {
                let sign_in_host = host.clone();
                rows.push(self.settings_group(
                    theme,
                    vec![div()
                            .debug_selector(|| "providers-remote-empty".to_string())
                            .w_full()
                            .px(DynamicSpacing::Base16.px(&theme))
                            .py(DynamicSpacing::Base24.px(&theme))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(icon(
                                "icons/cloud.svg",
                                IconSize::XLarge.px(&theme),
                                theme.text_muted,
                            ))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(tr!(
                                        "providers_remote.empty_title",
                                        host = host.name.clone()
                                    )),
                            )
                            .child(
                                div()
                                    .max_w(px(440.))
                                    .text_center()
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_muted)
                                    .child(tr!("providers_remote.empty_body")),
                            )
                            .child(div().pt(DynamicSpacing::Base08.px(&theme)).child(
                                mcp_text_button(
                                    "providers-remote-empty-sign-in".into(),
                                    tr!("providers_remote.sign_in"),
                                    theme,
                                    true,
                                    this.clone(),
                                    move |app, window, cx| {
                                        app.remote_provider_sign_in(
                                            sign_in_host.clone(),
                                            window,
                                            cx,
                                        )
                                    },
                                ),
                            ))
                            .into_any_element()],
                ));
            }
            Some(RemoteModels::Loaded(models)) => {
                let groups = group_by_provider(models);
                let group_rows: Vec<AnyElement> = groups
                    .iter()
                    .map(|(provider, models)| {
                        self.remote_provider_row(host, provider, models, theme, this.clone())
                    })
                    .collect();
                rows.push(self.settings_section(
                    theme,
                    &tr!("providers_remote.connected", count = groups.len()),
                    group_rows,
                ));
            }
        }
        rows
    }

    /// One signed-in provider on the host, with its models behind a
    /// disclosure.
    fn remote_provider_row(
        &self,
        host: &RemoteHost,
        provider: &str,
        models: &[&ListedModel],
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let key = format!("{}\u{0}{provider}", host.name);
        let expanded = self.remote_provider_expanded.contains(&key);
        let toggle_key = key.clone();
        let header = div()
            .id(ElementId::Name(
                format!("providers-remote-{provider}").into(),
            ))
            .debug_selector({
                let provider = provider.to_string();
                move || format!("providers-remote-{provider}")
            })
            .w_full()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.toggle_remote_provider(toggle_key.clone(), cx)
                });
            })
            .child(crate::model_selector::provider_mark(provider, theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(crate::providers::provider_display_name(provider)),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!("providers_remote.model_count", count = models.len())),
                    ),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.success)
                    .child(tr!("remote_hosts.reachable")),
            )
            .child(icon(
                if expanded {
                    "icons/chevron-up.svg"
                } else {
                    "icons/chevron-down.svg"
                },
                IconSize::XSmall.px(&theme),
                theme.text_muted,
            ));
        let mut row = div()
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(header);
        if expanded {
            let mut list = div()
                .pl(IconSize::Medium.px(&theme) + DynamicSpacing::Base12.px(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base04.px(&theme));
            for model in models {
                let mut traits = vec![tr!(
                    "providers_remote.context",
                    size = model.context.clone()
                )];
                if model.thinking {
                    traits.push(tr!("providers_remote.thinking"));
                }
                if model.images {
                    traits.push(tr!("providers_remote.images"));
                }
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base12.px(&theme))
                        .text_size(TextSize::Small.px(&theme))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(theme::code_font_family())
                                .text_color(theme.text_secondary)
                                .child(model.id.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(theme.text_muted)
                                .child(traits.join(" · ")),
                        ),
                );
            }
            row = row.child(list);
        }
        row.into_any_element()
    }
}

impl OrbitApp {
    /// The Add API key dialog: a key-based provider from pi's catalog, then
    /// the key (masked) or **Use this Mac's key**. Saving writes the host's
    /// `auth.json` over ssh; the key travels on stdin only.
    pub(super) fn remote_key_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let editor = self.remote_key_editor.as_ref()?;
        let signed_in: std::collections::HashSet<&str> =
            match self.remote_model_lists.get(&editor.host.name) {
                Some(RemoteModels::Loaded(models)) => {
                    models.iter().map(|m| m.provider.as_str()).collect()
                }
                _ => Default::default(),
            };

        // Providers that take an API key, as rows.
        let mut list = div()
            .id("remote-key-providers")
            .max_h(px(220.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base02.px(&theme));
        for provider in crate::providers::BUILTIN_PROVIDERS
            .iter()
            .filter(|p| p.api_key)
        {
            let selected = editor.provider == Some(provider.id);
            let pick_this = this.clone();
            let id = provider.id;
            let row = picker_entry(
                div().id(ElementId::Name(format!("remote-key-provider-{id}").into())),
                &theme,
            )
            .h(crate::theme::tokens::picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.selected_wash()))
            .child(crate::model_selector::provider_mark(id, theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.text_primary)
                    .child(provider.name),
            )
            .when(signed_in.contains(id), |row| {
                row.child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.success)
                        .child(tr!("remote_hosts.reachable")),
                )
            })
            .when(selected, |row| {
                row.child(icon(
                    "icons/check.svg",
                    crate::theme::tokens::context_menu::ICON.px(&theme),
                    theme.accent_text,
                ))
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                pick_this.update(cx, |app, cx| app.pick_remote_key_provider(id, cx));
            });
            list = list.child(crate::motion::hover(
                row,
                format!("remote-key-provider-{id}"),
                &theme,
                crate::motion::Hover::wash(&theme),
            ));
        }

        let local_key = editor
            .provider
            .is_some_and(|id| crate::providers::local_api_key(id).is_some());
        let mut body = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                input_label(tr!("providers_remote.key_provider"), &theme)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary),
            )
            .child(list)
            .child(
                input_label(tr!("providers_remote.key_label"), &theme)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary),
            )
            .child(TextField::new(editor.input.clone(), theme.background).render(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!(
                        "providers_remote.key_hint",
                        host = editor.host.name.clone()
                    )),
            );
        if let Some(error) = &editor.error {
            body = body.child(
                Banner::note(BannerTone::Error, error.clone(), &theme),
            );
        }

        let footer = div()
            .flex_none()
            .px(crate::theme::tokens::modal::footer_padding(&theme))
            .py(crate::theme::tokens::modal::footer_padding(&theme))
            .flex()
            .items_center()
            .gap(crate::theme::tokens::modal::footer_gap(&theme))
            .border_t_1()
            .border_color(theme.hairline())
            .children(local_key.then(|| {
                mcp_text_button(
                    "remote-key-use-local".into(),
                    tr!("providers_remote.use_local_key"),
                    theme,
                    false,
                    this.clone(),
                    |app, _, cx| app.save_remote_key(true, cx),
                )
            }))
            .child(div().flex_1())
            .child({
                let this = this.clone();
                modal_button(
                    "remote-key-cancel",
                    tr!("remote_hosts.cancel"),
                    ButtonVariant::Secondary,
                    &self.modal_cancel_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.close_remote_key_editor(cx))
                })
            })
            .child({
                let this = this.clone();
                modal_button(
                    "remote-key-save",
                    if editor.saving {
                        tr!("providers_remote.key_saving")
                    } else {
                        tr!("remote_hosts.save")
                    },
                    ButtonVariant::Primary,
                    &self.modal_confirm_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.save_remote_key(false, cx))
                })
            });

        let card = modal_card(&theme)
            .debug_selector(|| "remote-key-dialog".to_string())
            .modal_width(px(480.))
            .max_h(relative(1.))
            .key_context("Picker")
            .on_action(
                cx.listener(|app, _: &crate::PickerCancel, _, cx| app.close_remote_key_editor(cx)),
            )
            .on_action(
                cx.listener(|app, _: &crate::PickerConfirm, _, cx| app.save_remote_key(false, cx)),
            )
            .child(crate::ui::dialog::modal_header(
                &theme,
                tr!(
                    "providers_remote.key_title",
                    host = editor.host.name.clone()
                ),
                None,
            ))
            .child(
                div()
                    .id("remote-key-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(crate::theme::tokens::modal::section_padding_x(
                        &theme, false,
                    ))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body),
            )
            .child(footer);

        Some(
            modal_scrim(&theme)
                .id("remote-key-layer")
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.close_remote_key_editor(cx))
                })
                .child(crate::motion::enter(
                    card,
                    "remote-key-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }
}

/// The selector's menu: This Mac, then each enabled host, the current one
/// checked.
fn machine_menu(
    hosts: &[&RemoteHost],
    selected: Option<&RemoteHost>,
    theme: Theme,
    this: Entity<OrbitApp>,
) -> AnyElement {
    let theme = theme.for_floating();
    let entry = |id: String,
                 glyph: &'static str,
                 title: String,
                 detail: String,
                 checked: bool,
                 pick: Option<String>| {
        let this = this.clone();
        let row = picker_entry(div().id(ElementId::Name(id.clone().into())), &theme)
            .h(crate::theme::tokens::picker::two_line_entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .when(checked, |row| row.bg(theme.selected_wash()))
            .child(
                PickerRow::new(title, theme.text_primary, theme.text_muted)
                    .icon(glyph, theme.text_muted)
                    .detail(detail)
                    .mono()
                    .weight(FontWeight::MEDIUM)
                    .into_element(&theme),
            )
            .when(checked, |row| {
                row.child(icon(
                    "icons/check.svg",
                    crate::theme::tokens::context_menu::ICON.px(&theme),
                    theme.accent_text,
                ))
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                let pick = pick.clone();
                this.update(cx, |app, cx| app.select_providers_machine(pick, cx));
            });
        crate::motion::hover(row, id, &theme, crate::motion::Hover::wash(&theme))
    };
    let mut body = div()
        .py(crate::theme::tokens::picker::list_padding_y(&theme))
        .flex()
        .flex_col()
        .child(entry(
            "providers-machine-local".into(),
            "icons/monitor.svg",
            tr!("providers_remote.this_mac"),
            tr!("providers_remote.this_mac_detail"),
            selected.is_none(),
            None,
        ));
    for host in hosts {
        body = body.child(entry(
            format!("providers-machine-{}", host.name),
            "icons/cloud.svg",
            host.name.clone(),
            host.connection_label(),
            selected.is_some_and(|selected| selected.name == host.name),
            Some(host.name.clone()),
        ));
    }
    let dismiss = this.clone();
    let popup = popover_picker(div(), &theme)
        .debug_selector(|| "providers-machine-menu".to_string())
        .w(px(300.))
        .dismiss_on_outside(move |_, _, cx| {
            dismiss.update(cx, |app, cx| {
                app.menu_dismissed_at.arm();
                app.providers_machine_menu = false;
                cx.notify();
            })
        })
        .child(body);
    anchored_menu(Corner::TopLeft, point(
            px(0.),
            ButtonSize::Large.height(&theme) + crate::theme::tokens::popover::MENU_OFFSET,
        ), popup)
}

/// One line saying why a short ssh run on `host` (saving a key, setting the
/// workflow mode) did not complete.
pub(super) fn failure_text(failure: ProbeFailure, host: &RemoteHost) -> String {
    match failure {
        ProbeFailure::NeedsPassword { .. } | ProbeFailure::WrongPassword => {
            tr!("providers_remote.key_needs_login")
        }
        ProbeFailure::PiMissing => tr!("remote_hosts.pi_missing"),
        ProbeFailure::LoginRefused { message, .. } => message,
        ProbeFailure::HostKey(_) => tr!(
            "remote_hosts.host_key_unknown",
            command = super::ssh_auth_ui::ssh_command(host)
        ),
        ProbeFailure::Other(message) => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(provider: &str, id: &str) -> ListedModel {
        ListedModel {
            provider: provider.into(),
            id: id.into(),
            context: "1M".into(),
            max_output: "128K".into(),
            thinking: true,
            images: false,
        }
    }

    #[test]
    fn models_group_by_provider_in_listing_order() {
        let models = [
            model("openai", "gpt-a"),
            model("anthropic", "claude-a"),
            model("openai", "gpt-b"),
        ];
        let groups = group_by_provider(&models);
        assert_eq!(
            groups
                .iter()
                .map(|(p, m)| (p.as_str(), m.len()))
                .collect::<Vec<_>>(),
            [("openai", 2), ("anthropic", 1)]
        );
    }
}
