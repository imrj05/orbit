//! Formatting and small domain helpers the app's surfaces share: command
//! labels, queue row identity and previews, session titles, uptime. No UI —
//! anything that builds an element lives in `crate::ui`.

use super::*;

/// Turn a wire command name into a short human label, e.g. `set_model` →
/// `Set model failed`.
pub(crate) fn humanize_command(command: &str) -> String {
    let spaced = command.replace(['_', '.'], " ");
    let mut chars = spaced.chars();
    let label = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => tr!("helpers.command"),
    };
    tr!("helpers.command_failed", label = label)
}

/// A queued message's position in the whole strip, steering rows first — the
/// one number element ids key on, so a row keeps its identity when the lanes
/// are rendered in one pass.
pub(crate) fn queue_row_slot(queue: &PendingQueue, target: QueuedMessage) -> u64 {
    match target.lane {
        QueueLane::Steer => target.index as u64,
        QueueLane::FollowUp => (queue.steering.len() + target.index) as u64,
    }
}

/// The one-line preview a queue row shows for a pending message: its text
/// with every run of whitespace — newlines included — folded to one space.
///
/// `line_clamp` in gpui 0.2.2 bounds wrapped lines but still emits each
/// explicit newline, so a queued skill file (hundreds of short lines) would
/// grow the strip over the whole session page (issue #110). Folded to one
/// logical line, the row's two-line clamp holds. The queued message itself is
/// untouched: pi receives the text as written, and the row's editor opens the
/// original.
pub(crate) fn queue_preview(text: &str) -> String {
    let mut preview = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !preview.is_empty() {
            preview.push(' ');
        }
        preview.push_str(word);
    }
    preview
}

/// The title shown in the top bar and session lists. An explicit
/// `set_session_name` value wins over pi's live auto-title; neither
/// present is a new, unnamed task.
/// The folder half of the top bar's `folder / session` title: the remote
/// entry's folder on a remote session (the local workspace is not where it
/// runs), the current workspace otherwise. `None` when neither is known.
pub(crate) fn top_bar_folder_label(
    remote_dir: Option<&str>,
    workspace: Option<&std::path::Path>,
) -> Option<String> {
    if let Some(dir) = remote_dir {
        let dir = dir.trim();
        if dir.is_empty() {
            return None;
        }
        let name = dir.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        return Some(if name.is_empty() { "/".into() } else { name.to_owned() });
    }
    workspace.map(crate::sessions::workspace_label)
}

pub(crate) fn session_display_title(
    session_name: Option<&str>,
    current_title: Option<&str>,
) -> String {
    session_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            current_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| tr!("settings.new_task"))
}

/// "42s" / "3m 12s" / "2h 5m" / "4d 3h" for the Runtime uptime readout.
pub(crate) fn format_uptime(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m {}s", s / 60, s % 60),
        s if s < 86_400 => format!("{}h {}m", s / 3_600, (s % 3_600) / 60),
        s => format!("{}d {}h", s / 86_400, (s % 86_400) / 3_600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A queued skill file arrives with hundreds of newlines, and gpui's
    /// `line_clamp` still emits every one of them. The row preview folds them
    /// into one logical line so its two-line clamp holds (issue #110).
    #[test]
    fn a_queue_preview_folds_line_breaks_into_spaces() {
        assert_eq!(
            queue_preview("<skill name=\"design\">\n\n## Heading\nline one\n  line two  \n"),
            "<skill name=\"design\"> ## Heading line one line two"
        );
        assert_eq!(queue_preview(""), "");
        assert_eq!(queue_preview("one\ntwo"), "one two");
    }
}
