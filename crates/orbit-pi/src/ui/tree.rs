//! Tree rows: one line of a file tree — indent, disclosure, mark, name and a
//! trailing slot.
//!
//! The row owns everything that makes two trees read as the same control:
//! the indent step and the column the names align in, the turning disclosure
//! chevron, the folder mark, the name's type, the optional indent guides and
//! focus ring, and the fill for each [`TreeRowState`]. The caller owns the
//! data (depth, expanded, the file's own glyph), the trailing marks (a git
//! status letter, an unsaved dot), and every handler, attached to the returned
//! element only when the row is not being edited.

use gpui::{
    div, px, AnyElement, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, SharedString, Stateful, Styled,
};

use crate::theme::{
    tokens::{DynamicSpacing, IconSize, Radius, TextSize},
    Theme,
};
use crate::ui::icon::{disclosure_chevron, icon};
use crate::ui::row::RowState;
use crate::ui::style::control::ControlExt;

/// The left inset of a depth-0 directory row (its chevron's leading edge).
const INDENT_BASE: f32 = 7.;
/// One level of nesting.
const INDENT_STEP: f32 = 14.;
/// A file has no chevron; this extra inset lines its name up with its
/// sibling directories' names (chevron + gap + folder − file glyph).
const FILE_OFFSET: f32 = 16.;

/// The left inset of a row at `depth`.
pub(crate) fn indent(depth: usize, directory: bool) -> f32 {
    INDENT_BASE + if directory { 0. } else { FILE_OFFSET } + depth as f32 * INDENT_STEP
}

/// What the row stands for.
pub(crate) enum TreeNode {
    /// A folder: a chevron turned by `expanded`, then the folder mark.
    Directory { expanded: bool },
    /// A file: its own glyph (a devicon, a badge fallback), `IconSize::Small`.
    File { glyph: AnyElement },
}

/// How the row is lit, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeRowState {
    /// An inline editor (rename) owns the row: the active fill, no pointer
    /// treatment. The caller attaches no handlers.
    Editing,
    /// The row is what the surface beside the tree shows (the open file, the
    /// reviewed diff): the selected fill and primary ink.
    Selected,
    /// The keyboard cursor: a quieter wash than selection.
    Cursor,
    /// At rest: the pointer hover is the only treatment.
    Rest,
}

/// One tree row.
///
/// ```ignore
/// TreeRow::new(id, depth, TreeNode::Directory { expanded }, name)
///     .height(px(28.))
///     .state(TreeRowState::Cursor)
///     .guides(true)
///     .trailing(status_letter)
///     .render(&theme)
///     .on_click(toggle)
/// ```
pub(crate) struct TreeRow {
    id: SharedString,
    depth: usize,
    node: TreeNode,
    label: SharedString,
    editor: Option<AnyElement>,
    height: Pixels,
    state: TreeRowState,
    guides: bool,
    focus_ring: bool,
    trailing: Vec<AnyElement>,
}

impl TreeRow {
    pub(crate) fn new(
        id: impl Into<SharedString>,
        depth: usize,
        node: TreeNode,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            depth,
            node,
            label: label.into(),
            editor: None,
            height: px(28.),
            state: TreeRowState::Rest,
            guides: false,
            focus_ring: false,
            trailing: Vec::new(),
        }
    }

    pub(crate) fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub(crate) fn state(mut self, state: TreeRowState) -> Self {
        self.state = state;
        self
    }

    /// One hairline per ancestor through its chevron's centre, so nesting
    /// reads at a glance in a deep tree. Painted under the content.
    pub(crate) fn guides(mut self, guides: bool) -> Self {
        self.guides = guides;
        self
    }

    /// Ring the row (the selected row while the tree has keyboard focus),
    /// without changing its layout or its fill.
    pub(crate) fn focus_ring(mut self, focus_ring: bool) -> Self {
        self.focus_ring = focus_ring;
        self
    }

    /// An inline editor in place of the name (a rename `ComposerInput`).
    /// Trailing marks are dropped while it shows.
    pub(crate) fn editor(mut self, editor: Option<impl IntoElement>) -> Self {
        self.editor = editor.map(IntoElement::into_any_element);
        self
    }

    /// A mark after the name (status letter, unsaved dot).
    pub(crate) fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing.push(trailing.into_any_element());
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> Stateful<gpui::Div> {
        let directory = matches!(self.node, TreeNode::Directory { .. });
        let selected = self.state == TreeRowState::Selected;
        let mark_ink = if selected {
            theme.text_primary
        } else {
            theme.text_muted
        };
        let name_ink = if selected {
            theme.text_primary
        } else {
            theme.text_secondary
        };

        let mut row = div()
            .id(ElementId::Name(self.id.clone()))
            .relative()
            .h(self.height)
            .pl(px(indent(self.depth, directory)))
            .pr(DynamicSpacing::Base08.px(theme))
            .rounded(Radius::Medium.px(theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(theme));
        row = match self.state {
            TreeRowState::Editing => row.bg(theme.pressed_wash()),
            // The list rows' state fills, so a tree and a list agree.
            TreeRowState::Selected => row
                .cursor_pointer()
                .bg(RowState::Selected.rest_fill(theme).unwrap_or_default()),
            TreeRowState::Cursor => row
                .cursor_pointer()
                .bg(RowState::Cursor.rest_fill(theme).unwrap_or_default()),
            TreeRowState::Rest => row.cursor_pointer().hover_state(theme),
        };

        if self.guides {
            let chevron_mid = f32::from(IconSize::Indicator.px(theme)) / 2.;
            for level in 0..self.depth {
                row = row.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(INDENT_BASE + level as f32 * INDENT_STEP + chevron_mid))
                        .w(px(1.))
                        .bg(theme.hairline_subtle()),
                );
            }
        }

        row = match self.node {
            // One glyph turning a quarter turn, not two glyphs swapping — the
            // row is the same object between the two states.
            TreeNode::Directory { expanded } => row
                .child(disclosure_chevron(
                    format!("{}-caret", self.id),
                    expanded,
                    IconSize::Indicator.px(theme),
                    mark_ink,
                    theme,
                ))
                .child(icon(
                    "icons/folder.svg",
                    IconSize::Small.px(theme),
                    mark_ink,
                )),
            TreeNode::File { glyph } => row.child(glyph),
        };

        row = match self.editor {
            Some(editor) => row.child(div().flex().flex_1().min_w_0().child(editor)),
            None => row
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(TextSize::Small.px(theme))
                        .when_dir(directory, |el| el.font_weight(FontWeight::MEDIUM))
                        .text_color(name_ink)
                        .child(self.label),
                )
                .children(self.trailing),
        };

        if self.focus_ring {
            row = row.child(
                div()
                    .debug_selector(|| "tree-row-focus-ring".to_string())
                    .absolute()
                    .inset_0()
                    .rounded(Radius::Medium.px(theme))
                    .border_1()
                    .border_color(theme.focus_ring),
            );
        }
        row
    }
}

/// `when` for the directory flag without pulling `FluentBuilder` in.
trait WhenDir: Sized {
    fn when_dir(self, directory: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if directory {
            f(self)
        } else {
            self
        }
    }
}

impl<T> WhenDir for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn file_names_align_with_sibling_directory_names() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        // chevron + gap + folder == file offset + file glyph.
        let dir_marks = f32::from(IconSize::Indicator.px(&theme))
            + f32::from(DynamicSpacing::Base06.px(&theme))
            + f32::from(IconSize::Small.px(&theme));
        let file_marks = FILE_OFFSET + f32::from(IconSize::Small.px(&theme));
        assert!(
            (dir_marks - file_marks).abs() < 0.5,
            "{dir_marks} vs {file_marks}"
        );
        assert_eq!(indent(2, true), INDENT_BASE + 2. * INDENT_STEP);
        assert_eq!(indent(0, false) - indent(0, true), FILE_OFFSET);
    }

    #[test]
    fn each_state_lights_the_row_once() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let fill = |state| {
                let mut row = TreeRow::new("r", 1, TreeNode::Directory { expanded: false }, "src")
                    .state(state)
                    .render(&theme);
                row.style().background.clone()
            };
            assert_eq!(
                fill(TreeRowState::Editing),
                Some(theme.surface_active.into())
            );
            assert_eq!(
                fill(TreeRowState::Selected),
                Some(theme.surface_selected.into())
            );
            assert_eq!(fill(TreeRowState::Cursor), Some(theme.overlay.into()));
            assert_eq!(fill(TreeRowState::Rest), None, "{id:?}");
        }
    }
}
