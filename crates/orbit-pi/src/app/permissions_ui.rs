//! Settings → Permissions: one row per tool family, four states each.
//!
//! The page is deliberately flat, because the alternative — an editor with its
//! own scope, family, pattern and action fields — made the simple question
//! ("what may commands do?") read like a form to fill in. Every row here shows:
//!
//! - what is **set** for this scope: `Inherit` (no rule), or `Allow` / `Ask` /
//!   `Deny` as an explicit rule;
//! - what that resolves to, right-aligned as `→ Ask` — the same answer the
//!   guard extension gives, because `Rules::effective_action` mirrors its
//!   precedence;
//! - any **pattern rules** below it (`rm *` → Deny), indented, each removable.
//!
//! The scope chips at the top switch the whole page between every session, main
//! sessions only, and one subagent — that is the per-agent half, without a
//! second editor. Nothing here decides anything: the file written by
//! [`crate::permissions`] is what the guard enforces on the next tool call.
//!
//! Under **Full access** the board is replaced by a single statement, because
//! that mode runs before the rule table: nothing asks and nothing is refused
//! there, so showing editable rules would imply an effect they do not have.
//! Their stored rules survive untouched for when the mode is confined again.

use super::helpers::*;
use super::*;
use crate::permissions::{Action, Rule, Rules, Scope, Target};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, Radius, TextSize};

/// The inline "add a pattern" form for one family. Only one is open at a time.
pub(super) struct PatternEditor {
    pub(super) target: Target,
    pub(super) action: Action,
    pub(super) pattern: Entity<ComposerInput>,
}

impl OrbitApp {
    // ── state ───────────────────────────────────────────────────────

    /// Re-read the rules and the agents a scope can name. Called when the page
    /// opens, so a hand-edited file or a new agent shows up without a restart.
    pub(super) fn refresh_permissions(&mut self, cx: &mut Context<Self>) {
        self.permission_rules = Rules::load();
        self.permission_agents = crate::subagent_file::discover()
            .0
            .iter()
            .map(|agent| agent.name().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        self.permission_agents.sort();
        self.permission_agents.dedup();
        // A scope whose agent file was deleted would hide every rule silently;
        // fall back to the scope that always exists.
        if let Scope::Agent(name) = &self.permission_scope {
            if !self.permission_agents.contains(name) {
                self.permission_scope = Scope::Any;
            }
        }
        self.pattern_editor = None;
        cx.notify();
    }

    /// Every scope the page offers, in chip order.
    pub(super) fn scope_choices(&self) -> Vec<Scope> {
        let mut choices = vec![Scope::Any, Scope::Session];
        choices.extend(
            self.permission_agents
                .iter()
                .map(|name| Scope::Agent(name.clone())),
        );
        choices
    }

    /// A scope's chip label. Agent chips are just the agent's name: the row is
    /// already labelled "Applies to", so a prefix would only add noise.
    pub(super) fn scope_label(scope: &Scope) -> String {
        match scope {
            Scope::Any => tr!("permissions.scope.any"),
            Scope::Session => tr!("permissions.scope.session"),
            Scope::Agent(name) => name.clone(),
        }
    }

    pub(super) fn set_permission_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.permission_scope = scope;
        // The pattern form belongs to the scope it was opened in.
        self.pattern_editor = None;
        cx.notify();
    }

    /// Set one family's action in the current scope, or inherit again.
    ///
    /// `None` removes the rule rather than storing the mode's default: a file
    /// that only holds decisions a person made keeps answering "why is this
    /// allowed?" honestly.
    pub(super) fn set_family_action(
        &mut self,
        target: Target,
        action: Option<Action>,
        cx: &mut Context<Self>,
    ) {
        let scope = self.permission_scope.clone();
        self.permission_rules
            .set_family_action(&scope, target, action);
        self.persist_rules(cx);
    }

    pub(super) fn open_pattern_editor(&mut self, target: Target, cx: &mut Context<Self>) {
        let pattern = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("permission-pattern-input")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder_key("permissions.pattern_placeholder")
        });
        self.pattern_editor = Some(PatternEditor {
            target,
            action: Action::Deny,
            pattern,
        });
        cx.notify();
    }

    pub(super) fn cancel_pattern_editor(&mut self, cx: &mut Context<Self>) {
        self.pattern_editor = None;
        cx.notify();
    }

    pub(super) fn set_pattern_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Some(editor) = self.pattern_editor.as_mut() {
            editor.action = action;
            cx.notify();
        }
    }

    /// Add the pending pattern rule to the current scope.
    pub(super) fn add_pattern(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.pattern_editor.as_ref() else {
            return;
        };
        // A pattern mid-composition still holds the preedit; adding waits for
        // the IME rather than storing half a glob.
        if crate::composer::any_composing(cx, &[&editor.pattern]) {
            return;
        }
        let pattern = editor.pattern.read(cx).text();
        if pattern.trim().is_empty() {
            // "Every call" is the family row above this one, not a pattern.
            return;
        }
        let rule = Rule::new(
            self.permission_scope.clone(),
            editor.target,
            &pattern,
            editor.action,
        );
        if !self.permission_rules.upsert(rule) {
            self.toast_warning(tr!(
                "permissions.rule_limit",
                limit = crate::permissions::MAX_RULES
            ));
            return;
        }
        self.pattern_editor = None;
        self.persist_rules(cx);
    }

    pub(super) fn remove_pattern(&mut self, target: Target, pattern: &str, cx: &mut Context<Self>) {
        let scope = self.permission_scope.clone();
        if self
            .permission_rules
            .remove_pattern(&scope, target, pattern)
        {
            self.persist_rules(cx);
        }
    }

    /// Write the file the guard reads. A failure puts the page back on what is
    /// actually on disk: a rule the guard cannot read is not a rule.
    fn persist_rules(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.permission_rules.save() {
            self.permission_rules = Rules::load();
            self.toast_error(tr!("permissions.save_failed", error = error));
        }
        cx.notify();
    }

    /// Handle the baseline dropdown. Returns whether `kind` belongs here.
    pub(super) fn apply_permission_select(
        &mut self,
        kind: SettingsSelect,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        match kind {
            SettingsSelect::AccessMode => {
                if let Some(mode) = AccessMode::ALL.get(ix).copied() {
                    self.set_access_mode(mode, cx);
                }
                true
            }
            _ => false,
        }
    }

    // ── page ────────────────────────────────────────────────────────

    pub(super) fn permissions_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        vec![
            self.permissions_baseline(theme, this.clone(), cx),
            // Full access is answered before any rule, so there is nothing to
            // configure here: the board would only imply an effect it lacks.
            if self.access_mode == AccessMode::FullAccess {
                self.full_access_statement(theme)
            } else {
                self.permissions_tools(theme, this)
            },
        ]
    }

    /// What Full access means, in place of the rule board.
    fn full_access_statement(&self, theme: Theme) -> AnyElement {
        self.settings_section_desc(
            theme,
            &tr!("permissions.full_access"),
            Some(&tr!("permissions.full_access_description")),
            vec![self.setting_row(
                theme,
                &tr!("permissions.full_access_row"),
                Some(&tr!("permissions.full_access_row_description")),
                None,
                None,
            )],
        )
    }

    /// The mode every `Inherit` falls back to, with the same three choices the
    /// composer chip offers.
    fn permissions_baseline(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let selected = AccessMode::ALL
            .iter()
            .position(|mode| *mode == self.access_mode)
            .unwrap_or(0);
        let control = self.select_control(
            "permissions-access-mode",
            SettingsSelect::AccessMode,
            self.access_mode.label(),
            AccessMode::ALL.iter().map(|mode| mode.label()).collect(),
            selected,
            theme,
            this,
            cx,
        );
        self.settings_section_desc(
            theme,
            &tr!("permissions.baseline"),
            Some(&tr!("permissions.baseline_description")),
            vec![self.setting_row(
                theme,
                &tr!("permissions.access_mode"),
                Some(&self.access_mode.description()),
                None,
                Some(control),
            )],
        )
    }

    /// The scope chips and one row per tool family, each with the four-state
    /// control, its resolved value, and its pattern rules.
    fn permissions_tools(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let mut rows = vec![self.scope_row(theme, this.clone())];
        if let Scope::Agent(name) = &self.permission_scope {
            rows.push(note_row(
                &tr!("permissions.agent_note", name = name.as_str()),
                theme.warning,
                theme,
            ));
        }
        for target in Target::FAMILIES {
            rows.push(self.family_row(target, theme, this.clone()));
            for rule in self
                .permission_rules
                .pattern_rules(&self.permission_scope, target)
            {
                rows.push(self.pattern_row(target, rule, theme, this.clone()));
            }
            if self
                .pattern_editor
                .as_ref()
                .is_some_and(|editor| editor.target == target)
            {
                rows.push(self.pattern_editor_row(theme, this.clone()));
            }
        }
        self.settings_section_desc(
            theme,
            &tr!("permissions.tools"),
            Some(&tr!("permissions.tools_description")),
            rows,
        )
    }

    /// Which scope the page is editing. One chip per choice; the selected chip
    /// carries the fill and `text_primary`.
    fn scope_row(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let chips = self
            .scope_choices()
            .into_iter()
            .enumerate()
            .map(|(ix, scope)| {
                let selected = scope == self.permission_scope;
                let label = Self::scope_label(&scope);
                let click_scope = scope.clone();
                let this = this.clone();
                button_frame(
                    div()
                        .id(ElementId::Name(format!("permissions-scope-{ix}").into()))
                        .debug_selector(move || format!("permissions-scope-{ix}")),
                    &theme,
                    ButtonSize::Medium,
                )
                .rounded(Radius::Medium.px(&theme))
                .border_1()
                .border_color(if selected {
                    theme.border_strong
                } else {
                    theme.border
                })
                .when(selected, |chip| chip.bg(theme.surface_active))
                .text_color(if selected {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .cursor_pointer()
                .when(!selected, |chip| chip.hover_state(&theme))
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let scope = click_scope.clone();
                    this.update(cx, |app, cx| app.set_permission_scope(scope, cx));
                })
                .child(label)
            });
        row_frame(theme)
            .child(row_label(&tr!("permissions.applies_to"), theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .children(chips),
            )
            .into_any_element()
    }

    /// One family: what is set, what it resolves to, and the control.
    fn family_row(&self, target: Target, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let effective = self.permission_rules.effective_action(
            &self.permission_scope,
            target,
            self.access_mode,
        );
        let stored = self
            .permission_rules
            .family_rule(&self.permission_scope, target);
        let patterns = self
            .permission_rules
            .pattern_rules(&self.permission_scope, target)
            .len();

        let control = action_segments(
            format!("permissions-family-{}", target.as_wire()),
            stored,
            true,
            theme,
            {
                let this = this.clone();
                move |choice, cx| {
                    this.update(cx, |app, cx| app.set_family_action(target, choice, cx));
                }
            },
        );

        // The form replaces the button, so there is never a dead control while
        // a pattern is being typed.
        let open = self
            .pattern_editor
            .as_ref()
            .is_some_and(|editor| editor.target == target);
        let add_pattern = (!open).then(|| {
            let this = this.clone();
            ghost_button(
                format!("permissions-add-pattern-{}", target.as_wire()),
                &tr!("permissions.add_pattern"),
                theme,
            )
            .text_color(theme.text_muted)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| app.open_pattern_editor(target, cx));
            })
            .into_any_element()
        });

        row_frame(theme)
            .debug_selector(move || format!("permissions-family-{}", target.as_wire()))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(row_label(&target.label(), theme))
                    .child(
                        // The tool list is context, not content: it truncates
                        // rather than pushing the row's answer off the edge.
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(target.hint()),
                    )
                    .when(patterns > 0, |line| {
                        line.child(
                            div()
                                .flex_none()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(tr!("permissions.pattern_count", count = patterns)),
                        )
                    })
                    .child(effective_label(effective, theme)),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(control)
                    .children(add_pattern),
            )
            .into_any_element()
    }

    /// One stored pattern rule: the glob, what it resolves to, and the three
    /// states (a pattern without an explicit action would be pointless).
    fn pattern_row(
        &self,
        target: Target,
        rule: &Rule,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let pattern = rule.pattern.clone();
        let control = action_segments(
            format!("permissions-pattern-{}", rule.pattern),
            Some(rule.action),
            false,
            theme,
            {
                let this = this.clone();
                let pattern = pattern.clone();
                // A pattern rule always has an action; "inherit" is not one of
                // its states, so a click stores the choice.
                move |choice, cx| {
                    if let Some(action) = choice {
                        this.update(cx, |app, cx| {
                            app.set_pattern_action_of(target, &pattern, action, cx)
                        });
                    }
                }
            },
        );
        let remove = {
            let this = this.clone();
            let pattern = pattern.clone();
            ghost_button(
                format!("permissions-remove-pattern-{}", rule.pattern),
                &tr!("permissions.remove"),
                theme,
            )
            .text_color(theme.text_muted)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let pattern = pattern.clone();
                this.update(cx, |app, cx| app.remove_pattern(target, &pattern, cx));
            })
        };
        div()
            .w_full()
            .pl(DynamicSpacing::Base32.px(&theme))
            .pr(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme::code_font_family())
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_primary)
                            .child(format!("\"{}\"", rule.pattern)),
                    )
                    .child(div().flex_1())
                    .child(effective_label(rule.action, theme)),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(control)
                    .child(remove),
            )
            .into_any_element()
    }

    /// The inline form that adds one pattern to one family.
    fn pattern_editor_row(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let Some(editor) = self.pattern_editor.as_ref() else {
            return div().into_any_element();
        };
        let target = editor.target;
        let control = action_segments(
            format!("permissions-new-pattern-{}", target.as_wire()),
            Some(editor.action),
            false,
            theme,
            {
                let this = this.clone();
                move |choice, cx| {
                    if let Some(action) = choice {
                        this.update(cx, |app, cx| app.set_pattern_action(action, cx));
                    }
                }
            },
        );
        let add =
            {
                let this = this.clone();
                primary_button("permissions-pattern-add", &tr!("permissions.add"), theme)
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        this.update(cx, |app, cx| app.add_pattern(cx));
                    })
            };
        let cancel = {
            let this = this.clone();
            ghost_button(
                "permissions-pattern-cancel".into(),
                &tr!("permissions.cancel"),
                theme,
            )
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| app.cancel_pattern_editor(cx));
            })
        };
        div()
            .w_full()
            .pl(DynamicSpacing::Base32.px(&theme))
            .pr(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child({
                        let example = target.pattern_example();
                        if example.is_empty() {
                            tr!(
                                "permissions.pattern_hint_plain",
                                text = target.matches_text()
                            )
                        } else {
                            tr!(
                                "permissions.pattern_hint",
                                text = target.matches_text(),
                                example = example
                            )
                        }
                    }),
            )
            .child(
                input_field_frame(div(), &theme)
                    .w_full()
                    .bg(theme.background)
                    .child(editor.pattern.clone()),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(control)
                    .child(add)
                    .child(cancel),
            )
            .into_any_element()
    }

    /// Change a stored pattern's action. Written here so `pattern_row` can keep
    /// its closures small; the rule keeps its place in the file.
    pub(super) fn set_pattern_action_of(
        &mut self,
        target: Target,
        pattern: &str,
        action: Action,
        cx: &mut Context<Self>,
    ) {
        let scope = self.permission_scope.clone();
        self.permission_rules
            .upsert(Rule::new(scope, target, pattern, action));
        self.persist_rules(cx);
    }
}

/// A row of a family board: full width, a comfortable column, hairline rhythm.
fn row_frame(theme: Theme) -> gpui::Div {
    div()
        .w_full()
        .px(DynamicSpacing::Base16.px(&theme))
        .py(DynamicSpacing::Base12.px(&theme))
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
}

/// A row's leading label.
fn row_label(text: &str, theme: Theme) -> gpui::Div {
    div()
        .flex_none()
        .text_size(TextSize::Default.px(&theme))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_primary)
        .child(text.to_string())
}

/// What a row resolves to, right-aligned: `→ Ask`. The label carries the
/// meaning; `Deny` and `Ask` also take their status colour so a refusal reads
/// at a glance without colour being the only signal.
fn effective_label(action: Action, theme: Theme) -> AnyElement {
    let color = match action {
        Action::Allow => theme.text_muted,
        Action::Ask => theme.warning,
        Action::Deny => theme.error,
    };
    div()
        .flex_none()
        .text_size(TextSize::Small.px(&theme))
        .text_color(color)
        .child(tr!("permissions.effective", action = action.label()))
        .into_any_element()
}

/// A four-state (or three-state) control: `Inherit · Allow · Ask · Deny`.
///
/// `selected` is what is *stored* — `None` means the row is inheriting — while
/// the row's `→ …` label says what that resolves to. Choosing `Inherit` stores
/// nothing, so the file only ever holds decisions a person made.
fn action_segments<F>(
    id: String,
    selected: Option<Action>,
    include_inherit: bool,
    theme: Theme,
    on_pick: F,
) -> AnyElement
where
    F: Fn(Option<Action>, &mut App) + Clone + 'static,
{
    let mut choices: Vec<(Option<Action>, String)> = Vec::new();
    if include_inherit {
        choices.push((None, tr!("permissions.inherit")));
    }
    choices.extend(Action::ALL.iter().map(|action| {
        (
            Some(*action),
            tr!(match action {
                Action::Allow => "permissions.action.allow",
                Action::Ask => "permissions.action.ask",
                Action::Deny => "permissions.action.deny",
            }),
        )
    }));
    let count = choices.len();
    let segments = choices
        .into_iter()
        .enumerate()
        .map(|(ix, (choice, label))| {
            let is_selected = choice == selected;
            let pick = on_pick.clone();
            let id = format!("{id}-{ix}");
            segmented_segment(
                div()
                    .id(ElementId::Name(id.clone().into()))
                    .debug_selector(move || format!("{id}-segment")),
                &theme,
                ButtonSize::Medium,
                SegmentPosition::at(ix, count),
            )
            .border_color(theme.border)
            .when(is_selected, |segment| segment.bg(theme.surface_active))
            .text_color(if is_selected {
                theme.text_primary
            } else {
                theme.text_secondary
            })
            .cursor_pointer()
            .when(!is_selected, |segment| segment.hover_state(&theme))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| pick(choice, cx))
            .child(label)
        });
    div()
        .flex_none()
        .flex()
        .items_center()
        .children(segments)
        .into_any_element()
}

/// A quiet text button: no fill, no border, focusable frame.
fn ghost_button(id: String, label: &str, theme: Theme) -> gpui::Stateful<gpui::Div> {
    button_frame(
        div().id(ElementId::Name(id.into())),
        &theme,
        ButtonSize::Compact,
    )
    .rounded(Radius::Small.px(&theme))
    .text_color(theme.text_secondary)
    .cursor_pointer()
    .hover_state(&theme)
    .pressed_state(&theme, PressedRamp::Wash)
    .child(label.to_string())
}

/// The one filled action on the page.
fn primary_button(id: &'static str, label: &str, theme: Theme) -> gpui::Stateful<gpui::Div> {
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .rounded(Radius::Medium.px(&theme))
        .raised(theme.action, &theme)
        .text_color(theme.action_fg)
        .cursor_pointer()
        .child(label.to_string())
}

/// A one-line consequence a row cannot show by itself.
fn note_row(text: &str, color: Hsla, theme: Theme) -> AnyElement {
    div()
        .w_full()
        .px(DynamicSpacing::Base16.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .text_size(TextSize::Small.px(&theme))
        .text_color(color)
        .child(text.to_string())
        .into_any_element()
}
