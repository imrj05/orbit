//! Per-workspace sidebar marks: the curated HugeIcons and the semantic tints a
//! project header can wear. Orbit-owned, persisted inside
//! `~/.orbit-pi/workspaces.json` beside the project list, and resolved against
//! the active [`Theme`] at render time so a mark never fights a palette.

use gpui::{Hsla, SharedString};

use crate::theme::Theme;

/// One workspace's stored mark. Both halves are optional; the defaults (the
/// folder glyph, muted ink) apply when a field is unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceMark {
    /// Embedded HugeIcons file stem, e.g. `rocket-01`.
    pub icon: Option<String>,
    /// Stable tint key — see [`TINTS`].
    pub tint: Option<String>,
}

impl WorkspaceMark {
    /// Whether the mark is the unstyled default. Such an entry is dropped
    /// instead of persisted, so an untouched workspace writes no extra keys.
    pub fn is_default(&self) -> bool {
        self.icon.is_none() && self.tint.is_none()
    }
}

/// A curated subset of the embedded HugeIcons set that reads as a project
/// mark. Kept to four rows of six in the picker.
pub(crate) const PROJECT_ICONS: &[&str] = &[
    "folder",
    "branch",
    "rocket-01",
    "star",
    "spark",
    "cloud",
    "terminal",
    "monitor",
    "server-stack",
    "git-fork",
    "github",
    "image",
    "lock",
    "clock",
    "tag-01",
    "task",
    "compose",
    "magic-wand",
    "start-up",
    "extensions",
    "chat",
    "archive",
    "contrast",
    "side-by-side",
];

/// One row of the picker's icon grid.
pub(crate) const ICON_COLUMNS: usize = 6;

/// A tint resolved against the active theme.
type TintFn = fn(&Theme) -> Hsla;

/// The semantic tints, in picker order. The key is stable on disk; the color
/// follows the active palette. `syn_*` tokens are reused as a dependable
/// spread of hues that every palette tunes to stay legible.
pub(crate) const TINTS: &[(&str, TintFn)] = &[
    ("default", |theme| theme.text_3),
    ("accent", |theme| theme.accent),
    ("green", |theme| theme.ok_green),
    ("red", |theme| theme.stop_red),
    ("yellow", |theme| theme.warn),
    ("blue", |theme| theme.syn_function),
    ("purple", |theme| theme.syn_type),
    ("pink", |theme| theme.syn_string),
];

/// The ink for a persisted tint key, falling back to the muted default.
pub(crate) fn tint_color(theme: &Theme, key: Option<&str>) -> Hsla {
    key.and_then(|key| {
        TINTS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, color)| color(theme))
    })
    .unwrap_or(theme.text_3)
}

/// The asset path for a persisted icon stem, falling back to the folder
/// glyph. Stems are validated against [`PROJECT_ICONS`] so a hand-edited store
/// can never point the renderer at an arbitrary asset.
pub(crate) fn icon_path(icon: Option<&str>) -> SharedString {
    match icon.filter(|stem| PROJECT_ICONS.contains(stem)) {
        Some(stem) => format!("icons/{stem}.svg").into(),
        None => "icons/folder.svg".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_stems_and_keys_fall_back_to_the_defaults() {
        assert_eq!(icon_path(None), SharedString::from("icons/folder.svg"));
        assert_eq!(
            icon_path(Some("../../etc/passwd")),
            SharedString::from("icons/folder.svg")
        );
        assert_eq!(
            icon_path(Some("rocket-01")),
            SharedString::from("icons/rocket-01.svg")
        );

        let theme = crate::theme::Theme::default();
        assert_eq!(tint_color(&theme, Some("nonsense")), theme.text_3);
        assert_eq!(tint_color(&theme, None), theme.text_3);
        assert_eq!(tint_color(&theme, Some("accent")), theme.accent);
    }

    #[test]
    fn every_curated_icon_exists_in_the_embedded_set() {
        for stem in PROJECT_ICONS {
            let path = format!("icons/{stem}.svg");
            let bytes =
                <crate::assets::Assets as gpui::AssetSource>::load(&crate::assets::Assets, &path)
                    .expect("asset source reads");
            assert!(bytes.is_some(), "missing icon asset: {path}");
        }
    }

    #[test]
    fn default_mark_is_recognised() {
        assert!(WorkspaceMark::default().is_default());
        assert!(!WorkspaceMark {
            icon: Some("star".into()),
            tint: None,
        }
        .is_default());
        assert!(!WorkspaceMark {
            icon: None,
            tint: Some("accent".into()),
        }
        .is_default());
    }
}
