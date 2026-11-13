//! Settings → Subagents: a master-detail manager for the agents the
//! `subagent` tool can pick.
//!
//! The left column lists builtins and user files. The right column is one
//! scrolling form under a pinned identity bar: name and description, the
//! tool list (presets plus per-tool toggles), the workflow mode, model and
//! thinking, the prompt, the rendered preview, the file it saves to, and the
//! delegation limits that apply to every agent. Discovery and the surgical
//! writer live in [`crate::subagent_file`]; the Orbit-only half (enabled,
//! mode, limits) lives in [`crate::subagents`]. Nothing here duplicates
//! either: a save re-reads the file, refuses to clobber an external edit, and
//! leaves every key it does not own byte-for-byte.

use super::helpers::*;
use super::*;
use crate::git_panel::widgets::check_box;
use crate::subagent_file::{self, Agent, Source, Tier};
use crate::subagents;
use crate::theme::tokens::{
    button, input, ButtonSize, DynamicSpacing, IconSize, Radius, TextSize, BORDER_WIDTH,
};

/// One Skills-page-style dispatch point for every Subagents control.
#[derive(Clone)]
pub(super) enum SubagentAction {
    Select(String),
    New,
    Save,
    Duplicate,
    Reveal,
    Open,
    ToggleEnabled,
    SetSubagentsEnabled(bool),
    /// One of the three preset tool lists; the file's unknown tools survive.
    SetToolsPreset(Tier),
    /// Materialize pi's default set so every tool can be toggled.
    CustomizeTools,
    /// Drop the `tools:` key: the child inherits pi's defaults.
    UseDefaultTools,
    ToggleTool(String),
    SetMode(&'static str),
    SetThinking(String),
    SetLimit(SubagentLimit, i32),
    AskDelete,
    ConfirmDelete,
    CancelDelete,
    UndoDelete,
    /// The banner's remedy: the other extension owns the name because it was
    /// installed from Settings → Plugins.
    OpenPlugins,
}

/// The limits `subagents.json` owns, in the order the page lists them.
#[derive(Clone, Copy)]
pub(super) enum SubagentLimit {
    Concurrent,
    Tasks,
    Children,
    Timeout,
}

/// The five inputs and the facts a save needs. Rebuilt on selection, so the
/// fields always reflect the file on disk rather than a stale clone.
pub(super) struct SubagentEditor {
    pub name: Entity<ComposerInput>,
    pub description: Entity<ComposerInput>,
    pub model: Entity<ComposerInput>,
    pub thinking: Entity<ComposerInput>,
    pub prompt: Entity<ComposerInput>,
    pub tools: Option<Vec<String>>,
    pub mode: &'static str,
    pub path: Option<PathBuf>,
    pub loaded_hash: Option<u64>,
    pub is_new: bool,
    /// The values as loaded, so the header can say what has changed.
    baseline: Baseline,
    /// Repaint as a field is typed: the unsaved marker and the live
    /// capability line read from the inputs.
    _field_subs: Vec<Subscription>,
}

struct Baseline {
    name: String,
    description: String,
    model: String,
    thinking: String,
    prompt: String,
    tools: Option<Vec<String>>,
    mode: &'static str,
}

impl SubagentEditor {
    /// Whether the form differs from the file it was loaded from. A new
    /// draft is not "changed" — it has never been saved at all.
    fn is_dirty(&self, cx: &Context<OrbitApp>) -> bool {
        if self.is_new {
            return false;
        }
        let same = |field: &Entity<ComposerInput>, value: &str| field.read(cx).text() == value;
        !(same(&self.name, &self.baseline.name)
            && same(&self.description, &self.baseline.description)
            && same(&self.model, &self.baseline.model)
            && same(&self.thinking, &self.baseline.thinking)
            && same(&self.prompt, &self.baseline.prompt)
            && self.tools == self.baseline.tools
            && self.mode == self.baseline.mode)
    }
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
            .flex_col()
            .bg(theme.background)
            .font_family(theme::ui_font_family())
            .when_some(
                self.subagent_inactive_banner(theme, this.clone()),
                |page, banner| page.child(banner),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .child(self.subagents_list(theme, this.clone(), cx))
                    .child(self.subagent_detail(theme, this, cx)),
            )
            .into_any_element()
    }

    /// The truth about the `subagent` name when this session does not use it.
    ///
    /// The extension reports this every time a session starts: without it the
    /// page would present a catalog the model never sees, which is the silent
    /// lie D-C exists to prevent. The agents stay editable — they are the files
    /// whichever extension owns the name reads.
    fn subagent_inactive_banner(&self, theme: Theme, this: Entity<OrbitApp>) -> Option<AnyElement> {
        let status = self
            .subagent_status
            .as_ref()
            .filter(|status| !status.active)?;
        let detail = match status.owner.as_deref() {
            Some(owner) if !owner.is_empty() => {
                tr!("subagents_ui.inactive_owner", owner = owner.to_string())
            }
            // No owner and no useful reason means the extension could not read
            // pi's tool list at all; say nothing rather than guess.
            _ => status.reason.clone(),
        };

        let mut column = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base02.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(tr!("subagents_ui.inactive_title")),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(tr!("subagents_ui.inactive_body")),
            );
        if !detail.trim().is_empty() {
            // Truncated like every other path in the workbench: the full value
            // is the tooltip, and the file is reachable from Plugins either way.
            column = column.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .truncate()
                    .tip(detail.clone())
                    .child(detail),
            );
        }

        Some(
            div()
                .id("subagents-inactive")
                .debug_selector(|| "subagents-inactive".to_string())
                .w_full()
                .mx(DynamicSpacing::Base16.px(&theme))
                .mt(DynamicSpacing::Base12.px(&theme))
                .bg(theme.surface_secondary)
                .border_1()
                .border_color(theme.border)
                .rounded(Radius::XLarge.px(&theme))
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base12.px(&theme))
                .flex()
                .items_start()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(icon(
                    "icons/info.svg",
                    IconSize::Medium.px(&theme),
                    theme.warning,
                ))
                .child(column)
                .child(self.subagent_button(
                    "subagents-inactive-plugins",
                    tr!("subagents_ui.open_plugins"),
                    SubagentButtonKind::Secondary,
                    theme,
                    this,
                    SubagentAction::OpenPlugins,
                ))
                .into_any_element(),
        )
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
        if !self.subagents_skipped.is_empty() {
            footer = footer.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.warning)
                    .child(icon(
                        "icons/alert-circle.svg",
                        IconSize::XSmall.px(&theme),
                        theme.warning,
                    ))
                    .child(tr!(
                        "subagents_ui.skipped",
                        count = self.subagents_skipped.len()
                    )),
            );
        }
        footer = footer.child(
            div()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_muted)
                .whitespace_normal()
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
        let meta_color = if selected {
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
                        if !enabled {
                            theme.text_disabled
                        } else if selected {
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
                            } else if enabled {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(meta_color)
                            .child(capability_label(ceiling))
                            .child("·")
                            .child(mode_label(mode))
                            .when_some(agent.fields.model.clone(), |row, model| {
                                row.child("·").child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(theme::code_font_family())
                                        .child(model),
                                )
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
            return self.subagent_empty_detail(theme, this, cx);
        };

        let selected = self
            .selected_subagent
            .as_ref()
            .and_then(|name| self.subagents.iter().find(|agent| agent.name() == name));
        let enabled = selected
            .map(|agent| self.subagent_config.agent_enabled(agent.name()))
            .unwrap_or(true);
        let name_now = editor.name.read(cx).text().trim().to_string();
        let builtin_unchanged =
            selected.is_some_and(|agent| agent.path.is_none() && agent.name() == name_now);
        let overrides_builtin = editor.path.is_some()
            && subagent_file::builtin_agents()
                .iter()
                .any(|builtin| builtin.name() == name_now);
        let dirty = editor.is_dirty(cx);

        let header = self.subagent_header(
            theme,
            this.clone(),
            editor,
            enabled,
            dirty,
            builtin_unchanged,
            overrides_builtin,
        );

        let mut column = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header);
        if let Some(error) = &self.subagent_error {
            column = column.child(
                div()
                    .px(DynamicSpacing::Base20.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.error.opacity(0.07))
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(icon(
                        "icons/alert-circle.svg",
                        IconSize::XSmall.px(&theme),
                        theme.error,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.error)
                            .whitespace_normal()
                            .child(SharedString::from(error.clone())),
                    ),
            );
        }

        let mut sections = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base20.px(&theme));
        sections = sections.child(self.subagent_identity_section(&theme, editor, cx));
        sections = sections.child(self.subagent_tools_section(&theme, this.clone(), editor));
        sections = sections.child(self.subagent_mode_section(&theme, this.clone(), editor));
        sections = sections.child(self.subagent_model_section(&theme, this.clone(), editor, cx));
        sections = sections.child(self.subagent_prompt_section(&theme, editor));
        sections = sections.child(self.subagent_preview_section(&theme));
        if let Some(file) = self.subagent_file_section(
            &theme,
            this.clone(),
            editor,
            selected,
            overrides_builtin,
            cx,
        ) {
            sections = sections.child(file);
        }
        sections = sections.child(self.subagent_limits_section(&theme, this.clone()));

        let body = div()
            .id("subagents-detail")
            .debug_selector(|| "subagents-detail".to_string())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .px(DynamicSpacing::Base20.px(&theme))
                    .pt(DynamicSpacing::Base16.px(&theme))
                    .pb(DynamicSpacing::Base32.px(&theme))
                    .child(sections),
            );
        column.child(body).into_any_element()
    }

    fn subagent_empty_detail(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let new_agent = self.subagent_primary_button(
            "subagents-empty-new",
            tr!("subagents_ui.new_agent"),
            theme,
            this,
            SubagentAction::New,
        );
        div()
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
            .child(div().mt(DynamicSpacing::Base08.px(&theme)).child(new_agent))
            .into_any_element()
    }

    /// The pinned identity and action bar. Save is the one filled control;
    /// delete never lives here.
    #[allow(clippy::too_many_arguments)]
    fn subagent_header(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
        enabled: bool,
        dirty: bool,
        builtin_unchanged: bool,
        overrides_builtin: bool,
    ) -> AnyElement {
        let (source_label, action_label) = if editor.is_new {
            (tr!("subagents_ui.new"), tr!("subagents_ui.create"))
        } else if editor.path.is_none() && builtin_unchanged {
            (
                tr!("subagents_ui.built_in"),
                tr!("subagents_ui.create_override"),
            )
        } else if editor.path.is_none() {
            (tr!("subagents_ui.new"), tr!("subagents_ui.create"))
        } else if overrides_builtin {
            (tr!("subagents_ui.modified"), tr!("subagents_ui.save"))
        } else {
            (tr!("subagents_ui.yours"), tr!("subagents_ui.save"))
        };
        let mode = mode_label(editor.mode);
        let ceiling = capability_label(subagent_file::ceiling_for_tools(editor.tools.as_deref()));
        let mut meta = vec![source_label, ceiling, mode];
        if !editor.is_new && !enabled {
            meta.push(tr!("subagents_ui.disabled"));
        }

        let switch_this = this.clone();
        let enabled_switch = switch("subagent-enabled", enabled, &theme, move |_, _, cx| {
            switch_this.update(cx, |app, cx| {
                app.apply_subagent_action(SubagentAction::ToggleEnabled, cx)
            });
        });

        let identity = div()
            .px(DynamicSpacing::Base20.px(&theme))
            .pt(DynamicSpacing::Base16.px(&theme))
            .pb(DynamicSpacing::Base12.px(&theme))
            // The app paints its own caption buttons over the window's
            // top-right corner on Windows (above every surface); keep the
            // switch out from under them without eating the normal inset.
            .when(platform::draws_window_controls(), |row| {
                row.pr(px(platform::WINDOW_CONTROLS_W))
            })
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .size(px(36.))
                    .flex_none()
                    .rounded(Radius::Large.px(&theme))
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/task.svg",
                        IconSize::Medium.px(&theme),
                        theme.text_secondary,
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
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(meta.join(" · ")),
                    ),
            )
            .when(!editor.is_new, |row| {
                row.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(if enabled {
                                    tr!("subagents_ui.enabled")
                                } else {
                                    tr!("subagents_ui.disabled_state")
                                }),
                        )
                        .child(enabled_switch),
                )
            });

        let dirty_marker: AnyElement = if editor.is_new {
            div()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("subagents_ui.not_saved"))
                .into_any_element()
        } else if dirty {
            div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.accent_text)
                .child(div().size(px(6.)).rounded_full().bg(theme.accent))
                .child(tr!("subagents_ui.unsaved"))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let actions = div()
            .px(DynamicSpacing::Base20.px(&theme))
            .pb(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(self.subagent_primary_button(
                "subagents-save",
                action_label.clone(),
                theme,
                this.clone(),
                SubagentAction::Save,
            ))
            .when(!editor.is_new, |row| {
                row.child(self.subagent_secondary_button(
                    "subagents-duplicate",
                    tr!("subagents_ui.duplicate"),
                    theme,
                    this.clone(),
                    SubagentAction::Duplicate,
                ))
            })
            .child(div().flex_1())
            .child(dirty_marker);

        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(identity)
            .child(actions)
            .child(div().h(BORDER_WIDTH).bg(theme.border))
            .into_any_element()
    }

    // ── sections ────────────────────────────────────────────────────

    fn subagent_identity_section(
        &self,
        theme: &Theme,
        editor: &SubagentEditor,
        cx: &Context<Self>,
    ) -> AnyElement {
        let name_text = editor.name.read(cx).text();
        let name_trimmed = name_text.trim();
        let name_valid = subagent_file::is_kebab_name(name_trimmed);
        let name_taken = name_valid
            && self.subagents.iter().any(|agent| {
                agent.name() == name_trimmed
                    && Some(agent.name()) != self.selected_subagent.as_deref()
            });
        let name_problem = !name_trimmed.is_empty() && (!name_valid || name_taken);
        let name_hint = if !name_trimmed.is_empty() && !name_valid {
            tr!("subagents_ui.error_name")
        } else if name_taken {
            tr!("subagents_ui.error_name_taken", name = name_trimmed)
        } else {
            tr!("subagents_ui.name_hint")
        };

        let description = editor.description.read(cx).text();
        let count = description.chars().count();
        let counter = div()
            .flex_none()
            .text_size(TextSize::XSmall.px(theme))
            .font(crate::usage::view::num_font())
            .text_color(if count > 200 {
                theme.warning
            } else {
                theme.text_muted
            })
            .child(tr!("subagents_ui.description_count", count = count));

        let rows = vec![
            self.subagent_field(
                theme,
                tr!("subagents_ui.name"),
                editor.name.clone(),
                name_hint,
                if name_problem {
                    theme.error
                } else {
                    theme.text_muted
                },
                None,
            ),
            self.subagent_field(
                theme,
                tr!("subagents_ui.description"),
                editor.description.clone(),
                tr!("subagents_ui.description_hint"),
                theme.text_muted,
                Some(counter.into_any_element()),
            ),
        ];
        self.settings_section(*theme, &tr!("subagents_ui.identity"), rows)
    }

    fn subagent_tools_section(
        &self,
        theme: &Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
    ) -> AnyElement {
        let tools = editor.tools.clone();
        let tier = tools.as_deref().and_then(Tier::from_tools);
        let description = match tools.as_deref() {
            None => tr!("subagents_ui.can_all"),
            Some([]) => tr!("subagents_ui.tools_empty"),
            Some(tools) => match subagent_file::ceiling_for_tools(Some(tools)) {
                "read" => tr!("subagents_ui.can_read"),
                "edit" => tr!("subagents_ui.can_edit"),
                "exec" => tr!("subagents_ui.can_exec"),
                _ => tr!("subagents_ui.can_all"),
            },
        };

        let mut presets = div()
            .px(DynamicSpacing::Base16.px(theme))
            .pt(DynamicSpacing::Base12.px(theme))
            .pb(DynamicSpacing::Base04.px(theme))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(TextSize::Small.px(theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(tr!("subagents_ui.preset")),
            );
        for option in [None, Some(Tier::Read), Some(Tier::Edit), Some(Tier::Exec)] {
            let active = match option {
                None => tools.is_none(),
                Some(preset) => tier == Some(preset),
            };
            presets = presets.child(self.subagent_preset_row(theme, this.clone(), option, active));
        }

        let known = known_tools();
        let mut tools_row = div()
            .px(DynamicSpacing::Base16.px(theme))
            .pt(DynamicSpacing::Base08.px(theme))
            .pb(DynamicSpacing::Base12.px(theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(tr!("subagents_ui.tools")),
            );
        match tools.as_deref() {
            Some(tools) => {
                let mut chips = div()
                    .flex()
                    .flex_wrap()
                    .gap(DynamicSpacing::Base04.px(theme));
                for tool in &known {
                    chips = chips.child(self.subagent_tool_chip(
                        *theme,
                        this.clone(),
                        (*tool).to_string(),
                        tools.iter().any(|declared| declared.as_str() == *tool),
                    ));
                }
                let extras: Vec<&String> = tools
                    .iter()
                    .filter(|tool| !known.iter().any(|known| known == &tool.as_str()))
                    .collect();
                for tool in &extras {
                    chips = chips.child(self.subagent_tool_chip(
                        *theme,
                        this.clone(),
                        (*tool).clone(),
                        true,
                    ));
                }
                tools_row = tools_row.child(chips);
                if !extras.is_empty() {
                    tools_row = tools_row.child(
                        div()
                            .text_size(TextSize::XSmall.px(theme))
                            .text_color(theme.text_muted)
                            .whitespace_normal()
                            .child(tr!("subagents_ui.tools_extra")),
                    );
                }
            }
            None => {
                tools_row = tools_row.child(self.subagent_secondary_button(
                    "subagents-customize-tools",
                    tr!("subagents_ui.customize_tools"),
                    *theme,
                    this.clone(),
                    SubagentAction::CustomizeTools,
                ));
            }
        }

        self.settings_section_desc(
            *theme,
            &tr!("subagents_ui.what_it_can_do"),
            Some(&description),
            vec![presets.into_any_element(), tools_row.into_any_element()],
        )
    }

    fn subagent_preset_row(
        &self,
        theme: &Theme,
        this: Entity<OrbitApp>,
        option: Option<Tier>,
        active: bool,
    ) -> AnyElement {
        let (label, detail): (String, AnyElement) = match option {
            Some(tier) => (
                tier_short_label(tier),
                div()
                    .font_family(theme::code_font_family())
                    .text_size(theme.code_px(11.))
                    .text_color(theme.text_muted)
                    .truncate()
                    .child(tier.tools().join(" "))
                    .into_any_element(),
            ),
            None => (
                tr!("subagents_ui.inherit"),
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(tr!("subagents_ui.tools_inherit_hint"))
                    .into_any_element(),
            ),
        };
        let action = option
            .map(SubagentAction::SetToolsPreset)
            .unwrap_or(SubagentAction::UseDefaultTools);
        div()
            .id(ElementId::Name(
                format!("subagent-preset-{}", preset_id(option)).into(),
            ))
            .px(DynamicSpacing::Base08.px(theme))
            .py(DynamicSpacing::Base06.px(theme))
            .rounded(Radius::Medium.px(theme))
            .cursor_pointer()
            .when(active, |row| row.bg(theme.surface_selected))
            .when(!active, |row| row.hover_state(theme))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let action = action.clone();
                this.update(cx, |app, cx| app.apply_subagent_action(action, cx));
            })
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .size(px(7.))
                    .mt(px(5.))
                    .flex_none()
                    .rounded_full()
                    .bg(if active {
                        theme.accent
                    } else {
                        theme.border_strong
                    }),
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
                            .text_size(TextSize::Small.px(theme))
                            .font_weight(if active {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if active {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(label),
                    )
                    .child(detail),
            )
            .into_any_element()
    }

    fn subagent_mode_section(
        &self,
        theme: &Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
    ) -> AnyElement {
        let modes = ["inherit", "plan", "build", "ask"];
        let active_hint = match editor.mode {
            "plan" => tr!("subagents_ui.mode_plan_hint"),
            "build" => tr!("subagents_ui.mode_build_hint"),
            "ask" => tr!("subagents_ui.mode_ask_hint"),
            _ => tr!("subagents_ui.mode_inherit_hint"),
        };
        let segments = div()
            .flex()
            .items_center()
            .children(modes.into_iter().enumerate().map(|(index, mode)| {
                let this = this.clone();
                let active = editor.mode == mode;
                let label = match mode {
                    "plan" => tr!("subagents_ui.mode_plan"),
                    "build" => tr!("subagents_ui.mode_build"),
                    "ask" => tr!("subagents_ui.mode_ask"),
                    _ => tr!("subagents_ui.inherit"),
                };
                segmented_segment(
                    div().id(("subagent-mode", index)),
                    theme,
                    ButtonSize::Medium,
                    SegmentPosition::at(index, modes.len()),
                )
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .border_color(theme.border)
                .when(active, |segment| {
                    segment
                        .raised_flat(theme.surface_selected)
                        .text_color(theme.text_primary)
                })
                .when(!active, |segment| {
                    segment
                        .raised_flat(theme.surface_elevated)
                        .text_color(theme.text_secondary)
                        .hover_state(theme)
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, |app, cx| {
                        app.apply_subagent_action(SubagentAction::SetMode(mode), cx)
                    });
                })
                .child(label)
            }));
        let row = div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(theme))
            .child(segments)
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(active_hint),
            )
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(tr!("subagents_ui.mode_rule")),
            );
        self.settings_section(
            *theme,
            &tr!("subagents_ui.mode"),
            vec![row.into_any_element()],
        )
    }

    fn subagent_model_section(
        &self,
        theme: &Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
        cx: &Context<Self>,
    ) -> AnyElement {
        let model_text = editor.model.read(cx).text().trim().to_string();
        // Index 0 inherits the session's model. A pin the catalog cannot
        // resolve keeps its own row, so an honest value is never rewritten as
        // "session model" by a stray click.
        let session_label = if self.model_label.is_empty() {
            tr!("subagents_ui.use_session_model")
        } else {
            tr!(
                "subagents_ui.session_model",
                model = self.model_label.clone()
            )
        };
        let mut options = vec![session_label];
        options.extend(self.available_models.iter().map(|model| {
            format!(
                "{} · {}",
                crate::providers::provider_display_name(&model.provider),
                model.name
            )
        }));
        let selected = if model_text.is_empty() {
            0
        } else {
            match self
                .available_models
                .iter()
                .position(|model| format!("{}/{}", model.provider, model.id) == model_text)
            {
                Some(ix) => ix + 1,
                None => {
                    options.push(model_text.clone());
                    options.len() - 1
                }
            }
        };
        let model_row = div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap(input::gap(theme))
            // Same stack as the identity fields: label, control, hint. A
            // model chip beside the hint would squeeze the hint to nothing in
            // the detail column at the minimum window size.
            .child(
                input_label(tr!("subagents_ui.model"), theme)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary),
            )
            .child(self.select_control(
                "subagent-model-select",
                SettingsSelect::SubagentModel,
                options[selected].clone(),
                options,
                selected,
                *theme,
                this.clone(),
                cx,
            ))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(tr!("subagents_ui.model_hint")),
            )
            .into_any_element();

        let current = editor.thinking.read(cx).text().trim().to_string();
        let levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
        let mut chips = div()
            .flex()
            .flex_wrap()
            .gap(DynamicSpacing::Base04.px(theme))
            .child(self.subagent_thinking_chip(
                theme,
                this.clone(),
                String::new(),
                current.is_empty(),
            ));
        for level in levels {
            chips = chips.child(self.subagent_thinking_chip(
                theme,
                this.clone(),
                level.to_string(),
                current == level,
            ));
        }
        if !current.is_empty() && !levels.contains(&current.as_str()) {
            chips = chips.child(self.subagent_thinking_chip(
                theme,
                this.clone(),
                current.clone(),
                true,
            ));
        }
        let thinking_trailing: Option<AnyElement> = if current.is_empty() {
            Some(
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .child(tr!(
                        "subagents_ui.session_thinking",
                        thinking = self.thinking_label.clone()
                    ))
                    .into_any_element(),
            )
        } else {
            None
        };
        let thinking_row = div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(tr!("subagents_ui.thinking")),
            )
            .child(chips)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TextSize::XSmall.px(theme))
                            .text_color(theme.text_muted)
                            .whitespace_normal()
                            .child(tr!("subagents_ui.thinking_hint")),
                    )
                    .children(thinking_trailing),
            );

        self.settings_section(
            *theme,
            &tr!("subagents_ui.model"),
            vec![model_row, thinking_row.into_any_element()],
        )
    }

    fn subagent_prompt_section(&self, theme: &Theme, editor: &SubagentEditor) -> AnyElement {
        let row = div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .flex()
            .flex_col()
            .gap(input::gap(theme))
            .child(
                input_field_frame(div(), theme)
                    .w_full()
                    .bg(theme.background)
                    .child(editor.prompt.clone()),
            )
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(tr!("subagents_ui.prompt_hint")),
            )
            .into_any_element();
        self.settings_section(*theme, &tr!("subagents_ui.prompt"), vec![row])
    }

    fn subagent_preview_section(&self, theme: &Theme) -> AnyElement {
        let row = div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(theme))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .whitespace_normal()
                    .child(tr!("subagents_ui.preview_hint")),
            )
            .child(
                div()
                    .w_full()
                    .rounded(Radius::Medium.px(theme))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.code_bg)
                    .p(DynamicSpacing::Base08.px(theme))
                    .font_family(theme::code_font_family())
                    .text_size(theme.code_px(11.))
                    .text_color(theme.text_secondary)
                    .whitespace_normal()
                    .child(SharedString::from(self.subagent_preview())),
            )
            .into_any_element();
        self.settings_section(*theme, &tr!("subagents_ui.what_the_model_sees"), vec![row])
    }

    fn subagent_file_section(
        &self,
        theme: &Theme,
        this: Entity<OrbitApp>,
        editor: &SubagentEditor,
        selected: Option<&Agent>,
        overrides_builtin: bool,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if editor.is_new {
            return None;
        }
        let Some(path) = editor.path.clone() else {
            // A builtin has no file; saving writes an override that takes
            // precedence over it, which the page never used to say.
            let target = subagent_file::agents_dir()
                .join(format!("{}.md", editor.name.read(cx).text().trim()));
            let row = div()
                .px(DynamicSpacing::Base16.px(theme))
                .py(DynamicSpacing::Base12.px(theme))
                .flex()
                .flex_col()
                .items_start()
                .gap(DynamicSpacing::Base08.px(theme))
                .child(
                    div()
                        .w_full()
                        .text_size(TextSize::Small.px(theme))
                        .text_color(theme.text_secondary)
                        .whitespace_normal()
                        .child(tr!(
                            "subagents_ui.builtin_note",
                            path = target.display().to_string()
                        )),
                )
                .child(self.subagent_secondary_button(
                    "subagents-reveal-folder",
                    tr!("subagents_ui.reveal_folder"),
                    *theme,
                    this,
                    SubagentAction::Reveal,
                ));
            return Some(self.settings_section(
                *theme,
                &tr!("subagents_ui.file"),
                vec![row.into_any_element()],
            ));
        };

        let mut info = div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base04.px(theme))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(theme))
                    .font_family(theme::code_font_family())
                    .text_color(theme.text_muted)
                    .truncate()
                    .child(SharedString::from(path.display().to_string())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .child(match selected.and_then(|agent| agent.modified) {
                        Some(modified) => tr!(
                            "subagents_ui.modified_ago",
                            time = sessions::relative_time(modified)
                        ),
                        None => String::new(),
                    })
                    .when(overrides_builtin, |row| {
                        row.child("·").child(tr!(
                            "subagents_ui.override_note",
                            name = editor.name.read(cx).text().trim()
                        ))
                    }),
            );

        let confirming = self.subagent_delete_confirm;
        let actions: AnyElement = if confirming {
            let hint = if overrides_builtin {
                tr!(
                    "subagents_ui.reset_confirm_hint",
                    path = path.display().to_string(),
                    name = self.selected_subagent.clone().unwrap_or_default()
                )
            } else {
                tr!(
                    "subagents_ui.delete_confirm_hint",
                    path = path.display().to_string()
                )
            };
            div()
                .px(DynamicSpacing::Base16.px(theme))
                .py(DynamicSpacing::Base12.px(theme))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base08.px(theme))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .text_size(TextSize::Small.px(theme))
                        .text_color(theme.error)
                        .whitespace_normal()
                        .child(hint),
                )
                .child(self.subagent_danger_button(
                    "subagents-delete",
                    tr!("subagents_ui.delete"),
                    *theme,
                    this.clone(),
                    SubagentAction::ConfirmDelete,
                ))
                .child(self.subagent_secondary_button(
                    "subagents-cancel",
                    tr!("subagents_ui.cancel"),
                    *theme,
                    this.clone(),
                    SubagentAction::CancelDelete,
                ))
                .into_any_element()
        } else {
            div()
                .px(DynamicSpacing::Base16.px(theme))
                .py(DynamicSpacing::Base12.px(theme))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base08.px(theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(theme))
                        .child(self.subagent_secondary_button(
                            "subagents-reveal",
                            tr!("subagents_ui.reveal"),
                            *theme,
                            this.clone(),
                            SubagentAction::Reveal,
                        ))
                        .child(self.subagent_link(
                            "subagents-open",
                            tr!("subagents_ui.open_in_editor"),
                            *theme,
                            this.clone(),
                            SubagentAction::Open,
                        )),
                )
                .child(div().ml_auto().child(if overrides_builtin {
                    self.subagent_danger_button(
                        "subagents-reset",
                        tr!("subagents_ui.reset_to_builtin"),
                        *theme,
                        this.clone(),
                        SubagentAction::AskDelete,
                    )
                } else {
                    self.subagent_danger_button(
                        "subagents-delete",
                        tr!("subagents_ui.delete"),
                        *theme,
                        this.clone(),
                        SubagentAction::AskDelete,
                    )
                }))
                .into_any_element()
        };

        info = info.child(actions);
        Some(self.settings_section(
            *theme,
            &tr!("subagents_ui.file"),
            vec![info.into_any_element()],
        ))
    }

    fn subagent_limits_section(&self, theme: &Theme, this: Entity<OrbitApp>) -> AnyElement {
        let config = &self.subagent_config;
        let enabled_this = this.clone();
        let enabled = config.enabled;
        let rows = vec![
            self.subagent_control_row(
                theme,
                tr!("subagents_ui.limit_enabled"),
                Some(tr!("subagents_ui.limit_enabled_hint")),
                switch(
                    "subagents-master-enabled",
                    enabled,
                    theme,
                    move |_, _, cx| {
                        enabled_this.update(cx, |app, cx| {
                            app.apply_subagent_action(
                                SubagentAction::SetSubagentsEnabled(!enabled),
                                cx,
                            )
                        });
                    },
                ),
            ),
            self.subagent_control_row(
                theme,
                tr!("subagents_ui.limit_concurrent"),
                Some(tr!("subagents_ui.limit_concurrent_hint")),
                self.subagent_stepper(
                    "subagent-limit-concurrent",
                    config.max_concurrent,
                    1,
                    subagents::HARD_MAX_CONCURRENT,
                    config.max_concurrent.to_string(),
                    *theme,
                    this.clone(),
                    SubagentLimit::Concurrent,
                ),
            ),
            self.subagent_control_row(
                theme,
                tr!("subagents_ui.limit_tasks"),
                Some(tr!("subagents_ui.limit_tasks_hint")),
                self.subagent_stepper(
                    "subagent-limit-tasks",
                    config.max_tasks_per_run,
                    1,
                    subagents::HARD_MAX_TASKS,
                    config.max_tasks_per_run.to_string(),
                    *theme,
                    this.clone(),
                    SubagentLimit::Tasks,
                ),
            ),
            self.subagent_control_row(
                theme,
                tr!("subagents_ui.limit_children"),
                Some(tr!("subagents_ui.limit_children_hint")),
                self.subagent_stepper(
                    "subagent-limit-children",
                    config.max_children_per_session,
                    1,
                    subagents::HARD_MAX_CHILDREN,
                    config.max_children_per_session.to_string(),
                    *theme,
                    this.clone(),
                    SubagentLimit::Children,
                ),
            ),
            self.subagent_control_row(
                theme,
                tr!("subagents_ui.limit_timeout"),
                Some(tr!("subagents_ui.limit_timeout_hint")),
                self.subagent_stepper(
                    "subagent-limit-timeout",
                    config.timeout_minutes,
                    5,
                    subagents::HARD_MAX_TIMEOUT_MINUTES,
                    tr!("subagents_ui.minutes", count = config.timeout_minutes),
                    *theme,
                    this.clone(),
                    SubagentLimit::Timeout,
                ),
            ),
        ];
        self.settings_section_desc(
            *theme,
            &tr!("subagents_ui.limits"),
            Some(&tr!("subagents_ui.limits_hint")),
            rows,
        )
    }

    // ── rows and controls ───────────────────────────────────────────

    /// A labelled input inside a settings board: label, field, then the hint
    /// line with an optional trailing fact (a counter, the inherited model).
    fn subagent_field(
        &self,
        theme: &Theme,
        label: String,
        input: Entity<ComposerInput>,
        hint: String,
        hint_color: Hsla,
        trailing: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
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
                    .child(input),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TextSize::XSmall.px(theme))
                            .text_color(hint_color)
                            .whitespace_normal()
                            .child(hint),
                    )
                    .children(trailing),
            )
            .into_any_element()
    }

    /// A title/description row with its control on the right, matching the
    /// settings pages' row anatomy.
    fn subagent_control_row(
        &self,
        theme: &Theme,
        title: String,
        desc: Option<String>,
        control: AnyElement,
    ) -> AnyElement {
        div()
            .w_full()
            .px(DynamicSpacing::Base16.px(theme))
            .py(DynamicSpacing::Base12.px(theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base16.px(theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base03.px(theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(title),
                    )
                    .children(desc.map(|desc| {
                        div()
                            .text_size(TextSize::XSmall.px(theme))
                            .text_color(theme.text_muted)
                            .whitespace_normal()
                            .child(desc)
                    })),
            )
            .child(div().flex_none().child(control))
            .into_any_element()
    }

    fn subagent_tool_chip(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        tool: String,
        on: bool,
    ) -> AnyElement {
        let id = format!("subagent-tool-{tool}");
        let toggle = tool.clone();
        button_frame(
            div().id(ElementId::Name(id.into())),
            &theme,
            ButtonSize::Default,
        )
        .group(BUTTON_GROUP)
        .border_1()
        .border_color(if on {
            theme.border_strong
        } else {
            theme.border
        })
        .raised(
            if on {
                theme.surface_selected
            } else {
                theme.surface_elevated
            },
            &theme,
        )
        .text_color(if on {
            theme.text_primary
        } else {
            theme.text_muted
        })
        .cursor_pointer()
        .hover_state(&theme)
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            let tool = toggle.clone();
            this.update(cx, |app, cx| {
                app.apply_subagent_action(SubagentAction::ToggleTool(tool), cx)
            });
        })
        .child(check_box(on, theme))
        .child(
            div()
                .font_family(theme::code_font_family())
                .text_size(theme.code_px(11.))
                .child(tool),
        )
        .into_any_element()
    }

    fn subagent_thinking_chip(
        &self,
        theme: &Theme,
        this: Entity<OrbitApp>,
        level: String,
        active: bool,
    ) -> AnyElement {
        let id = if level.is_empty() {
            "subagent-thinking-inherit".to_string()
        } else {
            format!("subagent-thinking-{level}")
        };
        let label = if level.is_empty() {
            tr!("subagents_ui.inherit")
        } else {
            crate::model_selector::thinking_display(&level)
        };
        let action = SubagentAction::SetThinking(level.clone());
        button_frame(
            div().id(ElementId::Name(id.into())),
            theme,
            ButtonSize::Default,
        )
        .group(BUTTON_GROUP)
        .border_1()
        .border_color(if active {
            theme.accent.opacity(0.5)
        } else {
            theme.border
        })
        .raised(
            if active {
                theme.surface_selected
            } else {
                theme.surface_elevated
            },
            theme,
        )
        .text_color(if active {
            theme.text_primary
        } else {
            theme.text_secondary
        })
        .cursor_pointer()
        .when(!active, |chip| chip.hover_state(theme))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| app.apply_subagent_action(action.clone(), cx));
        })
        .child(label)
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn subagent_stepper(
        &self,
        id: &'static str,
        value: u32,
        min: u32,
        max: u32,
        display: String,
        theme: Theme,
        this: Entity<OrbitApp>,
        limit: SubagentLimit,
    ) -> AnyElement {
        let minus = self.subagent_step_button(
            format!("{id}-minus"),
            false,
            value > min,
            theme,
            this.clone(),
            limit,
        );
        let plus =
            self.subagent_step_button(format!("{id}-plus"), true, value < max, theme, this, limit);
        div()
            .id(ElementId::Name(id.into()))
            .h(ButtonSize::Medium.height(&theme))
            .flex()
            .items_center()
            .border_1()
            .border_color(theme.border)
            .rounded(button::radius(ButtonSize::Medium).px(&theme))
            .raised(theme.surface_elevated, &theme)
            .child(minus)
            .child(
                div()
                    .w(px(54.))
                    .flex()
                    .justify_center()
                    .text_size(TextSize::Small.px(&theme))
                    .font(crate::usage::view::num_font())
                    .text_color(theme.text_primary)
                    .child(display),
            )
            .child(plus)
            .into_any_element()
    }

    fn subagent_step_button(
        &self,
        id: String,
        plus: bool,
        enabled: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
        limit: SubagentLimit,
    ) -> AnyElement {
        let glyph = if plus {
            "icons/plus.svg"
        } else {
            "icons/minus.svg"
        };
        let color = if enabled {
            theme.text_secondary
        } else {
            theme.text_disabled
        };
        let button = icon_button_frame(
            div().id(ElementId::Name(id.into())),
            &theme,
            ButtonSize::Medium,
        )
        .child(icon(
            glyph,
            ButtonSize::Medium.icon_size().px(&theme),
            color,
        ));
        if !enabled {
            return button.cursor_not_allowed().into_any_element();
        }
        button
            .cursor_pointer()
            .hover_overlay(&theme)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let delta = if plus { 1 } else { -1 };
                this.update(cx, |app, cx| {
                    app.apply_subagent_action(SubagentAction::SetLimit(limit, delta), cx)
                });
            })
            .into_any_element()
    }

    fn subagent_primary_button(
        &self,
        id: &'static str,
        label: String,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: SubagentAction,
    ) -> AnyElement {
        self.subagent_button(id, label, SubagentButtonKind::Primary, theme, this, action)
    }

    fn subagent_secondary_button(
        &self,
        id: &'static str,
        label: String,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: SubagentAction,
    ) -> AnyElement {
        self.subagent_button(
            id,
            label,
            SubagentButtonKind::Secondary,
            theme,
            this,
            action,
        )
    }

    fn subagent_danger_button(
        &self,
        id: &'static str,
        label: String,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: SubagentAction,
    ) -> AnyElement {
        self.subagent_button(id, label, SubagentButtonKind::Danger, theme, this, action)
    }

    fn subagent_button(
        &self,
        id: &'static str,
        label: String,
        kind: SubagentButtonKind,
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
        let button = match kind {
            SubagentButtonKind::Primary => base
                .border_1()
                .border_color(gpui::transparent_black())
                .raised(theme.action, &theme)
                .text_color(theme.action_fg)
                .hover_action(&theme)
                .pressed_state(&theme, PressedRamp::Action),
            SubagentButtonKind::Secondary => base
                .border_1()
                .border_color(theme.border)
                .raised(theme.surface_elevated, &theme)
                .text_color(theme.text_secondary)
                .hover_state(&theme),
            SubagentButtonKind::Danger => base
                .border_1()
                .border_color(theme.border)
                .raised(theme.surface_elevated, &theme)
                .text_color(theme.error)
                .hover_danger_soft(&theme),
        };
        press(button)
            .child(div().child(label))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let action = action.clone();
                this.update(cx, |app, cx| app.apply_subagent_action(action, cx));
            })
            .into_any_element()
    }

    fn subagent_link(
        &self,
        id: &'static str,
        label: String,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: SubagentAction,
    ) -> AnyElement {
        press(
            div()
                .id(ElementId::Name(id.into()))
                .flex_none()
                .cursor_pointer()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.accent_text)
                .hover_ink(&theme),
        )
        .child(label)
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
        // Read fresh too: the answer changes when the user installs or removes
        // the extension that owns the name, and the page must not go on
        // claiming the opposite.
        self.subagent_status = subagents::load_status();
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
            mode,
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
        mode: &'static str,
        prompt: String,
        path: Option<PathBuf>,
        loaded_hash: Option<u64>,
        is_new: bool,
    ) -> SubagentEditor {
        let text_input =
            |cx: &mut Context<Self>, id: &'static str, value: String, lines: usize, placeholder| {
                cx.new(|cx| {
                    ComposerInput::new(cx)
                        .with_element_id(id)
                        .with_key_context("Composer Picker")
                        .with_max_lines(lines)
                        .with_wrap(lines > 1)
                        .with_placeholder_key(placeholder)
                        .with_text(value)
                })
            };
        let model_value = model.unwrap_or_default();
        let thinking_value = thinking.unwrap_or_default();
        let name_input = text_input(
            cx,
            "subagent-name",
            name.to_string(),
            1,
            "subagents_ui.name_placeholder",
        );
        let description_input = text_input(
            cx,
            "subagent-description",
            description.to_string(),
            1,
            "subagents_ui.description_placeholder",
        );
        let model_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("subagent-model")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_placeholder_key("subagents_ui.model_placeholder")
                .with_text(model_value.clone())
        });
        let thinking_input = text_input(
            cx,
            "subagent-thinking",
            thinking_value.clone(),
            1,
            "subagents_ui.thinking_placeholder",
        );
        let prompt_input = text_input(
            cx,
            "subagent-prompt",
            prompt.clone(),
            10,
            "subagents_ui.prompt_placeholder",
        );

        let mut subs = Vec::new();
        for field in [
            &name_input,
            &description_input,
            &model_input,
            &thinking_input,
            &prompt_input,
        ] {
            subs.push(cx.observe(field, |_, _, cx| cx.notify()));
        }

        SubagentEditor {
            name: name_input,
            description: description_input,
            model: model_input,
            thinking: thinking_input,
            prompt: prompt_input,
            tools: tools.clone(),
            mode,
            path,
            loaded_hash,
            is_new,
            baseline: Baseline {
                name: name.to_string(),
                description: description.to_string(),
                model: model_value,
                thinking: thinking_value,
                prompt,
                tools,
                mode,
            },
            _field_subs: subs,
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
        let mode = match self.subagent_config.agent_mode(agent.name()) {
            "plan" => "plan",
            "build" => "build",
            "ask" => "ask",
            _ => "inherit",
        };
        self.subagent_editor = Some(self.make_editor(
            cx,
            &name,
            &format!("{} (copy)", agent.fields.description),
            agent.fields.model.clone(),
            agent.fields.thinking.clone(),
            agent.fields.tools.clone(),
            mode,
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
            SubagentAction::OpenPlugins => self.set_settings_section(SettingsSection::Plugins, cx),
            SubagentAction::New => self.start_new_subagent(cx),
            SubagentAction::Save => self.save_subagent(cx),
            SubagentAction::Duplicate => self.duplicate_subagent(cx),
            SubagentAction::Reveal => {
                let path = self
                    .subagent_editor
                    .as_ref()
                    .and_then(|editor| editor.path.clone())
                    .unwrap_or_else(subagent_file::agents_dir);
                platform::reveal_in_file_manager(&path);
            }
            SubagentAction::Open => {
                if let Some(path) = self.subagent_editor.as_ref().and_then(|e| e.path.clone()) {
                    platform::open_path_default(&path);
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
            SubagentAction::SetSubagentsEnabled(enabled) => {
                self.subagent_config.enabled = enabled;
                self.persist_subagents();
            }
            SubagentAction::SetToolsPreset(tier) => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.tools = Some(preset_tools(editor.tools.as_deref(), tier));
                }
            }
            SubagentAction::CustomizeTools => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.tools = Some(Tier::Exec.tools());
                }
            }
            SubagentAction::UseDefaultTools => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.tools = None;
                }
            }
            SubagentAction::ToggleTool(tool) => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    if let Some(tools) = editor.tools.as_mut() {
                        if let Some(index) = tools.iter().position(|declared| declared == &tool) {
                            tools.remove(index);
                        } else {
                            tools.push(tool);
                        }
                    }
                }
            }
            SubagentAction::SetMode(mode) => {
                if let Some(editor) = self.subagent_editor.as_mut() {
                    editor.mode = mode;
                }
            }
            SubagentAction::SetThinking(level) => {
                if let Some(input) = self.subagent_editor.as_ref().map(|e| e.thinking.clone()) {
                    input.update(cx, |input, cx| input.set_text(level, cx));
                }
            }
            SubagentAction::SetLimit(limit, delta) => {
                let step = |value: u32, min: u32, max: u32, step: u32| {
                    let next = value as i64 + delta as i64 * step as i64;
                    next.clamp(min as i64, max as i64) as u32
                };
                let config = &mut self.subagent_config;
                match limit {
                    SubagentLimit::Concurrent => {
                        config.max_concurrent =
                            step(config.max_concurrent, 1, subagents::HARD_MAX_CONCURRENT, 1);
                    }
                    SubagentLimit::Tasks => {
                        config.max_tasks_per_run =
                            step(config.max_tasks_per_run, 1, subagents::HARD_MAX_TASKS, 1);
                    }
                    SubagentLimit::Children => {
                        config.max_children_per_session = step(
                            config.max_children_per_session,
                            1,
                            subagents::HARD_MAX_CHILDREN,
                            1,
                        );
                    }
                    SubagentLimit::Timeout => {
                        config.timeout_minutes = step(
                            config.timeout_minutes,
                            5,
                            subagents::HARD_MAX_TIMEOUT_MINUTES,
                            5,
                        );
                    }
                }
                self.persist_subagents();
            }
            SubagentAction::AskDelete => self.subagent_delete_confirm = true,
            SubagentAction::CancelDelete => self.subagent_delete_confirm = false,
            SubagentAction::ConfirmDelete => self.delete_subagent(cx),
            SubagentAction::UndoDelete => self.undo_subagent_delete(cx),
        }
        cx.notify();
    }

    fn persist_subagents(&mut self) {
        if let Err(error) = subagents::save(&self.subagent_config) {
            self.subagent_error = Some(error);
        }
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
                if renamed {
                    // Settings follow the agent to its new name; the old entry
                    // would otherwise linger with no file to match it.
                    if let Some(old_name) = self.selected_subagent.as_ref() {
                        config.rename_agent(old_name, &name);
                    }
                }
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
        // Undo restores these bytes, so a read failure must abort the delete:
        // deleting first would leave Undo writing an empty file over the agent.
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.subagent_error = Some(error.to_string());
                return;
            }
        };
        if let Err(error) = std::fs::remove_file(&path) {
            self.subagent_error = Some(error.to_string());
            return;
        }
        self.subagent_undo = Some(SubagentUndo {
            path,
            bytes,
            name: name.clone(),
        });
        // The file is gone, so its settings entry is dead weight: a disabled
        // flag left behind would silently apply to a new agent of that name.
        self.subagent_config.forget_agent(&name);
        self.persist_subagents();
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

    /// The tool description the model reads, mirroring `buildDescription` in
    /// `agents.js`: the same heading, agent lines, capability caveat, mode
    /// syntax, and closing advice.
    fn subagent_preview(&self) -> String {
        let mut lines = vec![
            tr!("subagents_ui.preview_heading"),
            String::new(),
            tr!("subagents_ui.preview_agents_heading"),
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
        lines.push(tr!("subagents_ui.preview_capability_note"));
        lines.push(tr!("subagents_ui.preview_modes"));
        lines.push(tr!("subagents_ui.preview_tail"));
        lines.push(String::new());
        lines.push(tr!("subagents_ui.preview_skip"));
        lines.join("\n")
    }
}

/// The three button weights the Subagents page uses.
#[derive(Clone, Copy)]
enum SubagentButtonKind {
    Primary,
    Secondary,
    Danger,
}

/// A deleted user agent's bytes, kept until the page reloads or another agent
/// is deleted, so Undo restores the exact file.
pub(super) struct SubagentUndo {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub name: String,
}

/// The tools the page can draw a checkbox for. The list mirrors
/// `subagent_file::Tier::tools` plus the platform's command tool; anything
/// else a file declares is shown as-is and preserved.
fn known_tools() -> Vec<&'static str> {
    let mut tools = vec!["read", "grep", "find", "ls", "edit", "write", "bash"];
    if cfg!(windows) {
        tools.push("powershell");
    }
    tools
}

fn is_known_tool(tool: &str) -> bool {
    known_tools().contains(&tool)
}

/// A preset's tool list plus anything the file declares that the page does
/// not know (an MCP tool, for example). A preset click must never silently
/// drop a capability.
fn preset_tools(current: Option<&[String]>, tier: Tier) -> Vec<String> {
    let extras: Vec<String> = current
        .unwrap_or(&[])
        .iter()
        .filter(|tool| !is_known_tool(tool))
        .cloned()
        .collect();
    let mut tools = tier.tools();
    for extra in extras {
        if !tools.contains(&extra) {
            tools.push(extra);
        }
    }
    tools
}

fn preset_id(option: Option<Tier>) -> &'static str {
    match option {
        Some(Tier::Read) => "read",
        Some(Tier::Edit) => "edit",
        Some(Tier::Exec) => "exec",
        None => "inherit",
    }
}

fn tier_short_label(tier: Tier) -> String {
    match tier {
        Tier::Read => tr!("subagents_ui.preset_read"),
        Tier::Edit => tr!("subagents_ui.preset_edit"),
        Tier::Exec => tr!("subagents_ui.preset_exec"),
    }
}

/// The capability sentence the model reads. `agents.js` prints `runs commands`
/// for every ceiling above `edit`, including `all` (a tool the agent's file
/// declares but Orbit does not know, such as an MCP tool), so the page says the
/// same thing the delegation prompt says.
fn capability_label(ceiling: &str) -> String {
    match ceiling {
        "read" => tr!("subagents_ui.capability_read"),
        "edit" => tr!("subagents_ui.capability_edit"),
        _ => tr!("subagents_ui.capability_exec"),
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
        // The page lives inside Settings; open it at the minimum window size
        // so the master-detail columns are actually exercised.
        let viewport = gpui::size(px(960.), px(640.));
        cx.update(|_, cx| {
            app.update(cx, |app, _| {
                app.settings_open = true;
                app.settings_section = SettingsSection::Subagents;
            });
        });

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
        let detail = cx
            .debug_bounds("subagents-detail")
            .expect("the detail column rendered");
        assert!(detail.size.width > px(0.), "{detail:?}");

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

    /// The page must not present a catalog the model never sees (D-C). The
    /// extension reports who owns `subagent` at every session start; this is
    /// that report reaching the surface, and it must vanish again when a later
    /// session owns the name.
    #[gpui::test]
    fn the_page_says_when_another_extension_owns_the_name(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        let viewport = gpui::size(px(960.), px(640.));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::Subagents;
                app.subagents = crate::subagent_file::builtin_agents();
                app.selected_subagent = Some("scout".into());
                app.rebuild_subagent_editor(cx);
            });
        });

        // No report yet: absence is not a verdict, so nothing is claimed.
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("subagents-inactive").is_none());

        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.subagent_status = Some(crate::subagents::Status {
                    active: false,
                    reason: "owned by another extension".into(),
                    owner: Some("/home/me/.pi/agent/extensions/pi-subagents/index.js".into()),
                });
                cx.notify();
            });
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
        let banner = cx
            .debug_bounds("subagents-inactive")
            .expect("the stand-down banner rendered");
        assert!(banner.size.width > px(0.), "{banner:?}");
        // The agents stay editable: they are the files the owner reads.
        assert!(
            cx.debug_bounds("subagents-detail").is_some(),
            "the agent form still renders under the banner"
        );

        // A later session that owns the name clears the claim.
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.subagent_status = Some(crate::subagents::Status {
                    active: true,
                    reason: "available".into(),
                    owner: None,
                });
                cx.notify();
            });
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("subagents-inactive").is_none());
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

    /// The panel promises the description the model reads, so it must carry the
    /// caveats `agents.js` wraps around the agent list, not just the list.
    #[gpui::test]
    fn the_preview_carries_every_paragraph_the_model_reads(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        let preview = cx.update(|_, cx| {
            app.update(cx, |app, _| {
                app.subagents = crate::subagent_file::builtin_agents();
                app.subagent_config = crate::subagents::Config::default();
                app.subagent_preview()
            })
        });
        assert!(
            preview.starts_with(&tr!("subagents_ui.preview_heading")),
            "{preview}"
        );
        assert!(preview.contains("scout"), "{preview}");
        for key in [
            "subagents_ui.preview_capability_note",
            "subagents_ui.preview_modes",
            "subagents_ui.preview_tail",
            "subagents_ui.preview_skip",
        ] {
            assert!(
                preview.contains(&tr!(key)),
                "the preview is missing {key}:\n{preview}"
            );
        }
    }

    /// A preset owns the tools the page knows; an MCP tool the file declares
    /// must survive the click.
    #[test]
    fn presets_keep_unknown_declared_tools() {
        let current = vec!["mcp__github__create_pr".to_string(), "bash".to_string()];
        let tools = preset_tools(Some(&current), Tier::Read);
        assert_eq!(
            tools,
            vec!["read", "grep", "find", "ls", "mcp__github__create_pr"]
        );
        assert_eq!(Tier::from_tools(&tools), None);
        // A known tool is the preset's business: Read takes bash away.
        assert!(!tools.iter().any(|tool| tool == "bash"));
    }

    /// The model control writes the file's `provider/id`, and its first row
    /// clears the pin. A value the catalog cannot resolve stays clickable but
    /// inert, so an unavailable pin is never silently rewritten.
    #[gpui::test]
    fn the_model_select_pins_and_clears_the_agent_model(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::Subagents;
                app.available_models = vec![ModelEntry {
                    id: "scout-model".into(),
                    name: "Scout Model".into(),
                    provider: "orbit".into(),
                    context_window: None,
                    thinking_levels: vec!["off".into()],
                }];
                app.subagents = crate::subagent_file::builtin_agents();
                app.selected_subagent = Some("scout".into());
                app.rebuild_subagent_editor(cx);

                let model = app.subagent_editor.as_ref().unwrap().model.clone();
                let current = |app: &OrbitApp, cx: &gpui::App| {
                    app.subagent_editor
                        .as_ref()
                        .unwrap()
                        .model
                        .read(cx)
                        .text()
                        .to_string()
                };
                assert_eq!(current(app, cx), "", "a builtin inherits until pinned");

                // Row 0 is the session's model; row 1 is the only catalog entry.
                assert!(app.apply_settings_select(SettingsSelect::SubagentModel, 1, cx));
                assert_eq!(current(app, cx), "orbit/scout-model");
                assert!(app.subagent_editor.as_ref().unwrap().is_dirty(cx));

                assert!(app.apply_settings_select(SettingsSelect::SubagentModel, 0, cx));
                assert_eq!(current(app, cx), "");
                assert!(!app.subagent_editor.as_ref().unwrap().is_dirty(cx));

                // A pin outside this machine's catalog is its own row; clicking
                // it must not clear the value.
                model.update(cx, |input, cx| input.set_text("elsewhere/missing", cx));
                assert!(app.apply_settings_select(SettingsSelect::SubagentModel, 2, cx));
                assert_eq!(current(app, cx), "elsewhere/missing");
            });
        });
    }
}
