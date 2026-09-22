//! Orbit Pi — a native desktop workbench for the pi coding agent.
//!
//! Pure Rust on GPUI; the agent runtime is the `pi` CLI spoken to over its
//! JSONL RPC protocol (see `crates/orbit-rpc`). Quit with cmd-q.

// Windows release builds are GUI applications: this sets the PE subsystem to
// `WINDOWS`, so double-clicking `orbit-pi.exe` (or the installer launching it)
// never spawns a console window. Debug builds keep the console so `eprintln!`
// diagnostics stay visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod access;
mod app;
mod app_icon;
mod ask;
mod assets;
mod auth;
mod branch_picker;
mod bundled_extensions;
mod checkpoint;
mod command_palette;
mod commit_message;
mod composer;
mod context_meter;
mod dialog;
mod dither;
mod favorites;
mod git;
mod git_panel;
mod highlight;
mod http;
mod mentions;
mod message_scroller;
mod model_selector;
mod model_selector_match;
mod notifications;
mod onboarding;
mod pi_update;
mod platform;
mod plugins;
mod providers;
mod quota;
mod review;
mod sessions;
mod shimmer;
mod sidepane;
mod skills;
mod terminal;
mod theme;
mod toast;
mod transcript;
mod transcript_view;
mod updater;
mod usage;
mod watch;
mod workspace_picker;

use std::time::Duration;

use app::OrbitApp;
use gpui::{
    actions, prelude::*, px, size, App, Application, AsyncWindowContext, Bounds, Entity, Focusable,
    KeyBinding, Menu, MenuItem, SystemMenuType, Timer, WindowBounds, WindowOptions,
};

// Composer-scoped actions (bound in the `Composer` key context).
actions!(
    composer_keys,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        Newline,
        SelectLeft,
        SelectRight,
        SelectAll,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineLeft,
        LineRight,
        SelectLineLeft,
        SelectLineRight,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        AutocompleteAccept,
        Submit,
    ]
);

// App-level actions.
actions!(
    orbit_keys,
    [
        Quit,
        AbortRun,
        NewSession,
        RefreshSessions,
        OpenSettings,
        OpenAbout,
        ToggleUsage,
        ToggleCommandPalette,
        ToggleModelMenu,
        ToggleThinkingMenu,
        CopyLastResponse,
        PrevTurn,
        NextTurn,
        CheckForUpdates,
        SteerRun,
        ToggleSearch,
        SearchNext,
        SearchPrev,
        SearchClose,
        ToggleTerminal
    ]
);

// Model-selector popup actions (bound to the `Picker` context, which rides
// on the popup's filter input).
actions!(
    picker_keys,
    [
        PickerCancel,
        PickerConfirm,
        PickerSelectNext,
        PickerSelectPrev
    ]
);
// Terminal-panel actions (bound to the `Terminal` context on the grid).
actions!(terminal_keys, [TerminalEscape]);

// Composer "+" add-menu actions (bound to the `AddMenu` context, which
// rides on the open menu's focus handle).
actions!(
    add_menu_keys,
    [AddMenuNext, AddMenuPrev, AddMenuConfirm, AddMenuClose]
);

// Access-mode picker actions (bound to the `AccessMenu` context, which rides
// on the open popup's focus handle).
actions!(
    access_menu_keys,
    [
        AccessMenuNext,
        AccessMenuPrev,
        AccessMenuConfirm,
        AccessMenuClose
    ]
);

// Inline access-guard approval actions (bound to the `Approval` context on the
// approval bar above the composer).
actions!(
    approval_keys,
    [ApprovalNext, ApprovalPrev, ApprovalConfirm, ApprovalClose]
);

// Extension-dialog actions (bound to the `DialogSelect` context on the option
// card and `DialogInput` on its text field).
actions!(
    dialog_keys,
    [DialogCancel, DialogConfirm, DialogNext, DialogPrev]
);

// Inline ask questionnaire actions (bound to the `AskPanel` context on the
// panel and `AskInput` on its text field).
actions!(
    ask_keys,
    [AskNext, AskPrev, AskConfirm, AskSubmit, AskClose]
);

fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("escape", AbortRun, None),
        // Composer keys only apply while the input is focused.
        KeyBinding::new("backspace", Backspace, Some("Composer")),
        KeyBinding::new("delete", Delete, Some("Composer")),
        KeyBinding::new("left", Left, Some("Composer")),
        KeyBinding::new("right", Right, Some("Composer")),
        KeyBinding::new("shift-left", SelectLeft, Some("Composer")),
        KeyBinding::new("shift-right", SelectRight, Some("Composer")),
        // Text-field conventions: the primary modifier (Cmd on macOS, Ctrl
        // elsewhere) + Left/Right jumps to the ends of the line (or the
        // wrapped row); Alt+Left/Right moves word by word.
        KeyBinding::new("secondary-left", LineLeft, Some("Composer")),
        KeyBinding::new("secondary-right", LineRight, Some("Composer")),
        KeyBinding::new("secondary-shift-left", SelectLineLeft, Some("Composer")),
        KeyBinding::new("secondary-shift-right", SelectLineRight, Some("Composer")),
        KeyBinding::new("alt-left", WordLeft, Some("Composer")),
        KeyBinding::new("alt-right", WordRight, Some("Composer")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("Composer")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("Composer")),
        KeyBinding::new("secondary-a", SelectAll, Some("Composer")),
        KeyBinding::new("home", Home, Some("Composer")),
        KeyBinding::new("end", End, Some("Composer")),
        KeyBinding::new("secondary-v", Paste, Some("Composer")),
        KeyBinding::new("secondary-c", Copy, Some("Composer")),
        KeyBinding::new("secondary-x", Cut, Some("Composer")),
        KeyBinding::new("enter", Submit, Some("Composer")),
        KeyBinding::new("secondary-enter", Submit, Some("Composer")),
        // Steer: inject the composer text into the running turn instead of
        // queuing a follow-up. With no run in flight it behaves like submit.
        KeyBinding::new("secondary-shift-enter", SteerRun, Some("Composer")),
        KeyBinding::new("alt-enter", SteerRun, Some("Composer")),
        // Tab accepts the highlighted `/`-command or `@`-file entry while
        // the autocomplete menu is open (Enter is the second way in).
        KeyBinding::new("tab", AutocompleteAccept, Some("Composer")),
        KeyBinding::new("shift-enter", Newline, Some("Composer")),
        KeyBinding::new("up", Up, Some("Composer")),
        KeyBinding::new("down", Down, Some("Composer")),
        KeyBinding::new("secondary-n", NewSession, None),
        KeyBinding::new("secondary-r", RefreshSessions, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        // The Usage page is a destination: the primary modifier + U matches
        // the sidebar row.
        KeyBinding::new("secondary-u", ToggleUsage, None),
        // Bottom terminal panel: the primary modifier + J is the workbench
        // convention for the panel toggle (and stays live while the shell has
        // focus, since app actions are not scoped to a key context).
        KeyBinding::new("secondary-j", ToggleTerminal, None),
        KeyBinding::new("secondary-p", ToggleCommandPalette, None),
        KeyBinding::new("secondary-period", AbortRun, None),
        // Transcript accelerators (work regardless of focus):
        // copy the newest response; jump between user turns like the rail.
        KeyBinding::new("secondary-shift-c", CopyLastResponse, None),
        KeyBinding::new("secondary-up", PrevTurn, None),
        KeyBinding::new("secondary-down", NextTurn, None),
        // Check for Updates (the app menu has no native home in Orbit yet).
        KeyBinding::new("secondary-shift-u", CheckForUpdates, None),
        // Model picker keys — the `Picker` context rides on the popup's
        // filter input, i.e. the *same* dispatch node as `Composer`, so these
        // bindings sit at the same depth as the composer ones. gpui breaks
        // depth ties by registration order, and these are registered later,
        // so enter/escape win over `Submit`/`AbortRun` while the popup is
        // open (arrows don't conflict — the input binds none).
        KeyBinding::new("escape", PickerCancel, Some("Picker")),
        KeyBinding::new("enter", PickerConfirm, Some("Picker")),
        KeyBinding::new("up", PickerSelectPrev, Some("Picker")),
        KeyBinding::new("down", PickerSelectNext, Some("Picker")),
        // Add-menu keys — the `AddMenu` context rides on the menu's own
        // focus handle (deeper than the global escape/enter bindings, so
        // these win while the menu is open).
        KeyBinding::new("escape", AddMenuClose, Some("AddMenu")),
        KeyBinding::new("enter", AddMenuConfirm, Some("AddMenu")),
        KeyBinding::new("up", AddMenuPrev, Some("AddMenu")),
        KeyBinding::new("down", AddMenuNext, Some("AddMenu")),
        // Access-mode picker keys — same shape as the add menu: the
        // `AccessMenu` context rides the popup's own focus handle.
        KeyBinding::new("escape", AccessMenuClose, Some("AccessMenu")),
        KeyBinding::new("enter", AccessMenuConfirm, Some("AccessMenu")),
        KeyBinding::new("up", AccessMenuPrev, Some("AccessMenu")),
        KeyBinding::new("down", AccessMenuNext, Some("AccessMenu")),
        // Inline approval bar keys. Registered after the composer bindings so
        // Enter confirms the highlighted action instead of submitting, and
        // Escape dismisses (denies) instead of aborting the run.
        KeyBinding::new("escape", ApprovalClose, Some("Approval")),
        KeyBinding::new("enter", ApprovalConfirm, Some("Approval")),
        KeyBinding::new("up", ApprovalPrev, Some("Approval")),
        KeyBinding::new("down", ApprovalNext, Some("Approval")),
        // Extension-dialog keys. `DialogSelect` rides the option card;
        // `DialogInput` rides the dialog's text field (which also carries
        // `Composer`, so caret/clipboard keys stay live). Registered after
        // the Composer bindings so Enter confirms instead of submitting and
        // Escape cancels the dialog instead of aborting the run.
        KeyBinding::new("escape", DialogCancel, Some("DialogSelect")),
        KeyBinding::new("escape", DialogCancel, Some("DialogInput")),
        KeyBinding::new("enter", DialogConfirm, Some("DialogSelect")),
        KeyBinding::new("enter", DialogConfirm, Some("DialogInput")),
        KeyBinding::new("up", DialogPrev, Some("DialogSelect")),
        KeyBinding::new("down", DialogNext, Some("DialogSelect")),
        // Inline ask questionnaire keys. Registered after the Composer
        // bindings so Enter answers instead of submitting the composer, and
        // Escape declines instead of aborting the run.
        KeyBinding::new("escape", AskClose, Some("AskPanel")),
        KeyBinding::new("escape", AskClose, Some("AskInput")),
        KeyBinding::new("enter", AskConfirm, Some("AskPanel")),
        KeyBinding::new("space", AskConfirm, Some("AskPanel")),
        KeyBinding::new("enter", AskSubmit, Some("AskInput")),
        KeyBinding::new("up", AskPrev, Some("AskPanel")),
        KeyBinding::new("down", AskNext, Some("AskPanel")),
        // In-transcript find (⌘F). The find field carries `Composer Search`,
        // so editing keys stay live; these bindings are registered after the
        // composer ones and win the same-depth tie, keeping Enter from
        // submitting the real composer while the bar is open.
        KeyBinding::new("secondary-f", ToggleSearch, None),
        KeyBinding::new("enter", SearchNext, Some("Search")),
        KeyBinding::new("shift-enter", SearchPrev, Some("Search")),
        KeyBinding::new("escape", SearchClose, Some("Search")),
        // Terminal grid. Escape must reach the shell (vim, less, `read`), but
        // the global `escape`-to-`AbortRun` binding is always enabled, so the
        // `Terminal` context has to re-bind it. Registered last: gpui breaks
        // an equal-depth tie by registration order, so this wins while the
        // grid owns focus.
        KeyBinding::new("escape", TerminalEscape, Some("Terminal")),
    ]);
}

/// The native macOS menu bar. GPUI dispatches these as ordinary actions
/// (`cx.dispatch_action`), validated against the focused window, so each item
/// is enabled only where its handler is live and shows the key equivalent
/// bound in [`bind_keys`].
///
/// GPUI's menu API only exposes app/action menus and the system `Services`
/// submenu — there is no selector for the standard Hide/Hide Others/Show All
/// or Window items — so this is deliberately the smallest set that matches the
/// real commands the app can perform.
fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Orbit".into(),
            items: vec![
                MenuItem::action("About Orbit Pi", OpenAbout),
                MenuItem::action("Check for Updates…", CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Orbit Pi", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Task", NewSession),
                MenuItem::action("Refresh Sessions", RefreshSessions),
            ],
        },
        // Editing keys ride the `Composer` context, so these enable while a
        // text field owns focus and grey out elsewhere.
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Cut", Cut),
                MenuItem::action("Copy", Copy),
                MenuItem::action("Paste", Paste),
                MenuItem::action("Select All", SelectAll),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Command Palette…", ToggleCommandPalette),
                MenuItem::action("Find in Transcript…", ToggleSearch),
                MenuItem::separator(),
                MenuItem::action("Toggle Terminal", ToggleTerminal),
                MenuItem::separator(),
                MenuItem::action("Usage", ToggleUsage),
            ],
        },
    ]
}

fn main() {
    // A hidden re-exec of this binary performs the Unix install swap after
    // the app quits; it must run before any GPUI setup.
    if let Some(code) = updater::run_install_helper() {
        std::process::exit(code);
    }

    let mut application = Application::new().with_assets(assets::Assets);
    // Remote author avatars need an HTTP client; without one GPUI renders
    // nothing for `img("https://…")` and the monogram fallback shows instead.
    if let Some(client) = http::avatar_client() {
        application = application.with_http_client(client);
    }
    application.run(|cx: &mut App| {
        // Rename the process before the menu bar is built so macOS labels the
        // application menu "Orbit" instead of the executable (`orbit-pi`).
        platform::set_process_name("Orbit");
        bind_keys(cx);
        // Install the native menu bar after the keymap exists so each item
        // picks up its key equivalent.
        cx.set_menus(app_menus());
        theme::init(cx);
        // Arm the background updater before the app reads its global. Debug
        // builds, a keyless build, and a bare `cargo run` binary all leave it
        // dormant. The launch check runs on its own thread.
        let updater = updater::Updater::init();
        cx.set_global(updater::UpdaterState(updater));
        // Bundle Zed's UI/mono faces plus the curated font catalog so every
        // picker choice resolves to a real face without OS dependencies.
        assets::register_zed_fonts(cx).expect("failed to register Zed fonts");
        assets::register_bundled_fonts(cx).expect("failed to register bundled fonts");
        app_icon::set_dock_icon();

        // Open maximized: full width of the screen, filling the visible
        // frame (Waku-style workbench). The computed bounds are the
        // restore size macOS returns to when the window is un-zoomed,
        // sized relative to the display so it always fits even on
        // small/scaled screens.
        let (w, h) = match cx.primary_display().map(|d| d.bounds().size) {
            Some(s) => (
                (f32::from(s.width) * 0.85).min(1440.),
                (f32::from(s.height) * 0.9).min(920.),
            ),
            None => (1240., 840.),
        };
        let restore_bounds = Bounds::centered(None, size(px(w), px(h)), cx);
        let _window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Maximized(restore_bounds)),
                    // Keep the window usable when shrunk: sidebar (min
                    // 200px) + a readable transcript + the composer.
                    window_min_size: Some(size(px(960.), px(640.))),
                    titlebar: Some(platform::titlebar_options()),
                    app_id: Some("dev.orbit.pi".into()),
                    focus: true,
                    ..Default::default()
                },
                |window, cx| {
                    let app: Entity<OrbitApp> = cx.new(OrbitApp::new);

                    // Focus the composer so typing works immediately; track
                    // window focus so background notifications know whether
                    // the user is already looking at the transcript.
                    app.update(cx, |app, cx| {
                        app.watch_window_activation(window, cx);
                        let handle = app.input.read(cx).focus_handle(cx);
                        window.focus(&handle);
                    });

                    // Heartbeat: drain pi events into the UI.
                    let heartbeat = app.clone();
                    window
                        .spawn(cx, async move |cx: &mut AsyncWindowContext| loop {
                            Timer::after(Duration::from_millis(90)).await;
                            heartbeat
                                .update(cx, |app: &mut OrbitApp, cx| app.tick(cx))
                                .ok();
                        })
                        .detach();

                    // Detect installed editors/terminals for the header
                    // "open in" control (off-thread; icons load once).
                    app.update(cx, |app, cx| app.detect_open_in_apps(cx));

                    app
                },
            )
            .unwrap();

        // If this launch replaced an older build, tell the waiting helper the
        // new window is up so it can commit the swap instead of rolling back.
        updater::signal_relaunch_ready();

        cx.activate(true);
        cx.on_action(|_: &Quit, cx| cx.quit());
    });
}
