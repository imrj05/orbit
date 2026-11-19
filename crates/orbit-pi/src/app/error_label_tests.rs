use super::format::*;

#[test]
fn humanize_command_reads_as_a_label() {
    assert_eq!(humanize_command("set_model"), "Set model failed");
    assert_eq!(
        humanize_command("get_available_models"),
        "Get available models failed"
    );
    assert_eq!(humanize_command("follow_up"), "Follow up failed");
    assert_eq!(humanize_command("auth.login"), "Auth login failed");
    // Empty / malformed commands still produce something readable.
    assert_eq!(humanize_command(""), "Command failed");
}

#[test]
fn session_display_title_prefers_an_explicit_name() {
    assert_eq!(
        session_display_title(Some("Feature work"), Some("first user message")),
        "Feature work"
    );
    assert_eq!(
        session_display_title(Some("  "), Some("first user message")),
        "first user message"
    );
    assert_eq!(session_display_title(None, Some("Fix login")), "Fix login");
    assert_eq!(session_display_title(None, None), "New task");
}

#[test]
fn top_bar_folder_label_names_the_session_folder() {
    use std::path::Path;
    assert_eq!(
        top_bar_folder_label(None, Some(Path::new("/Users/me/Developer/orbit"))),
        Some("orbit".into())
    );
    // A remote session names the host folder, not the local workspace.
    assert_eq!(
        top_bar_folder_label(Some("~/src/api/"), Some(Path::new("/Users/me/orbit"))),
        Some("api".into())
    );
    assert_eq!(top_bar_folder_label(Some("/"), None), Some("/".into()));
    assert_eq!(top_bar_folder_label(None, None), None);
}
