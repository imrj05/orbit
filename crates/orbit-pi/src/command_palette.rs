//! Command palette — the window-wide ⌘P surface for jumping to a session,
//! running an app command, or opening a settings section from one field.
//!
//! Direction contract (impeccable):
//! THESIS: one keystroke puts every place and action a query away — sessions
//! first, then commands, then settings — in a single top-centered card. It
//! refuses the split between the sidebar's session switcher, the composer's
//! chip popovers, and the settings gear: one filter ranks all of them.
//! OWN-WORLD: Orbit's raised-surface language (`menu_bg`, 1px `border_strong`,
//! layered contact+ambient shadow, `overlay_strong` highlight, accent for the
//! live session) lifted onto a dimmed modal layer — the same atoms as the
//! sidebar palette and chip popovers, at window scale.
//! STORY: the operator hits ⌘P, types two or three characters, and lands on a
//! session, a panel toggle, or a settings section; Enter executes and the
//! composer regains focus.
//! FIRST VIEWPORT: a 640px card floating near the top over a black scrim; a
//! tall search row with a search glyph; sectioned rows (Sessions / Commands /
//! Settings) with icon, label, inline detail, and a shortcut chip; a quiet
//! footer naming the keys.
//! FORM: a command-palette anatomy (sections, fuzzy scoring, wrap-around
//! navigation, scrim layer, in-memory results) adapted to Orbit's entity +
//! callback conventions. Opened with ⌘P or the sidebar Search row.
//!
//! Key handling reuses the existing `Picker` bindings (enter/escape/↑/↓),
//! which are registered after the composer's and therefore win at the same
//! dispatch depth; the filter input carries the `Composer Picker` context so
//! backspace/paste keep editing.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use gpui::{
    deferred, div, hsla, point, prelude::*, px, App, Context, ElementId, Entity, FocusHandle,
    Focusable, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Window,
};

use crate::app::{icon, SettingsSection};
use crate::composer::ComposerInput;
use crate::sessions::SessionInfo;
use crate::theme::{self, Theme};

/// Card width — room for a session title plus inline detail.
const CARD_W: f32 = 640.;
/// Uniform row height (icon + label + inline detail + shortcut chip).
const ROW_H: f32 = 44.;
/// Section header height ("Sessions" / "Commands" / "Settings").
const HEADER_H: f32 = 30.;
/// Search field row.
const SEARCH_H: f32 = 56.;
/// Footer hint bar.
const FOOTER_H: f32 = 30.;
/// Largest results height before the list scrolls (≈ 9 rows + headers).
const LIST_MAX_H: f32 = 430.;
/// Tallest the whole card may grow; the list scrolls past this.
const CARD_MAX_H: f32 = SEARCH_H + LIST_MAX_H + FOOTER_H;
/// Sessions listed when the query is empty (recent activity, newest first).
const EMPTY_SESSION_ROWS: usize = 6;
/// Sessions listed for a non-empty query.
const MAX_SESSION_RESULTS: usize = 12;

/// A command the palette can ask the app to run. Session opening goes
/// through its own callback; everything else is one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    NewSession,
    RefreshSessions,
    FocusComposer,
    ToggleSidebar,
    ToggleSidePanel,
    ToggleTerminal,
    ToggleProjectPanel,
    ReviewChanges,
    OpenGit,
    ChooseModel,
    ChooseThinking,
    AbortRun,
    CopySessionId,
    CloneSession,
    OpenSettings(SettingsSection),
}

/// Everything the palette needs to build its items, captured by the app at
/// open time. The palette never reaches back into app state — render reads
/// only this in-memory snapshot (render-purity rule).
pub struct PaletteSnapshot {
    pub sessions: Vec<SessionInfo>,
    /// The open session's path, so its row can be emphasized.
    pub active_path: Option<PathBuf>,
    /// An agent run is in flight (gates the Abort command).
    pub busy: bool,
    /// pi's session id for the active session (gates Copy Session ID).
    pub session_id: Option<String>,
    pub sidebar_visible: bool,
    pub side_panel_visible: bool,
    pub terminal_visible: bool,
    pub project_panel_visible: bool,
    pub can_choose_model: bool,
    pub can_choose_thinking: bool,
}

/// Result sections, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Sessions,
    Commands,
    Settings,
}

impl Section {
    fn label(self) -> String {
        match self {
            Self::Sessions => tr!("command_palette.sessions"),
            Self::Commands => tr!("command_palette.commands"),
            Self::Settings => tr!("command_palette.settings"),
        }
    }
}

/// What activating a row does.
#[derive(Debug, Clone)]
enum PaletteAction {
    OpenSession(SessionInfo),
    Run(PaletteCommand),
}

/// One row in the results list.
#[derive(Debug, Clone)]
struct PaletteItem {
    section: Section,
    icon: &'static str,
    label: String,
    detail: Option<String>,
    shortcut: Option<&'static str>,
    action: PaletteAction,
    /// Lowercased haystack for fuzzy matching (label + keywords + detail).
    search: String,
    /// Declaration order — the stable tie-break for commands/settings.
    order: usize,
    /// Last-activity seconds (sessions only; 0 for commands).
    recency: u64,
}

impl PaletteItem {
    fn command(
        label: impl Into<String>,
        icon: &'static str,
        shortcut: Option<&'static str>,
        command: PaletteCommand,
        keywords: &str,
        order: usize,
    ) -> Self {
        let label: String = label.into();
        Self {
            section: match command {
                PaletteCommand::OpenSettings(_) => Section::Settings,
                _ => Section::Commands,
            },
            icon,
            detail: match command {
                PaletteCommand::OpenSettings(_) => Some(tr!("command_palette.settings")),
                _ => None,
            },
            shortcut,
            action: PaletteAction::Run(command),
            label: label.clone(),
            search: format!("{} {keywords}", label.to_lowercase()),
            order,
            recency: 0,
        }
    }
}

/// Subsequence fuzzy score, Zed-style: every query character must appear in
/// order; word-boundary and consecutive hits score higher. Returns `None`
/// when the query is not a subsequence of the candidate.
fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }
    let query: Vec<char> = query.to_lowercase().chars().collect();
    let candidate: Vec<char> = candidate.to_lowercase().chars().collect();
    let mut score = 0u32;
    let mut qi = 0;
    let mut prev_hit = false;
    for (i, &c) in candidate.iter().enumerate() {
        if qi < query.len() && c == query[qi] {
            let boundary = i == 0 || !candidate[i - 1].is_alphanumeric();
            score += 1 + u32::from(boundary) * 6 + u32::from(prev_hit) * 3;
            qi += 1;
            prev_hit = true;
        } else {
            prev_hit = false;
        }
    }
    (qi == query.len()).then_some(score)
}

/// Open callback: the chosen session plus the ambient window.
type OpenSession = Box<dyn Fn(SessionInfo, &mut Window, &mut App)>;
/// Command callback: the chosen palette command plus the ambient window.
type RunCommand = Box<dyn Fn(PaletteCommand, &mut Window, &mut App)>;
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type PaletteDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

/// The palette entity. Created by `OrbitApp` on ⌘P; talks back exclusively
/// through the callbacks it was built with.
pub struct CommandPalette {
    snapshot: PaletteSnapshot,
    filter: Entity<ComposerInput>,
    scroll: ScrollHandle,
    highlighted: usize,
    last_filter: String,
    on_open: OpenSession,
    on_command: RunCommand,
    /// `bool` = dismissed by an outside mouse-down (vs. escape).
    on_dismiss: PaletteDismiss,
}

impl CommandPalette {
    pub fn new(
        snapshot: PaletteSnapshot,
        on_open: OpenSession,
        on_command: RunCommand,
        on_dismiss: PaletteDismiss,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("command_palette.search_sessions_commands_settings")
                .with_key_context("Composer Picker")
        });
        Self {
            snapshot,
            filter,
            scroll: ScrollHandle::new(),
            highlighted: 0,
            last_filter: String::new(),
            on_open,
            on_command,
            on_dismiss,
        }
    }

    // ── actions (Picker key context) ─────────────────────────────────────

    fn on_cancel(&mut self, _: &crate::PickerCancel, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(false, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.results(&self.last_filter);
        self.activate(self.highlighted, &rows, window, cx);
    }

    fn on_next(&mut self, _: &crate::PickerSelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn on_prev(&mut self, _: &crate::PickerSelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn on_scrim_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(true, window, cx);
    }

    // ── internals ────────────────────────────────────────────────────────

    /// Session rows (newest activity first — `load_sessions` pre-sorts).
    fn session_items(&self) -> Vec<PaletteItem> {
        self.snapshot
            .sessions
            .iter()
            .enumerate()
            .map(|(order, session)| {
                let workspace = crate::sessions::workspace_label(&session.cwd);
                let active = self.snapshot.active_path.as_ref() == Some(&session.path);
                let mut detail = format!(
                    "{workspace} · {}",
                    crate::sessions::relative_time(session.modified)
                );
                if active {
                    detail.push_str(&tr!("command_palette.current"));
                }
                PaletteItem {
                    section: Section::Sessions,
                    icon: "icons/chat.svg",
                    label: session.title.clone(),
                    detail: Some(detail),
                    shortcut: None,
                    action: PaletteAction::OpenSession(session.clone()),
                    search: format!(
                        "{} {} {} session chat conversation",
                        session.title.to_lowercase(),
                        workspace.to_lowercase(),
                        session.cwd.to_string_lossy().to_lowercase(),
                    ),
                    order,
                    recency: session
                        .modified
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                }
            })
            .collect()
    }

    /// Command + settings rows, gated by the snapshot's live facts so the
    /// palette never offers an action the app can't perform right now.
    fn command_items(&self) -> Vec<PaletteItem> {
        let mut order = 0usize;
        let mut next = || {
            let current = order;
            order += 1;
            current
        };
        let mut items = vec![
            PaletteItem::command(
                tr!("command_palette.new_session"),
                "icons/plus.svg",
                Some(crate::platform::shortcuts::NEW_SESSION),
                PaletteCommand::NewSession,
                "new session chat conversation start task",
                next(),
            ),
            PaletteItem::command(
                tr!("command_palette.focus_composer"),
                "icons/compose.svg",
                None,
                PaletteCommand::FocusComposer,
                "focus composer prompt input message write",
                next(),
            ),
            PaletteItem::command(
                tr!("command_palette.refresh_sessions"),
                "icons/refresh.svg",
                Some(crate::platform::shortcuts::REFRESH),
                PaletteCommand::RefreshSessions,
                "refresh reload sessions list disk",
                next(),
            ),
            PaletteItem::command(
                if self.snapshot.sidebar_visible {
                    tr!("command_palette.hide_sidebar")
                } else {
                    tr!("command_palette.show_sidebar")
                },
                "icons/layout-left.svg",
                None,
                PaletteCommand::ToggleSidebar,
                "toggle show hide left sidebar sessions history",
                next(),
            ),
            PaletteItem::command(
                if self.snapshot.side_panel_visible {
                    tr!("command_palette.hide_side_panel")
                } else {
                    tr!("command_palette.show_side_panel")
                },
                "icons/panel-right.svg",
                None,
                PaletteCommand::ToggleSidePanel,
                "toggle show hide right panel review git diff",
                next(),
            ),
            PaletteItem::command(
                if self.snapshot.terminal_visible {
                    tr!("command_palette.hide_terminal")
                } else {
                    tr!("command_palette.show_terminal")
                },
                "icons/terminal.svg",
                Some(crate::platform::shortcuts::TERMINAL),
                PaletteCommand::ToggleTerminal,
                "toggle show hide terminal shell console command line pty",
                next(),
            ),
            PaletteItem::command(
                if self.snapshot.project_panel_visible {
                    tr!("explorer.hide")
                } else {
                    tr!("explorer.show")
                },
                "icons/folder.svg",
                Some("⌘⇧E"),
                PaletteCommand::ToggleProjectPanel,
                "explorer files project panel tree folders workspace toggle show hide",
                next(),
            ),
            PaletteItem::command(
                tr!("command_palette.review_changes"),
                "icons/file-diff.svg",
                None,
                PaletteCommand::ReviewChanges,
                "review git diff changes files panel",
                next(),
            ),
            PaletteItem::command(
                tr!("command_palette.open_git"),
                "icons/git-commit.svg",
                None,
                PaletteCommand::OpenGit,
                "git commit push branch history graph changes",
                next(),
            ),
        ];
        if self.snapshot.can_choose_model {
            items.push(PaletteItem::command(
                tr!("command_palette.choose_model"),
                "icons/spark.svg",
                None,
                PaletteCommand::ChooseModel,
                "choose change select model provider agent",
                next(),
            ));
        }
        if self.snapshot.can_choose_thinking {
            items.push(PaletteItem::command(
                tr!("command_palette.choose_thinking_level"),
                "icons/thinking-medium.svg",
                None,
                PaletteCommand::ChooseThinking,
                "choose change select thinking level reasoning effort",
                next(),
            ));
        }
        if self.snapshot.busy {
            items.push(PaletteItem::command(
                tr!("command_palette.abort_run"),
                "icons/stop.svg",
                Some("Esc"),
                PaletteCommand::AbortRun,
                "abort stop cancel run agent working",
                next(),
            ));
        }
        if self.snapshot.session_id.is_some() {
            items.push(PaletteItem::command(
                tr!("command_palette.copy_session_id"),
                "icons/copy.svg",
                None,
                PaletteCommand::CopySessionId,
                "copy session id uuid identifier debug",
                next(),
            ));
            items.push(PaletteItem::command(
                tr!("command_palette.clone_session"),
                "icons/git-fork.svg",
                None,
                PaletteCommand::CloneSession,
                "clone duplicate fork copy session branch conversation",
                next(),
            ));
        }
        for (section, icon, label, keywords) in [
            (
                SettingsSection::General,
                "icons/settings.svg",
                tr!("settings.general"),
                "settings preferences general language font",
            ),
            (
                SettingsSection::Runtime,
                "icons/server-stack.svg",
                tr!("settings.runtime"),
                "settings preferences runtime process pi start stop restart",
            ),
            (
                SettingsSection::Agent,
                "icons/spark.svg",
                tr!("settings.agent"),
                "settings preferences agent steer follow-up compaction retry",
            ),
            (
                SettingsSection::Skills,
                "icons/magic-wand.svg",
                tr!("settings.skills"),
                "settings preferences skills skill.md instructions agent",
            ),
            (
                SettingsSection::Plugins,
                "icons/extensions.svg",
                tr!("settings.plugins"),
                "settings preferences plugins extensions packages install npm git",
            ),
            (
                SettingsSection::Models,
                "icons/tag-01.svg",
                tr!("settings.models"),
                "settings preferences models catalog favorites providers",
            ),
            (
                SettingsSection::Appearance,
                "icons/contrast.svg",
                tr!("settings.appearance"),
                "settings preferences appearance theme light dark",
            ),
            (
                SettingsSection::Providers,
                "icons/cloud.svg",
                tr!("settings.providers"),
                "settings preferences providers models api",
            ),
            (
                SettingsSection::About,
                "icons/info.svg",
                tr!("settings.about"),
                "settings about version app",
            ),
        ] {
            items.push(PaletteItem::command(
                label,
                icon,
                (section == SettingsSection::General)
                    .then_some(crate::platform::shortcuts::SETTINGS),
                PaletteCommand::OpenSettings(section),
                keywords,
                next(),
            ));
        }
        items
    }

    /// Rows for the current query: an empty query lists recent sessions plus
    /// every command; a query fuzzy-filters and ranks each section.
    fn results(&self, query: &str) -> Vec<PaletteItem> {
        let query = query.trim();
        if query.is_empty() {
            let mut rows: Vec<PaletteItem> = self
                .session_items()
                .into_iter()
                .take(EMPTY_SESSION_ROWS)
                .collect();
            rows.extend(self.command_items());
            return rows;
        }
        let rank = |items: Vec<PaletteItem>, cap: usize| -> Vec<PaletteItem> {
            let mut scored: Vec<(u32, PaletteItem)> = items
                .into_iter()
                .filter_map(|item| fuzzy_score(query, &item.search).map(|s| (s, item)))
                .collect();
            scored.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then(b.1.recency.cmp(&a.1.recency))
                    .then(a.1.order.cmp(&b.1.order))
            });
            scored.truncate(cap);
            scored.into_iter().map(|(_, item)| item).collect()
        };
        let mut rows = rank(self.session_items(), MAX_SESSION_RESULTS);
        rows.extend(rank(self.command_items(), usize::MAX));
        rows
    }

    /// Wrap-around ↑/↓ navigation, keeping the highlighted row
    /// fully visible in the scroll window.
    fn step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let rows = self.results(&self.last_filter);
        if rows.is_empty() {
            return;
        }
        let pos = self.highlighted.min(rows.len() - 1) as isize;
        let next = (pos + dir).rem_euclid(rows.len() as isize) as usize;
        self.highlighted = next;
        self.ensure_visible(&rows);
        cx.notify();
    }

    /// Pixel offset of a row's top edge, accounting for section headers.
    fn row_top(rows: &[PaletteItem], ix: usize) -> f32 {
        let mut y = 0.;
        let mut prev = None;
        for (i, item) in rows.iter().enumerate() {
            if prev != Some(item.section) {
                y += HEADER_H;
                prev = Some(item.section);
            }
            if i == ix {
                break;
            }
            y += ROW_H;
        }
        y
    }

    /// Total content height of the results list (rows + headers).
    fn content_height(rows: &[PaletteItem]) -> f32 {
        let headers = {
            let mut count = 0;
            let mut prev = None;
            for item in rows {
                if prev != Some(item.section) {
                    count += 1;
                    prev = Some(item.section);
                }
            }
            count
        };
        rows.len() as f32 * ROW_H + headers as f32 * HEADER_H
    }

    fn ensure_visible(&mut self, rows: &[PaletteItem]) {
        let content_h = Self::content_height(rows);
        let viewport_h = content_h.min(LIST_MAX_H);
        let row_top = Self::row_top(rows, self.highlighted);
        let current: f32 = self.scroll.offset().y.into();
        let mut offset = current;
        if row_top < current {
            offset = row_top;
        } else if row_top + ROW_H > current + viewport_h {
            offset = row_top + ROW_H - viewport_h;
        }
        let max_offset = (content_h - viewport_h).max(0.);
        self.scroll
            .set_offset(point(px(0.), px(offset.clamp(0., max_offset))));
    }

    fn activate(
        &mut self,
        ix: usize,
        rows: &[PaletteItem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match rows.get(ix) {
            Some(PaletteItem {
                action: PaletteAction::OpenSession(session),
                ..
            }) => (self.on_open)(session.clone(), window, cx),
            Some(PaletteItem {
                action: PaletteAction::Run(command),
                ..
            }) => (self.on_command)(*command, window, cx),
            None => {}
        }
    }
}

impl Focusable for CommandPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.read(cx).focus_handle(cx)
    }
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let needle = self.filter.read(cx).text().to_lowercase();
        if needle != self.last_filter {
            self.last_filter = needle.clone();
            self.highlighted = 0;
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
        let rows = self.results(&needle);
        if !rows.is_empty() {
            self.highlighted = self.highlighted.min(rows.len() - 1);
        }

        let this = cx.entity();
        let theme = *theme::get(cx);

        // Hug the content, capped so the card never owns the window.
        let viewport_h = f32::from(window.viewport_size().height);
        let top = (viewport_h * 0.09).clamp(48., 72.);
        let card_cap = (viewport_h - top - 36.).max(SEARCH_H + ROW_H + FOOTER_H);
        let content_h = if rows.is_empty() {
            140.
        } else {
            Self::content_height(&rows)
        };
        let list_cap = CARD_MAX_H.min(card_cap) - SEARCH_H - FOOTER_H;
        let list_h = content_h.min(list_cap);

        // ── results list ──
        let mut list = div()
            .id("command-palette-list")
            .w_full()
            .h(px(list_h))
            .flex_none()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(8.))
            .pb(px(8.))
            .flex()
            .flex_col();
        if rows.is_empty() {
            list = list.child(
                div()
                    .h_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .child(icon("icons/search.svg", 18., theme.text_3))
                    .child(
                        div()
                            .text_size(theme.ui_px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_2)
                            .child(tr!("command_palette.no_results")),
                    )
                    .child(
                        div()
                            .text_size(theme.ui_px(12.))
                            .text_color(theme.text_3)
                            .child(tr!(
                                "command_palette.try_a_session_title_a_command_or_a_setting"
                            )),
                    ),
            );
        } else {
            let mut prev_section = None;
            for ix in 0..rows.len() {
                if prev_section != Some(rows[ix].section) {
                    list = list.child(
                        div()
                            .h(px(HEADER_H))
                            .px(px(10.))
                            .pt(px(10.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .text_size(theme.ui_px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_3)
                            .child(rows[ix].section.label()),
                    );
                    prev_section = Some(rows[ix].section);
                }
                let is_current = match &rows[ix].action {
                    PaletteAction::OpenSession(session) => {
                        self.snapshot.active_path.as_ref() == Some(&session.path)
                    }
                    PaletteAction::Run(_) => false,
                };
                list = list.child(render_row(
                    &rows,
                    ix,
                    ix == self.highlighted,
                    is_current,
                    &this,
                    theme,
                ));
            }
        }

        // ── card ──
        let card = div()
            .w_full()
            .max_w(px(CARD_W))
            .flex_none()
            .rounded(px(14.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .font_family(theme::ui_font_family())
            // Clicks inside the card must not reach the scrim's dismiss.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            // search row
            .child(
                div()
                    .h(px(SEARCH_H))
                    .px(px(18.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(theme.ui_px(15.))
                    .text_color(theme.text)
                    .child(icon("icons/search.svg", 16., theme.text_3))
                    .child(div().flex_1().min_w_0().child(self.filter.clone())),
            )
            .child(list)
            // footer — the palette is new chrome; name the keys once, quietly.
            .child(
                div()
                    .h(px(FOOTER_H))
                    .px(px(14.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .border_t_1()
                    .border_color(theme.border)
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.text_3)
                    .child(tr!("command_palette.navigate"))
                    .child(tr!("command_palette.select"))
                    .child(tr!("command_palette.esc_close")),
            );

        // ── scrim layer ──
        let scrim = match theme.mode {
            theme::ThemeMode::Dark => hsla(0., 0., 0., 0.26),
            theme::ThemeMode::Light => hsla(0., 0., 0., 0.14),
        };
        div()
            .id("command-palette-layer")
            .absolute()
            .inset_0()
            .occlude()
            .bg(scrim)
            .px(px(24.))
            .pt(px(top))
            .flex()
            .items_start()
            .justify_center()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_scrim_down))
            .child(card)
    }
}

fn render_row(
    rows: &[PaletteItem],
    ix: usize,
    highlighted: bool,
    is_current_session: bool,
    this: &Entity<CommandPalette>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let item = rows[ix].clone();
    let this = this.clone();
    div()
        .id(ElementId::NamedInteger(
            "command-palette-row".into(),
            ix as u64,
        ))
        .h(px(ROW_H))
        .px(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(if highlighted {
            theme.border_strong
        } else {
            gpui::transparent_black()
        })
        .flex_none()
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        // Hover moves the keyboard highlight; click activates the row.
        .on_hover({
            let this = this.clone();
            move |hovering, _, cx| {
                if *hovering {
                    this.update(cx, |palette, cx| {
                        if palette.highlighted != ix {
                            palette.highlighted = ix;
                            cx.notify();
                        }
                    });
                }
            }
        })
        .on_click({
            let this = this.clone();
            move |_, window, cx| {
                this.update(cx, |palette, cx| {
                    let rows = palette.results(&palette.last_filter);
                    palette.activate(ix, &rows, window, cx);
                });
            }
        })
        .when(highlighted, |row| row.bg(theme.overlay_strong))
        .child(
            div()
                .size(px(20.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(icon(
                    item.icon,
                    15.,
                    if is_current_session {
                        theme.accent
                    } else {
                        theme.text_2
                    },
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_baseline()
                .gap(px(8.))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(theme.ui_px(13.5))
                        .font_weight(if highlighted {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if highlighted {
                            theme.text
                        } else {
                            theme.text_2
                        })
                        .child(item.label.clone()),
                )
                .when_some(item.detail.clone(), |row, detail| {
                    row.child(
                        div()
                            .flex_none()
                            .text_size(theme.ui_px(12.))
                            .text_color(theme.text_3)
                            .child(detail),
                    )
                }),
        )
        .when_some(item.shortcut, |row, shortcut| {
            row.child(
                div()
                    .h(px(22.))
                    .min_w(px(28.))
                    .px(px(7.))
                    .rounded(px(7.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.overlay_strong)
                    .text_size(theme.ui_px(11.5))
                    .text_color(theme.text_3)
                    .child(SharedString::from(shortcut)),
            )
        })
}

/// The palette paints above every other floating surface (chip popovers,
/// menus) — `OrbitApp` wraps the entity in this deferred layer.
pub fn layer(palette: Entity<CommandPalette>) -> impl IntoElement {
    deferred(palette).with_priority(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_subsequence_and_boundaries() {
        assert!(fuzzy_score("ns", "new session").is_some());
        assert!(fuzzy_score("sett", "Settings General").is_some());
        assert!(fuzzy_score("absent", "New Session").is_none());
        assert!(fuzzy_score("", "anything").is_some());
        // Word-boundary hits outrank mid-word hits.
        let boundary = fuzzy_score("side", "Hide Sidebar").unwrap();
        let midword = fuzzy_score("side", "considerations").unwrap();
        assert!(boundary > midword);
        // Consecutive runs beat scattered hits.
        let run = fuzzy_score("model", "Choose Model").unwrap();
        let scattered = fuzzy_score("model", "mode toggle panel").unwrap_or(0);
        assert!(run > scattered);
    }

    #[test]
    fn sections_order_sessions_first() {
        assert!(Section::Sessions < Section::Commands);
        assert!(Section::Commands < Section::Settings);
    }

    #[test]
    fn row_top_accounts_for_headers() {
        let item = |section: Section, order: usize| PaletteItem {
            section,
            icon: "icons/chat.svg",
            label: format!("Item {order}"),
            detail: None,
            shortcut: None,
            action: PaletteAction::Run(PaletteCommand::NewSession),
            search: String::new(),
            order,
            recency: 0,
        };
        let rows = vec![
            item(Section::Sessions, 0),
            item(Section::Sessions, 1),
            item(Section::Commands, 2),
        ];
        assert_eq!(CommandPalette::row_top(&rows, 0), HEADER_H);
        assert_eq!(CommandPalette::row_top(&rows, 1), HEADER_H + ROW_H);
        assert_eq!(
            CommandPalette::row_top(&rows, 2),
            HEADER_H * 2. + ROW_H * 2.
        );
        assert_eq!(
            CommandPalette::content_height(&rows),
            HEADER_H * 2. + ROW_H * 3.
        );
    }
}
