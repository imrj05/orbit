//! Rows: the settings row, and the picker row every menu-like list shares.
//!
//! A picker row is two parts that compose: [`picker_entry`], the inset,
//! rounded, focusable frame Zed's `ListItem` draws, and [`PickerRow`], its
//! text body — a title over an optional detail line on fixed line boxes, with
//! an optional leading glyph or logo on the title's line. Callers own the id,
//! the selection and hover treatment, the trailing slot and the click.

use std::sync::Arc;

use gpui::{
    div, img, AnyElement, FontWeight, Hsla, ImageSource, InteractiveElement, IntoElement,
    ObjectFit, ParentElement, Styled, StyledImage,
};

use crate::theme::{
    self,
    tokens::{
        context_menu, list_item, picker, BufferLineHeight, ButtonSize, DynamicSpacing, Radius,
        TextSize,
    },
    Theme,
};
use crate::ui::icon::icon;
use crate::ui::style::control::ControlExt;

/// A settings label and optional supporting text beside an intrinsic-size control.
pub(crate) fn setting_row(
    theme: Theme,
    title: &str,
    desc: Option<&str>,
    meta: Option<&str>,
    control: Option<AnyElement>,
) -> AnyElement {
    div()
        .w_full()
        .flex_none()
        .min_h(ButtonSize::Large.height(&theme) + DynamicSpacing::Base08.px(&theme))
        .px(DynamicSpacing::Base16.px(&theme))
        .py(DynamicSpacing::Base12.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base16.px(&theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base03.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(title.to_string()),
                )
                .children(desc.map(|d| {
                    div()
                        .min_w_0()
                        .text_size(TextSize::Default.px(&theme))
                        .text_color(theme.text_secondary)
                        // A long localized sentence wraps instead of running under the control.
                        .whitespace_normal()
                        .child(d.to_string())
                }))
                .children(meta.map(|m| {
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .truncate()
                        .child(m.to_string())
                })),
        )
        // Keep long controls from squeezing the title to one letter per line.
        .children(control.map(|c| {
            div()
                .flex_none()
                .min_w_0()
                .max_w(gpui::relative(0.6))
                .child(c)
        }))
        .into_any_element()
}

/// One picker row, Zed's inset `ListItem` at `Sparse` spacing: Base04
/// outside, Base06 inside, 4px above and below, 6px corners, and at least
/// one Comfortable line of Default text (31px). A row with a secondary line
/// grows to fit it (see `picker::two_line_entry_height`). Callers add the id,
/// colors, hover, selection, and click handling.
pub(crate) fn picker_entry<E: Styled + InteractiveElement>(el: E, theme: &Theme) -> E {
    el.min_h(picker::entry_height(theme))
        .mx(list_item::inset(theme))
        .px(list_item::padding_x(theme))
        .py(picker::ROW_SPACING.padding_y(theme))
        .rounded(list_item::RADIUS.px(theme))
        .flex()
        .items_center()
        .gap(list_item::content_gap(theme))
        .text_size(picker::TEXT.px(theme))
        .focus_state(theme)
}

/// One fixed text line box inside a picker row, at the Comfortable leading
/// [`picker::two_line_entry_height`] measures. Used by [`PickerRow`].
fn picker_line_height(size: TextSize, theme: &Theme) -> gpui::Pixels {
    BufferLineHeight::Comfortable.resolve(size.px(theme))
}

/// The text body of a two-line picker row: a title over a secondary detail
/// line, each on a fixed line box so the pair is exactly
/// [`picker::two_line_entry_height`] tall.
///
/// Every picker uses this one builder — the model selector's label column, the
/// "Work in" lists, the folder picker — so their rows share a rhythm and a row's
/// text can never spill into the row below it (which is what happens when a
/// two-line body is given a one-line height).
pub(crate) struct PickerRow {
    title: String,
    detail: Option<String>,
    detail_mono: bool,
    weight: gpui::FontWeight,
    title_color: Hsla,
    detail_color: Hsla,
    /// A leading glyph, drawn on the **title's** line.
    icon: Option<(&'static str, Hsla)>,
    /// A leading mark that is a logo image instead of a tinted icon.
    mark: Option<MarkElement>,
}

/// The leading mark of a picker row: either a tinted icon or a logo image.
pub(crate) enum MarkElement {
    Icon(&'static str),
    Image(Arc<gpui::Image>),
}

/// What a row's leading glyph says about the place, for rows that may carry a
/// logo image instead (`PickerRow::mark`).
#[derive(Debug, Clone, Copy)]
pub(crate) enum EntryMarkLike {
    Local,
    Folder,
}

impl EntryMarkLike {
    fn icon_path(self) -> &'static str {
        match self {
            Self::Local => "icons/monitor.svg",
            Self::Folder => "icons/folder.svg",
        }
    }
}

impl PickerRow {
    /// A row body with the picker's regular ink: `title_color` for the title,
    /// `detail_color` for the second line.
    pub(crate) fn new(title: impl Into<String>, title_color: Hsla, detail_color: Hsla) -> Self {
        Self {
            title: title.into(),
            detail: None,
            detail_mono: false,
            weight: gpui::FontWeight::NORMAL,
            title_color,
            detail_color,
            icon: None,
            mark: None,
        }
    }

    /// The leading glyph. It is drawn on the title's line and the detail line
    /// is indented to clear it, so one row reads as one block: glyph and title
    /// paired, description underneath. Centering the glyph in a two-line row
    /// instead leaves it floating between the lines, belonging to neither —
    /// which is what makes a scrolling list look like two things sliding past
    /// each other.
    pub(crate) fn icon(mut self, path: &'static str, color: Hsla) -> Self {
        self.icon = Some((path, color));
        self
    }

    /// The leading mark: a logo image when the workspace has one (the same
    /// mark the header's folder chip shows), else the [`EntryMark`] glyph.
    /// Drawn exactly like [`Self::icon`], on the title's line.
    pub(crate) fn mark(
        mut self,
        logo: Option<Arc<gpui::Image>>,
        entry_mark: EntryMarkLike,
        theme: &Theme,
    ) -> Self {
        self.mark = Some(match logo {
            Some(logo) => MarkElement::Image(logo),
            None => MarkElement::Icon(entry_mark.icon_path()),
        });
        let _ = theme;
        self
    }

    /// The second line: a path, a branch, a hint.
    pub(crate) fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Draw the second line in the code face — paths and branches are machine
    /// text, the same register the model picker uses for an id.
    pub(crate) fn mono(mut self) -> Self {
        self.detail_mono = true;
        self
    }

    /// The title's weight. Pickers mark the highlighted/selected row with
    /// Medium rather than a second color.
    pub(crate) fn weight(mut self, weight: gpui::FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// The row body. It is the flexible child (`flex_1` + `min_w_0`), so the
    /// row's trailing slot — a check, an age, a tag — lands at the right edge
    /// and long text ellipsizes inside the row instead of spilling past it.
    /// Wrap it in another div and that promise is gone.
    pub(crate) fn into_element(self, theme: &Theme) -> AnyElement {
        let gap = list_item::content_gap(theme);
        let icon_size = context_menu::ICON.px(theme);
        let leading = self.mark.map(|mark| match mark {
            MarkElement::Image(logo) => div()
                .debug_selector(|| "picker-row-leading".to_string())
                .flex_none()
                // Exactly the title's line box, so the mark and the title
                // share one optical line whatever the row's total height.
                .h(picker_line_height(picker::TEXT, theme))
                .flex()
                .items_center()
                .child(
                    img(ImageSource::Image(logo))
                        .size(icon_size)
                        .flex_none()
                        .rounded(Radius::Small.px(theme))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            MarkElement::Icon(path) => div()
                .debug_selector(|| "picker-row-leading".to_string())
                .flex_none()
                .h(picker_line_height(picker::TEXT, theme))
                .flex()
                .items_center()
                .child(icon(path, icon_size, theme.text_muted))
                .into_any_element(),
        });
        let leading = leading.or_else(|| {
            self.icon.map(|(path, color)| {
                div()
                    .debug_selector(|| "picker-row-leading".to_string())
                    .flex_none()
                    // Exactly the title's line box, so the glyph and the title
                    // share one optical line whatever the row's total height.
                    .h(picker_line_height(picker::TEXT, theme))
                    .flex()
                    .items_center()
                    .child(icon(path, icon_size, color))
                    .into_any_element()
            })
        });
        let indent = if leading.is_some() {
            icon_size + gap
        } else {
            gpui::px(0.)
        };
        let detail = self.detail.map(|detail| {
            let line = div()
                .w_full()
                .pl(indent)
                .truncate()
                .text_size(picker::SECONDARY_TEXT.px(theme))
                .line_height(picker_line_height(picker::SECONDARY_TEXT, theme))
                .text_color(self.detail_color)
                .child(detail);
            if self.detail_mono {
                line.font_family(theme::code_font_family())
                    .into_any_element()
            } else {
                line.into_any_element()
            }
        });
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(gap)
                    .children(leading)
                    .child(
                        div()
                            .debug_selector(|| "picker-row-title".to_string())
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(self.weight)
                            .line_height(picker_line_height(picker::TEXT, theme))
                            .text_color(self.title_color)
                            .child(self.title),
                    ),
            )
            .children(detail)
            .into_any_element()
    }
}

/// The body of one choice in a mode picker (access mode, workflow mode): a
/// glyph on a 26px tile, the choice's name over a wrapping one-line
/// description, and a check on the choice in force. Goes inside a
/// [`picker_entry`]; the caller owns the id, the row fill, hover and click.
///
/// `selected` is the choice in force (accent glyph, check); `highlighted` is
/// the keyboard or pointer cursor. Either lifts the tile and the ink.
pub(crate) fn option_row(
    theme: &Theme,
    glyph: &'static str,
    title: impl Into<gpui::SharedString>,
    description: impl Into<gpui::SharedString>,
    selected: bool,
    highlighted: bool,
) -> Vec<AnyElement> {
    let lit = selected || highlighted;
    let mut children = vec![
        div()
            .flex_none()
            .size(gpui::px(26.))
            .rounded(Radius::Medium.px(theme))
            .flex()
            .items_center()
            .justify_center()
            .bg(if lit {
                theme.text_primary.opacity(0.14)
            } else {
                theme.overlay
            })
            .child(icon(
                glyph,
                context_menu::ICON.px(theme),
                if selected {
                    theme.accent_text
                } else {
                    theme.text_muted
                },
            ))
            .into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if lit {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(title.into()),
            )
            .child(
                div()
                    .mt(DynamicSpacing::Base02.px(theme))
                    .whitespace_normal()
                    .text_size(picker::SECONDARY_TEXT.px(theme))
                    .text_color(if lit {
                        theme.text_primary.opacity(0.75)
                    } else {
                        theme.text_muted
                    })
                    .child(description.into()),
            )
            .into_any_element(),
    ];
    if selected {
        children.push(
            icon(
                "icons/check.svg",
                context_menu::ICON.px(theme),
                theme.accent_text,
            )
            .into_any_element(),
        );
    }
    children
}

/// Where a list row stands: what the pane beside the list shows, the
/// keyboard cursor, or neither. One mapping to fills for every list (Git's
/// History / Graph / GitHub lists, the trees), so "selected" and "cursor"
/// read the same everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowState {
    Selected,
    Cursor,
    Rest,
}

impl RowState {
    /// Selection wins over the cursor.
    pub(crate) fn of(selected: bool, cursor: bool) -> Self {
        if selected {
            Self::Selected
        } else if cursor {
            Self::Cursor
        } else {
            Self::Rest
        }
    }

    /// The fill the row holds at rest: `surface_selected` for the open row,
    /// the quieter `overlay` for the keyboard cursor, nothing otherwise.
    pub(crate) fn rest_fill(self, theme: &Theme) -> Option<gpui::Hsla> {
        match self {
            Self::Selected => Some(theme.selected_wash()),
            Self::Cursor => Some(theme.overlay),
            Self::Rest => None,
        }
    }
}

/// A list row's interaction: the pointer cursor, `state`'s rest fill, and the
/// hover wash fading in over it on `Motion::Interaction`. The caller owns the
/// row's frame (padding, radius, columns), its id and its click; `name` keys
/// the fade (use the row's id).
pub(crate) fn list_row(
    row: gpui::Stateful<gpui::Div>,
    name: impl Into<gpui::SharedString>,
    state: RowState,
    theme: &Theme,
) -> AnyElement {
    crate::motion::hover(
        row.cursor_pointer(),
        name.into(),
        theme,
        crate::motion::Hover::over(state.rest_fill(theme), theme.hover_wash()),
    )
}

/// The row height a two-line [`PickerRow`] fills exactly.
pub(crate) fn picker_row_height(theme: &Theme) -> gpui::Pixels {
    picker::two_line_entry_height(theme)
}

#[cfg(test)]
mod row_state_tests {
    use super::RowState;
    use crate::theme::{Theme, ThemeId};

    #[test]
    fn selection_wins_over_the_cursor_and_rest_paints_nothing() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        assert_eq!(RowState::of(true, true), RowState::Selected);
        assert_eq!(RowState::of(false, true), RowState::Cursor);
        assert_eq!(
            RowState::Selected.rest_fill(&theme),
            Some(theme.surface_selected)
        );
        assert_eq!(RowState::Cursor.rest_fill(&theme), Some(theme.overlay));
        assert_eq!(RowState::Rest.rest_fill(&theme), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_changes_the_hairline_never_the_geometry() {
        use gpui::{div, Styled};
        for id in crate::theme::ThemeId::ALL {
            let theme = Theme::for_id(id);
            for height in [NavRowHeight::Header, NavRowHeight::Item] {
                let mut rest = nav_row(div(), &theme, height, false);
                let mut cursor = nav_row(div(), &theme, height, true);
                let (rest, cursor) = (rest.style(), cursor.style());
                assert_eq!(rest.min_size, cursor.min_size, "{id:?} {height:?}");
                assert_eq!(
                    rest.border_widths, cursor.border_widths,
                    "{id:?} {height:?}"
                );
                assert_eq!(rest.corner_radii, cursor.corner_radii, "{id:?} {height:?}");
                assert_eq!(cursor.border_color, Some(theme.focus_ring));
                assert_eq!(rest.border_color.map(|c| c.a), Some(0.));
            }
        }
    }

    #[test]
    fn items_share_one_height_and_headers_are_shorter() {
        use gpui::{div, Styled};
        let theme = Theme::dark();
        let mut item = nav_row(div(), &theme, NavRowHeight::Item, false);
        let mut header = nav_row(div(), &theme, NavRowHeight::Header, false);
        assert_eq!(
            item.style().min_size.height,
            Some(ButtonSize::Medium.height(&theme).into())
        );
        assert_eq!(
            header.style().min_size.height,
            Some(ButtonSize::Default.height(&theme).into())
        );
    }

    #[test]
    fn the_open_row_keeps_its_selection_under_the_pointer() {
        let theme = Theme::dark();
        let open = nav_row_fill(&theme, true, false);
        assert_eq!(open.at(0.).0, Some(theme.surface_selected), "at rest");
        assert_eq!(
            open.at(1.).0,
            Some(theme.surface_selected),
            "under the pointer"
        );
        // The keyboard cursor rests on the wash; an idle row has none.
        assert_eq!(
            nav_row_fill(&theme, false, true).at(0.).0,
            Some(theme.surface_hover)
        );
        let idle = nav_row_fill(&theme, false, false).at(0.).0;
        assert!(
            idle.is_none_or(|fill| fill.a == 0.),
            "idle rows rest unfilled"
        );
    }

    #[test]
    fn only_the_choice_in_force_carries_a_check() {
        let theme = Theme::dark();
        let len = |selected, highlighted| {
            option_row(&theme, "icons/check.svg", "A", "B", selected, highlighted).len()
        };
        assert_eq!(len(true, false), 3, "tile, text, check");
        assert_eq!(len(true, true), 3);
        assert_eq!(len(false, true), 2, "a highlighted choice is not in force");
        assert_eq!(len(false, false), 2);
    }
}

/// How tall a navigation row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavRowHeight {
    /// A group header (a project in the sidebar): the Default control height.
    Header,
    /// An item (a session, "Show more"): the Medium control height, the one
    /// height every navigation row shares.
    Item,
}

impl NavRowHeight {
    fn size(self) -> ButtonSize {
        match self {
            Self::Header => ButtonSize::Default,
            Self::Item => ButtonSize::Medium,
        }
    }
}

/// The frame of a navigation row (sidebar sessions, project headers, the
/// "Show more" / "Show less" rows): full width, at least its height, the list
/// item's corners, a centered row, and a 1px hairline reserved in every state
/// so the keyboard cursor's focus ring never shifts the text.
///
/// The caller keeps the padding and gap (a header's glyph sits further out
/// than a session's), the id, and every handler — rows here are drag handles,
/// drop targets and context-menu anchors, so the frame is a style seam rather
/// than a wrapper. Fill is [`nav_row_fill`], faded in with `motion::hover`.
pub(crate) fn nav_row<E: Styled>(el: E, theme: &Theme, height: NavRowHeight, cursor: bool) -> E {
    el.w_full()
        .min_h(height.size().height(theme))
        .rounded(list_item::RADIUS.px(theme))
        .border_1()
        .border_color(if cursor {
            theme.focus_ring
        } else {
            gpui::transparent_black()
        })
        .flex()
        .items_center()
}

/// A navigation row's fill, for `motion::hover`. The open row (`selected`)
/// holds `surface_selected` at rest and under the pointer, so hovering it never
/// reads as leaving the selection; the keyboard cursor rests on the hover wash;
/// every other row fades the wash in.
pub(crate) fn nav_row_fill(theme: &Theme, selected: bool, cursor: bool) -> crate::motion::Hover {
    if selected {
        crate::motion::Hover::over(Some(theme.selected_wash()), theme.selected_wash())
    } else {
        crate::motion::Hover::over(cursor.then_some(theme.hover_wash()), theme.hover_wash())
    }
}
