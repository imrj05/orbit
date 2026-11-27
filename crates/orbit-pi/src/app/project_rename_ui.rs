//! The one-field rename dialog shared by sessions and projects. A project
//! rename (#166) stores the project's display name in
//! `~/.orbit-pi/workspaces.json` beside its icon and tint
//! ([`WorkspaceMark::name`]). The folder on disk and pi's sessions are never
//! touched; clearing the field brings the folder's own name back.
use super::*;
use crate::theme::tokens::{ButtonSize, Motion};
use crate::ui::dialog::{
    form_dialog_body, form_dialog_close, form_dialog_footer, form_dialog_header, modal_button,
    ModalSubtitle,
};
use crate::ui::field::{FormRow, TextField};
use crate::ui::prelude::*;

type RenameAction = fn(&mut OrbitApp, &mut Context<OrbitApp>);

struct RenameForm {
    title: String,
    description: String,
    field: FormRow,
    save_enabled: bool,
}

/// The open Rename dialog.
pub(crate) struct ProjectRename {
    /// The project's folder — the key its mark is stored under.
    pub(crate) cwd: PathBuf,
    /// The folder's own name, which an empty field restores.
    pub(crate) folder: String,
    pub(crate) input: Entity<ComposerInput>,
}

impl OrbitApp {
    /// The header menu's **Rename…**: open the dialog on the menu's project,
    /// seeded with the name it shows now.
    pub(super) fn on_workspace_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.workspace_menu.take() else {
            return;
        };
        self.open_project_rename(menu.cwd, menu.label, window, cx);
    }

    /// Open the rename dialog on one project, seeded with the name it shows
    /// now. Used by the header menu and, right after a project is created, so
    /// a folder can be named without hunting for the menu (#173).
    pub(super) fn open_project_rename(
        &mut self,
        cwd: PathBuf,
        folder: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_name_editing || self.project_rename.is_some() {
            cx.notify();
            return;
        }
        let current = self
            .workspace_marks
            .get(&cwd)
            .map(|mark| mark.display_name(&folder).to_string())
            .unwrap_or_else(|| folder.clone());
        let input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("project-rename-input")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder(folder.clone())
                .with_text(current)
        });
        let target = input.read(cx).focus_handle(cx);
        self.project_rename_focus.open(&target, window, cx);
        self.project_rename = Some(ProjectRename { cwd, folder, input });
        cx.notify();
    }

    /// Store the typed name (or clear it) and close.
    pub(super) fn project_rename_save(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.project_rename.as_ref() else {
            return;
        };
        // The field holds the preedit while an IME candidate window is open.
        if crate::composer::any_composing(cx, &[&rename.input]) {
            return;
        }
        let typed = rename.input.read(cx).text();
        let name = workspace_mark::custom_name(&typed, &rename.folder);
        let cwd = rename.cwd.clone();
        let mark = WorkspaceMark {
            name,
            ..self.workspace_marks.get(&cwd).cloned().unwrap_or_default()
        };
        self.set_workspace_mark(cwd, mark);
        self.project_rename = None;
        cx.notify();
    }

    pub(super) fn project_rename_cancel(&mut self, cx: &mut Context<Self>) {
        if self.project_rename.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn project_rename_layer(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let rename = self.project_rename.as_ref()?;
        let theme = theme.for_floating();
        Some(self.rename_dialog(
            "project-rename",
            RenameForm {
                title: tr!("sidebar.rename_project_title"),
                description: tr!("sidebar.project_name_hint"),
                field: FormRow::new(
                    tr!("sidebar.project_name"),
                    TextField::new(rename.input.clone(), theme.background).render(&theme),
                ),
                save_enabled: true,
            },
            [Self::project_rename_cancel, Self::project_rename_save],
            theme,
            cx,
        ))
    }

    pub(super) fn session_rename_layer(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        self.session_name_editing.then(|| {
            let theme = theme.for_floating();
            let mut field = FormRow::new(
                tr!("session.name"),
                TextField::new(self.session_name_input.clone(), theme.background).render(&theme),
            );
            let active = self.session_rename_target.is_none();
            let wand = if active {
                self.generate_title_button(
                    "session-rename-generate-title",
                    theme,
                    cx.entity(),
                    Some(&self.rename_generate_focus),
                )
            } else {
                None
            };
            if let Some(wand) = wand {
                field = field
                    .action(wand)
                    .hint(tr!("session.generate_title_applies"));
            }
            self.rename_dialog(
                "session-rename",
                RenameForm {
                    title: tr!("command_palette.rename_session"),
                    description: if active {
                        tr!("session.rename_description")
                    } else {
                        tr!("session.rename_opens_on_save")
                    },
                    field,
                    save_enabled: !active || self.title_generation.is_none(),
                },
                [Self::cancel_session_name_edit, Self::rename_session],
                theme,
                cx,
            )
        })
    }

    fn rename_dialog(
        &self,
        id: &'static str,
        form: RenameForm,
        [cancel, save]: [RenameAction; 2],
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let this = cx.entity();
        let click = |action: RenameAction| {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| this.update(cx, action)
        };
        let close = form_dialog_close(
            SharedString::from(format!("{id}-close")),
            &theme,
            click(cancel),
        )
        .track_focus(&self.rename_close_focus)
        .into_any_element();
        let cancel_button = modal_button(
            format!("{id}-cancel"),
            tr!("view.cancel"),
            ButtonVariant::Outline,
            &self.modal_cancel_focus,
        )
        .size(ButtonSize::Large)
        .on_click(&theme, click(cancel));
        let save_button = modal_button(
            format!("{id}-save"),
            tr!("settings.save_changes"),
            ButtonVariant::Primary,
            &self.modal_confirm_focus,
        )
        .size(ButtonSize::Large)
        .disabled(!form.save_enabled)
        .on_click(&theme, click(save));
        let card = modal_card(&theme)
            .debug_selector(|| format!("{id}-card"))
            .modal_width(theme.rems(24.))
            .max_h(relative(1.))
            .key_context("Picker")
            .on_action(cx.listener(move |app, _: &crate::PickerCancel, _, cx| cancel(app, cx)))
            .on_action(
                cx.listener(move |app, _: &crate::PickerConfirm, window, cx| {
                    if app.modal_cancel_focus.is_focused(window)
                        || app.rename_close_focus.is_focused(window)
                    {
                        cancel(app, cx);
                    } else if app.rename_generate_focus.is_focused(window) {
                        app.generate_title_in_rename(window, cx);
                    } else {
                        save(app, cx);
                    }
                }),
            )
            .child(
                form_dialog_header(
                    &theme,
                    form.title,
                    Some(ModalSubtitle::Text(form.description.into())),
                    Some(close),
                )
                .debug_selector(|| format!("{id}-header")),
            )
            .child(
                form_dialog_body(&theme, SharedString::from(format!("{id}-body")))
                    .debug_selector(|| format!("{id}-field"))
                    .child(form.field.render(&theme)),
            )
            .child(
                form_dialog_footer(&theme)
                    .debug_selector(|| format!("{id}-actions"))
                    .child(cancel_button)
                    .child(save_button),
            );

        modal_scrim(&theme)
            .id(SharedString::from(format!("{id}-layer")))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| this.update(cx, cancel))
            .child(crate::motion::enter(
                card,
                format!("{id}-in"),
                &theme,
                Motion::Layout,
                None,
            ))
            .into_any_element()
    }
}
