//! Regression tests: opening a session whose last turn changed files must
//! paint its changed-files card once and leave it alone.
//!
//! A cold open paints the stored transcript from disk, then pi boots and
//! answers `switch_session`, `get_messages` and `get_state` in that order. The
//! card used to go down on the `switch_session` reply and come back on
//! `get_messages`, and a worktree read made before `get_state` ran under the
//! session being left, so the card could list that session's files.

use super::*;
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

/// One settled turn that wrote `notes.md`.
fn snapshot() -> Value {
    serde_json::json!({
        "messages": [
            {"role": "user", "content": "write the notes"},
            {"role": "assistant", "content": [
                {"type": "toolCall", "id": "c1", "name": "write",
                 "arguments": {"path": "notes.md", "content": "one\ntwo"}}
            ]}
        ]
    })
}

fn card_paths(app: &OrbitApp) -> Vec<String> {
    app.transcript
        .changed_files_summary()
        .into_iter()
        .map(|(path, _, _)| path)
        .collect()
}

fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?}");
}

/// A repository where session `previous` changed `src/lib.rs` in its turn 1.
fn repository_with_previous_turn(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("orbit-open-card-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "--quiet", "--initial-branch=main"]);
    git(&root, &["config", "user.name", "Orbit Test"]);
    git(&root, &["config", "user.email", "orbit@example.com"]);
    std::fs::write(root.join("src/lib.rs"), "fn baseline() {}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "baseline"]);
    crate::checkpoint::capture_turn(&root, "previous", 0).unwrap();
    crate::checkpoint::capture_turn_start(&root, "previous", 1).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "fn baseline() {}\nfn previous() {}\n",
    )
    .unwrap();
    crate::checkpoint::capture_turn(&root, "previous", 1).unwrap();
    root
}

#[gpui::test]
fn finish_captures_the_open_turn_not_a_recovered_checkpoint_number(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.session_id = Some("session".into());
            app.current_workspace = Some(PathBuf::from("/tmp/orbit-missing-turn-repo"));
            assert!(app.turns.begin(0));
            app.turns.note_checkpoint(7);
            app.finish_turn(cx);
            assert_eq!(app.turns.latest, Some(1));
            assert!(app.sidepane.read(cx).has_pending_turn_checkpoint(1));
            assert!(!app.sidepane.read(cx).has_pending_turn_checkpoint(7));
        });
    });
    cx.run_until_parked();
}

/// The disk preview already shows the session and its card. pi's
/// `switch_session` reply lands a round trip before `get_messages`, so
/// dropping the turn clock there took the card (and the run footer it owns)
/// off screen for the frames in between.
#[gpui::test]
fn the_card_stays_up_while_pi_attaches_to_a_previewed_session(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.current_session_path = Some(PathBuf::from("/tmp/orbit-open-card.jsonl"));
            // The preview read from disk.
            app.apply_messages_snapshot(&snapshot(), cx);
            assert_eq!(card_paths(app), vec!["notes.md".to_string()]);

            let mut refresh = false;
            app.on_response(
                "switch_session",
                true,
                Some(&serde_json::json!({"cancelled": false})),
                None,
                "1",
                &mut refresh,
                cx,
            );
            assert_eq!(
                card_paths(app),
                vec!["notes.md".to_string()],
                "attaching pi to the session on screen must not take its card down"
            );
            assert_eq!(app.turns.latest, Some(1), "the previewed turn clock stays");

            // The authoritative snapshot repaints the same card in one update.
            app.on_response(
                "get_messages",
                true,
                Some(&snapshot()),
                None,
                "2",
                &mut refresh,
                cx,
            );
            assert_eq!(card_paths(app), vec!["notes.md".to_string()]);
        });
    });
}

#[gpui::test]
fn checkpoint_card_survives_equivalent_snapshot_but_not_a_different_turn(
    cx: &mut gpui::TestAppContext,
) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.apply_messages_snapshot(&snapshot(), cx);
            app.transcript
                .set_review_card_worktree_files(0, vec![("shell.txt".into(), 3, 1)]);
            app.apply_messages_snapshot(&snapshot(), cx);
            assert_eq!(card_paths(app), vec!["shell.txt"]);

            let mut different = snapshot();
            different["messages"][0]["content"] = serde_json::json!("another prompt");
            app.apply_messages_snapshot(&different, cx);
            assert_eq!(card_paths(app), vec!["notes.md"]);
        });
    });
}

#[gpui::test]
fn identical_incoming_snapshot_does_not_reuse_the_previous_sessions_checkpoint(
    cx: &mut gpui::TestAppContext,
) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(PathBuf::from("/tmp/orbit-old-card.jsonl"));
            app.apply_messages_snapshot(&snapshot(), cx);
            app.transcript
                .set_review_card_worktree_files(0, vec![("old-only.txt".into(), 1, 0)]);
            app.switch_to_session(
                SessionInfo {
                    path: PathBuf::from("/tmp/orbit-new-card.jsonl"),
                    id: "new".into(),
                    cwd: PathBuf::from("/tmp/orbit-missing-card-workspace"),
                    title: String::new(),
                    first_message: String::new(),
                    modified: SystemTime::now(),
                },
                true,
                cx,
            );
            app.apply_messages_snapshot(&snapshot(), cx);
            assert_eq!(card_paths(app), vec!["notes.md"]);
        });
    });
}

/// A turn still open when a process attaches belonged to the process it
/// replaces. That run is over, so its clock goes, as it did before.
#[gpui::test]
fn an_open_turn_is_dropped_when_a_new_process_attaches(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.current_session_path = Some(PathBuf::from("/tmp/orbit-open-card.jsonl"));
            app.apply_messages_snapshot(&snapshot(), cx);
            app.turns.open = true;

            let mut refresh = false;
            app.on_response(
                "switch_session",
                true,
                Some(&serde_json::json!({"cancelled": false})),
                None,
                "1",
                &mut refresh,
                cx,
            );
            assert!(!app.turns.open, "the interrupted turn is closed");
            assert_eq!(app.turns.latest, None);
            assert!(card_paths(app).is_empty());
        });
    });
}

/// Until pi answers `get_state`, the app does not know the incoming session's
/// id. Keeping the outgoing one made the preview's worktree read resolve that
/// session's checkpoints, and the card showed its files.
#[gpui::test]
fn an_opened_session_never_lists_the_previous_sessions_files(cx: &mut gpui::TestAppContext) {
    let repo = repository_with_previous_turn("stale-id");
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.session_id = Some("previous".to_string());
            // A missing workspace keeps the switch from spawning pi.
            let session = SessionInfo {
                path: PathBuf::from("/tmp/orbit-open-card-incoming.jsonl"),
                id: "incoming".to_string(),
                cwd: repo.join("missing"),
                title: String::new(),
                first_message: String::new(),
                modified: SystemTime::now(),
            };
            app.switch_to_session(session, true, cx);
            assert_eq!(
                app.session_id, None,
                "the outgoing session's id must not outlive the switch"
            );
            // Both sessions live in the same repository.
            app.current_workspace = Some(repo.clone());
            app.apply_messages_snapshot(&snapshot(), cx);
        });
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            card_paths(app.read(cx)),
            vec!["notes.md".to_string()],
            "the card describes the session on screen"
        );
    });
    let _ = std::fs::remove_dir_all(repo);
}
