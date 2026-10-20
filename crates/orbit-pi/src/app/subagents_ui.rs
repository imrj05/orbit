//! Settings → Subagents: a master-detail manager for delegation roles.
//!
//! The left column lists built-in, global, and project agent files; the right
//! column shows the parsed facts and the raw Markdown definition. User and
//! project files are editable and savable here. Built-ins are read-only; the
//! detail offers "duplicate to global/project" so they can be customized.
//!
//! Discovery, parsing, and file I/O live in [`crate::subagents`]; the runtime
//! extension reads the same files, so edits apply to the next delegation.

use super::helpers::*;
use super::*;
use crate::subagents::{self, Subagent, SubagentScope};
use crate::theme::tokens::{input, ButtonSize, DynamicSpacing, IconSize, Radius, TextSize};

/// A Subagents-page control, dispatched through one entry point.
#[derive(Clone)]
pub(super) enum SubagentAction {
    Select(PathBuf),
    New(SubagentScope),
    Save,
    Discard,
    AskDelete,
    ConfirmDelete,
    CancelDelete,
    Duplicate(SubagentScope),
    Open,
    Reveal,
    CopyPath,
    CopyDefinition,
    Refresh,
}

impl OrbitApp {
    // ── page ────────────────────────────────────────────────────────

    pub(super) fn render_subagents_page(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .bg(theme.bg_main)
            .font_family(theme::ui_font_family())
            .child(self.subagent_list(theme, this.clone(), cx))
            .child(self.subagent_detail(theme, this, cx))
            .into_any_element()
    }

    // ── list column ─────────────────────────────────────────────────

    fn subagent_list(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let needle = self.subagents_filter.read(cx).text().trim().to_lowercase();
        let matches = |agent: &&Subagent| {
            needle.is_empty()
                || agent.name.to_lowercase().contains(&needle)
                || agent.description.to_lowercase().contains(&needle)
                || scope_label(agent.scope).to_lowercase().contains(&needle)
        };
        let filtered: Vec<&Subagent> = self.subagents.iter().filter(matches).collect();

        let heading = div()
            .px(px(16.))
            .pt(px(14.))
            .pb(px(6.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(tr!("settings.subagents")),
            )
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(tr!("settings.subagents_description")),
            );

        let search = picker_search_frame(div(), &theme)
            .mx(DynamicSpacing::Base12.px(&theme))
            .mt(DynamicSpacing::Base12.px(&theme))
            .mb(DynamicSpacing::Base08.px(&theme))
            .child(icon(
                "icons/search.svg",
                input::ICON.px(&theme),
                theme.text_3,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.subagents_filter.clone()),
            );

        let new_rows = div()
            .mx(px(12.))
            .mb(px(8.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(self.subagent_action_button(
                "subagent-new-global".into(),
                &tr!("subagents_ui.new_global"),
                false,
                Some("icons/plus.svg"),
                theme,
                this.clone(),
                SubagentAction::New(SubagentScope::Global),
            ))
            .child(self.subagent_action_button(
                "subagent-new-project".into(),
                &tr!("subagents_ui.new_project"),
                false,
                Some("icons/plus.svg"),
                theme,
                this.clone(),
                SubagentAction::New(SubagentScope::Project),
            ));

        let mut list = div()
            .id("subagents-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .pb(px(8.));

        for scope in [
            SubagentScope::Project,
            SubagentScope::Global,
            SubagentScope::Builtin,
        ] {
            let group: Vec<&Subagent> = filtered
                .iter()
                .copied()
                .filter(|agent| agent.scope == scope)
                .collect();
            if group.is_empty() {
                continue;
            }
            list = list.child(self.subagent_group_label(scope, group.len(), theme));
            for agent in group {
                list = list.child(self.subagent_row(agent, theme, this.clone()));
            }
        }

        if filtered.is_empty() {
            list = list.child(
                div()
                    .px(px(16.))
                    .py(px(28.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(icon(
                        "icons/tools/task.svg",
                        IconSize::Custom(22. / 16.).px(&theme),
                        theme.text_3,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_2)
                            .child(tr!("subagents_ui.no_agents")),
                    )
                    .into_any_element(),
            );
        }

        let count_label = if filtered.len() == 1 {
            tr!("subagents_ui.agent_count_one")
        } else {
            tr!("subagents_ui.agent_count", count = filtered.len())
        };
        let footer = div()
            .px(px(14.))
            .py(px(8.))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_3)
                    .child(count_label),
            )
            .child(self.subagent_action_button(
                "subagents-refresh".into(),
                &tr!("subagents_ui.refresh"),
                false,
                Some("icons/refresh.svg"),
                theme,
                this.clone(),
                SubagentAction::Refresh,
            ));

        div()
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .bg(theme.bg_sidebar)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(heading)
            .child(search)
            .child(new_rows)
            .child(list)
            .child(footer)
            .into_any_element()
    }

    fn subagent_group_label(&self, scope: SubagentScope, count: usize, theme: Theme) -> AnyElement {
        div()
            .px(px(16.))
            .pt(px(10.))
            .pb(px(4.))
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child(scope_label(scope).to_uppercase()),
            )
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    fn subagent_row(&self, agent: &Subagent, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let selected = self.selected_subagent.as_deref() == Some(agent.file.as_path());
        let file = agent.file.clone();
        let description = agent.description.trim();
        div()
            .id(ElementId::Name(
                format!("subagent-row-{}", agent.file.display()).into(),
            ))
            .mx(px(10.))
            .px(px(8.))
            .py(px(7.))
            .rounded(Radius::Medium.px(&theme))
            .flex()
            .items_start()
            .gap_2p5()
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.active))
            .when(!selected, |row| row.hover(|style| style.bg(theme.bg_hover)))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.apply_subagent_action(SubagentAction::Select(file.clone()), cx)
                });
            })
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .rounded(Radius::Medium.px(&theme))
                    .bg(theme.bg_raised)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/tools/task.svg",
                        IconSize::Small.px(&theme),
                        if selected {
                            theme.active_fg
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
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(TextSize::Small.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if selected {
                                        theme.active_fg
                                    } else {
                                        theme.text
                                    })
                                    .child(agent.name.clone()),
                            )
                            .when(agent.shadowed, |row| {
                                row.child(self.subagent_chip(
                                    tr!("subagents_ui.shadowed"),
                                    theme.warn,
                                    theme,
                                ))
                            })
                            .when(!agent.valid, |row| {
                                row.child(self.subagent_chip(
                                    tr!("subagents_ui.invalid"),
                                    theme.crit,
                                    theme,
                                ))
                            }),
                    )
                    .when(!description.is_empty(), |col| {
                        col.child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_3)
                                .truncate()
                                .child(description.to_string()),
                        )
                    }),
            )
            .into_any_element()
    }

    fn subagent_chip(&self, label: String, color: gpui::Hsla, theme: Theme) -> AnyElement {
        div()
            .flex_none()
            .px(px(5.))
            .py(px(1.))
            .rounded(Radius::Small.px(&theme))
            .border_1()
            .border_color(color.opacity(0.4))
            .text_size(TextSize::XSmall.px(&theme))
            .text_color(color)
            .child(label)
            .into_any_element()
    }

    // ── detail column ───────────────────────────────────────────────

    fn subagent_detail(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let selected = self
            .selected_subagent
            .as_ref()
            .and_then(|path| self.subagents.iter().find(|agent| &agent.file == path))
            .cloned();

        let Some(agent) = selected else {
            return div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(icon(
                    "icons/tools/task.svg",
                    IconSize::Custom(30. / 16.).px(&theme),
                    theme.text_3,
                ))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(tr!("subagents_ui.select_an_agent")),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_2)
                        .child(tr!("subagents_ui.pick_one_from_the_list")),
                )
                .into_any_element();
        };

        let editable = agent.scope != SubagentScope::Builtin;
        let dirty = editable && self.subagent_is_dirty(cx);
        let actions = self.subagent_actions(&agent, dirty, theme, this.clone());
        let facts = self.subagent_facts(&agent, theme);
        let header = div()
            .flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .size(px(40.))
                    .flex_none()
                    .rounded(Radius::Large.px(&theme))
                    .bg(theme.bg_raised)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/tools/task.svg",
                        IconSize::Custom(20. / 16.).px(&theme),
                        theme.text_2,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(theme.ui_px(17.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(agent.name.clone()),
                            )
                            .when(dirty, |row| {
                                row.child(self.subagent_chip(
                                    tr!("subagents_ui.unsaved"),
                                    theme.warn,
                                    theme,
                                ))
                            }),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_3)
                            .truncate()
                            .child(agent.description.clone()),
                    ),
            );

        let error = self
            .subagent_editor_error
            .clone()
            .or_else(|| agent.error.clone());
        let body: AnyElement = if editable {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_size(input::LABEL.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_2)
                                .child(tr!("subagents_ui.editor_label")),
                        )
                        .child(
                            div()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_3)
                                .child(agent.display_path()),
                        ),
                )
                .children(error.as_ref().map(|error| {
                    div()
                        .w_full()
                        .px(DynamicSpacing::Base08.px(&theme))
                        .py(DynamicSpacing::Base06.px(&theme))
                        .rounded(Radius::Medium.px(&theme))
                        .border_1()
                        .border_color(theme.crit.opacity(0.45))
                        .bg(theme.crit.opacity(0.08))
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.crit)
                        .child(error.clone())
                }))
                .child(
                    div()
                        .w_full()
                        .min_h(px(360.))
                        .rounded(Radius::Large.px(&theme))
                        .border_1()
                        .border_color(if dirty {
                            theme.accent.opacity(0.5)
                        } else {
                            theme.border
                        })
                        .bg(theme.bg_composer)
                        .p(DynamicSpacing::Base08.px(&theme))
                        .child(self.subagent_editor.clone()),
                )
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_3)
                        .child(tr!("subagents_ui.read_only_builtin")),
                )
                .child(
                    div()
                        .id("subagent-readonly")
                        .w_full()
                        .min_h(px(240.))
                        .max_h(px(520.))
                        .overflow_y_scroll()
                        .rounded(Radius::Large.px(&theme))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.bg_composer)
                        .p(DynamicSpacing::Base12.px(&theme))
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(12.))
                        .text_color(theme.text_2)
                        .child(agent.raw.clone()),
                )
                .into_any_element()
        };

        div()
            .id("subagent-detail")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .px(px(24.))
            .pt(px(18.))
            .pb(px(24.))
            .gap_3()
            .child(header)
            .child(facts)
            .child(actions)
            .child(div().w_full().h(px(1.)).bg(theme.border))
            .child(body)
            .into_any_element()
    }

    fn subagent_facts(&self, agent: &Subagent, theme: Theme) -> AnyElement {
        let tools = if agent.tools.is_empty() {
            tr!("subagents_ui.default_tools")
        } else {
            agent.tools.join(", ")
        };
        let model = agent
            .model
            .clone()
            .unwrap_or_else(|| tr!("subagents_ui.inherits_parent"));
        let thinking = agent
            .thinking
            .clone()
            .unwrap_or_else(|| tr!("subagents_ui.inherits_parent"));
        let status = if !agent.valid {
            agent
                .error
                .clone()
                .unwrap_or_else(|| tr!("subagents_ui.invalid"))
        } else if agent.shadowed {
            tr!("subagents_ui.shadowed_hint")
        } else {
            tr!("subagents_ui.valid")
        };
        let status_color = if !agent.valid {
            theme.crit
        } else if agent.shadowed {
            theme.warn
        } else {
            theme.text
        };

        div()
            .w_full()
            .max_w(px(760.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .child(self.subagent_fact(
                theme,
                &tr!("subagents_ui.source"),
                self.subagent_value(theme, &scope_label(agent.scope)),
            ))
            .child(self.subagent_fact(
                theme,
                &tr!("subagents_ui.path"),
                self.subagent_value(theme, &agent.display_path()),
            ))
            .child(self.subagent_fact(
                theme,
                &tr!("subagents_ui.tools"),
                self.subagent_value(theme, &tools),
            ))
            .child(self.subagent_fact(
                theme,
                &tr!("subagents_ui.model"),
                self.subagent_value(theme, &model),
            ))
            .child(self.subagent_fact(
                theme,
                &tr!("subagents_ui.thinking"),
                self.subagent_value(theme, &thinking),
            ))
            .child(
                self.subagent_fact(
                    theme,
                    &tr!("subagents_ui.status"),
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(status_color)
                        .child(status)
                        .into_any_element(),
                ),
            )
            .into_any_element()
    }

    fn subagent_fact(&self, theme: Theme, label: &str, value: AnyElement) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(84.))
                    .flex_none()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_3)
                    .child(label.to_string()),
            )
            .child(div().flex_1().min_w_0().child(value))
            .into_any_element()
    }

    fn subagent_value(&self, theme: Theme, value: &str) -> AnyElement {
        div()
            .min_w_0()
            .truncate()
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text)
            .child(value.to_string())
            .into_any_element()
    }

    fn subagent_actions(
        &self,
        agent: &Subagent,
        dirty: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let confirming = self
            .subagent_delete_confirm
            .as_deref()
            .is_some_and(|path| path == agent.file.as_path());
        if confirming {
            return div()
                .w_full()
                .max_w(px(760.))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.crit)
                        .child(tr!("subagents_ui.delete_hint", path = agent.display_path())),
                )
                .child(self.subagent_action_button(
                    "subagent-delete-confirm".into(),
                    &tr!("sidebar.delete"),
                    true,
                    Some("icons/trash.svg"),
                    theme,
                    this.clone(),
                    SubagentAction::ConfirmDelete,
                ))
                .child(self.subagent_action_button(
                    "subagent-delete-cancel".into(),
                    &tr!("settings.cancel"),
                    false,
                    None,
                    theme,
                    this,
                    SubagentAction::CancelDelete,
                ))
                .into_any_element();
        }

        let editable = agent.scope != SubagentScope::Builtin;
        let mut actions = div()
            .w_full()
            .max_w(px(760.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2();

        if editable {
            actions = actions
                .child(self.subagent_action_button(
                    "subagent-save".into(),
                    &tr!("settings.save_changes"),
                    false,
                    Some("icons/check.svg"),
                    theme,
                    this.clone(),
                    SubagentAction::Save,
                ))
                .when(dirty, |actions| {
                    actions.child(self.subagent_action_button(
                        "subagent-discard".into(),
                        &tr!("git_panel.discard"),
                        false,
                        None,
                        theme,
                        this.clone(),
                        SubagentAction::Discard,
                    ))
                });
        } else {
            actions = actions
                .child(self.subagent_action_button(
                    "subagent-duplicate-global".into(),
                    &tr!("subagents_ui.duplicate_global"),
                    false,
                    Some("icons/copy.svg"),
                    theme,
                    this.clone(),
                    SubagentAction::Duplicate(SubagentScope::Global),
                ))
                .child(self.subagent_action_button(
                    "subagent-duplicate-project".into(),
                    &tr!("subagents_ui.duplicate_project"),
                    false,
                    Some("icons/copy.svg"),
                    theme,
                    this.clone(),
                    SubagentAction::Duplicate(SubagentScope::Project),
                ));
        }

        actions = actions
            .child(self.subagent_action_button(
                "subagent-copy-definition".into(),
                &tr!("subagents_ui.copy_definition"),
                false,
                Some("icons/copy.svg"),
                theme,
                this.clone(),
                SubagentAction::CopyDefinition,
            ))
            .child(self.subagent_action_button(
                "subagent-open".into(),
                &tr!("git_panel.open_file"),
                false,
                Some("icons/file.svg"),
                theme,
                this.clone(),
                SubagentAction::Open,
            ))
            .child(self.subagent_action_button(
                "subagent-reveal".into(),
                &tr!("sidebar.reveal_in_finder"),
                false,
                Some("icons/folder.svg"),
                theme,
                this.clone(),
                SubagentAction::Reveal,
            ))
            .child(self.subagent_action_button(
                "subagent-copy-path".into(),
                &tr!("sidebar.copy_path"),
                false,
                None,
                theme,
                this.clone(),
                SubagentAction::CopyPath,
            ))
            .child(div().flex_1());

        if editable {
            actions = actions.child(self.subagent_action_button(
                "subagent-delete".into(),
                &tr!("sidebar.delete"),
                true,
                Some("icons/trash.svg"),
                theme,
                this,
                SubagentAction::AskDelete,
            ));
        }

        actions.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn subagent_action_button(
        &self,
        id: String,
        label: &str,
        danger: bool,
        icon_path: Option<&'static str>,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: SubagentAction,
    ) -> AnyElement {
        let base = button_frame(
            div().id(ElementId::Name(id.into())),
            &theme,
            ButtonSize::Medium,
        )
        .group(BUTTON_GROUP)
        .cursor_pointer()
        .font_weight(FontWeight::MEDIUM);
        let (button, icon_color) = if danger {
            (
                base.border_1()
                    .border_color(theme.border)
                    .raised(theme.bg_raised, &theme)
                    .text_color(theme.crit)
                    .hover(|style| {
                        style
                            .border_color(theme.crit.opacity(0.6))
                            .raised(theme.crit.opacity(0.08), &theme)
                    }),
                theme.crit,
            )
        } else {
            (
                base.border_1()
                    .border_color(theme.border)
                    .raised(theme.bg_raised, &theme)
                    .text_color(theme.text_2)
                    .hover(|style| style.raised(theme.bg_hover, &theme)),
                theme.text_2,
            )
        };
        press(button)
            .when_some(icon_path, |button, path| {
                button.child(icon(
                    path,
                    ButtonSize::Medium.icon_size().px(&theme),
                    icon_color,
                ))
            })
            .child(div().child(label.to_string()))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let action = action.clone();
                this.update(cx, |app, cx| app.apply_subagent_action(action, cx));
            })
            .into_any_element()
    }

    // ── controller ──────────────────────────────────────────────────

    /// Re-scan the three roots. Unsaved editor content survives a re-scan
    /// unless the selected file disappeared or the caller asks for a reload.
    pub(super) fn refresh_subagents(&mut self, cx: &mut Context<Self>) {
        self.refresh_subagents_inner(false, cx);
    }

    fn refresh_subagents_inner(&mut self, reload_editor: bool, cx: &mut Context<Self>) {
        let dirty = self.subagent_is_dirty(cx);
        let previous = self.selected_subagent.clone();
        self.subagents = subagents::discover(&self.workspace_dir());
        let keep = previous
            .clone()
            .filter(|path| self.subagents.iter().any(|agent| &agent.file == path));
        self.selected_subagent =
            keep.or_else(|| self.subagents.first().map(|agent| agent.file.clone()));
        self.subagent_delete_confirm = None;
        let selection_changed = self.selected_subagent != previous;
        if reload_editor || !dirty || selection_changed {
            self.load_subagent_editor(cx);
        }
        cx.notify();
    }

    fn load_subagent_editor(&mut self, cx: &mut Context<Self>) {
        let selected = self
            .selected_subagent
            .as_ref()
            .and_then(|path| self.subagents.iter().find(|agent| &agent.file == path));
        let (raw, error) = match selected {
            Some(agent) => (agent.raw.clone(), agent.error.clone()),
            None => (String::new(), None),
        };
        self.subagent_editor_original = raw.clone();
        self.subagent_editor_error = error;
        self.subagent_editor
            .update(cx, |input, cx| input.set_text(raw, cx));
    }

    fn subagent_is_dirty(&self, cx: &Context<Self>) -> bool {
        self.selected_subagent.as_ref().is_some_and(|path| {
            self.subagents
                .iter()
                .find(|agent| &agent.file == path)
                .is_some_and(|agent| agent.scope != SubagentScope::Builtin)
        }) && self.subagent_editor.read(cx).text() != self.subagent_editor_original
    }

    fn selected_subagent(&self) -> Option<Subagent> {
        self.selected_subagent
            .as_ref()
            .and_then(|path| self.subagents.iter().find(|agent| &agent.file == path))
            .cloned()
    }

    /// Single dispatch point for every Subagents-page control.
    pub(super) fn apply_subagent_action(&mut self, action: SubagentAction, cx: &mut Context<Self>) {
        match action {
            SubagentAction::Select(path) => {
                if self.selected_subagent.as_ref() == Some(&path) {
                    return;
                }
                if self.subagent_is_dirty(cx) {
                    self.toast_warning(tr!("subagents_ui.unsaved_warning"));
                    return;
                }
                self.selected_subagent = Some(path);
                self.subagent_delete_confirm = None;
                self.load_subagent_editor(cx);
            }
            SubagentAction::New(scope) => self.new_subagent(scope, cx),
            SubagentAction::Save => self.save_subagent(cx),
            SubagentAction::Discard => {
                self.load_subagent_editor(cx);
                self.set_status(tr!("subagents_ui.discarded"));
            }
            SubagentAction::AskDelete => {
                if let Some(agent) = self.selected_subagent() {
                    if agent.scope == SubagentScope::Builtin {
                        self.toast_error(tr!("subagents_ui.builtin_read_only"));
                        return;
                    }
                    self.subagent_delete_confirm = Some(agent.file);
                }
            }
            SubagentAction::ConfirmDelete => self.delete_subagent(cx),
            SubagentAction::CancelDelete => {
                self.subagent_delete_confirm = None;
            }
            SubagentAction::Duplicate(scope) => self.duplicate_subagent(scope, cx),
            SubagentAction::Refresh => self.refresh_subagents(cx),
            SubagentAction::Open => {
                if let Some(agent) = self.selected_subagent() {
                    platform::open_path_default(&agent.file);
                }
            }
            SubagentAction::Reveal => {
                if let Some(agent) = self.selected_subagent() {
                    platform::reveal_in_file_manager(&agent.file);
                }
            }
            SubagentAction::CopyPath => {
                if let Some(agent) = self.selected_subagent() {
                    let path = agent.display_path();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
                    self.toast_success(tr!("subagents_ui.copied_path", path = path));
                }
            }
            SubagentAction::CopyDefinition => {
                if let Some(agent) = self.selected_subagent() {
                    let text = if agent.scope == SubagentScope::Builtin {
                        agent.raw.clone()
                    } else {
                        self.subagent_editor.read(cx).text()
                    };
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    self.toast_success(tr!("subagents_ui.copied_definition", name = agent.name));
                }
            }
        }
        cx.notify();
    }

    fn new_subagent(&mut self, scope: SubagentScope, cx: &mut Context<Self>) {
        if self.subagent_is_dirty(cx) {
            self.toast_warning(tr!("subagents_ui.unsaved_warning"));
            return;
        }
        if scope == SubagentScope::Project && self.current_workspace.is_none() {
            self.toast_error(tr!("subagents_ui.no_workspace"));
            return;
        }
        let Some(dir) = subagents::target_dir(&self.workspace_dir(), scope) else {
            self.toast_error(tr!("subagents_ui.builtin_read_only"));
            return;
        };
        let path = subagents::unique_new_path(&dir, "new-agent");
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "new-agent".to_string());
        let raw = subagents::template(&name);
        if let Err(error) = subagents::write_agent(&path, &raw) {
            self.set_error(format!("{}: {error}", path.display()));
            return;
        }
        self.selected_subagent = Some(path.clone());
        self.refresh_subagents_inner(true, cx);
        self.toast_success(tr!("subagents_ui.created", name = name));
    }

    fn save_subagent(&mut self, cx: &mut Context<Self>) {
        let Some(agent) = self.selected_subagent() else {
            return;
        };
        if agent.scope == SubagentScope::Builtin {
            self.toast_error(tr!("subagents_ui.builtin_read_only"));
            return;
        }
        let raw = self.subagent_editor.read(cx).text();
        let parsed = subagents::parse_agent(&raw, &agent.file, agent.scope);
        if !parsed.valid {
            self.subagent_editor_error = parsed.error;
            return;
        }
        if self.subagents.iter().any(|other| {
            other.file != agent.file && other.scope == agent.scope && other.name == parsed.name
        }) {
            self.subagent_editor_error =
                Some(tr!("subagents_ui.duplicate_name", name = parsed.name));
            return;
        }
        if let Err(error) = subagents::write_agent(&agent.file, &raw) {
            self.subagent_editor_error = Some(format!("{error}"));
            return;
        }
        self.subagent_editor_original = raw;
        self.subagent_editor_error = None;
        self.refresh_subagents_inner(true, cx);
        self.toast_success(tr!("subagents_ui.saved", name = parsed.name));
    }

    fn delete_subagent(&mut self, cx: &mut Context<Self>) {
        let Some(agent) = self.selected_subagent() else {
            return;
        };
        if agent.scope == SubagentScope::Builtin {
            self.toast_error(tr!("subagents_ui.builtin_read_only"));
            return;
        }
        if let Err(error) = subagents::delete_agent(&agent.file) {
            self.set_error(format!("{}: {error}", agent.file.display()));
            return;
        }
        self.subagent_delete_confirm = None;
        self.selected_subagent = None;
        self.refresh_subagents_inner(true, cx);
        self.toast_success(tr!("subagents_ui.deleted", name = agent.name));
    }

    fn duplicate_subagent(&mut self, scope: SubagentScope, cx: &mut Context<Self>) {
        if self.subagent_is_dirty(cx) {
            self.toast_warning(tr!("subagents_ui.unsaved_warning"));
            return;
        }
        if scope == SubagentScope::Project && self.current_workspace.is_none() {
            self.toast_error(tr!("subagents_ui.no_workspace"));
            return;
        }
        let Some(agent) = self.selected_subagent() else {
            return;
        };
        let Some(dir) = subagents::target_dir(&self.workspace_dir(), scope) else {
            self.toast_error(tr!("subagents_ui.builtin_read_only"));
            return;
        };
        let path = match subagents::duplicate_path(&dir, &agent.name) {
            Ok(path) => path,
            Err(error) => {
                self.toast_error(error);
                return;
            }
        };
        if let Err(error) = subagents::write_agent(&path, &agent.raw) {
            self.set_error(format!("{}: {error}", path.display()));
            return;
        }
        self.selected_subagent = Some(path.clone());
        self.refresh_subagents_inner(true, cx);
        self.toast_success(tr!(
            "subagents_ui.duplicated",
            path = subagents::display_path(&path)
        ));
    }
}

fn scope_label(scope: SubagentScope) -> String {
    match scope {
        SubagentScope::Builtin => tr!("subagents_ui.scope_builtin"),
        SubagentScope::Global => tr!("subagents_ui.scope_global"),
        SubagentScope::Project => tr!("subagents_ui.scope_project"),
    }
}
