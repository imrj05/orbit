//! The UI layer's everyday imports, like `gpui::prelude`: the frames, style
//! traits, icons, feedback and menu pieces nearly every surface uses.
//! `use crate::ui::prelude::*;` replaces the old `app::helpers` re-exports.
//! Named components (`Button`, `TreeRow`, `ButtonGroup`, …) are imported
//! from their own module, so a reader can see where each one comes from.

pub(crate) use crate::ui::button::{button_frame, icon_button_frame, Button, ButtonVariant};
pub(crate) use crate::ui::chrome::{
    header_chip, header_ctrl_radius, header_ghost_button, header_icon_button, header_lift,
    window_controls, window_drag_region, HEADER_CTRL_H,
};
pub(crate) use crate::ui::dialog::{
    modal_card, modal_footer, modal_header, modal_scrim, ModalSubtitle, ModalWidthExt,
};
pub(crate) use crate::ui::feedback::{
    empty_state, empty_state_glyph, error_state, refresh_glyph, running_spinner, spinner, EmptyFill,
};
pub(crate) use crate::ui::field::{input_field_frame, input_label, picker_search_frame};
pub(crate) use crate::ui::glyph::{
    embedded_image, embedded_image_w, file_badge, file_glyph, file_icon_tile, nerd_font_family,
    provider_mark_color,
};
pub(crate) use crate::ui::icon::{
    disclosure_chevron, dropdown_chevron, icon, icon_dyn, BUTTON_GROUP,
};
pub(crate) use crate::ui::kbd::{kbd_shortcut, working_folder_keyboard_hints};
pub(crate) use crate::ui::menu::{
    context_menu_entry, context_menu_separator, context_menu_surface, menu_header,
};
pub(crate) use crate::ui::popover::{
    anchored_popover_at, anchored_popover_in_window, picker_surface, popover_enter,
};
pub(crate) use crate::ui::row::{picker_entry, picker_row_height, EntryMarkLike, PickerRow};
pub(crate) use crate::ui::style::control::{
    press, AccentWash, ControlExt, PressedRamp, StatefulControlExt, TipExt,
};
pub(crate) use crate::ui::style::text::text_width;
pub(crate) use crate::ui::switch::switch;
