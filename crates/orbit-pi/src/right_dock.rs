//! The Subagents dock: one entry per delegated subagent child the reader
//! opened from a delegation card, and which of them is showing.
//!
//! Review is not part of this model. It is its own right dock with its own
//! toggle, so a subagent's response never lands in the review section
//! (issue #190); the two only share the right-dock slot, like the Explorer
//! and session details do.
//!
//! Pure state, no I/O and no GPUI: `app/right_dock_ui.rs` paints it.
//!
//! The entries belong to a session — they ride on its [`crate::transcript::Transcript`],
//! which is parked and restored with the session. Whether the dock is *shown*
//! is the app-wide `subagent_dock_open` flag, which this model never touches.
//! That split is the rule upstream's `aafd644f` had to patch in after the
//! fact: switching away and back restores the session's entries, and must not
//! pop a dock the reader hid back open. Only an explicit click on a
//! delegation card's open control shows the dock, and the caller does that.

/// One delegated child, addressed the way the transcript addresses its
/// delegation card: the message, the tool's flat index within that message's
/// steps, and the child within the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SubagentTab {
    pub message_ix: usize,
    pub tool_ix: usize,
    pub child_ix: usize,
}

/// A session's Subagents dock entries, in opening order, and the one showing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DockTabs {
    subagents: Vec<SubagentTab>,
    /// The entry whose response the dock shows; `None` only while empty.
    active: Option<SubagentTab>,
}

impl DockTabs {
    /// The entries, in the order they were opened.
    pub(crate) fn subagents(&self) -> &[SubagentTab] {
        &self.subagents
    }

    /// No entries: the session has nothing for the Subagents dock to show.
    pub(crate) fn is_empty(&self) -> bool {
        self.subagents.is_empty()
    }

    /// The entry the dock shows.
    pub(crate) fn active(&self) -> Option<SubagentTab> {
        self.active
    }

    /// Open `tab` (or bring an already open one forward) and make it active.
    pub(crate) fn open(&mut self, tab: SubagentTab) {
        if !self.subagents.contains(&tab) {
            self.subagents.push(tab);
        }
        self.active = Some(tab);
    }

    /// Show `tab`. An entry that is not open is ignored, so a stale click
    /// cannot conjure one.
    pub(crate) fn activate(&mut self, tab: SubagentTab) {
        if self.subagents.contains(&tab) {
            self.active = Some(tab);
        }
    }

    /// Close `tab`. Closing the active one moves to the entry above it, then
    /// the one below, then nothing — the way editor tabs fall back. Returns
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

    /// Drop the entries `keep` rejects (a reload whose rows no longer hold
    /// that delegation). An active entry that goes falls back to the first
    /// one left.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&SubagentTab) -> bool) {
        self.subagents.retain(|tab| keep(tab));
        if self
            .active
            .is_none_or(|active| !self.subagents.contains(&active))
        {
            self.active = self.subagents.first().copied();
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
    fn an_empty_dock_shows_nothing_and_review_is_not_an_entry() {
        let mut dock = DockTabs::default();
        assert!(dock.is_empty());
        assert_eq!(dock.active(), None);
        dock.open(tab(0));
        assert_eq!(dock.subagents(), &[tab(0)], "only the subagent, no Review entry");
        assert_eq!(dock.active(), Some(tab(0)));
        assert!(dock.close(tab(0)));
        assert!(dock.is_empty());
        assert_eq!(dock.active(), None);
    }

    #[test]
    fn opening_again_brings_the_entry_forward_without_duplicating_it() {
        let mut dock = DockTabs::default();
        dock.open(tab(0));
        dock.open(tab(1));
        dock.open(tab(0));
        assert_eq!(dock.subagents(), &[tab(0), tab(1)]);
        assert_eq!(dock.active(), Some(tab(0)));
    }

    #[test]
    fn closing_the_active_entry_falls_back_up_then_down_then_empty() {
        let mut dock = DockTabs::default();
        for child in 0..3 {
            dock.open(tab(child));
        }
        dock.activate(tab(1));
        dock.close(tab(1));
        assert_eq!(dock.active(), Some(tab(0)), "the entry above");
        dock.close(tab(0));
        assert_eq!(dock.active(), Some(tab(2)), "then the one below");
        dock.close(tab(2));
        assert_eq!(dock.active(), None);
    }

    #[test]
    fn closing_a_background_entry_keeps_the_active_one() {
        let mut dock = DockTabs::default();
        dock.open(tab(0));
        dock.open(tab(1));
        dock.close(tab(0));
        assert_eq!(dock.active(), Some(tab(1)));
        assert!(!dock.close(tab(0)), "closing twice is a no-op");
    }

    #[test]
    fn a_stale_activation_cannot_conjure_an_entry() {
        let mut dock = DockTabs::default();
        dock.activate(tab(4));
        assert!(dock.is_empty());
        assert_eq!(dock.active(), None);
    }

    #[test]
    fn a_reload_drops_entries_whose_delegation_is_gone() {
        let mut dock = DockTabs::default();
        dock.open(tab(0));
        dock.open(tab(1));
        dock.retain(|open| open.child_ix == 0);
        assert_eq!(dock.subagents(), &[tab(0)]);
        assert_eq!(dock.active(), Some(tab(0)), "the active entry went; the first left shows");
        dock.retain(|_| false);
        assert_eq!(dock.active(), None);
    }
}
