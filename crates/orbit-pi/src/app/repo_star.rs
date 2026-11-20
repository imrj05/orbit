//! Settings → About: star Orbit's repository from the app.
//!
//! Starring acts on the user's GitHub account, so it goes through the `gh`
//! CLI the user already signed in to (no token is collected here) and only on
//! an explicit click. The state is read once per launch, the first time About
//! is shown; when `gh` is missing, signed out or lacks the scope, the card
//! keeps its original behavior — opening the repository in the browser.

use super::*;
use crate::gh;

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

    /// The trailing control on the About star card, or `None` when the card
    /// should keep opening the browser (unknown, unavailable).
    pub(super) fn repo_star_button(&self, theme: Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let (starred, loading) = match self.repo_star {
            RepoStar::Unknown | RepoStar::Unavailable => return None,
            RepoStar::Checking => (false, true),
            RepoStar::NotStarred => (false, false),
            RepoStar::Starred => (true, false),
            // Show where the click is heading while GitHub answers.
            RepoStar::Updating(target) => (target, true),
        };
        let label = if starred {
            tr!("settings.starred")
        } else {
            tr!("settings.star")
        };
        let tip = if starred {
            tr!("settings.unstar_tip")
        } else {
            tr!("settings.star_tip")
        };
        let app = cx.entity();
        Some(
            Button::keyed("about-star-toggle", ButtonVariant::Secondary)
                .size(ButtonSize::Compact)
                .icon(if starred {
                    "icons/check.svg"
                } else {
                    "icons/star.svg"
                })
                .label(label)
                .loading(loading)
                .tip(tip)
                .on_mouse_up(&theme, move |_, _, cx| {
                    // The card under the button opens the browser on release;
                    // this click is the star itself.
                    cx.stop_propagation();
                    app.update(cx, |app, cx| app.toggle_repo_star(cx));
                })
                .into_any_element(),
        )
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
}
