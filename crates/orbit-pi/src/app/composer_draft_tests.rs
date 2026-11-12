//! Composer state that belongs to a *target*, not to the app: the unsent
//! draft a session carries while the user is elsewhere, and the long-paste
//! pills that hold text compactly until it is sent.

use super::*;

/// Long pastes collapse into a pill; anything a person would type inline
/// stays inline. The gate is the whole feature — a false positive swallows
/// text the user was still arranging, a false negative wrecks the box.
#[gpui::test]
fn long_pastes_pill_and_short_ones_stay_inline(_cx: &mut gpui::TestAppContext) {
    // Short, ordinary pastes ride the caret.
    assert!(!crate::composer::should_pill_paste("hello"));
    assert!(!crate::composer::should_pill_paste("a line\nand another"));
    assert!(!crate::composer::should_pill_paste(
        &"x".repeat(crate::composer::PASTE_PILL_MIN_CHARS - 1)
    ));

    // Many lines, or one very long line, becomes a pill.
    let many = (0..crate::composer::PASTE_PILL_MIN_LINES)
        .map(|ix| format!("line {ix}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(crate::composer::should_pill_paste(&many));
    assert!(crate::composer::should_pill_paste(
        &"x".repeat(crate::composer::PASTE_PILL_MIN_CHARS)
    ));
}

/// A draft is keyed to its session file, so switching away and back restores
/// exactly what was being written — and switching to a *different* session
/// never shows the first one's text.
#[gpui::test]
fn drafts_follow_their_session(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    let first = PathBuf::from("/tmp/orbit-draft-a.jsonl");
    let second = PathBuf::from("/tmp/orbit-draft-b.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // Writing in the first session.
            app.current_session_path = Some(first.clone());
            app.composer_is_new_task = false;
            app.input
                .update(cx, |input, cx| input.set_text("half a thought", cx));
            app.pasted_texts.push(PastedText::new("pasted".into()));
            app.files.push(AttachedFile {
                path: PathBuf::from("/tmp/notes.md"),
                name: "notes.md".into(),
                size: 12,
            });
            app.save_composer_draft(cx);

            // Leaving parks it and leaves nothing behind.
            app.restore_composer_draft(Some(&second), cx);
            assert_eq!(app.input.read(cx).text(), "");
            assert!(app.pasted_texts.is_empty());
            assert!(app.files.is_empty());

            // Coming back brings it, attachments included.
            app.restore_composer_draft(Some(&first), cx);
            assert_eq!(app.input.read(cx).text(), "half a thought");
            assert_eq!(app.pasted_texts.len(), 1);
            assert_eq!(app.pasted_texts[0].content, "pasted");
            assert_eq!(app.files.len(), 1);
            assert_eq!(app.files[0].name, "notes.md");
        });
    });
}

/// An empty composer parks no draft, so an unedited session does not leave a
/// blank entry behind (and a stale draft can never resurrect old text).
#[gpui::test]
fn empty_composers_park_no_draft(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let path = PathBuf::from("/tmp/orbit-draft-empty.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(path.clone());
            app.composer_is_new_task = false;
            app.input
                .update(cx, |input, cx| input.set_text("   \n  ", cx));
            app.save_composer_draft(cx);
            assert!(app.drafts.is_empty(), "whitespace is not a draft");
        });
    });
}

/// The New Task page has no session file yet, so its draft lives in its own
/// slot — typing there then opening a session must not leak it.
#[gpui::test]
fn the_new_task_draft_has_its_own_slot(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let path = PathBuf::from("/tmp/orbit-draft-task.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = None;
            app.input
                .update(cx, |input, cx| input.set_text("brand new task", cx));
            app.save_composer_draft(cx);
            assert!(app.new_task_draft.is_some());

            app.restore_composer_draft(Some(&path), cx);
            assert_eq!(app.input.read(cx).text(), "");

            app.restore_composer_draft(None, cx);
            assert_eq!(app.input.read(cx).text(), "brand new task");
        });
    });
}

/// A pi stand-in that stays alive and idle without answering anything: the
/// New Task draft swap happens before `new_session` is even written, so the
/// reuse path only needs the process to look live.
#[cfg(unix)]
struct FakePi {
    _dir: tempfile::TempDir,
    bin: PathBuf,
}

#[cfg(unix)]
impl FakePi {
    fn start() -> Self {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().expect("create fake-pi dir");
        let bin = dir.path().join("fake-pi");
        std::fs::write(&bin, "#!/bin/sh\nexec cat > /dev/null\n").expect("write fake pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake pi");
        Self { _dir: dir, bin }
    }
}

#[cfg(unix)]
fn adopt_fake_pi(app: &mut OrbitApp, fake: &FakePi) {
    let client = PiClient::spawn_with_bin(
        fake.bin.to_str().expect("utf-8 path"),
        fake._dir.path(),
        None,
    )
    .expect("spawn fake pi");
    app.adopt_client(client);
}

/// New Task (⌘N, the sidebar button, the menu) usually takes the *reuse* path:
/// the idle process is kept and `new_session` is sent in place. That path used
/// to skip the draft swap the parked-and-respawned path does, so a prompt and
/// its attachment typed on the New Task page were never loaded back — and the
/// session being left handed its own text to the new page.
#[cfg(unix)]
#[gpui::test]
fn the_new_task_draft_comes_back_through_the_reused_process(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    let fake = FakePi::start();
    let session = PathBuf::from("/tmp/orbit-draft-return.jsonl");
    let target = SessionInfo {
        path: session.clone(),
        id: "draft-return".into(),
        cwd: fake._dir.path().join("missing-workspace"),
        title: "Existing session".into(),
        first_message: String::new(),
        modified: SystemTime::UNIX_EPOCH,
    };

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            // The New Task page holds a prompt and a screenshot; opening a
            // session from the sidebar parks that draft in its own slot.
            app.current_session_path = None;
            app.input
                .update(cx, |input, cx| input.set_text("start a task", cx));
            app.attachments.push(Attachment {
                name: "start.png".into(),
                mime: "image/png".into(),
                data: String::new(),
                preview: None,
                original_len: 0,
                stored_len: 0,
                compressed: false,
            });
            // pi has already assigned a file even though nothing was sent.
            app.adopt_session_file(PathBuf::from("/tmp/orbit-initial-task.jsonl"));
            app.switch_to_session(target, false, cx);
            assert!(app.new_task_draft.is_some());
            assert_eq!(app.input.read(cx).text(), "");
            app.input
                .update(cx, |input, cx| input.set_text("on the session", cx));
            // A missing workspace leaves the target without a process. Give it
            // a live one to exercise New Task's reuse path.
            adopt_fake_pi(app, &fake);
        });
    });

    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx)
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert_eq!(
                app.input.read(cx).text(),
                "start a task",
                "the New Task draft must come back, not the session's text"
            );
            assert_eq!(app.attachments.len(), 1, "the attachment rides with it");
            assert_eq!(app.attachments[0].name, "start.png");
            assert!(app.current_session_path.is_none());
            assert_eq!(
                app.drafts.get(&session).map(|draft| draft.text.as_str()),
                Some("on the session"),
                "the session being left keeps its own draft"
            );

            // pi can report a file before the first message is sent. It is
            // still the New Task page, not a session draft keyed by that file.
            let new_file = PathBuf::from("/tmp/orbit-unsent-task.jsonl");
            app.adopt_session_file(new_file.clone());
            app.save_composer_draft(cx);
            assert!(app.new_task_draft.is_some());
            assert!(!app.drafts.contains_key(&new_file));
            app.current_session_path = Some(session.clone());
            app.restore_composer_draft(Some(&session), cx);
            assert_eq!(app.input.read(cx).text(), "on the session");
            app.save_composer_draft(cx);
            app.restore_composer_draft(None, cx);
            assert_eq!(app.input.read(cx).text(), "start a task");
            assert_eq!(app.attachments[0].name, "start.png");
        });
    });
}

#[cfg(unix)]
#[gpui::test]
fn github_task_preserves_new_task_drafts_and_starts_only_when_empty(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let fake = FakePi::start();
    let workspace = fake._dir.path().to_path_buf();
    let session = PathBuf::from("/tmp/github-draft-session.jsonl");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.current_workspace = Some(workspace.clone());
            app.current_session_path = None;
            app.input
                .update(cx, |input, cx| input.set_text("existing", cx));
            app.start_github_task(workspace.clone(), "github prompt".into(), window, cx);
            assert_eq!(app.input.read(cx).text(), "existing");
            assert!(!app.toasts.is_empty());

            app.input.update(cx, |input, cx| input.set_text("  ", cx));
            app.start_github_task(workspace.clone(), "github prompt".into(), window, cx);
            assert_eq!(app.input.read(cx).text(), "  ");

            app.input.update(cx, |input, cx| {
                input.clear(cx);
                input.pasted_texts.push("pending paste".into());
                input.pasted_files.push(PathBuf::from("/tmp/pending"));
            });
            app.start_github_task(workspace.clone(), "github prompt".into(), window, cx);
            assert_eq!(app.input.read(cx).pasted_texts, ["pending paste"]);
            assert_eq!(app.input.read(cx).pasted_files.len(), 1);
            app.input.update(cx, |input, _| {
                input.pasted_texts.clear();
                input.pasted_files.clear();
            });

            app.files.push(AttachedFile {
                path: "/tmp/a".into(),
                name: "a".into(),
                size: 1,
            });
            app.start_github_task(workspace.clone(), "github prompt".into(), window, cx);
            assert_eq!(app.files.len(), 1);
            assert_eq!(app.input.read(cx).text(), "");
            app.files.clear();

            app.current_session_path = Some(session.clone());
            app.composer_is_new_task = false;
            app.input
                .update(cx, |input, cx| input.set_text("session draft", cx));
            app.new_task_draft = Some(ComposerDraft {
                pasted_texts: vec![PastedText::new("stored paste".into())],
                ..Default::default()
            });
            app.start_github_task(workspace.clone(), "github prompt".into(), window, cx);
            assert_eq!(app.current_session_path.as_ref(), Some(&session));
            assert_eq!(app.input.read(cx).text(), "session draft");
            assert_eq!(
                app.new_task_draft.as_ref().unwrap().pasted_texts[0].content,
                "stored paste"
            );

            app.new_task_draft = None;
            app.start_github_task(workspace.clone(), "github prompt".into(), window, cx);
            assert_eq!(app.input.read(cx).text(), "github prompt");
            assert_eq!(app.drafts.get(&session).unwrap().text, "session draft");
            assert!(app.files.is_empty());
            assert!(app.pasted_texts.is_empty());
        });
    });
}

/// Pills are only a compact way to *hold* text: on submit the paste rejoins
/// the prompt as its own block, so the agent never reads it as a continuation
/// of the sentence above.
#[gpui::test]
fn pills_fold_back_into_the_prompt(_cx: &mut gpui::TestAppContext) {
    let paste = PastedText::new("fn main() {\n    todo!()\n}".into());
    assert_eq!(
        OrbitApp::fold_pasted_texts("explain this", std::slice::from_ref(&paste)),
        "explain this\n\nfn main() {\n    todo!()\n}"
    );
    // A pill on its own is the whole prompt — no leading blank block.
    assert_eq!(
        OrbitApp::fold_pasted_texts("", std::slice::from_ref(&paste)),
        "fn main() {\n    todo!()\n}"
    );
    assert_eq!(OrbitApp::fold_pasted_texts("just text", &[]), "just text");
}

/// The block the composer folds into a prompt must read back out of it, or a
/// sent message keeps showing a raw `@/path` line where the composer showed a
/// chip. Prose that merely mentions a file is left alone.
#[gpui::test]
fn the_folded_file_block_reads_back_out_of_the_sent_message(_cx: &mut gpui::TestAppContext) {
    let attached = |path: &str| AttachedFile {
        path: PathBuf::from(path),
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: 0,
    };
    let cwd = PathBuf::from("/work/repo");
    let files = [
        attached("/work/repo/src/main.rs"),
        attached("/Users/someone/Downloads/APP-Install.md"),
    ];
    let text =
        OrbitApp::fold_attached_files("can you check the app install process", &files, Some(&cwd));
    assert_eq!(
        OrbitApp::split_attached_files(&text),
        Some((
            "can you check the app install process",
            vec!["src/main.rs", "/Users/someone/Downloads/APP-Install.md"],
        ))
    );

    // A file-only prompt is the whole block: no empty prose bubble above it.
    let only = OrbitApp::fold_attached_files("", &files[..1], Some(&cwd));
    assert_eq!(
        OrbitApp::split_attached_files(&only),
        Some(("", vec!["src/main.rs"]))
    );

    // Whitespace paths round-trip through the backticked form.
    let spaced = [attached("/work/repo/my notes.md")];
    let text = OrbitApp::fold_attached_files("read this", &spaced, Some(&cwd));
    assert_eq!(
        OrbitApp::split_attached_files(&text),
        Some(("read this", vec!["my notes.md"]))
    );

    // Ordinary prose is never mistaken for a file block.
    for prose in [
        "look at src/main.rs",
        "run `npm test`",
        "`src/main.rs` is where it lives",
        "two\n\nparagraphs",
        "```\n@not-a-file\n```",
    ] {
        assert_eq!(
            OrbitApp::split_attached_files(prose),
            None,
            "{prose:?} is prose, not an attachment block"
        );
    }
}

/// Files ride the prompt as path mentions, one per line, workspace-relative
/// when they live inside it. A file-only message is still a sendable prompt.
#[gpui::test]
fn file_attachments_fold_into_the_prompt_as_path_mentions(_cx: &mut gpui::TestAppContext) {
    let attached = |path: &str| AttachedFile {
        path: PathBuf::from(path),
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: 0,
    };
    let cwd = PathBuf::from("/work/repo");
    let inside = attached("/work/repo/src/main.rs");
    let outside = attached("/tmp/notes.txt");

    assert_eq!(
        OrbitApp::fold_attached_files("explain this", std::slice::from_ref(&inside), Some(&cwd)),
        "explain this\n\n@src/main.rs"
    );
    // A file on its own is the whole prompt.
    assert_eq!(
        OrbitApp::fold_attached_files("", std::slice::from_ref(&outside), Some(&cwd)),
        "@/tmp/notes.txt"
    );
    // Each file gets its own line.
    assert_eq!(
        OrbitApp::fold_attached_files("", &[inside, outside], Some(&cwd)),
        "@src/main.rs\n@/tmp/notes.txt"
    );
    // Whitespace would split an `@` token, so the path becomes a code span.
    let spaced = attached("/work/repo/my notes.md");
    assert_eq!(
        OrbitApp::fold_attached_files("", std::slice::from_ref(&spaced), Some(&cwd)),
        "`my notes.md`"
    );
    // No files leaves the text exactly as typed.
    assert_eq!(
        OrbitApp::fold_attached_files("just text", &[], Some(&cwd)),
        "just text"
    );
}
