//! The Git page while the session runs on a remote host. The local Git page
//! reads this machine's disk, so on a remote session it would describe the
//! wrong folder; instead this page shows what the host itself reports about
//! the session's folder — repository or not, branch, upstream and
//! ahead/behind, changed paths, the last commit and `origin` — read over ssh
//! with a read-only script (`orbit_rpc::remote::git_status_script`). Nothing
//! is written on the host, and no Git action is offered here.

use super::*;
use crate::remote_hosts::{self, ProbeFailure, RemoteGit, RemoteGitRepo, RemoteHost};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, TextSize};
use crate::ui::button::IconButton;
use crate::ui::feedback::empty_state_with_action;
use crate::ui::prelude::*;
use crate::ui::style::surface::SurfaceExt;

/// The remote Git page's state: which host folder it describes, whether a
/// read is running, and what the last read returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RemoteGitView {
    /// `account:dir`, so a read for a previous session's folder is dropped.
    pub(crate) key: String,
    pub(crate) loading: bool,
    pub(crate) result: Option<Result<RemoteGit, String>>,
}

/// Which folder on which account a read describes.
pub(crate) fn remote_git_key(host: &RemoteHost) -> String {
    format!("{}:{}", host.account_key(), host.dir)
}

impl OrbitApp {
    /// Read the live remote session's folder state on its host. Never asks
    /// for a password: the session is already connected, so a cached one is
    /// used when the host needs it.
    pub(super) fn refresh_remote_git(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.remote.clone() else {
            return;
        };
        let key = remote_git_key(&host);
        if self.remote_git.key == key && self.remote_git.loading {
            return;
        }
        if self.remote_git.key != key {
            self.remote_git = RemoteGitView {
                key: key.clone(),
                ..Default::default()
            };
        }
        self.remote_git.loading = true;
        cx.notify();
        // Tests build the app without a network.
        if cfg!(test) {
            return;
        }
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let read_host = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { remote_hosts::git_status(&read_host, secret.as_deref()) })
                .await;
            let _ = this.update(cx, |app, cx| app.finish_remote_git(&host, outcome, cx));
        })
        .detach();
    }

    pub(super) fn finish_remote_git(
        &mut self,
        host: &RemoteHost,
        outcome: Result<RemoteGit, ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        if self.remote_git.key != remote_git_key(host) {
            return;
        }
        self.remote_git.loading = false;
        self.remote_git.result = Some(match outcome {
            Ok(git) => Ok(git),
            Err(ProbeFailure::NeedsPassword { .. } | ProbeFailure::WrongPassword) => {
                Err(tr!("remote_sessions.needs_login"))
            }
            Err(failure) => Err(super::remote_providers_ui::failure_text(failure, host)),
        });
        cx.notify();
    }

    /// The page body for Git while the session is remote.
    pub(super) fn render_remote_git_page(&self, theme: Theme, cx: &Context<Self>) -> AnyElement {
        let Some(host) = self.remote.as_ref() else {
            return div().into_any_element();
        };
        let this = cx.entity();
        let refresh = {
            let this = this.clone();
            Button::new("remote-git-refresh", ButtonVariant::Secondary)
                .label(tr!("common.retry"))
                .icon("icons/refresh.svg")
                .on_click(&theme, move |_, _, cx| {
                    this.update(cx, |app, cx| app.refresh_remote_git(cx))
                })
                .into_any_element()
        };
        let current = (self.remote_git.key == remote_git_key(host))
            .then_some(self.remote_git.result.as_ref())
            .flatten();
        match current {
            _ if self.remote_git.loading && current.is_none() => empty_state(
                theme,
                "icons/loader.svg",
                &tr!("remote_git.loading", host = host.name.clone()),
                None,
                EmptyFill::Full,
            ),
            None => empty_state_with_action(
                theme,
                "icons/server-stack.svg",
                &tr!("remote_git.title", host = host.name.clone()),
                Some(&host.dir),
                EmptyFill::Full,
                Some(refresh),
            ),
            Some(Err(error)) => empty_state_with_action(
                theme,
                "icons/stop.svg",
                &tr!("remote_git.unavailable", host = host.name.clone()),
                Some(error),
                EmptyFill::Full,
                Some(refresh),
            ),
            Some(Ok(RemoteGit::Missing)) => empty_state_with_action(
                theme,
                "icons/sidebar-git.svg",
                &tr!("remote_git.missing", host = host.name.clone()),
                Some(&tr!("remote_git.missing_detail")),
                EmptyFill::Full,
                Some(refresh),
            ),
            Some(Ok(RemoteGit::NotARepository)) => empty_state_with_action(
                theme,
                "icons/sidebar-git.svg",
                &tr!("git_panel.fail_not_a_repo"),
                Some(&tr!(
                    "remote_git.not_a_repo_detail",
                    folder = host.dir.clone(),
                    host = host.name.clone()
                )),
                EmptyFill::Full,
                Some(refresh),
            ),
            Some(Ok(RemoteGit::Repository(repo))) => self.remote_git_card(theme, host, repo, this),
        }
    }

    fn remote_git_card(
        &self,
        theme: Theme,
        host: &RemoteHost,
        repo: &RemoteGitRepo,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let unknown = || tr!("remote_git.unknown");
        let branch = repo
            .branch
            .clone()
            .unwrap_or_else(|| tr!("remote_git.detached"));
        let upstream = match (&repo.upstream, repo.ahead_behind) {
            (Some(upstream), Some((ahead, behind))) => tr!(
                "remote_git.upstream_counts",
                upstream = upstream.clone(),
                ahead = ahead,
                behind = behind
            ),
            (Some(upstream), None) => upstream.clone(),
            (None, _) => tr!("remote_git.no_upstream"),
        };
        let changed = match repo.changed {
            Some(0) => tr!("remote_git.clean"),
            Some(count) => tr!("remote_git.changed", count = count),
            None => unknown(),
        };
        let rows: Vec<(String, String, bool)> = vec![
            (tr!("remote_git.folder"), host.dir.clone(), true),
            (
                tr!("remote_git.root"),
                repo.root.clone().unwrap_or_else(unknown),
                true,
            ),
            (tr!("remote_git.branch"), branch, true),
            (tr!("remote_git.upstream"), upstream, false),
            (tr!("remote_git.changes"), changed, false),
            (
                tr!("remote_git.last_commit"),
                repo.head.clone().unwrap_or_else(unknown),
                false,
            ),
            (
                tr!("remote_git.origin"),
                repo.origin
                    .clone()
                    .unwrap_or_else(|| tr!("remote_git.no_origin")),
                true,
            ),
        ];
        let loading = self.remote_git.loading;
        let header = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(icon(
                "icons/server-stack.svg",
                crate::theme::tokens::IconSize::Small.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .truncate()
                    .child(tr!("remote_git.title", host = host.name.clone())),
            )
            .child(if loading {
                spinner(
                    ElementId::Name("remote-git-loading".into()),
                    crate::theme::tokens::IconSize::Small.px(&theme),
                    theme.text_muted,
                    theme,
                )
            } else {
                IconButton::new("remote-git-refresh-icon", "icons/refresh.svg")
                    .size(ButtonSize::Compact)
                    .tip(tr!("remote_git.refresh"))
                    .on_click(&theme, move |_, _, cx| {
                        this.update(cx, |app, cx| app.refresh_remote_git(cx))
                    })
                    .into_any_element()
            });
        let mut card = div()
            .w_full()
            .max_w(px(560.))
            .p(DynamicSpacing::Base16.px(&theme))
            .card_surface(
                &theme,
                theme.hairline(),
                theme.card_fill(theme.surface_elevated),
            )
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(header);
        for (ix, (label, value, mono)) in rows.into_iter().enumerate() {
            let mut value_el = div()
                .id(ElementId::Name(format!("remote-git-value-{ix}").into()))
                .flex_1()
                .min_w_0()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_primary)
                .truncate()
                .tip(value.clone())
                .child(value);
            if mono {
                value_el = value_el.font_family(crate::theme::code_font_family());
            }
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(
                        div()
                            .w(px(120.))
                            .flex_none()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(label),
                    )
                    .child(value_el),
            );
        }
        card = card.child(
            div()
                .pt(DynamicSpacing::Base04.px(&theme))
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("remote_git.read_only_note")),
        );
        div()
            .id("remote-git-page")
            .debug_selector(|| "remote-git-page".into())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(DynamicSpacing::Base24.px(&theme))
            .child(card)
            .into_any_element()
    }
}
