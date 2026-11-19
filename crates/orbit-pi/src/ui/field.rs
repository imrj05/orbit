//! Shared input fields: the frame, a [`TextField`] around a `ComposerInput`,
//! the picker search head, and a [`FormRow`] that labels a control.
//!
//! The input itself stays the `ComposerInput` entity (IME, selection,
//! placeholders resolved at paint through `with_placeholder_key`); these own
//! only what surrounds it.

use crate::theme::{
    tokens::{input, picker, DynamicSpacing, TextSize},
    Theme,
};
use crate::ui::icon::icon;
use gpui::{
    div, AnyElement, FontWeight, Hsla, IntoElement, ParentElement, Pixels, SharedString, Styled,
};

/// A text field's box, Zed's `InputField`: at least 32px tall, `px_2`
/// `py_1p5`, a 1px hairline, [`input::RADIUS`], and Default-size text, which the
/// `ComposerInput` inside inherits. Callers set the fill (and a focus border)
/// and give the input a `flex_1` wrapper when an icon shares the row.
pub(crate) fn input_field_frame<E: Styled>(el: E, theme: &Theme) -> E {
    el.min_h(input::min_height(theme))
        .px(input::padding_x(theme))
        .py(input::padding_y(theme))
        .flex()
        .items_center()
        .gap(input::gap(theme))
        .rounded(input::RADIUS.px(theme))
        .border_1()
        .border_color(theme.hairline())
        .text_size(TextSize::Default.px(theme))
}

/// A field's label, Zed's `InputField` label: Small text.
pub(crate) fn input_label(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div().text_size(input::LABEL.px(theme)).child(label.into())
}

/// A picker's search row, Zed's picker head: 36px tall, `px_2p5`, a hairline
/// below, and Default-size input text. Put the `ComposerInput` in a
/// `flex_1` child; a leading search icon takes `IconSize::Small`.
pub(crate) fn picker_search_frame<E: Styled>(el: E, theme: &Theme) -> E {
    el.flex_none()
        .h(picker::search_height(theme))
        .px(picker::search_padding_x(theme))
        .flex()
        .items_center()
        .gap(input::gap(theme))
        .overflow_hidden()
        .border_b_1()
        .border_color(theme.hairline())
        .text_size(TextSize::Default.px(theme))
}

/// The search field every picker, menu and filter shares: the picker head
/// ([`picker_search_frame`]), a muted search glass, and `input` (a
/// `ComposerInput` entity, whose placeholder resolves at paint through
/// `with_placeholder_key`) filling the rest of the row. Callers add a test
/// selector, a width or an ink.
pub(crate) fn search_field(input: impl IntoElement, theme: &Theme) -> gpui::Div {
    picker_search_frame(div(), theme)
        .child(icon(
            "icons/search.svg",
            input::ICON.px(theme),
            theme.text_muted,
        ))
        .child(div().flex_1().min_w_0().child(input))
}

/// A text field: [`input_field_frame`] at full width on `fill`, with the
/// input filling the row. The fill is the caller's because it depends on what
/// the field sits on (`background` inside a card, `surface_secondary` on a
/// page).
///
/// ```ignore
/// TextField::new(self.name_input.clone(), theme.background).render(&theme)
/// TextField::new(body_input, theme.surface_secondary).multiline(px(96.)).render(&theme)
/// ```
pub(crate) struct TextField {
    input: AnyElement,
    fill: Hsla,
    min_h: Option<Pixels>,
}

impl TextField {
    pub(crate) fn new(input: impl IntoElement, fill: Hsla) -> Self {
        Self {
            input: input.into_any_element(),
            fill,
            min_h: None,
        }
    }

    /// A multi-line field at least `min_h` tall (never below the one-line
    /// height). Its input column stretches to the whole box, so a click
    /// anywhere in the field lands in the text, not only on its first line.
    pub(crate) fn multiline(mut self, min_h: Pixels) -> Self {
        self.min_h = Some(min_h);
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> gpui::Div {
        let frame = input_field_frame(div(), theme)
            .w_full()
            .min_w_0()
            .bg(self.fill);
        match self.min_h {
            None => frame.child(div().flex_1().min_w_0().child(self.input)),
            Some(min_h) => {
                // `input_field_frame` centres a one-line input; GPUI 0.2.2
                // has no `items_stretch`, so set the alignment directly.
                let mut frame = frame.min_h(min_h.max(input::min_height(theme)));
                frame.style().align_items = Some(gpui::AlignItems::Stretch);
                frame.child(div().flex_1().min_w_0().flex().flex_col().child(self.input))
            }
        }
    }
}

/// A labelled form control: the label (Small, medium, secondary ink) with an
/// optional action at its right edge and an optional hint under it, then the
/// control, then an optional help line. Padding is the caller's: a form row
/// sits in a dialog body, a settings card or a page.
///
/// ```ignore
/// FormRow::new(tr!("worktree.field.name"), TextField::new(input, theme.background).render(&theme))
///     .hint(tr!("worktree.field.name_hint"))
///     .render(&theme)
/// ```
pub(crate) struct FormRow {
    label: SharedString,
    hint: Option<SharedString>,
    action: Option<AnyElement>,
    control: AnyElement,
    help: Option<AnyElement>,
}

impl FormRow {
    pub(crate) fn new(label: impl Into<SharedString>, control: impl IntoElement) -> Self {
        Self {
            label: label.into(),
            hint: None,
            action: None,
            control: control.into_any_element(),
            help: None,
        }
    }

    /// A quiet line under the label: what the field is for.
    pub(crate) fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// A control at the label row's right edge (a "Generate" button).
    pub(crate) fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// A line under the control — validation, a count, a resolved value. The
    /// caller builds it, because its ink can carry state (a warning).
    pub(crate) fn help(mut self, help: impl IntoElement) -> Self {
        self.help = Some(help.into_any_element());
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> gpui::Div {
        let head = form_label_with(self.label, self.hint, self.action, theme);
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(input::gap(theme))
            .child(head)
            .child(self.control)
            .children(self.help)
    }
}

/// A form row's head on its own — the label and an optional hint — for a
/// control group that is not a single field (a row of choice chips).
pub(crate) fn form_label(
    label: impl Into<SharedString>,
    hint: Option<SharedString>,
    theme: &Theme,
) -> gpui::Div {
    form_label_with(label.into(), hint, None, theme)
}

fn form_label_with(
    label: SharedString,
    hint: Option<SharedString>,
    action: Option<AnyElement>,
    theme: &Theme,
) -> gpui::Div {
    let label = input_label(label, theme)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_secondary);
    let label = match action {
        Some(action) => div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(DynamicSpacing::Base08.px(theme))
            .child(label)
            .child(action),
        None => label,
    };
    div()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base02.px(theme))
        .child(label)
        .children(hint.map(|hint| {
            div()
                .text_size(TextSize::XSmall.px(theme))
                .text_color(theme.text_muted)
                .child(hint)
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn a_multiline_field_stretches_its_input_and_never_shrinks_below_one_line() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let mut one_line = TextField::new(div(), theme.background).render(&theme);
        let mut tall = TextField::new(div(), theme.background)
            .multiline(gpui::px(1.))
            .render(&theme);
        let (one_line, tall) = (one_line.style(), tall.style());
        assert_eq!(tall.align_items, Some(gpui::AlignItems::Stretch));
        assert_ne!(one_line.align_items, Some(gpui::AlignItems::Stretch));
        assert_eq!(
            tall.min_size.height, one_line.min_size.height,
            "floor is one line"
        );
        assert_eq!(one_line.background, Some(theme.background.into()));
    }
}
