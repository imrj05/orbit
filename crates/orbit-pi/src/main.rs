//! Orbit Pi — a native desktop workbench for the pi coding agent.
//!
//! Pure Rust on GPUI; the agent runtime is the `pi` CLI spoken to over its
//! JSONL RPC protocol (see `crates/orbit-rpc`). Quit with cmd-q.

// Windows release builds are GUI applications: this sets the PE subsystem to
// `WINDOWS`, so double-clicking `orbit-pi.exe` (or the installer launching it)
// never spawns a console window. Debug builds keep the console so `eprintln!`
// diagnostics stay visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Compile every `locales/<locale>.yml` into the binary and make `en` the
// fallback for any key a translation is still missing. Must precede the
// `tr!` macro and every `mod` below so child modules inherit the macros.
rust_i18n::i18n!("locales", fallback = "en");

/// Translate a key in the active locale, returning an owned `String`.
///
/// Prefer [`tr_cow!`] on hot render paths — a literal lookup borrows when the
/// active locale is the fallback and only allocates for a real translation.
macro_rules! tr {
    ($key:expr) => {
        $crate::i18n::translate($key)
    };
    ($key:expr, $($args:tt)*) => {
        rust_i18n::t!($key, $($args)*).into_owned()
    };
}

/// Borrowed translation for hot render paths (no interpolation). Kept for
/// call sites that can borrow; plain `tr!` allocates and is the default.
#[allow(unused_macros)]
macro_rules! tr_cow {
    ($key:literal) => {
        rust_i18n::t!($key)
    };
}

mod access;
mod ai_review;
mod app;
mod app_icon;
mod ask;
mod assets;
mod auth;
mod auto_title;
mod branch_picker;
mod bundled_extensions;
mod checkpoint;
mod command_palette;
mod commit_message;
mod composer;
mod context_meter;
mod custom_ui;
mod dialog;
mod dither;
mod explorer;
mod favorites;
mod gh;
mod git;
mod git_ops;
mod git_panel;
mod highlight;
mod http;
mod i18n;
mod layout;
mod mentions;
mod message_scroller;
mod model_selector;
mod model_selector_match;
mod notifications;
mod onboarding;
mod pi_update;
mod pins;
mod platform;
mod plugins;
mod providers;
mod quota;
mod review;
mod rpc_patches;
mod session_defaults;
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
mod widgets;
mod workflow;
mod workspace_logo;
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
        Undo,
        Redo,
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
        ToggleTerminal,
        ToggleProjectPanel,
        ToggleSidebar,
        FocusSessions,
        CloseFiles,
        CloseFileTab,
        SaveFile,
        GitTabChanges,
        GitTabHistory,
        GitTabGraph,
        GitTabIssues,
        GitTabPulls
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

// Custom-UI surface action (bound to the `CustomUi` context on the surface's
// focus handle) so Escape reaches the component instead of aborting the run.
actions!(custom_ui_keys, [CustomUiEscape]);

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

// Workflow-mode picker actions (bound to the `WorkflowMenu` context, which
// rides on the open popup's focus handle).
actions!(
    workflow_menu_keys,
    [
        WorkflowMenuNext,
        WorkflowMenuPrev,
        WorkflowMenuConfirm,
        WorkflowMenuClose
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

// Update-modal action (bound to the `UpdateDialog` context on the modal's
// focus handle) so Escape dismisses the modal instead of aborting the run.
actions!(update_dialog_keys, [UpdateDialogClose]);

// Explorer inline name-prompt actions (bound to the `ExplorerEntry` context on
// the prompt's text field, which also carries `Composer`). Registered after the
// Composer bindings so Enter confirms the name instead of submitting the
// composer and Escape cancels instead of aborting the run.
actions!(
    explorer_entry_keys,
    [ExplorerEntryConfirm, ExplorerEntryCancel]
);

// Sessions-sidebar keyboard navigation (bound to the `Sidebar` context, which
// rides the sidebar's focus handle while it is keyboard-focused). Arrow keys
// move the cursor, Enter activates the row, Escape hands focus back to the
// composer.
actions!(
    sidebar_keys,
    [
        SidebarPrev,
        SidebarNext,
        SidebarHome,
        SidebarEnd,
        SidebarConfirm,
        SidebarClose
    ]
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
        KeyBinding::new("cmd-z", Undo, Some("Composer")),
        KeyBinding::new("cmd-shift-z", Redo, Some("Composer")),
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
        // The Explorer's code editor reuses the composer's text actions but
        // keeps its own context: Enter inserts a newline (it must not submit a
        // chat message), and cmd-s writes the file.
        KeyBinding::new("backspace", Backspace, Some("Editor")),
        KeyBinding::new("delete", Delete, Some("Editor")),
        KeyBinding::new("left", Left, Some("Editor")),
        KeyBinding::new("right", Right, Some("Editor")),
        KeyBinding::new("shift-left", SelectLeft, Some("Editor")),
        KeyBinding::new("shift-right", SelectRight, Some("Editor")),
        KeyBinding::new("cmd-left", LineLeft, Some("Editor")),
        KeyBinding::new("cmd-right", LineRight, Some("Editor")),
        KeyBinding::new("cmd-shift-left", SelectLineLeft, Some("Editor")),
        KeyBinding::new("cmd-shift-right", SelectLineRight, Some("Editor")),
        KeyBinding::new("alt-left", WordLeft, Some("Editor")),
        KeyBinding::new("alt-right", WordRight, Some("Editor")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("Editor")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("Editor")),
        KeyBinding::new("cmd-a", SelectAll, Some("Editor")),
        KeyBinding::new("home", Home, Some("Editor")),
        KeyBinding::new("end", End, Some("Editor")),
        KeyBinding::new("cmd-v", Paste, Some("Editor")),
        KeyBinding::new("cmd-c", Copy, Some("Editor")),
        KeyBinding::new("cmd-x", Cut, Some("Editor")),
        KeyBinding::new("cmd-z", Undo, Some("Editor")),
        KeyBinding::new("cmd-shift-z", Redo, Some("Editor")),
        KeyBinding::new("up", Up, Some("Editor")),
        KeyBinding::new("down", Down, Some("Editor")),
        KeyBinding::new("enter", Newline, Some("Editor")),
        KeyBinding::new("shift-enter", Newline, Some("Editor")),
        KeyBinding::new("cmd-s", SaveFile, Some("Editor")),
        KeyBinding::new("secondary-n", NewSession, None),
        KeyBinding::new("secondary-r", RefreshSessions, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        // The Usage page is a destination: the primary modifier + U matches
        // the sidebar row.
        KeyBinding::new("secondary-u", ToggleUsage, None),
        // Git page tabs: cmd-1..cmd-5 switch tabs while the page is open; the
        // handler is a no-op elsewhere, so they never surprise a chat session.
        KeyBinding::new("cmd-1", GitTabChanges, None),
        KeyBinding::new("cmd-2", GitTabHistory, None),
        KeyBinding::new("cmd-3", GitTabGraph, None),
        KeyBinding::new("cmd-4", GitTabIssues, None),
        KeyBinding::new("cmd-5", GitTabPulls, None),
        // Bottom terminal panel: the primary modifier + J is the workbench
        // convention for the panel toggle (and stays live while the shell has
        // focus, since app actions are not scoped to a key context).
        KeyBinding::new("secondary-j", ToggleTerminal, None),
        // Sessions sidebar: the primary modifier + B is the workbench
        // convention for the left panel toggle.
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        // Left project panel (Explorer): the primary modifier + Shift + E,
        // the workbench convention (secondary resolves to Cmd on macOS and
        // Ctrl elsewhere, matching the label the UI shows).
        KeyBinding::new("secondary-shift-e", ToggleProjectPanel, None),
        // On the Files surface, cmd-w closes the active file tab (the whole
        // surface when it was the last tab); cmd-shift-w closes the surface.
        KeyBinding::new("cmd-w", CloseFileTab, Some("Files")),
        KeyBinding::new("cmd-shift-w", CloseFiles, Some("Files")),
        KeyBinding::new("secondary-p", ToggleCommandPalette, None),
        // The palette does quick-open work as well (sessions are its top
        // hits), so the Raycast convention opens the same surface.
        KeyBinding::new("secondary-k", ToggleCommandPalette, None),
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
        // Workflow-mode picker keys — same shape again.
        KeyBinding::new("escape", WorkflowMenuClose, Some("WorkflowMenu")),
        KeyBinding::new("enter", WorkflowMenuConfirm, Some("WorkflowMenu")),
        KeyBinding::new("up", WorkflowMenuPrev, Some("WorkflowMenu")),
        KeyBinding::new("down", WorkflowMenuNext, Some("WorkflowMenu")),
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
        // Update modal. Registered after the Composer bindings so Escape
        // dismisses the modal instead of aborting the run.
        KeyBinding::new("escape", UpdateDialogClose, Some("UpdateDialog")),
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
        // Custom-UI surface: Escape must reach the component (which cancels
        // itself), but the global `escape`-to-`AbortRun` binding is always
        // enabled, so the `CustomUi` context re-binds it and forwards ESC.
        KeyBinding::new("escape", CustomUiEscape, Some("CustomUi")),
        // Explorer inline name prompt. The field carries `Composer
        // ExplorerEntry`, so caret/clipboard keys stay live; these win the
        // same-depth tie against `Submit`/`AbortRun` (registered later).
        KeyBinding::new("enter", ExplorerEntryConfirm, Some("ExplorerEntry")),
        KeyBinding::new("escape", ExplorerEntryCancel, Some("ExplorerEntry")),
        // Sessions sidebar: focus it with the primary modifier + Shift + B,
        // then navigate the row list with the arrows (Zed/VS Code convention).
        // The `Sidebar` context rides the sidebar's own focus handle, so these
        // only win while it is focused and never collide with the composer's
        // caret keys. Escape is registered here (after the global abort) so it
        // returns focus to the composer instead of aborting the run.
        KeyBinding::new("secondary-shift-b", FocusSessions, None),
        KeyBinding::new("up", SidebarPrev, Some("Sidebar")),
        KeyBinding::new("down", SidebarNext, Some("Sidebar")),
        KeyBinding::new("home", SidebarHome, Some("Sidebar")),
        KeyBinding::new("end", SidebarEnd, Some("Sidebar")),
        KeyBinding::new("enter", SidebarConfirm, Some("Sidebar")),
        KeyBinding::new("escape", SidebarClose, Some("Sidebar")),
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
///
/// Rebuilt (see `set_app_menus`) whenever the interface language changes, so
/// the labels always read in the active locale.
pub(crate) fn app_menus() -> Vec<Menu> {
    let app = tr!("app.name");
    vec![
        Menu {
            name: app.clone().into(),
            items: vec![
                MenuItem::action(tr!("menu.about", app = app.clone()), OpenAbout),
                MenuItem::action(tr!("menu.check_for_updates"), CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action(tr!("menu.settings"), OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu(tr!("menu.services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(tr!("menu.quit", app = app.clone()), Quit),
            ],
        },
        Menu {
            name: tr!("menu.file").into(),
            items: vec![
                MenuItem::action(tr!("menu.new_task"), NewSession),
                MenuItem::action(tr!("menu.refresh_sessions"), RefreshSessions),
            ],
        },
        // Editing keys ride the `Composer` context, so these enable while a
        // text field owns focus and grey out elsewhere.
        Menu {
            name: tr!("menu.edit").into(),
            items: vec![
                MenuItem::action(tr!("menu.cut"), Cut),
                MenuItem::action(tr!("menu.copy"), Copy),
                MenuItem::action(tr!("menu.paste"), Paste),
                MenuItem::action(tr!("menu.select_all"), SelectAll),
            ],
        },
        Menu {
            name: tr!("menu.view").into(),
            items: vec![
                MenuItem::action(tr!("menu.command_palette"), ToggleCommandPalette),
                MenuItem::action(tr!("menu.find_in_transcript"), ToggleSearch),
                MenuItem::separator(),
                MenuItem::action(tr!("menu.toggle_sidebar"), ToggleSidebar),
                MenuItem::action(tr!("menu.toggle_terminal"), ToggleTerminal),
                MenuItem::action(tr!("explorer.toggle"), ToggleProjectPanel),
                MenuItem::separator(),
                MenuItem::action(tr!("menu.usage"), ToggleUsage),
            ],
        },
    ]
}

/// Install the native menu bar for the active locale. Called once at startup
/// and again whenever the interface language changes.
pub(crate) fn set_app_menus(cx: &mut App) {
    cx.set_menus(app_menus());
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
        theme::init(cx);
        // Adopt the persisted interface language before the first paint (and
        // before the menu bar below reads its labels).
        i18n::set_language(theme::get(cx).ui.language);
        // Install the native menu bar after the keymap exists so each item
        // picks up its key equivalent.
        set_app_menus(cx);
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
        // frame. The computed bounds are the
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
                    theme::watch_system_appearance(window, cx);
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
