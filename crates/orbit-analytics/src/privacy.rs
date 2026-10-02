//! Defense-in-depth filtering for analytics payloads.
//!
//! Events are built from a closed enum, so sensitive data cannot enter by
//! construction. This module is the second line: it names the categories that
//! must never appear and provides checks used both at runtime (stripping
//! forbidden keys) and in tests (asserting payloads are clean).

use serde_json::Value;

/// Property keys that must never be sent. Matching is case-insensitive and
/// substring-based, so `github_username` and `projectPath` are both caught.
pub const FORBIDDEN_KEYS: &[&str] = &[
    "email",
    "name",
    "username",
    "user",
    "account",
    "github",
    "project",
    "workspace",
    "path",
    "repo",
    "repository",
    "file",
    "filename",
    "source",
    "code",
    "command",
    "cmd",
    "output",
    "shell",
    "history",
    "env",
    "environment",
    "api_key",
    "apikey",
    "token",
    "secret",
    "password",
    "credential",
    "cookie",
    "auth",
    "ip",
    "address",
    "location",
    "geo",
    "latitude",
    "longitude",
    "clipboard",
    "prompt",
    "response",
    "conversation",
    "message",
    "content",
    "url",
    "uri",
    "hostname",
    "host",
    "mac",
    "serial",
    "disk",
    "machine",
    "hardware",
    "cwd",
    "directory",
    "folder",
];

/// Value shapes that must never appear even under an innocuous key.
pub const FORBIDDEN_VALUE_MARKERS: &[&str] = &["@", "://", "\\", "~/", "ssh-"];

/// Whether a property key is forbidden.
pub fn is_forbidden_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    FORBIDDEN_KEYS
        .iter()
        .any(|forbidden| lowered.contains(forbidden))
}

/// Whether a scalar value looks like it carries something private. This is a
/// conservative check for tests; it is deliberately not applied to closed-set
/// enum strings like `dark` or `appearance`.
pub fn looks_sensitive(value: &Value) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    if text.is_empty() {
        return false;
    }
    // Absolute paths and secrets.
    if text.starts_with('/') || text.starts_with('~') {
        return true;
    }
    if text.len() > 64 {
        return true;
    }
    // Long hex/base64-ish blobs (tokens, hashes, ids that are not our UUIDs).
    text.len() >= 32
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='))
        && !is_uuid(text)
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36 && value.chars().filter(|c| *c == '-').count() == 4
}

/// Strip forbidden keys and obviously sensitive scalar values. Returns the
/// removed keys so a caller can log the count without logging the values.
pub fn sanitize(properties: &mut std::collections::BTreeMap<String, Value>) -> usize {
    let before = properties.len();
    properties.retain(|key, value| !is_forbidden_key(key) && !looks_sensitive(value));
    before - properties.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn forbidden_keys_are_caught() {
        assert!(is_forbidden_key("project_path"));
        assert!(is_forbidden_key("github_username"));
        assert!(is_forbidden_key("API_KEY"));
        assert!(is_forbidden_key("terminal_command"));
        assert!(!is_forbidden_key("app_version"));
        assert!(!is_forbidden_key("os"));
        assert!(!is_forbidden_key("architecture"));
        assert!(!is_forbidden_key("mode"));
        assert!(!is_forbidden_key("setting"));
    }

    #[test]
    fn sensitive_values_are_caught() {
        assert!(looks_sensitive(&json!("/Users/someone/project")));
        assert!(looks_sensitive(&json!(
            "ghp_abcdefghijklmnopqrstuvwxyz012345"
        )));
        assert!(!looks_sensitive(&json!("dark")));
        assert!(!looks_sensitive(&json!("aarch64")));
        assert!(!looks_sensitive(&json!(
            "550e8400-e29b-41d4-a716-446655440000"
        )));
    }

    #[test]
    fn sanitize_removes_only_the_bad_keys() {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert("mode".to_string(), json!("dark"));
        properties.insert("project_path".to_string(), json!("/tmp/x"));
        properties.insert("token".to_string(), json!("abc"));
        assert_eq!(sanitize(&mut properties), 2);
        assert_eq!(properties.len(), 1);
        assert!(properties.contains_key("mode"));
    }
}
