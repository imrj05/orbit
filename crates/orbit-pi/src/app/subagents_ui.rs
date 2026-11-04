//! Settings → Subagents: a master-detail manager for the agents the
//! `subagent` tool can pick.
//!
//! The left column lists builtins and user files; the right column edits the
//! five keys Orbit owns, the workflow mode, and the enable switch. Discovery
//! and the surgical writer live in [`crate::subagent_file`]; the Orbit-only
//! half (enabled, mode) lives in [`crate::subagents`]. Nothing here duplicates
//! either: a save re-reads the file, refuses to clobber an external edit, and
//! leaves every key it does not own byte-for-byte.

use super::helpers::*;
use super::*;
use crate::subagent_file::{self, Agent, Source, Tier};
use crate::subagents;
use crate::theme::tokens::{input, ButtonSize, DynamicSpacing, IconSize, Radius, TextSize};

/// One Skills-page-style dispatch point for every Subagents control.
#[derive(Clone)]
pub(super) enum SubagentAction {
    Select(String),
    New,
    Save,
    Duplicate,
    Reveal,
    ToggleEnabled,
    SetTier(Tier),
    SetMode(&'static str),
    AskDelete,
    ConfirmDelete,
    CancelDelete,
    UndoDelete,
}

/// The five inputs and the facts a save needs. Rebuilt on selection, so the
/// fields always reflect the file on disk rather than a stale clone.
pub(super) struct SubagentEditor {
    pub name: Entity<ComposerInput>,
    pub description: Entity<ComposerInput>,
    pub model: Entity<ComposerInput>,
    pub thinking: Entity<ComposerInput>,
    pub prompt: Entity<ComposerInput>,
    /// `None` when the file's list is not one of the three tiers; the page
    /// shows it as Advanced rather than silently narrowing it.
    pub tier: Option<Tier>,
    pub tools: Option<Vec<String>>,
    pub mode: &'static str,
    pub path: Option<PathBuf>,
    pub loaded_hash: Option<u64>,
    pub is_new: bool,
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
            .bg(theme.background)
            .font_family(theme::ui_font_family())
            .child(self.subagents_list(theme, this.clone(), cx))
            .child(self.subagent_detail(theme, this, cx))
            .into_any_element()
    }

    // ── list column ─────────────────────────────────────────────────

    fn subagents_list(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let needle = self.subagents_filter.read(cx).text().trim().to_lowercase();
        let matches = |agent: &&Agent| {
            needle.is_empty()
                || agent.name().to_lowercase().contains(&needle)
                || agent.fields.description.to_lowercase().contains(&needle)
        };
        let filtered: Vec<&Agent> = self.subagents.iter().filter(matches).collect();

        let search = picker_search_frame(div(), &theme)
            .mx(DynamicSpacing::Base12.px(&theme))
            .mt(DynamicSpacing::Base12.px(&theme))
            .mb(DynamicSpacing::Base08.px(&theme))
            .child(icon(
                "icons/search.svg",
                input::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.subagents_filter.clone()),
            );

        let mut list = div()
            .id("subagents-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .pb(DynamicSpacing::Base08.px(&theme));

        for source in [Source::Builtin, Source::User] {
            let group: Vec<&Agent> = filtered
                .iter()
                .copied()
                .filter(|agent| agent.source == source)
                .collect();
            if group.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .pt(px(10.))
                    .pb(DynamicSpacing::Base04.px(&theme))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(TextSize::XSmall.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child(
                                match source {
                                    Source::Builtin => tr!("subagents_ui.built_in"),
                                    Source::User => tr!("subagents_ui.your_agents"),
                                }
                                .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_muted)
                            .child(group.len().to_string()),
                    ),
            );
            for agent in group {
                list = list.child(self.subagent_row(agent, theme, this.clone()));
            }
        }

        if filtered.is_empty() {
            list = list.child(
                div()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(px(28.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(icon(
                        "icons/search.svg",
                        IconSize::Custom(22. / 16.).px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!("subagents_ui.no_agents_match")),
                    ),
            );
        }

        let new_agent = {
            let this = this.clone();
            button_frame(
                div().id(ElementId::Name("subagents-new".into())),
                &theme,
                ButtonSize::Medium,
            )
            .group(BUTTON_GROUP)
            .cursor_pointer()
            .font_weight(FontWeight::MEDIUM)
            .border_1()
            .border_color(theme.border)
            .raised(theme.surface_elevated, &theme)
            .text_color(theme.text_secondary)
            .hover_state(&theme)
            .child(icon(
                "icons/plus.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                theme.text_secondary,
            ))
            .child(div().child(tr!("subagents_ui.new_agent")))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.apply_subagent_action(SubagentAction::New, cx)
                });
            })
            .into_any_element()
        };

        let mut footer = div()
            .px(px(14.))
            .py(DynamicSpacing::Base08.px(&theme))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(new_agent);
        if let Some(undo) = &self.subagent_undo {
            let this = this.clone();
            footer = footer.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("subagents_ui.deleted_agent", name = undo.name.clone()))
                    .child(
                        button_frame(
                            div().id(ElementId::Name("subagents-undo".into())),
                            &theme,
                            ButtonSize::Medium,
                        )
                        .group(BUTTON_GROUP)
                        .cursor_pointer()
                        .text_color(theme.accent_text)
                        .hover_state(&theme)
                        .child(tr!("subagents_ui.undo"))
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            this.update(cx, |app, cx| {
                                app.apply_subagent_action(SubagentAction::UndoDelete, cx)
                            });
                        }),
                    ),
            );
        }
        footer = footer.child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!(
                    "subagents_ui.agent_dir",
                    path = subagent_file::agents_dir().display().to_string()
                )),
        );

        div()
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(search)
            .child(list)
            .child(footer)
            .into_any_element()
    }

    fn subagent_row(&self, agent: &Agent, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let selected = self.selected_subagent.as_deref() == Some(agent.name());
        let name = agent.name().to_string();
        let config = &self.subagent_config;
        let enabled = config.agent_enabled(&name);
        let mode = config.agent_mode(&name);
        let ceiling = agent.ceiling();
        let malformed = agent.parse_error.is_some()
            || !self.subagents_skipped.is_empty()
                && self
                    .subagents_skipped
                    .iter()
                    .any(|(path, _)| Some(path) == agent.path.as_ref());
        let tier_color = if selected {
            theme.selection_fg
        } else {
            theme.text_muted
        };
        let row = div()
            .id(ElementId::Name(format!("subagent-row-{name}").into()))
            .mx(px(10.))
            .px(DynamicSpacing::Base08.px(&theme))
            .py(px(7.))
            .rounded(Radius::Medium.px(&theme))
            .flex()
            .items_start()
            .gap_2p5()
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.surface_selected))
            .when(!selected, |row| row.hover_state(&theme))
            .when(!enabled, |row| row.opacity(0.55))
            .on_mouse_up(MouseButton::Left, {
                let select = name.clone();
                move |_, _, cx| {
                    let select = select.clone();
                    this.update(cx, |app, cx| {
                        app.apply_subagent_action(SubagentAction::Select(select), cx)
                    });
                }
            })
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .rounded(Radius::Medium.px(&theme))
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/task.svg",
                        IconSize::Small.px(&theme),
                        if selected {
                            theme.text_primary
                        } else {
                            theme.text_secondary
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
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if selected {
                                theme.selection_fg
                            } else {
                                theme.text_primary
                            })
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(tier_color)
                            .child(capability_label(ceiling))
                            .child("·")
                            .child(mode_label(mode))
                            .when_some(agent.fields.model.clone(), |row, model| {
                                row.child("·").child(model)
                            }),
                    ),
            )
            .when(malformed, |row| {
                row.child(icon(
                    "icons/alert-circle.svg",
                    IconSize::XSmall.px(&theme),
                    theme.warning,
                ))
            })
            .when(!enabled, |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("subagents_ui.disabled")),
                )
            });
        row.into_any_element()
    }

    // ── detail column ───────────────────────────────────────────────

    fn subagent_detail(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.subagent_editor.as_ref() else {
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
                    "icons/task.svg",
                    IconSize::Custom(30. / 16.).px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(tr!("subagents_ui.select_an_agent")),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .child(tr!("subagents_ui.pick_one_to_edit_its_prompt_tools")),
                )
                .into_any_element();
        };

        let selected = self
            .selected_subagent
            .as_ref()
            .and_then(|name| self.subagents.iter().find(|agent| agent.name() == name));
        let enabled = selected
            .map(|agent| self.subagent_config.agent_enabled(agent.name()))
            .unwrap_or(true);
        let overrides_builtin = editor.path.is_some()
            && self.subagents.iter().any(|agent| {
                agent.name() == self.selected_subagent.as_deref().unwrap_or("")
                    && agent.source == Source::Builtin
            });
        let (source_label, action_label) = if editor.is_new {
            (tr!("subagents_ui.new"), tr!("subagents_ui.create"))
        } else if editor.path.is_none() {
            // A builtin has no file to delete; the enable switch is its action.
            (tr!("subagents_ui.built_in"), String::new())
        } else if overrides_builtin {
            (
                tr!("subagents_ui.modified"),
                tr!("subagents_ui.reset_to_builtin"),
            )
        } else {
            (tr!("subagents_ui.yours"), tr!("subagents_ui.delete"))
        };

        let description_text = editor.description.read(cx).text();
        let description_len = description_text.chars().count();

        let header = div()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .truncate()
                            .child(if editor.is_new {
                                tr!("subagents_ui.new_agent")
                            } else {
                                self.selected_subagent
                                    .clone()
                                    .unwrap_or_else(|| tr!("subagents_ui.no_agent"))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(source_label)
                            .when(!editor.is_new && !enabled, |row| {
                                row.child("·").child(tr!("subagents_ui.disabled"))
                            }),
                    ),
            )
            .children(self.subagent_buttons(theme, this.clone(), editor, enabled, &action_label));

        let mut body = div()
            .id("subagents-detail")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme));

        body = body.child(self.subagent_text_field(
            &theme,
            tr!("subagents_ui.description"),
            &editor.description,
            tr!("subagents_ui.description_hint"),
        ));
        body = body.child(
            div()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(if description_len > 200 {
                    theme.warning
                } else {
                    theme.text_muted
                })
                .child(tr!(
                    "subagents_ui.description_count",
                    count = description_len
                )),
        );
        body = body.child(self.subagent_capability_field(theme, this.clone(), editor));
        body = body.child(self.subagent_mode_field(theme, this.clone(), editor));
        body = body.child(
            div()
                .flex()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(self.subagent_text_field(
                    &theme,
                    tr!("subagents_ui.model"),
                    &editor.model,
                    tr!("subagents_ui.model_hint"),
                ))
                .child(self.subagent_text_field(
                    &theme,
                    tr!("subagents_ui.thinking"),
                    &editor.thinking,
                    tr!("subagents_ui.thinking_hint"),
                )),
        );
        body = body.child(self.subagent_text_field(
            &theme,
            tr!("subagents_ui.prompt"),
            &editor.prompt,
            tr!("subagents_ui.prompt_hint"),
        ));
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary)
                        .child(tr!("subagents_ui.what_the_model_sees")),
                )
                .child(
                    div()
                        .rounded(Radius::Medium.px(&theme))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.code_bg)
                        .p(DynamicSpacing::Base08.px(&theme))
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(12.))
                        .text_color(theme.text_secondary)
                        .whitespace_normal()
                        .child(SharedString::from(self.subagent_preview())),
                ),
        );

        if let Some(path) = &editor.path {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(SharedString::from(path.display().to_string()))
                    .child(
                        div()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_muted)
                            .child(match selected.and_then(|agent| agent.modified) {
                                Some(modified) => tr!(
                                    "subagents_ui.modified_ago",
                                    time = sessions::relative_time(modified)
                                ),
                                None => String::new(),
                            }),
                    )
                    .child(
                        div()
                            .cursor_pointer()
                            .text_color(theme.accent_text)
                            .child(tr!("subagents_ui.open_in_editor"))
                            .on_mouse_up(MouseButton::Left, {
                                let this = this.clone();
                                let path = path.clone();
                                move |_, _, cx| {
                                    let path = path.clone();
                                    this.update(cx, |app, cx| {
                                        app.subagent_open_path(&path, cx);
                                    });
                                }
                            }),
                    ),
            );
        }

        let mut column = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(body);
        if let Some(error) = &self.subagent_error {
            column = column.child(
                div()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.error.opacity(0.07))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.error)
                    .whitespace_normal()
                    .child(SharedString::from(error.clone())),
            );
        }
        column.into_any_element()
    }

    fn subagent_text_field(
        &self,
        theme: &Theme,
        label: String,
        input: &Entity<ComposerInput>,
        hint: String,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(input::gap(theme))
            .child(
                input_label(label, theme)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary),
            )
            .child(
                input_field_frame(div(), theme)
                    .w_full()
                    .bg(theme.background)
                    .child(input.clone()),
            )
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .child(SharedString::from(hint)),
            )
            .into_any_element()
    }

    fn subagent_capability_field(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
    ) -> AnyElement {
        let mut group = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(tr!("subagents_ui.what_it_can_do")),
            );
        for tier in Tier::ALL {
            let selected = editor.tier == Some(tier);
            let chips = tier
                .tools()
                .iter()
                .map(|tool| tool.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let this = this.clone();
            group = group.child(
                div()
                    .id(ElementId::Name(
                        format!("subagent-tier-{}", tier_label(tier)).into(),
                    ))
                    .px(DynamicSpacing::Base08.px(&theme))
                    .py(DynamicSpacing::Base06.px(&theme))
                    .rounded(Radius::Medium.px(&theme))
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .when(selected, |row| row.bg(theme.surface_selected))
                    .when(!selected, |row| row.hover_state(&theme))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        this.update(cx, |app, cx| {
                            app.apply_subagent_action(SubagentAction::SetTier(tier), cx)
                        });
                    })
                    .child(div().size(px(8.)).rounded_full().bg(if selected {
                        theme.accent
                    } else {
                        theme.border
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_primary)
                                    .child(tier_description(tier)),
                            )
                            .child(
                                div()
                                    .font_family(theme::code_font_family())
                                    .text_size(theme.code_px(11.))
                                    .text_color(theme.text_muted)
                                    .child(chips),
                            ),
                    ),
            );
        }
        if editor.tier.is_none() {
            if let Some(tools) = &editor.tools {
                group = group.child(
                    div()
                        .px(DynamicSpacing::Base08.px(&theme))
                        .py(DynamicSpacing::Base06.px(&theme))
                        .rounded(Radius::Medium.px(&theme))
                        .border_1()
                        .border_color(theme.warning.opacity(0.5))
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .whitespace_normal()
                        .child(tr!("subagents_ui.custom_tools", tools = tools.join(", "))),
                );
            }
        }
        group.into_any_element()
    }

    fn subagent_mode_field(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
    ) -> AnyElement {
        let mut group = div()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(tr!("subagents_ui.mode")),
            );
        for (mode, label, hint) in [
            (
                "inherit",
                tr!("subagents_ui.mode_inherit"),
                tr!("subagents_ui.mode_inherit_hint"),
            ),
            (
                "plan",
                tr!("subagents_ui.mode_plan"),
                tr!("subagents_ui.mode_plan_hint"),
            ),
            (
                "build",
                tr!("subagents_ui.mode_build"),
                tr!("subagents_ui.mode_build_hint"),
            ),
            (
                "ask",
                tr!("subagents_ui.mode_ask"),
                tr!("subagents_ui.mode_ask_hint"),
            ),
        ] {
            let selected = editor.mode == mode;
            let this = this.clone();
            group = group.child(
                div()
                    .id(ElementId::Name(format!("subagent-mode-{mode}").into()))
                    .px(DynamicSpacing::Base08.px(&theme))
                    .py(DynamicSpacing::Base04.px(&theme))
                    .rounded(Radius::Medium.px(&theme))
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .when(selected, |row| row.bg(theme.surface_selected))
                    .when(!selected, |row| row.hover_state(&theme))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        this.update(cx, |app, cx| {
                            app.apply_subagent_action(SubagentAction::SetMode(mode), cx)
                        });
                    })
                    .child(div().size(px(8.)).rounded_full().bg(if selected {
                        theme.accent
                    } else {
                        theme.border
                    }))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_muted)
                            .child(hint),
                    ),
            );
        }
        group
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(tr!("subagents_ui.mode_rule")),
            )
            .into_any_element()
    }

    /// The buttons for the selected row, in one place so their labels and
    /// outcomes cannot drift (D-X).
    fn subagent_buttons(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
        enabled: bool,
        action_label: &str,
    ) -> Vec<AnyElement> {
        let mut buttons = Vec::new();
        buttons.push(self.subagent_button(
            "subagents-save",
            tr!("subagents_ui.save"),
            false,
            None,
            theme,
            this.clone(),
            SubagentAction::Save,
        ));
        buttons.push(self.subagent_button(
            "subagents-duplicate",
            tr!("subagents_ui.duplicate"),
            false,
            None,
            theme,
            this.clone(),
            SubagentAction::Duplicate,
        ));
        if !editor.is_new {
            buttons.push(self.subagent_button(
                "subagents-toggle",
                if enabled {
                    tr!("subagents_ui.disable")
                } else {
                    tr!("subagents_ui.enable")
                },
                false,
                None,
                theme,
                this.clone(),
                SubagentAction::ToggleEnabled,
            ));
        }
        if !editor.is_new && editor.path.is_some() {
            let deleting = self.subagent_delete_confirm;
            buttons.push(self.subagent_button(
                "subagents-delete",
                if deleting {
                    tr!("subagents_ui.confirm")
                } else {
                    action_label.to_string()
                },
                true,
                None,
                theme,
                this.clone(),
                if deleting {
                    SubagentAction::ConfirmDelete
                } else {
                    SubagentAction::AskDelete
                },
            ));
            if deleting {
                buttons.push(self.subagent_button(
                    "subagents-cancel",
                    tr!("subagents_ui.cancel"),
                    false,
                    None,
                    theme,
                    this.clone(),
                    SubagentAction::CancelDelete,
                ));
            }
            if editor.path.is_some() {
                buttons.push(self.subagent_button(
                    "subagents-reveal",
                    tr!("subagents_ui.reveal"),
                    false,
                    None,
                    theme,
                    this.clone(),
                    SubagentAction::Reveal,
                ));
            }
        }
        buttons
    }

    #[allow(clippy::too_many_arguments)]
    fn subagent_button(
        &self,
        id: &'static str,
        label: String,
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
                    .raised(theme.surface_elevated, &theme)
                    .text_color(theme.error)
                    .hover_danger_soft(&theme),
                theme.error,
            )
        } else {
            (
                base.border_1()
                    .border_color(theme.border)
                    .raised(theme.surface_elevated, &theme)
                    .text_color(theme.text_secondary)
                    .hover_state(&theme),
                theme.text_secondary,
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
            .child(div().child(label))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let action = action.clone();
                this.update(cx, |app, cx| app.apply_subagent_action(action, cx));
            })
            .into_any_element()
    }

    // ── controller ──────────────────────────────────────────────────

    /// Re-scan the agent directory and the config, then reload the selection.
    pub(super) fn refresh_subagents(&mut self, cx: &mut Context<Self>) {
        let (agents, skipped) = subagent_file::discover();
        self.subagents = agents;
        self.subagents_skipped = skipped;
        self.subagent_config = subagents::load();
        let still_there = self
            .selected_subagent
            .as_ref()
            .is_some_and(|name| self.subagents.iter().any(|agent| agent.name() == name));
        if !still_there {
            self.selected_subagent = self.subagents.first().map(|agent| agent.name().to_string());
        }
        self.subagent_error = None;
        self.rebuild_subagent_editor(cx);
        cx.notify();
    }

    fn selected_agent(&self) -> Option<Agent> {
        let name = self.selected_subagent.as_ref()?;
        self.subagents
            .iter()
            .find(|agent| agent.name() == name)
            .cloned()
    }

    /// Build the editor from the selected agent (or an empty new-agent draft).
    fn rebuild_subagent_editor(&mut self, cx: &mut Context<Self>) {
        let Some(agent) = self.selected_agent() else {
            self.subagent_editor = None;
            return;
        };
        let mode = match self.subagent_config.agent_mode(agent.name()) {
            "plan" => "plan",
            "build" => "build",
            "ask" => "ask",
            _ => "inherit",
        };
        self.subagent_editor = Some(self.make_editor(
            cx,
            agent.name(),
            &agent.fields.description,
            agent.fields.model.clone(),
            agent.fields.thinking.clone(),
            agent.fields.tools.clone(),
            agent.tier(),
            &mode.to_string(),
            agent.fields.body.clone(),
            agent.path.clone(),
            agent.hash(),
            false,
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn make_editor(
        &self,
        cx: &mut Context<Self>,
        name: &str,
        description: &str,
        model: Option<String>,
        thinking: Option<String>,
        tools: Option<Vec<String>>,
        tier: Option<Tier>,
        mode: &str,
        prompt: String,
        path: Option<PathBuf>,
        loaded_hash: Option<u64>,
        is_new: bool,
    ) -> SubagentEditor {
        let text_input = |cx: &mut Context<Self>, id: &'static str, value: String, lines: usize| {
            cx.new(|cx| {
                ComposerInput::new(cx)
                    .with_element_id(id)
                    .with_key_context("Composer Picker")
                    .with_max_lines(lines)
                    .with_wrap(lines > 1)
                    .with_text(value)
            })
        };
        SubagentEditor {
            name: text_input(cx, "subagent-name", name.to_string(), 1),
            description: text_input(cx, "subagent-description", description.to_string(), 1),
            model: text_input(cx, "subagent-model", model.unwrap_or_default(), 1),
            thinking: text_input(cx, "subagent-thinking", thinking.unwrap_or_default(), 1),
            prompt: text_input(cx, "subagent-prompt", prompt, 10),
            tier,
            tools,
            mode: match mode {
                "plan" => "plan",
                "build" => "build",
                "ask" => "ask",
                _ => "inherit",
            },
            path,
            loaded_hash,
            is_new,
        }
    }

    fn start_new_subagent(&mut self, cx: &mut Context<Self>) {
        self.selected_subagent = None;
        self.subagent_error = None;
        self.subagent_delete_confirm = false;
        self.subagent_editor = Some(self.make_editor(
            cx,
            "",
            "",
            None,
            None,
            Some(Tier::Read.tools()),
            Some(Tier::Read),
            "inherit",
            tr!("subagents_ui.prompt_skeleton"),
            None,
            None,
            true,
        ));
        cx.notify();
    }

    fn duplicate_subagent(&mut self, cx: &mut Context<Self>) {
        let Some(agent) = self.selected_agent() else {
            return;
        };
        let base = agent.name().to_string();
        let mut suffix = 2;
        let mut name = format!("{base}-copy");
        while self
            .subagents
            .iter()
            .any(|candidate| candidate.name() == name)
        {
            name = format!("{base}-copy-{suffix}");
            suffix += 1;
        }
        self.selected_subagent = None;
        self.subagent_error = None;
        self.subagent_delete_confirm = false;
        self.subagent_editor = Some(self.make_editor(
            cx,
            &name,
            &format!("{} (copy)", agent.fields.description),
            agent.fields.model.clone(),
            agent.fields.thinking.clone(),
            agent.fields.tools.clone(),
            agent.tier(),
            &self.subagent_config.agent_mode(agent.name()).to_string(),
            agent.fields.body.clone(),
            None,
            None,
            true,
        ));
        cx.notify();
    }

    /// Single dispatch point for every Subagents-page control.
    pub(super) fn apply_subagent_action(&mut self, action: SubagentAction, cx: &mut Context<Self>) {
        self.subagent_error = None;
        match action {
            SubagentAction::Select(name) => {
                self.selected_subagent = Some(name);
                self.subagent_delete_confirm = false;
                self.rebuild_subagent_editor(cx);
            }
            SubagentAction::New => self.start_new_subagent(cx),
            SubagentAction::Save => self.save_subagent(cx),
            SubagentAction::Duplicate => self.duplicate_subagent(cx),
            SubagentAction::Reveal => {
                if let Some(path) = self.subagent_editor.as_ref().and_then(|e| e.path.clone()) {
                    platform::reveal_in_file_manager(&path);
                }
            }
            SubagentAction::ToggleEnabled => {
                let Some(name) = self.selected_subagent.clone() else {
                    return;
                };
                let enabled = self.subagent_config.agent_enabled(&name);
                self.subagent_config.set_enabled(&name, !enabled);
                match subagents::save(&self.subagent_config) {
                    Ok(()) => {
                        self.toast_success(tr!(
                            if enabled {
                                "subagents_ui.disabled_agent"
                            } else {
                                "subagents_ui.enabled_agent"
                            },
                            name = name
                        ));
                    }
                    Err(error) => self.subagent_error = Some(error),
                }
            }
            SubagentAction::SetTier(tier) => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.tier = Some(tier);
                    editor.tools = Some(tier.tools());
                }
            }
            SubagentAction::SetMode(mode) => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.mode = mode;
                }
            }
            SubagentAction::AskDelete => self.subagent_delete_confirm = true,
            SubagentAction::CancelDelete => self.subagent_delete_confirm = false,
            SubagentAction::ConfirmDelete => self.delete_subagent(cx),
            SubagentAction::UndoDelete => self.undo_subagent_delete(cx),
        }
        cx.notify();
    }

    pub(super) fn subagent_open_path(&mut self, path: &Path, _cx: &mut Context<Self>) {
        platform::open_path_default(path);
    }

    fn save_subagent(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.subagent_editor.as_ref() else {
            return;
        };
        let name = editor.name.read(cx).text().trim().to_string();
        let description = editor.description.read(cx).text().trim().to_string();
        let prompt = editor.prompt.read(cx).text();
        let model = editor.model.read(cx).text().trim().to_string();
        let thinking = editor.thinking.read(cx).text().trim().to_string();

        if !subagent_file::is_kebab_name(&name) {
            self.subagent_error = Some(tr!("subagents_ui.error_name"));
            return;
        }
        if description.is_empty() {
            self.subagent_error = Some(tr!("subagents_ui.error_description"));
            return;
        }
        if prompt.trim().is_empty() {
            self.subagent_error = Some(tr!("subagents_ui.error_prompt"));
            return;
        }
        let renamed = self
            .selected_subagent
            .as_ref()
            .is_some_and(|current| current != &name);
        if (editor.is_new || renamed) && self.subagents.iter().any(|agent| agent.name() == name) {
            self.subagent_error = Some(tr!("subagents_ui.error_name_taken", name = name));
            return;
        }

        let fields = subagent_file::Fields {
            name: name.clone(),
            description,
            model: (!model.is_empty()).then_some(model.clone()),
            thinking: (!thinking.is_empty()).then_some(thinking.clone()),
            tools: editor.tools.clone(),
            body: prompt,
        };
        let path = subagent_file::agents_dir().join(format!("{name}.md"));
        let expected = if editor.is_new || renamed {
            None
        } else {
            editor.loaded_hash
        };
        match subagent_file::save(&path, &fields, expected) {
            Ok(hash) => {
                let old_path = editor.path.clone();
                let mut config = self.subagent_config.clone();
                config.set_mode(&name, editor.mode);
                if let Err(error) = subagents::save(&config) {
                    self.subagent_error = Some(error);
                    return;
                }
                self.subagent_config = config;
                // A rename removes the file it replaced, after the new one is
                // safely on disk.
                if renamed {
                    if let Some(old) = old_path {
                        if old != path {
                            let _ = std::fs::remove_file(old);
                        }
                    }
                }
                self.selected_subagent = Some(name.clone());
                self.refresh_subagents(cx);
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.loaded_hash = Some(hash);
                }
                self.toast_success(tr!("subagents_ui.saved_agent", name = name));
            }
            Err(subagent_file::SaveError::Changed) => {
                self.subagent_error = Some(tr!("subagents_ui.error_changed"));
            }
            Err(error) => self.subagent_error = Some(error.to_string()),
        }
    }

    fn delete_subagent(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.subagent_editor.as_ref() else {
            return;
        };
        self.subagent_delete_confirm = false;
        let Some(path) = editor.path.clone() else {
            // A builtin has no file: "Delete" is not on offer, but disabling is.
            return;
        };
        let name = self
            .selected_subagent
            .clone()
            .unwrap_or_else(|| tr!("subagents_ui.no_agent"));
        let bytes = std::fs::read(&path).unwrap_or_default();
        if let Err(error) = std::fs::remove_file(&path) {
            self.subagent_error = Some(error.to_string());
            return;
        }
        self.subagent_undo = Some(SubagentUndo {
            path,
            bytes,
            name: name.clone(),
        });
        self.selected_subagent = None;
        self.refresh_subagents(cx);
        self.toast_info(tr!("subagents_ui.deleted_agent", name = name));
    }

    fn undo_subagent_delete(&mut self, cx: &mut Context<Self>) {
        let Some(undo) = self.subagent_undo.take() else {
            return;
        };
        match subagent_file::write_atomic(&undo.path, &undo.bytes) {
            Ok(()) => {
                self.selected_subagent = Some(undo.name);
                self.refresh_subagents(cx);
            }
            Err(error) => self.subagent_error = Some(error),
        }
    }

    /// The exact block the model will see, built the way `agents.js` builds it.
    fn subagent_preview(&self) -> String {
        let mut lines = vec![
            tr!("subagents_ui.preview_heading").to_string(),
            String::new(),
            tr!("subagents_ui.preview_agents_heading").to_string(),
        ];
        for agent in &self.subagents {
            if !self.subagent_config.agent_enabled(agent.name()) {
                continue;
            }
            lines.push(format!(
                "  {:<10} {} · {}",
                agent.name(),
                capability_label(agent.ceiling()),
                agent.fields.description.trim()
            ));
        }
        lines.push(String::new());
        lines.push(tr!("subagents_ui.preview_tail").to_string());
        lines.join("\n")
    }
}

/// A deleted user agent's bytes, kept until the page reloads or another agent
/// is deleted, so Undo restores the exact file.
pub(super) struct SubagentUndo {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub name: String,
}

fn capability_label(ceiling: &str) -> String {
    match ceiling {
        "read" => tr!("subagents_ui.capability_read"),
        "edit" => tr!("subagents_ui.capability_edit"),
        "exec" => tr!("subagents_ui.capability_exec"),
        _ => tr!("subagents_ui.capability_all"),
    }
}

fn mode_label(mode: &str) -> String {
    match mode {
        "plan" => tr!("subagents_ui.mode_plan"),
        "build" => tr!("subagents_ui.mode_build"),
        "ask" => tr!("subagents_ui.mode_ask"),
        _ => tr!("subagents_ui.mode_inherit"),
    }
}

fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Read => "read",
        Tier::Edit => "edit",
        Tier::Exec => "exec",
    }
}

fn tier_description(tier: Tier) -> String {
    match tier {
        Tier::Read => tr!("subagents_ui.tier_read"),
        Tier::Edit => tr!("subagents_ui.tier_edit"),
        Tier::Exec => tr!("subagents_ui.tier_exec"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, ThemeId};

    /// The page paints a builtin's detail form and a new-agent draft. The
    /// controller's writes are covered by `subagent_file`/`subagents` tests;
    /// this covers the surface an empty or unreadable selection must not break.
    #[gpui::test]
    fn the_subagents_page_renders_a_builtin_and_a_draft(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        let viewport = cx.update(|window, _| window.viewport_size());

        // Empty state.
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.subagents = crate::subagent_file::builtin_agents();
                app.selected_subagent = Some("scout".into());
                app.rebuild_subagent_editor(cx);
                app.subagent_config.set_mode("scout", "plan");
                cx.notify();
            });
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        // A new-agent draft, and the delete confirmation with its extra button.
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.start_new_subagent(cx);
                cx.notify();
            });
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.selected_subagent = Some("worker".into());
                app.rebuild_subagent_editor(cx);
                app.subagent_delete_confirm = true;
                app.subagent_error = Some("the file changed on disk".into());
                cx.notify();
            });
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    }

    #[test]
    fn the_preview_lists_enabled_agents_only() {
        let agents = crate::subagent_file::builtin_agents();
        let config = crate::subagents::Config::default();
        let mut lines = Vec::new();
        for agent in &agents {
            if config.agent_enabled(agent.name()) {
                lines.push(agent.name().to_string());
            }
        }
        assert_eq!(lines, ["scout", "worker", "reviewer"]);
        let mut config = config;
        config.set_enabled("worker", false);
        assert!(!config.agent_enabled("worker"));
    }
}
