//! Git branch picker — search, checkout, and create-from-current-state.
//!
//! Branches arrive newest-commit first and each row carries that commit's age,
//! so the list answers "which branch was I last working on" before the user
//! reads a single name. Opened from the status-bar branch chip; follows the
//! same popover conventions as [`crate::command_palette::CommandPalette`].

use gpui::{
    div, point, prelude::*, px, App, Context, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, IntoElement, MouseDownEvent, ParentElement, Pixels, Render, ScrollHandle, Styled,
    Subscription, Window,
};

use crate::app::helpers::text_width;
use crate::app::{
    button_frame, icon, input_field_frame, menu_header, picker_entry, picker_search_frame,
    picker_surface, ControlExt, PressedRamp, StatefulControlExt,
};
use crate::composer::ComposerInput;
use crate::git::BranchEntry;
use crate::theme::tokens::RaisedExt;
use crate::theme::tokens::{
    context_menu, input, list, list_item, picker, ButtonSize, DynamicSpacing, TextSize,
};
use crate::theme::{self, Theme};

const POPOVER_W: f32 = 320.;
/// Branch rows visible before the list scrolls.
const VISIBLE_ROWS: f32 = 7.;

/// A branch row: a one-line picker entry. Layout and the keyboard reveal
/// math both read these, so they can never disagree.
fn row_h(theme: &Theme) -> f32 {
    picker::entry_height(theme).into()
}

/// Air between branch rows.
fn row_gap(theme: &Theme) -> f32 {
    DynamicSpacing::Base01.px(theme).into()
}

fn row_stride(theme: &Theme) -> f32 {
    row_h(theme) + row_gap(theme)
}

/// Largest list height before it scrolls.
fn list_max_h(theme: &Theme) -> f32 {
    VISIBLE_ROWS * row_stride(theme)
}

/// The name column's width: the popover's inner width less the row's chrome —
/// the two hairline borders, the row's outer inset and inner padding, the
/// branch mark, the reserved check slot, the gaps between them, and the age
/// column. Stated once so the name and its truncation always agree.
fn name_column_w(theme: &Theme, age_w: Pixels) -> Pixels {
    let gap = list_item::content_gap(theme);
    (px(POPOVER_W)
        - px(2.)
        - 2. * list_item::inset(theme)
        - 2. * list_item::padding_x(theme)
        - 2. * context_menu::ICON.px(theme)
        - 3. * gap
        - age_w)
        .max(px(0.))
}

/// Checkout/create callback: the chosen branch plus the ambient window.
type BranchAction = Box<dyn Fn(String, &mut Window, &mut App)>;
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type BranchPickerDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

/// One branch row: the name plus its "last active" age, both resolved once
/// when the picker opens (a popover never lives long enough for the label to
/// go stale).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    name: String,
    /// Age of the tip commit in words (`3d ago`), `None` when git has no date.
    age: Option<String>,
}

impl Row {
    fn new(entry: BranchEntry) -> Self {
        Self {
            age: entry.commit_date.as_deref().map(crate::gh::relative_time),
            name: entry.name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Browse,
    Create,
}

pub struct BranchPicker {
    branches: Vec<Row>,
    current: String,
    filter: Entity<ComposerInput>,
    create_input: Entity<ComposerInput>,
    mode: Mode,
    list_scroll: ScrollHandle,
    highlighted: usize,
    last_filter: String,
    _filter_sub: Subscription,
    on_checkout: BranchAction,
    on_create: BranchAction,
    on_dismiss: BranchPickerDismiss,
}

impl BranchPicker {
    pub fn new(
        workspace_label: String,
        branches: Vec<BranchEntry>,
        current: String,
        on_checkout: BranchAction,
        on_create: BranchAction,
        on_dismiss: BranchPickerDismiss,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("branch_picker.search_branches")
                .with_placeholder_var("workspace", workspace_label)
                .with_key_context("Composer Picker")
        });
        // Keep the derived filter state in sync from the input's notifications
        // so `render` only reads it.
        let filter_sub = cx.observe(&filter, |picker: &mut Self, filter, cx| {
            let needle = filter.read(cx).text().to_lowercase();
            if needle != picker.last_filter {
                picker.last_filter = needle;
                picker.highlighted = 0;
                picker.list_scroll.set_offset(point(px(0.), px(0.)));
                cx.notify();
            }
        });
        let create_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("branch_picker.new_branch_name")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        Self {
            branches: branches.into_iter().map(Row::new).collect(),
            current,
            filter,
            create_input,
            mode: Mode::Browse,
            list_scroll: ScrollHandle::new(),
            highlighted: 0,
            last_filter: String::new(),
            _filter_sub: filter_sub,
            on_checkout,
            on_create,
            on_dismiss,
        }
    }

    fn on_cancel(&mut self, _: &crate::PickerCancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Create {
            self.mode = Mode::Browse;
            self.highlighted = 0;
            window.focus(&self.filter.read(cx).focus_handle(cx));
            cx.notify();
            return;
        }
        (self.on_dismiss)(false, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Create {
            // Enter confirms an IME candidate before it names a branch: a
            // half-composed name must not create one (issue #100).
            if self.create_input.read(cx).is_composing() {
                return;
            }
            let name = self.create_input.read(cx).text().trim().to_string();
            if !name.is_empty() {
                (self.on_create)(name, window, cx);
            }
            return;
        }
        if self.filter.read(cx).is_composing() {
            return;
        }
        let rows = self.filtered_branches(&self.last_filter);
        if let Some(branch) = rows.get(self.highlighted).cloned() {
            if branch.name != self.current {
                (self.on_checkout)(branch.name, window, cx);
            } else {
                (self.on_dismiss)(false, window, cx);
            }
        }
    }

    fn on_next(&mut self, _: &crate::PickerSelectNext, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Browse {
            return;
        }
        self.step(1, cx);
    }

    fn on_prev(&mut self, _: &crate::PickerSelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Browse {
            return;
        }
        self.step(-1, cx);
    }

    fn on_outside_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(true, window, cx);
    }

    fn filtered_branches(&self, needle: &str) -> Vec<Row> {
        self.branches
            .iter()
            .filter(|branch| needle.is_empty() || branch.name.to_lowercase().contains(needle))
            .cloned()
            .collect()
    }

    fn step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let rows = self.filtered_branches(&self.last_filter);
        if rows.is_empty() {
            return;
        }
        let pos = self.highlighted.min(rows.len() - 1);
        let next = if dir > 0 {
            (pos + 1).min(rows.len() - 1)
        } else {
            pos.saturating_sub(1)
        };
        self.highlighted = next;
        let theme = theme::get(cx);
        let (row_h, row_gap) = (row_h(theme), row_gap(theme));
        let row_top = next as f32 * row_stride(theme);
        // `ScrollHandle`'s offset is negative once scrolled down.
        let current = -f32::from(self.list_scroll.offset().y);
        let n = rows.len() as f32;
        let content_h = (n * row_h + (n - 1.).max(0.) * row_gap).max(0.);
        let viewport_h = content_h.min(list_max_h(theme));
        let mut offset = current;
        if row_top < current {
            offset = row_top;
        } else if row_top + row_h > current + viewport_h {
            offset = row_top + row_h - viewport_h;
        }
        let max_offset = (content_h - viewport_h).max(0.);
        self.list_scroll
            .set_offset(point(px(0.), px(-offset.clamp(0., max_offset))));
        cx.notify();
    }

    fn begin_create(
        &mut self,
        _: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = Mode::Create;
        self.create_input.update(cx, |input, cx| {
            input.clear(cx);
        });
        window.focus(&self.create_input.read(cx).focus_handle(cx));
        cx.notify();
    }
}

impl Focusable for BranchPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.mode {
            Mode::Browse => self.filter.read(cx).focus_handle(cx),
            Mode::Create => self.create_input.read(cx).focus_handle(cx),
        }
    }
}

impl Render for BranchPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *theme::get(cx);
        let this = cx.entity();

        if self.mode == Mode::Create {
            // The same picker surface as Browse, so switching modes never
            // swaps the popover's chrome.
            return picker_surface(div(), &theme)
                .debug_selector(|| "branch-picker".to_string())
                .w(px(POPOVER_W))
                .py(DynamicSpacing::Base06.px(&theme))
                .flex()
                .flex_col()
                .overflow_hidden()
                .occlude()
                .on_mouse_down_out(cx.listener(Self::on_outside_down))
                .on_action(cx.listener(Self::on_cancel))
                .on_action(cx.listener(Self::on_confirm))
                .child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .pt(DynamicSpacing::Base04.px(&theme))
                        .pb(DynamicSpacing::Base08.px(&theme))
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(tr!("branch_picker.create_and_checkout_new_branch")),
                        )
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(tr!("branch_picker.uncommitted_changes_come_with_you")),
                        )
                        .child(
                            input_field_frame(div(), &theme)
                                .w_full()
                                .bg(theme.background)
                                .child(self.create_input.clone()),
                        ),
                )
                .child(
                    button_frame(div(), &theme, ButtonSize::Medium)
                        .mx(DynamicSpacing::Base08.px(&theme))
                        .mb(DynamicSpacing::Base04.px(&theme))
                        .raised(theme.action, &theme)
                        .hover_action(&theme)
                        .cursor_pointer()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.action_fg)
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|picker, _, window, cx| {
                                let name = picker.create_input.read(cx).text().trim().to_string();
                                if !name.is_empty() {
                                    (picker.on_create)(name, window, cx);
                                }
                            }),
                        )
                        .child(tr!("branch_picker.create_branch")),
                );
        }

        let rows = self.filtered_branches(&self.last_filter);

        // The age is a column, not a per-row label: reserve the widest age in
        // the list so every timestamp ends on one edge and the name column has
        // one width. `truncate()` needs that width to be definite — gpui caches
        // a nowrap text layout by wrap width alone, so a flex-sized column
        // keeps the layout it measured at the row's width and paints past the
        // column edge instead of ending in an ellipsis.
        let age_size = picker::SECONDARY_TEXT.px(&theme);
        let age_w = rows
            .iter()
            .filter_map(|row| row.age.as_deref())
            .map(|age| text_width(window, age, age_size).ceil())
            .max()
            .unwrap_or(px(0.));
        let name_w = name_column_w(&theme, age_w);

        let mut list = div()
            .id("branch-picker-list")
            .w_full()
            .max_h(px(list_max_h(&theme)))
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(px(row_gap(&theme)));
        for (ix, branch) in rows.iter().enumerate() {
            let selected = branch.name == self.current;
            let highlighted = ix == self.highlighted;
            let name = branch.name.clone();
            let action = branch.name.clone();
            let age = branch.age.clone();
            // The current branch is the one row a click can't checkout, so
            // the highlight is the selection signal and the check is the
            // fact.
            let this = this.clone();
            list = list.child(
                picker_entry(
                    div()
                        .id(ElementId::NamedInteger("branch-row".into(), ix as u64))
                        .debug_selector(move || format!("branch-row-{ix}")),
                    &theme,
                )
                .h(px(row_h(&theme)))
                .flex_none()
                .cursor_pointer()
                .pressed_state(&theme, PressedRamp::Wash)
                .when(selected || highlighted, |row| {
                    row.bg(theme.surface_selected)
                })
                .when(!selected && !highlighted, |row| row.hover_state(&theme))
                .on_hover({
                    let this = this.clone();
                    move |hovering, _, cx| {
                        if *hovering {
                            this.update(cx, |picker, cx| {
                                if picker.highlighted != ix {
                                    picker.highlighted = ix;
                                    cx.notify();
                                }
                            });
                        }
                    }
                })
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(move |picker, _, window, cx| {
                        if picker.current == action {
                            (picker.on_dismiss)(false, window, cx);
                        } else {
                            (picker.on_checkout)(action.clone(), window, cx);
                        }
                    }),
                )
                .child(icon(
                    "icons/branch.svg",
                    context_menu::ICON.px(&theme),
                    if selected {
                        theme.accent_text
                    } else {
                        theme.text_muted
                    },
                ))
                // Name and age share one baseline row: the name truncates
                // inside its column, and every row reserves the same age
                // column so the dates read straight down the list.
                .child(
                    div()
                        .debug_selector(move || format!("branch-name-{ix}"))
                        .w(name_w)
                        .min_w_0()
                        .truncate()
                        .font_weight(if highlighted || selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if highlighted {
                            theme.text_primary
                        } else if selected {
                            theme.selection_fg
                        } else {
                            theme.text_secondary
                        })
                        .child(name),
                )
                // Every row reserves the age column, dated or not, so the
                // check sits at one x down the whole list.
                .child(
                    div()
                        .debug_selector(move || format!("branch-age-{ix}"))
                        .w(age_w)
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .text_size(age_size)
                        .text_color(if highlighted || selected {
                            theme.text_secondary
                        } else {
                            theme.text_muted
                        })
                        .children(age),
                )
                // The check keeps its slot on every row — empty rows reserve
                // it — so the ages above stay in one right-aligned column
                // instead of jumping left on the current branch.
                .child(
                    div()
                        .debug_selector(move || format!("branch-check-{ix}"))
                        .flex_none()
                        .flex()
                        .items_center()
                        .w(context_menu::ICON.px(&theme))
                        .when(selected, |slot| {
                            slot.child(icon(
                                "icons/check.svg",
                                context_menu::ICON.px(&theme),
                                theme.accent_text,
                            ))
                        }),
                ),
            );
        }

        picker_surface(div(), &theme)
            .debug_selector(|| "branch-picker".to_string())
            .w(px(POPOVER_W))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(Self::on_outside_down))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .child(
                picker_search_frame(div(), &theme)
                    .child(icon(
                        "icons/search.svg",
                        input::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(div().flex_1().min_w_0().child(self.filter.clone())),
            )
            .child(
                menu_header(tr!("branch_picker.branches"), &theme)
                    .pt(picker::list_padding_y(&theme))
                    // Branch count in the header's trailing slot, on the
                    // sub-header's own metrics — same as the workspace
                    // picker.
                    .when(!rows.is_empty(), |header| {
                        header.child(
                            div()
                                .flex_none()
                                .h(list::sub_header_height(&theme))
                                .px(list::sub_header_inset_x(&theme))
                                .flex()
                                .items_center()
                                .text_size(list::SUB_HEADER_TEXT.px(&theme))
                                .text_color(theme.text_muted)
                                .child(rows.len().to_string()),
                        )
                    }),
            )
            .child(if rows.is_empty() {
                // Zed's no-match state: one muted picker entry.
                div()
                    .py(picker::list_padding_y(&theme))
                    .child(
                        picker_entry(div(), &theme)
                            .text_color(theme.text_muted)
                            .child(tr!("branch_picker.no_branches_match")),
                    )
                    .into_any_element()
            } else {
                list.into_any_element()
            })
            // Footer action, below a full-width hairline like Zed's picker
            // footer.
            .child(
                div()
                    .py(picker::list_padding_y(&theme))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        picker_entry(div(), &theme)
                            .h(picker::entry_height(&theme))
                            .cursor_pointer()
                            .hover_state(&theme)
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::begin_create))
                            .child(icon(
                                "icons/plus.svg",
                                context_menu::ICON.px(&theme),
                                theme.text_secondary,
                            ))
                            .child(
                                div()
                                    .text_color(theme.text_secondary)
                                    .child(tr!("branch_picker.create_and_checkout_new_branch_2")),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    fn picker(
        cx: &mut Context<BranchPicker>,
        branches: Vec<(&str, Option<String>)>,
    ) -> BranchPicker {
        BranchPicker::new(
            "orbit".into(),
            branches
                .into_iter()
                .map(|(name, commit_date)| BranchEntry {
                    name: name.to_string(),
                    commit_date,
                })
                .collect(),
            "main".into(),
            Box::new(|_: String, _: &mut Window, _: &mut App| {}) as BranchAction,
            Box::new(|_: String, _: &mut Window, _: &mut App| {}) as BranchAction,
            Box::new(|_: bool, _: &mut Window, _: &mut App| {}) as BranchPickerDismiss,
            cx,
        )
    }

    /// Regression: a long branch name ran into its "last active" age and the
    /// list's columns moved row to row, because the age kept only its natural
    /// width and the name took whatever was left. The name now owns one column
    /// and every age ends on the same edge, whatever the age's own width
    /// ("just now" is wider than "2d ago").
    #[gpui::test]
    fn branch_names_and_ages_hold_one_column(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let now = chrono::Utc::now();
        let picker = cx.update(|_, cx| {
            cx.new(|cx| {
                picker(
                    cx,
                    vec![
                        ("refector/new-session-composer", Some(now.to_rfc3339())),
                        ("main", Some((now - chrono::Duration::days(2)).to_rfc3339())),
                        (
                            "feat/sidebar-awaiting-review",
                            Some((now - chrono::Duration::days(11)).to_rfc3339()),
                        ),
                    ],
                )
            })
        });
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(POPOVER_W + 16.), px(600.)),
            |_, _| picker.clone(),
        );

        let theme = cx.update(|_, cx| *theme::get(cx));
        let gap = list_item::content_gap(&theme);
        let padding = list_item::padding_x(&theme);
        let names = ["branch-name-0", "branch-name-1", "branch-name-2"];
        let ages = ["branch-age-0", "branch-age-1", "branch-age-2"];
        let rows = ["branch-row-0", "branch-row-1", "branch-row-2"];

        let first_name = cx.debug_bounds(names[0]).expect("first name column");
        let first_age = cx.debug_bounds(ages[0]).expect("first age column");
        let first_check = cx.debug_bounds("branch-check-0").expect("first check");
        let first_row = cx.debug_bounds(rows[0]).expect("first row");
        assert!(
            (first_name.right() + gap - first_age.left()).abs() <= px(1.),
            "the name stops one gap before the age: name {first_name:?}, age {first_age:?}"
        );
        assert!(
            (first_age.right() + gap - first_check.left()).abs() <= px(1.),
            "the age column ends one gap before the check slot"
        );
        assert!(
            (first_check.right() + padding - first_row.right()).abs() <= px(1.),
            "the check sits in the row's padding: check {first_check:?}, row {first_row:?}"
        );
        // The columns add up to the row's content width: the name column is
        // the exact remainder, so no space is left to push the row out.
        let icon = context_menu::ICON.px(&theme);
        let content_w = first_row.size.width - 2. * padding;
        assert!(
            (icon
                + first_name.size.width
                + first_age.size.width
                + first_check.size.width
                + 3. * gap
                - content_w)
                .abs()
                <= px(1.),
            "the row's columns fill its content width"
        );

        // One column across the list: every name ends on the same edge and
        // every age ends on the same edge, even though the ages differ in
        // width ("just now" against "2d ago").
        for ix in 1..3 {
            let name = cx.debug_bounds(names[ix]).expect("name column");
            let age = cx.debug_bounds(ages[ix]).expect("age column");
            let row = cx.debug_bounds(rows[ix]).expect("row");
            assert!(
                (name.right() - first_name.right()).abs() <= px(1.)
                    && (name.size.width - first_name.size.width).abs() <= px(1.),
                "row {ix} keeps the name column: {name:?} vs {first_name:?}"
            );
            assert!(
                (age.right() - first_age.right()).abs() <= px(1.),
                "row {ix} keeps the age column's right edge: {age:?} vs {first_age:?}"
            );
            assert!(
                (row.size.width - first_row.size.width).abs() <= px(1.),
                "rows share one width"
            );
        }
    }
}
