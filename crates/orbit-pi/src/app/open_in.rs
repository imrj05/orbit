use super::helpers::*;
use super::*;

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
        let Some(app) = self.open_in_apps.iter().find(|app| app.id == app_id).cloned() else {
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
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.open_in_apps.is_empty() || self.current_workspace.is_none() {
            return;
        }
        self.open_in_menu_open = !self.open_in_menu_open;
        if self.open_in_menu_open {
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
        let preferred_icon = preferred.icon.clone();
        let this = cx.entity().clone();
        let path = Rc::from(path.as_path());

        let primary = div()
            .id("header-open-in")
            .group(BUTTON_GROUP)
            .h_full()
            .px(px(2.))
            .rounded_tl(px(HEADER_CTRL_R))
            .rounded_bl(px(HEADER_CTRL_R))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .active(|s| s.bg(theme.active).text_color(theme.active_fg))
            .child(
                img(ImageSource::Image(preferred_icon))
                    .size(px(20.))
                    .flex_none(),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_open_in_primary));

        let caret = div()
            .id("header-open-in-caret")
            .group(BUTTON_GROUP)
            .relative()
            .h_full()
            .w(px(19.))
            .rounded_tr(px(HEADER_CTRL_R))
            .rounded_br(px(HEADER_CTRL_R))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when(self.open_in_menu_open, |s| {
                s.bg(theme.active).text_color(theme.active_fg)
            })
            .child(icon("icons/chevron-down.svg", 11., theme.text_3))
            // Pin the dropdown to the caret's bottom-right — same zero-size
            // anchor trick as the session row menu, so flex centering doesn't
            // pull the popup toward the button's middle.
            .children(self.open_in_menu_popup(&this, path, preferred_id, theme, cx))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_open_in_caret));

        Some(
            header_chip(
                div()
                    .h(px(HEADER_CTRL_H))
                    .rounded(px(HEADER_CTRL_R))
                    .flex_none()
                    .flex()
                    .items_center(),
                &theme,
            )
            .child(primary)
            .child(div().w(px(1.)).h_full().flex_none().bg(theme.border))
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
            .w_full()
            .px(px(4.))
            .py(px(4.))
            .flex()
            .flex_col()
            .gap(px(2.));
        for app in apps.iter() {
            let selected = app.id == preferred_id;
            let this = this.clone();
            let path = path.clone();
            let app_id = app.id;
            let app_icon = app.icon.clone();
            let label = app.label;
            list = list.child(
                div()
                    .id(ElementId::Name(format!("open-in-{}", app.id).into()))
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(theme.active))
                    .hover(|style| style.bg(theme.overlay))
                    .child(img(ImageSource::Image(app_icon)).size(px(16.)).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme.ui_px(12.))
                            .text_color(if selected {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(label),
                    )
                    .when(selected, |row| {
                        row.child(icon("icons/check.svg", 11., theme.accent))
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
                div()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(theme.ui_px(12.))
                    .text_color(theme.text_3)
                    .child(tr!("open_in.no_matches")),
            );
        }

        let popup = div()
            .w(px(200.))
            .font_family(theme::ui_font_family())
            .rounded(px(8.))
            .popover_surface(theme)
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out({
                let this = this.clone();
                move |_: &MouseDownEvent, _, cx: &mut App| {
                    this.update(cx, |app, cx| {
                        if app.open_in_menu_open {
                            // Arm the click-through guard so this same click's
                            // mouse-up on the caret cannot reopen the menu it
                            // just dismissed.
                            app.menu_dismissed_at = Some(Instant::now());
                            app.open_in_menu_open = false;
                            cx.notify();
                        }
                    });
                }
            })
            // Search field — filters the apps below.
            .child(
                div()
                    .h(px(30.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(theme.ui_px(12.))
                    .child(icon("icons/search.svg", 13., theme.text_3))
                    .child(self.open_in_filter.clone()),
            )
            .child(list);

        Some(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .size(px(0.))
                .child(
                    anchored()
                        .position_mode(AnchoredPositionMode::Local)
                        .anchor(Corner::TopRight)
                        .offset(point(px(0.), px(4.)))
                        .snap_to_window()
                        .child(deferred(popup)),
                )
                .into_any_element(),
        )
    }
}
