//! File paths written in transcript text (issue #199): what counts as one, how
//! far it reaches under the pointer, and where it points on this machine.
//!
//! Pure string work — no filesystem access. Whether the path exists is asked
//! off the UI thread when an action runs, never while a menu opens.

use std::path::{Path, PathBuf};

/// Characters that end a path in running text. Whitespace always does; so do
/// quotes, backticks, brackets and the CJK punctuation an assistant writes
/// right after a path with no space (`foo.zip（242 MB）`). ASCII `:` is kept:
/// it is part of `E:/…` and of a `file.rs:12` location.
fn is_boundary(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\''
                | '`'
                | '<'
                | '>'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '|'
                | '（'
                | '）'
                | '「'
                | '」'
                | '『'
                | '』'
                | '【'
                | '】'
                | '《'
                | '》'
                | '，'
                | '。'
                | '；'
                | '：'
                | '、'
                | '！'
                | '？'
                | '“'
                | '”'
                | '‘'
                | '’'
        )
}

/// The run of path characters around byte `offset` in `text`, or `None` when
/// the offset sits on a boundary with no path character just before it. (A
/// pointer past a word's last glyph lands on the space after it, so the
/// character before the offset counts too.)
pub fn token_at(text: &str, offset: usize) -> Option<&str> {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let on_path = |c: Option<char>| c.is_some_and(|c| !is_boundary(c));
    if !on_path(text[offset..].chars().next()) {
        let before = text[..offset].chars().next_back();
        if !on_path(before) {
            return None;
        }
        offset -= before.map_or(0, char::len_utf8);
    }
    let start = text[..offset]
        .char_indices()
        .rev()
        .find(|(_, c)| is_boundary(*c))
        .map(|(ix, c)| ix + c.len_utf8())
        .unwrap_or(0);
    let end = text[offset..]
        .char_indices()
        .find(|(_, c)| is_boundary(*c))
        .map(|(ix, _)| offset + ix)
        .unwrap_or(text.len());
    (start < end).then(|| &text[start..end])
}

/// `text` read as a file path, cleaned of what surrounds it in prose: wrapping
/// quotes, trailing sentence punctuation, a `file://` scheme and a trailing
/// `:line[:col]` location. `None` for anything that does not look like a path
/// — a URL, a version number, a bare word, an empty string.
///
/// A path is absolute (`/…`, `~/…`, `C:\…`, `C:/…`, `\\server\…`), explicitly
/// relative (`./…`, `../…`), or contains a separator or ends in a file
/// extension with a letter in it (`src/lib.rs`, `MANIFEST.csv`; not `v1.2.9`).
pub fn candidate(text: &str) -> Option<String> {
    let mut text = text.trim().trim_matches(|c| matches!(c, '"' | '\'' | '`'));
    text = text.trim_end_matches(['.', ',', ';', '!', '?', ':']);
    if let Some(rest) = text.strip_prefix("file://") {
        text = rest;
    } else if text.contains("://") || text.starts_with("mailto:") {
        return None;
    }
    let text = strip_location(text);
    if text.is_empty() || text.chars().any(|c| c.is_control()) {
        return None;
    }
    let absolute = text.starts_with('/')
        || text.starts_with("~/")
        || text.starts_with("\\\\")
        || is_drive_path(text);
    let relative = text.starts_with("./") || text.starts_with("../");
    let has_separator = text.contains('/') || text.contains('\\');
    if absolute || relative || has_separator || has_file_extension(text) {
        Some(text.to_string())
    } else {
        None
    }
}

/// `C:\…` or `C:/…`.
fn is_drive_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// Drop a trailing `:12` or `:12:3` an assistant appends to point at a line.
/// A drive letter's colon is never at the end of digits, so it survives.
fn strip_location(text: &str) -> &str {
    let mut text = text;
    for _ in 0..2 {
        match text.rsplit_once(':') {
            Some((head, tail))
                if !head.is_empty()
                    && !tail.is_empty()
                    && tail.bytes().all(|b| b.is_ascii_digit()) =>
            {
                text = head;
            }
            _ => break,
        }
    }
    text
}

/// A last segment shaped like `name.ext`: a non-empty name and a 1–8
/// character alphanumeric extension containing at least one letter.
fn has_file_extension(text: &str) -> bool {
    let name = text.rsplit(['/', '\\']).next().unwrap_or(text);
    match name.rsplit_once('.') {
        Some(("", _)) => false,
        // `e.g`, `i.e`: a one-letter stem and extension is prose.
        Some((stem, ext)) if stem.chars().count() == 1 && ext.len() == 1 => false,
        Some((_, ext)) => {
            (1..=8).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && ext.chars().any(|c| c.is_ascii_alphabetic())
        }
        None => false,
    }
}

/// Where a [`candidate`] points on this machine: `~` expands to `home`, and a
/// relative path is joined to the session's folder `cwd`.
pub fn resolve(candidate: &str, cwd: &Path, home: &Path) -> PathBuf {
    if let Some(rest) = candidate.strip_prefix("~/") {
        return home.join(rest);
    }
    let path = Path::new(candidate);
    if path.is_absolute() || is_drive_path(candidate) || candidate.starts_with("\\\\") {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

/// The path under the pointer: the token around `offset` in `text`, read as a
/// [`candidate`].
pub fn at(text: &str, offset: usize) -> Option<String> {
    token_at(text, offset).and_then(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_recognized_and_prose_is_not() {
        for path in [
            "/Users/me/out.zip",
            "~/notes.md",
            "C:\\Agent\\out.zip",
            "E:/Agent/yunting/handoff-v1.2.9.zip",
            "\\\\server\\share\\a.txt",
            "./build.sh",
            "../README",
            "src/lib.rs",
            "MANIFEST.csv",
        ] {
            assert_eq!(candidate(path).as_deref(), Some(path), "{path}");
        }
        for prose in [
            "hello",
            "v1.2.9",
            "1.5",
            "e.g.",
            "https://example.com/a.zip",
            "mailto:a@b.c",
            "",
            "...",
        ] {
            assert_eq!(candidate(prose), None, "{prose}");
        }
    }

    #[test]
    fn surrounding_prose_is_trimmed() {
        assert_eq!(candidate("`src/lib.rs`").as_deref(), Some("src/lib.rs"));
        assert_eq!(candidate("src/lib.rs.").as_deref(), Some("src/lib.rs"));
        assert_eq!(candidate("src/lib.rs:42").as_deref(), Some("src/lib.rs"));
        assert_eq!(candidate("src/lib.rs:42:7").as_deref(), Some("src/lib.rs"));
        assert_eq!(candidate("file:///tmp/a.txt").as_deref(), Some("/tmp/a.txt"));
        assert_eq!(candidate("C:/a.txt").as_deref(), Some("C:/a.txt"), "the drive survives");
    }

    /// The reporter's line: a Windows path followed straight away by a
    /// full-width parenthesis, with no space between them.
    #[test]
    fn the_token_stops_at_cjk_punctuation() {
        let line = "E:/Agent/yunting/yunting-music-handoff-v1.2.9-20261128.zip（242 MB，944 项）";
        let offset = line.find("handoff").unwrap();
        assert_eq!(
            at(line, offset).as_deref(),
            Some("E:/Agent/yunting/yunting-music-handoff-v1.2.9-20261128.zip")
        );
        assert_eq!(at(line, line.find("242").unwrap()), None, "the size is not a path");
    }

    #[test]
    fn the_token_is_the_word_under_the_pointer() {
        let line = "Wrote out/report.pdf and logs/run.log.";
        assert_eq!(at(line, line.find("report").unwrap()).as_deref(), Some("out/report.pdf"));
        assert_eq!(at(line, line.find("run").unwrap()).as_deref(), Some("logs/run.log"));
        assert_eq!(at(line, line.find("and").unwrap()), None);
        assert_eq!(token_at("a  b", 2), None, "between two spaces");
        assert_eq!(
            at(line, line.find(" and").unwrap()).as_deref(),
            Some("out/report.pdf"),
            "just past the path's last glyph"
        );
        // A multi-byte offset in the middle of a character does not panic.
        assert_eq!(at("é/a.txt", 1).as_deref(), Some("é/a.txt"));
    }

    #[test]
    fn relative_paths_resolve_against_the_session_folder() {
        let cwd = Path::new("/work/repo");
        let home = Path::new("/home/me");
        assert_eq!(resolve("src/a.rs", cwd, home), PathBuf::from("/work/repo/src/a.rs"));
        assert_eq!(resolve("~/a.md", cwd, home), PathBuf::from("/home/me/a.md"));
        assert_eq!(resolve("/etc/hosts", cwd, home), PathBuf::from("/etc/hosts"));
        assert_eq!(resolve("E:/a.zip", cwd, home), PathBuf::from("E:/a.zip"));
    }
}
