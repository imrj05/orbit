//! The right pane's tabs: **Review** first, then one tab per delegated
//! subagent child the reader opened from a delegation card.
//!
//! Pure state, no I/O and no GPUI: `app/right_dock_ui.rs` paints it.
//!
//! The tabs belong to a session — they ride on its [`crate::transcript::Transcript`],
//! which is parked and restored with the session. Whether the pane is *shown*
//! is the app-wide pane flag, which this model never touches. That split is
//! the rule upstream's `aafd644f` had to patch in after the fact: switching
//! away and back restores the session's tabs, and must not pop a pane the
//! reader hid back open. Only an explicit click on "Open in side pane" shows
//! the pane, and the caller does that.

/// One delegated child, addressed the way the transcript addresses its
/// delegation card: the message, the tool's flat index within that message's
/// steps, and the child within the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SubagentTab {
    pub message_ix: usize,
    pub tool_ix: usize,
    pub child_ix: usize,
}

/// What the right pane is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockTab {
    Review,
    Subagent(SubagentTab),
}

/// A session's right-pane tabs. Review is always present and is the fallback;
/// subagent tabs are added by an explicit open and kept in opening order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DockTabs {
    subagents: Vec<SubagentTab>,
    /// `None` is Review.
    active: Option<SubagentTab>,
}

impl DockTabs {
    /// The subagent tabs, in the order they were opened.
    pub(crate) fn subagents(&self) -> &[SubagentTab] {
        &self.subagents
    }

    /// No subagent tabs: the pane is plain Review and draws no tab strip.
    pub(crate) fn is_empty(&self) -> bool {
        self.subagents.is_empty()
    }

    pub(crate) fn active(&self) -> DockTab {
        match self.active {
            Some(tab) => DockTab::Subagent(tab),
            None => DockTab::Review,
        }
    }

    /// Open `tab` (or bring an already open one forward) and make it active.
    pub(crate) fn open(&mut self, tab: SubagentTab) {
        if !self.subagents.contains(&tab) {
            self.subagents.push(tab);
        }
        self.active = Some(tab);
    }

    /// Make `tab` active. A subagent tab that is not open is ignored, so a
    /// stale click cannot conjure a tab.
    pub(crate) fn activate(&mut self, tab: DockTab) {
        match tab {
            DockTab::Review => self.active = None,
            DockTab::Subagent(tab) if self.subagents.contains(&tab) => self.active = Some(tab),
            DockTab::Subagent(_) => {}
        }
    }

    /// Close `tab`. Closing the active one moves to its left neighbour, then
    /// its right one, then Review — the way editor tabs fall back. Returns
    /// whether anything closed.
    pub(crate) fn close(&mut self, tab: SubagentTab) -> bool {
        let Some(ix) = self.subagents.iter().position(|open| *open == tab) else {
            return false;
        };
        self.subagents.remove(ix);
        if self.active == Some(tab) {
            self.active = ix
                .checked_sub(1)
                .and_then(|left| self.subagents.get(left))
                .or_else(|| self.subagents.get(ix))
                .copied();
        }
        true
    }

    /// Drop the tabs `keep` rejects (a reload whose rows no longer hold that
    /// delegation). An active tab that goes falls back to Review.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&SubagentTab) -> bool) {
        self.subagents.retain(|tab| keep(tab));
        if self
            .active
            .is_some_and(|active| !self.subagents.contains(&active))
        {
            self.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(child_ix: usize) -> SubagentTab {
        SubagentTab {
            message_ix: 1,
            tool_ix: 0,
            child_ix,
        }
    }

    #[test]
    fn review_is_the_default_and_the_fallback() {
        let mut dock = DockTabs::default();
        assert!(dock.is_empty());
        assert_eq!(dock.active(), DockTab::Review);
        dock.open(tab(0));
        assert_eq!(dock.active(), DockTab::Subagent(tab(0)));
        assert!(dock.close(tab(0)));
        assert!(dock.is_empty());
        assert_eq!(dock.active(), DockTab::Review);
    }

    #[test]
    fn opening_again_brings_the_tab_forward_without_duplicating_it() {
        let mut dock = DockTabs::default();
        dock.open(tab(0));
        dock.open(tab(1));
        dock.open(tab(0));
        assert_eq!(dock.subagents(), &[tab(0), tab(1)]);
        assert_eq!(dock.active(), DockTab::Subagent(tab(0)));
    }

    #[test]
    fn closing_the_active_tab_falls_back_left_then_right_then_review() {
        let mut dock = DockTabs::default();
        for child in 0..3 {
            dock.open(tab(child));
        }
        dock.activate(DockTab::Subagent(tab(1)));
        dock.close(tab(1));
        assert_eq!(dock.active(), DockTab::Subagent(tab(0)), "left neighbour");
        dock.close(tab(0));
        assert_eq!(dock.active(), DockTab::Subagent(tab(2)), "then the right one");
        dock.close(tab(2));
        assert_eq!(dock.active(), DockTab::Review);
    }

    #[test]
    fn closing_a_background_tab_keeps_the_active_one() {
        let mut dock = DockTabs::default();
        dock.open(tab(0));
        dock.open(tab(1));
        dock.close(tab(0));
        assert_eq!(dock.active(), DockTab::Subagent(tab(1)));
        assert!(!dock.close(tab(0)), "closing twice is a no-op");
    }

    #[test]
    fn a_stale_activation_cannot_conjure_a_tab() {
        let mut dock = DockTabs::default();
        dock.activate(DockTab::Subagent(tab(4)));
        assert!(dock.is_empty());
        assert_eq!(dock.active(), DockTab::Review);
    }

    #[test]
    fn a_reload_drops_tabs_whose_delegation_is_gone() {
        let mut dock = DockTabs::default();
        dock.open(tab(0));
        dock.open(tab(1));
        dock.retain(|open| open.child_ix == 0);
        assert_eq!(dock.subagents(), &[tab(0)]);
        assert_eq!(dock.active(), DockTab::Review, "the active tab went");
    }
}
