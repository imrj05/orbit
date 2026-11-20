use super::*;
use crate::theme::tokens::{context_menu, picker, popover, ButtonSize, IconSize, BASE_REM_PX};
use crate::ui::field::search_field;
use crate::ui::popover::{dropdown_below, DropdownAlign};
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::prelude::*;

/// The slot an "open in" app icon draws in: 18px at the default UI scale.
///
/// Larger than the 14px [`context_menu::ICON`] a glyph entry uses. App artwork
/// keeps a ~12% transparent margin and sits in a 31px picker row, so a
/// glyph-sized slot read as "tiny" next to the 14px label; 18px leaves ~16px of
/// visible mark. `platform` rasterizes each icon at 2× this slot so a Retina
/// window blits it 1:1.
const APP_ICON: IconSize = IconSize::Custom(18. / BASE_REM_PX);

/// Hover group of the header's open-in split, so its seam can follow the lift.
const OPEN_IN_SPLIT: &str = "open-in-split";

impl OrbitApp {
    /// Resolve installed folder-capable apps once, off-thread.
    pub(crate) fn detect_open_in_apps(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let apps = cx
                .background_executor()
                .spawn(async move { platform::detect_open_in_apps() })
                .await;
            if apps.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.open_in_apps = Rc::new(apps);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn preferred_open_in_app<'a>(&'a self, _: &App) -> Option<&'a ExternalApp> {
        self.open_in_prefs
            .preferred_app(self.current_workspace.as_deref(), &self.open_in_apps)
    }

    pub(super) fn open_workspace_in_app(
        &mut self,
        path: &Path,
        app_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(app) = self
            .open_in_apps
            .iter()
            .find(|app| app.id == app_id)
            .cloned()
        else {
            return;
        };
        platform::open_path_in_app(path, &app);
        self.open_in_prefs.remember(path, app_id);
        let prefs = self.open_in_prefs.clone();
        let previous_save = self.open_in_save_task.take();
        self.open_in_save_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous_save) = previous_save {
                previous_save.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move { prefs.persist() })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.toast_error(tr!("open_in.save_failed", error = error));
                    cx.notify();
                });
            }
        }));
        self.open_in_menu_open = false;
        cx.notify();
    }

    pub(super) fn on_open_in_primary(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.current_workspace.clone() else {
            return;
        };
        let Some(app) = self.preferred_open_in_app(cx) else {
            return;
        };
        // Opening a fallback must not replace a saved app that is temporarily
        // unavailable. Only an explicit menu selection changes the preference.
        platform::open_path_in_app(&path, app);
        self.open_in_menu_open = false;
        cx.notify();
    }

    pub(super) fn on_open_in_caret(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The open menu dismisses on this same click's mouse-down; without
        // this guard the mouse-up would toggle it straight back open.
        if self.menu_dismissed_at.swallows() {
            return;
        }
        if self.open_in_apps.is_empty() || self.current_workspace.is_none() {
            return;
        }
        self.open_in_menu_open = !self.open_in_menu_open;
        if self.open_in_menu_open {
            self.dismiss_work_in_menu();
            self.open_in_filter
                .update(cx, |filter, cx| filter.clear(cx));
            let handle = self.open_in_filter.read(cx).focus_handle(cx);
            window.focus(&handle);
        }
        cx.notify();
    }

    /// Split "open in" control: preferred app icon on the left, chevron menu
    /// on the right listing every installed editor/terminal.
    pub(super) fn render_open_in_control(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let path = self.current_workspace.as_ref()?;
        let preferred = self.preferred_open_in_app(cx)?;
        if self.open_in_apps.is_empty() {
            return None;
        }

        let theme = *theme::get(cx);
        let preferred_id = preferred.id;
        let preferred_label = preferred.label;
        let preferred_icon = preferred.icon.clone();
        let this = cx.entity().clone();
        let path = Rc::from(path.as_path());

        // Bespoke: the two halves of the header's open-in split button, which
        // share one chip's frame and round only its outer corners.
        let primary = icon_button_frame(div().id("header-open-in"), &theme, ButtonSize::Medium)
            .group(BUTTON_GROUP)
            .tip(tr!("open_in.open_in_app", app = preferred_label))
            .h_full()
            .rounded(px(0.))
            .rounded_tl(header_ctrl_radius(&theme))
            .rounded_bl(header_ctrl_radius(&theme))
            .cursor_pointer()
            .pressed_state(&theme, PressedRamp::Wash)
            .child(
                img(ImageSource::Image(preferred_icon))
                    .size(APP_ICON.px(&theme))
                    .flex_none(),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_open_in_primary));

        // Bespoke: the split button's caret half (see the primary above).
        let caret = icon_button_frame(
            div().id("header-open-in-caret"),
            &theme,
            ButtonSize::Compact,
        )
        .group(BUTTON_GROUP)
        .relative()
        .tip(tr!("open_in.choose_app"))
        .h_full()
        .rounded(px(0.))
        .rounded_tr(header_ctrl_radius(&theme))
        .rounded_br(header_ctrl_radius(&theme))
        .cursor_pointer()
        .when(self.open_in_menu_open, |s| {
            s.bg(theme.pressed_wash()).text_color(theme.text_primary)
        })
        .child(icon(
            "icons/chevron-down.svg",
            IconSize::XSmall.px(&theme),
            theme.text_muted,
        ))
        // Pin the dropdown to the caret's bottom-right — same zero-size
        // anchor trick as the session row menu, so flex centering doesn't
        // pull the popup toward the button's middle.
        .children(self.open_in_menu_popup(&this, path, preferred_id, theme, cx))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_open_in_caret));

        // The seam between the halves belongs to the chip's frame: header chips
        // rest ghost, so it shows only while the split is lifted (hovered or
        // its menu open) and never floats as a stray rule in the bar.
        let menu_open = self.open_in_menu_open;
        let seam = div()
            .w(px(1.))
            .h_full()
            .flex_none()
            .when(menu_open, |seam| seam.bg(theme.hairline()))
            .group_hover(OPEN_IN_SPLIT, |seam| seam.bg(theme.hairline()));
        let split = header_chip(
            div()
                .group(OPEN_IN_SPLIT)
                .h(px(HEADER_CTRL_H))
                .rounded(header_ctrl_radius(&theme))
                .flex_none()
                .flex()
                .items_center(),
            &theme,
        );
        Some(
            split
                .when(menu_open, |split| header_lift(split, &theme))
                .child(primary)
                .child(seam)
                .child(caret)
                .into_any_element(),
        )
    }

    pub(super) fn open_in_menu_popup(
        &self,
        this: &Entity<OrbitApp>,
        path: Rc<Path>,
        preferred_id: &str,
        theme: Theme,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        if !self.open_in_menu_open {
            return None;
        }

        let preferred_id = preferred_id.to_string();
        let needle = self.open_in_filter.read(cx).text().to_lowercase();
        let apps: Vec<ExternalApp> = self
            .open_in_apps
            .iter()
            .filter(|app| needle.is_empty() || app.label.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        let mut list = div()
            .id("open-in-list")
            .w_full()
            .max_h(picker::default_max_height(&theme))
            .overflow_y_scroll()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();
        for app in apps.iter() {
            let selected = app.id == preferred_id;
            let this = this.clone();
            let path = path.clone();
            let app_id = app.id;
            let app_icon = app.icon.clone();
            let label = app.label;
            list = list.child(
                picker_entry(
                    div().id(ElementId::Name(format!("open-in-{}", app.id).into())),
                    &theme,
                )
                .flex_none()
                .cursor_pointer()
                .when(selected, |row| row.bg(theme.selected_wash()))
                .hover_state(&theme)
                .child(
                    img(ImageSource::Image(app_icon))
                        .size(APP_ICON.px(&theme))
                        .flex_none(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(if selected {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        })
                        .child(label),
                )
                .when(selected, |row| {
                    row.child(icon(
                        "icons/check.svg",
                        context_menu::ICON.px(&theme),
                        theme.accent_text,
                    ))
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| {
                        app.open_workspace_in_app(&path, app_id, cx);
                    });
                }),
            );
        }

        if apps.is_empty() {
            list = list.child(
                picker_entry(div(), &theme)
                    .text_color(theme.text_muted)
                    .child(tr!("open_in.no_matches")),
            );
        }

        let popup = popover_picker(div(), &theme)
            .w(px(200.))
            .dismiss_on_outside({
                let this = this.clone();
                move |_: &MouseDownEvent, _, cx: &mut App| {
                    this.update(cx, |app, cx| {
                        if app.open_in_menu_open {
                            // Arm the click-through guard so this same click's
                            // mouse-up on the caret cannot reopen the menu it
                            // just dismissed.
                            app.menu_dismissed_at.arm();
                            app.open_in_menu_open = false;
                            cx.notify();
                        }
                    });
                }
            })
            // Search field — filters the apps below.
            .child(search_field(self.open_in_filter.clone(), &theme))
            .child(list);

        Some(
            dropdown_below(
                DropdownAlign::End,
                popover::MENU_OFFSET,
                popup,
                "anchored_popover-in-10",
                &theme,
            )
            .into_any_element(),
        )
    }
}
