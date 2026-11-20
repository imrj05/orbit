//! SSH password logins for remote hosts, after t3code's flow.
//!
//! Every connection is first tried without a password (ssh `BatchMode`: keys
//! and the agent only). When the server refuses every key but offers a
//! password, Orbit asks for one in its own dialog and runs the same connection
//! again with ssh pointed at the askpass helper, which prints the password
//! ssh passes it (`orbit_rpc::remote::ASKPASS_SCRIPT`). A wrong password asks
//! again, up to [`MAX_PASSWORD_ATTEMPTS`].
//!
//! The password lives only in memory, keyed by ssh account
//! ([`RemoteHost::account_key`]), until Orbit quits or the server refuses it.
//! It is never written to disk, never logged, and the field is masked and
//! refuses copy. An unknown or changed host key is never trusted from here:
//! the user verifies the fingerprint in a terminal, the same rule t3code uses.
//!
//! The three things that connect go through [`OrbitApp::run_remote_probe`]:
//! the form's **Test connection**, a row's **Test**, and starting a task from
//! the folder picker (which checks the host before spawning pi there).

use super::remote_hosts_ui::HostCheck;
use super::remote_providers_ui::RemoteModels;
use super::*;
use crate::remote_hosts::{self, ProbeFailure, RemoteHost};
use crate::theme::tokens::{input, modal, DynamicSpacing, TextSize};
use crate::ui::button::ButtonVariant;
use crate::ui::dialog::modal_button;
use crate::ui::field::TextField;
use crate::ui::prelude::*;
use crate::ui::banner::{Banner, BannerTone};
use orbit_rpc::remote::HostKeyProblem;

/// Password tries per connection before Orbit stops asking.
const MAX_PASSWORD_ATTEMPTS: u8 = 3;

/// What a connection check is for, and so where its result lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthThen {
    /// The Remote hosts form's **Test connection** (for this form generation).
    EditorTest(u64),
    /// A host row's **Test**.
    RowTest(String),
    /// Starting a task on the host from the folder picker.
    StartTask,
    /// Settings → Providers & models listing this host's providers.
    ProviderList(String),
    /// The "Add to a server" dialog (this opening of it) before its clone.
    AddToServer(u64),
    /// A sidebar server group's refresh (this host entry's name).
    SessionList(String),
    /// Opening a remote session from the sidebar (its path on the host).
    ResumeSession(String),
}

/// Where an install was started, and so where its result and the check
/// after it land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallTarget {
    /// A saved host's row.
    Row(String),
    /// The Remote hosts form (its current field values).
    Editor,
}

/// The open password dialog.
pub(crate) struct PasswordPrompt {
    pub(super) host: RemoteHost,
    pub(super) input: Entity<ComposerInput>,
    /// Passwords already tried for this connection.
    pub(super) attempt: u8,
    pub(super) error: Option<String>,
    pub(super) then: AuthThen,
}

/// The command a user runs to verify a host key themselves.
pub(crate) fn ssh_command(host: &RemoteHost) -> String {
    match host.port {
        Some(port) => format!("ssh -p {port} {}", host.destination()),
        None => format!("ssh {}", host.destination()),
    }
}

impl OrbitApp {
    /// The password remembered for `host`'s ssh account this run, if any.
    pub(super) fn remote_secret(&self, host: &RemoteHost) -> Option<String> {
        self.remote_host_secrets.get(&host.account_key()).cloned()
    }

    /// Install pi on a host, then check it again. Runs pi's own installer over
    /// the same ssh connection as a check (remembered password included),
    /// plus Node.js 22 from nodejs.org when the host has none; no sudo, no
    /// terminal, no profile edits (see `orbit_rpc::remote::install_script`).
    /// Started only by the user's click on **Install pi**.
    pub(super) fn remote_host_install(&mut self, target: InstallTarget, cx: &mut Context<Self>) {
        let host = match &target {
            InstallTarget::Row(name) => {
                let Some(host) = self
                    .remote_hosts
                    .iter()
                    .find(|host| &host.name == name)
                    .cloned()
                else {
                    return;
                };
                if self.remote_host_checks.get(name) == Some(&HostCheck::Installing) {
                    return;
                }
                self.remote_host_checks
                    .insert(name.clone(), HostCheck::Installing);
                host
            }
            InstallTarget::Editor => {
                let Some(Ok(host)) = self.remote_host_editor_host(cx) else {
                    return;
                };
                let Some(editor) = self.remote_host_editor.as_mut() else {
                    return;
                };
                if editor.check == Some(HostCheck::Installing) {
                    return;
                }
                editor.check = Some(HostCheck::Installing);
                host
            }
        };
        let generation = self
            .remote_host_editor
            .as_ref()
            .map(|editor| editor.check_gen)
            .unwrap_or_default();
        cx.notify();
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let install_host = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { remote_hosts::install(&install_host, secret.as_deref()) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_install(host, target, generation, outcome, cx);
            });
        })
        .detach();
    }

    pub(super) fn finish_remote_install(
        &mut self,
        host: RemoteHost,
        target: InstallTarget,
        generation: u64,
        outcome: Result<String, ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        // The form may have closed or been re-tested while pi installed.
        let then = match &target {
            InstallTarget::Row(name) => {
                if !self.remote_host_checks.contains_key(name) {
                    return;
                }
                AuthThen::RowTest(name.clone())
            }
            InstallTarget::Editor => {
                let Some(editor) = self.remote_host_editor.as_mut() else {
                    return;
                };
                if editor.check_gen != generation {
                    return;
                }
                editor.check_gen += 1;
                AuthThen::EditorTest(editor.check_gen)
            }
        };
        match outcome {
            Ok(version) => {
                self.toast_success(tr!(
                    "remote_hosts.installed",
                    host = host.name.clone(),
                    version = version
                ));
                // Confirm the way every check does, so the row ends on what a
                // session will actually find.
                self.run_remote_probe(host, then, 0, cx);
            }
            Err(failure) => {
                let message = match failure {
                    ProbeFailure::NeedsPassword { .. } | ProbeFailure::WrongPassword => {
                        tr!("remote_hosts.install_needs_test")
                    }
                    ProbeFailure::HostKey(problem) => {
                        let command = ssh_command(&host);
                        match problem {
                            HostKeyProblem::Unknown => {
                                tr!("remote_hosts.host_key_unknown", command = command)
                            }
                            HostKeyProblem::Changed => {
                                tr!("remote_hosts.host_key_changed", command = command)
                            }
                        }
                    }
                    ProbeFailure::LoginRefused { message, .. } => message,
                    ProbeFailure::PiMissing => tr!("remote_hosts.pi_missing"),
                    ProbeFailure::Other(message) => {
                        tr!("remote_hosts.install_failed", error = message)
                    }
                };
                self.remote_probe_failed(&then, message, cx);
            }
        }
    }

    /// Start a task on `host` from the folder picker: check the connection
    /// first, so a refused key, a missing `pi` or an unknown host key is
    /// reported before a session exists, and ask for a password when needed.
    pub(super) fn connect_remote_task(&mut self, host: RemoteHost, cx: &mut Context<Self>) {
        self.run_remote_probe(host, AuthThen::StartTask, 0, cx);
    }

    /// Run the read-only check for `host` with the remembered password (if
    /// any) and route the result to `then`. `attempt` counts passwords tried.
    pub(super) fn run_remote_probe(
        &mut self,
        host: RemoteHost,
        then: AuthThen,
        attempt: u8,
        cx: &mut Context<Self>,
    ) {
        // The Providers page asks for the host's model list, not the check;
        // a password typed for it comes back through here.
        if matches!(then, AuthThen::ProviderList(_)) {
            self.fetch_remote_models(host, attempt, cx);
            return;
        }
        // A session list retried with a password is the list itself again.
        if matches!(then, AuthThen::SessionList(_)) {
            self.fetch_remote_sessions(host, attempt, true, cx);
            return;
        }
        match &then {
            AuthThen::ProviderList(_) => {}
            AuthThen::EditorTest(_) => {
                if let Some(editor) = self.remote_host_editor.as_mut() {
                    editor.error = None;
                    editor.check = Some(HostCheck::Checking);
                }
            }
            AuthThen::RowTest(name) => {
                self.remote_host_checks
                    .insert(name.clone(), HostCheck::Checking);
            }
            AuthThen::StartTask => {
                self.set_status(tr!("remote.connecting", host = host.name.clone()));
            }
            AuthThen::ResumeSession(_) => {
                self.set_status(tr!("remote.connecting", host = host.name.clone()));
            }
            // The dialog's own button says it is connecting; a list is
            // redirected above.
            AuthThen::AddToServer(_) | AuthThen::SessionList(_) => {}
        }
        cx.notify();
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let probe_host = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { remote_hosts::probe(&probe_host, secret.as_deref()) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_probe(host, then, attempt, outcome, cx);
            });
        })
        .detach();
    }

    /// Whether a result for `then` still has somewhere to land: the form may
    /// have closed or moved on, the row may have been removed.
    fn probe_target_alive(&self, then: &AuthThen) -> bool {
        match then {
            AuthThen::EditorTest(generation) => self
                .remote_host_editor
                .as_ref()
                .is_some_and(|editor| editor.check_gen == *generation),
            AuthThen::RowTest(name) => self.remote_host_checks.contains_key(name),
            AuthThen::StartTask => true,
            AuthThen::ProviderList(name) => self.remote_model_lists.contains_key(name),
            AuthThen::AddToServer(generation) => self.server_clone_waiting(*generation),
            AuthThen::SessionList(_) | AuthThen::ResumeSession(_) => true,
        }
    }

    pub(super) fn finish_remote_probe(
        &mut self,
        host: RemoteHost,
        then: AuthThen,
        attempt: u8,
        outcome: Result<String, ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        if !self.probe_target_alive(&then) {
            return;
        }
        match outcome {
            Ok(version) => self.remote_probe_succeeded(host, then, version, cx),
            Err(ProbeFailure::NeedsPassword { offered: true })
                if remote_hosts::passwords_supported() =>
            {
                self.open_password_prompt(host, then, attempt, None, cx);
            }
            Err(ProbeFailure::NeedsPassword { offered: true }) => {
                self.remote_probe_failed(&then, tr!("remote_hosts.password_unsupported"), cx);
            }
            Err(ProbeFailure::NeedsPassword { offered: false }) => {
                self.remote_probe_failed(&then, tr!("remote_hosts.keys_only"), cx);
            }
            Err(ProbeFailure::WrongPassword) => {
                // A refused password is forgotten at once.
                self.remote_host_secrets.remove(&host.account_key());
                if attempt < MAX_PASSWORD_ATTEMPTS {
                    let error = Some(tr!("remote_hosts.wrong_password"));
                    self.open_password_prompt(host, then, attempt, error, cx);
                } else {
                    self.remote_probe_failed(&then, tr!("remote_hosts.wrong_password"), cx);
                }
            }
            Err(ProbeFailure::HostKey(problem)) => {
                let command = ssh_command(&host);
                let message = match problem {
                    HostKeyProblem::Unknown => {
                        tr!("remote_hosts.host_key_unknown", command = command)
                    }
                    HostKeyProblem::Changed => {
                        tr!("remote_hosts.host_key_changed", command = command)
                    }
                };
                self.remote_probe_failed(&then, message, cx);
            }
            Err(ProbeFailure::LoginRefused { suggested, message }) => match suggested {
                Some(user) => {
                    let explained = tr!(
                        "remote_hosts.login_refused",
                        current = host.user.clone().unwrap_or_else(|| "root".into()),
                        user = user.clone()
                    );
                    self.remote_probe_wrong_user(&then, explained, user, cx);
                }
                None => self.remote_probe_failed(
                    &then,
                    tr!("remote_hosts.login_refused_unnamed", message = message),
                    cx,
                ),
            },
            Err(ProbeFailure::PiMissing) => {
                let command = host.install_command();
                match &then {
                    AuthThen::EditorTest(_) => {
                        if let Some(editor) = self.remote_host_editor.as_mut() {
                            editor.check = Some(HostCheck::PiMissing { command });
                        }
                    }
                    AuthThen::RowTest(name) => {
                        self.remote_host_checks
                            .insert(name.clone(), HostCheck::PiMissing { command });
                    }
                    AuthThen::StartTask => {
                        self.set_status(String::new());
                        self.toast_error(tr!(
                            "remote.connect_failed",
                            error = tr!("remote_hosts.pi_missing_with_command", command = command)
                        ));
                    }
                    AuthThen::ProviderList(name) => {
                        self.remote_model_lists
                            .insert(name.clone(), RemoteModels::PiMissing);
                    }
                    AuthThen::AddToServer(generation) => self.server_clone_failed(
                        *generation,
                        tr!("remote_hosts.pi_missing_with_command", command = command),
                    ),
                    AuthThen::SessionList(name) => self.remote_sessions_failed(
                        name,
                        tr!("remote_hosts.pi_missing_with_command", command = command),
                    ),
                    AuthThen::ResumeSession(_) => {
                        self.set_status(String::new());
                        self.toast_error(tr!(
                            "remote.connect_failed",
                            error = tr!("remote_hosts.pi_missing_with_command", command = command)
                        ));
                    }
                }
                cx.notify();
            }
            Err(ProbeFailure::Other(message)) => self.remote_probe_failed(&then, message, cx),
        }
    }

    fn remote_probe_succeeded(
        &mut self,
        host: RemoteHost,
        then: AuthThen,
        version: String,
        cx: &mut Context<Self>,
    ) {
        match then {
            AuthThen::EditorTest(_) => {
                if let Some(editor) = self.remote_host_editor.as_mut() {
                    editor.check = Some(HostCheck::Reachable(version));
                }
            }
            AuthThen::RowTest(name) => {
                self.remote_host_checks
                    .insert(name, HostCheck::Reachable(version));
            }
            AuthThen::StartTask => {
                self.start_task_at(TaskPlace::Remote(host), cx);
                self.composer_focus_pending = true;
            }
            // A listing finishes in `finish_remote_model_list`, never here.
            AuthThen::ProviderList(_) => {}
            AuthThen::AddToServer(generation) => self.server_clone_connected(generation, cx),
            AuthThen::ResumeSession(path) => self.resume_remote_session_on(host, path, cx),
            // A list finishes in `finish_remote_sessions`, never here.
            AuthThen::SessionList(_) => {}
        }
        cx.notify();
    }

    /// The server named the account to use instead: rows and the form offer
    /// to switch to it; a task start says so.
    fn remote_probe_wrong_user(
        &mut self,
        then: &AuthThen,
        message: String,
        user: String,
        cx: &mut Context<Self>,
    ) {
        match then {
            AuthThen::EditorTest(_) => {
                if let Some(editor) = self.remote_host_editor.as_mut() {
                    editor.check = Some(HostCheck::WrongUser { message, user });
                }
            }
            AuthThen::RowTest(name) => {
                self.remote_host_checks
                    .insert(name.clone(), HostCheck::WrongUser { message, user });
            }
            AuthThen::StartTask => {
                self.set_status(String::new());
                self.toast_error(tr!("remote.connect_failed", error = message));
            }
            AuthThen::ProviderList(name) => {
                self.remote_model_lists
                    .insert(name.clone(), RemoteModels::Failed(message));
            }
            AuthThen::AddToServer(generation) => self.server_clone_failed(*generation, message),
            AuthThen::SessionList(name) => self.remote_sessions_failed(name, message),
            AuthThen::ResumeSession(_) => {
                self.set_status(String::new());
                self.toast_error(tr!("remote.connect_failed", error = message));
            }
        }
        cx.notify();
    }

    fn remote_probe_failed(&mut self, then: &AuthThen, message: String, cx: &mut Context<Self>) {
        match then {
            AuthThen::EditorTest(_) => {
                if let Some(editor) = self.remote_host_editor.as_mut() {
                    editor.check = Some(HostCheck::Failed(message));
                }
            }
            AuthThen::RowTest(name) => {
                self.remote_host_checks
                    .insert(name.clone(), HostCheck::Failed(message));
            }
            AuthThen::StartTask => {
                self.set_status(String::new());
                self.toast_error(tr!("remote.connect_failed", error = message));
            }
            AuthThen::ProviderList(name) => {
                self.remote_model_lists
                    .insert(name.clone(), RemoteModels::Failed(message));
            }
            AuthThen::AddToServer(generation) => self.server_clone_failed(*generation, message),
            AuthThen::SessionList(name) => self.remote_sessions_failed(name, message),
            AuthThen::ResumeSession(_) => {
                self.set_status(String::new());
                self.toast_error(tr!("remote.connect_failed", error = message));
            }
        }
        cx.notify();
    }

    fn open_password_prompt(
        &mut self,
        host: RemoteHost,
        then: AuthThen,
        attempt: u8,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("remote-password-input")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_masked(true)
                .with_placeholder_key("remote_hosts.password_placeholder")
        });
        self.remote_password_prompt = Some(PasswordPrompt {
            host,
            input,
            attempt,
            error,
            then,
        });
        self.remote_password_focus_pending = true;
        cx.notify();
    }

    pub(super) fn remote_password_submit(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.remote_password_prompt.as_mut() else {
            return;
        };
        if prompt.input.read(cx).is_composing() {
            return;
        }
        let secret = prompt.input.read(cx).text().to_string();
        if secret.is_empty() {
            prompt.error = Some(tr!("remote_hosts.password_required"));
            cx.notify();
            return;
        }
        let Some(prompt) = self.remote_password_prompt.take() else {
            return;
        };
        self.remote_host_secrets
            .insert(prompt.host.account_key(), secret);
        self.run_remote_probe(prompt.host, prompt.then, prompt.attempt + 1, cx);
    }

    pub(super) fn remote_password_cancel(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.remote_password_prompt.take() else {
            return;
        };
        if self.probe_target_alive(&prompt.then) {
            match prompt.then {
                AuthThen::StartTask => self.set_status(tr!("remote_hosts.password_cancelled")),
                then => self.remote_probe_failed(&then, tr!("remote_hosts.password_cancelled"), cx),
            }
        }
        cx.notify();
    }

    fn remote_password_confirm_action(
        &mut self,
        _: &crate::PickerConfirm,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_password_submit(cx);
    }

    fn remote_password_cancel_action(
        &mut self,
        _: &crate::PickerCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_password_cancel(cx);
    }

    /// The password dialog, above Settings and the New Task page.
    pub(super) fn remote_password_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let prompt = self.remote_password_prompt.as_ref()?;
        let destination = prompt.host.connection_label();

        let mut body = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(tr!("remote_hosts.password_body", host = destination)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(input::gap(&theme))
                    .child(
                        input_label(tr!("remote_hosts.password_label"), &theme)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary),
                    )
                    .child(
                        TextField::new(prompt.input.clone(), theme.background)
                            .render(&theme)
                            .debug_selector(|| "remote-password-field".to_string()),
                    ),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("remote_hosts.password_hint")),
            );
        if let Some(error) = &prompt.error {
            body = body.child(
                Banner::note(BannerTone::Error, error.clone(), &theme)
                    .debug_selector(|| "remote-password-error".to_string()),
            );
        }

        let footer = modal_footer(&theme)
            .child({
                let this = this.clone();
                modal_button(
                    "remote-password-cancel",
                    tr!("remote_hosts.cancel"),
                    ButtonVariant::Secondary,
                    &self.modal_cancel_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.remote_password_cancel(cx))
                })
            })
            .child({
                let this = this.clone();
                modal_button(
                    "remote-password-connect",
                    tr!("remote_hosts.connect"),
                    ButtonVariant::Primary,
                    &self.modal_confirm_focus,
                )
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.remote_password_submit(cx))
                })
            });

        let card = modal_card(&theme)
            .debug_selector(|| "remote-password-dialog".to_string())
            .modal_width(px(440.))
            .key_context("Picker")
            .on_action(cx.listener(Self::remote_password_confirm_action))
            .on_action(cx.listener(Self::remote_password_cancel_action))
            .child(modal_header(
                &theme,
                tr!("remote_hosts.password_title"),
                None,
            ))
            .child(
                div()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body),
            )
            .child(footer);

        Some(
            modal_scrim(&theme)
                .id("remote-password-layer")
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| app.remote_password_cancel(cx))
                })
                .child(crate::motion::enter(
                    card,
                    "remote-password-card-in",
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

    #[test]
    fn the_terminal_command_names_the_port_only_when_set() {
        let mut host = RemoteHost {
            name: "vps".into(),
            ssh: "vps.example".into(),
            user: Some("deploy".into()),
            port: None,
            dir: "/srv".into(),
            enabled: true,
            local_dir: None,
        };
        assert_eq!(ssh_command(&host), "ssh deploy@vps.example");
        host.port = Some(2222);
        assert_eq!(ssh_command(&host), "ssh -p 2222 deploy@vps.example");
    }
}
