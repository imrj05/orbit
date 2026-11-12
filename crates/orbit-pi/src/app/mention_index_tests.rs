//! `@` ranks an in-memory workspace index instead of spawning `fd`.
//!
//! These tests pin the behaviour that replaced the subprocess: the menu is the
//! walk (so it agrees with the Explorer about what exists), a file the user just
//! created is in the next `@` without a restart, dotfiles are findable, and a
//! re-typed query does not rank the workspace again.

use super::*;
use crate::theme::{Theme, ThemeId};

fn app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

/// A throwaway workspace, named per test so parallel tests cannot see each
/// other's files.
fn workspace(name: &str, files: &[&str]) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("orbit-mention-index-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create workspace");
    for file in files {
        let path = root.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "").expect("write file");
    }
    root
}

/// Point the app at `root` and wait for a fresh index to answer `query` with
/// `expected`. Waits on the menu, not on internals: the walk lands on a
/// background thread, and what matters is that the answer arrives.
fn wait_for_entry(
    app: &Entity<OrbitApp>,
    cx: &mut gpui::TestAppContext,
    query: &str,
    expected: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let found = app.update(cx, |app, cx| {
            app.at_paths(query, cx).iter().any(|hit| hit == expected)
        });
        if found {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "`@{query}` never offered {expected}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn open_workspace(app: &Entity<OrbitApp>, cx: &mut gpui::TestAppContext, root: &Path) {
    app.update(cx, |app, cx| {
        app.current_workspace = Some(root.to_path_buf());
        app.invalidate_index(true, cx);
    });
}

/// The menu is the walk. A nested file is offered, and `.git` — which a walk
/// that includes hidden entries would otherwise drag in whole — is not.
#[gpui::test]
fn the_menu_is_the_workspace_walk(cx: &mut gpui::TestAppContext) {
    let root = workspace(
        "walk",
        &["src/deep/main.rs", ".git/config", ".git/objects/ab/cd"],
    );
    let app = app(cx);
    open_workspace(&app, cx, &root);

    wait_for_entry(&app, cx, "main", "src/deep/main.rs");
    app.update(cx, |app, cx| {
        assert!(
            app.at_paths("config", cx).is_empty(),
            "`.git` is metadata, never a mention"
        );
        assert!(
            !app.at_paths("main", cx)
                .iter()
                .any(|hit| hit.starts_with(".git")),
            "no VCS object may reach the menu"
        );
    });
    let _ = fs::remove_dir_all(&root);
}

/// `.gitignore` decides, and it decides in a plain folder too — the same rule
/// the Explorer follows, because it is the same walk.
#[gpui::test]
fn gitignore_applies_outside_a_repository(cx: &mut gpui::TestAppContext) {
    let root = workspace("ignore", &["keep.rs", "build/out.js"]);
    fs::write(root.join(".gitignore"), "build/\n").expect("write gitignore");
    let app = app(cx);
    open_workspace(&app, cx, &root);

    wait_for_entry(&app, cx, "keep", "keep.rs");
    app.update(cx, |app, cx| {
        assert!(
            app.at_paths("out", cx).is_empty(),
            "a gitignored directory stays out of the menu"
        );
    });
    let _ = fs::remove_dir_all(&root);
}

/// Dotfiles are searchable — `fd --hidden` found them, so `@` must too — while
/// a plain query still prefers the files the Explorer shows.
#[gpui::test]
fn hidden_files_are_searchable(cx: &mut gpui::TestAppContext) {
    let root = workspace("hidden", &[".env", "env.example"]);
    let app = app(cx);
    open_workspace(&app, cx, &root);

    wait_for_entry(&app, cx, ".env", ".env");
    app.update(cx, |app, cx| {
        assert_eq!(
            app.at_paths("env", cx).first().map(String::as_str),
            Some("env.example"),
            "a visible file outranks a dotfile on a tie"
        );
    });
    let _ = fs::remove_dir_all(&root);
}

/// A file created in the app is in the next `@` — no restart, no manual
/// refresh — and a deleted one drops out.
#[gpui::test]
fn a_created_file_reaches_the_menu_and_a_deleted_one_leaves(cx: &mut gpui::TestAppContext) {
    let root = workspace("ops", &["alpha.rs"]);
    let app = app(cx);
    open_workspace(&app, cx, &root);
    wait_for_entry(&app, cx, "alpha", "alpha.rs");

    fs::write(root.join("beta.rs"), "").expect("create beta");
    app.update(cx, |app, cx| app.invalidate_index(true, cx));
    wait_for_entry(&app, cx, "beta", "beta.rs");

    fs::remove_file(root.join("alpha.rs")).expect("delete alpha");
    app.update(cx, |app, cx| app.invalidate_index(true, cx));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let gone = app.update(cx, |app, cx| {
            !app.at_paths("alpha", cx)
                .iter()
                .any(|hit| hit == "alpha.rs")
        });
        if gone {
            break;
        }
        assert!(Instant::now() < deadline, "the deleted file stayed in `@`");
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = fs::remove_dir_all(&root);
}

/// Redrawing the popup must not re-rank the workspace: the same query inside
/// one index generation is served from the memo, and a rebuild clears it.
#[gpui::test]
fn a_retyped_query_is_served_from_the_memo(cx: &mut gpui::TestAppContext) {
    let root = workspace("memo", &["main.rs"]);
    let app = app(cx);
    open_workspace(&app, cx, &root);
    wait_for_entry(&app, cx, "main", "main.rs");

    app.update(cx, |app, cx| {
        let first = app.at_paths("main", cx);
        let generation = app.mention_index_generation;
        let second = app.at_paths("main", cx);
        assert_eq!(first, second, "the memo must answer identically");
        assert_eq!(
            app.mention_query_memo.as_ref().map(|(g, _, _)| *g),
            Some(generation),
            "the hits are stamped with the generation that produced them"
        );

        // A rebuild invalidates the memo, so stale paths cannot be served.
        app.invalidate_index(true, cx);
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, _| {
        assert!(
            app.mention_query_memo.is_none(),
            "a rebuild must drop the previous generation's hits"
        );
    });
    let _ = fs::remove_dir_all(&root);
}
