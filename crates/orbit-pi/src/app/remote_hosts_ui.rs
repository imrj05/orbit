//! Settings → Remote hosts: the machines an agent-only session can run on.
//!
//! Modelled on t3code's environment panel and built from Orbit's MCP page
//! anatomy: a pinned toolbar with **Add host**, one board of host rows (name,
//! `user@host:port`, the remote directory, the last connection check) with
//! Test / Edit / Remove, and an add/edit modal. The form takes an SSH host or
//! `~/.ssh/config` alias (offered as suggestions), an optional user and port,
//! the remote directory, and a display name; **Test connection** runs the
//! read-only probe against the values on screen before anything is saved.
//!
//! Every save goes through `remote_hosts::upsert`, which writes only the keys
//! Orbit owns, so hand edits to `remote-hosts.json` survive. Probes run on the
//! background executor; a result is dropped when the form or row it was for
//! has moved on (see `check_gen`).

use super::mcp_ui::mcp_text_button;
use super::*;
use crate::ui::chip::{ChipButton, ChipStyle};
use crate::ui::popover::{dropdown_below, DropdownAlign};
use crate::ui::button::{Button, ButtonVariant};
use crate::remote_hosts::{self, RemoteHost};
use crate::theme::tokens::{input, modal, ButtonSize, DynamicSpacing, IconSize, TextSize};
use crate::ui::button::IconButton;
use crate::ui::dialog::modal_button;
use crate::ui::field::TextField;
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::prelude::*;
use crate::ui::banner::{Banner, BannerTone};

/// How many `~/.ssh/config` aliases the form offers at once.
const MAX_ALIAS_SUGGESTIONS: usize = 8;

/// The last connection check for a host or for the open form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostCheck {
    Checking,
    /// What `pi --version` printed on the host.
    Reachable(String),
    /// The last line ssh or the host wrote.
    Failed(String),
    /// The server only lets `user` log in (the cloud root guard); the row
    /// offers to switch to it.
    WrongUser {
        message: String,
        user: String,
    },
    /// Connected, but pi is not installed for this account; the row offers
    /// the install command.
    PiMissing {
        command: String,
    },
    /// pi's installer is running on the host.
    Installing,
}

/// The add/edit modal's open state. Inputs are `ComposerInput` entities, like
/// the MCP form, so selection, IME and clipboard behave like every field.
pub(crate) struct RemoteHostEditor {
    /// The host being edited (its saved name), `None` when adding.
    pub(super) original: Option<String>,
    pub(super) name: Entity<ComposerInput>,
    pub(super) ssh: Entity<ComposerInput>,
    pub(super) user: Entity<ComposerInput>,
    pub(super) port: Entity<ComposerInput>,
    pub(super) dir: Entity<ComposerInput>,
    /// The project on this Mac the entry belongs to (optional).
    pub(super) local_dir: Entity<ComposerInput>,
    /// `~/.ssh/config` aliases, read once when the form opens.
    pub(super) aliases: Vec<String>,
    pub(super) error: Option<String>,
    pub(super) check: Option<HostCheck>,
    /// Bumped by every Test, so a slow probe for older field values cannot
    /// overwrite a newer result.
    pub(super) check_gen: u64,
}

impl RemoteHostEditor {
    fn composing(&self, cx: &App) -> bool {
        crate::composer::any_composing(
            cx,
            &[
                &self.name,
                &self.ssh,
                &self.user,
                &self.port,
                &self.dir,
                &self.local_dir,
            ],
        )
    }
}

/// Aliases that match what is typed in the host field (prefix matches
/// first), without the one already typed in full.
pub(crate) fn alias_suggestions(aliases: &[String], typed: &str) -> Vec<String> {
    let needle = typed.trim().to_lowercase();
    let mut prefix = Vec::new();
    let mut inner = Vec::new();
    for alias in aliases {
        let lower = alias.to_lowercase();
        if lower == needle {
            continue;
        }
        if needle.is_empty() || lower.starts_with(&needle) {
            prefix.push(alias.clone());
        } else if lower.contains(&needle) {
            inner.push(alias.clone());
        }
    }
    prefix.extend(inner);
    prefix.truncate(MAX_ALIAS_SUGGESTIONS);
    prefix
}

/// Turn the form's text into a host, or the first problem with it in words a
/// person can act on. The display name defaults to the host when left empty.
pub(crate) fn host_from_fields(
    name: &str,
    ssh: &str,
    user: &str,
    port: &str,
    dir: &str,
    local_dir: &str,
) -> Result<RemoteHost, String> {
    let ssh = ssh.trim();
    if ssh.is_empty() {
        return Err(tr!("remote_hosts.error_host_required"));
    }
    let port = match port.trim() {
        "" => None,
        text => match text.parse::<u16>() {
            Ok(port) if port > 0 => Some(port),
            _ => return Err(tr!("remote_hosts.error_port")),
        },
    };
    let dir = dir.trim();
    if dir.is_empty() {
        return Err(tr!("remote_hosts.error_dir_required"));
    }
    if !dir.starts_with('/') {
        return Err(tr!("remote_hosts.error_dir_absolute"));
    }
    let name = match name.trim() {
        "" => ssh.rsplit('@').next().unwrap_or(ssh).to_string(),
        name => name.to_string(),
    };
    let user = Some(user.trim().to_string()).filter(|user| !user.is_empty());
    let local_dir = match local_dir.trim() {
        "" => None,
        "/" => Some("/".to_string()),
        local if local.starts_with('/') || local == "~" || local.starts_with("~/") => {
            Some(local.trim_end_matches('/').to_string())
        }
        _ => return Err(tr!("remote_hosts.error_local_dir_absolute")),
    };
    let host = RemoteHost {
        name,
        ssh: ssh.to_string(),
        user,
        port,
        dir: dir.to_string(),
        enabled: true,
        local_dir,
    };
    host.target().map_err(|err| format!("{err:#}"))?;
    Ok(host)
}

impl OrbitApp {
    /// Re-read `remote-hosts.json` into the page's state.
    pub(super) fn reload_remote_hosts(&mut self) {
        let (hosts, skipped) = remote_hosts::load();
        // A check describes a host by name; drop the ones for hosts that left.
        self.remote_host_checks
            .retain(|name, _| hosts.iter().any(|host| &host.name == name));
        self.remote_hosts = hosts;
        self.remote_hosts_skipped = skipped;
    }

    pub(super) fn remote_hosts_section_opened(&mut self, cx: &mut Context<Self>) {
        self.reload_remote_hosts();
        cx.notify();
    }

    pub(super) fn remote_host_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_remote_host_editor(None, window, cx);
    }

    pub(super) fn remote_host_edit(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = self
            .remote_hosts
            .iter()
            .find(|host| host.name == name)
            .cloned();
        if host.is_some() {
            self.open_remote_host_editor(host, window, cx);
        }
    }

    fn open_remote_host_editor(
        &mut self,
        host: Option<RemoteHost>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field =
            |id: &'static str, placeholder: &'static str, text: String, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    ComposerInput::new(cx)
                        .with_element_id(id)
                        .with_key_context("Composer Picker")
                        .with_max_lines(1)
                        .with_placeholder_key(placeholder)
                        .with_text(text)
                })
            };
        let text = |pick: fn(&RemoteHost) -> String| host.as_ref().map(pick).unwrap_or_default();
        let ssh = field(
            "remote-host-ssh",
            "remote_hosts.ssh_placeholder",
            text(|h| h.ssh.clone()),
            cx,
        );
        let user = field(
            "remote-host-user",
            "remote_hosts.user_placeholder",
            text(|h| h.user.clone().unwrap_or_default()),
            cx,
        );
        let port = field(
            "remote-host-port",
            "remote_hosts.port_placeholder",
            text(|h| h.port.map(|p| p.to_string()).unwrap_or_default()),
            cx,
        );
        let dir = field(
            "remote-host-dir",
            "remote_hosts.dir_placeholder",
            text(|h| h.dir.clone()),
            cx,
        );
        let local_dir = field(
            "remote-host-local-dir",
            "remote_hosts.local_dir_placeholder",
            text(|h| h.local_dir.clone().unwrap_or_default()),
            cx,
        );
        let name = field(
            "remote-host-name",
            "remote_hosts.name_placeholder",
            text(|h| h.name.clone()),
            cx,
        );
        let target = ssh.read(cx).focus_handle(cx);
        self.remote_host_editor_focus.open(&target, window, cx);
        self.remote_host_remove_confirm = None;
        self.remote_host_editor = Some(RemoteHostEditor {
            original: host.map(|host| host.name),
            name,
            ssh,
            user,
            port,
            dir,
            local_dir,
            aliases: remote_hosts::discover_ssh_aliases(),
            error: None,
            check: None,
            check_gen: 0,
        });
        cx.notify();
    }

    pub(super) fn remote_host_editor_close(&mut self, cx: &mut Context<Self>) {
        if self.remote_host_editor.take().is_some() {
            cx.notify();
        }
    }

    fn remote_host_editor_cancel(
        &mut self,
        _: &crate::PickerCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_host_editor_close(cx);
    }

    fn remote_host_editor_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_host_editor_save(cx);
    }

    /// The form's current values as a host, or the reason they are not one.
    pub(super) fn remote_host_editor_host(&self, cx: &App) -> Option<Result<RemoteHost, String>> {
        let editor = self.remote_host_editor.as_ref()?;
        let read = |input: &Entity<ComposerInput>| input.read(cx).text().to_string();
        Some(host_from_fields(
            &read(&editor.name),
            &read(&editor.ssh),
            &read(&editor.user),
            &read(&editor.port),
            &read(&editor.dir),
            &read(&editor.local_dir),
        ))
    }

    pub(super) fn remote_host_editor_save(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.remote_host_editor.as_ref() else {
            return;
        };
        // Save reads every field; a field mid-composition still holds the
        // preedit, so ⏎ waits for the IME (issue #100).
        if editor.composing(cx) {
            return;
        }
        let original = editor.original.clone();
        // The form has no toggle: an edit keeps the host on or off as it was.
        let enabled = original
            .as_ref()
            .and_then(|name| self.remote_hosts.iter().find(|host| &host.name == name))
            .is_none_or(|host| host.enabled);
        let result = self
            .remote_host_editor_host(cx)
            .unwrap_or_else(|| Err(String::new()))
            .and_then(|mut host| {
                host.enabled = enabled;
                remote_hosts::upsert(original.as_deref(), &host)
                    .map(|()| host)
                    .map_err(|err| format!("{err:#}"))
            });
        match result {
            Ok(host) => {
                // A live remote session on this host restarts against the new
                // values; the running process keeps the old ones until then.
                if let (Some(active), Some(original)) = (self.remote.as_mut(), &original) {
                    if &active.name == original {
                        *active = host.clone();
                    }
                }
                if let Some(check) = self
                    .remote_host_editor
                    .as_ref()
                    .and_then(|editor| editor.check.clone())
                    .filter(|check| *check != HostCheck::Checking)
                {
                    self.remote_host_checks.insert(host.name.clone(), check);
                }
                self.remote_host_editor = None;
                self.reload_remote_hosts();
                self.toast_success(tr!("remote_hosts.saved", host = host.name));
            }
            Err(error) => {
                if let Some(editor) = self.remote_host_editor.as_mut() {
                    editor.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    /// Test the values on screen, before they are saved. A host that wants
    /// a password gets the password dialog (see `ssh_auth_ui`).
    pub(super) fn remote_host_editor_test(&mut self, cx: &mut Context<Self>) {
        let Some(result) = self.remote_host_editor_host(cx) else {
            return;
        };
        let Some(editor) = self.remote_host_editor.as_mut() else {
            return;
        };
        let host = match result {
            Ok(host) => host,
            Err(error) => {
                editor.error = Some(error);
                cx.notify();
                return;
            }
        };
        // A new generation: a slower result for older field values is dropped.
        editor.check_gen += 1;
        let generation = editor.check_gen;
        self.run_remote_probe(
            host,
            super::ssh_auth_ui::AuthThen::EditorTest(generation),
            0,
            cx,
        );
    }

    /// Test a saved host from its row.
    pub(super) fn remote_host_test(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(host) = self
            .remote_hosts
            .iter()
            .find(|host| host.name == name)
            .cloned()
        else {
            return;
        };
        if matches!(
            self.remote_host_checks.get(&name),
            Some(HostCheck::Checking | HostCheck::Installing)
        ) {
            return;
        }
        self.run_remote_probe(host, super::ssh_auth_ui::AuthThen::RowTest(name), 0, cx);
    }

    /// Save `user` as the account for the host named `name` (the server asked
    /// for it), then test again.
    pub(super) fn remote_host_use_user(
        &mut self,
        name: String,
        user: String,
        cx: &mut Context<Self>,
    ) {
        let Some(mut host) = self
            .remote_hosts
            .iter()
            .find(|host| host.name == name)
            .cloned()
        else {
            return;
        };
        host.user = Some(user);
        match remote_hosts::upsert(Some(&name), &host) {
            Ok(()) => {
                if let Some(active) = self.remote.as_mut().filter(|active| active.name == name) {
                    *active = host.clone();
                }
                self.reload_remote_hosts();
                self.remote_host_checks.remove(&name);
                self.remote_host_test(name, cx);
            }
            Err(err) => {
                self.remote_host_checks
                    .insert(name, HostCheck::Failed(format!("{err:#}")));
                cx.notify();
            }
        }
    }

    /// The form's version: fill the Username field and test again.
    pub(super) fn remote_host_editor_use_user(&mut self, user: String, cx: &mut Context<Self>) {
        let Some(editor) = self.remote_host_editor.as_ref() else {
            return;
        };
        let field = editor.user.clone();
        field.update(cx, |input, cx| input.set_text(user, cx));
        self.remote_host_editor_test(cx);
    }

    /// Open or close a card's ⋯ menu. The press that lands on ⋯ while its menu
    /// is open first dismisses the menu as an outside click (mouse-down); its
    /// mouse-up must not re-open it, so the gesture guard swallows it — the
    /// same rule every other menu trigger follows.
    pub(super) fn toggle_remote_host_menu(&mut self, name: String, cx: &mut Context<Self>) {
        if self.menu_dismissed_at.swallows() {
            return;
        }
        self.remote_host_menu = if self.remote_host_menu.as_deref() == Some(name.as_str()) {
            None
        } else {
            Some(name)
        };
        cx.notify();
    }

    /// Turn a host on or off. Off keeps it saved but out of the folder picker.
    pub(super) fn remote_host_toggle_enabled(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(mut host) = self
            .remote_hosts
            .iter()
            .find(|host| host.name == name)
            .cloned()
        else {
            return;
        };
        host.enabled = !host.enabled;
        match remote_hosts::upsert(Some(&name), &host) {
            Ok(()) => self.reload_remote_hosts(),
            Err(err) => self.toast_error(format!("{err:#}")),
        }
        cx.notify();
    }

    pub(super) fn remote_host_request_remove(&mut self, name: String, cx: &mut Context<Self>) {
        self.remote_host_remove_confirm = Some(name);
        cx.notify();
    }

    pub(super) fn remote_host_cancel_remove(&mut self, cx: &mut Context<Self>) {
        self.remote_host_remove_confirm = None;
        cx.notify();
    }

    pub(super) fn remote_host_confirm_remove(&mut self, name: String, cx: &mut Context<Self>) {
        self.remote_host_remove_confirm = None;
        match remote_hosts::remove(&name) {
            Ok(()) => {
                self.reload_remote_hosts();
                self.toast_info(tr!("remote_hosts.removed", host = name));
            }
            Err(err) => self.toast_error(format!("{err:#}")),
        }
        cx.notify();
    }

    /// Fill the host field from a `~/.ssh/config` suggestion, and the display
    /// name too while it is still empty.
    pub(super) fn remote_host_pick_alias(&mut self, alias: String, cx: &mut Context<Self>) {
        let Some(editor) = self.remote_host_editor.as_ref() else {
            return;
        };
        let (ssh, name) = (editor.ssh.clone(), editor.name.clone());
        ssh.update(cx, |input, cx| input.set_text(alias.clone(), cx));
        if name.read(cx).text().trim().is_empty() {
            name.update(cx, |input, cx| input.set_text(alias, cx));
        }
        cx.notify();
    }

    // ── rendering ─────────────────────────────────────────────────────────

    pub(super) fn remote_hosts_toolbar(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        // The button's own fade (`faded`), not a `motion::hover` wrapped around
        // a button that already carries its hover: under Reduce Motion the
        // wrapper falls back to a second `.hover()` and GPUI panics.
        let add = Button::new("remote-host-add", ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .icon("icons/plus.svg")
            .label(tr!("remote_hosts.add"))
            .faded(&theme, move |el| {
                el.on_mouse_up(MouseButton::Left, move |_, window, cx| {
                    this.update(cx, |app, cx| app.remote_host_add(window, cx))
                })
                .debug_selector(|| "remote-host-add".to_string())
            });
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .child(add)
            .into_any_element()
    }

    pub(super) fn remote_hosts_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        for reason in &self.remote_hosts_skipped {
            rows.push(self.settings_section(
                theme,
                &tr!("remote_hosts.skipped_title"),
                vec![self.setting_row(
                    theme,
                    reason,
                    Some(&tr!("remote_hosts.skipped_hint")),
                    None,
                    None,
                )],
            ));
        }
        if self.remote_hosts.is_empty() {
            rows.push(self.remote_hosts_empty(theme, this));
            return rows;
        }
        // One card per host, after t3code's environment panel: each machine
        // reads as its own object with its own toggle and menu.
        let mut cards = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .px(DynamicSpacing::Base04.px(&theme))
                    .text_size(TextSize::Default.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(tr!("remote_hosts.list_hint")),
            );
        for host in &self.remote_hosts {
            cards = cards.child(self.settings_group(
                theme,
                vec![self.remote_host_card(host, theme, this.clone())],
            ));
        }
        rows.push(cards.into_any_element());
        rows
    }

    fn remote_hosts_empty(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let add = mcp_text_button(
            "remote-host-empty-add".into(),
            tr!("remote_hosts.add"),
            theme,
            true,
            this,
            |app, window, cx| app.remote_host_add(window, cx),
        );
        self.settings_group(
            theme,
            vec![div()
                .w_full()
                .px(DynamicSpacing::Base16.px(&theme))
                .py(DynamicSpacing::Base24.px(&theme))
                .flex()
                .flex_col()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(icon(
                    "icons/server-stack.svg",
                    IconSize::XLarge.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(tr!("remote_hosts.empty_title")),
                )
                .child(
                    div()
                        .max_w(px(420.))
                        .text_center()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("remote_hosts.empty_body")),
                )
                .child(div().pt(DynamicSpacing::Base08.px(&theme)).child(add))
                .into_any_element()],
        )
    }

    /// The status word after the connection in a card's subtitle, and its
    /// ink: words carry the meaning, colour only reinforces it.
    fn remote_host_status(&self, host: &RemoteHost, theme: Theme) -> (String, Hsla) {
        if !host.enabled {
            return (tr!("remote_hosts.status_off"), theme.text_muted);
        }
        match self.remote_host_checks.get(&host.name) {
            None => (tr!("remote_hosts.status_unchecked"), theme.text_muted),
            Some(HostCheck::Checking) => (tr!("remote_hosts.checking"), theme.text_secondary),
            Some(HostCheck::Installing) => {
                (tr!("remote_hosts.status_installing"), theme.text_secondary)
            }
            Some(HostCheck::Reachable(_)) => (tr!("remote_hosts.reachable"), theme.success),
            Some(HostCheck::PiMissing { .. }) => {
                (tr!("remote_hosts.status_pi_missing"), theme.error)
            }
            Some(HostCheck::Failed(_) | HostCheck::WrongUser { .. }) => {
                (tr!("remote_hosts.status_failed"), theme.error)
            }
        }
    }

    /// One host as a t3code-style card: identity and `SSH user@host:port ·
    /// status` on the left, the on/off switch and the ⋯ menu on the right;
    /// below, the directory and — when the last check found a problem — what
    /// it was and the fix (Install pi, Use opc, …).
    fn remote_host_card(
        &self,
        host: &RemoteHost,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let name = host.name.clone();
        let confirming = self.remote_host_remove_confirm.as_deref() == Some(name.as_str());
        let active = self.remote_host_name() == Some(name.as_str());
        let check = self.remote_host_checks.get(&name);
        let menu_open = self.remote_host_menu.as_deref() == Some(name.as_str());
        let (status, status_ink) = self.remote_host_status(host, theme);
        // Dim only the identity when off, so the controls stay readable.
        let identity_opacity = if host.enabled { 1.0 } else { 0.6 };

        let title = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Large.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(name.clone()),
            )
            .when(active, |line| {
                line.child(
                    div()
                        .flex_none()
                        .px(DynamicSpacing::Base06.px(&theme))
                        .rounded(crate::theme::tokens::Radius::Small.px(&theme))
                        .bg(theme.success.opacity(0.12))
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.success)
                        .child(tr!("remote_hosts.in_use")),
                )
            });
        let subtitle = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .text_size(TextSize::Default.px(&theme))
            .text_color(theme.text_secondary)
            .child(tr!(
                "remote_hosts.ssh_label",
                target = host.connection_label()
            ))
            .child(div().text_color(theme.text_muted).child("·"))
            .child(div().text_color(status_ink).child(status));

        let switch_name = name.clone();
        let menu_name = name.clone();
        let menu_this = this.clone();
        // The button's own fade (`faded`), as for Add host: wrapping it in a
        // second `motion::hover` panics under Reduce Motion.
        let menu = menu_open.then(|| remote_host_menu(&name, theme, this.clone()));
        let more_selector = format!("remote-host-more-{name}");
        let more = IconButton::new(
            ElementId::Name(format!("remote-host-more-{name}").into()),
            "icons/more.svg",
        )
        .open(menu_open)
        .ink(theme.text_secondary)
        .tip(tr!("remote_hosts.more"))
        .faded(&theme, move |button| {
            button
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    menu_this.update(cx, |app, cx| {
                        app.toggle_remote_host_menu(menu_name.clone(), cx)
                    });
                })
                // The menu's zero-size anchor is positioned against this box.
                .relative()
                .debug_selector(move || more_selector.clone())
                .children(menu)
        });

        let header = div()
            .w_full()
            .flex()
            .items_start()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .flex_none()
                    .pt(DynamicSpacing::Base04.px(&theme))
                    .opacity(identity_opacity)
                    .child(icon(
                        "icons/server-stack.svg",
                        IconSize::Medium.px(&theme),
                        theme.text_muted,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .opacity(identity_opacity)
                    .child(title)
                    .child(subtitle),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(switch(
                        format!("remote-host-enabled-{name}"),
                        host.enabled,
                        &theme,
                        {
                            let this = this.clone();
                            move |_, cx| {
                                this.update(cx, |app, cx| {
                                    app.remote_host_toggle_enabled(switch_name.clone(), cx)
                                })
                            }
                        },
                    ))
                    .child(more),
            );

        // The detail area under a hairline: where it runs, and what the last
        // check found when it needs a decision.
        let mut detail = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.text_muted)
                            .child(tr!("remote_hosts.dir_label")),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(theme::code_font_family())
                            .text_color(theme.text_secondary)
                            .tip(host.dir.clone())
                            .child(host.dir.clone()),
                    ),
            );
        if let Some(check) = check.filter(|check| !matches!(check, HostCheck::Reachable(_))) {
            detail = detail.child(check_line(&format!("row-{name}"), check, theme));
            if let HostCheck::PiMissing { command } = check {
                detail = detail.child(pi_install_hint(
                    &name,
                    super::ssh_auth_ui::InstallTarget::Row(name.clone()),
                    command,
                    theme,
                    this.clone(),
                ));
            }
            if let HostCheck::WrongUser { user, .. } = check {
                let (row_name, user) = (name.clone(), user.clone());
                detail = detail.child(div().flex().child(mcp_text_button(
                    format!("remote-host-use-user-{row_name}"),
                    tr!("remote_hosts.use_user", user = user.clone()),
                    theme,
                    true,
                    this.clone(),
                    move |app, _, cx| app.remote_host_use_user(row_name.clone(), user.clone(), cx),
                )));
            }
        }
        if let Some(HostCheck::Reachable(version)) = check {
            if !version.is_empty() {
                detail = detail.child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!(
                            "remote_hosts.reachable_version",
                            version = version.clone()
                        )),
                );
            }
        }
        if confirming {
            let (cancel_name, remove_name) = (name.clone(), name.clone());
            detail = detail.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!("remote_hosts.remove_confirm")),
                    )
                    .child(mcp_text_button(
                        format!("remote-host-remove-cancel-{cancel_name}"),
                        tr!("remote_hosts.cancel"),
                        theme,
                        false,
                        this.clone(),
                        |app, _, cx| app.remote_host_cancel_remove(cx),
                    ))
                    .child(mcp_text_button(
                        format!("remote-host-remove-confirm-{remove_name}"),
                        tr!("remote_hosts.remove"),
                        theme,
                        true,
                        this,
                        move |app, _, cx| app.remote_host_confirm_remove(remove_name.clone(), cx),
                    )),
            );
        }

        div()
            .id(ElementId::Name(format!("remote-host-row-{name}").into()))
            .debug_selector({
                let name = name.clone();
                move || format!("remote-host-row-{name}")
            })
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(header)
            .child(
                div()
                    // Indented to the text column, past the icon, like t3code.
                    .ml(IconSize::Medium.px(&theme) + DynamicSpacing::Base12.px(&theme))
                    .pt(DynamicSpacing::Base12.px(&theme))
                    .border_t_1()
                    .border_color(theme.hairline())
                    .child(detail),
            )
            .into_any_element()
    }

    pub(super) fn remote_host_editor_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let editor = self.remote_host_editor.as_ref()?;
        let title = if editor.original.is_some() {
            tr!("remote_hosts.edit_title")
        } else {
            tr!("remote_hosts.add_title")
        };

        let field =
            |label: String, hint: Option<String>, input: Entity<ComposerInput>| -> AnyElement {
                div()
                    .flex()
                    .flex_col()
                    .gap(input::gap(&theme))
                    .child(
                        input_label(label, &theme)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary),
                    )
                    .child(TextField::new(input, theme.background).render(&theme))
                    .children(hint.map(|hint| {
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(hint)
                    }))
                    .into_any_element()
            };

        // `~/.ssh/config` aliases matching the host field, as chips under it.
        let suggestions = alias_suggestions(&editor.aliases, &editor.ssh.read(cx).text());
        let mut host_field = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(field(
                tr!("remote_hosts.ssh_label_field"),
                Some(tr!("remote_hosts.ssh_hint")),
                editor.ssh.clone(),
            ));
        if !suggestions.is_empty() {
            let mut chips = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("remote_hosts.from_ssh_config")),
                );
            for (ix, alias) in suggestions.into_iter().enumerate() {
                let this = this.clone();
                chips = chips.child(ChipButton::new(format!("remote-host-alias-{ix}"), ChipStyle::Outlined { inset: false })
                            .size(ButtonSize::Default)
                            .label(alias.clone())
                            .mono()
                            .caret(false)
                            .on_mouse_up(&theme, {
                        let alias = alias.clone();
                        move |_, _, cx| {
                            this.update(cx, |app, cx| app.remote_host_pick_alias(alias.clone(), cx))
                        }
                    }));
            }
            host_field = host_field.child(chips);
        }

        let user_port = div()
            .flex()
            .items_start()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(div().flex_1().min_w_0().child(field(
                tr!("remote_hosts.user_label"),
                None,
                editor.user.clone(),
            )))
            .child(div().w(relative(0.3)).flex_none().child(field(
                tr!("remote_hosts.port_label"),
                None,
                editor.port.clone(),
            )));

        let mut body = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base16.px(&theme))
            .child(host_field)
            .child(user_port)
            .child(field(
                tr!("remote_hosts.dir_label"),
                Some(tr!("remote_hosts.dir_hint")),
                editor.dir.clone(),
            ))
            .child(field(
                tr!("remote_hosts.local_dir_label"),
                Some(tr!("remote_hosts.local_dir_hint")),
                editor.local_dir.clone(),
            ))
            .child(field(
                tr!("remote_hosts.name_label"),
                Some(tr!("remote_hosts.name_hint")),
                editor.name.clone(),
            ));
        if let Some(error) = &editor.error {
            body = body.child(
                Banner::note(BannerTone::Error, error.clone(), &theme),
            );
        }

        let testing = editor.check == Some(HostCheck::Checking);
        let test = mcp_text_button(
            "remote-host-editor-test".into(),
            tr!("remote_hosts.test_connection"),
            theme,
            false,
            this.clone(),
            move |app, _, cx| {
                if !testing {
                    app.remote_host_editor_test(cx);
                }
            },
        );
        let cancel = modal_button(
            "remote-host-editor-cancel",
            tr!("remote_hosts.cancel"),
            ButtonVariant::Secondary,
            &self.modal_cancel_focus,
        )
        .size(ButtonSize::Large)
        // The buttons' own fade, not a second `motion::hover` (it panics under
        // Reduce Motion: the button already carries a hover).
        .faded(&theme, {
            let this = this.clone();
            move |el| {
                el.on_click(move |_, _, cx| {
                    this.update(cx, |app, cx| app.remote_host_editor_close(cx))
                })
            }
        });
        let save = modal_button(
            "remote-host-editor-save",
            tr!("remote_hosts.save"),
            ButtonVariant::Primary,
            &self.modal_confirm_focus,
        )
        .size(ButtonSize::Large)
        .faded(&theme, {
            let this = this.clone();
            move |el| {
                el.on_click(move |_, _, cx| {
                    this.update(cx, |app, cx| app.remote_host_editor_save(cx))
                })
            }
        });

        let footer = modal_footer(&theme)
            .child(test)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .children(
                        editor
                            .check
                            .as_ref()
                            .map(|check| check_line("editor", check, theme)),
                    )
                    .children(match &editor.check {
                        Some(HostCheck::PiMissing { command }) => Some(pi_install_hint(
                            "editor",
                            super::ssh_auth_ui::InstallTarget::Editor,
                            command,
                            theme,
                            this.clone(),
                        )),
                        Some(HostCheck::WrongUser { user, .. }) => {
                            let user = user.clone();
                            Some(mcp_text_button(
                                "remote-host-editor-use-user".into(),
                                tr!("remote_hosts.use_user", user = user.clone()),
                                theme,
                                true,
                                this.clone(),
                                move |app, _, cx| app.remote_host_editor_use_user(user.clone(), cx),
                            ))
                        }
                        _ => None,
                    }),
            )
            .child(cancel)
            .child(save);

        let card = modal_card(&theme)
            .debug_selector(|| "remote-host-editor".to_string())
            .modal_width(px(560.))
            .max_h(relative(1.))
            .key_context("Picker")
            .on_action(cx.listener(Self::remote_host_editor_cancel))
            .on_action(cx.listener(Self::remote_host_editor_confirm))
            .child(modal_header(
                &theme,
                title,
                Some(ModalSubtitle::Text(
                    tr!("remote_hosts.editor_subtitle").into(),
                )),
            ))
            .child(
                div()
                    .id("remote-host-editor-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body),
            )
            .child(footer);

        Some(
            modal_scrim(&theme)
                .id("remote-host-editor-layer")
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.remote_host_editor_close(cx))
                })
                .child(crate::motion::enter(
                    card,
                    "remote-host-editor-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }
}

/// What one ⋯ menu entry does when chosen.
type MenuAction = Box<dyn Fn(&mut OrbitApp, &mut Window, &mut Context<OrbitApp>)>;

/// The card's ⋯ menu: Test connection, Edit, Remove. Anchored under its
/// button like the Worktrees row menu; an outside press dismisses it.
fn remote_host_menu(name: &str, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
    let theme = theme.for_floating();
    let entry = |id: String,
                 glyph: &'static str,
                 label: String,
                 this: Entity<OrbitApp>,
                 action: MenuAction| {
        let row = picker_entry(div().id(ElementId::Name(id.clone().into())), &theme)
            .h(crate::theme::tokens::picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .child(icon(
                glyph,
                crate::theme::tokens::context_menu::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.text_secondary)
                    .child(label),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                this.update(cx, |app, cx| {
                    app.remote_host_menu = None;
                    action(app, window, cx);
                    cx.notify();
                });
            });
        crate::motion::hover(row, id, &theme, crate::motion::Hover::wash(&theme))
    };
    let (test, edit, remove) = (name.to_string(), name.to_string(), name.to_string());
    let clone_to = name.to_string();
    let body = div()
        .py(crate::theme::tokens::picker::list_padding_y(&theme))
        .flex()
        .flex_col()
        .child(entry(
            format!("remote-host-menu-test-{name}"),
            "icons/refresh.svg",
            tr!("remote_hosts.test_connection"),
            this.clone(),
            Box::new(move |app, _, cx| app.remote_host_test(test.clone(), cx)),
        ))
        .child(entry(
            format!("remote-host-menu-edit-{name}"),
            "icons/pencil.svg",
            tr!("remote_hosts.edit"),
            this.clone(),
            Box::new(move |app, window, cx| app.remote_host_edit(&edit, window, cx)),
        ))
        .child(entry(
            format!("remote-host-menu-clone-{name}"),
            "icons/cloud-upload.svg",
            tr!("server_clone.menu_item"),
            this.clone(),
            Box::new(move |app, window, cx| {
                app.open_server_clone(Some(clone_to.clone()), window, cx)
            }),
        ))
        .child(context_menu_separator(&theme))
        .child(entry(
            format!("remote-host-menu-remove-{name}"),
            "icons/trash.svg",
            tr!("remote_hosts.remove"),
            this.clone(),
            Box::new(move |app, _, cx| app.remote_host_request_remove(remove.clone(), cx)),
        ));
    let selector = format!("remote-host-menu-{name}");
    let popup = popover_picker(div(), &theme)
        .debug_selector(move || selector.clone())
        // Sized to its longest label, so "Add a project from this Mac…" (and
        // its longer translations) is not cut off.
        .min_w(crate::theme::tokens::context_menu::MIN_WIDTH)
        .dismiss_on_outside(move |_, _, cx| {
            this.update(cx, |app, cx| {
                app.menu_dismissed_at.arm();
                app.remote_host_menu = None;
                cx.notify();
            })
        })
        .child(body);
    // Hung from a zero-size anchor at the "…" button's bottom-right corner.
    // An `anchored` dropped straight into the button's flex flow took the
    // static position of its own menu child centred in the 28px button, so
    // the menu opened ~100px to the left of its trigger and above it.
    dropdown_below(
        DropdownAlign::End,
        crate::theme::tokens::popover::MENU_OFFSET,
        popup,
        format!("remote-host-menu-in-{name}"),
        &theme,
    )
}

/// pi is missing: **Install pi** runs pi's own installer on the host (plus
/// Node.js 22 from nodejs.org when the host has none), and **Copy command**
/// keeps the manual route. The line above the buttons says exactly what the
/// install does, because it changes the server.
fn pi_install_hint(
    id: &str,
    target: super::ssh_auth_ui::InstallTarget,
    command: &str,
    theme: Theme,
    this: Entity<OrbitApp>,
) -> AnyElement {
    let copied = command.to_string();
    div()
        .pt(DynamicSpacing::Base04.px(&theme))
        .flex()
        .flex_col()
        // Buttons keep their own width: a stretched flex column made the
        // copy button span the whole row.
        .items_start()
        .gap(DynamicSpacing::Base06.px(&theme))
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_secondary)
                .child(tr!("remote_hosts.install_explained")),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .debug_selector({
                    let id = id.to_string();
                    move || format!("remote-host-install-actions-{id}")
                })
                .child(mcp_text_button(
                    format!("remote-host-install-{id}"),
                    tr!("remote_hosts.install_pi"),
                    theme,
                    true,
                    this.clone(),
                    move |app, _, cx| app.remote_host_install(target.clone(), cx),
                ))
                .child(mcp_text_button(
                    format!("remote-host-copy-install-{id}"),
                    tr!("remote_hosts.copy_install"),
                    theme,
                    false,
                    this,
                    move |app, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                        app.toast_info(tr!("remote_hosts.install_copied"));
                        cx.notify();
                    },
                )),
        )
        .into_any_element()
}

/// One line saying what the last check found. Status color reinforces the
/// words, never replaces them.
fn check_line(id: &str, check: &HostCheck, theme: Theme) -> AnyElement {
    let (glyph, color, text): (AnyElement, Hsla, String) = match check {
        HostCheck::Checking => (
            spinner(
                ElementId::Name(format!("remote-host-check-{id}").into()),
                IconSize::XSmall.px(&theme),
                theme.text_muted,
                theme,
            ),
            theme.text_muted,
            tr!("remote_hosts.checking"),
        ),
        HostCheck::Reachable(version) => (
            icon(
                "icons/circle-check.svg",
                IconSize::XSmall.px(&theme),
                theme.success,
            )
            .into_any_element(),
            theme.text_secondary,
            if version.is_empty() {
                tr!("remote_hosts.reachable")
            } else {
                tr!("remote_hosts.reachable_version", version = version.clone())
            },
        ),
        HostCheck::Failed(error) => (
            icon(
                "icons/alert-circle.svg",
                IconSize::XSmall.px(&theme),
                theme.error,
            )
            .into_any_element(),
            theme.error,
            tr!("remote_hosts.failed", error = error.clone()),
        ),
        HostCheck::Installing => (
            spinner(
                ElementId::Name(format!("remote-host-installing-{id}").into()),
                IconSize::XSmall.px(&theme),
                theme.text_muted,
                theme,
            ),
            theme.text_secondary,
            tr!("remote_hosts.installing"),
        ),
        HostCheck::PiMissing { .. } => (
            icon(
                "icons/alert-circle.svg",
                IconSize::XSmall.px(&theme),
                theme.error,
            )
            .into_any_element(),
            theme.error,
            tr!("remote_hosts.pi_missing"),
        ),
        HostCheck::WrongUser { message, .. } => (
            icon(
                "icons/alert-circle.svg",
                IconSize::XSmall.px(&theme),
                theme.error,
            )
            .into_any_element(),
            theme.error,
            message.clone(),
        ),
    };
    div()
        .id(ElementId::Name(
            format!("remote-host-check-line-{id}").into(),
        ))
        .min_w_0()
        .flex()
        // A failure is read, not glanced at: it wraps instead of truncating,
        // and the glyph stays on its first line.
        .items_start()
        .gap(DynamicSpacing::Base04.px(&theme))
        .text_size(TextSize::Small.px(&theme))
        .text_color(color)
        .child(
            div()
                .flex_none()
                .pt(DynamicSpacing::Base02.px(&theme))
                .child(glyph),
        )
        .child(div().flex_1().min_w_0().child(text))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host card's "…" menu hangs under its trigger, right edges aligned.
    /// Mounted as a bare `anchored` inside the centred button, it took the
    /// static position of its own 220px body and opened ~100px to the left of
    /// the button and above it.
    #[gpui::test]
    fn host_menu_opens_under_its_trigger(cx: &mut gpui::TestAppContext) {
        // Reduced motion paints the menu at rest on the first frame.
        let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        cx.update(|cx| cx.set_global(theme));
        let (app, cx) = cx.add_window_view(|_, cx| OrbitApp::new(cx));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::RemoteHosts;
                // In memory only: nothing here reads or writes the host file.
                app.remote_hosts = vec![RemoteHost {
                    name: "prod".into(),
                    ssh: "vps".into(),
                    user: None,
                    port: None,
                    dir: "/srv/app".into(),
                    enabled: true,
                    local_dir: None,
                }];
                app.remote_hosts_skipped.clear();
                cx.notify();
            })
        });
        let draw = |cx: &mut gpui::VisualTestContext| {
            let _ = cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(1200.), px(820.)),
                |_, _| app.clone(),
            );
        };
        draw(cx);
        // The page says the feature is experimental, and what that means.
        assert!(
            cx.debug_bounds("experimental-badge").is_some(),
            "Remote hosts carries the Experimental tag"
        );
        assert!(
            cx.debug_bounds("remote-hosts-experimental-note").is_some(),
            "the page explains what Experimental means"
        );
        let more = cx
            .debug_bounds("remote-host-more-prod")
            .expect("the host card's menu button");
        cx.simulate_click(more.center(), gpui::Modifiers::none());
        draw(cx);
        let menu = cx
            .debug_bounds("remote-host-menu-prod")
            .expect("the host menu opened");
        assert!(
            menu.top() >= more.bottom(),
            "the menu must open below its trigger: menu {menu:?}, trigger {more:?}"
        );
        assert!(
            (menu.right() - more.right()).abs() <= px(1.),
            "the menu's right edge must line up with the trigger's: menu {menu:?}, trigger {more:?}"
        );
    }

    #[test]
    fn suggestions_rank_prefix_matches_first_and_skip_the_exact_alias() {
        let aliases: Vec<String> = ["prod", "staging", "prod-eu", "devbox", "my-prod"]
            .iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(alias_suggestions(&aliases, "prod"), ["prod-eu", "my-prod"]);
        assert_eq!(alias_suggestions(&aliases, "").len(), 5);
        assert!(alias_suggestions(&aliases, "zzz").is_empty());
    }

    #[test]
    fn fields_become_a_host_or_a_reason() {
        let host = host_from_fields("", "deploy@vps", "", "2222", "/srv/app", "").expect("valid");
        assert_eq!(host.name, "vps", "the name defaults to the host");
        assert_eq!(host.port, Some(2222));
        assert!(host.user.is_none());

        let host = host_from_fields("Prod", "vps", "deploy", "", " /srv ", "").expect("valid");
        assert_eq!(host.destination(), "deploy@vps");
        assert_eq!(host.dir, "/srv");

        assert!(
            host_from_fields("", "", "", "", "/srv", "").is_err(),
            "host required"
        );
        assert!(
            host_from_fields("", "vps", "", "abc", "/srv", "").is_err(),
            "port is a number"
        );
        assert!(
            host_from_fields("", "vps", "", "0", "/srv", "").is_err(),
            "port 0"
        );
        assert!(
            host_from_fields("", "vps", "", "", "", "").is_err(),
            "dir required"
        );
        assert!(
            host_from_fields("", "vps", "", "", "~/app", "").is_err(),
            "dir absolute"
        );
        assert!(
            host_from_fields("", "-oProxy=x", "", "", "/srv", "").is_err(),
            "no option injection"
        );

        let host =
            host_from_fields("", "vps", "", "", "/srv", " ~/code/app/ ").expect("valid");
        assert_eq!(host.local_dir.as_deref(), Some("~/code/app"));
        assert!(
            host_from_fields("", "vps", "", "", "/srv", "code/app").is_err(),
            "the local folder is absolute or under ~"
        );
    }
}
