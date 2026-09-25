//! The Orbit shell model: the `OrbitApp` struct (shared state), the
//! `Render`/`Focusable` entry points, and the feature modules that drive it.
//!
//! This file keeps the state model, the shared types, and the controller
//! wiring. Feature logic lives in descendant modules (see the map at the
//! bottom of the file), which can reach the struct's private fields directly:
//!
//! - [`runtime`] — pi process lifecycle, status/error, provider auth
//! - [`events`] — the heartbeat: event drain, responses, session watcher
//! - [`session`] — prompt/queue/turn lifecycle and navigation
//! - [`pickers`] — model, command-palette, branch, and workspace pickers
//! - [`pi_update_ui`] — the launch-time pi self-update: check, install, report
//! - [`composer_ops`] — autocomplete, attachments, add-menu, model chips
//! - [`sidebar`] — session/workspace sidebar rendering + row menus
//! - [`settings`] — Settings surface (General/Runtime/Agent/Skills/Plugins/Providers)
//! - [`skills_ui`] — Settings → Skills master-detail page + controllers
//! - [`toast_ui`] — the in-app toast stack: push/dismiss helpers and layer
//! - [`view`] — top-level chrome: sidebar, transcript, composer, status bar
//! - [`open_in`] — "open workspace in" app detection and menu
//! - [`helpers`] — icons, file glyphs, and small formatting helpers

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use base64::Engine as _;
use gpui::{
    anchored, deferred, div, img, linear_color_stop, linear_gradient, list, point, prelude::*, px,
    radians, relative, svg, uniform_list, AnchoredPositionMode, Animation, AnimationExt,
    AnyElement, App, ClipboardItem, Context, Corner, CursorStyle, DragMoveEvent, ElementId, Entity,
    ExternalPaths, FocusHandle, Focusable, FontWeight, Hsla, Image, ImageSource, IntoElement,
    ListAlignment, ListState, MouseButton, MouseDownEvent, MouseUpEvent, ObjectFit,
    PathPromptOptions, Pixels, Point, Render, Resource, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Subscription, TextAlign, Transformation, UniformListScrollHandle,
    Window,
};
use orbit_rpc::{
    CommandBody, ContextUsage, Event, PendingQueue, PiClient, QuotaReport, SessionState,
    SessionUsage,
};
use serde_json::Value;

use crate::access::AccessMode;
use crate::ai_review::{Report, ReviewKind, ReviewStatus};
use crate::ask::{AskPrompt, AskQuestion};
use crate::auth::{AuthEffect, AuthManager, AuthSupport, LoginPhase, ProviderStatus};
use crate::branch_picker::BranchPicker;
use crate::bundled_extensions::BundledExtensions;
use crate::checkpoint;
use crate::command_palette::{self, CommandPalette, PaletteCommand, PaletteSnapshot};
use crate::composer::ComposerInput;
use crate::context_meter::{self, ContextMeterData, ContextPopup};
use crate::custom_ui::{CustomCancel, CustomFrame, CustomInput, CustomUi, CustomUiSurfaces};
use crate::dialog::{ApprovalRequest, Dialog, DialogRequest, DialogResponse};
use crate::git_panel::GitPanel;
use crate::mentions::{self, AcEntry, SharedAutocomplete, SlashCommand, Trigger, TriggerKind};
use crate::model_selector::{
    provider_icon, thinking_display, thinking_icon, ModelSelector, PickerKind,
};
use crate::notifications;
use crate::onboarding::{self, Dependency};
use crate::platform::{self, ExternalApp};
use crate::plugins::{PackageScope, PluginPackage};
use crate::providers::{self, CustomProvider};
use crate::quota::{QuotaManager, QuotaSupport};
use crate::sessions::{self, SessionInfo};
use crate::sidepane::{SidePane, SidePaneResize};
use crate::skills::Skill;
use crate::terminal::{TerminalPanel, TerminalResize};
use crate::theme::{self, Theme, ThemeMode};
use crate::toast;
use crate::transcript::{self, Transcript};
use crate::usage::page::UsagePage;
use crate::watch;
use crate::widgets::{ExtensionWidget, WidgetPlacement};
use crate::workflow::WorkflowMode;
use crate::workspace_picker::{WorkspaceEntry, WorkspacePicker};

const SIDEBAR_DEFAULT_W: f32 = 248.;
const SIDEBAR_MIN_W: f32 = 200.;
/// Sessions shown under each workspace group before "Show more" appears.
const SIDEBAR_GROUP_SESSIONS_VISIBLE: usize = 10;
/// Room the side-pane resize keeps for the transcript column.
const PANE_MAX_RESERVE: f32 = 480.;

/// Drag marker for the sidebar resize handle (gpui typed drag state).
struct SidebarResize;

/// An invisible drag ghost — resizing leaves no floating preview.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
const CONTENT_MAX_W: f32 = 960.;

/// Rows shown in the `/`-command and `@`-file autocomplete menu.
const AUTOCOMPLETE_LIMIT: usize = 8;
/// Images that may ride along with one prompt.
const MAX_ATTACHMENTS: usize = 8;

/// How long a status message stays visible in the status bar.
const STATUS_MESSAGE_TTL: Duration = Duration::from_secs(6);

/// How often the app polls pi for new quota-bridge session entries. The
/// bridge appends only on change, so this is a cheap idempotent read.
const QUOTA_ENTRY_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Session start races the bridge's first fetch (it runs on `session_start`
/// too), so the first few polls use a short interval until a snapshot lands
/// — or the bootstrap budget runs out for an account with no providers.
const QUOTA_ENTRY_BOOTSTRAP_INTERVAL: Duration = Duration::from_secs(3);
const QUOTA_ENTRY_BOOTSTRAP_POLLS: u8 = 10;

/// Rows in the composer's "+" add menu (icon, label key, trailing hint).
const ADD_MENU_ITEMS: [(&str, &str, &str); 3] = [
    ("icons/image.svg", "composer.attach_image", ""),
    ("icons/file.svg", "composer.attach_file", ""),
    ("icons/at-sign.svg", "composer.mention_file", "@"),
];

/// Maximum sessions kept alive in the background. Beyond this, the
/// least-recently parked idle session is evicted (its process torn down);
/// running ones never are.
const MAX_LIVE_SESSIONS: usize = 6;

/// How long an idle parked session stays warm before its process is reaped.
/// Reusing a warm pi process makes re-opening a recent session instant
/// instead of paying Node startup again; the TTL bounds the memory cost.
const PARKED_IDLE_TTL: Duration = Duration::from_secs(300);

/// How long the session-details Update button shows its success check after a
/// rename commits, before reverting to the label.
const RENAME_FEEDBACK: Duration = Duration::from_millis(1400);

/// A session kept warm in the background: its own pi process, its own live
/// transcript, and its own agent-run state. Both running and idle sessions
/// are parked when the user switches away, so reopening is a resume (no
/// process spawn). Events keep draining every tick; a running session
/// resumes exactly where the stream left off.
struct ParkedSession {
    client: PiClient,
    transcript: Transcript,
    busy: bool,
    added: u64,
    removed: u64,
    /// Extension `setWidget` blocks live at park time, so switching back to a
    /// warm session restores them (a parked process never re-emits).
    widgets: Vec<ExtensionWidget>,
    /// When this session was last parked; drives idle TTL reaping.
    parked_at: Instant,
}

/// An in-flight automatic retry (`auto_retry_start` → `auto_retry_end`):
/// which attempt pi is waiting on, out of how many, and the transient
/// provider error that triggered it.
struct RetryDetail {
    attempt: u64,
    max: Option<u64>,
    error: String,
}

/// The status bar's branch chip: the checked-out branch and its divergence
/// from upstream. `ahead_behind` is `None` without an upstream — unknown is
/// not the same fact as `↑0 ↓0`.
#[derive(PartialEq)]
struct BranchStatus {
    name: String,
    ahead_behind: Option<(usize, usize)>,
    /// Local branches other than the current one (`None` when the list could
    /// not be read).
    other_branches: Option<usize>,
}

pub struct OrbitApp {
    client: Option<PiClient>,
    /// Live state of the active pi process, surfaced in Settings → Runtime.
    runtime: RuntimeStatus,
    /// Result of auto-applying pi's RPC patches before this launch, surfaced
    /// in Settings → Runtime. See [`crate::rpc_patches`].
    rpc_patches: crate::rpc_patches::PatchReport,
    /// Sessions with a live pi process, keyed by session-file path. The
    /// active session lives in `client`/`transcript` above; this map holds
    /// the background ones (see `ParkedSession`).
    /// Sidebar width in pixels — adjusted by dragging its right edge.
    sidebar_width: Pixels,
    lives: HashMap<PathBuf, ParkedSession>,
    transcript: Transcript,
    sessions: Vec<SessionInfo>,
    /// Debounced watcher over pi's session store — sessions written by the
    /// CLI or another Orbit window refresh the sidebar without `cmd-r`.
    session_watcher: Option<sessions::SessionWatcher>,
    /// Watches the active workspace so Review and the Git page refresh when
    /// files or git state change without an RPC event.
    workspace_watcher: Option<watch::WorkspaceWatcher>,
    /// The directory `workspace_watcher` is pointed at, so a workspace move
    /// re-points the watch (and a failed start is not retried every tick).
    workspace_watch_dir: Option<PathBuf>,
    sidebar_list: ListState,
    sidebar_visible: bool,
    /// Bumped on every sidebar toggle. The render keys its slide animation on
    /// it, so an open→close→open cycle animates every time (a boolean key
    /// would let the repeat skip) while the first frame — generation 0 — draws
    /// the settled state with no launch animation.
    sidebar_slide_gen: u64,
    /// Keyboard cursor for the sessions sidebar: an index into the current
    /// sidebar rows. `None` until the sidebar takes keyboard focus (⌘⇧B);
    /// the row it names paints the focused surface in `render_side_row`.
    sidebar_cursor: Option<usize>,
    /// Focus handle carrying the `Sidebar` key context while the sidebar is
    /// being keyboard-navigated (↑/↓/⏎/Esc).
    sidebar_focus: FocusHandle,
    pub(crate) input: Entity<ComposerInput>,
    model_label: String,
    /// Pi model id of the active model (stable match key for the picker).
    model_id: String,
    /// Provider id of the active model (drives the brand glyph on the chip).
    model_provider: String,
    thinking_label: String,
    busy: bool,
    /// Pending steering + follow-up messages reported by pi's `queue_update`
    /// (and echoed by `clear_queue`). While the agent runs, messages sent from
    /// the composer are queued as follow-ups and shown here until delivered.
    queue: PendingQueue,
    /// A `clear_queue` was sent as part of Escape; restore the returned text
    /// into the composer when the response arrives (docs' interactive-Esc).
    restore_queue_on_clear: bool,
    /// Settings → Agent: `set_follow_up_mode` value.
    follow_up_mode: String,
    /// Settings → Agent: `set_auto_compaction` value (read back from state).
    auto_compaction: bool,
    /// Settings → Agent: `set_auto_retry` value. pi's `get_state` does not
    /// expose this, so it reflects the last value Orbit sent.
    auto_retry: bool,
    /// Settings → Agent: auto session titles. Persisted to
    /// `~/.orbit-pi/auto-title.json`, which the bundled title extension reads.
    auto_title: crate::auto_title::AutoTitleConfig,
    /// Display name pi reports for the session (`get_state.sessionName`).
    session_name: Option<String>,
    /// pi is compacting right now (`get_state` / `compaction_*`).
    is_compacting: bool,
    /// pi is inside an automatic-retry delay (`auto_retry_*`). pi doesn't
    /// expose this in `get_state`, so it's event-driven.
    retrying: bool,
    /// Detail of the in-flight automatic retry (attempt counter + the
    /// provider's transient error), shown on the persistent run-status
    /// strip until the retry resolves.
    retry_detail: Option<RetryDetail>,
    /// The most recent command / protocol / extension error. Shown as a
    /// dismissible red banner until cleared — a failure is never dropped
    /// (docs: #error-handling).
    error: Option<String>,
    /// The in-app toast stack: action confirmations and notifications that
    /// land while the window is frontmost. Rendered bottom-right over every
    /// surface (see [`toast_ui`]).
    toasts: toast::Toasts,
    /// Text of the last optimistic follow-up. Cleared once pi confirms it in
    /// `queue_update`; restored to the composer if the command fails.
    pending_follow_up: Option<String>,
    /// Rename field in the session-details popover.
    session_name_input: Entity<ComposerInput>,
    status: String,
    /// When the current `status` message was set; the status bar shows it
    /// for [`STATUS_MESSAGE_TTL`] and then lets it lapse.
    status_at: Option<Instant>,
    current_title: Option<String>,
    current_workspace: Option<PathBuf>,
    /// Logo found at a conventional path in the current workspace, shown in
    /// the new-task page's folder field; `None` keeps the folder glyph.
    workspace_logo: Option<Arc<Image>>,
    /// Status-bar branch chip: the checked-out branch and its divergence from
    /// upstream, fetched off-thread so render never shells out to git.
    branch: Option<BranchStatus>,
    /// Generation of the newest branch fetch; a late result from an older
    /// fetch is discarded.
    branch_fetch: u64,
    /// Real line counts from edit/write tool calls this session.
    added: u64,
    removed: u64,
    focus: FocusHandle,
    /// Catalog of models reported by `get_available_models`.
    available_models: Vec<ModelEntry>,
    /// Thinking levels reported by `get_available_thinking_levels`.
    available_thinking_levels: Vec<String>,
    /// The open picker popup (model or thinking dropdown), if any. The
    /// kind travels with the entity; creating/dropping this *is* the
    /// open/closed state. Each popup is anchored above its own chip.
    model_selector: Option<(PickerKind, Entity<ModelSelector>)>,
    /// The window-wide command palette (⌘P / sidebar Search row), if open.
    command_palette: Option<Entity<CommandPalette>>,
    /// A blocking extension dialog (`extension_ui_request`), if one is open.
    /// pi holds the run until the answer is sent, so this is a modal surface.
    dialog: Option<Entity<Dialog>>,
    /// Focus the dialog (or its text field) on the next paint — `tick` has no
    /// window to focus with.
    dialog_focus_pending: bool,
    /// The open inline access-guard approval, if any: a compact bar above the
    /// composer rather than the modal above. pi holds the tool call until it
    /// is answered.
    approval: Option<ApprovalRequest>,
    /// Highlighted button in the approval bar (arrow keys + hover move it).
    approval_highlight: usize,
    /// Focus handle that carries the `Approval` key context while the bar is
    /// open (focus moves here so ↑/↓/Enter/Escape hit it).
    approval_focus: FocusHandle,
    /// Focus the approval bar on the next paint (`tick` has no window).
    approval_focus_pending: bool,
    /// The id of the running `ask_user_question` tool call, if any. Set on
    /// `tool_execution_start`, cleared on its `tool_execution_end`.
    ask_tool_id: Option<String>,
    /// The questionnaire the running ask tool was invoked with, parsed once
    /// so the panel can render structured questions/options without reading
    /// the request's flattened title back apart.
    ask_questions: Vec<AskQuestion>,
    /// The live inline questionnaire panel above the composer, answering the
    /// extension's `select` / `input` requests without a scrim modal.
    ask: Option<AskPrompt>,
    /// Set once the panel commits: the buffered answers replay to the
    /// extension one blocking request at a time, so incoming requests are
    /// answered here instead of opening a panel.
    ask_replay: Option<ask::AskReplay>,
    /// Focus handle carrying the `AskPanel` key context.
    ask_focus: FocusHandle,
    /// Focus the panel (or its text field) on the next paint.
    ask_focus_pending: bool,
    /// Extension `setWidget` text blocks (above/below the composer), keyed by
    /// the extension's `widgetKey`. Per-session, so it parks with the session.
    extension_widgets: Vec<ExtensionWidget>,
    /// Live `ctx.ui.custom()` surfaces, newest (last) owning focus; multiple
    /// may be open when a component nests.
    custom_ui: CustomUiSurfaces,
    /// Focus the top custom surface on the next paint — `tick` has no window.
    custom_ui_focus_pending: bool,
    /// Whether pi advertises the `custom` extension-UI capability in
    /// `get_state.capabilities`. Purely informational today: frames are
    /// handled whenever they arrive.
    custom_ui_supported: bool,
    /// Full-window image lightbox for a transcript attachment image. `None`
    /// is closed. Opened by clicking an image tile, dismissed by click or
    /// Escape.
    lightbox: Option<Arc<Image>>,
    /// Open in-transcript find (⌘F): a find field, the matching message
    /// indices, and the selected hit. `None` is closed.
    transcript_search: Option<search::TranscriptSearch>,
    /// Open row-actions menu in the sessions sidebar (which session's path
    /// plus whether the popup is showing the delete confirmation).
    session_menu: Option<SessionMenu>,
    /// Whether the settings surface replaces the main content area.
    settings_open: bool,
    /// Active section within the settings surface.
    settings_section: SettingsSection,
    /// Open dropdown on the settings surface (language / font sizes).
    settings_select: Option<SettingsSelect>,
    /// Filter text for the open settings dropdown.
    settings_filter: Entity<ComposerInput>,
    /// Keyboard cursor in the open settings dropdown: an original option
    /// index, so it survives filtering. `<Enter>` picks it; the list keeps
    /// it in view.
    settings_select_highlight: Option<usize>,
    /// Scroll handle for the settings dropdown's option list.
    settings_select_scroll: UniformListScrollHandle,
    /// Settings → General: which notification channels are on (persisted to
    /// `~/.orbit-pi/notifications.json`), plus the last macOS permission
    /// read. The read drives the honest "blocked" / "unbundled" rows — a
    /// switch the OS is blocking must not look like it is working.
    notification_prefs: notifications::Prefs,
    notification_auth: notifications::DesktopAuth,
    notification_auth_pending: bool,
    /// Whether this window is frontmost. Notifications are held while it is:
    /// the transcript itself is the notification then.
    window_active: bool,
    /// A banner click asked for the window; the next paint brings it forward
    /// (`tick` has no `Window` to activate with).
    activate_window_pending: bool,
    /// Keeps the window-activation observer alive (registered from `main`
    /// once the window exists).
    _window_activation: Option<Subscription>,
    /// Visited sessions, oldest first — drives the top-bar back/forward
    /// navigation. `history_index` points at the active entry.
    session_history: Vec<SessionInfo>,
    history_index: usize,
    /// Active workspace groups the user has explicitly collapsed.
    collapsed_workspaces: HashSet<String>,
    /// Non-active workspace groups the user has explicitly expanded.
    expanded_workspace_groups: HashSet<String>,
    /// Workspace groups whose session list is expanded past
    /// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`]. The value is how many extra
    /// sessions are revealed; each "Show more" click adds one step of
    /// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`], so a long history grows ten rows
    /// at a time instead of landing all at once.
    expanded_session_groups: HashMap<String, usize>,
    /// The projects Orbit lists in its sidebar — its own, user-curated folder
    /// list. pi owns the session files; this only records which folders the
    /// user added, persisted to `~/.orbit-pi/workspaces.json`. A workspace is
    /// added when the user picks it to work in; removing one drops only this
    /// entry and never touches pi.
    workspaces: Vec<PathBuf>,
    /// Open row-actions menu on a workspace group header (which workspace's
    /// label + cwd). Mutually exclusive with `session_menu`.
    workspace_menu: Option<WorkspaceMenu>,
    /// Path of the active session file, for the sidebar highlight.
    current_session_path: Option<PathBuf>,
    /// When the popup was dismissed by an outside mouse-down; guards against
    /// the same click's mouse-up immediately re-opening it via the chip.
    menu_dismissed_at: Option<Instant>,
    /// Live context-window usage from `get_session_stats`. `None` when pi
    /// hasn't advertised a window (no model) or the command isn't supported.
    context: Option<ContextUsage>,
    /// Cumulative session token/cost totals from the same `get_session_stats`
    /// response. Unlike `context`, this spans the whole session (all turns,
    /// tools, and compaction summaries), so the popup can show cost + cache.
    session_usage: Option<SessionUsage>,
    /// Hover compact card vs click-to-open breakdown for the context ring.
    context_popup: ContextPopup,
    /// Shared `/`-command and `@`-file menu state between the composer
    /// (which owns ↑/↓) and this app (which owns Enter/Escape + rendering).
    autocomplete: SharedAutocomplete,
    /// Dismissed via outside mouse-down; cleared when the trigger changes.
    autocomplete_dismissed: bool,
    /// Trigger (kind + query) the autocomplete highlight was synced against.
    last_ac_trigger: Option<(TriggerKind, String)>,
    /// Slash commands reported by pi (`get_commands` — extensions + skills).
    slash_commands: Vec<SlashCommand>,
    /// Workspace files for `@`-mentions (cached per workspace).
    mention_files: Vec<String>,
    mention_files_workspace: Option<PathBuf>,
    /// Images queued to ride along with the next prompt (pasted or picked).
    attachments: Vec<Attachment>,
    /// Files are being dragged over the composer (external OS drag).
    file_drag_hovered: bool,
    /// Installed folder-capable apps for the header's "open in" control.
    open_in_apps: Rc<Vec<ExternalApp>>,
    /// Whether the open-in app picker dropdown is open.
    open_in_menu_open: bool,
    /// Filter text for the open-in menu.
    open_in_filter: Entity<ComposerInput>,
    /// Whether the composer's "+" add menu is open.
    add_menu_open: bool,
    /// Highlighted row in the add menu (arrow keys + hover move it).
    add_menu_highlight: usize,
    /// Focus handle that carries the `AddMenu` key context while the menu
    /// is open (focus moves here so ↑/↓/Enter/Escape hit the menu, then
    /// returns to the composer).
    add_menu_focus: FocusHandle,
    /// Git branch picker anchored to the status-bar branch chip.
    branch_picker: Option<Entity<BranchPicker>>,
    /// Folder selector anchored under the new-task page's workspace field.
    workspace_picker: Option<Entity<WorkspacePicker>>,
    /// A checkout/create is running on the background executor.
    branch_operation_pending: bool,
    /// Persisted open-in choices, resolved against the active workspace.
    open_in_prefs: platform::OpenInPrefs,
    /// Serializes preference writes so rapid selections cannot save out of order.
    open_in_save_task: Option<gpui::Task<()>>,
    /// Keeps the theme global observer alive so a settings toggle redraws.
    _theme_sub: Subscription,
    /// Keeps the composer observer alive: edits re-render the app so the
    /// send button's quiet/ready state tracks the text live.
    _input_sub: Subscription,
    /// Keeps the transcript `cmd-c` interceptor alive: a live transcript
    /// selection wins over the focused composer's own copy, but only while
    /// one exists (the composer keeps its copy otherwise).
    _copy_selection_sub: Subscription,
    /// Onboarding dependency check results (pi, node, git).
    deps: Vec<Dependency>,
    /// Whether the setup page is open on request (Settings → About →
    /// Requirements) rather than because something is missing.
    setup_open: bool,
    /// The machine this build is running on. Probed at startup and on the
    /// setup page's Refresh (the OS probe runs a command, so it never happens
    /// on the render path); the setup page and Settings → About read it.
    host: platform::Host,
    /// Whether the setup page's Refresh check is in flight (spins the button).
    refreshing: bool,
    /// Whether the top-bar session-details popover is open.
    session_details_open: bool,
    /// A popover-triggered title generation is in flight; the next
    /// `session_info_changed` seeds the rename field from its result.
    title_generating: bool,
    /// When the last successful session rename committed, so the popover's
    /// Update button can flash its check before reverting to the label. The
    /// stamp lets a second click extend the flash instead of clearing early.
    rename_saved_at: Option<Instant>,
    /// Whether the top-bar provider-quota popover is open.
    quota_popup_open: bool,
    /// A manual quota refresh is in flight: the popover's refresh button spins
    /// until the `quota.list` reply lands, or a short timeout clears it.
    quota_refreshing: bool,
    /// When the in-flight manual refresh started, so its spin is kept visible
    /// for a minimum duration even if the reply is immediate.
    quota_refresh_started: Option<Instant>,
    /// The `sessionId` pi reports for the active session (its task id).
    session_id: Option<String>,
    /// Current agent turn number for this session (0 = none yet).
    turn_count: usize,
    /// A turn's start checkpoint was captured and awaits its end.
    turn_open: bool,
    /// Latest turn with a captured end checkpoint (drives Review's Last Turn).
    latest_turn: Option<usize>,
    /// Right side pane — Review (git diff).
    sidepane: Entity<SidePane>,
    /// The dedicated AI reviewer process, when one is running. Kept out of
    /// `lives` — it is not a user session and must never appear in the sidebar.
    ai_review: Option<ai_review::ReviewAgent>,
    /// What the running/last reviewer was asked to inspect.
    ai_review_kind: Option<ReviewKind>,
    /// The reviewer's lifecycle, mirrored into the Review pane each frame.
    ai_review_status: ReviewStatus,
    /// The parsed findings (and prose) of the last completed review.
    ai_report: Option<Report>,
    /// Monotonic id guarding against a superseded review's async diff
    /// collection launching a process after the user started or cancelled one.
    ai_review_generation: u64,
    /// Right dock — the workspace file tree (cmd-shift-e).
    project_panel: Entity<crate::explorer::ProjectPanel>,
    /// Full-page read-only file viewer (the Files surface).
    file_viewer: Entity<crate::explorer::FileViewer>,
    /// Bottom panel — an integrated shell (cmd-j).
    terminal_panel: Entity<TerminalPanel>,
    /// Whether the Git page replaces the chat area.
    git_open: bool,
    /// The full-page Git panel (tabs + commit bar).
    git_panel: Entity<GitPanel>,
    /// Whether the Usage page replaces the chat area.
    usage_open: bool,
    /// The Usage page: analytics over pi's own session store.
    usage: Entity<UsagePage>,
    /// Custom providers read from `~/.pi/agent/models.json` (cached; reloaded
    /// when the Providers page opens, on Refresh, and after a save/remove).
    custom_providers: Vec<CustomProvider>,
    /// Parse/read error from models.json, surfaced on the Providers page
    /// instead of silently overwriting a hand-edited file.
    custom_providers_error: Option<String>,
    /// Credentials read from `~/.pi/agent/auth.json` (cached alongside the
    /// custom providers).
    provider_auth: HashMap<String, providers::ProviderAuth>,
    /// Parse/read error from auth.json.
    provider_auth_error: Option<String>,
    /// Non-sensitive provider-authentication state driven by the `auth.*`
    /// RPC namespace. When pi doesn't advertise those commands this stays
    /// unsupported and the page keeps the Terminal login fallback.
    auth: AuthManager,
    /// Non-secret account quota/balance/spend per connected provider, from the
    /// `quota.*` RPC namespace and the bundled bridge extension's session
    /// entries. Empty when nothing has been reported yet.
    quota: QuotaManager,
    /// The bundled pi extensions (quota bridge + access guard), spawned with
    /// `--extension` on every session process.
    extensions: BundledExtensions,
    /// The active access mode. Persisted to `~/.orbit-pi/access.json`, which
    /// the guard extension reads fresh on every tool call.
    access_mode: AccessMode,
    /// Whether the composer's access-mode picker popover is open.
    access_menu_open: bool,
    /// Highlighted row in the access-mode picker (arrow keys + hover move it).
    access_menu_highlight: usize,
    /// Focus handle that carries the `AccessMenu` key context while the picker
    /// is open (focus moves here so ↑/↓/Enter/Escape hit it).
    access_menu_focus: FocusHandle,
    /// The active session's workflow mode (Plan / Build / Ask). Persisted per
    /// session to `~/.orbit-pi/workflow.json`, which the workflow extension
    /// reads fresh on every agent hook.
    workflow_mode: WorkflowMode,
    /// A mode chosen on the New Task page, before a session id exists to key
    /// it to.
    workflow_pending: Option<WorkflowMode>,
    /// Whether the composer's workflow-mode picker popover is open.
    workflow_menu_open: bool,
    /// Highlighted row in the workflow-mode picker.
    workflow_menu_highlight: usize,
    /// Focus handle that carries the `WorkflowMenu` key context while open.
    workflow_menu_focus: FocusHandle,
    /// `get_entries` cursor for the bridge's quota snapshots. Entry ids are
    /// per-session, so this resets when the active session changes.
    quota_entries_cursor: Option<String>,
    /// One bridge poll in flight at a time.
    quota_entries_inflight: bool,
    /// Remaining fast (bootstrap) polls for a fresh session; 0 = steady state.
    quota_entries_bootstrap: u8,
    /// When the next bridge poll is due; throttles the 90 ms heartbeat.
    quota_entries_next_poll: Instant,
    /// True once a credential changed and pi needs a restart to load it
    /// (pi reads auth.json only at startup). Only used on the file-based
    /// fallback path; RPC logins take effect live.
    provider_auth_dirty: bool,
    /// Built-in catalog size per provider (from pi's bundled model data),
    /// loaded off-thread so unconfigured providers show a real count.
    provider_catalog_counts: HashMap<String, usize>,
    /// Authoritative provider metadata introspected from pi-ai (empty when
    /// unavailable — then the curated table is used instead).
    provider_metadata: Vec<providers::DynamicProvider>,
    /// The metadata load was kicked off (success or not) — avoids re-spawning
    /// on every reload.
    provider_metadata_loaded: bool,
    /// The open API-key editor, if any.
    provider_key_editor: Option<ProviderKeyEditor>,
    /// The open provider editor (add or edit), if any.
    provider_editor: Option<ProviderEditor>,
    /// Provider id awaiting inline remove confirmation.
    provider_remove_confirm: Option<String>,
    /// Provider id whose usage/quota popup is open.
    provider_usage_open: Option<String>,
    /// Refresh button spin state on the Providers page.
    providers_refreshing: bool,
    /// Search filter for the provider grid.
    provider_filter: Entity<ComposerInput>,
    /// Re-render the grid as the filter is typed.
    _provider_filter_sub: Subscription,
    /// Search filter for the Models page.
    models_filter: Entity<ComposerInput>,
    /// Re-render the model list as the filter is typed.
    _models_filter_sub: Subscription,
    /// Models page: show favorited models only (the second filter next to
    /// the search field).
    models_favorites_only: bool,
    /// Skills discovered for the current workspace (project + user scope).
    skills: Vec<Skill>,
    /// Installed pi packages (plugins) from user + project settings.
    plugins: Vec<PluginPackage>,
    /// A malformed settings file surfaced on the Plugins page.
    plugins_error: Option<String>,
    /// Source field for installing a new plugin.
    plugin_source_input: Entity<ComposerInput>,
    /// Install the next plugin into project scope instead of user scope.
    plugin_install_project: bool,
    /// Description of the plugin operation in flight, if any.
    plugin_action: Option<String>,
    /// Manual-refresh feedback: the toolbar button turns until this instant,
    /// so an instant reload still acknowledges the click.
    plugin_refresh_spin_until: Option<Instant>,
    /// Plugin source awaiting inline remove confirmation.
    plugin_remove_confirm: Option<String>,
    /// Re-render the toolbar as the install field is typed.
    _plugin_source_sub: Subscription,
    /// Search filter for the installed-plugin list.
    plugins_filter: Entity<ComposerInput>,
    /// Re-render the plugin list as the filter is typed.
    _plugins_filter_sub: Subscription,
    /// Settings → Skills: filter field for the skill list.
    skills_filter: Entity<ComposerInput>,
    /// The skill selected in the master-detail page (its `SKILL.md` path).
    selected_skill: Option<PathBuf>,
    /// Cached `SKILL.md` body for the selected skill.
    skill_content: Option<String>,
    /// A skill directory awaiting delete confirmation.
    skill_delete_confirm: Option<PathBuf>,
    /// Re-render the skill list as the filter is typed.
    _skills_filter_sub: Subscription,
    /// Background updater status mirrored from the global: `Idle` (nothing to
    /// show), `Available` (the footer and settings offer the update),
    /// `Updating` (spinning until the app quits to install). The payload
    /// details stay on the worker; only the release version rides along.
    updater_status: crate::updater::UpdateStatus,
    /// The staged release's version (e.g. `0.0.3`), mirrored beside
    /// `updater_status` so the settings buttons can name the download.
    updater_version: Option<String>,
    /// The staged release's notes from the feed, mirrored for the update
    /// modal's changelog.
    updater_notes: Option<String>,
    /// The feed's releases, newest first, mirrored for the modal's Version
    /// History. Empty until the first check answers.
    updater_history: Vec<crate::updater::Release>,
    /// Whether the modal is showing Version History instead of the state's
    /// body. The dialog underneath is preserved, so Back returns to it.
    updater_history_open: bool,
    /// The open update modal, if any. `None` leaves the download control and
    /// the Check for Updates command running against `updater_status` alone.
    updater_dialog: Option<UpdateDialog>,
    /// Focus handle that carries the `UpdateDialog` key context while the
    /// modal is open, so Escape dismisses it instead of aborting the run.
    updater_dialog_focus: FocusHandle,
    /// Focus the update modal on the next paint (`tick` has no window).
    updater_dialog_focus_pending: bool,
    /// True while the pointer is over the sidebar updater pill, which expands
    /// it from the download icon into the "Update" label (the reference app's
    /// pattern).
    updater_button_hovered: bool,
    /// In-flight pill width and label cross-fade. The animation closure writes
    /// them, so a reversal mid-flight starts from the last painted frame
    /// instead of snapping back to the collapsed width.
    updater_button_width: Rc<Cell<f32>>,
    updater_button_label_reveal: Rc<Cell<f32>>,
    /// Width/reveal the current pill animation started from, plus a generation
    /// that keys `with_animation` so each hover change restarts it.
    updater_button_animation_from_width: f32,
    updater_button_animation_from_reveal: f32,
    updater_button_animation_generation: u64,
    /// Mirror of the persisted automatic-check preference, refreshed when the
    /// updater reports and on toggle, so frames never read the file.
    automatic_updates_enabled: bool,
}

/// An image queued to ride along with the next prompt.
struct Attachment {
    name: String,
    mime: String,
    /// base64 payload (no `data:` prefix) — pi's prompt image shape.
    data: String,
    /// Decoded image for the chip thumbnail.
    preview: Option<Arc<gpui::Image>>,
}

impl Attachment {
    fn from_image(image: &gpui::Image, index: usize) -> Self {
        let ext = match image.format {
            gpui::ImageFormat::Png => "png",
            gpui::ImageFormat::Jpeg => "jpg",
            gpui::ImageFormat::Webp => "webp",
            gpui::ImageFormat::Gif => "gif",
            gpui::ImageFormat::Bmp => "bmp",
            gpui::ImageFormat::Svg => "svg",
            gpui::ImageFormat::Tiff => "tiff",
        };
        Self {
            name: tr!("app.pasted_image", index = index + 1, ext = ext),
            mime: image.format.mime_type().to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(&image.bytes),
            preview: Some(Arc::new(image.clone())),
        }
    }

    /// Images only — anything else returns `None` so a drop can fall back to
    /// referencing the file by path instead.
    fn from_path(path: &Path) -> Option<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)?;
        let (mime, format) = match ext.as_str() {
            "png" => ("image/png", gpui::ImageFormat::Png),
            "jpg" | "jpeg" => ("image/jpeg", gpui::ImageFormat::Jpeg),
            "webp" => ("image/webp", gpui::ImageFormat::Webp),
            "gif" => ("image/gif", gpui::ImageFormat::Gif),
            "bmp" => ("image/bmp", gpui::ImageFormat::Bmp),
            _ => return None,
        };
        let bytes = std::fs::read(path).ok()?;
        let preview = Arc::new(gpui::Image::from_bytes(format, bytes.clone()));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());
        Some(Self {
            name,
            mime: mime.into(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            preview: Some(preview),
        })
    }

    /// The wire shape pi's `prompt.images` expects.
    fn to_prompt_image(&self) -> Value {
        serde_json::json!({ "type": "image", "data": self.data, "mimeType": self.mime })
    }
}

/// A model choice from the pi runtime catalog.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModelEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    /// Provider-reported context window in tokens, when the catalog exposes it.
    pub(crate) context_window: Option<u64>,
}

impl OrbitApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let autocomplete: SharedAutocomplete = Rc::new(std::cell::RefCell::new(
            mentions::AutocompleteState::default(),
        ));
        let input = cx.new(|cx| ComposerInput::new(cx).with_autocomplete(autocomplete.clone()));
        // Filter fields for the settings dropdowns and the open-in menu.
        // `Composer Picker` keeps backspace/delete working while Enter/arrows
        // dispatch to the (unhandled) Picker actions rather than submitting.
        let settings_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("app.filter")
                .with_key_context("Composer Picker")
        });
        let open_in_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("app.filter")
                .with_key_context("Composer Picker")
        });
        let provider_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-filter")
                .with_placeholder_key("app.search_providers")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let provider_filter_sub = cx.observe(&provider_filter, |_, _, cx| cx.notify());
        let models_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("models-filter")
                .with_placeholder_key("app.search_models")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let models_filter_sub = cx.observe(&models_filter, |_, _, cx| cx.notify());

        // Settings → Plugins: the install-source field. `Composer Picker`
        // keeps editing keys live; Enter is unhandled, so the Install button
        // is the only commit path.
        let plugin_source_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("plugin-source-input")
                .with_placeholder_key("app.plugin_source_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let plugin_source_sub = cx.observe(&plugin_source_input, |_, _, cx| cx.notify());
        let plugins_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("plugins-filter")
                .with_placeholder_key("app.search_installed_plugins")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let plugins_filter_sub = cx.observe(&plugins_filter, |_, _, cx| cx.notify());

        // Settings → Skills: the list filter. `Composer Picker` keeps editing
        // keys live so typing filters the master list.
        let skills_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("skills-filter")
                .with_placeholder_key("app.search_skills")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let skills_filter_sub = cx.observe(&skills_filter, |_, _, cx| cx.notify());

        // Session-details popover: rename field. Default `Composer` key context
        // keeps real text editing (selection, clipboard, arrows); Enter routes
        // to Submit, which commits the name instead of the composer while this
        // field holds focus (see `on_submit`).
        let session_name_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("session-name-input")
                .with_placeholder_key("app.session_name")
                .with_max_lines(1)
                .with_wrap(false)
        });

        // Spawn pi rooted at the folder the user last worked in. Launched
        // from Finder the process cwd is `/`, so the store is the real
        // default; a deleted folder falls back to cwd. Sessions live in the
        // real ~/.pi/agent/sessions so they are shared with the CLI.
        let workspace = load_last_workspace()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let workspace_logo = crate::workspace_logo::load(&workspace);
        let extensions = BundledExtensions::install();
        // Teach the installed pi's RPC mode the capabilities Orbit uses
        // (custom UI, quota, auth) before spawning it. Best-effort and cached
        // across launches; see `rpc_patches`.
        let rpc_patches = crate::rpc_patches::apply_on_launch();
        // Load the access mode and write it back so the guard extension finds
        // the file on the very first tool call of the session.
        let access_mode = AccessMode::load();
        access_mode.persist();
        let (client, connect_error) = match extensions.spawn(&workspace) {
            Ok(client) => (Some(client), String::new()),
            Err(err) => (None, tr!("runtime.pi_spawn_failed", error = err)),
        };
        let runtime = RuntimeStatus {
            started_at: client.as_ref().map(|_| Instant::now()),
            alive: client.is_some(),
            exited: false,
            error: (!connect_error.is_empty()).then(|| connect_error.clone()),
        };

        let theme_sub = cx.observe_global::<Theme>(|this, cx| {
            this.input.update(cx, |_, cx| cx.notify());
            if let Some((_, selector)) = &this.model_selector {
                selector.update(cx, |_, cx| cx.notify());
            }
            cx.notify();
        });

        // Composer edits notify only the input entity; re-render the app so
        // the send button's quiet/ready state tracks the text as you type.
        let input_sub = cx.observe(&input, |_, _, cx| cx.notify());

        // `cmd-c` with a live transcript selection copies that selection even
        // while the composer holds focus — an interceptor is the only hook
        // that runs before focus-path action dispatch, so the composer keeps
        // its own copy whenever the transcript has nothing selected.
        let copy_selection_sub = {
            let app = cx.entity().downgrade();
            cx.intercept_keystrokes(move |event, _window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.key != "c"
                    || !keystroke.modifiers.platform
                    || keystroke.modifiers.shift
                    || keystroke.modifiers.alt
                    || keystroke.modifiers.control
                {
                    return;
                }
                let _ = app.update(cx, |app, cx| {
                    let Some(text) = app.transcript.selected_text() else {
                        return;
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    cx.stop_propagation();
                });
            })
        };

        // Probe the runtime pieces we need (pi, node, git) so the setup page
        // can show install commands when something is missing.
        let deps = onboarding::check_dependencies();
        let host = platform::host();

        // Right side pane: Review (git diff).
        let sidepane = cx.new(SidePane::new);
        // Bottom panel: an integrated shell.
        let terminal_panel = cx.new(TerminalPanel::new);
        // Full-page Git panel (Changes / History / Graph).
        let git_panel = cx.new(GitPanel::new);
        // Usage analytics over pi's own session store.
        let usage = cx.new(UsagePage::new);
        // Right dock — the workspace file tree. A row click routes to the app,
        // which opens the Files surface; the panel stays viewer-agnostic.
        let app_weak = cx.entity().downgrade();
        let project_panel = cx.new(|cx| {
            crate::explorer::ProjectPanel::new(
                Rc::new({
                    let app_weak = app_weak.clone();
                    move |path, display, cx: &mut App| {
                        let _ = app_weak.update(cx, |app, cx| {
                            app.open_file_in_viewer(path, display, cx)
                        });
                    }
                }),
                Rc::new({
                    let app_weak = app_weak.clone();
                    move |request, cx: &mut App| {
                        let _ = app_weak
                            .update(cx, |app, cx| app.on_file_op(request, cx));
                    }
                }),
                Rc::new(move |cx: &mut App| {
                    // The panel closes itself inside its own listener (it already
                    // holds that entity's lease), so this only repaints the
                    // shell. Calling back into `project_panel.update` here would
                    // double-lease the entity and abort.
                    let _ = app_weak.update(cx, |_app, cx| cx.notify());
                }),
                cx,
            )
        });
        // Full-page read-only file viewer.
        let viewer_weak = cx.entity().downgrade();
        let file_viewer = cx.new(|cx| {
            crate::explorer::FileViewer::new(
                Rc::new(move |cx: &mut App| {
                    // The viewer hides itself inside its own listener (it already
                    // holds that entity's lease), so this only repaints the
                    // shell. Calling back into `file_viewer.update` here would
                    // double-lease the entity and abort.
                    let _ = viewer_weak.update(cx, |_app, cx| cx.notify());
                }),
                cx,
            )
        });

        let mut app = Self {
            client,
            runtime,
            rpc_patches,
            sidebar_width: px(crate::layout::sidebar_width()
                .unwrap_or(SIDEBAR_DEFAULT_W)
                .max(SIDEBAR_MIN_W)),
            lives: HashMap::new(),
            transcript: Transcript::new(),
            sessions: sessions::load_sessions(),
            session_watcher: sessions::SessionWatcher::start(),
            workspace_watcher: None,
            workspace_watch_dir: None,
            sidebar_list: ListState::new(0, ListAlignment::Top, px(44.)),
            sidebar_visible: true,
            sidebar_slide_gen: 0,
            sidebar_cursor: None,
            sidebar_focus: cx.focus_handle(),
            input,
            model_label: "…".into(),
            model_id: String::new(),
            model_provider: String::new(),
            thinking_label: "…".into(),
            busy: false,
            queue: PendingQueue::default(),
            restore_queue_on_clear: false,
            follow_up_mode: "one-at-a-time".into(),
            auto_compaction: true,
            auto_retry: true,
            auto_title: crate::auto_title::AutoTitleConfig::load(),
            session_name: None,
            is_compacting: false,
            retrying: false,
            retry_detail: None,
            error: None,
            toasts: toast::Toasts::new(),
            pending_follow_up: None,
            session_name_input,
            status: connect_error.clone(),
            status_at: (!connect_error.is_empty()).then(Instant::now),
            current_title: None,
            current_workspace: Some(workspace),
            workspace_logo,
            branch: None,
            branch_fetch: 0,
            added: 0,
            removed: 0,
            focus: cx.focus_handle(),
            available_models: Vec::new(),
            available_thinking_levels: Vec::new(),
            model_selector: None,
            command_palette: None,
            dialog: None,
            dialog_focus_pending: false,
            approval: None,
            approval_highlight: 0,
            approval_focus: cx.focus_handle(),
            approval_focus_pending: false,
            ask_tool_id: None,
            ask_questions: Vec::new(),
            ask: None,
            ask_replay: None,
            ask_focus: cx.focus_handle(),
            ask_focus_pending: false,
            extension_widgets: Vec::new(),
            custom_ui: CustomUiSurfaces::default(),
            custom_ui_focus_pending: false,
            custom_ui_supported: false,
            lightbox: None,
            transcript_search: None,
            session_menu: None,
            settings_open: false,
            settings_section: SettingsSection::General,
            settings_select: None,
            settings_filter,
            settings_select_highlight: None,
            settings_select_scroll: UniformListScrollHandle::new(),
            notification_prefs: notifications::Prefs::load(),
            notification_auth: notifications::DesktopAuth::Unknown,
            notification_auth_pending: false,
            window_active: true,
            activate_window_pending: false,
            _window_activation: None,
            session_history: Vec::new(),
            history_index: 0,
            collapsed_workspaces: HashSet::new(),
            expanded_workspace_groups: HashSet::new(),
            expanded_session_groups: HashMap::new(),
            workspaces: load_workspaces(),
            workspace_menu: None,
            current_session_path: None,
            menu_dismissed_at: None,
            context: None,
            session_usage: None,
            context_popup: ContextPopup::None,
            autocomplete,
            autocomplete_dismissed: false,
            last_ac_trigger: None,
            slash_commands: Vec::new(),
            mention_files: Vec::new(),
            mention_files_workspace: None,
            attachments: Vec::new(),
            file_drag_hovered: false,
            open_in_apps: Rc::new(Vec::new()),
            open_in_menu_open: false,
            open_in_filter,
            add_menu_open: false,
            add_menu_highlight: 0,
            add_menu_focus: cx.focus_handle(),
            branch_picker: None,
            workspace_picker: None,
            branch_operation_pending: false,
            open_in_prefs: platform::OpenInPrefs::load(),
            open_in_save_task: None,
            _theme_sub: theme_sub,
            _input_sub: input_sub,
            _copy_selection_sub: copy_selection_sub,
            deps,
            setup_open: false,
            host,
            refreshing: false,
            session_details_open: false,
            title_generating: false,
            rename_saved_at: None,
            quota_popup_open: false,
            quota_refreshing: false,
            quota_refresh_started: None,
            session_id: None,
            turn_count: 0,
            turn_open: false,
            latest_turn: None,
            sidepane,
            ai_review: None,
            ai_review_kind: None,
            ai_review_status: ReviewStatus::default(),
            ai_report: None,
            ai_review_generation: 0,
            project_panel,
            file_viewer,
            terminal_panel,
            git_open: false,
            git_panel: git_panel.clone(),
            usage_open: false,
            usage: usage.clone(),
            custom_providers: Vec::new(),
            custom_providers_error: None,
            provider_auth: HashMap::new(),
            provider_auth_error: None,
            auth: AuthManager::new(),
            quota: QuotaManager::new(),
            extensions,
            access_mode,
            access_menu_open: false,
            access_menu_highlight: 0,
            access_menu_focus: cx.focus_handle(),
            workflow_mode: WorkflowMode::default(),
            workflow_pending: None,
            workflow_menu_open: false,
            workflow_menu_highlight: 0,
            workflow_menu_focus: cx.focus_handle(),
            quota_entries_cursor: None,
            quota_entries_inflight: false,
            quota_entries_bootstrap: QUOTA_ENTRY_BOOTSTRAP_POLLS,
            quota_entries_next_poll: Instant::now(),
            provider_auth_dirty: false,
            provider_catalog_counts: HashMap::new(),
            provider_metadata: Vec::new(),
            provider_metadata_loaded: false,
            provider_key_editor: None,
            provider_editor: None,
            provider_remove_confirm: None,
            provider_usage_open: None,
            providers_refreshing: false,
            provider_filter: provider_filter.clone(),
            _provider_filter_sub: provider_filter_sub,
            models_filter: models_filter.clone(),
            _models_filter_sub: models_filter_sub,
            models_favorites_only: false,
            skills: Vec::new(),
            plugins: Vec::new(),
            plugins_error: None,
            plugin_source_input: plugin_source_input.clone(),
            plugin_install_project: false,
            plugin_action: None,
            plugin_refresh_spin_until: None,
            plugin_remove_confirm: None,
            _plugin_source_sub: plugin_source_sub,
            plugins_filter: plugins_filter.clone(),
            _plugins_filter_sub: plugins_filter_sub,
            skills_filter: skills_filter.clone(),
            selected_skill: None,
            skill_content: None,
            skill_delete_confirm: None,
            _skills_filter_sub: skills_filter_sub,
            updater_status: cx
                .try_global::<crate::updater::UpdaterState>()
                .and_then(|state| state.0.as_ref())
                .map(|updater| updater.status())
                .unwrap_or_default(),
            updater_version: cx
                .try_global::<crate::updater::UpdaterState>()
                .and_then(|state| state.0.as_ref())
                .and_then(|updater| updater.available_version()),
            updater_notes: cx
                .try_global::<crate::updater::UpdaterState>()
                .and_then(|state| state.0.as_ref())
                .and_then(|updater| updater.available_notes()),
            updater_history: cx
                .try_global::<crate::updater::UpdaterState>()
                .and_then(|state| state.0.as_ref())
                .map(|updater| updater.history())
                .unwrap_or_default(),
            updater_history_open: false,
            updater_dialog: std::env::var_os("ORBIT_OPEN_UPDATE_DIALOG")
                .is_some_and(|value| value == "1")
                .then(|| UpdateDialog::Available {
                    version: "0.0.11".into(),
                    notes: Some(
                        "### Contributors\n\n- **Dumitru Moloșnic** ([#11](https://github.com/imrj05/orbit/pull/11)) — light,\n  dark, and system appearance modes; transcript table sizing, streaming\n  scroll-position, and multiline command-preview fixes."
                            .into(),
                    ),
                    from_check: false,
                }),
            updater_dialog_focus: cx.focus_handle(),
            updater_dialog_focus_pending: false,
            updater_button_hovered: false,
            updater_button_width: Rc::new(Cell::new(updater_ui::UPDATER_PILL_COLLAPSED_W)),
            updater_button_label_reveal: Rc::new(Cell::new(0.)),
            updater_button_animation_from_width: updater_ui::UPDATER_PILL_COLLAPSED_W,
            updater_button_animation_from_reveal: 0.,
            updater_button_animation_generation: 0,
            automatic_updates_enabled: cx
                .try_global::<crate::updater::UpdaterState>()
                .and_then(|state| state.0.as_ref())
                .map(|updater| updater.automatically_checks_for_updates())
                .unwrap_or(false),
        };

        let app_weak = cx.entity().downgrade();
        // A changed-file row on the Git page opens its diff in Review. The
        // Review pane replaces the Git page, so the diff gets the column and
        // the change list is not left behind.
        let review_sidepane = app.sidepane.clone();
        let review_app = app_weak.clone();
        app.git_panel.update(cx, |panel, _| {
            panel.set_open_file(Rc::new(move |path, _window, cx| {
                review_sidepane.update(cx, |pane, cx| pane.show_file(path, cx));
                let _ = review_app.update(cx, |app, cx| {
                    app.git_open = false;
                    cx.notify();
                });
            }));
        });
        // A conflicted (or history) file on the Git page opens in the Files
        // editor. `open_file_in_viewer` leaves the Git page, which is the
        // intended "resolve it, then continue the merge" flow.
        app.git_panel.update(cx, |panel, _| {
            panel.set_open_path(Rc::new(move |path, display, _window, cx| {
                let _ = app_weak.update(cx, |app, cx| {
                    app.open_file_in_viewer(path, display, cx);
                });
            }));
        });
        // The Git page's Back button leaves the page. The panel closes itself
        // first, so this only clears the app flag (never re-enters the panel).
        let app_weak = cx.entity().downgrade();
        app.git_panel.update(cx, |panel, _| {
            panel.set_on_close(Rc::new(move |_window, cx| {
                let _ = app_weak.update(cx, |app, cx| {
                    app.git_open = false;
                    cx.notify();
                });
            }));
        });

        // The Usage page's two callbacks: leave the page, and open a session
        // it names (by pi session id, resolved against the loaded sessions).
        let app_weak = cx.entity().downgrade();
        app.usage.update(cx, |page, _| {
            page.set_on_close(Rc::new(move |_window, cx| {
                let _ = app_weak.update(cx, |app, cx| {
                    app.usage_open = false;
                    cx.notify();
                });
            }));
        });
        let app_weak = cx.entity().downgrade();
        app.usage.update(cx, |page, _| {
            page.set_open_session(Rc::new(move |session_id, window, cx| {
                let _ = app_weak.update(cx, |app, cx| {
                    app.open_session_by_id(session_id, window, cx);
                });
            }));
        });
        // Warm the store scan at launch so the page is instant when opened.
        app.usage.update(cx, |page, cx| page.open(cx));

        // The theme observer above only fires on a *change*; the first sync
        // happens in `main`, so the initial paint is already themed.

        if app.client.is_some() {
            app.send(CommandBody::GetState, "get_state");
            app.send(CommandBody::GetAvailableModels, "get_available_models");
            app.send(
                CommandBody::GetAvailableThinkingLevels,
                "get_available_thinking_levels",
            );
            app.send(CommandBody::GetCommands, "get_commands");
            app.probe_auth();
        }
        // The branch chip tracks the launch workspace even before a session
        // is open.
        app.refresh_branch_status(cx);
        // Check for a newer pi in the background; a newer release installs
        // itself through pi's own updater (see `pi_update_ui`).
        app.check_pi_update_on_launch(cx);
        // Route banner clicks back to their session (bundle builds only).
        notifications::init();
        // A first launch with notifications on shows macOS's permission
        // prompt, the way any native app announces them. Unbundled runs
        // no-op here; the settings row reports that state instead.
        if app.notification_prefs.desktop {
            notifications::request_permission();
        }
        app
    }

    /// Track whether the window is frontmost — the gate on background
    /// notifications. Registered from `main` after the window exists;
    /// `observe_window_activation` invokes the callback once at
    /// registration, so the field starts truthful.
    pub(super) fn watch_window_activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.window_active = window.is_window_active();
        self._window_activation = Some(cx.observe_window_activation(window, |app, window, _| {
            app.window_active = window.is_window_active();
        }));
    }

    /// Sidebar view of the session store: the sessions on disk plus a
    /// placeholder row for the open session while pi hasn't flushed its
    /// file yet. pi creates a session's `.jsonl` lazily — only when the
    /// first message is appended — so right after `new_session` the store
    /// holds nothing new and a disk-only list hides the session the user
    /// just started. A draft (nothing sent yet) stays hidden; once the
    /// first prompt lands in the transcript the placeholder shows it
    /// instantly at the top, and it disappears once the real row loads
    /// (same path ⇒ no duplicate).
    ///
    /// Only sessions inside Orbit's own project list reach the sidebar (and
    /// the ⌘P palette); everything else pi has on disk is left where it is.
    pub(super) fn sidebar_sessions(&self) -> Vec<SessionInfo> {
        let mut listed: Vec<SessionInfo> = self
            .sessions
            .iter()
            .filter(|session| {
                let cwd = normalize_workspace_path(&session.cwd.to_string_lossy());
                self.workspaces.contains(&cwd)
            })
            .cloned()
            .collect();
        // An explicit rename lives in memory until the next disk scan; keep
        // the open row in step with the header so the sidebar doesn't lag.
        if let Some(name) = self
            .session_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            if let Some(path) = &self.current_session_path {
                if let Some(session) = listed.iter_mut().find(|s| &s.path == path) {
                    session.title = name.to_string();
                }
            }
        }
        let first_message = self.transcript.first_user_message();
        sessions_with_placeholder(
            &listed,
            self.current_session_path.as_deref(),
            self.session_name
                .as_deref()
                .or(self.current_title.as_deref()),
            first_message.as_deref(),
            self.current_workspace.as_deref(),
            !self.transcript.is_empty(),
        )
    }

    pub(super) fn workspace_label(&self) -> String {
        self.current_workspace
            .as_ref()
            .map(|p| sessions::workspace_label(p))
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "workspace".into())
            })
    }

    /// Point the app at `cwd` and remember it for the next launch, so the
    /// new-task page opens on the last folder instead of the process cwd
    /// (which is `/` when the app is launched from Finder).
    pub(super) fn set_current_workspace(&mut self, cwd: PathBuf) {
        persist_last_workspace(&cwd);
        self.workspace_logo = crate::workspace_logo::load(&cwd);
        self.current_workspace = Some(cwd);
    }

    /// Add `cwd` to Orbit's project list if it isn't already there. Called
    /// whenever the user picks a folder to work in — starting a task there,
    /// browsing for one, or opening one of its sessions. Never writes to pi.
    pub(super) fn add_workspace(&mut self, cwd: PathBuf) {
        let cwd = normalize_workspace_path(&cwd.to_string_lossy());
        if self.workspaces.iter().any(|w| w == &cwd) {
            return;
        }
        self.workspaces.push(cwd);
        persist_workspaces(&self.workspaces);
    }

    /// Drop a project from Orbit's sidebar. pi's session files stay exactly
    /// where they are — only Orbit's own list changes.
    pub(super) fn remove_workspace(&mut self, cwd: &Path) {
        let before = self.workspaces.len();
        self.workspaces.retain(|w| w.as_path() != cwd);
        if self.workspaces.len() != before {
            persist_workspaces(&self.workspaces);
        }
    }

    /// Refetch the status bar's branch chip off-thread: branch name plus
    /// ahead/behind against its upstream. Called at launch, when the workspace
    /// moves, when the workspace watcher reports git state changed (commit,
    /// checkout, fetch), and after an in-app branch switch — render must never
    /// spawn `git`.
    pub(super) fn refresh_branch_status(&mut self, cx: &mut Context<Self>) {
        self.branch_fetch = self.branch_fetch.wrapping_add(1);
        let fetch = self.branch_fetch;
        let cwd = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        let Some(cwd) = cwd else {
            self.branch = None;
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let branch = cx
                .background_executor()
                .spawn(async move {
                    crate::git::current_branch(&cwd).map(|name| {
                        // The picker's own list, so the count and the popup it
                        // opens always agree.
                        let other_branches = crate::git::list_branches(&cwd).ok().map(|branches| {
                            branches.iter().filter(|branch| **branch != name).count()
                        });
                        BranchStatus {
                            name,
                            ahead_behind: crate::git::ahead_behind(&cwd),
                            other_branches,
                        }
                    })
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                // A newer fetch (workspace move or fresh git state) superseded
                // this one; drop the stale result.
                if app.branch_fetch != fetch {
                    return;
                }
                if app.branch != branch {
                    app.branch = branch;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Focusable for OrbitApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

// ── sidebar model ──────────────────────────────────────────────────────────

enum SideRow {
    /// Workspace group header: label, session count, collapsed state.
    Workspace {
        label: String,
        count: usize,
        collapsed: bool,
        /// Workspace path — used to spawn a new session in this project.
        cwd: PathBuf,
    },
    /// Session row — index into the (newest-first) sessions list.
    Session(usize),
    /// Reveal the next batch of hidden sessions in a workspace group
    /// (`count` = the step size, at most one
    /// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`]). `can_collapse` adds the
    /// right-side collapse affordance once the group has grown past the
    /// base cap.
    ShowMore {
        label: String,
        count: usize,
        can_collapse: bool,
    },
    /// Collapse a workspace group back to the truncated list.
    ShowLess { label: String },
}

/// `~/.orbit-pi/workspaces.json` — the folders Orbit lists in its sidebar.
/// Orbit-owned: pi owns the session files, this only records which projects
/// the user added. Removing a workspace here never touches pi.
fn workspaces_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("workspaces.json")
}

fn load_workspaces() -> Vec<PathBuf> {
    let Ok(raw) = fs::read_to_string(workspaces_path()) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    value
        .get("workspaces")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(normalize_workspace_path)
                .collect()
        })
        .unwrap_or_default()
}

fn persist_workspaces(workspaces: &[PathBuf]) {
    let path = workspaces_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let paths: Vec<String> = workspaces
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let payload = serde_json::json!({ "workspaces": paths });
    let _ = fs::write(path, payload.to_string());
}

/// `~/.orbit-pi/last-workspace.json` — the folder the last task ran in. The
/// process cwd is `/` when the app is launched from Finder/Dock, so the
/// new-task page restores this instead of showing `/`. Orbit-owned, like the
/// workspaces list.
fn last_workspace_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("last-workspace.json")
}

/// The remembered folder, if the store is readable and the folder still
/// exists on disk.
fn load_last_workspace() -> Option<PathBuf> {
    let raw = fs::read_to_string(last_workspace_path()).ok()?;
    let value = serde_json::from_str::<Value>(&raw).ok()?;
    let path = normalize_workspace_path(value.get("path")?.as_str()?);
    path.is_dir().then_some(path)
}

fn persist_last_workspace(path: &Path) {
    let file = last_workspace_path();
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let path = normalize_workspace_path(&path.to_string_lossy());
    let payload = serde_json::json!({ "path": path.to_string_lossy() });
    let _ = fs::write(file, payload.to_string());
}

/// A workspace path without a trailing separator, so it compares equal to
/// pi's `cwd` values (which never carry one). An empty remainder (`/`) is
/// kept as-is.
fn normalize_workspace_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim_end_matches(std::path::MAIN_SEPARATOR);
    if trimmed.is_empty() {
        PathBuf::from(raw)
    } else {
        PathBuf::from(trimmed)
    }
}

/// State of the row-actions popup in the sessions sidebar: which session
/// it belongs to (by path), what the row can offer, and whether the popup
/// is currently showing the delete confirmation.
#[derive(Clone, PartialEq)]
struct SessionMenu {
    path: PathBuf,
    /// Copy of the row title for the confirm copy.
    title: String,
    /// Sessions with a live pi process (active or parked) must not be
    /// deleted — the process would recreate the file mid-run.
    deletable: bool,
    /// The popup is showing the delete confirmation instead of the menu.
    confirm_delete: bool,
    /// Right-click origin in window coordinates: the popup floats at the
    /// pointer, context-menu style. `None` anchors it below the row's `…`
    /// button (the pointer-triggered path).
    at: Option<Point<Pixels>>,
}

/// State of the row-actions popup on a workspace group header: which
/// workspace it belongs to (by group label + cwd).
#[derive(Clone, PartialEq)]
struct WorkspaceMenu {
    label: String,
    cwd: PathBuf,
    /// Right-click origin in window coordinates (see [`SessionMenu::at`]).
    at: Option<Point<Pixels>>,
}

/// Sections of the settings surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Runtime,
    Agent,
    Skills,
    Plugins,
    Models,
    Appearance,
    Providers,
    Shortcuts,
    About,
}

/// The update modal's state. One surface serves both entry points: the
/// download control starts at [`Self::Available`] (a signed release is
/// already staged), while Check for Updates starts at [`Self::Checking`] and
/// lands on `Available`, `UpToDate`, or `Failed` when the worker reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateDialog {
    /// A check is in flight; the modal shows "Searching for a new version…".
    Checking,
    /// A signed release is staged and ready. `from_check` picks the secondary
    /// button's label: a check offers Cancel, the download control offers
    /// Later.
    Available {
        version: String,
        notes: Option<String>,
        from_check: bool,
    },
    /// The check completed and this build is current.
    UpToDate,
    /// The check or install failed, with the message to show inline.
    Failed(String),
    /// This build can't check for updates itself — a dev run or a binary
    /// outside a managed install. The modal explains instead of toasting.
    Unavailable,
}

/// The provider editor's open state. Inputs are `ComposerInput` entities so
/// they get real text editing (selection, IME, clipboard) for free.
struct ProviderEditor {
    /// Existing provider id, or `None` when adding a new one.
    original_id: Option<String>,
    /// Present in the live catalog — a built-in, or a custom provider pi has
    /// already loaded. Allows saving with an empty model list (an override).
    in_catalog: bool,
    id: Entity<ComposerInput>,
    name: Entity<ComposerInput>,
    base_url: Entity<ComposerInput>,
    api_key: Entity<ComposerInput>,
    models: Entity<ComposerInput>,
    /// Selected API family (chips above the fields).
    api: String,
    /// A key is already stored in models.json (placeholder + hint).
    had_api_key: bool,
    error: Option<String>,
}

/// What a [`ProviderKeyEditor`] is collecting. Ollama Cloud needs both a
/// monthly-credit API key and an optional legacy session; everything else uses
/// the plain API-key form.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProviderKeyKind {
    ApiKey,
    /// A pasted `Cookie:` header for Ollama Cloud's settings-page usage.
    OllamaCloudSession,
}

/// The API-key editor's open state (one field, provider-scoped).
struct ProviderKeyEditor {
    provider_id: String,
    provider_name: String,
    /// Supports OAuth too, so the modal can offer the sign-in path as well.
    oauth: bool,
    note: &'static str,
    kind: ProviderKeyKind,
    key: Entity<ComposerInput>,
    error: Option<String>,
}

/// API families pi can speak for a custom endpoint, shown as chips.
const PROVIDER_APIS: [&str; 9] = [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
    "mistral-conversations",
    "amazon-bedrock",
    "azure-openai-responses",
    "openai-codex-responses",
    "google-vertex",
];

/// One row of the Providers grid — the built-in catalog merged with the live
/// model catalog, models.json, and auth.json.
struct ProviderView {
    id: String,
    name: String,
    /// Has models in the live catalog (pi is serving it right now).
    active: bool,
    model_count: usize,
    /// Built-in catalog size from pi's bundled data (0 when unknown).
    catalog_count: usize,
    /// Has an entry in models.json (custom, or a built-in override).
    custom: bool,
    has_api_key: bool,
    base_url: String,
    api: String,
    /// pi ships this provider (vs. a models.json-only custom endpoint).
    builtin: bool,
    /// `/login <id>` offers a subscription/OAuth flow.
    oauth: bool,
    /// Storing an API key in auth.json works for this provider.
    api_key: bool,
    /// Environment variables pi reads for this provider's key.
    env_names: Vec<String>,
    /// The one that is actually set in this process, if any.
    env_var: Option<String>,
    note: &'static str,
    /// Credential in auth.json, if any.
    auth: Option<providers::ProviderAuth>,
    /// Live credential facts from the `auth.*` RPC namespace, when pi
    /// supports it. Preferred over the on-disk `auth` snapshot.
    live_status: Option<ProviderStatus>,
    /// Account quota/balance/spend from the `quota.*` RPC namespace, when
    /// available and connected.
    quota: Option<QuotaReport>,
    /// The provider's env var is set in this process's environment.
    env_authed: bool,
}

impl ProviderView {
    /// Any credential present (RPC status, auth.json, or environment).
    fn connected(&self) -> bool {
        self.live_status
            .as_ref()
            .is_some_and(|status| status.authenticated)
            || self.auth.is_some()
            || self.env_authed
    }

    /// The credential kind to display: live RPC status wins over the file.
    fn credential_kind(&self) -> Option<&str> {
        if let Some(status) = &self.live_status {
            if status.authenticated {
                return Some(status.credential.as_str());
            }
        }
        self.auth.map(|auth| match auth.kind {
            providers::AuthKind::OAuth => "oauth",
            providers::AuthKind::ApiKey => "api_key",
            providers::AuthKind::OllamaSession => "session",
        })
    }
}

/// Render an epoch-millisecond expiry as a short local date/time.
fn format_epoch_ms(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(ms).single() {
        Some(datetime) => datetime.format("%b %-d %H:%M").to_string(),
        None => "—".to_string(),
    }
}

/// Human label for a discovered auth method. pi's own label wins; otherwise a
/// sensible default keyed off the method id (never off a provider id).
fn method_label(id: &str, label: &str, connected: bool) -> String {
    if !label.is_empty() && label != id {
        return label.to_string();
    }
    match id {
        "browser" | "oauth" => {
            if connected {
                tr!("settings.reconnect")
            } else {
                tr!("settings.sign_in")
            }
        }
        "device_code" => tr!("settings.use_device_code"),
        other => {
            let mut chars = other.replace(['_', '-'], " ").chars().collect::<Vec<_>>();
            if let Some(first) = chars.first_mut() {
                first.make_ascii_uppercase();
            }
            chars.into_iter().collect()
        }
    }
}

/// A provider-card button action, dispatched through one handler so the card
/// can build buttons without a closure per action.
#[derive(Clone)]
enum ProviderAction {
    SignIn {
        id: String,
        name: String,
    },
    /// Start a login through the `auth.*` RPC namespace using a discovered
    /// method id (`browser`, `device_code`, …).
    AuthStart {
        id: String,
        name: String,
        method: String,
    },
    /// Cancel the active login for a provider.
    AuthCancel,
    /// Dismiss a finished login card.
    AuthDismiss,
    /// Open a device-code / verification URL in the browser.
    AuthOpenUrl(String),
    /// Copy a value (device code) to the clipboard.
    AuthCopy(String),
    EditKey {
        id: String,
        name: String,
        oauth: bool,
        note: &'static str,
    },
    /// Open the Ollama Cloud session editor (a pasted cookie header) — the
    /// legacy session/weekly path, distinct from the monthly-credit API key.
    /// Carries the provider id that opened it (`ollama` or `ollama-cloud`).
    EditOllamaSession {
        id: String,
        name: String,
    },
    SignOut {
        id: String,
    },
    /// Open the provider's usage/quota popup (windows, balances, spend).
    ShowUsage {
        id: String,
    },
    Configure {
        id: String,
    },
    Remove {
        id: String,
    },
    ConfirmRemove {
        id: String,
    },
    CancelRemove,
    SaveKey,
    Restart,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProviderButtonStyle {
    Primary,
    Ghost,
    Danger,
}

/// Live state of the active pi agent process, for Settings → Runtime.
#[derive(Default)]
struct RuntimeStatus {
    /// When the active process was adopted (drives the uptime readout).
    started_at: Option<Instant>,
    /// Whether the process is still running.
    alive: bool,
    /// Whether it exited on its own (as opposed to being stopped here).
    exited: bool,
    /// The last spawn failure, if any.
    error: Option<String>,
}

/// The runtime panel's headline state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeState {
    Running,
    Exited,
    Stopped,
    Failed,
}

/// Which dropdown is open on the settings surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSelect {
    Theme(ThemeMode),
    Language,
    UiFontSize,
    TerminalFont,
    EditorFont,
    SpacingDensity,
    UiFontFamily,
    CodeFontFamily,
    BackdropBlur,
    BackdropCell,
    BackdropFade,
    /// The model the auto-title extension asks (Settings → Agent).
    TitleModel,
}

// ── feature modules ───────────────────────────────────────────────────────
// `app.rs` keeps the `OrbitApp` model, the shared types, and the controller
// wiring. Rendering and feature-specific logic live in child modules; they
// are descendants of `app`, so they reach private fields/methods directly.
mod ai_review;
mod ask;
mod composer_ops;
mod dialogs;
mod events;
mod helpers;
mod open_in;
mod pi_update_ui;
mod pickers;
mod runtime;
mod search;
mod session;
mod settings;
mod sidebar;
mod skills_ui;
mod toast_ui;
mod updater_ui;
mod view;

#[cfg(test)]
mod backdrop_layout_tests;
#[cfg(test)]
mod devicons_tests;
#[cfg(test)]
mod error_label_tests;
#[cfg(test)]
mod popup_layout_tests;
#[cfg(test)]
mod sidebar_active_reveal_tests;
#[cfg(test)]
mod sidebar_placeholder_tests;
#[cfg(test)]
mod titlebar_layout_tests;

// `icon` and friends are part of the crate-wide UI kit; keep their original
// `crate::app::…` paths stable for the other modules that import them.
pub(crate) use helpers::{
    button_frame, context_menu_entry, context_menu_separator, context_menu_surface, empty_state,
    file_badge, file_glyph, icon, icon_button_frame, icon_dyn, input_field_frame, menu_header,
    nerd_font_family, picker_entry, picker_search_frame, picker_surface, press, refresh_glyph,
    spinner, EmptyFill, BUTTON_GROUP, PRESS_DIM,
};
use sidebar::sessions_with_placeholder;
