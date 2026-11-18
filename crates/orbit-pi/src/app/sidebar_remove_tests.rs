use super::sidebar::*;
use super::*;
use std::fs;

fn group_labels(rows: &[SideRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| match r {
            SideRow::Workspace { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

fn rows_for(workspaces: &[PathBuf]) -> Vec<SideRow> {
    build_sidebar_rows(
        &[],
        workspaces,
        WorkspaceSort::Manual,
        &HashMap::new(),
        "none",
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        &HashSet::new(),
        &None,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
}

#[test]
fn drop_workspace_removes_the_exact_path() {
    let mut workspaces = vec![PathBuf::from("/work/alpha"), PathBuf::from("/work/beta")];
    assert!(drop_workspace(&mut workspaces, Path::new("/work/alpha")));
    assert_eq!(workspaces, vec![PathBuf::from("/work/beta")]);

    // A path that isn't listed is a no-op.
    assert!(!drop_workspace(&mut workspaces, Path::new("/work/gamma")));
    assert_eq!(workspaces, vec![PathBuf::from("/work/beta")]);
}

#[test]
fn removed_workspace_leaves_no_sidebar_group() {
    let mut workspaces = vec![PathBuf::from("/work/alpha"), PathBuf::from("/work/beta")];
    // A workspace with no sessions still gets a header while it is listed.
    assert_eq!(group_labels(&rows_for(&workspaces)), vec!["alpha", "beta"]);

    drop_workspace(&mut workspaces, Path::new("/work/alpha"));
    assert_eq!(
        group_labels(&rows_for(&workspaces)),
        vec!["beta"],
        "the removed empty workspace must not leave its heading behind"
    );
}

/// Two spellings of one folder must not survive as two workspaces: removing
/// the one the user clicked has to take the other with it, or the second
/// header looks like the removal did nothing.
#[cfg(unix)]
#[test]
fn drop_workspace_removes_every_symlinked_spelling() {
    let root = std::env::temp_dir().join(format!("orbit-drop-ws-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let real = root.join("project");
    fs::create_dir_all(&real).unwrap();
    let link = root.join("shortcut");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let real = fs::canonicalize(&real).unwrap();
    let mut workspaces = vec![real.clone(), link.clone()];
    assert!(drop_workspace(&mut workspaces, &link));
    assert!(
        workspaces.is_empty(),
        "both spellings must be dropped: {workspaces:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// A legacy store that listed the same folder under two spellings collapses to
/// one entry when it is read back.
#[cfg(unix)]
#[test]
fn parse_workspace_store_collapses_symlinked_duplicates() {
    let root = std::env::temp_dir().join(format!("orbit-parse-ws-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let real = root.join("project");
    fs::create_dir_all(&real).unwrap();
    let link = root.join("shortcut");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let store = parse_workspace_store(&serde_json::json!({
        "workspaces": [link.to_string_lossy(), real.to_string_lossy()],
    }));
    assert_eq!(store.workspaces.len(), 1, "{:?}", store.workspaces);

    let _ = fs::remove_dir_all(&root);
}

/// The workspace menu's **Archive all sessions** (#131): every session of
/// that workspace is archived, and nothing from another workspace.
#[gpui::test]
fn archiving_a_workspace_archives_only_its_sessions(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    let session = |name: &str, cwd: &str| SessionInfo {
        path: PathBuf::from(format!("/archive-test/{name}.jsonl")),
        id: name.into(),
        cwd: PathBuf::from(cwd),
        title: name.into(),
        first_message: String::new(),
        modified: SystemTime::UNIX_EPOCH,
    };
    let app = cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.workspaces = vec![
                PathBuf::from("/archive-test/alpha"),
                PathBuf::from("/archive-test/beta"),
            ];
            app.sessions = vec![
                session("a1", "/archive-test/alpha"),
                session("a2", "/archive-test/alpha"),
                session("b1", "/archive-test/beta"),
            ];
            app.workspace_menu = Some(WorkspaceMenu {
                label: sessions::workspace_label(Path::new("/archive-test/alpha")),
                cwd: PathBuf::from("/archive-test/alpha"),
                at: None,
                picker: false,
                confirm_delete: false,
                can_move_up: false,
                can_move_down: true,
                cursor: Default::default(),
            });
            app.on_workspace_archive_sessions(cx);

            assert!(app.workspace_menu.is_none(), "the menu closes");
            assert!(crate::archive::contains(Path::new(
                "/archive-test/a1.jsonl"
            )));
            assert!(crate::archive::contains(Path::new(
                "/archive-test/a2.jsonl"
            )));
            assert!(
                !crate::archive::contains(Path::new("/archive-test/b1.jsonl")),
                "another workspace's session is left alone"
            );
        });
    });
}

// ── Delete project & files (#131) ──────────────────────────────────────
// The one irreversible action in the sidebar: these guards are what stop it
// from removing a filesystem root, the home folder, a folder that merely
// contains other projects, or a folder a live pi process is working in.

fn session_in(name: &str, cwd: &str) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(format!("/guard-test/{name}.jsonl")),
        id: name.into(),
        cwd: PathBuf::from(cwd),
        title: name.into(),
        first_message: String::new(),
        modified: SystemTime::UNIX_EPOCH,
    }
}

#[test]
fn deleting_a_root_is_refused() {
    let refused = project_delete_refusal(Path::new("/"), &[], &HashSet::new());
    assert_eq!(refused, Some(ProjectDeleteRefusal::Root));
}

#[test]
fn deleting_the_home_folder_is_refused() {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let Some(home) = home else {
        return;
    };
    assert_eq!(
        project_delete_refusal(&home, &[], &HashSet::new()),
        Some(ProjectDeleteRefusal::Home)
    );
}

/// A folder that *contains* another project is not the project the user
/// right-clicked: deleting it would take unrelated work with it.
#[test]
fn deleting_a_parent_of_another_project_is_refused() {
    let sessions = [session_in("nested", "/guard-test/alpha/sub")];
    assert_eq!(
        project_delete_refusal(Path::new("/guard-test/alpha"), &sessions, &HashSet::new()),
        Some(ProjectDeleteRefusal::ContainsProjects)
    );
}

/// The project's own sessions are not nested projects — they are what the
/// folder is for, so they do not block the delete.
#[test]
fn the_projects_own_sessions_do_not_block_it() {
    let sessions = [session_in("own", "/guard-test/alpha")];
    assert_eq!(
        project_delete_refusal(Path::new("/guard-test/alpha"), &sessions, &HashSet::new()),
        None
    );
}

/// A live pi process (open or parked) still reads and writes inside the
/// folder; removing it underneath is refused until that task is closed.
#[test]
fn deleting_a_folder_with_a_live_session_is_refused() {
    let sessions = [session_in("own", "/guard-test/alpha")];
    let live: HashSet<PathBuf> = sessions.iter().map(|s| s.path.clone()).collect();
    assert_eq!(
        project_delete_refusal(Path::new("/guard-test/alpha"), &sessions, &live),
        Some(ProjectDeleteRefusal::SessionLive)
    );
}

/// Each refusal has a sentence the user can act on; a key mismatch would
/// render the raw key in the toast.
#[test]
fn every_refusal_has_a_sentence() {
    for refusal in [
        ProjectDeleteRefusal::Root,
        ProjectDeleteRefusal::Home,
        ProjectDeleteRefusal::ContainsProjects,
        ProjectDeleteRefusal::SessionLive,
    ] {
        let reason = refusal.reason();
        assert!(!reason.trim().is_empty(), "{refusal:?} has no sentence");
        assert!(
            !reason.starts_with("sidebar."),
            "{refusal:?} rendered its key"
        );
    }
}
