//! Conservative detection of CSS colour literals in assistant prose.
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) struct ColorMatch {
    pub range: Range<usize>,
    pub rgba: [u8; 4],
}

/// Return byte ranges into the original text. Links are filtered by the caller.
/// A trailing token in a live response may still be incomplete, so wait for a
/// delimiter before recognizing it. Named colours are intentionally omitted:
/// ordinary prose makes their interpretation ambiguous.
pub(super) fn find(text: &str, live: bool) -> Vec<ColorMatch> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        let preceding = text[..i].chars().next_back();
        let boundary = preceding
            .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '/' | '#' | '.' | '-')));
        if boundary {
            let rest = &text[i..];
            let end = if ch == '#' {
                let n = rest[1..].bytes().take_while(u8::is_ascii_hexdigit).count();
                if matches!(n, 3 | 4 | 6 | 8) {
                    Some(i + 1 + n)
                } else {
                    None
                }
            } else {
                ["rgba(", "rgb(", "hsla(", "hsl(", "oklch(", "oklab("]
                    .iter()
                    .find(|prefix| rest.starts_with(**prefix))
                    .and_then(|_| rest.find(')').map(|n| i + n + 1))
            };
            if let Some(end) = end {
                let next = text[end..].chars().next();
                // Reject pieces of paths, hashes, identifiers, or URLs.
                let valid_end = next
                    .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '/' | '#' | '-')));
                let token_start = text[..i]
                    .char_indices()
                    .rev()
                    .find(|(_, c)| c.is_whitespace())
                    .map_or(0, |(n, c)| n + c.len_utf8());
                let path_prefix =
                    text[token_start..i].contains('/') || text[token_start..i].contains('\\');
                if valid_end && !path_prefix && !(live && end == text.len()) {
                    if let Ok(color) = text[i..end].parse::<csscolorparser::Color>() {
                        out.push(ColorMatch {
                            range: i..end,
                            rgba: color.to_rgba8(),
                        });
                        i = end;
                        continue;
                    }
                }
            }
        }
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_colour_values_and_correct_channels() {
        let s = "🎨 #F4711D, rgb(244 113 29), oklch(0.69 0.19 45) and #f718";
        let found = find(s, false);
        assert_eq!(found.len(), 4);
        assert_eq!(&s[found[0].range.clone()], "#F4711D");
        assert_eq!(found[0].rgba, [244, 113, 29, 255]);
        assert_eq!(found[1].rgba, [244, 113, 29, 255]);
        assert_eq!(found[3].rgba, [255, 119, 17, 136]);
    }

    #[test]
    fn rejects_ambiguous_invalid_and_partial_values() {
        for s in [
            "a12b34",
            "#define",
            "foo/#abcdef",
            "https://a/#abcdef",
            "#abcdef123",
            "rgb(bad)",
        ] {
            assert!(find(s, false).is_empty(), "{s}");
        }
        assert!(find("#F47", true).is_empty());
        assert_eq!(find("#F4711D ", true).len(), 1);
        assert_eq!(find("#F4711D", false).len(), 1);
        assert_eq!(find("hello\u{2003}#fff", false).len(), 1);
    }
}
