//! "Add to a server": the dialog's state and how a connection result lands.
//! The scripts that run on the host are covered in `orbit_rpc::remote` and
//! the local half (git, URLs, payload) in `crate::server_clone`.

use super::server_clone_ui::{
    host_for_clone, CloneEntry, ClonePhase, ServerCloneDialog, SourceState,
};
use super::ssh_auth_ui::AuthThen;
use super::*;
use crate::remote_hosts::{ProbeFailure, RemoteHost};
use crate::server_clone::{CloneProtocol, IgnoredFile, ProjectSource, MAX_FILE_BYTES};
use crate::theme::{Theme, ThemeId};

fn host(name: &str) -> RemoteHost {
    RemoteHost {
        name: name.into(),
        ssh: format!("{name}.example"),
        user: Some("deploy".into()),
        port: None,
        dir: "/srv/app".into(),
        enabled: true,
        local_dir: None,
    }
}

fn source() -> ProjectSource {
    ProjectSource {
        root: PathBuf::from("/Users/me/demo"),
        subdir: String::new(),
        name: "demo".into(),
        origin: "https://github.com/o/demo.git".into(),
        files: vec![
            IgnoredFile {
                path: ".env".into(),
                size: 12,
                selected: true,
            },
            IgnoredFile {
                path: "dump.sql".into(),
                size: MAX_FILE_BYTES + 1,
                selected: false,
            },
        ],
        truncated: false,
    }
}

/// An app with the dialog open at generation 7 and the project still being
/// read, as `open_server_clone` leaves it.
fn open_dialog(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        let app = cx.new(OrbitApp::new);
        app.update(cx, |app, cx| {
            let folder = cx.new(ComposerInput::new);
            app.server_clone = Some(ServerCloneDialog {
                generation: 7,
                source: SourceState::Loading,
                hosts: vec![host("vps"), host("box")],
                host_ix: 0,
                protocol: CloneProtocol::Ssh,
                folder,
                phase: ClonePhase::Idle,
                error: None,
            });
        });
        app
    })
}

#[test]
fn a_clone_gets_its_own_host_entry_or_reuses_one() {
    let server = host("vps");
    let local = std::path::Path::new("/Users/me/code/demo");
    let (new, entry) = host_for_clone(
        &server,
        "demo",
        "/home/deploy/demo",
        local,
        std::slice::from_ref(&server),
    );
    assert_eq!(entry, CloneEntry::New);
    assert_eq!(
        new.name,
        tr!("server_clone.host_name", project = "demo", host = "vps")
    );
    assert_eq!(new.dir, "/home/deploy/demo");
    assert_eq!(
        new.local_dir.as_deref(),
        Some("/Users/me/code/demo"),
        "the entry is linked to the project it was cloned from"
    );
    assert_eq!(
        (new.ssh.as_str(), new.user.as_deref()),
        ("vps.example", Some("deploy"))
    );

    let (again, entry) = host_for_clone(
        &server,
        "demo",
        "/home/deploy/demo",
        local,
        &[server.clone(), new.clone()],
    );
    assert_eq!(
        entry,
        CloneEntry::Unchanged,
        "the same server and folder reuse the saved entry"
    );
    assert_eq!(again.name, new.name);

    let unlinked = RemoteHost {
        local_dir: None,
        ..new.clone()
    };
    let (linked, entry) = host_for_clone(
        &server,
        "demo",
        "/home/deploy/demo",
        local,
        std::slice::from_ref(&unlinked),
    );
    assert_eq!(
        entry,
        CloneEntry::Linked,
        "an unlinked entry gains the link"
    );
    assert_eq!(linked.name, new.name);
    assert_eq!(linked.local_dir.as_deref(), Some("/Users/me/code/demo"));

    let (other, _) = host_for_clone(
        &server,
        "demo",
        "/home/deploy/demo-2",
        local,
        std::slice::from_ref(&new),
    );
    assert_eq!(other.name, format!("{} 2", new.name), "names stay unique");
}

#[gpui::test]
fn the_project_fills_the_folder_and_protocol_once_read(cx: &mut gpui::TestAppContext) {
    let app = open_dialog(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_server_clone_source(3, Ok(source()), cx);
            assert!(
                matches!(
                    app.server_clone.as_ref().unwrap().source,
                    SourceState::Loading
                ),
                "a result for another opening is dropped"
            );
            app.finish_server_clone_source(7, Ok(source()), cx);
            let dialog = app.server_clone.as_ref().unwrap();
            assert_eq!(dialog.folder.read(cx).text(), "~/demo");
            assert_eq!(
                dialog.protocol,
                CloneProtocol::Https,
                "the origin's own protocol"
            );
        });
    });
}

#[gpui::test]
fn files_toggle_but_an_oversized_one_stays_off(cx: &mut gpui::TestAppContext) {
    let app = open_dialog(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_server_clone_source(7, Ok(source()), cx);
            app.server_clone_toggle_file(0, cx);
            app.server_clone_toggle_file(1, cx);
            let SourceState::Ready(source) = &app.server_clone.as_ref().unwrap().source else {
                panic!("ready");
            };
            assert!(!source.files[0].selected, ".env unticked");
            assert!(!source.files[1].selected, "over the cap cannot be ticked");
        });
    });
}

#[gpui::test]
fn a_folder_outside_home_or_absolute_paths_is_refused(cx: &mut gpui::TestAppContext) {
    let app = open_dialog(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_server_clone_source(7, Ok(source()), cx);
            let folder = app.server_clone.as_ref().unwrap().folder.clone();
            folder.update(cx, |input, cx| input.set_text("code/demo", cx));
            app.server_clone_submit(cx);
            let dialog = app.server_clone.as_ref().unwrap();
            assert_eq!(dialog.phase, ClonePhase::Idle, "nothing started");
            assert_eq!(
                dialog.error.as_deref(),
                Some(tr!("server_clone.folder_invalid").as_str())
            );
        });
    });
}

/// The connection check runs through the Remote hosts flow; its failures
/// land in the dialog, and only for the opening that asked.
#[gpui::test]
fn a_failed_connection_lands_in_the_dialog(cx: &mut gpui::TestAppContext) {
    let app = open_dialog(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.server_clone.as_mut().unwrap().phase = ClonePhase::Connecting;
            app.finish_remote_probe(
                host("vps"),
                AuthThen::AddToServer(3),
                0,
                Err(ProbeFailure::Other("stale".into())),
                cx,
            );
            assert_eq!(
                app.server_clone.as_ref().unwrap().phase,
                ClonePhase::Connecting
            );

            app.finish_remote_probe(
                host("vps"),
                AuthThen::AddToServer(7),
                0,
                Err(ProbeFailure::PiMissing),
                cx,
            );
            let dialog = app.server_clone.as_ref().unwrap();
            assert_eq!(dialog.phase, ClonePhase::Idle, "the user can try again");
            assert!(
                dialog
                    .error
                    .as_deref()
                    .is_some_and(|e| e.contains("pi.dev/install.sh")),
                "pi missing offers the install command: {:?}",
                dialog.error
            );
        });
    });
}

#[gpui::test]
fn the_dialog_stays_open_while_the_host_works(cx: &mut gpui::TestAppContext) {
    let app = open_dialog(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.server_clone.as_mut().unwrap().phase = ClonePhase::Cloning;
            app.close_server_clone(cx);
            assert!(
                app.server_clone.is_some(),
                "a clone in flight keeps its dialog"
            );
            app.server_clone.as_mut().unwrap().phase = ClonePhase::Idle;
            app.close_server_clone(cx);
            assert!(app.server_clone.is_none());
        });
    });
}

#[gpui::test]
fn the_dialog_renders_its_parts(cx: &mut gpui::TestAppContext) {
    let app = open_dialog(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.finish_server_clone_source(7, Ok(source()), cx)
        })
    });
    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(1280.), px(900.)),
        |_, _| app.clone(),
    );
    for id in [
        "server-clone-dialog",
        "server-clone-host-0",
        "server-clone-host-1",
        "server-clone-protocol-0",
        "server-clone-file-0",
    ] {
        assert!(cx.debug_bounds(id).is_some(), "{id} painted");
    }
}
