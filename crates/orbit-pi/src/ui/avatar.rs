//! Avatars: a person's round mark — their photo when there is one, a tinted
//! monogram otherwise.
//!
//! The avatar owns the disc, the photo's crop, the monogram's tint and
//! initials, and the rule that a slot is never blank: the monogram stands in
//! while the photo loads and when it fails. Where a photo URL comes from (a
//! GitHub login, an event payload) is the caller's domain knowledge.

use gpui::{
    div, hsla, img, px, AnyElement, FontWeight, IntoElement, ObjectFit, ParentElement,
    SharedString, Styled, StyledImage,
};

use crate::theme::{Theme, ThemeMode};

/// A round avatar `diameter` px across.
///
/// ```ignore
/// Avatar::new(name, email).photo(url).diameter(14.).render(&theme)
/// ```
pub(crate) struct Avatar {
    name: SharedString,
    seed: SharedString,
    photo: Option<String>,
    diameter: f32,
}

impl Avatar {
    /// `name` gives the initials; `seed` (an email, or empty) keeps the
    /// monogram's tint stable for one person across sessions. An empty seed
    /// falls back to the name.
    pub(crate) fn new(name: impl Into<SharedString>, seed: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            seed: seed.into(),
            photo: None,
            diameter: 14.,
        }
    }

    /// The photo to show over the monogram, when the caller knows one.
    pub(crate) fn photo(mut self, url: Option<String>) -> Self {
        self.photo = url;
        self
    }

    pub(crate) fn diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter;
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> AnyElement {
        let Some(url) = self.photo else {
            return monogram(&self.name, &self.seed, self.diameter, theme);
        };
        let theme = *theme;
        let (name, seed, diameter) = (self.name, self.seed, self.diameter);
        img(url)
            .size(px(diameter))
            .rounded_full()
            .object_fit(ObjectFit::Cover)
            .with_loading({
                let (name, seed) = (name.clone(), seed.clone());
                move || monogram(&name, &seed, diameter, &theme)
            })
            .with_fallback(move || monogram(&name, &seed, diameter, &theme))
            .into_any_element()
    }
}

/// The initials disc. Saturation stays low so identity doesn't read as
/// confetti; the ink is near-white on a mid-dark tint in both modes.
fn monogram(name: &str, seed: &str, diameter: f32, theme: &Theme) -> AnyElement {
    let (saturation, lightness) = match theme.mode {
        ThemeMode::Dark => (0.40, 0.34),
        ThemeMode::Light => (0.46, 0.34),
    };
    div()
        .flex_none()
        .size(px(diameter))
        .rounded_full()
        .bg(hsla(hue(seed, name), saturation, lightness, 1.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(theme.ui_px(diameter / 2.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(hsla(0., 0., 0.985, 1.))
        .child(initials(name))
        .into_any_element()
}

/// Up to two initials from a name: first and last word, or the first two
/// letters of a single word (`octocat` → `OC`), `?` for an empty name.
pub(crate) fn initials(name: &str) -> String {
    let pick = |word: &str| word.chars().next().map(|c| c.to_uppercase().to_string());
    let mut words = name.split_whitespace();
    let first = words.next().unwrap_or("");
    let last = words.last();
    match (pick(first), last.and_then(pick)) {
        (Some(a), Some(b)) => format!("{a}{b}"),
        (Some(a), None) => {
            let mut chars = first.chars();
            chars.next();
            match chars.next() {
                Some(b) => format!("{a}{}", b.to_uppercase()),
                None => a,
            }
        }
        _ => "?".to_string(),
    }
}

/// A stable hue in `0..1` from the seed (falling back to the name). FNV-1a
/// keeps it cheap and stable across runs; eight fixed hues keep neighbours
/// distinct.
fn hue(seed: &str, name: &str) -> f32 {
    const HUES: [f32; 8] = [14., 44., 96., 158., 202., 236., 274., 330.];
    let seed = if seed.trim().is_empty() { name } else { seed };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.trim().to_ascii_lowercase().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    HUES[(hash % HUES.len() as u64) as usize] / 360.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_first_and_last_words_or_two_letters() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("Grace Brewster Hopper"), "GH");
        assert_eq!(initials("octocat"), "OC");
        assert_eq!(initials("x"), "X");
        assert_eq!(initials("   "), "?");
    }

    #[test]
    fn the_tint_is_stable_and_keyed_by_seed_before_name() {
        assert_eq!(hue("a@b.c", "Ada"), hue("A@B.C ", "Someone else"));
        assert_eq!(hue("", "Ada"), hue("  ", "ada"));
        assert!((0.0..1.0).contains(&hue("a@b.c", "Ada")));
    }
}
