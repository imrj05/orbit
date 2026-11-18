//! OAuth answers exist only in a transient ComposerInput and the outgoing RPC.
//! Authorization codes are masked; provider text questions remain readable.
//! The single-window shell owns this transient UI state as an app global; the
//! auth reducer remains display-only and independent of GPUI.
use super::*;
use crate::theme::tokens::{ButtonSize, DynamicSpacing, TextSize};
use crate::ui::prelude::*;
use crate::ui::prelude::{button_frame, ControlExt, PressedRamp};
use gpui::{Global, RenderOnce};

#[derive(Default)]
struct AuthPromptUi {
    prompt: Option<PromptInput>,
    key_pending: Option<String>,
}
impl Global for AuthPromptUi {}

struct PromptInput {
    session_id: String,
    provider: String,
    request_id: String,
    input: Entity<ComposerInput>,
    focus_pending: bool,
}

impl OrbitApp {
    /// Called for every auth event and heartbeat, even when there are no effects.
    /// Drop/clear answers on close, cancel, timeout, restart, or account switch.
    pub(super) fn sync_auth_prompt(&self, cx: &mut Context<Self>) {
        cx.default_global::<AuthPromptUi>();
        let wanted = self
            .auth
            .login()
            .filter(|login| !login.phase.is_terminal())
            .and_then(|login| login.prompt.as_ref().map(|prompt| (login, prompt)));
        let unchanged = match (wanted, cx.global::<AuthPromptUi>().prompt.as_ref()) {
            (Some((login, prompt)), Some(ui)) => {
                login.id == ui.session_id
                    && login.provider == ui.provider
                    && prompt.request_id == ui.request_id
            }
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        if let Some(old) = cx.global_mut::<AuthPromptUi>().prompt.take() {
            old.input.update(cx, |input, cx| input.clear(cx));
        }
        if let Some((login, prompt)) = wanted {
            let input = cx.new(|cx| {
                ComposerInput::new(cx)
                    .with_element_id("oauth-prompt-input")
                    .with_key_context("Composer Picker")
                    .with_max_lines(1)
                    .with_wrap(false)
                    .with_masked(prompt.kind == "manual_code")
                    .with_placeholder(prompt.placeholder.clone().unwrap_or_default())
            });
            cx.global_mut::<AuthPromptUi>().prompt = Some(PromptInput {
                session_id: login.id.clone(),
                provider: login.provider.clone(),
                request_id: prompt.request_id.clone(),
                input,
                focus_pending: true,
            });
        }
    }

    fn auth_submit_prompt(&mut self, request_id: &str, cx: &mut Context<Self>) {
        let Some(ui) = cx.global::<AuthPromptUi>().prompt.as_ref() else {
            return;
        };
        if ui.request_id != request_id || crate::composer::any_composing(cx, &[&ui.input]) {
            return;
        }
        let value = ui.input.read(cx).text();
        if value.encode_utf16().count() > 16384 {
            return;
        }
        let session_id = ui.session_id.clone();
        let provider = ui.provider.clone();
        if !self.auth.take_prompt(&session_id, &provider, request_id) {
            return;
        }
        self.sync_auth_prompt(cx);
        let result = self.client.as_ref().map(|client| {
            client.send(CommandBody::AuthRespond {
                session_id: session_id.clone(),
                request_id: request_id.to_owned(),
                value,
            })
        });
        if let Some(Ok((_, rx))) = result {
            cx.spawn(async move |this, cx| {
                let response = cx
                    .background_executor()
                    .spawn(async move { rx.recv_timeout(Duration::from_secs(30)).ok() })
                    .await;
                if !matches!(response, Some(Event::Response { success: true, .. })) {
                    let _ = this.update(cx, |app, cx| app.auth_prompt_failed(&session_id, cx));
                }
            })
            .detach();
        } else {
            self.auth_prompt_failed(&session_id, cx);
        }
        cx.notify();
    }

    fn auth_prompt_failed(&mut self, session_id: &str, cx: &mut Context<Self>) {
        if self
            .auth
            .login()
            .is_none_or(|login| login.id != session_id || login.phase.is_terminal())
        {
            return;
        }
        // Do not display a server's error text: an older server may echo input.
        self.auth
            .on_login_response(false, None, Some(&tr!("auth.sign_in_incomplete")));
        self.send(
            CommandBody::AuthCancel {
                session_id: session_id.to_owned(),
            },
            "auth.cancel",
        );
        self.sync_auth_prompt(cx);
        cx.notify();
    }

    /// No selected-profile writer ever opens auth.json in Rust: pi owns its
    /// cross-process lock and synchronizes the live runtime before acknowledging.
    pub(super) fn save_profile_api_key(
        &mut self,
        provider: String,
        name: String,
        key: String,
        cx: &mut Context<Self>,
    ) {
        cx.default_global::<AuthPromptUi>();
        if cx.global::<AuthPromptUi>().key_pending.is_some() {
            return;
        }
        let selected = self.selected_account_id(&provider);
        let editor_input = self
            .provider_key_editor
            .as_ref()
            .map(|editor| editor.key.clone());
        let result = self.client.as_ref().map(|client| {
            client.send(CommandBody::AuthSetApiKey {
                provider: provider.clone(),
                key,
            })
        });
        let Some(Ok((id, rx))) = result else {
            if let Some(editor) = &mut self.provider_key_editor {
                editor.error = Some(tr!("auth.sign_in_incomplete"));
            }
            cx.notify();
            return;
        };
        // Erase the UI copy immediately; never include the answer in a toast,
        // status, diagnostic, reducer, or persisted Orbit state.
        if let Some(editor) = &self.provider_key_editor {
            editor.key.update(cx, |input, cx| input.clear(cx));
        }
        cx.global_mut::<AuthPromptUi>().key_pending = Some(id.clone());
        cx.spawn(async move |this, cx| {
            let response = cx
                .background_executor()
                .spawn(async move { rx.recv_timeout(Duration::from_secs(30)).ok() })
                .await;
            let _ = this.update(cx, |app, cx| {
                if cx.global::<AuthPromptUi>().key_pending.as_deref() != Some(id.as_str()) {
                    return;
                }
                cx.global_mut::<AuthPromptUi>().key_pending = None;
                if app.selected_account_id(&provider) != selected {
                    return;
                }
                let success = matches!(response, Some(Event::Response { success: true, .. }));
                let same_editor = app.provider_key_editor.as_ref().is_some_and(|editor| {
                    editor.provider_id == provider
                        && editor_input
                            .as_ref()
                            .is_some_and(|input| input == &editor.key)
                });
                if success {
                    if same_editor {
                        app.provider_key_editor = None;
                    }
                    app.handle_auth_effects(vec![AuthEffect::RefreshProviders], cx);
                    app.toast_success(tr!(
                        "settings.key_saved_for",
                        what = tr!("settings.api_key"),
                        name = name
                    ));
                } else if same_editor {
                    if let Some(editor) = &mut app.provider_key_editor {
                        // Fixed copy: an old/over-sharing server must not echo a key.
                        editor.error = Some(tr!("auth.sign_in_incomplete"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn sign_out_profile(&mut self, provider: String, cx: &mut Context<Self>) {
        let selected = self.selected_account_id(&provider);
        let result = self.client.as_ref().map(|client| {
            client.send(CommandBody::AuthLogout {
                provider: provider.clone(),
            })
        });
        let Some(Ok((_, rx))) = result else {
            self.provider_auth_error = Some(tr!("auth.sign_in_incomplete"));
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let response = cx
                .background_executor()
                .spawn(async move { rx.recv_timeout(Duration::from_secs(30)).ok() })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.selected_account_id(&provider) != selected {
                    return;
                }
                if matches!(response, Some(Event::Response { success: true, .. })) {
                    app.auth.on_logout_response(true, &provider);
                    app.handle_auth_effects(vec![AuthEffect::RefreshProviders], cx);
                    app.toast_success(tr!("settings.signed_out", id = provider));
                } else {
                    app.provider_auth_error = Some(tr!("auth.sign_in_incomplete"));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn selected_auth_paths(&self) -> Vec<(String, Result<PathBuf, String>)> {
        self.provider_accounts
            .selection()
            .into_keys()
            .map(|provider| {
                let path = self
                    .account_auth_path(&provider)
                    .map_err(|_| tr!("settings.auth_json_read_error"));
                (provider, path)
            })
            .collect()
    }
}

/// Overlay *only* selected providers with metadata from their canonical files.
/// Never merge another provider/credential from a profile into the Default map.
pub(super) fn overlay_account_auth(
    auth: Result<HashMap<String, providers::ProviderAuth>, String>,
    paths: Vec<(String, Result<PathBuf, String>)>,
) -> Result<HashMap<String, providers::ProviderAuth>, String> {
    let mut auth = auth?;
    for (provider, path) in paths {
        auth.remove(&provider);
        let path = path?;
        let bytes = std::fs::read(path).map_err(|_| tr!("settings.auth_json_read_error"))?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| tr!("settings.auth_json_read_error"))?;
        let kind = match value
            .get(&provider)
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str)
        {
            Some("oauth") => Some(providers::AuthKind::OAuth),
            Some("api_key") => Some(providers::AuthKind::ApiKey),
            _ => None,
        };
        if let Some(kind) = kind {
            auth.insert(provider, providers::ProviderAuth { kind });
        }
    }
    Ok(auth)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ProfileFile(PathBuf);
    impl ProfileFile {
        fn new(contents: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            // Tests run in parallel: the counter keeps two fixtures created in
            // the same nanosecond from sharing (and deleting) one file.
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "orbit-profile-auth-{}-{unique}-{n}.json",
                std::process::id()
            ));
            std::fs::write(&path, contents).unwrap();
            Self(path)
        }
    }
    impl Drop for ProfileFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn default_auth() -> Result<HashMap<String, providers::ProviderAuth>, String> {
        Ok(HashMap::from([
            (
                "example".into(),
                providers::ProviderAuth {
                    kind: providers::AuthKind::OAuth,
                },
            ),
            (
                "other".into(),
                providers::ProviderAuth {
                    kind: providers::AuthKind::ApiKey,
                },
            ),
        ]))
    }

    #[test]
    fn empty_selected_profile_does_not_show_defaults_credential() {
        let profile = ProfileFile::new("{}");
        let auth = overlay_account_auth(
            default_auth(),
            vec![("example".into(), Ok(profile.0.clone()))],
        )
        .unwrap();
        assert!(!auth.contains_key("example"));
        assert_eq!(auth["other"].kind, providers::AuthKind::ApiKey);
    }

    #[test]
    fn selected_metadata_is_provider_scoped_and_contains_no_key() {
        let profile = ProfileFile::new(
            r#"{"example":{"type":"api_key","key":"synthetic-private-key"},"other":{"type":"oauth","refresh":"synthetic-refresh"}}"#,
        );
        let auth = overlay_account_auth(
            default_auth(),
            vec![("example".into(), Ok(profile.0.clone()))],
        )
        .unwrap();
        assert_eq!(auth["example"].kind, providers::AuthKind::ApiKey);
        assert_eq!(auth["other"].kind, providers::AuthKind::ApiKey);
        let rendered = format!("{auth:?}");
        assert!(!rendered.contains("synthetic"));
    }

    #[test]
    fn invalid_selected_metadata_fails_closed_without_echoing_contents() {
        let profile = ProfileFile::new("not json synthetic-private-key");
        let error = overlay_account_auth(
            default_auth(),
            vec![("example".into(), Ok(profile.0.clone()))],
        )
        .unwrap_err();
        assert!(!error.contains("synthetic-private-key"));
        assert!(overlay_account_auth(
            default_auth(),
            vec![("example".into(), Err("unavailable".into()))]
        )
        .is_err());
        std::fs::remove_file(&profile.0).unwrap();
        assert!(overlay_account_auth(
            default_auth(),
            vec![("example".into(), Ok(profile.0.clone()))]
        )
        .is_err());
    }
}

#[derive(IntoElement)]
pub(super) struct AuthPromptView {
    pub app: Entity<OrbitApp>,
    pub request_id: String,
}

impl RenderOnce for AuthPromptView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *theme::get(cx);
        let Some((input, focus_pending)) = cx
            .try_global::<AuthPromptUi>()
            .and_then(|state| state.prompt.as_ref())
            .filter(|ui| ui.request_id == self.request_id)
            .map(|ui| (ui.input.clone(), ui.focus_pending))
        else {
            return div();
        };
        if focus_pending {
            if let Some(prompt) = cx.global_mut::<AuthPromptUi>().prompt.as_mut() {
                prompt.focus_pending = false;
            }
            window.focus(&input.read(cx).focus_handle(cx));
        }
        let confirm = self.app.clone();
        let request = self.request_id.clone();
        let cancel = self.app.clone();
        let click = self.app;
        let click_request = self.request_id;
        div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .on_action(move |_: &crate::PickerConfirm, _, cx| {
                confirm.update(cx, |app, cx| app.auth_submit_prompt(&request, cx));
            })
            .on_action(move |_: &crate::PickerCancel, _, cx| {
                cancel.update(cx, |app, cx| {
                    app.auth_cancel_login(cx);
                    app.sync_auth_prompt(cx);
                });
            })
            .child(input)
            .child(
                button_frame(div().id("oauth-prompt-submit"), &theme, ButtonSize::Medium)
                    .cursor_pointer()
                    .raised(theme.action, &theme)
                    .text_color(theme.action_fg)
                    .hover_action(&theme)
                    .pressed_state(&theme, PressedRamp::Action)
                    .text_size(TextSize::Small.px(&theme))
                    .child(tr!("ask.submit"))
                    .on_click(move |_, _, cx| {
                        click.update(cx, |app, cx| app.auth_submit_prompt(&click_request, cx));
                    }),
            )
    }
}
