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

    /// The live remote session's repository as the host last reported it:
    /// `None` until a read for this session's folder has come back, and for a
    /// folder that is not a repository or a read that failed.
    pub(super) fn remote_repo(&self) -> Option<&RemoteGitRepo> {
        let host = self.remote.as_ref()?;
        if self.remote_git.key != remote_git_key(host) {
            return None;
        }
        match &self.remote_git.result {
            Some(Ok(RemoteGit::Repository(repo))) => Some(repo),
            _ => None,
        }
    }

    /// The branch chip while the session runs on a host: this Mac's branch
    /// selector (`branch_chip_face`, the same `BranchPicker`) showing the
    /// host's own branch, ahead/behind and branches, read over ssh. A pick or
    /// "Create and checkout" switches the folder **on the host**. Absent on a
    /// detached HEAD or a folder that is not a repository, like the local chip.
    pub(super) fn remote_branch_chip(
        &self,
        theme: Theme,
        opens_below: bool,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let host = self.remote.as_ref()?;
        let repo = self.remote_repo()?;
        let name = repo.branch.clone()?;
        let open = self.branch_picker.is_some();
        let face = Self::branch_chip_face(
            "remote-branch-chip",
            name,
            repo.ahead_behind
                .map(|(ahead, behind)| (ahead as usize, behind as usize)),
            open,
            self.branch_operation_pending,
            (!open).then(|| tr!("remote_git.branch_chip_tip", host = host.name.clone())),
            theme,
            cx,
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _, window, cx| app.toggle_branch_picker(window, cx)),
        );
        let popup = if opens_below {
            self.branch_picker_popup_below(&theme)
        } else {
            self.branch_picker_popup(&theme)
        };
        Some(Self::branch_chip_shell(face, popup))
    }

    /// Open the branch picker on the host's branches. The list is read over
    /// ssh first (the chip dims meanwhile), then the same `BranchPicker` this
    /// Mac uses opens with it.
    pub(super) fn open_remote_branch_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.remote.clone() else {
            return;
        };
        let Some(current) = self.remote_repo().and_then(|repo| repo.branch.clone()) else {
            return;
        };
        self.branch_operation_pending = true;
        cx.notify();
        // Tests build the app without a network.
        if cfg!(test) {
            return;
        }
        let secret = self.remote_secret(&host);
        cx.spawn_in(window, async move |this, cx| {
            let list_host = host.clone();
            let listed = cx
                .background_executor()
                .spawn(async move { remote_hosts::list_branches(&list_host, secret.as_deref()) })
                .await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.branch_operation_pending = false;
                match listed {
                    Ok(branches) => {
                        app.show_remote_branch_picker(&host, branches, current, window, cx)
                    }
                    Err(failure) => app.toast_error(tr!(
                        "pickers.branch_list_failed",
                        error = super::remote_providers_ui::failure_text(failure, &host)
                    )),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn show_remote_branch_picker(
        &mut self,
        host: &RemoteHost,
        mut branches: Vec<crate::git::BranchEntry>,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The session may have left this host while the list was read.
        if self.remote.as_ref().map(remote_git_key) != Some(remote_git_key(host)) {
            return;
        }
        if !branches.iter().any(|entry| entry.name == current) {
            branches.insert(
                0,
                crate::git::BranchEntry {
                    name: current.clone(),
                    commit_date: None,
                },
            );
        }
        let label = host
            .dir
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(&host.name)
            .to_string();
        let this = cx.weak_entity();
        let on_checkout = Box::new(move |branch: String, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| app.switch_remote_branch(branch, false, cx))
                .ok();
        }) as Box<dyn Fn(String, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_create = Box::new(move |name: String, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| app.switch_remote_branch(name, true, cx))
                .ok();
        }) as Box<dyn Fn(String, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_dismiss = Box::new(move |by_mouse: bool, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                if by_mouse {
                    app.menu_dismissed_at.arm();
                }
                app.branch_picker = None;
                cx.notify();
            })
            .ok();
        }) as Box<dyn Fn(bool, &mut Window, &mut App)>;
        let picker = cx.new(|cx| {
            crate::branch_picker::BranchPicker::new(
                label,
                branches,
                current,
                on_checkout,
                on_create,
                on_dismiss,
                cx,
            )
        });
        window.focus(&picker.read(cx).focus_handle(cx));
        self.branch_picker = Some(picker);
    }

    /// Switch the host's folder to `branch` (or create it from HEAD with
    /// `create`), then read the folder again so the chip and the Git page
    /// follow.
    pub(super) fn switch_remote_branch(
        &mut self,
        branch: String,
        create: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.remote.clone() else {
            return;
        };
        let branch = branch.trim().to_string();
        if !orbit_rpc::remote::is_branch_name(&branch) {
            self.toast_error(tr!("git.branch_name_invalid"));
            cx.notify();
            return;
        }
        self.branch_operation_pending = true;
        self.branch_picker = None;
        cx.notify();
        if cfg!(test) {
            return;
        }
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let run_host = host.clone();
            let name = branch.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    remote_hosts::switch_branch(&run_host, secret.as_deref(), &name, create)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_branch_switch(&host, branch, create, outcome, cx);
            });
        })
        .detach();
    }

    pub(super) fn finish_remote_branch_switch(
        &mut self,
        host: &RemoteHost,
        branch: String,
        create: bool,
        outcome: Result<Result<(), String>, ProbeFailure>,
        cx: &mut Context<Self>,
    ) {
        self.branch_operation_pending = false;
        match outcome {
            Ok(Ok(())) if create => {
                self.toast_success(tr!("pickers.created_and_switched", label = branch))
            }
            Ok(Ok(())) => self.toast_success(tr!("pickers.switched_to", label = branch)),
            Ok(Err(reason)) if create => {
                self.toast_error(tr!("pickers.branch_create_failed", err = reason))
            }
            Ok(Err(reason)) => {
                self.toast_error(tr!("pickers.branch_switch_failed", error = reason))
            }
            Err(failure) => self.toast_error(tr!(
                "pickers.branch_switch_failed",
                error = super::remote_providers_ui::failure_text(failure, host)
            )),
        }
        self.refresh_remote_git(cx);
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
