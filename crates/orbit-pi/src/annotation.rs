//! Transcript passages quoted into the composer (issue #187).
//!
//! A quote travels inside the prompt text, because the prompt is all pi keeps:
//! a session reloaded from disk has nothing else to read a quote back from.
//! [`fold`] writes the shape and [`split`] reads it back, so a sent message
//! shows the same quote chips live and after a reload. The shape is plain text
//! a model reads as a quotation:
//!
//! ```text
//! > the quoted line
//! > another quoted line
//! Comment: the optional one-line comment
//!
//! the typed message
//! ```
//!
//! Every quote block comes first, oldest first, separated by a blank line;
//! the typed message follows. `Comment:` is a parse marker, not UI copy, so it
//! is never translated.

/// Quotes past this many characters are cut and end with `…`.
pub(crate) const MAX_QUOTE_CHARS: usize = 4_000;
/// Quotes one composer holds at once; another is refused with a toast.
pub(crate) const MAX_ANNOTATIONS: usize = 8;

const COMMENT_PREFIX: &str = "Comment: ";

/// Which side of the conversation a quote was taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuoteSource {
    User,
    Assistant,
}

/// A selection made ready to quote: trimmed, blank-line runs collapsed, and
/// capped at [`MAX_QUOTE_CHARS`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Quote {
    pub text: String,
    pub truncated: bool,
}

/// One quote read back from a sent message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SentQuote {
    pub quote: String,
    pub comment: Option<String>,
}

/// Normalize a transcript selection into a quote; `None` when nothing but
/// whitespace was selected.
pub(crate) fn normalize_quote(raw: &str) -> Option<Quote> {
    let mut lines: Vec<&str> = Vec::new();
    for line in raw.lines().map(str::trim_end) {
        // One blank line keeps a paragraph break; a run of them says nothing more.
        if line.trim().is_empty() && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(if line.trim().is_empty() { "" } else { line });
    }
    while lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    // Leading indentation of the first line is selection noise, not content.
    let text = lines.join("\n");
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    match text.char_indices().nth(MAX_QUOTE_CHARS) {
        Some((cut, _)) => Some(Quote {
            text: format!("{}…", &text[..cut]),
            truncated: true,
        }),
        None => Some(Quote {
            text: text.to_string(),
            truncated: false,
        }),
    }
}

/// Lead `text` with one block per `(quote, comment)` pair, oldest first.
pub(crate) fn fold(text: &str, quotes: &[(&str, &str)]) -> String {
    let mut blocks: Vec<String> = quotes
        .iter()
        .map(|(quote, comment)| {
            let mut block = quote
                .lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let comment = one_line(comment);
            if !comment.is_empty() {
                block.push('\n');
                block.push_str(COMMENT_PREFIX);
                block.push_str(&comment);
            }
            block
        })
        .filter(|block| !block.is_empty())
        .collect();
    if !text.is_empty() {
        blocks.push(text.to_string());
    }
    blocks.join("\n\n")
}

/// Read the quote blocks [`fold`] wrote back off the front of a sent message.
/// The remainder is the message the person typed, exactly as sent.
pub(crate) fn split(text: &str) -> (Vec<SentQuote>, &str) {
    let mut quotes = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (block, next) = match rest.split_once("\n\n") {
            Some((block, next)) => (block, next),
            None => (rest, ""),
        };
        let Some(quote) = parse_block(block) else {
            break;
        };
        quotes.push(quote);
        rest = next;
    }
    (quotes, rest)
}

/// One quote block: `>` lines, then at most one `Comment:` line.
fn parse_block(block: &str) -> Option<SentQuote> {
    let mut quote: Vec<&str> = Vec::new();
    let mut comment = None;
    for line in block.lines() {
        if comment.is_some() {
            // Nothing follows the comment inside a block.
            return None;
        }
        if let Some(body) = line.strip_prefix('>') {
            quote.push(body.strip_prefix(' ').unwrap_or(body));
        } else if !quote.is_empty() {
            comment = Some(line.strip_prefix(COMMENT_PREFIX)?.to_string());
        } else {
            return None;
        }
    }
    (!quote.is_empty()).then(|| SentQuote {
        quote: quote.join("\n"),
        comment,
    })
}

/// A comment is one line: inner line breaks become spaces.
fn one_line(comment: &str) -> String {
    comment.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_is_trimmed_and_keeps_single_line_breaks() {
        let quote = normalize_quote("\n  first line  \nsecond line\n\n").unwrap();
        assert_eq!(quote.text, "first line\nsecond line");
        assert!(!quote.truncated);
    }

    #[test]
    fn blank_line_runs_inside_a_quote_collapse_to_one() {
        let quote = normalize_quote("one\n\n\n\ntwo").unwrap();
        assert_eq!(quote.text, "one\n\ntwo");
    }

    #[test]
    fn a_whitespace_selection_is_not_a_quote() {
        assert_eq!(normalize_quote(""), None);
        assert_eq!(normalize_quote(" \n\t\n "), None);
    }

    #[test]
    fn a_long_quote_is_cut_on_a_char_boundary_and_marked() {
        let raw = "é".repeat(MAX_QUOTE_CHARS + 10);
        let quote = normalize_quote(&raw).unwrap();
        assert!(quote.truncated);
        assert_eq!(quote.text.chars().count(), MAX_QUOTE_CHARS + 1);
        assert!(quote.text.ends_with('…'));
        // Exactly at the cap is not truncated.
        let raw = "x".repeat(MAX_QUOTE_CHARS);
        assert!(!normalize_quote(&raw).unwrap().truncated);
    }

    #[test]
    fn fold_leads_with_quotes_in_order_then_the_typed_text() {
        let folded = fold(
            "what did you mean?",
            &[("first passage", "too vague"), ("second\npassage", "")],
        );
        assert_eq!(
            folded,
            "> first passage\nComment: too vague\n\n> second\n> passage\n\nwhat did you mean?"
        );
    }

    #[test]
    fn fold_without_quotes_is_the_text_and_without_text_is_the_quotes() {
        assert_eq!(fold("plain", &[]), "plain");
        assert_eq!(fold("", &[("only a quote", "")]), "> only a quote");
    }

    #[test]
    fn a_blank_line_inside_a_quote_stays_inside_its_block() {
        let folded = fold("", &[("one\n\ntwo", "")]);
        assert_eq!(folded, "> one\n>\n> two");
        let (quotes, body) = split(&folded);
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].quote, "one\n\ntwo");
        assert_eq!(body, "");
    }

    #[test]
    fn a_comment_is_one_line() {
        let folded = fold("", &[("q", "  line one\nline two  ")]);
        assert_eq!(folded, "> q\nComment: line one line two");
    }

    #[test]
    fn split_reads_back_what_fold_wrote() {
        let folded = fold(
            "the typed message\n\nwith two paragraphs",
            &[("first passage", "too vague"), ("second\npassage", "")],
        );
        let (quotes, body) = split(&folded);
        assert_eq!(
            quotes,
            vec![
                SentQuote {
                    quote: "first passage".into(),
                    comment: Some("too vague".into()),
                },
                SentQuote {
                    quote: "second\npassage".into(),
                    comment: None,
                },
            ]
        );
        assert_eq!(body, "the typed message\n\nwith two paragraphs");
    }

    #[test]
    fn split_leaves_a_message_without_quotes_alone() {
        let text = "a message\n\n> quoted later is prose, not an annotation";
        assert_eq!(split(text), (Vec::new(), text));
        // A paragraph that only starts like a comment is prose.
        assert_eq!(split("Comment: hi").0, Vec::new());
    }

    #[test]
    fn split_keeps_a_file_block_after_the_quotes_for_the_file_split() {
        let text = "> passage\n\n@src/main.rs";
        let (quotes, body) = split(text);
        assert_eq!(quotes.len(), 1);
        assert_eq!(body, "@src/main.rs");
    }
}
