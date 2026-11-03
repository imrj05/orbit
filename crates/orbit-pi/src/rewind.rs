//! Conversation rewind — pi's `fork` at an earlier user message.
//!
//! pi's `fork` does not truncate the session being read. It writes a **new**
//! session file holding the branch up to the entry *before* the chosen user
//! message and moves the process onto it; the session being left stays on disk
//! and in the sidebar. That is why the UI asks no confirmation before a rewind:
//! nothing is destroyed, and the old conversation is one sidebar row away.
//!
//! `get_fork_messages` names entry ids but the transcript snapshot does not.
//! [`point_for_turn`] preselects only when the prompt text identifies exactly
//! one entry; repeated text may include abandoned branches and requires an
//! explicit choice.

use serde_json::Value;

/// One user message pi will fork before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkPoint {
    /// pi's session entry id — the only handle `fork` accepts.
    pub entry_id: String,
    /// The message as pi stored it.
    pub text: String,
}

/// Read a `get_fork_messages` response payload.
///
/// Rows without an entry id are dropped: an unnamed row could be drawn but
/// never acted on, and pi has no id-less entry to fork from.
pub fn fork_points(data: &Value) -> Vec<ForkPoint> {
    data.get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| {
                    let entry_id = message.get("entryId").and_then(Value::as_str)?.trim();
                    if entry_id.is_empty() {
                        return None;
                    }
                    Some(ForkPoint {
                        entry_id: entry_id.to_string(),
                        text: message
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The comparison key for a message: every whitespace run becomes one space.
///
/// pi stores the prompt as sent, while the transcript renders the steps it was
/// split into, so indentation and a trailing newline can differ between two
/// copies of the same message without the message differing.
fn key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A uniquely identifiable fork point for the `turn`-th user message.
/// Duplicate text cannot identify a session entry reliably.
pub fn point_for_turn<'a>(
    points: &'a [ForkPoint],
    texts: &[String],
    turn: usize,
) -> Option<&'a ForkPoint> {
    let wanted = key(texts.get(turn)?);
    if wanted.is_empty() {
        return None;
    }
    // `get_fork_messages` can include abandoned branches, so matching text
    // cannot distinguish identical prompts even when their ordering seems to
    // agree. Require an unambiguous match; otherwise the user chooses a row.
    let mut matches = points.iter().filter(|point| key(&point.text) == wanted);
    let point = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(point)
}

/// A one-line preview of a prompt: its first line with content, whitespace
/// collapsed. Clipping is the row's job, so a long prompt keeps its own words
/// instead of a truncated copy made here.
pub fn preview(text: &str) -> String {
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    key(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn point(entry_id: &str, text: &str) -> ForkPoint {
        ForkPoint {
            entry_id: entry_id.into(),
            text: text.into(),
        }
    }

    fn texts(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn a_fork_message_list_reads_ids_and_text() {
        let data = json!({
            "messages": [
                {"entryId": "abc123", "text": "First prompt..."},
                {"entryId": "def456", "text": "Second prompt..."},
            ]
        });
        assert_eq!(
            fork_points(&data),
            vec![
                point("abc123", "First prompt..."),
                point("def456", "Second prompt...")
            ]
        );
    }

    #[test]
    fn a_row_without_an_entry_id_is_not_a_fork_point() {
        let data = json!({
            "messages": [
                {"text": "no id"},
                {"entryId": "  ", "text": "blank id"},
                {"entryId": "abc123", "text": "kept"},
            ]
        });
        assert_eq!(fork_points(&data), vec![point("abc123", "kept")]);
    }

    #[test]
    fn a_payload_without_messages_reads_as_an_empty_list() {
        assert!(fork_points(&json!({})).is_empty());
        assert!(fork_points(&json!({"messages": null})).is_empty());
    }

    #[test]
    fn turns_pair_with_points_from_the_newest_backwards() {
        let points = vec![point("a", "one"), point("b", "two"), point("c", "three")];
        let turns = texts(&["one", "two", "three"]);
        assert_eq!(point_for_turn(&points, &turns, 0), Some(&points[0]));
        assert_eq!(point_for_turn(&points, &turns, 2), Some(&points[2]));
        assert_eq!(
            point_for_turn(&points, &turns, 1).map(|p| p.entry_id.as_str()),
            Some("b")
        );
    }

    /// Compaction removes early turns from the transcript while the entry list
    /// keeps them: the visible tail must still pair with its own entries.
    #[test]
    fn compacted_history_does_not_shift_the_tail() {
        let points = vec![
            point("a", "dropped prompt"),
            point("b", "summary turn"),
            point("c", "kept prompt"),
            point("d", "latest prompt"),
        ];
        let turns = texts(&["kept prompt", "latest prompt"]);
        assert_eq!(
            point_for_turn(&points, &turns, 0).map(|p| p.entry_id.as_str()),
            Some("c")
        );
        assert_eq!(
            point_for_turn(&points, &turns, 1).map(|p| p.entry_id.as_str()),
            Some("d")
        );
    }

    /// "continue" three times: each transcript row is the *next* occurrence
    /// backwards, never the first.
    #[test]
    fn repeated_prompts_are_not_automatically_preselected() {
        let points = vec![
            point("a", "continue"),
            point("b", "continue"),
            point("c", "continue"),
        ];
        let turns = texts(&["continue", "continue"]);
        assert_eq!(point_for_turn(&points, &turns, 0), None);
        assert_eq!(point_for_turn(&points, &turns, 1), None);
    }

    #[test]
    fn a_turn_with_no_matching_entry_does_not_hide_the_ones_behind_it() {
        let points = vec![point("a", "one")];
        let turns = texts(&["one", "typed while pi restarted"]);
        // The unpersisted prompt has no entry of its own…
        assert_eq!(point_for_turn(&points, &turns, 1), None);
        // …and must not take the persisted prompt's entry away from it.
        assert_eq!(
            point_for_turn(&points, &turns, 0).map(|p| p.entry_id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn whitespace_differences_still_match() {
        let points = vec![point("a", "fix   the failing\ntest\n")];
        let turns = texts(&["fix the failing test"]);
        assert_eq!(
            point_for_turn(&points, &turns, 0).map(|p| p.entry_id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn an_empty_turn_is_never_matched() {
        let points = vec![point("a", "")];
        let turns = texts(&[""]);
        assert_eq!(point_for_turn(&points, &turns, 0), None);
    }

    #[test]
    fn preview_reads_the_first_line_with_content() {
        assert_eq!(
            preview("\n\n  Fix the   failing test\nand re-run"),
            "Fix the failing test"
        );
        assert_eq!(preview("   "), "");
        assert_eq!(preview("one line"), "one line");
    }
}
