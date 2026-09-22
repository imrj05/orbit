//! Transcript paint — Waku `transcript_view.rs` layout, honest Orbit data.
//!
//! Rows are plain GPUI flex trees (no component library): user turns are
//! End-aligned neutral bubbles, assistant turns are Start-aligned prose. A
//! ghost copy/timestamp footer reveals on row hover. Settled: **Worked for**
//! fold → thinking/tool cards → answer → files → copy footer. Live: thinking
//! + activity cards → answer → files → **Working for**.
//!
//! Timestamps render on the footer when pi provides one (snapshot
//! `timestamp` fields, or a wall-clock stamp taken at `message_end`).
//! Not painted (pi does not provide them): git Review, conversation fork,
//! per-tool checkpoint diffs — an edit/write tool instead shows an inline
//! diff built from its own `oldText`/`newText` arguments.

use std::{
    cell::{Cell, RefCell},
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    deferred, div, img, linear_color_stop, linear_gradient, list, point, prelude::*, px, svg,
    Animation, AnimationExt, AnyElement, App, ClipboardItem, ElementId, Font, FontFeatures,
    FontStyle, FontWeight, Hsla, Image, ImageSource, InteractiveText, ObjectFit, Pixels,
    ScrollHandle, SharedString, StrikethroughStyle, StyledText, TextAlign, TextRun, UnderlineStyle,
    Window,
};

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use serde_json::Value;

use orbit_rpc::MessageUsage;

use crate::context_meter::format_tokens;
use crate::highlight::{self, Token};
use crate::message_scroller::{self, MessageScrollerState};
use crate::shimmer::ShimmerText;
use crate::theme::{self, Theme, ThemeMode};
use crate::transcript::{ChatMessage, Step, ToolCall};

/// Opens the changed-files Review in the side pane (see `sidepane.rs`) —
/// handed down from the app so the transcript's Review buttons can point
/// at it without knowing the pane exists.
pub(crate) type ReviewOpener = Rc<dyn Fn(&mut Window, &mut App)>;

/// Opens one attachment image in the app's full-window lightbox. Handed down
/// from the app so an image tile can open a surface it doesn't own.
pub(crate) type ImageOpener = Rc<dyn Fn(Arc<Image>, &mut Window, &mut App)>;

/// Waku `CONTENT_MAX_WIDTH` (the `max-w-[760px]` transcript column).
/// Orbit uses 960 so fenced code and tables can use the full pane; body
/// line-height (14/26) keeps prose readable without a nested measure cap
/// that mis-measured wrapped markdown and stacked lines on top of each other.
const CONTENT_MAX_WIDTH: f32 = 960.0;
/// Extra space before a follow-up user message (Waku `pt-8`).
const FOLLOWUP_TURN_TOP_GAP: f32 = 32.0;
/// Waku user-bubble `max-w-[540px]`.
const USER_BUBBLE_MAX_WIDTH: f32 = 540.0;
/// Waku message-footer action button `size-[27px]`.
const FOOTER_BUTTON_SIZE: f32 = 27.0;
const COPY_FEEDBACK: Duration = Duration::from_secs(2);
const NAVIGATION_RAIL_LEFT: f32 = 16.0;
const NAVIGATION_RAIL_WIDTH: f32 = 44.0;
const NAVIGATION_RAIL_TICK_WIDTH: f32 = 32.0;
const NAVIGATION_RAIL_TICK_HEIGHT: f32 = 2.0;
const NAVIGATION_RAIL_TURN_HEIGHT: f32 = 12.0;
const NAVIGATION_RAIL_INACTIVE_OPACITY: f32 = 0.45;
/// Tick width by emphasis distance from the hovered turn (Waku rail).
const NAVIGATION_RAIL_EMPHASIS_SCALE: [f32; 4] = [1.0, 0.68, 0.44, 0.25];
/// Waku caps the rail at 80% of the viewport and hides it below an 872px
/// transcript container.
const NAVIGATION_RAIL_MAX_HEIGHT: f32 = 0.8;
const NAVIGATION_RAIL_MIN_MAIN_WIDTH: f32 = 1040.0;
const CHANGED_FILES_PREVIEW_LIMIT: usize = 3;
const CONTENT_GAP: f32 = 10.0;
const NAVIGATION_RAIL_CONTENT_GAP: f32 = 12.0;
const NAVIGATION_RAIL_PREVIEW_WIDTH: f32 = 320.0;
const NAVIGATION_RAIL_PREVIEW_MAX_HEIGHT: f32 = 126.0;
/// Detail JSON/outputs are truncated to keep one virtualized row bounded.
const DETAIL_TEXT_CAP: usize = 4000;
/// Tool *output* gets a much larger budget than arguments — logs are the
/// point of the Output section. The collapsed preview keeps the row small;
/// expanding paints up to [`OUTPUT_EXPANDED_PAINT_LINES`] lines.
const OUTPUT_TEXT_CAP: usize = 100_000;
/// A detail section taller than this collapses to a preview…
const OUTPUT_COLLAPSE_LINES: usize = 16;
/// …showing this many lines collapsed.
const OUTPUT_PREVIEW_LINES: usize = 12;
/// Even expanded, one virtualized row paints at most this many output lines
/// (the copy button always carries the full captured text).
const OUTPUT_EXPANDED_PAINT_LINES: usize = 400;
/// A settled code block taller than this collapses to a preview…
const CODE_COLLAPSE_LINES: usize = 28;
/// …showing this many lines collapsed. Live (streaming) rows never collapse
/// — a growing block's edge must stay visible.
const CODE_PREVIEW_LINES: usize = 24;
/// Code-block copy feedback shares the detail-section feedback map; section
/// `2` never collides with Arguments (`0`) / Output (`1`).
const CODE_COPY_SECTION: u8 = 2;
const CODE_COPY_BUTTON: f32 = 28.0;
/// Inline edit diff — an edit/write tool's `oldText`/`newText` shown as a
/// compact diff. Row height and text size track the Review pane's diff.
const EDIT_DIFF_TEXT_SIZE: f32 = 12.5;
const EDIT_DIFF_ROW_HEIGHT: f32 = 19.0;
/// Copy feedback for an inline edit diff; section `3` never collides with
/// Arguments (`0`) / Output (`1`) / code block (`2`).
const EDIT_DIFF_COPY_SECTION: u8 = 3;
/// A diff taller than this collapses to a preview so one virtualized row
/// stays bounded; expanding paints at most [`EDIT_DIFF_PAINT_LINES`].
const EDIT_DIFF_COLLAPSE_LINES: usize = 40;
const EDIT_DIFF_PREVIEW_LINES: usize = 24;
const EDIT_DIFF_PAINT_LINES: usize = 400;

type ExpandedActivities = Rc<RefCell<HashMap<(usize, usize), bool>>>;
type ExpandedTools = Rc<RefCell<HashSet<(usize, usize)>>>;
type CopiedSections = Rc<RefCell<HashMap<(usize, usize, u8), Instant>>>;
/// Tool detail sections expanded past their collapsed preview, keyed
/// `(message_ix, flat_tool_ix, section)`.
type ExpandedSections = Rc<RefCell<HashSet<(usize, usize, u8)>>>;
/// Code blocks expanded past their collapsed preview, keyed
/// `(message_ix, prose_salt, block_ix)`.
type ExpandedBlocks = Rc<RefCell<HashSet<(usize, u64, usize)>>>;

pub(crate) struct TranscriptView {
    pub messages: Rc<RefCell<Vec<ChatMessage>>>,
    pub scroller: MessageScrollerState,
    pub streaming: Rc<Cell<Option<usize>>>,
    pub stream_started: Rc<Cell<Option<Instant>>>,
    pub expanded_turns: Rc<RefCell<HashSet<usize>>>,
    pub expanded_files: Rc<RefCell<HashSet<usize>>>,
    pub expanded_activities: ExpandedActivities,
    /// Per-tool detail-card open state, keyed `(message_ix, tool_ix)`.
    pub expanded_tools: ExpandedTools,
    pub copied: Rc<RefCell<HashMap<usize, Instant>>>,
    /// Per detail-section copy feedback, keyed `(message_ix, tool_ix, section)`.
    pub copied_sections: CopiedSections,
    /// Tool output sections expanded past their collapsed preview.
    pub expanded_sections: ExpandedSections,
    /// Code blocks expanded past their collapsed preview.
    pub expanded_blocks: ExpandedBlocks,
    /// Rail tick currently hovered (drives the turn preview card).
    pub hovered_turn: Rc<Cell<Option<usize>>>,
    /// Assistant row whose footer usage metric is hovered (drives the
    /// per-message token/cost breakdown card).
    pub hovered_usage: Rc<Cell<Option<usize>>>,
    /// One-time rail hint: dismissal state (per install) + when it was
    /// first shown (drives its self-dismiss timeout).
    pub rail_hint_dismissed: Rc<Cell<bool>>,
    pub rail_hint_shown_at: Rc<Cell<Option<Instant>>>,
    /// Workspace of the open session — roots the Review git diff.
    pub workspace: Option<PathBuf>,
    /// Viewport height (caps the rail at 80%, like Waku).
    pub viewport_height: Pixels,
    /// Main-area width (gates the rail at 872px, like Waku).
    pub main_width: Pixels,
    /// Rail scroll position + last auto-scrolled turn.
    pub rail_scroll: ScrollHandle,
    pub rail_autoscroll: Rc<Cell<Option<usize>>>,
    /// End-of-task changed-files summary — `Some` renders one extra row
    /// after the last message (the list's tail slot).
    pub summary_files: Option<Vec<(String, u64, u64)>>,
    /// When the settled run finished (last message's timestamp) — shown in
    /// the summary card's footer next to the copy affordance.
    pub summary_finished_at: Option<i64>,
    /// The whole run's reported usage — carried onto the summary footer so
    /// the copy/time/usage details are shown once, on the last element,
    /// instead of duplicated above and below the changed-files card.
    pub summary_usage: Option<MessageUsage>,
    /// Opens the changed-files Review in the side pane (the cards'
    /// "Review" buttons); `None` hides the buttons.
    pub review_changes: Option<ReviewOpener>,
    /// Opens an attachment image in the full-window lightbox; `None` leaves
    /// the tiles non-interactive.
    pub image_opener: Option<ImageOpener>,
    /// Message indices matching the open find query; `None` when find is
    /// closed. Rows get a quiet wash.
    pub search_hits: Option<Rc<RefCell<HashSet<usize>>>>,
    /// The selected find hit (stronger wash + a ring); `None` when find is
    /// closed.
    pub search_active: Option<Rc<Cell<Option<usize>>>>,
}

struct RowPaint {
    messages: Rc<RefCell<Vec<ChatMessage>>>,
    ix: usize,
    row_count: usize,
    theme: Theme,
    live: bool,
    live_elapsed: Option<Duration>,
    fold_open: bool,
    copied: bool,
    /// True for the last message when the tail summary owns the footer, so
    /// the per-message footer is not drawn a second time above it.
    suppress_footer: bool,
    expanded_turns: Rc<RefCell<HashSet<usize>>>,
    expanded_activities: ExpandedActivities,
    copied_at: Rc<RefCell<HashMap<usize, Instant>>>,
    scroller: MessageScrollerState,
    expanded_tools: ExpandedTools,
    copied_sections: CopiedSections,
    expanded_sections: ExpandedSections,
    expanded_blocks: ExpandedBlocks,
    hovered_usage: Rc<Cell<Option<usize>>>,
    image_opener: Option<ImageOpener>,
    search_hit: bool,
    search_active: bool,
}

/// The last message's own footer is suppressed when the tail summary owns
/// the run footer (copy + time + usage), so those details render exactly once
/// — on the last element, after the changed-files card.
fn suppress_message_footer(has_summary: bool, ix: usize, row_count: usize) -> bool {
    has_summary && ix + 1 == row_count
}

pub(crate) fn render_transcript(view: TranscriptView, cx: &gpui::App) -> impl IntoElement + use<> {
    let theme = *theme::get(cx);
    let messages = view.messages.clone();
    let streaming = view.streaming.clone();
    let stream_started = view.stream_started.clone();
    let expanded_turns = view.expanded_turns.clone();
    let expanded_files = view.expanded_files.clone();
    let expanded_activities = view.expanded_activities.clone();
    let expanded_tools = view.expanded_tools.clone();
    let copied = view.copied.clone();
    let copied_sections = view.copied_sections.clone();
    let expanded_sections = view.expanded_sections.clone();
    let expanded_blocks = view.expanded_blocks.clone();
    let hovered_turn = view.hovered_turn.clone();
    let hovered_usage = view.hovered_usage.clone();
    let rail_hint_dismissed = view.rail_hint_dismissed.clone();
    let rail_hint_shown_at = view.rail_hint_shown_at.clone();
    let workspace = view.workspace.clone();
    let rail_scroll = view.rail_scroll.clone();
    let rail_autoscroll = view.rail_autoscroll.clone();
    let scroller = view.scroller.clone();
    let summary_files = view.summary_files.clone();
    let summary_finished_at = view.summary_finished_at;
    let summary_usage = view.summary_usage.clone();
    let review_changes = view.review_changes.clone();
    let image_opener = view.image_opener.clone();
    let search_hits = view.search_hits.clone();
    let search_active = view.search_active.clone();

    let (user_turns, active_turn) = {
        let messages = messages.borrow();
        let user_turns: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, message)| message.user)
            .map(|(ix, _)| ix)
            .collect();
        // While the reader sits at the live edge the indicator tracks the
        // streaming/latest turn; once they scroll away it follows the
        // viewport instead (fixes the tick staying pinned to the newest
        // turn while reading earlier ones).
        let viewport_hint = if scroller.is_following_tail() {
            None
        } else {
            Some(scroller.first_visible_index())
        };
        let active_turn = active_user_index(&messages, streaming.get(), viewport_hint);
        (user_turns, active_turn)
    };
    let show_rail = user_turns.len() >= 2
        && view.main_width >= px(NAVIGATION_RAIL_MIN_MAIN_WIDTH)
        && scroller.is_scrollable();
    // The one-time rail hint shows while the rail is up, until the reader
    // uses it (tick click or turn jump) or its TTL lapses. While it shows,
    // it takes the preview card's slot so the two never stack.
    let rail_hint_active = show_rail && !rail_hint_dismissed.get();
    if rail_hint_active && rail_hint_shown_at.get().is_none() {
        rail_hint_shown_at.set(Some(Instant::now()));
    }

    // One prompt/response preview pair per user turn (rail hover cards).
    let turn_snippets: Vec<(String, String)> = {
        let messages = messages.borrow();
        user_turns
            .iter()
            .map(|&ix| {
                let prompt = snippet(&messages[ix].text(), 100);
                let response = messages
                    .get(ix + 1..)
                    .unwrap_or(&[])
                    .iter()
                    .take_while(|message| !message.user)
                    .find(|message| !message.text().trim().is_empty())
                    .map(|message| snippet(&message.text(), 240))
                    .unwrap_or_default();
                (prompt, response)
            })
            .collect()
    };

    let list_el = list(view.scroller.list_state(), move |ix, _window, cx| {
        let live = streaming.get() == Some(ix);
        let live_elapsed = if live {
            stream_started.get().map(|started| started.elapsed())
        } else {
            None
        };
        let fold_open = expanded_turns.borrow().contains(&ix);
        let copied_now = copied
            .borrow()
            .get(&ix)
            .is_some_and(|at| at.elapsed() < COPY_FEEDBACK);
        let row_count = messages.borrow().len();
        // The list's tail slot (ix == row_count) is the end-of-task
        // changed-files summary — pinned after the last message, in the
        // same centered column as every other row.
        if ix >= row_count {
            let Some(files) = summary_files.as_ref() else {
                return div().into_any_element();
            };
            let theme = *theme::get(cx);
            let card = render_changed_files(
                files,
                theme,
                ix,
                workspace.as_deref(),
                expanded_files.borrow().contains(&ix),
                expanded_files.clone(),
                scroller.clone(),
                review_changes.clone(),
            );
            // One run footer, after the changed-files card (Waku turn order:
            // answer → files card → Copy): the copy affordance copies the
            // answer, with the settled time and the run's usage. The last
            // message's own footer is suppressed (see `suppress_footer`) so
            // these details are not shown twice.
            let last_ix = row_count.saturating_sub(1);
            let copy_text = messages
                .borrow()
                .get(last_ix)
                .filter(|message| !message.user)
                .map(|message| message.text())
                .unwrap_or_default();
            let copied_last = copied
                .borrow()
                .get(&last_ix)
                .is_some_and(|at| at.elapsed() < COPY_FEEDBACK);
            let stamp = summary_finished_at
                .map(|millis| footer_time_stamp(summary_time_label(millis), theme));
            let footer = div().mt(px(10.)).px(px(4.)).child(render_message_footer(
                copy_text,
                last_ix,
                stamp,
                summary_usage.clone(),
                copied_last,
                false,
                theme,
                copied.clone(),
                hovered_usage.clone(),
            ));
            return div()
                .id(ElementId::NamedInteger("transcript-row".into(), ix as u64))
                .w_full()
                .flex()
                .justify_center()
                .px(px(20.))
                .pb(px(22.))
                .child(
                    div()
                        .w_full()
                        .max_w(px(CONTENT_MAX_WIDTH))
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(card)
                        .child(footer),
                )
                .into_any_element();
        }
        render_row(RowPaint {
            messages: messages.clone(),
            ix,
            row_count,
            theme: *theme::get(cx),
            live,
            live_elapsed,
            fold_open,
            copied: copied_now,
            suppress_footer: suppress_message_footer(summary_files.is_some(), ix, row_count),
            expanded_turns: expanded_turns.clone(),
            expanded_activities: expanded_activities.clone(),
            copied_at: copied.clone(),
            scroller: scroller.clone(),
            expanded_tools: expanded_tools.clone(),
            copied_sections: copied_sections.clone(),
            expanded_sections: expanded_sections.clone(),
            expanded_blocks: expanded_blocks.clone(),
            hovered_usage: hovered_usage.clone(),
            image_opener: image_opener.clone(),
            search_hit: search_hits
                .as_ref()
                .is_some_and(|hits| hits.borrow().contains(&ix)),
            search_active: search_active
                .as_ref()
                .is_some_and(|active| active.get() == Some(ix)),
        })
        .into_any_element()
    })
    .w_full()
    .h_full();

    // Same height chain as the sessions sidebar: this panel is a `flex_1` +
    // `min_h_0` child of a column, so `h_full` on the list is a definite
    // height. Jump-to-latest and the bottom fade live on the scroller wrap.
    let scroller = message_scroller::render_scroller(view.scroller.clone(), theme, list_el);
    div()
        .id(ElementId::Name("transcript-panel".into()))
        .w_full()
        .min_w_0()
        .h_full()
        .min_h_0()
        .relative()
        .child(scroller)
        .when(show_rail, |shell| {
            shell.child(render_navigation_rail(
                turn_snippets,
                user_turns,
                active_turn,
                hovered_turn.clone(),
                view.scroller.clone(),
                view.viewport_height,
                rail_scroll,
                rail_autoscroll,
                theme,
                rail_hint_active,
                rail_hint_dismissed,
                rail_hint_shown_at,
            ))
        })
}

/// Waku's conversation rail: ticks per user turn, vertically centered,
/// capped at 80% of the viewport, wheel-scrollable with edge fades. Tick
/// widths fan out around the *hovered* turn only; the active turn is the
/// full-opacity tick while idle.
#[allow(clippy::too_many_arguments)]
fn render_navigation_rail(
    snippets: Vec<(String, String)>,
    user_turns: Vec<usize>,
    active_turn: Option<usize>,
    hovered: Rc<Cell<Option<usize>>>,
    scroller: MessageScrollerState,
    viewport_height: Pixels,
    rail_scroll: ScrollHandle,
    rail_autoscroll: Rc<Cell<Option<usize>>>,
    theme: Theme,
    rail_hint_active: bool,
    rail_hint_dismissed: Rc<Cell<bool>>,
    rail_hint_shown_at: Rc<Cell<Option<Instant>>>,
) -> AnyElement {
    let pitch = px(NAVIGATION_RAIL_TURN_HEIGHT);
    let content_height = px(user_turns.len() as f32 * NAVIGATION_RAIL_TURN_HEIGHT);
    let rail_height = content_height.min(viewport_height * NAVIGATION_RAIL_MAX_HEIGHT);
    let scrollable = content_height > rail_height + px(1.);
    let scroll_offset = rail_scroll.offset().y;

    // Waku scrolls the active tick into view whenever it changes.
    let active_pos =
        active_turn.and_then(|ix| user_turns.iter().position(|candidate| *candidate == ix));
    if scrollable {
        if let Some(pos) = active_pos {
            let top = px(pos as f32 * NAVIGATION_RAIL_TURN_HEIGHT);
            let visible_bottom = scroll_offset + rail_height;
            if rail_autoscroll.get() != Some(pos)
                && (top < scroll_offset || top + pitch > visible_bottom)
            {
                let target = (top + pitch / 2. - rail_height / 2.)
                    .clamp(px(0.), content_height - rail_height);
                rail_scroll.set_offset(point(px(0.), target));
            }
            rail_autoscroll.set(Some(pos));
        }
    }
    let scroll_offset = rail_scroll.offset().y;
    let at_top = !scrollable || scroll_offset <= px(0.5);
    let at_bottom = !scrollable || scroll_offset >= content_height - rail_height - px(0.5);

    // The emphasized (hovered) turn anchors the width fan-out; with nothing
    // hovered every tick rests at the 0.25 scale, like Waku's idle rail.
    let emphasized_turn = hovered.get();
    let emphasized_pos =
        emphasized_turn.and_then(|ix| user_turns.iter().position(|candidate| *candidate == ix));
    let hover_info =
        emphasized_pos.map(|pos| (pos, snippets[pos].0.clone(), snippets[pos].1.clone()));

    let ticks: Vec<(usize, String, f32, Hsla)> = user_turns
        .iter()
        .copied()
        .zip(snippets.iter().cloned())
        .enumerate()
        .map(|(turn_pos, (user_ix, (prompt, _response)))| {
            let distance = emphasized_pos.map(|pos| pos.abs_diff(turn_pos));
            let scale = distance
                .and_then(|d| NAVIGATION_RAIL_EMPHASIS_SCALE.get(d))
                .copied()
                .unwrap_or(0.25);
            let prominent = emphasized_turn == Some(user_ix) || active_turn == Some(user_ix);
            let width = NAVIGATION_RAIL_TICK_WIDTH * scale;
            let color = if prominent {
                theme.text
            } else {
                theme.text_3.opacity(NAVIGATION_RAIL_INACTIVE_OPACITY)
            };
            (user_ix, prompt, width, color)
        })
        .collect();

    let shell_hover = hovered.clone();
    // The ticks map consumes one clone of the hint cells; the hint card at
    // the end of this function needs its own.
    let tick_hint_dismissed = rail_hint_dismissed.clone();
    let tick_hint_shown_at = rail_hint_shown_at.clone();
    let mut body = div()
        .relative()
        .w(px(NAVIGATION_RAIL_WIDTH))
        .h(rail_height)
        .child(
            div()
                .id(ElementId::Name("rail-ticks".into()))
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .overflow_y_scroll()
                .track_scroll(&rail_scroll)
                .flex()
                .flex_col()
                .children(
                    ticks
                        .into_iter()
                        .map(move |(user_ix, _prompt, width, color)| {
                            let scroller = scroller.clone();
                            let hover_state = hovered.clone();
                            let tick_hint_dismissed = tick_hint_dismissed.clone();
                            let tick_hint_shown_at = tick_hint_shown_at.clone();
                            div()
                                .id(ElementId::NamedInteger("nav-tick".into(), user_ix as u64))
                                .w(px(NAVIGATION_RAIL_WIDTH))
                                .h(px(NAVIGATION_RAIL_TURN_HEIGHT))
                                .flex_none()
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .on_hover({
                                    let hover_state = hover_state.clone();
                                    move |hovering: &bool, _, cx| {
                                        // On leave, clear only when this tick
                                        // owns the hover state (moving between
                                        // ticks hands it to the next one).
                                        let next = if *hovering {
                                            Some(user_ix)
                                        } else if hover_state.get() == Some(user_ix) {
                                            None
                                        } else {
                                            hover_state.get()
                                        };
                                        if hover_state.get() != next {
                                            hover_state.set(next);
                                            cx.refresh_windows();
                                        }
                                    }
                                })
                                .on_click({
                                    move |_, _, cx| {
                                        // Using the rail is the lesson — dismiss the
                                        // one-time hint (persists per install).
                                        crate::transcript::dismiss_rail_hint_state(
                                            &tick_hint_dismissed,
                                            &tick_hint_shown_at,
                                        );
                                        scroller.scroll_to_item(user_ix);
                                        cx.refresh_windows();
                                    }
                                })
                                .child(
                                    div()
                                        .id(ElementId::NamedInteger(
                                            "nav-tick-bar".into(),
                                            user_ix as u64,
                                        ))
                                        .h(px(NAVIGATION_RAIL_TICK_HEIGHT))
                                        .w(px(width))
                                        .rounded_full()
                                        .bg(color)
                                        .hover(|style| style.bg(theme.text)),
                                )
                        }),
                ),
        )
        .when(!at_top, |rail| rail.child(render_rail_fade(true, theme)))
        .when(!at_bottom, |rail| {
            rail.child(render_rail_fade(false, theme))
        });

    // Hover preview, clamped inside the rail body's vertical span (Waku
    // clamps `previewTop` against the rail bounds). While the one-time hint
    // is up it owns this slot, so the two floating cards never stack.
    if let Some((pos, prompt, response)) = hover_info {
        let visible_center =
            px(pos as f32 * NAVIGATION_RAIL_TURN_HEIGHT) + pitch / 2. - scroll_offset;
        let preview_top = (visible_center - px(NAVIGATION_RAIL_PREVIEW_MAX_HEIGHT / 2.))
            .max(px(0.))
            .min((rail_height - px(NAVIGATION_RAIL_PREVIEW_MAX_HEIGHT)).max(px(0.)));
        body = body.child(render_rail_preview(
            &prompt,
            &response,
            theme,
            preview_top,
            pos + 1,
            user_turns.len(),
        ));
    } else if rail_hint_active {
        body = body.child(render_rail_hint(
            theme,
            rail_hint_dismissed,
            rail_hint_shown_at,
        ));
    }

    div()
        .id(ElementId::Name("conversation-navigation-rail".into()))
        .absolute()
        .top_0()
        .left(px(NAVIGATION_RAIL_LEFT))
        .w(px(NAVIGATION_RAIL_WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .justify_center()
        // Belt and braces: leaving the rail entirely clears the preview even
        // if an individual tick's leave event was missed (e.g. scrolling).
        .on_hover({
            let hover_state = shell_hover;
            move |hovering: &bool, _, cx| {
                if !hovering && hover_state.get().is_some() {
                    hover_state.set(None);
                    cx.refresh_windows();
                }
            }
        })
        .child(body)
        .into_any_element()
}

/// Waku's rail edge fade: a 20px gradient from the background so scrolling
/// ticks dissolve instead of clipping.
fn render_rail_fade(top: bool, theme: Theme) -> impl IntoElement {
    let base = div()
        .absolute()
        .left_0()
        .right_0()
        .h(px(20.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(theme.bg_main.opacity(0.), 0.),
            linear_color_stop(theme.bg_main, 1.),
        ));
    if top {
        // Solid at the top edge, fading down: flip the stop order.
        base.top_0().bg(linear_gradient(
            180.,
            linear_color_stop(theme.bg_main, 0.),
            linear_color_stop(theme.bg_main.opacity(0.), 1.),
        ))
    } else {
        base.bottom_0()
    }
}

/// Waku's rail hover card: the turn's number, the turn's prompt, and a
/// short response snippet, vertically positioned by the caller (clamped to
/// the rail's span) at Waku's 60px left offset (rail width + gap).
fn render_rail_preview(
    prompt: &str,
    response: &str,
    theme: Theme,
    top: Pixels,
    turn_number: usize,
    turn_total: usize,
) -> impl IntoElement {
    div()
        .absolute()
        .left(px(NAVIGATION_RAIL_WIDTH + NAVIGATION_RAIL_CONTENT_GAP))
        .top(top)
        .w(px(NAVIGATION_RAIL_PREVIEW_WIDTH))
        .max_h(px(NAVIGATION_RAIL_PREVIEW_MAX_HEIGHT))
        .overflow_hidden()
        .rounded(px(14.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.bg_raised)
        .shadow(theme.card_shadow())
        .px(px(15.))
        .py(px(12.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(
            div()
                .text_size(theme.ui_px(10.5))
                .line_height(theme.ui_px(14.))
                .text_color(theme.text_3)
                .child(format!(
                    "Turn {} of {} · click to jump",
                    turn_number, turn_total
                )),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .text_size(theme.ui_px(14.))
                .line_height(theme.ui_px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(prompt.to_string()),
        )
        .when(!response.is_empty(), |card| {
            card.child(
                div()
                    .w_full()
                    .max_h(px(60.))
                    .overflow_hidden()
                    .whitespace_normal()
                    .text_size(theme.ui_px(13.))
                    .line_height(theme.ui_px(20.))
                    .text_color(theme.text_3)
                    .child(response.to_string()),
            )
        })
}

/// One-time affordance hint beside the rail: teaches what the ticks are,
/// that they jump, and the keyboard mirror — then never shows again on
/// this install (dismissed by use, click, or TTL).
fn render_rail_hint(
    theme: Theme,
    rail_hint_dismissed: Rc<Cell<bool>>,
    rail_hint_shown_at: Rc<Cell<Option<Instant>>>,
) -> impl IntoElement {
    div()
        .id(ElementId::Name("rail-hint".into()))
        .absolute()
        .left(px(NAVIGATION_RAIL_WIDTH + NAVIGATION_RAIL_CONTENT_GAP))
        .top_0()
        .w(px(216.))
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.bg_raised)
        .shadow(theme.card_shadow())
        .px(px(13.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(3.))
        .cursor_pointer()
        .child(
            div()
                .text_size(theme.ui_px(12.5))
                .line_height(theme.ui_px(16.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child("Jump between turns"),
        )
        .child(
            div()
                .text_size(theme.ui_px(11.5))
                .line_height(theme.ui_px(16.))
                .text_color(theme.text_3)
                .child(format!(
                    "Click a line — or press {} — to revisit any prompt.",
                    crate::platform::shortcuts::TURNS
                )),
        )
        .on_click(move |_, _, cx| {
            crate::transcript::dismiss_rail_hint_state(&rail_hint_dismissed, &rail_hint_shown_at);
            cx.refresh_windows();
        })
}

/// Whitespace-normalized grapheme snippet
/// (same presentation Waku's navigation previews use).
fn snippet(text: &str, max_graphemes: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut graphemes = normalized.graphemes(true);
    let head: String = graphemes.by_ref().take(max_graphemes).collect();
    if graphemes.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn render_row(paint: RowPaint) -> AnyElement {
    let messages = paint.messages.borrow();
    let Some(message) = messages.get(paint.ix) else {
        return div().into_any_element();
    };
    let followup = starts_followup_turn(&messages, paint.ix);
    let first = paint.ix == 0;
    let last = paint.ix + 1 == paint.row_count;

    let inner = if message.user {
        render_user_bubble(message, &paint).into_any_element()
    } else {
        render_assistant(message, &paint).into_any_element()
    };

    div()
        .id(ElementId::NamedInteger(
            "transcript-row".into(),
            paint.ix as u64,
        ))
        .w_full()
        .flex()
        .justify_center()
        .px(px(20.))
        .py(px(8.))
        .when(first, |row| row.pt(px(22.)))
        .when(followup, |row| row.pt(px(FOLLOWUP_TURN_TOP_GAP)))
        .when(last, |row| row.pb(px(22.)))
        // In-transcript find: matched rows get a quiet wash; the selected
        // hit is stronger. Painted on the full-width row so it reads as a
        // band, like a browser's find.
        .when(paint.search_hit, |row| {
            row.bg(paint.theme.overlay.opacity(0.5))
        })
        .when(paint.search_active, |row| {
            row.bg(paint.theme.accent.opacity(0.14))
        })
        .child(
            div()
                .w_full()
                .max_w(px(CONTENT_MAX_WIDTH))
                .min_w_0()
                .child(inner),
        )
        .into_any_element()
}

/// End-aligned user row: Waku's neutral raised bubble, persistent quiet
/// footer below. Attached images render as a tile grid above the text
/// bubble.
fn render_user_bubble(message: &ChatMessage, paint: &RowPaint) -> impl IntoElement {
    let theme = paint.theme;
    let ix = paint.ix;
    let text = message.text();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.))
        // Attachment tiles (images queued with the prompt). Cover-cropped
        // squares, Waku-style; wrap when a message carries several.
        .when(!message.images.is_empty(), |column| {
            column.child(
                div()
                    .max_w(px(USER_BUBBLE_MAX_WIDTH))
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(6.))
                    .children(message.images.iter().enumerate().map(|(image_ix, image)| {
                        let image = image.clone();
                        let opener = paint.image_opener.clone();
                        div()
                            .id(ElementId::NamedInteger(
                                "user-image".into(),
                                (ix as u64) << 20 | image_ix as u64,
                            ))
                            .size(px(112.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme.border)
                            .overflow_hidden()
                            .bg(theme.bg_raised)
                            .when(opener.is_some(), |tile| {
                                tile.cursor_pointer()
                                    .hover(|s| s.border_color(theme.border_strong))
                            })
                            .child(
                                img(ImageSource::Image(image.clone()))
                                    .size_full()
                                    .object_fit(ObjectFit::Cover),
                            )
                            .on_click(move |_, window, cx| {
                                if let Some(opener) = &opener {
                                    opener(image.clone(), window, cx);
                                }
                            })
                    })),
            )
        })
        .when(!text.is_empty(), |column| {
            column.child(
                div()
                    .max_w(px(USER_BUBBLE_MAX_WIDTH))
                    .rounded(px(12.))
                    .bg(theme.bg_raised)
                    .text_color(theme.text)
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(theme.ui_px(14.))
                    .whitespace_normal()
                    .child(render_prose(
                        &text,
                        ix,
                        0,
                        theme,
                        paint.copied_sections.clone(),
                        paint.expanded_blocks.clone(),
                        true,
                        paint.scroller.clone(),
                    )),
            )
        })
        .child(render_message_footer(
            text,
            ix,
            message
                .finished_at
                .and_then(format_time)
                .map(|time| footer_time_stamp(time, theme)),
            message.usage(),
            paint.copied,
            true,
            theme,
            paint.copied_at.clone(),
            paint.hovered_usage.clone(),
        ))
}

fn render_assistant(message: &ChatMessage, paint: &RowPaint) -> impl IntoElement {
    let theme = paint.theme;
    let ix = paint.ix;
    let mut content = div()
        .w_full()
        .max_w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .items_start()
        .gap(theme.space(CONTENT_GAP));

    // The turn fold precedes the run's first work (Waku: collapsed turns
    // hide the pre-answer work behind a single "Worked for" divider).
    if !paint.live && message.has_hidden_work() {
        content = content.child(render_turn_fold(
            ix,
            paint.fold_open,
            message.elapsed,
            theme,
            paint.expanded_turns.clone(),
            paint.scroller.clone(),
        ));
    }

    // Steps render in sequence: each step's thought/tool group sits directly
    // above the text it produced (Waku's interleaved activity rows).
    let answer_start = message
        .steps
        .iter()
        .position(|step| !step.text.trim().is_empty());
    let last_step = message.steps.len().saturating_sub(1);
    // Running count of tools across the message's steps — the flat index
    // space the per-tool detail/copy keys use (stable across steps).
    let mut tool_base = 0usize;

    for (step_ix, step) in message.steps.iter().enumerate() {
        let before_answer = answer_start.is_none_or(|answer| step_ix < answer);
        let is_live_step = paint.live && step_ix == last_step;
        let has_work = !step.thinking.is_empty() || !step.tools.is_empty();

        if has_work {
            // Pre-answer work hides behind the turn fold; later bursts stay
            // visible as collapsed, expandable groups.
            let show_work = paint.live || paint.fold_open || !before_answer;
            if show_work {
                let open = paint
                    .expanded_activities
                    .borrow()
                    .get(&(ix, step_ix))
                    .copied()
                    .unwrap_or(is_live_step);
                content = content.child(render_step_group(
                    ix,
                    step_ix,
                    tool_base,
                    step,
                    open,
                    is_live_step,
                    paint.live_elapsed.unwrap_or(Duration::ZERO),
                    theme,
                    paint.expanded_activities.clone(),
                    paint.expanded_tools.clone(),
                    paint.copied_sections.clone(),
                    paint.expanded_sections.clone(),
                    paint.scroller.clone(),
                ));
            }
        }
        tool_base += step.tools.len();

        if !step.text.is_empty() {
            // Live rows never collapse code blocks — a growing block's tail
            // edge must stay visible while it streams.
            content = content.child(div().w_full().min_w_0().pt(px(4.)).child(render_prose(
                &step.text,
                ix,
                (step_ix as u64 + 1) * 4096,
                theme,
                paint.copied_sections.clone(),
                paint.expanded_blocks.clone(),
                !paint.live,
                paint.scroller.clone(),
            )));
        }
    }

    // Changed files render ONLY in the end-of-task summary card pinned
    // after the last row (once the run settles) — not per message.

    // A turn that ended in a provider/agent error (e.g. an unsupported
    // model). pi carries the message in `errorMessage`; render it so a
    // failed turn is never an empty row.
    if let Some(error) = &message.error {
        content = content.child(render_assistant_error(error, theme));
    }

    // A cancelled turn keeps its partial content and says so, quietly —
    // "Stopped", never an error card (the user asked for the stop).
    if message.aborted && !paint.live {
        content = content.child(render_stopped_marker(theme));
    }

    if paint.live {
        let activity = message.steps.last().and_then(working_activity_label);
        content = content.child(render_working_indicator(
            paint.live_elapsed.unwrap_or(Duration::ZERO),
            activity,
            theme,
        ));
    }

    let mut row = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .items_start()
        .child(content);

    if !paint.live && !paint.suppress_footer && !message.text().is_empty() {
        row = row.child(render_message_footer(
            message.text(),
            ix,
            message
                .finished_at
                .and_then(format_time)
                .map(|time| footer_time_stamp(time, theme)),
            message.usage(),
            paint.copied,
            false,
            theme,
            paint.copied_at.clone(),
            paint.hovered_usage.clone(),
        ));
    }

    row
}

/// A settled assistant turn that failed: the provider/agent `errorMessage`
/// pi carries on the message, as a red card. Mirrors the app-level error
/// banner so the failure is visible in context too.
fn render_assistant_error(error: &str, theme: Theme) -> AnyElement {
    div()
        .w_full()
        .max_w_full()
        .min_w_0()
        .bg(theme.crit.opacity(0.1))
        .border_1()
        .border_color(theme.crit.opacity(0.45))
        .rounded_lg()
        .px(px(12.))
        .py(px(9.))
        .flex()
        .items_start()
        .gap_2p5()
        .child(glyph("icons/info.svg", 15., theme.crit))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_size(theme.ui_px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child("Agent error"),
                )
                .child(
                    div()
                        .text_size(theme.ui_px(12.5))
                        .text_color(theme.text_2)
                        .whitespace_normal()
                        .child(error.to_string()),
                ),
        )
        .into_any_element()
}

/// Waku reasoning row: "Thinking" while live, "Thought for <duration>" once
/// settled; the raw reasoning text is expandable detail (auto-open live).
#[allow(clippy::too_many_arguments)]
/// One step's activity group: a collapsed "Ran 7 commands · 4 thoughts"
/// summary line (Waku) that expands into the thought card and the step's
/// tool rows.
/// Waku's activity summary: counts the step's tool kinds and thoughts —
/// "Ran 7 commands · 4 thoughts", "Ran 1 file read · 1 thought".
fn step_activity_title(step: &Step, live: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    let (mut commands, mut reads, mut edits, mut other) = (0usize, 0usize, 0usize, 0usize);
    for tool in &step.tools {
        match tool.name.as_str() {
            "bash" | "shell" => commands += 1,
            "read" | "grep" | "find" | "glob" | "search" => reads += 1,
            "edit" | "write" => edits += 1,
            _ => other += 1,
        }
    }
    let units = |n: usize, word: &str| format!("{} {}{}", n, word, if n == 1 { "" } else { "s" });
    if commands > 0 {
        parts.push(format!("Ran {}", units(commands, "command")));
    }
    if reads > 0 {
        parts.push(format!("Ran {}", units(reads, "file read")));
    }
    if edits > 0 {
        parts.push(format!("Ran {}", units(edits, "file edit")));
    }
    if other > 0 {
        parts.push(format!("Ran {}", units(other, "tool")));
    }
    if !step.thinking.is_empty() {
        parts.push(if live {
            "Thinking".to_string()
        } else {
            units(1, "thought")
        });
    }
    if parts.is_empty() {
        return if live {
            "Working".into()
        } else {
            "Worked".into()
        };
    }
    parts.join(" \u{b} ")
}

#[allow(clippy::too_many_arguments)]
fn render_step_group(
    ix: usize,
    step_ix: usize,
    tool_base: usize,
    step: &Step,
    open: bool,
    live: bool,
    elapsed: Duration,
    theme: Theme,
    expanded_activities: ExpandedActivities,
    expanded_tools: ExpandedTools,
    copied_sections: CopiedSections,
    expanded_sections: ExpandedSections,
    scroller: MessageScrollerState,
) -> impl IntoElement {
    let title = step_activity_title(step, live);
    let last = step.tools.len().saturating_sub(1);
    let mut group = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .id(ElementId::NamedInteger(
                    "activity-toggle".into(),
                    (ix as u64) << 16 | step_ix as u64,
                ))
                .w_full()
                .min_w_0()
                .h(px(26.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .text_size(theme.ui_px(12.5))
                .line_height(theme.ui_px(16.))
                .hover(|style| style.text_color(theme.text))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_2)
                        .child(title),
                )
                .child(glyph(
                    if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    10.,
                    theme.text_3,
                ))
                .on_click({
                    let expanded_activities = expanded_activities.clone();
                    let scroller = scroller.clone();
                    move |_, _, cx| {
                        let key = (ix, step_ix);
                        let mut map = expanded_activities.borrow_mut();
                        let next = !map.get(&key).copied().unwrap_or(false);
                        map.insert(key, next);
                        scroller.remeasure_toggle(ix);
                        cx.refresh_windows();
                    }
                }),
        );

    if open {
        let mut body = div()
            .w_full()
            .min_w_0()
            .ml(px(6.))
            .pl(px(12.))
            .pb(px(2.))
            .border_l_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(8.));
        if !step.thinking.is_empty() {
            body = body.child(render_thinking_body(&step.thinking, live, theme));
        }
        body = body.children(step.tools.iter().enumerate().map(|(tool_ix, tool)| {
            let flat = tool_base + tool_ix;
            render_activity_card(
                tool,
                live && tool_ix == last,
                !live,
                elapsed,
                theme,
                (ix, flat),
                expanded_tools.borrow().contains(&(ix, flat)),
                expanded_tools.clone(),
                copied_sections.clone(),
                expanded_sections.clone(),
                scroller.clone(),
            )
        }));
        group = group.child(body);
    }

    group
}

/// The reasoning card body inside a step group ("Thinking" live, "Thought
/// for <duration>" settled).
fn render_thinking_body(thinking: &str, live: bool, theme: Theme) -> impl IntoElement {
    let title = if live {
        "Thinking".to_string()
    } else {
        "Thought".to_string()
    };
    let detail = cap_chars(thinking, DETAIL_TEXT_CAP);
    div()
        .rounded(px(9.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.overlay)
        .px(px(12.))
        .py(px(8.))
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(theme.ui_px(13.))
                .line_height(theme.ui_px(17.))
                .child(glyph("icons/spark.svg", 13., theme.text_3))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.accent)
                        .child(title),
                ),
        )
        .child(
            div()
                .font_family(theme::code_font_family())
                .text_size(theme.code_px(12.))
                .line_height(theme.code_px(18.))
                .text_color(theme.tool_meta)
                .whitespace_normal()
                .child(detail),
        )
}

/// The `ask_user_question` card: a compact question record for the transcript.
/// While the tool waits the options read as a plain numbered list; once it
/// answers, the chosen option is marked (or the typed custom answer shown).
/// The live, answerable version is the inline panel above the composer.
fn render_ask_card(tool: &ToolCall, theme: Theme, key: (usize, usize)) -> AnyElement {
    use crate::ask;

    let questions = ask::questions_from_args(tool.args.as_ref());
    let answered = ask::answer_envelope(tool.output.as_ref());
    let declined = ask::is_declined(tool.output.as_ref());
    let waiting = tool.output.is_none() && !tool.failed;

    let mut card = div()
        .id(ElementId::NamedInteger(
            "ask-card".into(),
            (key.0 as u64) << 16 | key.1 as u64,
        ))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(9.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.overlay)
        .flex()
        .flex_col();

    // ── header ──
    let mut header = div()
        .h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(theme.ui_px(13.))
        .line_height(theme.ui_px(17.))
        .child(glyph("icons/task.svg", 13., theme.text_3))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.accent)
                .child("Question"),
        );
    if questions.len() > 1 {
        header = header.child(
            div()
                .flex_none()
                .text_size(theme.ui_px(11.))
                .text_color(theme.text_3)
                .child(format!("{} asked", questions.len())),
        );
    }
    if declined {
        header = header.child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .text_size(theme.ui_px(11.))
                .text_color(theme.text_3)
                .child("No answer"),
        );
    } else if tool.failed {
        header = header.child(div().flex_1().flex().justify_end().child(glyph(
            "icons/stop.svg",
            12.,
            theme.del_red,
        )));
    }
    card = card.child(header);

    // ── body: one block per question ──
    let mut body = div().px(px(10.)).pb(px(10.)).flex().flex_col().gap(px(10.));
    for question in &questions {
        let mut block = div().flex().flex_col().gap(px(4.));
        let mut head = div().flex().items_center().gap(px(6.));
        if !question.header.trim().is_empty() {
            head = head.child(
                div()
                    .flex_none()
                    .px(px(6.))
                    .py(px(1.))
                    .rounded(px(4.))
                    .bg(theme.overlay_strong)
                    .text_size(theme.ui_px(10.5))
                    .line_height(theme.ui_px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child(SharedString::from(question.header.clone())),
            );
        }
        head = head.child(
            div()
                .min_w_0()
                .flex_1()
                .whitespace_normal()
                .text_size(theme.ui_px(13.))
                .line_height(theme.ui_px(18.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(SharedString::from(question.question.clone())),
        );
        block = block.child(head);

        // The answer, when the tool completed with one. A selection is marked
        // in the list below; a typed custom answer gets its own row.
        let answer =
            answered.and_then(|envelope| ask::envelope_answer(envelope, &question.question));
        let custom = answer.filter(|answer| !ask::answer_is_option_selection(question, answer));

        let mut list = div().flex().flex_col().gap(px(1.));
        for (ix, option) in question.options.iter().enumerate() {
            let chosen =
                answer.is_some_and(|answer| ask::answer_includes_label(answer, &option.label));
            let mut row = div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(6.))
                .when(chosen, |row| row.bg(theme.accent.opacity(0.10)));
            // A plain number keeps the list readable at a glance.
            row = row.child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(12.))
                    .line_height(theme.ui_px(17.))
                    .text_color(if chosen { theme.accent } else { theme.text_3 })
                    .child(format!("{}.", ix + 1)),
            );
            row = row.child(
                div()
                    .min_w_0()
                    .flex_1()
                    .whitespace_normal()
                    .text_size(theme.ui_px(12.5))
                    .line_height(theme.ui_px(17.))
                    .text_color(if chosen { theme.text } else { theme.text_2 })
                    .child(SharedString::from(option.label.clone())),
            );
            if chosen {
                row = row.child(glyph("icons/check.svg", 11., theme.accent));
            }
            list = list.child(row);
        }
        if let Some(answer) = custom {
            list = list.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .bg(theme.accent.opacity(0.10))
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme.ui_px(12.))
                            .line_height(theme.ui_px(17.))
                            .text_color(theme.accent)
                            .child("A."),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .whitespace_normal()
                            .text_size(theme.ui_px(12.5))
                            .line_height(theme.ui_px(17.))
                            .text_color(theme.text)
                            .child(SharedString::from(answer.to_string())),
                    ),
            );
        }
        block = block.child(list);

        if waiting {
            block = block.child(
                div()
                    .px(px(8.))
                    .text_size(theme.ui_px(11.))
                    .line_height(theme.ui_px(15.))
                    .text_color(theme.text_3)
                    .child("Waiting for an answer…"),
            );
        }
        body = body.child(block);
    }
    card = card.child(body);
    card.into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_activity_card(
    tool: &ToolCall,
    pulse: bool,
    complete: bool,
    elapsed: Duration,
    theme: Theme,
    key: (usize, usize),
    tool_open: bool,
    expanded_tools: ExpandedTools,
    copied_sections: CopiedSections,
    expanded_sections: ExpandedSections,
    scroller: MessageScrollerState,
) -> AnyElement {
    // A structured questionnaire gets its own card — the raw arguments JSON
    // and the generic "Command/Arguments" detail would bury the question.
    if crate::ask::is_ask_tool(&tool.name)
        && !crate::ask::questions_from_args(tool.args.as_ref()).is_empty()
    {
        return render_ask_card(tool, theme, key);
    }
    let action = activity_action_label(&tool.name);
    let detail = activity_preview(tool);
    let is_command = tool_command(tool).is_some();
    let has_diff = tool.added > 0 || tool.removed > 0;
    let added = tool.added;
    let removed = tool.removed;
    // Expandable like Waku's activity rows: full arguments and, when captured
    // live, the tool result.
    let has_detail =
        tool.args.as_ref().is_some_and(|args| !args.is_null()) || tool.output.is_some();
    // An edit/write tool carries its own change; the header copy button and
    // the expanded body both read from it.
    let diff = edit_diff(tool);

    let mut card = div()
        .id(ElementId::NamedInteger(
            "activity-card".into(),
            (key.0 as u64) << 16 | key.1 as u64,
        ))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(9.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.overlay)
        .child(
            div()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(theme.ui_px(13.))
                .line_height(theme.ui_px(17.))
                .when(has_detail, |row| row.cursor_pointer())
                .hover(|style| style.bg(theme.overlay_strong))
                .child(glyph(activity_icon(&tool.name), 13., theme.text_3))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.accent)
                        .child(action),
                )
                .when(!detail.is_empty(), |row| {
                    if is_command {
                        // Terminal preview: a muted prompt sigil then the
                        // command in the code face, syntax-colored exactly
                        // like the expanded detail.
                        let tokens = highlight::tokenize_cached(highlight::Lang::Shell, &detail);
                        row.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .font_family(theme::code_font_family())
                                .text_size(theme.code_px(12.))
                                .child(div().flex_none().text_color(theme.text_3).child("$"))
                                .child(
                                    div().min_w_0().overflow_hidden().whitespace_nowrap().child(
                                        syntax_styled(
                                            &detail,
                                            Some(tokens.as_ref()),
                                            0,
                                            theme.tool_meta,
                                            theme,
                                        ),
                                    ),
                                ),
                        )
                    } else {
                        row.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(theme.tool_meta)
                                .child(detail),
                        )
                    }
                })
                .when(has_diff, |row| {
                    row.child(render_line_delta(added, removed, theme, 12.5))
                })
                .when(pulse, |row| {
                    row.child(pulse_dot(theme, elapsed.as_millis()))
                })
                .when(tool.failed, |row| {
                    row.child(glyph("icons/stop.svg", 12., theme.del_red))
                })
                .when(complete && !tool.failed, |row| {
                    row.child(glyph("icons/check.svg", 11., theme.text_3))
                })
                .when_some(diff.clone(), |row, rows| {
                    let copied = copied_sections
                        .borrow()
                        .get(&(key.0, key.1, EDIT_DIFF_COPY_SECTION))
                        .is_some_and(|at| at.elapsed() < COPY_FEEDBACK);
                    let patch = edit_patch_text(&rows);
                    let copied_sections = copied_sections.clone();
                    row.child(
                        div()
                            .id(ElementId::NamedInteger(
                                "copy-edit-diff".into(),
                                (key.0 as u64) << 16 | key.1 as u64,
                            ))
                            .flex_none()
                            .size(px(20.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|style| style.bg(theme.overlay_strong))
                            .child(glyph(
                                if copied {
                                    "icons/check.svg"
                                } else {
                                    "icons/copy.svg"
                                },
                                12.,
                                if copied { theme.ok_green } else { theme.text_3 },
                            ))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(patch.clone()));
                                copied_sections
                                    .borrow_mut()
                                    .insert((key.0, key.1, EDIT_DIFF_COPY_SECTION), Instant::now());
                                cx.stop_propagation();
                                cx.refresh_windows();
                            }),
                    )
                })
                .child(glyph(
                    if has_detail && tool_open {
                        "icons/chevron-down.svg"
                    } else if has_detail {
                        "icons/chevron-right.svg"
                    } else {
                        // Placeholder keeps the row height stable either way.
                        "icons/chevron-right.svg"
                    },
                    12.,
                    if has_detail {
                        theme.text_3
                    } else {
                        theme.text_3.opacity(0.)
                    },
                )),
        );
    // A failed run carries its first error line right on the card — the
    // fact a user scanning the turn needs, without opening the detail.
    if tool.failed {
        card = card.child(render_tool_error_strip(tool, theme));
    }
    card = card.when(has_detail, |row| {
        let scroller = scroller.clone();
        row.on_click(move |_, _, cx| {
            let mut open = expanded_tools.borrow_mut();
            if !open.remove(&key) {
                open.insert(key);
            }
            drop(open);
            scroller.remeasure_toggle(key.0);
            cx.refresh_windows();
        })
    });

    if tool_open && has_detail {
        card = card.child(render_tool_detail(
            tool,
            key,
            copied_sections,
            expanded_sections,
            scroller.clone(),
            theme,
        ));
    }
    card.into_any_element()
}

/// First human-readable error line from a failed tool's captured output.
fn first_error_line(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(|line| cap_chars(line, 200)),
        Value::Array(items) => items.iter().find_map(first_error_line),
        Value::Object(map) => map.values().find_map(first_error_line),
        _ => None,
    }
}

/// One-line error strip on a failed tool card: red-washed, drawn stop
/// glyph, and the actual error text (never invented — the fallback says
/// when nothing was captured).
fn render_tool_error_strip(tool: &ToolCall, theme: Theme) -> impl IntoElement {
    let snippet = tool
        .output
        .as_ref()
        .and_then(first_error_line)
        .unwrap_or_else(|| "The run failed without an error message.".to_string());
    div()
        .w_full()
        .border_t_1()
        .border_color(theme.border_strong)
        .bg(theme.del_red.opacity(0.07))
        .px(px(8.))
        .py(px(4.))
        .flex()
        .items_center()
        .gap(px(6.))
        .flex_none()
        .child(glyph("icons/stop.svg", 11., theme.del_red))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(theme.ui_px(11.))
                .line_height(theme.ui_px(15.))
                .text_color(theme.del_red)
                .child(snippet),
        )
}

/// The expandable detail card: a Command (terminal-highlighted shell), or
/// Arguments (JSON) when the tool is not a shell command, then the Output.
/// Each section carries its own copy button; tall sections collapse to a
/// preview so a long log never buries the answer that follows it.
fn render_tool_detail(
    tool: &ToolCall,
    key: (usize, usize),
    copied_sections: CopiedSections,
    expanded_sections: ExpandedSections,
    scroller: MessageScrollerState,
    theme: Theme,
) -> impl IntoElement {
    // An edit/write tool shows its change as an inline diff — the arguments
    // JSON would just be the same text, unread. Everything else: a command
    // tool shows its shell command as terminal input, and the rest show the
    // JSON arguments. The result is JSON-highlighted when the tool returned
    // structured data, plain otherwise.
    let diff = edit_diff(tool);
    let first = match tool_command(tool) {
        Some(command) => Some(("Command", command, Some(highlight::Lang::Shell), true)),
        None if diff.is_some() => None,
        None => tool.args.as_ref().map(|args| {
            (
                "Arguments",
                display_value_capped(args, DETAIL_TEXT_CAP),
                section_lang(Some(args)),
                false,
            )
        }),
    };
    let second = tool.output.as_ref().map(|output| {
        (
            "Output",
            display_value_capped(output, OUTPUT_TEXT_CAP),
            section_lang(Some(output)),
            false,
        )
    });
    let sections: Vec<_> = [
        first.map(|(label, content, lang, prompt)| (0u8, label, content, lang, prompt)),
        second.map(|(label, content, lang, prompt)| (1u8, label, content, lang, prompt)),
    ]
    .into_iter()
    .flatten()
    .filter(|(_, _, content, _, _)| !content.trim().is_empty())
    .collect();
    let has_sections = !sections.is_empty();

    let mut container = div()
        .w_full()
        .min_w_0()
        .border_t_1()
        .border_color(theme.border_strong)
        .flex()
        .flex_col();
    if let Some(rows) = diff.as_ref() {
        container = container.child(render_edit_diff(
            rows,
            key,
            expanded_sections.clone(),
            scroller.clone(),
            theme,
        ));
    }
    if has_sections {
        container = container.child(
            div()
                .w_full()
                .min_w_0()
                .px(px(10.))
                .py(px(8.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .children(sections.into_iter().map(
                    move |(section, label, content, lang, prompt)| {
                        render_detail_section(
                            key,
                            section,
                            label,
                            content,
                            lang,
                            prompt,
                            copied_sections.clone(),
                            expanded_sections.clone(),
                            scroller.clone(),
                            theme,
                        )
                    },
                )),
        );
    }
    container
}

/// A structured (non-string) value highlights as JSON; strings and scalars
/// stay plain so raw command output is not mislabeled as a document.
fn section_lang(value: Option<&Value>) -> Option<highlight::Lang> {
    match value {
        Some(Value::String(_)) | None => None,
        Some(_) => Some(highlight::Lang::Json),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_detail_section(
    key: (usize, usize),
    section: u8,
    label: &str,
    content: String,
    lang: Option<highlight::Lang>,
    prompt: bool,
    copied_sections: CopiedSections,
    expanded_sections: ExpandedSections,
    scroller: MessageScrollerState,
    theme: Theme,
) -> impl IntoElement {
    let copied = copied_sections
        .borrow()
        .get(&(key.0, key.1, section))
        .is_some_and(|at| at.elapsed() < COPY_FEEDBACK);
    let copy_content = content.clone();
    // Tall sections (a test run's log, a big edit's arguments) collapse to a
    // prefix preview; the toggle remeasures the row. Copy always carries the
    // full captured text, folded or not.
    let lines: Vec<&str> = content.split('\n').collect();
    let foldable = lines.len() > OUTPUT_COLLAPSE_LINES;
    let expanded = foldable
        && expanded_sections
            .borrow()
            .contains(&(key.0, key.1, section));
    let visible: &[&str] = if foldable && !expanded {
        &lines[..OUTPUT_PREVIEW_LINES]
    } else if lines.len() > OUTPUT_EXPANDED_PAINT_LINES {
        &lines[..OUTPUT_EXPANDED_PAINT_LINES]
    } else {
        &lines
    };
    let paint_clipped = expanded && lines.len() > OUTPUT_EXPANDED_PAINT_LINES;
    let card = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .h(px(22.))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(theme.ui_px(11.5))
                        .line_height(theme.ui_px(15.))
                        .text_color(theme.text_2)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .id(ElementId::NamedInteger(
                            "copy-activity-section".into(),
                            ((key.0 as u64) << 16 | key.1 as u64) << 8 | section as u64,
                        ))
                        .flex_none()
                        .size(px(24.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.overlay_strong))
                        .child(glyph(
                            if copied {
                                "icons/check.svg"
                            } else {
                                "icons/copy.svg"
                            },
                            12.,
                            if copied { theme.ok_green } else { theme.text_3 },
                        ))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_content.clone()));
                            copied_sections
                                .borrow_mut()
                                .insert((key.0, key.1, section), Instant::now());
                            cx.refresh_windows();
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_start()
                .gap(px(8.))
                .when(prompt, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .font_family(theme::code_font_family())
                            .text_size(theme.term_px(13.))
                            .line_height(theme.term_px(20.))
                            .text_color(theme.text_3)
                            .child("$"),
                    )
                })
                .child(render_detail_body(&content, visible, lang, theme)),
        );
    if foldable {
        let total = lines.len();
        let toggle_label = if expanded {
            "Show less".to_string()
        } else {
            format!("Show all {total} lines")
        };
        card.child(
            div()
                .id(ElementId::NamedInteger(
                    "section-fold".into(),
                    ((key.0 as u64) << 16 | key.1 as u64) << 8 | section as u64,
                ))
                .w_full()
                .h(px(22.))
                .flex()
                .items_center()
                .gap(px(5.))
                .cursor_pointer()
                .text_size(theme.ui_px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_3)
                .hover(|style| style.text_color(theme.text))
                .child(glyph(
                    if expanded {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    10.,
                    theme.text_3,
                ))
                .child(toggle_label)
                .when(paint_clipped, |toggle| {
                    toggle.child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.text_3)
                            .child(format!(
                                "Showing first {OUTPUT_EXPANDED_PAINT_LINES} — copy for the full log"
                            )),
                    )
                })
                .on_click(move |_, _, cx| {
                    let fold_key = (key.0, key.1, section);
                    let mut open = expanded_sections.borrow_mut();
                    if !open.remove(&fold_key) {
                        open.insert(fold_key);
                    }
                    drop(open);
                    scroller.remeasure_toggle(key.0);
                    cx.refresh_windows();
                }),
        )
    } else {
        card
    }
}

/// Highlighted, wrapping tool text: one line per row so a long command or
/// blob wraps inside the card rather than forcing it wide. `visible` is the
/// prefix slice actually painted (collapse preview / paint cap); token
/// ranges index lines absolutely, and a prefix slice keeps them aligned.
fn render_detail_body(
    content: &str,
    visible: &[&str],
    lang: Option<highlight::Lang>,
    theme: Theme,
) -> AnyElement {
    let tokens = lang.map(|lang| highlight::tokenize_cached(lang, content));
    div()
        .flex_1()
        .min_w_0()
        .font_family(theme::code_font_family())
        .text_size(theme.term_px(13.))
        .line_height(theme.term_px(20.))
        .text_color(theme.code_text)
        .whitespace_normal()
        .flex()
        .flex_col()
        .children(visible.iter().enumerate().map(move |(line_ix, line)| {
            div().w_full().min_w_0().child(syntax_styled(
                line,
                tokens.as_deref(),
                line_ix,
                theme.code_text,
                theme,
            ))
        }))
        .into_any_element()
}

// ── inline edit diff ───────────────────────────────────────────────────────

/// Kind of one row in an inline edit diff. `Break` separates two edits that
/// arrived in the same tool call so their hunks don't run together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffRowKind {
    Context,
    Addition,
    Deletion,
    Break,
}

/// One painted row: the text plus the syntax tokens resolved for its line.
#[derive(Clone)]
struct DiffRow {
    kind: DiffRowKind,
    text: String,
    tokens: Option<Vec<Token>>,
}

type EditDiffCache = HashMap<u64, Rc<Vec<DiffRow>>>;

thread_local! {
    /// Bounded per-tool edit-diff memo. A card repaints every frame (and on
    /// every streaming tick); the line diff and token pass run only when the
    /// tool's text actually changes.
    static EDIT_DIFF_CACHE: RefCell<EditDiffCache> = RefCell::new(HashMap::new());
}

/// The inline diff for an `edit` / `write` tool, or `None` when its arguments
/// carry no text to diff. Content comes only from the tool itself.
fn edit_diff(tool: &ToolCall) -> Option<Rc<Vec<DiffRow>>> {
    let key = {
        let mut hasher = DefaultHasher::new();
        tool.name.hash(&mut hasher);
        tool.path.hash(&mut hasher);
        if let Some(args) = tool.args.as_ref() {
            if let Some(edits) = args.get("edits").and_then(Value::as_array) {
                for edit in edits {
                    edit.get("oldText")
                        .and_then(Value::as_str)
                        .hash(&mut hasher);
                    edit.get("newText")
                        .and_then(Value::as_str)
                        .hash(&mut hasher);
                }
            }
            if let Some(content) = args.get("content").and_then(Value::as_str) {
                content.hash(&mut hasher);
            }
        }
        hasher.finish()
    };
    EDIT_DIFF_CACHE.with(|cache| {
        if let Some(hit) = cache.borrow().get(&key) {
            return Some(hit.clone());
        }
        let rows = Rc::new(build_edit_diff(tool)?);
        let mut cache = cache.borrow_mut();
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(key, rows.clone());
        Some(rows)
    })
}

fn build_edit_diff(tool: &ToolCall) -> Option<Vec<DiffRow>> {
    let lang = tool.path.as_deref().and_then(path_language);
    match tool.name.as_str() {
        "edit" => {
            let edits = tool.args.as_ref()?.get("edits")?.as_array()?;
            let mut rows: Vec<DiffRow> = Vec::new();
            for edit in edits {
                let (Some(old), Some(new)) = (
                    edit.get("oldText").and_then(Value::as_str),
                    edit.get("newText").and_then(Value::as_str),
                ) else {
                    continue;
                };
                if !rows.is_empty() {
                    rows.push(DiffRow {
                        kind: DiffRowKind::Break,
                        text: String::new(),
                        tokens: None,
                    });
                }
                rows.extend(line_diff(old, new, lang));
            }
            (!rows.is_empty()).then_some(rows)
        }
        "write" => {
            let content = tool.args.as_ref()?.get("content")?.as_str()?;
            let lines = split_diff_lines(content);
            let tokens = lang.map(|lang| highlight::tokenize(lang, content));
            Some(
                lines
                    .iter()
                    .enumerate()
                    .map(|(ix, text)| DiffRow {
                        kind: DiffRowKind::Addition,
                        text: (*text).to_string(),
                        tokens: tokens.as_ref().and_then(|t| t.get(ix)).cloned(),
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Language for a file path's extension, when the lexer knows it.
fn path_language(path: &str) -> Option<highlight::Lang> {
    let ext = Path::new(path).extension()?.to_str()?;
    highlight::lang_for_tag(ext)
}

/// Split a code block into lines, dropping a single trailing newline so a
/// file that ends `\n` doesn't paint a phantom empty row.
fn split_diff_lines(text: &str) -> Vec<&str> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

/// Longest-common-subsequence line diff: unchanged lines become context, the
/// rest are deletions (old only) and additions (new only). Oversized edits
/// fall back to a whole-block replacement so the paint stays bounded.
fn line_diff(old: &str, new: &str, lang: Option<highlight::Lang>) -> Vec<DiffRow> {
    let old_lines = split_diff_lines(old);
    let new_lines = split_diff_lines(new);
    let old_tokens = lang.map(|lang| highlight::tokenize(lang, old));
    let new_tokens = lang.map(|lang| highlight::tokenize(lang, new));
    let (n, m) = (old_lines.len(), new_lines.len());
    let mut ops: Vec<(DiffRowKind, Option<usize>, Option<usize>)> = Vec::new();
    if n.saturating_mul(m) > 250_000 {
        ops.extend((0..n).map(|i| (DiffRowKind::Deletion, Some(i), None)));
        ops.extend((0..m).map(|j| (DiffRowKind::Addition, None, Some(j))));
    } else {
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if old_lines[i] == new_lines[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0usize, 0usize);
        while i < n && j < m {
            if old_lines[i] == new_lines[j] {
                ops.push((DiffRowKind::Context, Some(i), Some(j)));
                i += 1;
                j += 1;
            } else if lcs[i + 1][j] >= lcs[i][j + 1] {
                ops.push((DiffRowKind::Deletion, Some(i), None));
                i += 1;
            } else {
                ops.push((DiffRowKind::Addition, None, Some(j)));
                j += 1;
            }
        }
        while i < n {
            ops.push((DiffRowKind::Deletion, Some(i), None));
            i += 1;
        }
        while j < m {
            ops.push((DiffRowKind::Addition, None, Some(j)));
            j += 1;
        }
    }
    ops.into_iter()
        .map(|(kind, old_ix, new_ix)| {
            let (text, tokens) = match kind {
                DiffRowKind::Deletion => (
                    old_ix.and_then(|ix| old_lines.get(ix)).copied(),
                    old_tokens
                        .as_ref()
                        .and_then(|t| old_ix.and_then(|ix| t.get(ix))),
                ),
                _ => (
                    new_ix.and_then(|ix| new_lines.get(ix)).copied(),
                    new_tokens
                        .as_ref()
                        .and_then(|t| new_ix.and_then(|ix| t.get(ix))),
                ),
            };
            DiffRow {
                kind,
                text: text.unwrap_or("").to_string(),
                tokens: tokens.cloned(),
            }
        })
        .collect()
}

/// Unified-patch text for the copy button — `+`/`-`/space prefixes, minus the
/// cosmetic break rows.
fn edit_patch_text(rows: &[DiffRow]) -> String {
    let mut out = String::new();
    for row in rows {
        let marker = match row.kind {
            DiffRowKind::Addition => Some('+'),
            DiffRowKind::Deletion => Some('-'),
            DiffRowKind::Context => Some(' '),
            DiffRowKind::Break => None,
        };
        if let Some(marker) = marker {
            out.push(marker);
            out.push_str(&row.text);
            out.push('\n');
        }
    }
    out
}

/// The diff body inside an expanded edit card, full-bleed like the Review
/// pane's rows. Long diffs fold to a preview; copy always carries the whole
/// patch.
fn render_edit_diff(
    rows: &[DiffRow],
    key: (usize, usize),
    expanded_sections: ExpandedSections,
    scroller: MessageScrollerState,
    theme: Theme,
) -> AnyElement {
    let total = rows.len();
    let foldable = total > EDIT_DIFF_COLLAPSE_LINES;
    let expanded = foldable
        && expanded_sections
            .borrow()
            .contains(&(key.0, key.1, EDIT_DIFF_COPY_SECTION));
    let visible = if foldable && !expanded {
        EDIT_DIFF_PREVIEW_LINES.min(total)
    } else {
        EDIT_DIFF_PAINT_LINES.min(total)
    };
    let clipped = expanded && total > EDIT_DIFF_PAINT_LINES;
    let mut body = div()
        .w_full()
        .min_w_0()
        .py(px(4.))
        .flex()
        .flex_col()
        .children(
            rows.iter()
                .take(visible)
                .map(|row| render_edit_diff_row(row, theme)),
        );
    if foldable {
        let label = if expanded {
            "Show less".to_string()
        } else {
            format!("Show all {total} lines")
        };
        body = body.child(
            div()
                .id(ElementId::NamedInteger(
                    "edit-diff-fold".into(),
                    ((key.0 as u64) << 16 | key.1 as u64) << 8 | EDIT_DIFF_COPY_SECTION as u64,
                ))
                .w_full()
                .h(px(24.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(5.))
                .cursor_pointer()
                .text_size(theme.ui_px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_3)
                .hover(|style| style.text_color(theme.text))
                .child(glyph(
                    if expanded {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    10.,
                    theme.text_3,
                ))
                .child(label)
                .when(clipped, |toggle| {
                    toggle.child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.text_3)
                            .child(format!(
                                "Showing first {EDIT_DIFF_PAINT_LINES} — copy for the whole diff"
                            )),
                    )
                })
                .on_click(move |_, _, cx| {
                    let fold_key = (key.0, key.1, EDIT_DIFF_COPY_SECTION);
                    let mut open = expanded_sections.borrow_mut();
                    if !open.remove(&fold_key) {
                        open.insert(fold_key);
                    }
                    drop(open);
                    scroller.remeasure_toggle(key.0);
                    // The fold is not a card toggle — don't collapse the card
                    // out from under the click.
                    cx.stop_propagation();
                    cx.refresh_windows();
                }),
        );
    }
    body.into_any_element()
}

fn render_edit_diff_row(row: &DiffRow, theme: Theme) -> AnyElement {
    if row.kind == DiffRowKind::Break {
        return div()
            .w_full()
            .flex_none()
            .h(px(9.))
            .border_t_1()
            .border_color(theme.border)
            .into_any_element();
    }
    let (body_bg, gutter_bg, edge, marker_color, marker) = match row.kind {
        DiffRowKind::Addition => (
            Some(theme.add_green.opacity(diff_body_wash(theme))),
            Some(theme.add_green.opacity(diff_gutter_wash(theme))),
            Some(theme.add_green),
            theme.add_green,
            "+",
        ),
        DiffRowKind::Deletion => (
            Some(theme.del_red.opacity(diff_body_wash(theme))),
            Some(theme.del_red.opacity(diff_gutter_wash(theme))),
            Some(theme.del_red),
            theme.del_red,
            "-",
        ),
        _ => (None, None, None, theme.text_3, ""),
    };
    div()
        .w_full()
        .min_w_0()
        .min_h(px(EDIT_DIFF_ROW_HEIGHT))
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(EDIT_DIFF_TEXT_SIZE))
        .line_height(theme.code_px(18.))
        .when_some(edge, |row, edge| row.border_l_2().border_color(edge))
        .child(
            div()
                .w(px(24.))
                .flex_none()
                .flex()
                .items_start()
                .justify_center()
                .text_color(marker_color)
                .when_some(gutter_bg, |gutter, bg| gutter.bg(bg))
                .child(marker),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .pr(px(12.))
                .overflow_hidden()
                // A long line wraps to the next row instead of being clipped —
                // the diff must stay readable at any pane width.
                .whitespace_normal()
                .when_some(body_bg, |body, bg| body.bg(bg))
                .text_color(theme.code_text)
                .child(diff_row_text(row, theme)),
        )
        .into_any_element()
}

fn diff_body_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.18
    } else {
        0.12
    }
}

fn diff_gutter_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.24
    } else {
        0.15
    }
}

/// Syntax-colored text for one diff row (tokens are already per-line).
fn diff_row_text(row: &DiffRow, theme: Theme) -> StyledText {
    let font = mono_font();
    let display = if row.text.is_empty() {
        " "
    } else {
        row.text.as_str()
    };
    let mut runs: Vec<TextRun> = Vec::new();
    if let Some(tokens) = row.tokens.as_ref() {
        let mut offset = 0usize;
        for token in tokens {
            let start = token.range.start.min(row.text.len());
            let end = token.range.end.min(row.text.len());
            if start > offset {
                runs.push(code_run(start - offset, theme.code_text, &font));
            }
            if end > start {
                runs.push(code_run(end - start, theme.token_color(token.class), &font));
            }
            offset = offset.max(end);
        }
        if offset < row.text.len() {
            runs.push(code_run(row.text.len() - offset, theme.code_text, &font));
        }
    }
    if runs.is_empty() {
        runs.push(code_run(display.len(), theme.code_text, &font));
    }
    StyledText::new(display.to_string()).with_runs(runs)
}

/// Arguments / result text: strings as-is, everything else pretty-printed.
fn display_value_capped(value: &Value, cap: usize) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    cap_chars(&text, cap)
}

fn cap_chars(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    } else {
        text.to_string()
    }
}

/// The quiet timestamp chip in a message/run footer.
fn footer_time_stamp(text: String, theme: Theme) -> AnyElement {
    div()
        .flex_none()
        .h(px(FOOTER_BUTTON_SIZE))
        .px(px(4.))
        .flex()
        .items_center()
        .text_size(theme.ui_px(11.5))
        .line_height(theme.ui_px(16.))
        .text_color(theme.text_3)
        .child(text)
        .into_any_element()
}

/// Message footer: persistent quiet copy button + timestamp stamp and, when
/// pi reports usage, a compact `↑in ↓out · $cost` metric. Hovering the metric
/// opens the full token/cache/cost breakdown.
#[allow(clippy::too_many_arguments)]
fn render_message_footer(
    copy_text: String,
    ix: usize,
    stamp: Option<AnyElement>,
    usage: Option<MessageUsage>,
    copied: bool,
    align_right: bool,
    theme: Theme,
    copied_at: Rc<RefCell<HashMap<usize, Instant>>>,
    hovered_usage: Rc<Cell<Option<usize>>>,
) -> impl IntoElement {
    let button = div()
        .id(ElementId::NamedInteger("copy-response".into(), ix as u64))
        .flex_none()
        .size(px(FOOTER_BUTTON_SIZE))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.bg(theme.overlay_strong))
        .child(glyph(
            if copied {
                "icons/check.svg"
            } else {
                "icons/copy.svg"
            },
            18.,
            if copied { theme.ok_green } else { theme.text_3 },
        ))
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
            copied_at.borrow_mut().insert(ix, Instant::now());
            cx.refresh_windows();
        });
    let mut footer = div()
        .h(px(FOOTER_BUTTON_SIZE))
        .flex()
        .items_center()
        .gap(px(1.))
        .when(align_right, |row| row.justify_end());
    if align_right {
        // Waku's right-aligned footer: timestamp first, then actions.
        if let Some(stamp) = stamp {
            footer = footer.child(stamp);
        }
        footer = footer.child(button);
    } else {
        footer = footer.child(button);
        if let Some(stamp) = stamp {
            footer = footer.child(stamp);
        }
    }
    if let Some(metric) = usage_metric(usage, ix, theme, hovered_usage) {
        footer = footer.child(metric);
    }
    footer
}

/// Compact `↑in ↓out · $cost` footer metric with a hover breakdown card.
/// `None` when the turn has no reported usage, so the footer stays clean.
fn usage_metric(
    usage: Option<MessageUsage>,
    ix: usize,
    theme: Theme,
    hovered_usage: Rc<Cell<Option<usize>>>,
) -> Option<AnyElement> {
    let usage = usage?;
    let hovered = hovered_usage.get() == Some(ix);
    let label = {
        let mut label = format!(
            "↑{} ↓{}",
            format_tokens(usage.input),
            format_tokens(usage.output)
        );
        if usage.cache_read > 0 {
            if let Some(percent) = usage.cache_read_percent() {
                label.push_str(&format!(" · {percent:.0}% cached"));
            }
        }
        if let Some(cost) = usage.cost.filter(|cost| *cost > 0.0) {
            label.push_str(" · ");
            label.push_str(&format_message_cost(cost));
        }
        label
    };
    let mut metric = div()
        .id(ElementId::NamedInteger("usage-metric".into(), ix as u64))
        .relative()
        .ml(px(6.))
        .h(px(FOOTER_BUTTON_SIZE))
        .flex()
        .items_center()
        .text_size(theme.ui_px(11.5))
        .line_height(theme.ui_px(16.))
        .text_color(theme.text_3)
        .child(label)
        .on_hover(move |is_hovered, _, cx| {
            let next = if *is_hovered { Some(ix) } else { None };
            if hovered_usage.get() != next {
                hovered_usage.set(next);
                cx.refresh_windows();
            }
        });
    if hovered {
        metric = metric.child(
            div()
                .absolute()
                .bottom_full()
                .left_0()
                .mb(px(6.))
                .child(deferred(usage_breakdown_card(&usage, theme))),
        );
    }
    Some(metric.into_any_element())
}

/// Hover card: the full per-turn token/cache/cost breakdown behind the
/// compact footer metric.
fn usage_breakdown_card(usage: &MessageUsage, theme: Theme) -> AnyElement {
    let cost = usage
        .cost
        .map(format_message_cost)
        .unwrap_or_else(|| "—".into());
    div()
        .w(px(224.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.menu_bg)
        .shadow(theme.card_shadow())
        .px(px(12.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(theme.ui_px(11.5))
                .text_color(theme.text_3)
                .child("Message usage"),
        )
        .child(usage_metric_row(
            "icons/usage-input.svg",
            "Input",
            format_tokens(usage.input),
            theme,
        ))
        .child(usage_metric_row(
            "icons/usage-output.svg",
            "Output",
            format_tokens(usage.output),
            theme,
        ))
        .child(usage_metric_row(
            "icons/cache-read.svg",
            "Cache read",
            cache_read_label(usage),
            theme,
        ))
        .child(usage_metric_row(
            "icons/cache-write.svg",
            "Cache write",
            format_tokens(usage.cache_write),
            theme,
        ))
        .child(div().w_full().h(px(1.)).bg(theme.border))
        .child(usage_metric_row(
            "icons/usage-total.svg",
            "Total",
            format_tokens(usage.total),
            theme,
        ))
        .child(usage_metric_row(
            "icons/usage-cost.svg",
            "Cost",
            cost,
            theme,
        ))
        .into_any_element()
}

fn usage_metric_row(
    icon_path: &'static str,
    label: &'static str,
    value: String,
    theme: Theme,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(7.))
        .child(glyph(icon_path, 13., theme.text_3))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(theme.ui_px(12.))
                .text_color(theme.text_3)
                .whitespace_nowrap()
                .child(label),
        )
        .child(
            div()
                .text_size(theme.ui_px(12.))
                .text_color(theme.text)
                .whitespace_nowrap()
                .child(value),
        )
}

/// `40.0K · 89% hit` for the cache-read row: the token count plus the
/// provider cache hit rate over prompt tokens (`None` when the prompt was
/// empty, so no meaningless `0%`).
fn cache_read_label(usage: &MessageUsage) -> String {
    let tokens = format_tokens(usage.cache_read);
    match usage.cache_read_percent() {
        Some(percent) => format!("{tokens} · {percent:.0}% hit"),
        None => tokens,
    }
}

/// Per-message costs are usually fractions of a cent, so show four decimals
/// below one cent rather than collapsing every row to `<$0.01`.
fn format_message_cost(cost: f64) -> String {
    if !cost.is_finite() || cost <= 0.0 {
        "$0.00".into()
    } else if cost < 0.01 {
        format!("${cost:.4}")
    } else if cost < 100.0 {
        format!("${cost:.2}")
    } else {
        format!("${cost:.0}")
    }
}

/// Local `HH:MM` for an epoch-millis stamp; `None` when it cannot be resolved.
fn format_time(millis: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
}

fn glyph(path: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    svg()
        .path(path)
        .flex_none()
        .size(px(size))
        .text_color(color)
}

fn pulse_dot(theme: Theme, elapsed_ms: u128) -> impl IntoElement {
    let on = elapsed_ms.is_multiple_of(400);
    div().size(px(5.)).rounded_full().bg(if on {
        theme.accent
    } else {
        theme.accent.opacity(0.35)
    })
}

fn fold_label(elapsed: Option<Duration>) -> String {
    match elapsed {
        Some(duration) => format!("Worked for {}", format_duration(duration)),
        None => "Worked".to_string(),
    }
}

/// Waku `formatDuration` spoken units: `5 minutes 41 seconds`.
fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs().max(1);
    if secs < 60 {
        return format!("{} {}", secs, plural_unit("second", secs));
    }
    if secs < 3_600 {
        let minutes = secs / 60;
        let remaining = secs % 60;
        let first = format!("{} {}", minutes, plural_unit("minute", minutes));
        return if remaining > 0 {
            format!(
                "{} {} {}",
                first,
                remaining,
                plural_unit("second", remaining)
            )
        } else {
            first
        };
    }
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    let first = format!("{} {}", hours, plural_unit("hour", hours));
    if minutes > 0 {
        format!("{} {} {}", first, minutes, plural_unit("minute", minutes))
    } else {
        first
    }
}

fn plural_unit(word: &str, count: u64) -> String {
    if count == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

fn render_turn_fold(
    ix: usize,
    expanded: bool,
    elapsed: Option<Duration>,
    theme: Theme,
    expanded_turns: Rc<RefCell<HashSet<usize>>>,
    scroller: MessageScrollerState,
) -> impl IntoElement {
    let label = fold_label(elapsed);
    div()
        .w_full()
        .h(px(24.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().h(px(1.)).flex_1().bg(theme.border))
        .child(
            div()
                .id(ElementId::NamedInteger("turn-fold".into(), ix as u64))
                .h(px(24.))
                .px(px(2.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .cursor_pointer()
                .text_size(theme.ui_px(13.5))
                .line_height(theme.ui_px(18.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_3)
                .hover(|style| style.text_color(theme.text_2))
                .child(label)
                .child(glyph(
                    if expanded {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    11.5,
                    theme.text_3,
                ))
                .on_click(move |_, _, cx| {
                    toggle_index(&expanded_turns, ix);
                    scroller.remeasure_toggle(ix);
                    cx.refresh_windows();
                }),
        )
        .child(div().h(px(1.)).flex_1().bg(theme.border))
}

fn working_wave_dots(theme: Theme, elapsed_ms: u128) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(3.))
        .children((0..3).map(move |i| {
            let phase = ((elapsed_ms / 180) + i as u128 * 2) % 6;
            let on = phase < 3;
            div()
                .size(px(4.))
                .rounded_full()
                .bg(if on { theme.accent } else { theme.border })
        }))
}

/// What the agent is doing right now, for the live indicator: "Running
/// cargo test", "Reading src/auth.rs", "Thinking…". Derived from the live
/// step's latest tool so the status names real work, never a bare spinner.
fn working_activity_label(step: &Step) -> Option<String> {
    if let Some(tool) = step.tools.last() {
        let detail = activity_preview(tool);
        let verb = match tool.name.as_str() {
            "bash" | "shell" | "terminal" | "exec" | "run" => "Running",
            "read" => "Reading",
            "grep" | "find" | "glob" | "search" => "Searching",
            "edit" | "write" => "Editing",
            other => {
                let action = activity_action_label(other);
                return Some(if detail.is_empty() {
                    format!("Using {action}")
                } else {
                    format!("{action} {detail}")
                });
            }
        };
        return Some(if detail.is_empty() {
            verb.to_string()
        } else {
            format!("{verb} {detail}")
        });
    }
    if !step.thinking.is_empty() {
        return Some("Thinking…".into());
    }
    None
}

/// Quiet end-of-turn marker for a cancelled generation: a stop glyph and
/// "Stopped" in the meta color. Sits where the working indicator was.
fn render_stopped_marker(theme: Theme) -> impl IntoElement {
    div()
        .h(px(22.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(glyph("icons/stop.svg", 11., theme.text_3))
        .child(
            div()
                .text_size(theme.ui_px(13.5))
                .line_height(theme.ui_px(18.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_3)
                .child("Stopped"),
        )
}

fn render_working_indicator(
    elapsed: Duration,
    activity: Option<String>,
    theme: Theme,
) -> impl IntoElement {
    // Name the current activity when we know it; the elapsed time stays so
    // a long-running command still reads as progress, not a hang.
    let label = match activity {
        Some(activity) => format!("{} · {}", activity, format_working_elapsed(elapsed)),
        None => format!("Working for {}", format_working_elapsed(elapsed)),
    };
    let label_size = theme.ui_px(13.5);
    let label_line = theme.ui_px(18.);
    // The live label carries the sidebar's running-session shimmer — the same
    // ember band sweeping across the ink. Reduce-motion keeps the accent
    // signal without the perpetual sweep.
    let label_el: AnyElement = if theme.ui.reduce_motion {
        div()
            .min_w_0()
            .truncate()
            .text_size(label_size)
            .line_height(label_line)
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.accent)
            .child(label)
            .into_any_element()
    } else {
        ShimmerText::new(label, theme.text_3, theme.accent)
            .with_animation(
                ElementId::NamedInteger("working-shimmer".into(), 0),
                Animation::new(Duration::from_millis(2000)).repeat(),
                |mut text, delta| {
                    text.phase = delta;
                    text
                },
            )
            .into_any_element()
    };
    div()
        // The row needs a definite width: the shimmer paints at `width: 100%`
        // of its wrapper, so an indefinite (shrink-to-fit) row would collapse
        // the label — timer and all — to nothing.
        .w_full()
        .h(px(22.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(working_wave_dots(theme, elapsed.as_millis()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(label_size)
                .line_height(label_line)
                .font_weight(FontWeight::MEDIUM)
                .child(label_el),
        )
}

/// Waku `formatWorkingElapsed` compact form: `12s` / `5m 41s` / `1h 2m`.
fn format_working_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3_600 {
        let minutes = secs / 60;
        let remaining = secs % 60;
        return if remaining > 0 {
            format!("{minutes}m {remaining}s")
        } else {
            format!("{minutes}m")
        };
    }
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    if minutes > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{hours}h")
    }
}

fn activity_preview(tool: &ToolCall) -> String {
    if let Some(path) = &tool.path {
        return path.clone();
    }
    // A command tool's pi summary is the raw JSON arguments; show the shell
    // command itself in the header instead of `{"command":"…"}`.
    let summary = tool_command(tool).unwrap_or_else(|| tool.summary.clone());
    if summary.len() > 72 {
        let mut out: String = summary.chars().take(72).collect();
        out.push('…');
        out
    } else {
        summary
    }
}

/// The shell command a command-running tool was invoked with, when the tool
/// carries one in its arguments. `None` for edit/read/other tools.
fn tool_command(tool: &ToolCall) -> Option<String> {
    if !matches!(
        tool.name.as_str(),
        "bash" | "shell" | "terminal" | "exec" | "run"
    ) {
        return None;
    }
    tool.args
        .as_ref()?
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn render_line_delta(added: u64, removed: u64, theme: Theme, size: f32) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .text_size(theme.ui_px(size))
        .line_height(theme.ui_px(size + 3.))
        .child(
            div()
                .flex_none()
                .text_color(theme.add_green)
                .child(format!("+{added}")),
        )
        .child(
            div()
                .flex_none()
                .text_color(theme.del_red)
                .child(format!("-{removed}")),
        )
}

// ── Markdown ────────────────────────────────────────────────────────────────
// Waku `.markdown` parity: 14px/22px body, 0.9rem rhythm between blocks,
// h1/h2/h3 at 20/18/16px semibold, `ml-5` lists with hanging indents,
// `border-l-2` blockquotes, mono inline-code chips, rounded pre blocks,
// and clickable underlined links.

/// Flattened inline runs: the body text, one [`TextRun`] per styled span,
/// and the link `(byte range, url)` pairs.
type InlineRuns = (SharedString, Vec<TextRun>, Vec<(Range<usize>, String)>);

/// One styled inline span produced by [`parse_inline`].
#[derive(Clone, Default)]
struct InlineSpan {
    text: String,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
    link: Option<String>,
}

fn flush_span(spans: &mut Vec<InlineSpan>, current: &mut InlineSpan) {
    if !current.text.is_empty() {
        spans.push(std::mem::take(current));
    }
}

/// Parse GFM inline syntax: `` `code` ``, `**bold**`, `*italic*`, `_italic_`,
/// `~~strike~~`, `[links](url)`. `\*`-style escapes render literally.
fn parse_inline(text: &str) -> Vec<InlineSpan> {
    let mut spans: Vec<InlineSpan> = Vec::new();
    let mut current = InlineSpan::default();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\\' if i + 1 < chars.len() && "*_`~[]".contains(chars[i + 1]) => {
                current.text.push(chars[i + 1]);
                i += 2;
            }
            '`' => match (i + 1..chars.len()).find(|&j| chars[j] == '`') {
                Some(close) => {
                    flush_span(&mut spans, &mut current);
                    spans.push(InlineSpan {
                        text: chars[i + 1..close].iter().collect(),
                        code: true,
                        bold: current.bold,
                        ..InlineSpan::default()
                    });
                    i = close + 1;
                }
                None => {
                    current.text.push('`');
                    i += 1;
                }
            },
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                flush_span(&mut spans, &mut current);
                current.bold = !current.bold;
                i += 2;
            }
            '_' if i + 1 < chars.len()
                && chars[i + 1] == '_'
                && (i == 0 || !chars[i - 1].is_alphanumeric()) =>
            {
                flush_span(&mut spans, &mut current);
                current.bold = !current.bold;
                i += 2;
            }
            '~' if i + 1 < chars.len() && chars[i + 1] == '~' => {
                flush_span(&mut spans, &mut current);
                current.strikethrough = !current.strikethrough;
                i += 2;
            }
            '*' => {
                // Italic: an opening `*` needs a non-space next char, a
                // closing one a non-space previous char.
                let opens = i + 1 < chars.len() && !chars[i + 1].is_whitespace();
                let closes = i > 0 && !chars[i - 1].is_whitespace();
                if (current.italic && closes) || (!current.italic && opens) {
                    flush_span(&mut spans, &mut current);
                    current.italic = !current.italic;
                } else {
                    current.text.push('*');
                }
                i += 1;
            }
            '_' if i == 0 || !chars[i - 1].is_alphanumeric() => {
                let opens = i + 1 < chars.len() && !chars[i + 1].is_whitespace();
                let closes = i > 0 && !chars[i - 1].is_whitespace();
                if (current.italic && closes) || (!current.italic && opens) {
                    flush_span(&mut spans, &mut current);
                    current.italic = !current.italic;
                } else {
                    current.text.push('_');
                }
                i += 1;
            }
            '[' => {
                // Try `[label](url)` — fall back to a literal `[`.
                let mut label_end = None;
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '\n' {
                    if chars[j] == ']' && j + 1 < chars.len() && chars[j + 1] == '(' {
                        label_end = Some(j);
                        break;
                    }
                    j += 1;
                }
                let mut matched = false;
                if let Some(label_end) = label_end {
                    if let Some(close) = (label_end + 2..chars.len())
                        .take_while(|&k| chars[k] != '\n')
                        .find(|&k| chars[k] == ')')
                    {
                        flush_span(&mut spans, &mut current);
                        let mut span = current.clone();
                        span.text = chars[i + 1..label_end].iter().collect();
                        span.link = Some(chars[label_end + 2..close].iter().collect());
                        spans.push(span);
                        i = close + 1;
                        matched = true;
                    }
                }
                if !matched {
                    current.text.push('[');
                    i += 1;
                }
            }
            other => {
                current.text.push(other);
                i += 1;
            }
        }
    }
    flush_span(&mut spans, &mut current);
    spans
}

/// Build `(text, runs, links)` for a [`StyledText`] from inline spans. Each
/// link is a `(byte range, url)` pair into the flattened body.
fn inline_runs(
    spans: &[InlineSpan],
    base_weight: FontWeight,
    base_color: Hsla,
    theme: Theme,
) -> InlineRuns {
    let mut body = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut links: Vec<(Range<usize>, String)> = Vec::new();
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let start = body.len();
        body.push_str(&span.text);
        let len = body.len() - start;
        let mut font = ui_font();
        if span.code {
            font.family = theme::code_font_family();
            font.weight = FontWeight::NORMAL;
            font.style = FontStyle::Normal;
        } else {
            font.weight = if span.bold {
                FontWeight::BOLD
            } else {
                base_weight
            };
            font.style = if span.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            };
        }
        // Inline code and links both take the accent so they read as
        // interactive/technical at a glance; body text keeps its base color.
        let color = if span.code || span.link.is_some() {
            theme.accent
        } else {
            base_color
        };
        runs.push(TextRun {
            len,
            font,
            color,
            background_color: span.code.then_some(theme.inline_code_bg),
            underline: span.link.is_some().then(|| UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            }),
            strikethrough: span.strikethrough.then(|| StrikethroughStyle {
                thickness: px(1.),
                color: None,
            }),
        });
        if let Some(url) = &span.link {
            links.push((start..body.len(), url.clone()));
        }
    }
    (body.into(), runs, links)
}

/// The window's default UI face (Zed's IBM Plex Sans), explicit for
/// [`TextRun`] construction.
fn ui_font() -> Font {
    Font {
        family: theme::ui_font_family(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

/// A styled, word-wrapping paragraph. Links open via `cx.open_url`.
///
/// Line height must clear inline-code backgrounds: a tight 14/22 measure let
/// the next wrapped line (and the next block) paint through the previous one.
fn paragraph_text(
    text: &str,
    size: f32,
    line_height: f32,
    weight: FontWeight,
    color: Hsla,
    key: ElementId,
    theme: Theme,
) -> impl IntoElement {
    let spans = parse_inline(text);
    let (body, runs, links) = inline_runs(&spans, weight, color, theme);
    let ranges: Vec<Range<usize>> = links.iter().map(|(range, _)| range.clone()).collect();
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .text_size(theme.ui_px(size))
        .line_height(theme.ui_px(line_height))
        .text_color(color)
        .child(
            InteractiveText::new(key, StyledText::new(body).with_runs(runs)).on_click(
                ranges,
                move |range_ix: usize, _, cx| {
                    if let Some((_, url)) = links.get(range_ix) {
                        cx.open_url(url);
                    }
                },
            ),
        )
}

fn md_id(ix: usize, salt: u64, block_ix: usize, sub: usize) -> ElementId {
    ElementId::NamedInteger(
        "md".into(),
        ((ix as u64) << 40) | ((salt & 0xffff) << 24) | ((block_ix as u64) << 8) | sub as u64,
    )
}

// ── block model ─────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Block {
    Paragraph(Vec<String>),
    Heading(u8, String),
    Code(Option<String>, Vec<String>),
    List(Vec<ListItem>),
    Quote(Vec<String>),
    /// GitHub-style alert (`> [!NOTE]` …): a labeled, tinted callout.
    Alert(AlertKind, Vec<String>),
    Rule,
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        aligns: Vec<TableAlign>,
    },
}

/// GFM column alignment parsed from a table's delimiter row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TableAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TableAlign {
    fn text_align(self) -> TextAlign {
        match self {
            Self::Left => TextAlign::Left,
            Self::Center => TextAlign::Center,
            Self::Right => TextAlign::Right,
        }
    }
}

/// Kinds of GitHub alert blockquote (`> [!WARNING]` …). Each maps to one
/// semantic token color and one icon, so the family stays consistent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Note | Self::Important | Self::Warning => "icons/info.svg",
            Self::Tip => "icons/spark.svg",
            Self::Caution => "icons/stop.svg",
        }
    }

    fn color(self, theme: Theme) -> Hsla {
        match self {
            Self::Note | Self::Important => theme.accent,
            Self::Tip => theme.ok_green,
            Self::Warning => theme.warn,
            Self::Caution => theme.crit,
        }
    }
}

#[derive(Clone)]
struct ListItem {
    depth: usize,
    ordered: bool,
    number: u64,
    /// GFM task-list state: `Some(true)` checked, `Some(false)` open, `None`
    /// a plain item. Task items drop the bullet/number for a checkbox.
    checked: Option<bool>,
    text: String,
}

fn flush_paragraph(paragraph: &mut Vec<String>, blocks: &mut Vec<Block>) {
    if !paragraph.is_empty() {
        blocks.push(Block::Paragraph(std::mem::take(paragraph)));
    }
}

fn heading_line(trimmed: &str) -> Option<(u8, String)> {
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&level) {
        if let Some(body) = trimmed[level..].strip_prefix(' ') {
            let body = body.trim();
            if !body.is_empty() {
                // Keep the real level; the renderer gives h4+ a smaller step
                // rather than flattening everything past h3 to one size.
                return Some((level as u8, body.to_string()));
            }
        }
    }
    None
}

fn is_rule(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    match compact.chars().next() {
        Some(first @ ('-' | '*' | '_')) => {
            compact.len() >= 3 && compact.chars().all(|c| c == first)
        }
        _ => false,
    }
}

fn list_item_line(line: &str) -> Option<(usize, bool, u64, Option<bool>, String)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    let (ordered, number, body) = if let Some(rest) = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| t.strip_prefix("+ "))
    {
        (false, 0u64, rest)
    } else {
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let after = t[digits..]
            .strip_prefix(". ")
            .or_else(|| t[digits..].strip_prefix(") "))?;
        (true, t[..digits].parse().ok()?, after)
    };
    let (checked, body) = match task_status(body) {
        Some((checked, rest)) => (Some(checked), rest),
        None => (None, body),
    };
    Some((
        indent.div_ceil(2).min(2),
        ordered,
        number,
        checked,
        body.to_string(),
    ))
}

/// Strip a leading GFM task marker (`[ ] `, `[x] `, `[X] `) from an item's
/// body, returning its state and the remaining text.
fn task_status(body: &str) -> Option<(bool, &str)> {
    let rest = body.strip_prefix('[')?;
    let mut chars = rest.chars();
    let checked = match chars.next()? {
        ' ' => false,
        'x' | 'X' => true,
        _ => return None,
    };
    let after = &rest[1..];
    let after = after.strip_prefix(']')?;
    Some((checked, after.strip_prefix(' ').unwrap_or(after)))
}

fn split_table_row(line: &str) -> Vec<String> {
    let s = line.trim();
    let s = s.strip_prefix('|').unwrap_or(s);
    let s = s.strip_suffix('|').unwrap_or(s);
    s.split('|').map(|cell| cell.trim().to_string()).collect()
}

/// Parse a GFM delimiter row (`| --- | :--: | ---: |`) into per-column
/// alignment; `None` when the line is not a delimiter row.
fn table_aligns(trimmed: &str) -> Option<Vec<TableAlign>> {
    if !trimmed.contains('-') || !trimmed.contains('|') {
        return None;
    }
    if !trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
        return None;
    }
    let cells = split_table_row(trimmed);
    if cells.is_empty() {
        return None;
    }
    Some(
        cells
            .iter()
            .map(|cell| {
                let left = cell.starts_with(':');
                let right = cell.ends_with(':');
                match (left, right) {
                    (true, true) => TableAlign::Center,
                    (false, true) => TableAlign::Right,
                    _ => TableAlign::Left,
                }
            })
            .collect(),
    )
}

/// GitHub alert marker on the first quoted line, e.g. `[!WARNING]`.
fn alert_marker(body: &[String]) -> Option<AlertKind> {
    let first = body.first()?.trim();
    let inner = first.strip_prefix("[!")?.strip_suffix(']')?;
    Some(match inner.trim().to_ascii_uppercase().as_str() {
        "NOTE" => AlertKind::Note,
        "TIP" => AlertKind::Tip,
        "IMPORTANT" => AlertKind::Important,
        "WARNING" => AlertKind::Warning,
        "CAUTION" => AlertKind::Caution,
        _ => return None,
    })
}

fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush_paragraph(&mut paragraph, &mut blocks);
            let marker = &trimmed[..trimmed.len().min(3)];
            // The fence info string's first token is the language hint
            // (```rust, ```text, …) — it becomes the code block's chip.
            let language = trimmed[3..]
                .split_whitespace()
                .next()
                .filter(|lang| !lang.is_empty())
                .map(str::to_string);
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(marker) {
                body.push(lines[i].to_string());
                i += 1;
            }
            i += 1; // closing fence (or EOF)
            blocks.push(Block::Code(language, body));
            continue;
        }

        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            i += 1;
            continue;
        }

        if let Some((level, body)) = heading_line(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            blocks.push(Block::Heading(level, body));
            i += 1;
            continue;
        }

        if is_rule(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }

        if trimmed.contains('|') {
            if let Some(aligns) = lines
                .get(i + 1)
                .and_then(|separator| table_aligns(separator.trim()))
            {
                flush_paragraph(&mut paragraph, &mut blocks);
                let header = split_table_row(trimmed);
                i += 2;
                let mut rows = Vec::new();
                while i < lines.len() {
                    let row_line = lines[i].trim();
                    if row_line.is_empty() || !row_line.contains('|') {
                        break;
                    }
                    rows.push(split_table_row(row_line));
                    i += 1;
                }
                blocks.push(Block::Table {
                    header,
                    rows,
                    aligns,
                });
                continue;
            }
        }

        if trimmed.starts_with('>') {
            flush_paragraph(&mut paragraph, &mut blocks);
            let mut body = Vec::new();
            while i < lines.len() {
                match lines[i].trim_start().strip_prefix('>') {
                    Some(rest) => {
                        body.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
                        i += 1;
                    }
                    None => break,
                }
            }
            match alert_marker(&body) {
                Some(kind) => {
                    body.remove(0);
                    // Drop the blank separator line GitHub allows after the
                    // marker so the callout body starts on real content.
                    while body.first().is_some_and(|line| line.trim().is_empty()) {
                        body.remove(0);
                    }
                    blocks.push(Block::Alert(kind, body));
                }
                None => blocks.push(Block::Quote(body)),
            }
            continue;
        }

        if let Some((depth, ordered, number, checked, body)) = list_item_line(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            let mut items = Vec::new();
            let mut pending = ListItem {
                depth,
                ordered,
                number,
                checked,
                text: body,
            };
            i += 1;
            while i < lines.len() {
                let l = lines[i];
                if l.trim().is_empty() {
                    // A blank line keeps the list open only when another
                    // item follows (loose list).
                    let mut look = i + 1;
                    while look < lines.len() && lines[look].trim().is_empty() {
                        look += 1;
                    }
                    if look < lines.len() && list_item_line(lines[look]).is_some() {
                        i = look;
                        continue;
                    }
                    break;
                }
                if let Some((depth, ordered, number, checked, body)) = list_item_line(l) {
                    // A marker-type change starts a new list.
                    if ordered != pending.ordered {
                        break;
                    }
                    items.push(std::mem::replace(
                        &mut pending,
                        ListItem {
                            depth,
                            ordered,
                            number,
                            checked,
                            text: body,
                        },
                    ));
                    i += 1;
                } else if l.starts_with("  ") || l.starts_with('\t') {
                    // Indented continuation of the pending item.
                    pending.text.push(' ');
                    pending.text.push_str(l.trim());
                    i += 1;
                } else {
                    break;
                }
            }
            items.push(pending);
            blocks.push(Block::List(items));
            continue;
        }

        paragraph.push(line.trim().to_string());
        i += 1;
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    blocks
}

thread_local! {
    /// Memoized markdown block parses, keyed by content hash. Every visible
    /// row re-renders on each heartbeat while a run streams (the live clock
    /// alone repaints at ~11 Hz); parsing is pure over the source text, so
    /// each distinct text parses once and rows share the blocks.
    static BLOCK_PARSE_CACHE: RefCell<HashMap<u64, Rc<Vec<Block>>>> =
        RefCell::new(HashMap::new());
}

/// Parse markdown into blocks, memoized by content. The cache is small and
/// cleared when full — a stale-entry miss costs one reparse, never a wrong
/// render (keyed by the text's full 64-bit hash).
fn parse_blocks_cached(text: &str) -> Rc<Vec<Block>> {
    let key = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(text, &mut hasher);
        std::hash::Hasher::finish(&hasher)
    };
    BLOCK_PARSE_CACHE.with(|cache| {
        if let Some(hit) = cache.borrow().get(&key) {
            return hit.clone();
        }
        let blocks = Rc::new(parse_blocks(text));
        let mut cache = cache.borrow_mut();
        if cache.len() >= 64 {
            cache.clear();
        }
        cache.insert(key, blocks.clone());
        blocks
    })
}

/// Waku `.markdown`: 14px/22px body. Spacing is graded by block pair rather
/// than one uniform gap, so a heading reads as a section start, a list hugs
/// the paragraph that introduces it, and consecutive paragraphs breathe.
///
/// Prose blocks (paragraphs, headings, lists, quotes, rules) cap at a
/// comfortable reading width; code, tables, and alerts use the full content
/// column. `collapsible` is false for live rows — streaming code blocks
/// never collapse out from under their own growing edge.
#[allow(clippy::too_many_arguments)]
fn render_prose(
    text: &str,
    ix: usize,
    salt: u64,
    theme: Theme,
    copied_sections: CopiedSections,
    expanded_blocks: ExpandedBlocks,
    collapsible: bool,
    scroller: MessageScrollerState,
) -> impl IntoElement + use<> {
    let blocks = parse_blocks_cached(text);
    let gaps: Vec<f32> = blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            if i == 0 {
                0.0
            } else {
                block_gap(&blocks[i - 1], block)
            }
        })
        .collect();
    div().w_full().min_w_0().flex().flex_col().children(
        blocks
            .iter()
            .enumerate()
            .map(move |(block_ix, block)| {
                let gap = gaps[block_ix];
                div()
                    .w_full()
                    .min_w_0()
                    .when(gap > 0.0, |node| node.mt(px(gap)))
                    .child(render_block(
                        block,
                        ix,
                        salt,
                        block_ix,
                        theme,
                        copied_sections.clone(),
                        expanded_blocks.clone(),
                        collapsible,
                        scroller.clone(),
                    ))
            })
            .collect::<Vec<_>>(),
    )
}

/// Render a standalone markdown document — a skill's `SKILL.md` body — with
/// the transcript's block renderer and no transcript-specific state, so the
/// two surfaces can never drift.
pub(crate) fn render_markdown_document(text: &str, theme: Theme) -> impl IntoElement + use<> {
    let copied: CopiedSections = Rc::new(RefCell::new(HashMap::new()));
    let expanded: ExpandedBlocks = Rc::new(RefCell::new(HashSet::new()));
    // Not collapsible: the page has no persistent fold state for a document
    // preview, and a fold control that could not remember its state would be
    // a broken affordance.
    render_prose(
        text,
        0,
        0,
        theme,
        copied,
        expanded,
        false,
        MessageScrollerState::new(1),
    )
}

/// Vertical space to place *above* `next`, given `prev`. More room above a
/// heading than below it, tight joins for lists and their introducer, and a
/// clear break around code and tables (the brief's rhythm table).
fn block_gap(prev: &Block, next: &Block) -> f32 {
    use Block::{Alert, Code, Heading, List, Paragraph, Quote, Rule, Table};
    match (prev, next) {
        (_, Heading(..)) => 20.0,
        (Heading(..), _) => 8.0,
        (Paragraph(..), Paragraph(..)) => 14.0,
        (_, Code(..)) => 12.0,
        (Code(..), _) => 14.0,
        (_, List(..)) => 10.0,
        (List(..), _) => 14.0,
        (_, Quote(..)) | (Quote(..), _) => 12.0,
        (_, Alert(..)) | (Alert(..), _) => 12.0,
        (_, Table { .. }) | (Table { .. }, _) => 14.0,
        (Rule, _) => 14.0,
        _ => 10.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_block(
    block: &Block,
    ix: usize,
    salt: u64,
    block_ix: usize,
    theme: Theme,
    copied_sections: CopiedSections,
    expanded_blocks: ExpandedBlocks,
    collapsible: bool,
    scroller: MessageScrollerState,
) -> AnyElement {
    match block {
        Block::Paragraph(lines) => paragraph_text(
            &lines.join(" "),
            14.,
            26.,
            FontWeight::NORMAL,
            theme.assistant_text,
            md_id(ix, salt, block_ix, 0),
            theme,
        )
        .into_any_element(),
        Block::Heading(level, text) => {
            let (size, line_height, color) = match level {
                1 => (20., 30., theme.text),
                2 => (18., 28., theme.text),
                3 => (16., 26., theme.text),
                4 => (15., 24., theme.text),
                _ => (14., 24., theme.text_2),
            };
            paragraph_text(
                text,
                size,
                line_height,
                FontWeight::SEMIBOLD,
                color,
                md_id(ix, salt, block_ix, 0),
                theme,
            )
            .into_any_element()
        }
        Block::Code(language, lines) => render_code_block(
            language.as_deref(),
            lines,
            ix,
            salt,
            block_ix,
            theme,
            copied_sections,
            expanded_blocks,
            collapsible,
            scroller,
        )
        .into_any_element(),
        Block::Rule => div().w_full().h(px(1.)).bg(theme.border).into_any_element(),
        Block::Alert(kind, lines) => {
            render_alert(*kind, lines, ix, salt, block_ix, theme).into_any_element()
        }
        Block::Quote(lines) => div()
            .w_full()
            .min_w_0()
            .border_l_2()
            .border_color(theme.text_3)
            .bg(theme.overlay)
            .rounded_r(px(8.))
            .pl(px(14.))
            .pr(px(12.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .children(lines.iter().enumerate().map(move |(sub, line)| {
                paragraph_text(
                    line,
                    14.,
                    26.,
                    FontWeight::NORMAL,
                    theme.text_2,
                    md_id(ix, salt, block_ix, sub),
                    theme,
                )
            }))
            .into_any_element(),
        Block::List(items) => div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(
                items
                    .iter()
                    .enumerate()
                    .map(move |(sub, item)| render_list_item(item, ix, salt, block_ix, sub, theme)),
            )
            .into_any_element(),
        Block::Table {
            header,
            rows,
            aligns,
        } => render_table(header, rows, aligns, ix, salt, block_ix, theme).into_any_element(),
    }
}

/// A GitHub-style callout: a tinted, rounded container with a semantic
/// icon + label and readable body copy. Deliberately no accent bar — the
/// tint, icon, and label carry the meaning without a heavy left edge.
fn render_alert(
    kind: AlertKind,
    lines: &[String],
    ix: usize,
    salt: u64,
    block_ix: usize,
    theme: Theme,
) -> impl IntoElement {
    let color = kind.color(theme);
    div()
        .w_full()
        .min_w_0()
        .rounded(px(10.))
        .bg(color.opacity(0.08))
        .px(px(12.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(glyph(kind.icon(), 14., color))
                .child(
                    div()
                        .text_size(theme.ui_px(12.))
                        .line_height(theme.ui_px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(kind.label()),
                ),
        )
        .children(lines.iter().enumerate().map(move |(sub, line)| {
            paragraph_text(
                line,
                14.,
                26.,
                FontWeight::NORMAL,
                theme.assistant_text,
                md_id(ix, salt, block_ix, sub),
                theme,
            )
        }))
}

/// `ml-5` bullet / numbered item with a hanging indent on wrap. GFM task
/// items swap the marker for a checkbox and mute checked text.
fn render_list_item(
    item: &ListItem,
    ix: usize,
    salt: u64,
    block_ix: usize,
    sub: usize,
    theme: Theme,
) -> AnyElement {
    let marker: AnyElement = match item.checked {
        Some(checked) => task_checkbox(checked, theme),
        None => {
            let text = if item.ordered {
                format!("{}.", item.number)
            } else {
                "•".to_string()
            };
            div()
                .text_size(theme.ui_px(14.))
                .line_height(theme.ui_px(26.))
                .text_color(if item.ordered {
                    theme.text
                } else {
                    theme.text_3
                })
                .child(text)
                .into_any_element()
        }
    };
    // Completed tasks recede but stay above the 4.5:1 body-contrast floor.
    let text_color = if item.checked == Some(true) {
        theme.text_2
    } else {
        theme.assistant_text
    };
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_start()
        .pl(px(20. + 20. * item.depth as f32))
        .child(
            div()
                .flex_none()
                .w(px(20.))
                .flex()
                .items_center()
                .justify_start()
                .child(marker),
        )
        .child(div().min_w_0().flex_1().child(paragraph_text(
            &item.text,
            14.,
            26.,
            FontWeight::NORMAL,
            text_color,
            md_id(ix, salt, block_ix, sub),
            theme,
        )))
        .into_any_element()
}

/// GFM task-list checkbox: accent-filled check when done, hairline empty box
/// when open. The `send_fg` check color stays legible on the accent in both
/// appearances.
fn task_checkbox(checked: bool, theme: Theme) -> AnyElement {
    let size = 14.0;
    let mut box_ = div()
        .flex_none()
        .size(px(size))
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center();
    if checked {
        box_ = box_
            .bg(theme.accent)
            .child(glyph("icons/check.svg", 10., theme.send_fg));
    } else {
        box_ = box_.border_1().border_color(theme.border_strong);
    }
    div()
        .h(px(26.))
        .flex()
        .items_center()
        .child(box_)
        .into_any_element()
}

/// A first-class code block: a title strip naming the language with a copy
/// affordance, then syntax-highlighted monospace lines that scroll
/// horizontally rather than wrapping or clipping. Highlight colors come from
/// [`Theme::token_color`], the same map the review diff uses, so a snippet
/// reads identically in the transcript and the side pane.
///
/// Settled blocks taller than [`CODE_COLLAPSE_LINES`] show a
/// [`CODE_PREVIEW_LINES`]-line preview with a "Show remaining N lines" bar;
/// copy always carries the complete code regardless of the fold.
#[allow(clippy::too_many_arguments)]
fn render_code_block(
    language: Option<&str>,
    lines: &[String],
    ix: usize,
    salt: u64,
    block_ix: usize,
    theme: Theme,
    copied_sections: CopiedSections,
    expanded_blocks: ExpandedBlocks,
    collapsible: bool,
    scroller: MessageScrollerState,
) -> impl IntoElement {
    let code = lines.join("\n");
    let tokens = language
        .and_then(highlight::lang_for_tag)
        .map(|lang| highlight::tokenize_cached(lang, &code));
    let label = code_block_label(language);
    // Block identity within the message: the prose salt scopes the index to
    // the step/user run it was parsed from, so two steps' third blocks
    // never share copy feedback, element state, or fold state.
    let block_key = salt as usize + block_ix;
    let copied = copied_sections
        .borrow()
        .get(&(ix, block_key, CODE_COPY_SECTION))
        .is_some_and(|at| at.elapsed() < COPY_FEEDBACK);
    let copy_button = div()
        .id(ElementId::NamedInteger(
            "copy-code".into(),
            (ix as u64) << 32 | block_key as u64,
        ))
        .flex_none()
        .size(px(CODE_COPY_BUTTON))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.bg(theme.overlay_strong))
        .child(glyph(
            if copied {
                "icons/check.svg"
            } else {
                "icons/copy.svg"
            },
            14.,
            if copied { theme.ok_green } else { theme.text_3 },
        ))
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
            copied_sections
                .borrow_mut()
                .insert((ix, block_key, CODE_COPY_SECTION), Instant::now());
            cx.refresh_windows();
        });
    // Title strip: the language name and the copy affordance in one row, so
    // context and action sit in the same place on every code block.
    let header = div()
        .w_full()
        .min_w_0()
        .h(px(34.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .bg(theme.overlay)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(theme.ui_px(11.5))
                .line_height(theme.ui_px(15.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_3)
                .child(label),
        )
        .child(copy_button);

    // Long settled blocks fold to a prefix preview; the fold state is keyed
    // by (message, prose run, block) so steps never collide.
    let folded = collapsible && lines.len() > CODE_COLLAPSE_LINES;
    let expanded = folded && expanded_blocks.borrow().contains(&(ix, salt, block_ix));
    let visible = if folded && !expanded {
        &lines[..CODE_PREVIEW_LINES]
    } else {
        lines
    };

    let mut card = div()
        .w_full()
        .min_w_0()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.code_bg)
        .overflow_hidden()
        .child(header)
        .child(
            div()
                .id(ElementId::NamedInteger(
                    "code-scroll".into(),
                    (ix as u64) << 32 | block_key as u64,
                ))
                .w_full()
                .min_w_0()
                .flex()
                .items_start()
                .overflow_x_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_none()
                        .px(px(16.))
                        .pt(px(8.))
                        .pb(px(12.))
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(13.))
                        .line_height(theme.code_px(21.))
                        .text_color(theme.code_text)
                        .children(visible.iter().enumerate().map(|(line_ix, line)| {
                            code_line(line, tokens.as_deref(), line_ix, theme)
                        })),
                ),
        );

    if folded {
        let hidden = lines.len() - CODE_PREVIEW_LINES;
        let label = if expanded {
            "Show less".to_string()
        } else {
            format!("Show remaining {hidden} lines")
        };
        card = card.child(
            div()
                .id(ElementId::NamedInteger(
                    "code-fold".into(),
                    (ix as u64) << 32 | block_key as u64,
                ))
                .w_full()
                .h(px(30.))
                .px(px(12.))
                .border_t_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .text_size(theme.ui_px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_2)
                .hover(|style| style.bg(theme.overlay).text_color(theme.text))
                .child(label)
                .child(div().flex_1())
                .child(glyph(
                    if expanded {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    11.,
                    theme.text_3,
                ))
                .on_click(move |_, _, cx| {
                    let key = (ix, salt, block_ix);
                    let mut open = expanded_blocks.borrow_mut();
                    if !open.remove(&key) {
                        open.insert(key);
                    }
                    drop(open);
                    scroller.remeasure_toggle(ix);
                    cx.refresh_windows();
                }),
        );
    }

    card
}

/// Display label for a code block's title strip: a friendly language name
/// for known fences, the raw tag when the language is unrecognized, and
/// `Plain text` for a bare fence.
fn code_block_label(language: Option<&str>) -> String {
    match language {
        // D2: no native diagram renderer. A mermaid fence stays a copyable
        // code block — the label says so instead of pretending to preview.
        Some(tag) if tag.eq_ignore_ascii_case("mermaid") => {
            "Mermaid · source (preview unavailable)".to_string()
        }
        Some(tag) => highlight::language_label(tag)
            .map(str::to_string)
            .unwrap_or_else(|| tag.to_string()),
        None => "Plain text".to_string(),
    }
}

/// One code line as syntax-colored `TextRun`s over the monospace face.
/// `whitespace_nowrap` keeps indentation exact and lets the block scroll
/// instead of wrap; an empty line carries a space so its row keeps height.
fn code_line(
    text: &str,
    tokens: Option<&Vec<Vec<Token>>>,
    line_ix: usize,
    theme: Theme,
) -> AnyElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .child(syntax_styled(text, tokens, line_ix, theme.code_text, theme))
        .into_any_element()
}

/// Build a line of syntax-colored text: plain gaps and token spans over the
/// monospace face, with an empty line carrying a space so its row keeps
/// height. Shared by code blocks (no-wrap) and tool detail (wrapping).
fn syntax_styled(
    text: &str,
    tokens: Option<&Vec<Vec<Token>>>,
    line_ix: usize,
    base: Hsla,
    theme: Theme,
) -> StyledText {
    let font = mono_font();
    let display = if text.is_empty() { " " } else { text };
    let mut runs: Vec<TextRun> = Vec::new();
    if let Some(spans) = tokens.and_then(|lines| lines.get(line_ix)) {
        let mut offset = 0usize;
        for token in spans {
            let start = token.range.start.min(text.len());
            let end = token.range.end.min(text.len());
            if start > offset {
                runs.push(code_run(start - offset, base, &font));
            }
            if end > start {
                runs.push(code_run(end - start, theme.token_color(token.class), &font));
            }
            offset = offset.max(end);
        }
        if offset < text.len() {
            runs.push(code_run(text.len() - offset, base, &font));
        }
    }
    if runs.is_empty() {
        runs.push(code_run(display.len(), base, &font));
    }
    StyledText::new(display.to_string()).with_runs(runs)
}

fn code_run(len: usize, color: Hsla, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn mono_font() -> Font {
    Font {
        family: theme::code_font_family(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

/// GFM table: outer border, semibold header, dividers, content-weighted
/// columns (`th`/`td` styling from Waku's `.markdown table` rules).
fn render_table(
    header: &[String],
    rows: &[Vec<String>],
    aligns: &[TableAlign],
    ix: usize,
    salt: u64,
    block_ix: usize,
    theme: Theme,
) -> AnyElement {
    let columns = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    // Column widths are estimated from the longest cell and fixed in px.
    // Narrow tables lay out as before (the last column absorbs the slack);
    // wide ones overflow into a horizontal scroller instead of crushing
    // every column into unreadable wraps.
    let mut widths = vec![96f32; columns];
    for (col, width) in widths.iter_mut().enumerate() {
        let mut longest = 4usize;
        if let Some(cell) = header.get(col) {
            longest = longest.max(cell.chars().count());
        }
        for row in rows {
            if let Some(cell) = row.get(col) {
                longest = longest.max(cell.chars().count());
            }
        }
        *width = (longest as f32 * 6.8 + 28.).clamp(96., 340.);
    }
    let make_cell = |text: &str,
                     widths: &[f32],
                     col: usize,
                     last: bool,
                     strong: bool,
                     align: TableAlign,
                     sub: usize,
                     salt: u64,
                     theme: Theme| {
        let cell = div()
            .px(px(12.))
            .py(px(8.))
            .text_align(align.text_align())
            .child(paragraph_text(
                text,
                13.,
                22.,
                if strong {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                },
                theme.assistant_text,
                md_id(ix, salt, block_ix, sub),
                theme,
            ));
        if last {
            // The last column grows to fill a too-wide conversation, so a
            // narrow table never leaves dead space inside its border.
            cell.flex_grow().flex_basis(px(0.)).min_w(px(widths[col]))
        } else {
            cell.flex_none().w(px(widths[col]))
        }
    };
    let align_at = |col: usize| aligns.get(col).copied().unwrap_or_default();
    let header_last = header.len().saturating_sub(1);
    let mut inner = div().flex_none().min_w_full().flex().flex_col();
    inner = inner.child(div().w_full().flex().bg(theme.overlay).children(
        header.iter().enumerate().map(|(col, text)| {
            make_cell(
                text,
                &widths,
                col,
                col == header_last,
                true,
                align_at(col),
                col,
                salt,
                theme,
            )
        }),
    ));
    for (row_ix, row) in rows.iter().enumerate() {
        let row_last = row.len().saturating_sub(1);
        inner = inner.child(
            div()
                .w_full()
                .flex()
                .border_t_1()
                .border_color(theme.border)
                .children(row.iter().enumerate().map(|(col, text)| {
                    make_cell(
                        text,
                        &widths,
                        col,
                        col == row_last,
                        false,
                        align_at(col),
                        64 + (row_ix * 64) + col,
                        salt,
                        theme,
                    )
                })),
        );
    }
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .id(ElementId::NamedInteger(
                    "table-scroll".into(),
                    ((ix as u64) << 40) | ((salt & 0xffff) << 24) | ((block_ix as u64) << 8),
                ))
                .w_full()
                .min_w_0()
                .overflow_x_scroll()
                .child(inner),
        )
        .into_any_element()
}
/// Waku-style footer stamp for the changed-files summary: `Today 1:15 PM`,
/// `Yesterday 6:07 PM`, then a short date (`Sep 6`) once past yesterday.
fn summary_time_label(millis: i64) -> String {
    summary_time_label_at(millis, chrono::Local::now())
}

fn summary_time_label_at(millis: i64, now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::{DateTime, Local, TimeZone};
    let Some(utc) = DateTime::from_timestamp_millis(millis) else {
        return String::new();
    };
    let dt: DateTime<Local> = Local.from_utc_datetime(&utc.naive_local());
    let days = (now.date_naive() - dt.date_naive()).num_days();
    let clock = dt.format("%-I:%M %p");
    match days {
        0 => format!("Today {clock}"),
        1 => format!("Yesterday {clock}"),
        _ => dt.format("%b %-d").to_string(),
    }
}

fn changed_files_title(count: usize) -> String {
    if count == 1 {
        "Changed 1 file".to_string()
    } else {
        format!("Changed {count} files")
    }
}

/// Show a changed-file path relative to the session workspace when possible.
fn workspace_relative_path(path: &str, workspace: Option<&Path>) -> String {
    let path = Path::new(path);
    if let Some(root) = workspace {
        if path.is_absolute() {
            if let Ok(rel) = path.strip_prefix(root) {
                let rel = rel.to_string_lossy();
                let rel = rel.trim_start_matches(['/', '\\']);
                if !rel.is_empty() {
                    return rel.replace('\\', "/");
                }
            }
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

/// Waku `ChangedFilesCard`: raised tile, "Changed N files" with a ±delta
/// underneath, a Review affordance, and roomy file rows with right-aligned
/// line counts. Shows 3 rows; expanded shows up to 12 with a clip note.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_changed_files(
    files: &[(String, u64, u64)],
    theme: Theme,
    message_ix: usize,
    workspace: Option<&Path>,
    expanded: bool,
    expanded_files: Rc<RefCell<HashSet<usize>>>,
    scroller: MessageScrollerState,
    review_changes: Option<ReviewOpener>,
) -> impl IntoElement {
    const EXPANDED_PREVIEW_LIMIT: usize = 12;
    let additions: u64 = files.iter().map(|(_, added, _)| *added).sum();
    let deletions: u64 = files.iter().map(|(_, _, removed)| *removed).sum();
    let title = changed_files_title(files.len());
    let can_expand = files.len() > CHANGED_FILES_PREVIEW_LIMIT;
    let visible = if expanded {
        files.len().min(EXPANDED_PREVIEW_LIMIT)
    } else {
        files.len().min(CHANGED_FILES_PREVIEW_LIMIT)
    };

    let mut rows = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(theme.tool_border);
    for (path, added, removed) in files.iter().take(visible) {
        let label = workspace_relative_path(path, workspace);
        rows = rows.child(
            div()
                .h(px(32.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_size(theme.ui_px(11.5))
                        .line_height(theme.ui_px(16.))
                        .text_color(theme.text_2)
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(theme.ui_px(10.5))
                        .text_color(theme.add_green)
                        .child(format!("+{added}")),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(theme.ui_px(10.5))
                        .text_color(theme.del_red)
                        .child(format!("-{removed}")),
                ),
        );
    }

    // Review affordance: opens the changed-files diff in the right side
    // pane's Review tab (Waku parity — no external editor hop).
    let review = review_changes.map(|review| {
        div()
            .id(ElementId::NamedInteger(
                "review-changes".into(),
                message_ix as u64,
            ))
            .h(px(28.))
            .px(px(10.))
            .rounded(px(7.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.overlay)
            .flex()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            .text_size(theme.ui_px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.text_2)
            .hover(|style| style.bg(theme.overlay_strong).text_color(theme.text))
            .child(glyph("icons/file-diff.svg", 12., theme.text_3))
            .child("Review")
            .on_click(move |_, window, cx| review(window, cx))
    });

    let mut header = div()
        .min_h(px(56.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .size(px(34.))
                .flex_none()
                .rounded(px(9.))
                .bg(theme.accent.opacity(0.14))
                .flex()
                .items_center()
                .justify_center()
                .child(glyph("icons/file-diff.svg", 15., theme.accent)),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(theme.ui_px(13.))
                        .line_height(theme.ui_px(17.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.))
                        .flex()
                        .gap(px(6.))
                        .text_size(theme.ui_px(11.))
                        .line_height(theme.ui_px(14.))
                        .child(
                            div()
                                .text_color(theme.add_green)
                                .child(format!("+{additions}")),
                        )
                        .child(
                            div()
                                .text_color(theme.del_red)
                                .child(format!("-{deletions}")),
                        ),
                ),
        );
    if let Some(review) = review {
        header = header.child(review);
    }

    let mut card = div()
        .w_full()
        .min_w_0()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.bg_raised)
        .shadow(theme.card_shadow())
        .overflow_hidden()
        .child(header)
        .child(rows);

    if can_expand {
        let remaining = files.len() - CHANGED_FILES_PREVIEW_LIMIT;
        let label = if expanded {
            "Show fewer files".to_string()
        } else if remaining == 1 {
            "Show 1 more file".to_string()
        } else {
            format!("Show {remaining} more files")
        };
        let clipped = expanded && files.len() > EXPANDED_PREVIEW_LIMIT;
        let mut toggle = div()
            .id(ElementId::NamedInteger(
                "changed-files-toggle".into(),
                message_ix as u64,
            ))
            .h(px(34.))
            .px(px(14.))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_size(theme.ui_px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.text_2)
            .hover(|style| style.bg(theme.bg_hover).text_color(theme.text))
            .child(label);
        if clipped {
            toggle = toggle.child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .font_weight(FontWeight::NORMAL)
                    .text_size(theme.ui_px(11.5))
                    .text_color(theme.text_3)
                    .child(format!(
                        "Showing first {EXPANDED_PREVIEW_LIMIT} of {}",
                        files.len()
                    )),
            );
        }
        card = card.child(
            toggle
                .child(div().flex_1())
                .child(glyph(
                    if expanded {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    11.,
                    theme.text_3,
                ))
                .on_click(move |_, _, cx| {
                    toggle_index(&expanded_files, message_ix);
                    scroller.remeasure_toggle(message_ix);
                    cx.refresh_windows();
                }),
        );
    }

    card
}

fn toggle_index(set: &Rc<RefCell<HashSet<usize>>>, ix: usize) {
    let mut set = set.borrow_mut();
    if !set.remove(&ix) {
        set.insert(ix);
    }
}

pub(crate) fn activity_icon(name: &str) -> &'static str {
    match name {
        "edit" | "write" => "icons/file-diff.svg",
        "read" | "grep" | "find" | "glob" | "search" => "icons/search.svg",
        "bash" | "shell" => "icons/spark.svg",
        _ => "icons/task.svg",
    }
}

fn activity_action_label(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Tool".to_string(),
    }
}

pub(crate) fn active_user_index(
    messages: &[ChatMessage],
    streaming: Option<usize>,
    viewport_hint: Option<usize>,
) -> Option<usize> {
    if messages.is_empty() {
        return None;
    }
    // The reader scrolled away from the live edge: the active tick is the
    // turn the viewport is reading — the newest user turn at or above the
    // first visible row — not the latest turn in the transcript.
    if streaming.is_none() {
        if let Some(first_visible) = viewport_hint {
            let clamped = first_visible.min(messages.len() - 1);
            if let Some(ix) = messages[..clamped + 1]
                .iter()
                .enumerate()
                .rev()
                .find(|(_, message)| message.user)
                .map(|(ix, _)| ix)
            {
                return Some(ix);
            }
        }
    }
    let end = streaming
        .unwrap_or(messages.len() - 1)
        .min(messages.len() - 1);
    messages[..end + 1]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| message.user)
        .map(|(ix, _)| ix)
}

fn starts_followup_turn(messages: &[ChatMessage], ix: usize) -> bool {
    ix > 0 && messages[ix].user && !messages[ix - 1].user
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_label_falls_back_without_elapsed() {
        assert_eq!(fold_label(None), "Worked");
        assert_eq!(
            fold_label(Some(Duration::from_secs(341))),
            "Worked for 5 minutes 41 seconds"
        );
    }

    #[test]
    fn duration_format_matches_waku_spoken_forms() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1 second");
        assert_eq!(format_duration(Duration::from_secs(12)), "12 seconds");
        assert_eq!(format_duration(Duration::from_secs(60)), "1 minute");
        assert_eq!(
            format_duration(Duration::from_secs(100)),
            "1 minute 40 seconds"
        );
        assert_eq!(
            format_duration(Duration::from_secs(341)),
            "5 minutes 41 seconds"
        );
        assert_eq!(format_duration(Duration::from_secs(3_600)), "1 hour");
        assert_eq!(
            format_duration(Duration::from_secs(3_721)),
            "1 hour 2 minutes"
        );
    }

    #[test]
    fn working_elapsed_uses_waku_short_form() {
        assert_eq!(format_working_elapsed(Duration::from_secs(0)), "0s");
        assert_eq!(format_working_elapsed(Duration::from_secs(12)), "12s");
        assert_eq!(format_working_elapsed(Duration::from_secs(60)), "1m");
        assert_eq!(format_working_elapsed(Duration::from_secs(341)), "5m 41s");
        assert_eq!(format_working_elapsed(Duration::from_secs(3_600)), "1h");
        assert_eq!(format_working_elapsed(Duration::from_secs(3_721)), "1h 2m");
    }

    #[test]
    fn changed_files_title_uses_screenshot_copy() {
        assert_eq!(changed_files_title(1), "Changed 1 file");
        assert_eq!(changed_files_title(2), "Changed 2 files");
    }

    /// The run footer (copy + time + usage) lives once, on the tail summary;
    /// the last message's own footer steps aside so the details don't repeat.
    #[test]
    fn tail_summary_owns_the_single_footer() {
        // No summary: every message keeps its footer.
        assert!(!suppress_message_footer(false, 0, 3));
        assert!(!suppress_message_footer(false, 2, 3));
        // Summary present: only the last message's footer is suppressed.
        assert!(!suppress_message_footer(true, 0, 3));
        assert!(!suppress_message_footer(true, 1, 3));
        assert!(suppress_message_footer(true, 2, 3));
    }

    #[test]
    fn workspace_relative_path_strips_workspace_prefix() {
        // `Path::is_absolute` needs a drive prefix on Windows, so the root is
        // built from a path that is absolute on whatever host runs the test.
        let root = crate::platform::home_dir().join("orbit-ws");
        let file = root.join("src").join("main.rs");
        assert_eq!(
            workspace_relative_path(&file.to_string_lossy(), Some(&root)),
            "src/main.rs"
        );
        assert_eq!(
            workspace_relative_path("src/main.rs", Some(&root)),
            "src/main.rs"
        );
    }

    #[test]
    fn message_cost_keeps_sub_cent_precision() {
        assert_eq!(format_message_cost(0.0), "$0.00");
        assert_eq!(format_message_cost(0.0012), "$0.0012");
        assert_eq!(format_message_cost(0.45), "$0.45");
        assert_eq!(format_message_cost(12.345), "$12.35");
        assert_eq!(format_message_cost(250.0), "$250");
        assert_eq!(format_message_cost(f64::NAN), "$0.00");
    }

    #[test]
    fn summary_time_label_matches_screenshot_copy() {
        use chrono::{Duration, TimeZone, Timelike, Utc};
        let at = |y: i32, m: u32, d: u32, h: u32, min: u32| {
            Utc.with_ymd_and_hms(y, m, d, h, min, 0)
                .single()
                .unwrap()
                .with_timezone(&chrono::Local)
        };
        // Bucket boundaries are relative to `now`, so the test is
        // timezone-independent.
        let now = at(2026, 9, 8, 16, 0);
        let millis = |dt: chrono::DateTime<chrono::Local>| dt.timestamp_millis();
        // 18:07 local yesterday — the screenshot's "Yesterday 6:07 PM".
        let yesterday = (now - Duration::days(1))
            .with_hour(18)
            .unwrap()
            .with_minute(7)
            .unwrap();
        assert_eq!(
            summary_time_label_at(millis(yesterday), now),
            "Yesterday 6:07 PM"
        );
        // Same day → Today + 12-hour clock, no leading zero on the hour.
        let today = now.with_hour(13).unwrap().with_minute(5).unwrap();
        assert_eq!(summary_time_label_at(millis(today), now), "Today 1:05 PM");
        // Older than yesterday falls back to a short date.
        let older = now - Duration::days(3);
        assert_eq!(
            summary_time_label_at(millis(older), now),
            older.format("%b %-d").to_string()
        );
    }

    #[test]
    fn activity_icon_maps_pi_tools() {
        assert_eq!(activity_icon("edit"), "icons/file-diff.svg");
        assert_eq!(activity_icon("bash"), "icons/spark.svg");
        assert_eq!(activity_icon("grep"), "icons/search.svg");
        assert_eq!(activity_icon("mcp"), "icons/task.svg");
    }

    #[test]
    fn command_tools_surface_the_shell_command_not_json() {
        let bash = ToolCall {
            name: "bash".into(),
            summary: r#"{"command":"ls -la"}"#.into(),
            path: None,
            added: 0,
            removed: 0,
            id: None,
            args: Some(serde_json::json!({ "command": "ls -la" })),
            output: None,
            failed: false,
            duration: None,
        };
        assert_eq!(tool_command(&bash).as_deref(), Some("ls -la"));
        // The header preview shows the command, not the raw JSON summary.
        assert_eq!(activity_preview(&bash), "ls -la");
        // Non-command tools do not fabricate a command.
        let edit = ToolCall {
            name: "edit".into(),
            ..bash.clone()
        };
        assert_eq!(tool_command(&edit), None);
    }

    #[test]
    fn line_diff_keeps_context_around_an_insertion() {
        let old =
            "let columns = self.tool_columns();\nlet mut elements = Vec::new();\nfor row in rows {";
        let new = "let columns = self.tool_columns();\nlet total_calls = snapshot.tools.calls.max(1);\nlet mut elements = Vec::new();\nfor row in rows {";
        let rows = line_diff(old, new, Some(highlight::Lang::Rust));
        let kinds: Vec<DiffRowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(
            kinds,
            vec![
                DiffRowKind::Context,
                DiffRowKind::Addition,
                DiffRowKind::Context,
                DiffRowKind::Context,
            ]
        );
        assert_eq!(
            rows[1].text,
            "let total_calls = snapshot.tools.calls.max(1);"
        );
        // The line is lexed, so the keyword carries a token.
        assert!(rows[1].tokens.as_ref().is_some_and(|t| !t.is_empty()));
    }

    #[test]
    fn build_edit_diff_walks_pi_edit_arguments() {
        let tool = ToolCall {
            name: "edit".into(),
            summary: String::new(),
            path: Some("src/view.rs".into()),
            added: 1,
            removed: 1,
            id: None,
            args: Some(serde_json::json!({
                "path": "src/view.rs",
                "edits": [{ "oldText": "let a = 1;", "newText": "let a = 2;" }]
            })),
            output: None,
            failed: false,
            duration: None,
        };
        let rows = build_edit_diff(&tool).expect("diff");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, DiffRowKind::Deletion);
        assert_eq!(rows[1].kind, DiffRowKind::Addition);
        assert_eq!(edit_patch_text(&rows), "-let a = 1;\n+let a = 2;\n");
        // A read tool has no edit text to show.
        let read = ToolCall {
            name: "read".into(),
            ..tool
        };
        assert!(build_edit_diff(&read).is_none());
    }

    #[test]
    fn write_diff_marks_every_line_added() {
        let tool = ToolCall {
            name: "write".into(),
            summary: String::new(),
            path: Some("src/new.rs".into()),
            added: 2,
            removed: 0,
            id: None,
            args: Some(serde_json::json!({ "path": "src/new.rs", "content": "a\nb\n" })),
            output: None,
            failed: false,
            duration: None,
        };
        let rows = build_edit_diff(&tool).expect("diff");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.kind == DiffRowKind::Addition));
        assert_eq!(rows[1].text, "b");
    }

    #[test]
    fn section_lang_only_highlights_structured_values() {
        assert_eq!(section_lang(None), None);
        assert_eq!(
            section_lang(Some(&Value::String("raw output".into()))),
            None
        );
        assert_eq!(
            section_lang(Some(&serde_json::json!({ "a": 1 }))),
            Some(highlight::Lang::Json)
        );
    }

    #[test]
    fn first_error_line_picks_first_nonempty_string() {
        assert_eq!(
            first_error_line(&Value::String(
                "error: file not found\n  at main.rs:1".into()
            )),
            Some("error: file not found".to_string())
        );
        // Structured results dig out the first text-bearing value.
        assert_eq!(
            first_error_line(&serde_json::json!({"message": {"text": "boom"}})),
            Some("boom".to_string())
        );
        assert_eq!(first_error_line(&serde_json::json!(null)), None);
    }

    #[test]
    fn active_user_index_tracks_streaming_turn() {
        let messages = vec![
            ChatMessage {
                user: true,
                steps: vec![Step {
                    text: "a".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: false,
                steps: vec![Step {
                    text: "b".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: true,
                steps: vec![Step {
                    text: "c".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: false,
                steps: vec![Step {
                    text: "d".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
        ];
        assert_eq!(active_user_index(&messages, Some(1), None), Some(0));
        assert_eq!(active_user_index(&messages, Some(3), None), Some(2));
        assert_eq!(active_user_index(&messages, None, None), Some(2));
    }

    #[test]
    fn active_user_index_follows_viewport_when_scrolled() {
        let messages = vec![
            ChatMessage {
                user: true,
                steps: vec![Step {
                    text: "a".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: false,
                steps: vec![Step {
                    text: "b".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: true,
                steps: vec![Step {
                    text: "c".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: false,
                steps: vec![Step {
                    text: "d".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
        ];
        // Reader scrolled back to the first turn: the tick for turn 0 is
        // active even though the newest turn is turn 2.
        assert_eq!(active_user_index(&messages, None, Some(0)), Some(0));
        // Still reading within the first turn's run (row 1).
        assert_eq!(active_user_index(&messages, None, Some(1)), Some(0));
        // Reached the second user turn: its tick takes over.
        assert_eq!(active_user_index(&messages, None, Some(2)), Some(2));
        assert_eq!(active_user_index(&messages, None, Some(3)), Some(2));
        // Out-of-range hint clamps to the last row.
        assert_eq!(active_user_index(&messages, None, Some(99)), Some(2));
    }

    #[test]
    fn parse_inline_extracts_bold_code_and_links() {
        let spans = parse_inline("plain **bold** and `code` and [x](https://a.b)");
        assert_eq!(spans.len(), 6);
        assert_eq!(spans[0].text, "plain ");
        assert!(spans[1].bold);
        assert_eq!(spans[1].text, "bold");
        assert!(spans[3].code);
        assert_eq!(spans[3].text, "code");
        assert_eq!(spans[5].link.as_deref(), Some("https://a.b"));
        assert_eq!(spans[5].text, "x");
    }

    #[test]
    fn parse_inline_keeps_unmatched_markers_literal() {
        let spans = parse_inline("a * b ` c [d");
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "a * b ` c [d");
        assert!(spans.iter().all(|s| !s.bold && !s.code && s.link.is_none()));
    }

    #[test]
    fn parse_inline_escapes_render_literally() {
        let spans = parse_inline(r"\*not bold\*");
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "*not bold*");
        assert!(spans.iter().all(|s| !s.bold));
    }

    #[test]
    fn parse_blocks_splits_paragraphs_headings_and_fences() {
        let blocks = parse_blocks("one\ntwo\n\n## Title\n\n```rs\nlet a = 1;\n```");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], Block::Paragraph(lines) if lines.join(" ") == "one two"));
        assert!(matches!(&blocks[1], Block::Heading(2, title) if title == "Title"));
        assert!(
            matches!(&blocks[2], Block::Code(Some(lang), lines) if lang == "rs"
            && lines == &["let a = 1;".to_string()])
        );
        // A bare fence carries no language; the chip is simply absent.
        let bare = parse_blocks("```\nx\n```");
        assert!(matches!(&bare[0], Block::Code(None, lines) if lines == &["x".to_string()]));
    }

    #[test]
    fn mermaid_fences_stay_copyable_code_blocks() {
        // D2: diagrams are never faked. A mermaid fence parses as an ordinary
        // code block (so it is copyable) and its label is honest about the
        // missing preview.
        let blocks = parse_blocks("```mermaid\ngraph TD; A-->B;\n```");
        assert!(matches!(&blocks[0], Block::Code(Some(lang), lines)
                if lang == "mermaid" && lines == &["graph TD; A-->B;".to_string()]));
        assert_eq!(
            code_block_label(Some("Mermaid")),
            "Mermaid · source (preview unavailable)"
        );
    }

    #[test]
    fn parse_blocks_collects_loose_and_nested_lists() {
        let blocks = parse_blocks("- a\n- b\n\n  - nested\n\n3. third\n4. fourth");
        assert_eq!(blocks.len(), 2);
        let Block::List(items) = &blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 3);
        assert!(!items[0].ordered);
        assert_eq!(items[2].depth, 1);
        assert_eq!(items[2].text, "nested");
        let Block::List(ordered) = &blocks[1] else {
            panic!("expected ordered list");
        };
        assert!(ordered[0].ordered);
        assert_eq!(ordered[0].number, 3);
    }

    #[test]
    fn parse_blocks_handles_rules_quotes_and_tables() {
        let blocks =
            parse_blocks("---\n\n> quoted\n> lines\n\n| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert!(matches!(blocks[0], Block::Rule));
        assert!(matches!(&blocks[1], Block::Quote(lines) if lines.len() == 2));
        let Block::Table { header, rows, .. } = &blocks[2] else {
            panic!("expected table");
        };
        assert_eq!(header, &["a", "b"]);
        assert_eq!(rows, &[vec!["1".to_string(), "2".to_string()]]);
    }

    #[test]
    fn parse_blocks_reads_task_list_state() {
        let blocks = parse_blocks("- [ ] open\n- [x] done\n- plain");
        let Block::List(items) = &blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(items[0].checked, Some(false));
        assert_eq!(items[0].text, "open");
        assert_eq!(items[1].checked, Some(true));
        assert_eq!(items[1].text, "done");
        assert_eq!(items[2].checked, None);
        assert_eq!(items[2].text, "plain");
    }

    #[test]
    fn parse_blocks_reads_table_alignment() {
        let blocks = parse_blocks("| a | b | c |\n| :-- | :-: | --: |\n| 1 | 2 | 3 |");
        let Block::Table { aligns, .. } = &blocks[0] else {
            panic!("expected table");
        };
        assert_eq!(
            aligns,
            &[TableAlign::Left, TableAlign::Center, TableAlign::Right]
        );
    }

    #[test]
    fn parse_blocks_detects_github_alerts() {
        let blocks = parse_blocks("> [!WARNING]\n> This overwrites the file.");
        let Block::Alert(kind, lines) = &blocks[0] else {
            panic!("expected alert");
        };
        assert_eq!(*kind, AlertKind::Warning);
        assert_eq!(lines, &["This overwrites the file.".to_string()]);
        // A plain blockquote stays a quote.
        let quote = parse_blocks("> just a quote");
        assert!(matches!(&quote[0], Block::Quote(_)));
        // An unknown marker is body text, not an alert.
        let unknown = parse_blocks("> [!SOMETHING]\n> x");
        assert!(matches!(&unknown[0], Block::Quote(_)));
    }

    #[test]
    fn code_block_label_prefers_friendly_names() {
        assert_eq!(code_block_label(Some("ts")), "TypeScript");
        assert_eq!(code_block_label(Some("rs")), "Rust");
        // Unknown fence: keep the raw tag rather than dropping context.
        assert_eq!(code_block_label(Some("brainfuck")), "brainfuck");
        // Bare fence: a generic label keeps the header consistent.
        assert_eq!(code_block_label(None), "Plain text");
    }

    #[test]
    fn block_rhythm_separates_headings_more_than_paragraphs() {
        let para = || Block::Paragraph(vec!["x".into()]);
        let heading = || Block::Heading(2, "h".into());
        let list = || Block::List(Vec::new());
        assert!(block_gap(&para(), &heading()) > block_gap(&para(), &para()));
        assert!(block_gap(&heading(), &para()) < block_gap(&para(), &heading()));
        assert!(block_gap(&para(), &list()) < block_gap(&list(), &para()));
    }

    #[test]
    fn inline_runs_pairs_link_ranges_with_urls() {
        let theme = Theme::dark();
        let spans = parse_inline("see [docs](https://pi.dev) now");
        let (body, runs, links) = inline_runs(&spans, FontWeight::NORMAL, theme.text, theme);
        assert_eq!(body.as_ref(), "see docs now");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].1, "https://pi.dev");
        assert_eq!(&body[links[0].0.clone()], "docs");
        // Underline style rides on the link run.
        assert!(runs.iter().any(|run| run.underline.is_some()));
    }

    #[test]
    fn followup_gap_is_user_after_assistant() {
        let messages = vec![
            ChatMessage {
                user: true,
                steps: vec![Step {
                    text: "a".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: false,
                steps: vec![Step {
                    text: "b".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
            ChatMessage {
                user: true,
                steps: vec![Step {
                    text: "c".into(),
                    ..Step::default()
                }],
                elapsed: None,
                images: Vec::new(),
                finished_at: None,
                error: None,
                aborted: false,
            },
        ];
        assert!(!starts_followup_turn(&messages, 0));
        assert!(!starts_followup_turn(&messages, 1));
        assert!(starts_followup_turn(&messages, 2));
    }

    #[test]
    fn working_indicator_names_the_live_tool() {
        let bash = ToolCall {
            name: "bash".into(),
            summary: r#"{"command":"cargo test"}"#.into(),
            path: None,
            added: 0,
            removed: 0,
            id: None,
            args: Some(serde_json::json!({ "command": "cargo test" })),
            output: None,
            failed: false,
            duration: None,
        };
        let step = Step {
            tools: vec![bash],
            ..Step::default()
        };
        assert_eq!(
            working_activity_label(&step).as_deref(),
            Some("Running cargo test")
        );

        let read = ToolCall {
            name: "read".into(),
            summary: "src/auth.rs".into(),
            path: Some("src/auth.rs".into()),
            added: 0,
            removed: 0,
            id: None,
            args: Some(serde_json::json!({ "path": "src/auth.rs" })),
            output: None,
            failed: false,
            duration: None,
        };
        let step = Step {
            tools: vec![read],
            ..Step::default()
        };
        assert_eq!(
            working_activity_label(&step).as_deref(),
            Some("Reading src/auth.rs")
        );

        // Thinking-only work reads as thinking; an empty step yields no
        // label so the caller falls back to the generic working form.
        let thinking = Step {
            thinking: "reasoning".into(),
            ..Step::default()
        };
        assert_eq!(
            working_activity_label(&thinking).as_deref(),
            Some("Thinking…")
        );
        assert_eq!(working_activity_label(&Step::default()), None);
    }

    #[test]
    fn parse_cache_returns_the_same_blocks_for_the_same_text() {
        let text = "# Title\n\nSome **bold** prose.\n\n- one\n- two\n";
        let first = parse_blocks_cached(text);
        let second = parse_blocks_cached(text);
        // Same content, shared allocation — the streaming hot path must not
        // re-parse an unchanged message every frame.
        assert!(Rc::ptr_eq(&first, &second));
        assert!(matches!(first[0], Block::Heading(1, _)));
        let third = parse_blocks_cached("# Title\n\nchanged\n");
        assert!(!Rc::ptr_eq(&first, &third));
    }
}
