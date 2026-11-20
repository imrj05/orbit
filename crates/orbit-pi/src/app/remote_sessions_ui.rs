//! Remote sessions in the sidebar: one group per enabled remote host entry,
//! listing the sessions that ran in its folder on that server, newest first.
//! Opening one resumes it **on the host** — the remote start (connection
//! check, password prompt, guard, workflow extension) followed by
//! `switch_session` with the host's own path, the same reattach a remote
//! restart uses. pi reads the transcript there; nothing is copied here.
//!
//! The lists come from `remote_hosts::list_sessions` (one read-only ssh run per
//! server, shared by every entry on that server) and are fetched at launch,
//! after a remote turn settles, on ⌘R, and from a group's refresh button.
//! Launch and settle fetches never ask for a password; the refresh button may.

use super::remote_ui::TaskPlace;
use super::ssh_auth_ui::AuthThen;
use super::*;
use crate::motion::Hover;
use crate::remote_hosts::{self, ProbeFailure, RemoteHost};
use crate::remote_sessions::{self, RemoteSession};
use crate::theme::tokens::{list_item, ButtonSize, DynamicSpacing, IconSize, TextSize};
use crate::ui::button::IconButton;
use crate::ui::prelude::*;

/// What a server's list is doing, beside the sessions it last returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RemoteListStatus {
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
}

/// A remote session row: everything it paints and what opening it needs.
pub(crate) struct RemoteSessionRow {
    pub(crate) host: RemoteHost,
    pub(crate) session: RemoteSession,
}

impl OrbitApp {
    /// Fetch every enabled server's session list. `interactive` lets a
    /// server that wants a password ask for one; the quiet fetches (launch,
    /// ⌘R) only note that it needs a login.
    pub(super) fn refresh_remote_sessions(&mut self, interactive: bool, cx: &mut Context<Self>) {
        // Tests build the app without a network: never read this machine's
        // host list or reach its servers from one.
        if cfg!(test) {
            return;
        }
        self.reload_remote_hosts();
        let mut seen = HashSet::new();
        let hosts: Vec<RemoteHost> = self
            .remote_hosts
            .iter()
            .filter(|host| host.enabled && seen.insert(host.account_key()))
            .cloned()
            .collect();
        for host in hosts {
            self.fetch_remote_sessions(host, 0, interactive, cx);
        }
    }

    /// Refetch the server the live remote session runs on: a settled turn may
    /// have made its session listable (pi lists a session once it has a
    /// message).
    pub(super) fn refresh_active_remote_sessions(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        if let Some(host) = self.remote.clone() {
            self.fetch_remote_sessions(host, 0, false, cx);
        }
    }

    pub(super) fn fetch_remote_sessions(
        &mut self,
        host: RemoteHost,
        attempt: u8,
        interactive: bool,
        cx: &mut Context<Self>,
    ) {
        let key = host.account_key();
        let status = self.remote_session_status.entry(key).or_default();
        if status.loading {
            return;
        }
        status.loading = true;
        cx.notify();
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let list_host = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { remote_hosts::list_sessions(&list_host, secret.as_deref()) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_sessions(host, attempt, interactive, outcome, cx);
            });
        })
        .detach();
    }

    pub(super) fn finish_remote_sessions(
        &mut self,
        host: RemoteHost,
        attempt: u8,
        interactive: bool,
        outcome: Result<Vec<RemoteSession>, ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        let key = host.account_key();
        let status = self.remote_session_status.entry(key.clone()).or_default();
        status.loading = false;
        match outcome {
            Ok(sessions) => {
                status.error = None;
                self.remote_session_lists.insert(key, sessions);
            }
            // The refresh button: password prompts, host keys and the rest
            // are explained the way every remote check explains them, and an
            // explanation lands on the group (see `remote_sessions_failed`).
            Err(failure) if interactive => {
                let name = host.name.clone();
                self.finish_remote_probe(
                    host,
                    AuthThen::SessionList(name),
                    attempt,
                    Err(failure),
                    cx,
                );
            }
            // A quiet fetch never asks: a server that wants a password says
            // so on its group, and its refresh button asks.
            Err(ProbeFailure::NeedsPassword { .. } | ProbeFailure::WrongPassword) => {
                status.error = Some(tr!("remote_sessions.needs_login"));
            }
            Err(failure) => {
                status.error = Some(super::remote_providers_ui::failure_text(failure, &host));
            }
        }
        cx.notify();
    }

    /// A refresh that could not list `name`'s server: say why on its group.
    pub(super) fn remote_sessions_failed(&mut self, name: &str, message: String) {
        let Some(host) = self.remote_hosts.iter().find(|host| host.name == name) else {
            return;
        };
        let status = self
            .remote_session_status
            .entry(host.account_key())
            .or_default();
        status.loading = false;
        status.error = Some(message);
    }

    /// The refresh button on a server group.
    pub(super) fn refresh_remote_host_sessions(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(host) = self
            .remote_hosts
            .iter()
            .find(|host| host.name == name)
            .cloned()
        {
            self.fetch_remote_sessions(host, 0, true, cx);
        }
    }

    pub(super) fn toggle_remote_session_group(&mut self, name: &str, cx: &mut Context<Self>) {
        if !self.remote_sessions_collapsed.remove(name) {
            self.remote_sessions_collapsed.insert(name.to_string());
        }
        cx.notify();
    }

    /// The sidebar rows for the servers: a header per enabled host entry and,
    /// unless it is collapsed, its newest sessions.
    pub(super) fn remote_session_rows(&self) -> Vec<SideRow> {
        let mut rows = Vec::new();
        for (host, sessions) in
            remote_sessions::group(&self.remote_hosts, &self.remote_session_lists)
        {
            rows.extend(self.remote_host_group_rows(host, &sessions));
        }
        rows
    }

    /// One host entry's rows: its header and, unless collapsed, its newest
    /// sessions.
    fn remote_host_group_rows(
        &self,
        host: &RemoteHost,
        sessions: &[&RemoteSession],
    ) -> Vec<SideRow> {
        let status = self
            .remote_session_status
            .get(&host.account_key())
            .cloned()
            .unwrap_or_default();
        let collapsed = self.remote_sessions_collapsed.contains(&host.name);
        let mut rows = vec![SideRow::RemoteHost {
            name: host.name.clone(),
            count: sessions.len(),
            collapsed,
            status,
        }];
        if !collapsed {
            rows.extend(
                sessions
                    .iter()
                    .take(remote_sessions::GROUP_VISIBLE)
                    .map(|session| remote_row(host, session)),
            );
        }
        rows
    }

    /// Lay the servers' rows into the workspace-grouped sidebar. An entry
    /// linked to a local project (`RemoteHost::local_dir`) lists inside that
    /// project's group, after its local sessions; a collapsed project keeps
    /// only the open remote session visible, like its local rows. Entries with
    /// no link, or linked to a project that is not listed, follow as groups
    /// of their own.
    pub(super) fn place_remote_session_rows(&self, rows: &mut Vec<SideRow>) {
        let mut unlinked = Vec::new();
        for (host, sessions) in
            remote_sessions::group(&self.remote_hosts, &self.remote_session_lists)
        {
            let header = host
                .local_path()
                .map(|path| crate::sessions::workspace_label(&path))
                .and_then(|label| {
                    rows.iter().position(
                        |row| matches!(row, SideRow::Workspace { label: l, .. } if *l == label),
                    )
                });
            let Some(header) = header else {
                unlinked.extend(self.remote_host_group_rows(host, &sessions));
                continue;
            };
            let end = rows[header + 1..]
                .iter()
                .position(|row| matches!(row, SideRow::Workspace { .. }))
                .map_or(rows.len(), |offset| header + 1 + offset);
            let group: Vec<SideRow> = match &rows[header] {
                SideRow::Workspace {
                    collapsed: true, ..
                } => sessions
                    .iter()
                    .filter(|session| self.is_active_remote_session(host, &session.path))
                    .map(|session| remote_row(host, session))
                    .collect(),
                _ => self.remote_host_group_rows(host, &sessions),
            };
            rows.splice(end..end, group);
        }
        rows.extend(unlinked);
    }

    /// Whether `session` on `host` is the one on screen.
    pub(super) fn is_active_remote_session(&self, host: &RemoteHost, path: &str) -> bool {
        self.remote
            .as_ref()
            .is_some_and(|live| live.account_key() == host.account_key())
            && self.current_session_path.as_deref() == Some(Path::new(path))
    }

    /// Open a remote session: check the connection (asking for a password
    /// when the server wants one), then resume it there.
    pub(super) fn open_remote_session(
        &mut self,
        host: RemoteHost,
        session: RemoteSession,
        cx: &mut Context<Self>,
    ) {
        if self.is_active_remote_session(&host, &session.path) {
            return;
        }
        // pi runs where the session ran, which may be below the entry's folder.
        let place = RemoteHost {
            dir: session.cwd.clone(),
            ..host
        };
        self.run_remote_probe(place, AuthThen::ResumeSession(session.path), 0, cx);
    }

    /// Connected: start pi on the host and point it at the session's file
    /// there. A resumed session keeps its own model and workflow mode, so the
    /// launch carries neither (see `remote_launch_workflow`).
    pub(super) fn resume_remote_session_on(
        &mut self,
        host: RemoteHost,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let name = host.name.clone();
        self.remote_resuming = true;
        self.start_task_at(TaskPlace::Remote(host), cx);
        self.remote_resuming = false;
        if self.client.is_none() {
            // The spawn failed; its banner says why.
            return;
        }
        self.default_model_armed = false;
        self.composer_is_new_task = false;
        self.current_session_path = Some(PathBuf::from(&path));
        self.send(
            CommandBody::SwitchSession { session_path: path },
            "switch_session",
        );
        self.set_status(tr!("remote_sessions.resumed", host = name));
        self.composer_focus_pending = true;
        cx.notify();
    }
}

fn remote_row(host: &RemoteHost, session: &RemoteSession) -> SideRow {
    SideRow::RemoteSession(Box::new(RemoteSessionRow {
        host: host.clone(),
        session: session.clone(),
    }))
}

/// A server group's header: the server glyph and the entry's name, then a
/// spinner while listing, the count, or the reason it could not list; the
/// refresh button fades in on hover. A press toggles the group.
pub(super) fn render_remote_host_row(
    name: &str,
    count: usize,
    collapsed: bool,
    status: &RemoteListStatus,
    this: &Entity<OrbitApp>,
    cursor: bool,
    theme: Theme,
) -> AnyElement {
    let toggle = this.clone();
    let refresh = this.clone();
    let (toggle_name, refresh_name) = (name.to_string(), name.to_string());
    let id = format!("remote-host-group-{name}");
    let trailing: AnyElement = if status.loading {
        spinner(
            ElementId::Name(format!("remote-host-group-loading-{name}").into()),
            IconSize::XSmall.px(&theme),
            theme.text_muted,
            theme,
        )
    } else if let Some(error) = &status.error {
        div()
            .id(ElementId::Name(
                format!("remote-host-group-error-{name}").into(),
            ))
            .flex_none()
            .tip(error.clone())
            .child(icon(
                "icons/alert-circle.svg",
                IconSize::XSmall.px(&theme),
                theme.warning,
            ))
            .into_any_element()
    } else if count > 0 {
        div()
            .flex_none()
            .min_w(ButtonSize::Compact.height(&theme))
            .flex()
            .justify_end()
            .text_size(TextSize::XSmall.px(&theme))
            .text_color(theme.text_muted)
            .child(format!("{count}"))
            .into_any_element()
    } else {
        div().into_any_element()
    };
    let selector = id.clone();
    div()
        .w_full()
        .pt(DynamicSpacing::Base12.px(&theme))
        .pb(DynamicSpacing::Base02.px(&theme))
        .group("remote-host-row")
        .child(crate::motion::hover(
            div()
                .id(ElementId::Name(id.clone().into()))
                .debug_selector(move || selector.clone())
                .w_full()
                .min_h(ButtonSize::Default.height(&theme))
                .pl(DynamicSpacing::Base06.px(&theme))
                .pr(DynamicSpacing::Base08.px(&theme))
                .rounded(list_item::RADIUS.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .cursor_pointer()
                .border_1()
                .border_color(theme.hairline().opacity(0.))
                .when(cursor, |row| row.border_color(theme.focus_ring))
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    toggle.update(cx, |app, cx| {
                        app.toggle_remote_session_group(&toggle_name, cx)
                    });
                })
                .child(dropdown_chevron(
                    format!("remote-host-group-caret-{name}"),
                    !collapsed,
                    IconSize::XSmall.px(&theme),
                    theme.text_muted,
                    &theme,
                ))
                .child(icon(
                    "icons/server-stack.svg",
                    IconSize::Small.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary)
                        .child(name.to_string()),
                )
                .child(
                    IconButton::new(
                        ElementId::Name(format!("remote-host-refresh-{name}").into()),
                        "icons/refresh.svg",
                    )
                    .size(ButtonSize::Compact)
                    .tip(tr!("remote_sessions.refresh"))
                    .faded(&theme, |el| {
                        el.opacity(0.)
                            .group_hover("remote-host-row", |button| button.opacity(1.))
                            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                                cx.stop_propagation();
                                refresh.update(cx, |app, cx| {
                                    app.refresh_remote_host_sessions(&refresh_name, cx)
                                });
                            })
                    }),
                )
                .child(trailing),
            id,
            &theme,
            Hover::over(cursor.then_some(theme.hover_wash()), theme.hover_wash()),
        ))
        .into_any_element()
}

/// A remote session row: its title and age on one line, like a local
/// session's, selected while it is the one on screen. A press resumes it on
/// the host.
pub(super) fn render_remote_session_row(
    row: &RemoteSessionRow,
    active: bool,
    this: &Entity<OrbitApp>,
    cursor: bool,
    theme: Theme,
) -> AnyElement {
    let open = this.clone();
    let (host, session) = (row.host.clone(), row.session.clone());
    let id = format!("remote-session-{}-{}", row.host.name, row.session.id);
    let selector = id.clone();
    let card = div()
        .id(ElementId::Name(id.clone().into()))
        .debug_selector(move || selector.clone())
        .w_full()
        .px(DynamicSpacing::Base08.px(&theme))
        .py(DynamicSpacing::Base04.px(&theme))
        .min_h(ButtonSize::Medium.height(&theme))
        .rounded(list_item::RADIUS.px(&theme))
        .border_1()
        .border_color(theme.hairline().opacity(0.))
        .when(cursor, |card| card.border_color(theme.focus_ring))
        .when(active, |card| card.bg(theme.selected_wash()))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .cursor_pointer()
        .tip(row.session.first_message.clone())
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            let (host, session) = (host.clone(), session.clone());
            open.update(cx, |app, cx| app.open_remote_session(host, session, cx));
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(TextSize::Small.px(&theme))
                .text_color(if active {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .child(row.session.title.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_muted)
                .child(crate::sessions::relative_time(row.session.modified)),
        );
    super::sidebar::nested_group_row(
        crate::motion::hover(
            card,
            id,
            &theme,
            Hover::over(
                active.then_some(theme.selected_wash()),
                theme.hover_wash(),
            ),
        ),
        &theme,
    )
}
