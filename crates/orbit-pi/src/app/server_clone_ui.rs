//! "Add to a server": the dialog and flow that put the current project on a
//! remote host, after t3code's "Add to another machine". The host clones the
//! project's `origin` with its own git, Orbit copies the ignored files the
//! user ticks (`.env` and the like are ticked by default), saves a host entry
//! for the new folder, and starts a session there.
//!
//! Nothing of the working tree is uploaded: only what is pushed reaches the
//! host, plus the picked ignored files, each sent on ssh's stdin (see
//! `crate::server_clone` and `orbit_rpc::remote::{clone_script,
//! copy_files_script}`). The connection check, password prompt and the
//! explained failures are the Remote hosts flow's own (`AuthThen::AddToServer`).

use std::sync::atomic::{AtomicU64, Ordering};

use super::remote_ui::TaskPlace;
use super::ssh_auth_ui::AuthThen;
use super::*;
use crate::git_panel::widgets::check_box;
use crate::remote_hosts::{self, ProbeFailure, RemoteHost};
use crate::server_clone::{self, CloneProtocol, CloneResult, ProjectSource};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, TextSize};
use crate::ui::button::ButtonVariant;
use crate::ui::button::{ButtonGroup, SegmentFill};
use crate::ui::dialog::modal_button;
use crate::ui::field::TextField;
use crate::ui::prelude::*;
use crate::ui::banner::{Banner, BannerTone};

/// Each dialog opening gets its own number, so a result for one that was
/// closed (or reopened) lands nowhere.
static GENERATION: AtomicU64 = AtomicU64::new(1);

/// The local project, while it is read and once it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceState {
    Loading,
    Ready(ProjectSource),
    Failed(String),
}

/// Where a submitted clone is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClonePhase {
    Idle,
    Connecting,
    Cloning,
    Copying,
}

pub(crate) struct ServerCloneDialog {
    pub(super) generation: u64,
    pub(super) source: SourceState,
    /// The enabled hosts, in Settings order.
    pub(super) hosts: Vec<RemoteHost>,
    pub(super) host_ix: usize,
    pub(super) protocol: CloneProtocol,
    /// The folder on the host, `~/…` or absolute.
    pub(super) folder: Entity<ComposerInput>,
    pub(super) phase: ClonePhase,
    pub(super) error: Option<String>,
}

impl ServerCloneDialog {
    pub(super) fn host(&self) -> Option<&RemoteHost> {
        self.hosts.get(self.host_ix)
    }

    fn busy(&self) -> bool {
        self.phase != ClonePhase::Idle
    }
}

/// The host the connection check runs against: the picked host with `/` as
/// its folder, because the project's folder does not exist there yet.
fn probe_host(host: &RemoteHost) -> RemoteHost {
    RemoteHost {
        dir: "/".into(),
        ..host.clone()
    }
}

/// What saving the clone's host entry has to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloneEntry {
    /// No entry for this server and folder yet: add one.
    New,
    /// An entry exists but was not linked to the local project: link it.
    Linked,
    /// An entry exists and already says which local folder it is.
    Unchanged,
}

/// The host entry saved for the new folder: the server's connection details
/// with the clone as its folder, under a name no other entry has, linked to
/// `local` (the project on this Mac) so its sessions list in that project's
/// sidebar group. A saved entry for the same server and folder is reused; it
/// only gains the link when it had none.
pub(crate) fn host_for_clone(
    server: &RemoteHost,
    project: &str,
    dir: &str,
    local: &Path,
    saved: &[RemoteHost],
) -> (RemoteHost, CloneEntry) {
    let local_dir = Some(local.to_string_lossy().into_owned());
    if let Some(existing) = saved.iter().find(|host| {
        host.ssh == server.ssh
            && host.user == server.user
            && host.port == server.port
            && host.dir == dir
    }) {
        if existing.local_path().is_some() {
            return (existing.clone(), CloneEntry::Unchanged);
        }
        let linked = RemoteHost {
            local_dir,
            ..existing.clone()
        };
        return (linked, CloneEntry::Linked);
    }
    let base = tr!(
        "server_clone.host_name",
        project = project.to_string(),
        host = server.name.clone()
    );
    let taken = |name: &str| saved.iter().any(|host| host.name == name);
    let name = if taken(&base) {
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .unwrap_or(base)
    } else {
        base
    };
    (
        RemoteHost {
            name,
            dir: dir.to_string(),
            enabled: true,
            local_dir,
            ..server.clone()
        },
        CloneEntry::New,
    )
}

impl OrbitApp {
    /// Open the dialog for the current project, with `preselect` (a host
    /// name) picked when it is one of the enabled hosts.
    pub(super) fn open_server_clone(
        &mut self,
        preselect: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.current_workspace.clone() else {
            self.toast_info(tr!("server_clone.no_project"));
            cx.notify();
            return;
        };
        let hosts: Vec<RemoteHost> = remote_hosts::load()
            .0
            .into_iter()
            .filter(|host| host.enabled)
            .collect();
        if hosts.is_empty() {
            self.toast_info(tr!("server_clone.no_hosts"));
            cx.notify();
            return;
        }
        let host_ix = preselect
            .and_then(|name| hosts.iter().position(|host| host.name == name))
            .unwrap_or(0);
        let folder = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("server-clone-folder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_placeholder_key("server_clone.folder_placeholder")
        });
        let target = folder.read(cx).focus_handle(cx);
        self.server_clone_focus.open(&target, window, cx);
        let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
        self.server_clone = Some(ServerCloneDialog {
            generation,
            source: SourceState::Loading,
            hosts,
            host_ix,
            protocol: CloneProtocol::Ssh,
            folder,
            phase: ClonePhase::Idle,
            error: None,
        });
        cx.spawn(async move |this, cx| {
            let source = cx
                .background_executor()
                .spawn(async move { server_clone::inspect(&workspace) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_server_clone_source(generation, source, cx)
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_server_clone_source(
        &mut self,
        generation: u64,
        source: Result<ProjectSource, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .server_clone
            .as_mut()
            .filter(|dialog| dialog.generation == generation)
        else {
            return;
        };
        match source {
            Ok(source) => {
                dialog.protocol =
                    server_clone::protocol_of(&source.origin).unwrap_or(CloneProtocol::Ssh);
                if dialog.folder.read(cx).text().trim().is_empty() {
                    let suggested = format!("~/{}", source.name);
                    dialog
                        .folder
                        .update(cx, |input, cx| input.set_text(suggested, cx));
                }
                dialog.source = SourceState::Ready(source);
            }
            Err(message) => dialog.source = SourceState::Failed(message),
        }
        cx.notify();
    }

    /// Close the dialog, unless a clone is under way (the host is already
    /// working on it; the result has to land somewhere).
    pub(super) fn close_server_clone(&mut self, cx: &mut Context<Self>) {
        if self.server_clone.as_ref().is_some_and(|d| !d.busy()) {
            self.server_clone = None;
            cx.notify();
        }
    }

    pub(super) fn server_clone_pick_host(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = self.server_clone.as_mut().filter(|d| !d.busy()) {
            if ix < dialog.hosts.len() {
                dialog.host_ix = ix;
                dialog.error = None;
                cx.notify();
            }
        }
    }

    pub(super) fn server_clone_set_protocol(
        &mut self,
        protocol: CloneProtocol,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.server_clone.as_mut().filter(|d| !d.busy()) {
            dialog.protocol = protocol;
            dialog.error = None;
            cx.notify();
        }
    }

    pub(super) fn server_clone_toggle_file(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(dialog) = self.server_clone.as_mut().filter(|d| !d.busy()) else {
            return;
        };
        if let SourceState::Ready(source) = &mut dialog.source {
            if let Some(file) = source.files.get_mut(ix).filter(|file| !file.too_large()) {
                file.selected = !file.selected;
                cx.notify();
            }
        }
    }

    /// **Clone**: check the fields, then connect (asking for a password when
    /// the server wants one); the clone starts once the check passes.
    pub(super) fn server_clone_submit(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.server_clone.as_mut().filter(|d| !d.busy()) else {
            return;
        };
        if !matches!(dialog.source, SourceState::Ready(_)) {
            return;
        }
        let folder = dialog.folder.read(cx).text().trim().to_string();
        if !orbit_rpc::remote::is_clone_destination(&folder) {
            dialog.error = Some(tr!("server_clone.folder_invalid"));
            cx.notify();
            return;
        }
        let Some(host) = dialog.host().cloned() else {
            return;
        };
        dialog.error = None;
        dialog.phase = ClonePhase::Connecting;
        let generation = dialog.generation;
        self.run_remote_probe(probe_host(&host), AuthThen::AddToServer(generation), 0, cx);
    }

    /// Whether a connection result for `generation` still has a dialog.
    pub(super) fn server_clone_waiting(&self, generation: u64) -> bool {
        self.server_clone
            .as_ref()
            .is_some_and(|d| d.generation == generation && d.phase == ClonePhase::Connecting)
    }

    /// The connection check failed: say why in the dialog and let the user
    /// change something and try again.
    pub(super) fn server_clone_failed(&mut self, generation: u64, message: String) {
        if let Some(dialog) = self
            .server_clone
            .as_mut()
            .filter(|d| d.generation == generation)
        {
            dialog.phase = ClonePhase::Idle;
            dialog.error = Some(message);
        }
    }

    /// Connected: clone on the host, then copy the picked files, off the UI
    /// thread. Files are read on this Mac only now, so the copy is what was on
    /// disk when the clone was asked for.
    pub(super) fn server_clone_connected(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(dialog) = self
            .server_clone
            .as_mut()
            .filter(|d| d.generation == generation)
        else {
            return;
        };
        let (SourceState::Ready(source), Some(host)) = (&dialog.source, dialog.host().cloned())
        else {
            return;
        };
        let source = source.clone();
        let url = source.url(dialog.protocol);
        let folder = dialog.folder.read(cx).text().trim().to_string();
        let picked: Vec<String> = source
            .files
            .iter()
            .filter(|file| file.selected)
            .map(|file| file.path.clone())
            .collect();
        dialog.phase = ClonePhase::Cloning;
        let secret = self.remote_secret(&host);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let clone_host = host.clone();
            let clone_secret = secret.clone();
            let (clone_url, clone_folder) = (url.clone(), folder.clone());
            let cloned = cx
                .background_executor()
                .spawn(async move {
                    remote_hosts::clone_project(
                        &clone_host,
                        clone_secret.as_deref(),
                        &clone_url,
                        &clone_folder,
                    )
                })
                .await;
            let path = match cloned.map(|stdout| server_clone::parse_clone(&stdout)) {
                Ok(CloneResult::Cloned(path) | CloneResult::Reused(path)) => path,
                Ok(CloneResult::Failed { kind, detail }) => {
                    let message = server_clone::clone_failure_text(&kind, &detail, &url, &folder);
                    let _ = this.update(cx, |app, cx| {
                        app.server_clone_failed(generation, message);
                        cx.notify();
                    });
                    return;
                }
                Err(failure) => {
                    let _ = this.update(cx, |app, cx| {
                        let message = ssh_failure_text(failure, &host);
                        app.server_clone_failed(generation, message);
                        cx.notify();
                    });
                    return;
                }
            };
            let mut failures = Vec::new();
            if !picked.is_empty() {
                let _ = this.update(cx, |app, cx| {
                    if let Some(dialog) = app
                        .server_clone
                        .as_mut()
                        .filter(|d| d.generation == generation)
                    {
                        dialog.phase = ClonePhase::Copying;
                    }
                    cx.notify();
                });
                let (copy_host, copy_path) = (host.clone(), path.clone());
                let root = source.root.clone();
                failures = cx
                    .background_executor()
                    .spawn(async move {
                        let (payload, mut failures) = server_clone::copy_payload(&root, &picked);
                        let sent: Vec<String> = picked
                            .iter()
                            .filter(|path| !failures.iter().any(|(failed, _)| failed == *path))
                            .cloned()
                            .collect();
                        if !sent.is_empty() {
                            match remote_hosts::copy_files(
                                &copy_host,
                                secret.as_deref(),
                                &copy_path,
                                &payload,
                            ) {
                                Ok(stdout) => {
                                    failures.extend(server_clone::parse_copy(&stdout, &sent))
                                }
                                Err(failure) => {
                                    let reason = ssh_failure_text(failure, &copy_host);
                                    failures.extend(
                                        sent.into_iter().map(|path| (path, reason.clone())),
                                    );
                                }
                            }
                        }
                        failures
                    })
                    .await;
            }
            let _ = this.update(cx, |app, cx| {
                app.finish_server_clone(generation, host, source, path, failures, cx);
            });
        })
        .detach();
    }

    /// The clone is on the host: save its host entry, report any file that
    /// did not copy, close the dialog and start a session in the new folder.
    pub(super) fn finish_server_clone(
        &mut self,
        generation: u64,
        server: RemoteHost,
        source: ProjectSource,
        clone_path: String,
        failures: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        if self
            .server_clone
            .as_ref()
            .is_none_or(|d| d.generation != generation)
        {
            return;
        }
        self.server_clone = None;
        let dir = server_clone::session_dir(&clone_path, &source.subdir);
        let (saved, _) = remote_hosts::load();
        let local = if source.subdir.is_empty() {
            source.root.clone()
        } else {
            source.root.join(&source.subdir)
        };
        let (host, entry) = host_for_clone(&server, &source.name, &dir, &local, &saved);
        let original = match entry {
            CloneEntry::New => Some(None),
            CloneEntry::Linked => Some(Some(host.name.clone())),
            CloneEntry::Unchanged => None,
        };
        if let Some(original) = original {
            match remote_hosts::upsert(original.as_deref(), &host) {
                Ok(()) => self.reload_remote_hosts(),
                Err(err) => {
                    self.toast_error(tr!("server_clone.save_failed", error = format!("{err:#}")))
                }
            }
        }
        if failures.is_empty() {
            self.toast_success(tr!(
                "server_clone.done",
                project = source.name.clone(),
                host = server.name.clone()
            ));
        } else {
            let body = failures
                .iter()
                .map(|(path, reason)| format!("{path}: {reason}"))
                .collect::<Vec<_>>()
                .join("\n");
            self.push_toast(
                crate::toast::ToastKind::Warning,
                tr!(
                    "server_clone.copy_failed",
                    count = failures.len(),
                    host = server.name.clone()
                ),
                Some(body),
            );
        }
        // The session is the point: leave Settings if the dialog came from a
        // host card, then start in the clone.
        if self.settings_open {
            self.close_settings(cx);
        }
        self.start_task_at(TaskPlace::Remote(host), cx);
        self.composer_focus_pending = true;
        cx.notify();
    }

    pub(super) fn server_clone_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let dialog = self.server_clone.as_ref()?;
        let label = |text: String| {
            input_label(text, &theme)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
        };
        let muted = |text: String| {
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(text)
        };
        let host_name = dialog.host().map(|h| h.name.clone()).unwrap_or_default();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme));

        // From this Mac.
        match &dialog.source {
            SourceState::Loading => {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .child(spinner(
                            "server-clone-loading",
                            IconSize::Small.px(&theme),
                            theme.text_muted,
                            theme,
                        ))
                        .child(muted(tr!("server_clone.reading"))),
                );
            }
            SourceState::Failed(message) => {
                body = body.child(error_box(message.clone(), theme));
            }
            SourceState::Ready(source) => {
                body = body
                    .child(label(tr!("server_clone.repository")))
                    .child(self.server_clone_repository(source, dialog, theme, this.clone()));
            }
        }

        // The server.
        body = body.child(label(tr!("server_clone.server")));
        let mut hosts = div()
            .id("server-clone-hosts")
            .max_h(px(156.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base02.px(&theme));
        for (ix, host) in dialog.hosts.iter().enumerate() {
            let selected = ix == dialog.host_ix;
            let pick = this.clone();
            let id = format!("server-clone-host-{ix}");
            let selector = id.clone();
            let row = picker_entry(div().id(ElementId::Name(id.clone().into())), &theme)
                .debug_selector(move || selector.clone())
                .h(crate::theme::tokens::picker::two_line_entry_height(&theme))
                .flex_none()
                .cursor_pointer()
                .when(selected, |row| row.bg(theme.selected_wash()))
                .child(
                    PickerRow::new(host.name.clone(), theme.text_primary, theme.text_muted)
                        .icon("icons/server-stack.svg", theme.text_muted)
                        .detail(host.connection_label())
                        .mono()
                        .into_element(&theme),
                )
                .when(selected, |row| {
                    row.child(icon(
                        "icons/check.svg",
                        crate::theme::tokens::context_menu::ICON.px(&theme),
                        theme.accent_text,
                    ))
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    pick.update(cx, |app, cx| app.server_clone_pick_host(ix, cx));
                });
            hosts = hosts.child(crate::motion::hover(
                row,
                id,
                &theme,
                crate::motion::Hover::wash(&theme),
            ));
        }
        body = body
            .child(hosts)
            .child(label(tr!("server_clone.folder", host = host_name.clone())))
            .child(TextField::new(dialog.folder.clone(), theme.background).render(&theme));

        if let SourceState::Ready(source) = &dialog.source {
            body = body
                .child(label(tr!("server_clone.files")))
                .child(self.server_clone_files(source, theme, this.clone()));
            let git_host = server_clone::git_host(&source.origin).unwrap_or_default();
            body = body.child(muted(tr!(
                "server_clone.access_hint",
                host = host_name.clone(),
                git_host = git_host
            )));
        }
        if let Some(error) = &dialog.error {
            body = body.child(error_box(error.clone(), theme));
        }

        let busy = dialog.busy();
        let submit_label = match dialog.phase {
            ClonePhase::Idle => tr!("server_clone.submit", host = host_name.clone()),
            ClonePhase::Connecting => tr!("remote.connecting", host = host_name.clone()),
            ClonePhase::Cloning => tr!("server_clone.cloning"),
            ClonePhase::Copying => tr!("server_clone.copying"),
        };
        let footer = div()
            .flex_none()
            .px(crate::theme::tokens::modal::footer_padding(&theme))
            .py(crate::theme::tokens::modal::footer_padding(&theme))
            .flex()
            .items_center()
            .gap(crate::theme::tokens::modal::footer_gap(&theme))
            .border_t_1()
            .border_color(theme.hairline())
            .children(busy.then(|| {
                spinner(
                    "server-clone-busy",
                    IconSize::Small.px(&theme),
                    theme.text_muted,
                    theme,
                )
            }))
            .child(div().flex_1())
            .children((!busy).then(|| {
                let this = this.clone();
                modal_button(
                    "server-clone-cancel",
                    tr!("remote_hosts.cancel"),
                    ButtonVariant::Secondary,
                    &self.modal_cancel_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.close_server_clone(cx))
                })
            }))
            .child({
                let this = this.clone();
                modal_button(
                    "server-clone-submit",
                    submit_label,
                    ButtonVariant::Primary,
                    &self.modal_confirm_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.server_clone_submit(cx))
                })
            });

        let card = modal_card(&theme)
            .debug_selector(|| "server-clone-dialog".to_string())
            .modal_width(px(540.))
            .max_h(relative(1.))
            .key_context("Picker")
            .on_action(
                cx.listener(|app, _: &crate::PickerCancel, _, cx| app.close_server_clone(cx)),
            )
            .on_action(
                cx.listener(|app, _: &crate::PickerConfirm, _, cx| app.server_clone_submit(cx)),
            )
            .child(crate::ui::dialog::modal_header(
                &theme,
                tr!("server_clone.title"),
                Some(crate::ui::dialog::ModalSubtitle::Text(
                    match &dialog.source {
                        SourceState::Ready(source) => {
                            tr!("server_clone.subtitle", project = source.name.clone())
                        }
                        _ => tr!("server_clone.subtitle_plain"),
                    }
                    .into(),
                )),
            ))
            .child(
                div()
                    .id("server-clone-body")
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
                .id("server-clone-layer")
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.close_server_clone(cx))
                })
                .child(crate::motion::enter(
                    card,
                    "server-clone-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }

    /// The clone URL (mono) and, when the origin can be written both ways,
    /// an SSH / HTTPS switch.
    fn server_clone_repository(
        &self,
        source: &ProjectSource,
        dialog: &ServerCloneDialog,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let url = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .font_family(crate::theme::code_font_family())
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text_secondary)
            .child(source.url(dialog.protocol));
        let mut row = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(url);
        if source.switchable() {
            const PROTOCOLS: [(CloneProtocol, &str); 2] =
                [(CloneProtocol::Ssh, "SSH"), (CloneProtocol::Https, "HTTPS")];
            let mut segments = ButtonGroup::new(SegmentFill::Active).size(ButtonSize::Default);
            for (ix, (protocol, name)) in PROTOCOLS.iter().copied().enumerate() {
                let pick = this.clone();
                segments = segments
                    .segment(
                        ElementId::Name(format!("server-clone-protocol-{ix}").into()),
                        name,
                        dialog.protocol == protocol,
                        move |_, _, cx| {
                            pick.update(cx, |app, cx| app.server_clone_set_protocol(protocol, cx));
                        },
                    )
                    .selector(format!("server-clone-protocol-{ix}"));
            }
            row = row.child(segments.render(&theme));
        }
        row.into_any_element()
    }

    /// The ignored files as checkbox rows (path and size); a file over the
    /// size cap is shown but cannot be ticked.
    fn server_clone_files(
        &self,
        source: &ProjectSource,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        if source.files.is_empty() {
            return div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("server_clone.no_files"))
                .into_any_element();
        }
        let mut list = div()
            .id("server-clone-files")
            .max_h(px(180.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .rounded(crate::theme::tokens::panel::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.hairline());
        for (ix, file) in source.files.iter().enumerate() {
            let disabled = file.too_large();
            let toggle = this.clone();
            let id = format!("server-clone-file-{ix}");
            let size = if disabled {
                tr!("server_clone.too_large")
            } else {
                crate::skills::format_size(file.size)
            };
            let selector = id.clone();
            let row = div()
                .id(ElementId::Name(id.clone().into()))
                .debug_selector(move || selector.clone())
                .flex_none()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .px(DynamicSpacing::Base08.px(&theme))
                .py(DynamicSpacing::Base04.px(&theme))
                .when(!disabled, |row| row.cursor_pointer())
                .when(disabled, |row| row.opacity(0.5))
                .child(check_box(file.selected, theme))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(crate::theme::code_font_family())
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_primary)
                        .child(file.path.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(size),
                )
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    toggle.update(cx, |app, cx| app.server_clone_toggle_file(ix, cx));
                });
            list = list.child(crate::motion::hover(
                row,
                id,
                &theme,
                crate::motion::Hover::wash(&theme),
            ));
        }
        let mut column = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(list);
        if source.truncated {
            column = column.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!(
                        "server_clone.truncated",
                        count = server_clone::MAX_LISTED_FILES
                    )),
            );
        }
        column.into_any_element()
    }
}

/// Why an ssh run for the clone or the copy did not reach the host. The
/// password was accepted moments earlier by the connection check, so a
/// refused login here means it changed or expired.
fn ssh_failure_text(failure: ProbeFailure, host: &RemoteHost) -> String {
    match failure {
        ProbeFailure::NeedsPassword { .. } | ProbeFailure::WrongPassword => {
            tr!("server_clone.login_lost", host = host.name.clone())
        }
        other => super::remote_providers_ui::failure_text(other, host),
    }
}

/// A tinted line for an error inside the dialog (the key dialog's shape).
fn error_box(message: String, theme: Theme) -> gpui::Div {
    Banner::note(BannerTone::Error, message, &theme)
}
