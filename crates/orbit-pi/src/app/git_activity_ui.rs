//! The sidebar Git button's activity: the background GitHub check for new
//! issues and pull requests, marking a list seen when the user opens it, the
//! Settings → Privacy switch, and the button's tooltip.
//!
//! Local changes are counted with the branch fetch (`refresh_branch_status`);
//! everything here is the GitHub half. The pure rules live in
//! [`crate::git_activity`].
use super::*;
use crate::git_activity::{self, Kind, Stamp};

/// What one GitHub check read: the workspace it ran in, the repository's
/// `owner/name`, the signed-in login, and the open issues and pull requests.
/// Workspace, `owner/name`, login, open issues, open pull requests, and how
/// many open pull requests wait on the user's review.
type CheckResult = (PathBuf, String, Option<String>, Vec<Stamp>, Vec<Stamp>, usize);

impl OrbitApp {
    /// Heartbeat: mark a GitHub tab seen when the user arrives on it, and
    /// start a check when one is due — every [`git_activity::POLL_INTERVAL`],
    /// and at once for a workspace not checked yet.
    pub(super) fn tick_git_activity(&mut self, cx: &mut Context<Self>) {
        self.mark_open_github_tab_seen(cx);
        let moved = self.github_checked_for != self.current_workspace;
        if moved && (self.git_activity.new_issues > 0 || self.git_activity.new_pulls > 0) {
            // The counts belong to the repository just left.
            self.set_github_new(0, 0, cx);
        }
        let due = moved
            || self
                .github_checked_at
                .is_none_or(|at| at.elapsed() >= git_activity::POLL_INTERVAL);
        if due {
            self.check_github(cx);
        }
    }

    /// The window came forward: check again, unless a check ran within
    /// [`git_activity::MIN_POLL_GAP`].
    pub(super) fn check_github_on_focus(&mut self, cx: &mut Context<Self>) {
        if self
            .github_checked_at
            .is_some_and(|at| at.elapsed() >= git_activity::MIN_POLL_GAP)
        {
            self.check_github(cx);
        }
    }

    /// Ask `gh` for the current repository's open issues and pull requests,
    /// off the UI thread. Without `gh`, a sign-in or a GitHub remote the
    /// check simply finds nothing; it runs again on the next interval.
    fn check_github(&mut self, cx: &mut Context<Self>) {
        // Tests drive real `OrbitApp`s through the heartbeat; they must not
        // start `gh` (a network call) on the developer's account.
        if cfg!(test) || !self.git_activity_store.github_checks || self.github_checking {
            return;
        }
        // A remote session's folder is on another machine.
        if self.remote.is_some() {
            return;
        }
        let Some(cwd) = self.current_workspace.clone() else {
            return;
        };
        self.github_checking = true;
        self.github_checked_at = Some(Instant::now());
        self.github_checked_for = Some(cwd.clone());
        let known_repo = self
            .github_repo
            .as_ref()
            .filter(|(path, _)| *path == cwd)
            .map(|(_, name)| name.clone());
        let known_login = self.github_login.clone();
        cx.spawn(async move |this, cx| {
            let result: Option<CheckResult> = cx
                .background_executor()
                .spawn(async move {
                    let repo = match known_repo {
                        Some(repo) => repo,
                        None => crate::gh::repo_name(&cwd).ok()?,
                    };
                    let login = known_login
                        .or_else(|| crate::gh::current_user(&cwd).ok().map(|user| user.login));
                    let issues = crate::gh::open_stamps(&cwd, Kind::Issues).ok()?;
                    let pulls = crate::gh::open_stamps(&cwd, Kind::Pulls).ok()?;
                    // A failed count is no count, not a failed check.
                    let reviews = crate::gh::review_requested_count(&cwd).unwrap_or(0);
                    Some((cwd, repo, login, issues, pulls, reviews))
                })
                .await;
            let _ = this.update(cx, |app, cx| app.apply_github_check(result, cx));
        })
        .detach();
    }

    fn apply_github_check(&mut self, result: Option<CheckResult>, cx: &mut Context<Self>) {
        self.github_checking = false;
        let Some((cwd, repo, login, issues, pulls, reviews)) = result else {
            self.set_github_new(0, 0, cx);
            self.set_review_requests(0, cx);
            return;
        };
        // The user switched workspace while `gh` ran; the next tick checks
        // the new one.
        if self.current_workspace.as_ref() != Some(&cwd) {
            return;
        }
        self.github_repo = Some((cwd, repo.clone()));
        if login.is_some() {
            self.github_login = login;
        }
        let now = git_activity::now_stamp();
        if self.git_activity_store.baseline(&repo, &now) {
            self.persist_git_activity_store(cx);
        }
        let me = self.github_login.as_deref();
        let new = |kind: Kind, items: &[Stamp]| {
            let since = self
                .git_activity_store
                .seen_since(&repo, kind)
                .unwrap_or(now.as_str());
            git_activity::count_new(items, since, me)
        };
        let (new_issues, new_pulls) = (new(Kind::Issues, &issues), new(Kind::Pulls, &pulls));
        self.set_github_new(new_issues, new_pulls, cx);
        self.set_review_requests(reviews, cx);
    }

    fn set_review_requests(&mut self, count: usize, cx: &mut Context<Self>) {
        if self.git_activity.review_requests != count {
            self.git_activity.review_requests = count;
            cx.notify();
        }
    }

    /// Arriving on the Git page's Issues or Pull requests tab marks that list
    /// seen for the current repository, once per arrival.
    fn mark_open_github_tab_seen(&mut self, cx: &mut Context<Self>) {
        let tab = if self.git_open {
            self.git_panel.read(cx).github_tab()
        } else {
            None
        };
        if tab == self.git_host_tab {
            return;
        }
        self.git_host_tab = tab;
        let Some(kind) = tab else {
            return;
        };
        let Some(repo) = self
            .github_repo
            .as_ref()
            .filter(|(path, _)| Some(path) == self.current_workspace.as_ref())
            .map(|(_, name)| name.clone())
        else {
            return;
        };
        self.git_activity_store
            .mark_seen(&repo, kind, &git_activity::now_stamp());
        self.persist_git_activity_store(cx);
        let (issues, pulls) = match kind {
            Kind::Issues => (0, self.git_activity.new_pulls),
            Kind::Pulls => (self.git_activity.new_issues, 0),
        };
        self.set_github_new(issues, pulls, cx);
    }

    /// Record new counts and hand them to the Git page's tabs.
    fn set_github_new(&mut self, issues: usize, pulls: usize, cx: &mut Context<Self>) {
        if (self.git_activity.new_issues, self.git_activity.new_pulls) != (issues, pulls) {
            self.git_activity.new_issues = issues;
            self.git_activity.new_pulls = pulls;
            cx.notify();
        }
        self.git_panel
            .update(cx, |panel, cx| panel.set_github_new(issues, pulls, cx));
    }

    /// Write the store off the UI thread.
    fn persist_git_activity_store(&self, cx: &mut Context<Self>) {
        let store = self.git_activity_store.clone();
        cx.background_spawn(async move { store.persist() }).detach();
    }

    /// Settings → Privacy: turn the background GitHub check on or off. Off
    /// clears the counts at once; on checks right away.
    pub(super) fn toggle_github_checks(&mut self, cx: &mut Context<Self>) {
        self.git_activity_store.github_checks = !self.git_activity_store.github_checks;
        self.persist_git_activity_store(cx);
        if self.git_activity_store.github_checks {
            self.github_checked_at = None;
            self.check_github(cx);
        } else {
            self.set_github_new(0, 0, cx);
            self.set_review_requests(0, cx);
        }
        cx.notify();
    }

    /// The sidebar Git button's tooltip: the command and its chord, then why
    /// the button is marked ("Git (⌘⇧G) · 4 changed files · 1 new issue").
    pub(super) fn git_nav_tooltip(&self) -> String {
        let mut parts = vec![crate::commands::tooltip(crate::commands::CommandId::OpenGit, false)];
        parts.extend(self.git_activity.reasons());
        parts.join(" · ")
    }
}
