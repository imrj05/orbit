//! A delegated child's live transcript, read from the event log the
//! subagents extension writes (`contrib/orbit-subagents-extension/log.js`).
//!
//! The log holds the child pi's own `--mode json` lines verbatim — the same
//! event stream an RPC session prints — so each line goes through
//! [`Event::parse_line`] into a [`Transcript`] exactly as the main session's
//! events do. Thinking, tool calls, their output and the answer stream into
//! the side pane the way they stream into the chat.
//!
//! Reading is incremental: [`ChildLog`] keeps the byte offset it has consumed
//! and any trailing partial line, and the app reads only the appended bytes,
//! off the UI thread ([`read_appended`]), on the heartbeat.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use orbit_rpc::Event;

use crate::transcript::Transcript;

/// The most a single heartbeat read takes from a log. A replay of a long
/// finished child arrives over a few ticks instead of one long read.
pub(crate) const READ_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

/// The directory the extension writes child logs to.
pub(crate) fn log_dir(home: &Path) -> PathBuf {
    home.join(".orbit-pi").join("subagent-logs")
}

/// `raw` as a path Orbit may read, or `None`.
///
/// The path arrives in a tool result's `details`, which a session file can
/// carry from anywhere, so it is honoured only when it names a `.jsonl` file
/// directly inside Orbit's own log directory — never an arbitrary file.
pub(crate) fn trusted_log_path(raw: &str, home: &Path) -> Option<PathBuf> {
    let path = Path::new(raw);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    let plain = name
        .strip_suffix(".jsonl")
        .is_some_and(|stem| !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    (plain && path.parent()? == log_dir(home)).then(|| path.to_path_buf())
}

/// Up to [`READ_CHUNK_BYTES`] of `path` from byte `offset`. An empty result
/// means nothing new. Blocking: call it on the background executor.
pub(crate) fn read_appended(path: &Path, offset: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len <= offset {
        return Ok(Vec::new());
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(READ_CHUNK_BYTES).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Append `bytes` to `partial` and drain every complete (LF-terminated) line,
/// with a trailing `\r` stripped. A line still being written stays in
/// `partial` for the next read.
pub(crate) fn take_lines(partial: &mut Vec<u8>, bytes: &[u8]) -> Vec<String> {
    partial.extend_from_slice(bytes);
    let Some(last) = partial.iter().rposition(|byte| *byte == b'\n') else {
        return Vec::new();
    };
    let rest = partial.split_off(last + 1);
    let complete = std::mem::replace(partial, rest);
    complete
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty())
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect()
}

/// One child's transcript and how far its log has been read.
pub(crate) struct ChildLog {
    pub transcript: Transcript,
    offset: u64,
    partial: Vec<u8>,
    /// A read is in flight; the heartbeat starts no second one.
    pub reading: bool,
    /// The log could not be read (deleted, permissions): the pane falls back
    /// to the summary instead of an empty transcript.
    pub unreadable: bool,
    /// The child has settled and its log was read to the end: nothing more
    /// will arrive, so the heartbeat stops opening the file.
    pub drained: bool,
}

impl ChildLog {
    pub(crate) fn new() -> Self {
        Self {
            transcript: Transcript::new(),
            offset: 0,
            partial: Vec::new(),
            reading: false,
            unreadable: false,
            drained: false,
        }
    }

    /// Where the next read starts.
    pub(crate) fn offset(&self) -> u64 {
        self.offset
    }

    /// Feed bytes read from [`Self::offset`]. Returns whether the transcript
    /// changed (so the pane repaints).
    pub(crate) fn apply(&mut self, bytes: &[u8]) -> bool {
        self.reading = false;
        if bytes.is_empty() {
            return false;
        }
        self.offset += bytes.len() as u64;
        let mut changed = false;
        for line in take_lines(&mut self.partial, bytes) {
            changed |= self.transcript.apply_event(&Event::parse_line(&line));
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_plain_jsonl_inside_the_log_dir_is_trusted() {
        let home = Path::new("/Users/me");
        let dir = log_dir(home);
        let good = dir.join("abc123-c1.jsonl");
        assert_eq!(
            trusted_log_path(good.to_str().unwrap(), home),
            Some(good.clone())
        );
        for raw in [
            "relative/abc-c1.jsonl",
            "/etc/passwd",
            "/Users/me/.orbit-pi/subagent-logs/../access.json",
            "/Users/me/.orbit-pi/subagent-logs/nested/a-c1.jsonl",
            "/Users/me/.orbit-pi/subagent-logs/a-c1.json",
            "/Users/me/.orbit-pi/subagent-logs/.jsonl",
            "/Users/me/.orbit-pi/subagent-logs/a b.jsonl",
            "/Users/other/.orbit-pi/subagent-logs/a-c1.jsonl",
        ] {
            assert_eq!(trusted_log_path(raw, home), None, "{raw}");
        }
    }

    #[test]
    fn lines_split_on_lf_and_keep_a_partial_tail() {
        let mut partial = Vec::new();
        assert!(take_lines(&mut partial, b"{\"a\":1}\r\n{\"b\"").len() == 1);
        assert_eq!(partial, b"{\"b\"");
        assert_eq!(take_lines(&mut partial, b":2}\n\n"), vec!["{\"b\":2}"]);
        assert!(partial.is_empty());
        assert!(take_lines(&mut partial, b"no newline yet").is_empty());
    }

    #[test]
    fn a_childs_events_build_its_transcript_across_reads() {
        let lines = [
            json!({"type": "message_start", "message": {"role": "user", "content": "Task: find auth"}}),
            json!({"type": "message_end", "message": {"role": "user", "content": "Task: find auth"}}),
            json!({"type": "message_start", "message": {"role": "assistant", "content": []}}),
            json!({"type": "message_end", "message": {"role": "assistant", "content": [
                {"type": "text", "text": "auth is in src/auth.rs"}
            ], "stopReason": "stop"}}),
        ]
        .map(|line| format!("{line}\n"))
        .concat();
        let bytes = lines.as_bytes();
        let mut log = ChildLog::new();
        // Split mid-line: the first read leaves a partial line behind.
        let cut = bytes.len() / 2;
        log.reading = true;
        log.apply(&bytes[..cut]);
        assert!(!log.reading);
        assert_eq!(log.offset(), cut as u64);
        assert!(log.apply(&bytes[cut..]));
        assert_eq!(log.offset(), bytes.len() as u64);
        assert_eq!(log.transcript.message_count(), 2);
        assert_eq!(
            log.transcript.first_user_message().as_deref(),
            Some("Task: find auth")
        );
        assert_eq!(
            log.transcript.last_response_text().map(|(_, text)| text).as_deref(),
            Some("auth is in src/auth.rs")
        );
    }

    #[test]
    fn reading_appended_bytes_resumes_at_the_offset() {
        let dir = std::env::temp_dir().join(format!("orbit-subagent-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("r-c1.jsonl");
        std::fs::write(&file, b"one\ntwo\n").unwrap();
        assert_eq!(read_appended(&file, 0).unwrap(), b"one\ntwo\n");
        assert_eq!(read_appended(&file, 4).unwrap(), b"two\n");
        assert!(read_appended(&file, 8).unwrap().is_empty());
        assert!(read_appended(&dir.join("missing.jsonl"), 0).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
