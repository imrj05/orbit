//! Settings → About: star Orbit's repository from the app.
//!
//! Starring acts on the user's GitHub account, so it goes through the `gh`
//! CLI the user already signed in to (no token is collected here) and only on
//! an explicit click. The state is read once per launch, the first time About
//! is shown; when `gh` is missing, signed out or lacks the scope, the Star
//! button opens the repository in the browser instead.

use super::*;
use crate::gh;
use crate::theme::tokens::ButtonSize;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::prelude::icon;

/// What the About card knows about the user's star on Orbit's repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RepoStar {
    /// Not asked yet this launch.
    #[default]
    Unknown,
    /// The starred probe is in flight.
    Checking,
    /// `gh` cannot star here (missing, signed out, no scope, or the manifest
    /// is not a GitHub URL): the card opens the browser instead.
    Unavailable,
    NotStarred,
    Starred,
    /// A star (`true`) or unstar (`false`) request is in flight.
    Updating(bool),
}

impl RepoStar {
    /// The state a finished probe settles into.
    pub(crate) fn after_check(result: &Result<bool, String>) -> Self {
        match result {
            Ok(true) => Self::Starred,
            Ok(false) => Self::NotStarred,
            Err(_) => Self::Unavailable,
        }
    }

    /// The state a finished star/unstar settles into: the target on success,
    /// the state before the click on failure (GitHub did not change).
    pub(crate) fn after_update(target: bool, result: &Result<(), String>) -> Self {
        match (result, target) {
            (Ok(()), true) | (Err(_), false) => Self::Starred,
            (Ok(()), false) | (Err(_), true) => Self::NotStarred,
        }
    }
}

fn repo_slug() -> Option<(String, String)> {
    gh::github_repo_slug(env!("CARGO_PKG_REPOSITORY"))
}

impl OrbitApp {
    /// Heartbeat hook: the first time About is on screen, ask `gh` whether the
    /// repository is starred. Driven from the tick because several paths
    /// switch the settings section by assignment.
    pub(super) fn tick_repo_star(&mut self, cx: &mut Context<Self>) {
        if self.repo_star != RepoStar::Unknown
            || !self.settings_open
            || self.settings_section != SettingsSection::About
        {
            return;
        }
        let Some((owner, name)) = repo_slug() else {
            self.repo_star = RepoStar::Unavailable;
            return;
        };
        self.repo_star = RepoStar::Checking;
        let probe = cx.background_spawn(async move { gh::repo_starred(&owner, &name) });
        self.repo_star_task = Some(cx.spawn(async move |this, cx| {
            let result = probe.await;
            this.update(cx, |app, cx| {
                app.repo_star = RepoStar::after_check(&result);
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// The About card's Star / Starred button: star or unstar through `gh`.
    pub(super) fn toggle_repo_star(&mut self, cx: &mut Context<Self>) {
        let target = match self.repo_star {
            RepoStar::NotStarred => true,
            RepoStar::Starred => false,
            _ => return,
        };
        let Some((owner, name)) = repo_slug() else {
            return;
        };
        self.repo_star = RepoStar::Updating(target);
        let request =
            cx.background_spawn(async move { gh::set_repo_starred(&owner, &name, target) });
        self.repo_star_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |app, cx| {
                app.repo_star = RepoStar::after_update(target, &result);
                match (&result, target) {
                    (Ok(()), true) => app.toast_success(tr!("settings.starred_thanks")),
                    (Ok(()), false) => {}
                    (Err(error), true) => {
                        app.toast_error(tr!("settings.star_failed", error = error))
                    }
                    (Err(error), false) => {
                        app.toast_error(tr!("settings.unstar_failed", error = error))
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// About's Star control, beside the GitHub link. With a usable `gh` it
    /// stars or unstars in place (Starred carries the accent star); without
    /// one — unknown, missing, signed out — it opens the repository in the
    /// browser, so the button never promises an action it cannot take.
    pub(super) fn repo_star_button(&self, theme: Theme, cx: &Context<Self>) -> AnyElement {
        let in_app = matches!(
            self.repo_star,
            RepoStar::Checking
                | RepoStar::NotStarred
                | RepoStar::Starred
                | RepoStar::Updating(_)
        );
        let (starred, loading) = match self.repo_star {
            RepoStar::Unknown | RepoStar::Unavailable | RepoStar::NotStarred => (false, false),
            RepoStar::Checking => (false, true),
            RepoStar::Starred => (true, false),
            // Show where the click is heading while GitHub answers.
            RepoStar::Updating(target) => (target, true),
        };
        let (label, tip) = match (in_app, starred) {
            (false, _) => (tr!("settings.star"), tr!("settings.star_tip_browser")),
            (true, false) => (tr!("settings.star"), tr!("settings.star_tip")),
            (true, true) => (tr!("settings.starred"), tr!("settings.unstar_tip")),
        };
        let icon_px = ButtonSize::Medium.icon_size().px(&theme);
        let mut button = Button::keyed("about-star-toggle", ButtonVariant::Secondary)
            .size(ButtonSize::Medium)
            .label(label)
            .loading(loading)
            .tip(tip);
        // The one accent on the page: a star you have given.
        button = if starred && !loading {
            button.leading(icon("icons/star.svg", icon_px, theme.accent_text))
        } else {
            button.icon("icons/star.svg")
        };
        let app = cx.entity();
        button
            .on_mouse_up(&theme, move |_, _, cx| {
                if in_app {
                    app.update(cx, |app, cx| app.toggle_repo_star(cx));
                } else {
                    cx.open_url(env!("CARGO_PKG_REPOSITORY"));
                }
            })
            .debug_selector(|| "about-star-toggle".to_string())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::RepoStar;

    #[test]
    fn a_probe_settles_into_starred_not_starred_or_unavailable() {
        assert_eq!(RepoStar::after_check(&Ok(true)), RepoStar::Starred);
        assert_eq!(RepoStar::after_check(&Ok(false)), RepoStar::NotStarred);
        assert_eq!(
            RepoStar::after_check(&Err("gh auth login".into())),
            RepoStar::Unavailable
        );
    }

    #[test]
    fn a_failed_update_keeps_the_state_github_still_has() {
        assert_eq!(RepoStar::after_update(true, &Ok(())), RepoStar::Starred);
        assert_eq!(RepoStar::after_update(false, &Ok(())), RepoStar::NotStarred);
        assert_eq!(
            RepoStar::after_update(true, &Err("HTTP 403".into())),
            RepoStar::NotStarred
        );
        assert_eq!(
            RepoStar::after_update(false, &Err("HTTP 403".into())),
            RepoStar::Starred
        );
    }

    /// About is one identity header (mark, name, version line) over one
    /// board; Star sits beside the GitHub link on the Source row, and with no
    /// `gh` answer yet it is still offered (it opens the browser).
    #[gpui::test]
    fn about_shows_identity_and_star_beside_github(cx: &mut gpui::TestAppContext) {
        use super::super::*;
        use crate::theme::{Theme, ThemeId};
        use gpui::{point, px};

        let cx = cx.add_empty_window();
        cx.update(|_, cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        let viewport = gpui::size(px(1200.), px(820.));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::About;
                app.repo_star = RepoStar::Unavailable;
                cx.notify();
            });
        });
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        let title = cx.debug_bounds("settings-page-title").expect("identity name");
        let version = cx.debug_bounds("about-version").expect("version line");
        assert!(version.origin.y > title.origin.y, "version sits under the name");
        let star = cx.debug_bounds("about-star-toggle").expect("star offered");
        let github = cx.debug_bounds("about-github").expect("github link");
        assert_eq!(star.size.height, github.size.height, "one control height");
        assert_eq!(star.origin.y, github.origin.y, "star and GitHub share a row");
        assert!(star.origin.x < github.origin.x, "star leads the link");
    }
}
