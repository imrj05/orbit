//! Model selector popup — a searchable picker popover anchored above the
//! composer chips.
//!
//! Follows the same GPUI conventions as the command palette and the branch
//! picker:
//!
//! - compact raised surface (`surface_elevated`, strong border, layered shadow)
//! - inset search field, then a quiet horizontally scrolling scope rail
//!   (`All`, `Recents`/`Favorites` when they exist, then each provider); the
//!   selected provider scrolls to the rail's centre and edge shadows grow with
//!   the scroll distance. The list itself groups models by provider
//! - models are grouped under provider headers (mark + name + count), with
//!   recently used models pinned under a `Recents` section and favorited models
//!   under a `Favorites` section above the provider groups (a model that is both
//!   appears once, under `Recents`); selecting a model records it as recent; a
//!   thinking list is flat, one icon-chip row per reasoning level
//! - model rows pair a provider tile with the model name and context window,
//!   followed by the hover star and chosen check. The id is in the tooltip.
//!   Thinking rows use the same two-line label/hint rhythm
//! - option rows share one tight `Base01` rhythm, the same gap the composer's
//!   access/workflow menus use (the command palette stacks rows with none);
//!   grouping is carried by the uppercase section headers alone, never by a
//!   hairline between every row
//! - every model row carries a star: an accent star at rest for favorites,
//!   revealed on hover for the rest, so any model can be pinned
//! - keyboard highlight uses `overlay_strong`; the current choice keeps the
//!   `active` fill plus an accent check, so "highlighted" and "chosen" never
//!   read as the same state
//! - a quiet footer shows the result count and keyboard navigation hints
//! - keyboard navigation: `up`/`down`/`enter`/`escape` are bound to the
//!   `Picker` key context on the filter input

use gpui::{
    div, linear_color_stop, linear_gradient, point, prelude::*, px, AnyElement, App, Context,
    ElementId, Entity, FocusHandle, Focusable, FontWeight, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, ScrollHandle, SharedString, Styled, Window,
};
use std::time::{Duration, Instant};

use crate::app::{
    button_frame, icon, icon_button_frame, icon_dyn, picker_entry, picker_search_frame,
    picker_surface, provider_mark_color, ControlExt, ModelEntry, PressedRamp, StatefulControlExt,
    TipExt,
};
use crate::composer::ComposerInput;
use crate::context_meter::format_tokens;
use crate::favorites::Favorites;
use crate::model_selector_match::is_model_selected;
use crate::providers::provider_display_name;
use crate::recent_models::Recents;
use crate::theme::tokens::{
    context_menu, input, list, picker, BufferLineHeight, ButtonSize, DynamicSpacing, IconSize,
    Radius, TextSize,
};
use crate::theme::{self, Theme};

/// Model popover width — room for the search field, mode tabs, a provider
/// header, and one-line rows with their metadata.
const MODEL_POPOVER_W: f32 = 400.;
/// Thinking popover width — a small flat set of levels, so it hugs tighter
/// than the model catalog while still fitting each level's hint.
const THINKING_POPOVER_W: f32 = 300.;
/// Option rows visible before the list scrolls.
const VISIBLE_ROWS: f32 = 6.;
/// How far the scope rail travels before its edge shadow is at full strength.
const RAIL_SHADOW_FADE: f32 = 28.;

/// The list's layout metrics, resolved from the theme so the keyboard
/// scroll math measures exactly what the rows paint.
#[derive(Debug, Clone, Copy)]
struct ListMetrics {
    /// An option row, held at a fixed height for its two-line label and detail.
    row_h: f32,
    /// A group header: a list sub-header row plus its bottom padding.
    header_h: f32,
    /// Air between list children. One tight `Base01` step, matching the
    /// composer's access/workflow menus, so a run of option rows reads as one
    /// quiet stack instead of a list of separated cards.
    row_gap: f32,
}

impl ListMetrics {
    fn new(kind: PickerKind, theme: &Theme) -> Self {
        Self {
            // Keep the measured row height in sync with the two line boxes
            // and the extra breathing room painted by each model row.
            row_h: match kind {
                PickerKind::Model => {
                    (picker::two_line_entry_height(theme) + DynamicSpacing::Base04.px(theme)).into()
                }
                PickerKind::Thinking => picker::two_line_entry_height(theme).into(),
            },
            header_h: (list::sub_header_height(theme) + list::sub_header_padding_bottom(theme))
                .into(),
            row_gap: DynamicSpacing::Base01.px(theme).into(),
        }
    }

    /// Largest list height before it scrolls (≈ 6 visible option rows).
    fn list_max_h(&self) -> f32 {
        VISIBLE_ROWS * (self.row_h + self.row_gap)
    }
}

/// Model-select callback: model name, model id, and the ambient window.
type SelectModel = Box<dyn Fn(&str, &str, &mut Window, &mut App)>;
/// Thinking-level callback: the chosen level plus the ambient window.
type SelectLevel = Box<dyn Fn(&str, &mut Window, &mut App)>;
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type SelectorDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

/// Which single-section dropdown a picker popup shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    /// Model catalog from `get_available_models`.
    Model,
    /// Thinking levels from `get_available_thinking_levels`.
    Thinking,
}

/// What the model list is currently scoped to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Scope {
    /// The full catalog, grouped by provider.
    #[default]
    All,
    /// Only recently used models, most-recent first.
    Recents,
    /// Only favorited models.
    Favorites,
    /// A single provider, listed flat.
    Provider(String),
}

/// One child of the picker list.
#[derive(Clone)]
enum Row {
    /// A group header. `favorites` marks the pinned Favorites section.
    Header {
        provider: String,
        favorites: bool,
        /// The pinned Recents section at the top of the list.
        recents: bool,
    },
    /// A thinking level (`level` is the raw pi value; display only).
    Level { level: String, selected: bool },
    /// `model_ix` indexes into the full model catalog, so selection survives
    /// while the filtered set is recomputed every frame.
    Model { model_ix: usize, selected: bool },
}

impl Row {
    fn is_header(&self) -> bool {
        matches!(self, Row::Header { .. })
    }

    /// Laid-out height; headers are shorter than option rows.
    fn height(&self, metrics: &ListMetrics) -> f32 {
        if self.is_header() {
            metrics.header_h
        } else {
            metrics.row_h
        }
    }
}

/// A single-section model/thinking dropdown. Created by `OrbitApp` when one
/// of the composer chips is clicked; it talks back exclusively through the
/// callbacks it was built with, so it never borrows app state.
pub struct ModelSelector {
    kind: PickerKind,
    models: Vec<ModelEntry>,
    levels: Vec<String>,
    current_model: String,
    current_model_id: String,
    current_model_provider: String,
    current_level: String,
    filter: Entity<ComposerInput>,
    /// The model list scope. Provider scopes drop the group headers (the chip
    /// already states the provider); Favorites pins its section at the top.
    scope: Scope,
    /// Horizontal scroll position of the scope rail, so selecting a provider
    /// can bring it to the centre.
    scope_scroll: ScrollHandle,
    /// One follow-up repaint after the rail first lays out, so the edge shadows
    /// can read the real overflow (`max_offset` is only known in prepaint).
    scope_rail_measured: bool,
    /// Scroll position of the popup's list (keyboard navigation keeps the
    /// highlighted row in view via this handle).
    list_scroll: ScrollHandle,
    highlighted: usize,
    /// Catalog index of the active model (stable match key).
    selected_catalog_ix: Option<usize>,
    /// Ignore hover-driven highlight briefly so opening under the cursor
    /// doesn't snap back to row 0.
    suppress_hover_until: Option<Instant>,
    /// Deferred popovers need a follow-up scroll after layout settles.
    needs_scroll: bool,
    last_filter: String,
    on_select_model: SelectModel,
    on_select_level: SelectLevel,
    /// `bool` = dismissed by an outside mouse-down (vs. escape), so the
    /// owner can suppress the trigger chip's click-through.
    on_dismiss: SelectorDismiss,
}

impl ModelSelector {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: PickerKind,
        models: Vec<ModelEntry>,
        levels: Vec<String>,
        current_model: String,
        current_model_id: String,
        current_model_provider: String,
        current_level: String,
        on_select_model: SelectModel,
        on_select_level: SelectLevel,
        on_dismiss: SelectorDismiss,
        cx: &mut Context<Self>,
    ) -> Self {
        // The filter input carries both the `Composer` context (so backspace,
        // paste, etc. keep working) and the `Picker` flag (so the picker's
        // enter/escape/arrows take precedence at the same dispatch depth).
        let placeholder_key = match kind {
            PickerKind::Model => "model_selector.search_models",
            PickerKind::Thinking => "model_selector.search_levels",
        };
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("picker-filter")
                .with_placeholder_key(placeholder_key)
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let selected_catalog_ix = Self::resolve_selected_catalog_ix(
            kind,
            &models,
            &levels,
            &current_model,
            &current_model_id,
            &current_model_provider,
            &current_level,
        );
        let rows = Self::rows_for(
            kind,
            &models,
            &levels,
            &current_level,
            selected_catalog_ix,
            &Scope::All,
            &Favorites::default(),
            &Recents::default(),
            "",
        );
        let mut list_scroll = ScrollHandle::new();
        if let Some(ix) = Self::selected_row_index(&rows) {
            let metrics = ListMetrics::new(kind, theme::get(cx));
            Self::apply_scroll_to_row(&mut list_scroll, &rows, ix, &metrics);
        }
        let highlighted = Self::first_selectable(&rows).unwrap_or(0);
        Self {
            kind,
            models,
            levels,
            current_model,
            current_model_id,
            current_model_provider,
            current_level,
            filter,
            scope: Scope::All,
            scope_scroll: ScrollHandle::new(),
            scope_rail_measured: false,
            list_scroll,
            highlighted,
            selected_catalog_ix,
            suppress_hover_until: Some(Instant::now() + Duration::from_millis(400)),
            needs_scroll: true,
            last_filter: String::new(),
            on_select_model,
            on_select_level,
            on_dismiss,
        }
    }

    fn resolve_selected_catalog_ix(
        kind: PickerKind,
        models: &[ModelEntry],
        levels: &[String],
        current_model: &str,
        current_model_id: &str,
        current_model_provider: &str,
        current_level: &str,
    ) -> Option<usize> {
        match kind {
            PickerKind::Model => models.iter().position(|model| {
                is_model_selected(
                    model,
                    current_model,
                    current_model_id,
                    current_model_provider,
                )
            }),
            PickerKind::Thinking => levels
                .iter()
                .position(|level| level.eq_ignore_ascii_case(current_level)),
        }
    }

    fn refresh_selected_catalog_ix(&mut self) {
        self.selected_catalog_ix = Self::resolve_selected_catalog_ix(
            self.kind,
            &self.models,
            &self.levels,
            &self.current_model,
            &self.current_model_id,
            &self.current_model_provider,
            &self.current_level,
        );
    }

    /// The visible list children for `needle`, scoped to `scoped_provider`.
    /// Unscoped, model rows group under a provider header (providers in
    /// first-appearance order, catalog order within a provider); scoped, the
    /// list is flat because the chip already states the provider. Thinking
    /// levels are always flat. Filtering is a case-insensitive substring
    /// match on name/id/provider/level.
    #[allow(clippy::too_many_arguments)]
    fn rows_for(
        kind: PickerKind,
        models: &[ModelEntry],
        levels: &[String],
        current_level: &str,
        selected_catalog_ix: Option<usize>,
        scope: &Scope,
        favorites: &Favorites,
        recents: &Recents,
        needle: &str,
    ) -> Vec<Row> {
        let matches = |text: &str| needle.is_empty() || text.to_lowercase().contains(needle);
        let mut rows = Vec::new();

        if kind == PickerKind::Thinking {
            for level in levels.iter().filter(|level| {
                matches(level)
                    || matches(&thinking_display(level))
                    || matches(&thinking_hint(level))
            }) {
                rows.push(Row::Level {
                    level: level.clone(),
                    selected: level.eq_ignore_ascii_case(current_level),
                });
            }
            return rows;
        }

        // A dedicated Recents scope: flat and in most-recent-first order (not
        // catalog order), still honoring the text filter.
        if *scope == Scope::Recents {
            for (provider, id) in recents.entries() {
                let Some(ix) = models
                    .iter()
                    .position(|model| &model.provider == provider && &model.id == id)
                else {
                    continue;
                };
                let model = &models[ix];
                if matches(&model.name) || matches(&model.id) || matches(&model.provider) {
                    rows.push(Row::Model {
                        model_ix: ix,
                        selected: selected_catalog_ix == Some(ix),
                    });
                }
            }
            return rows;
        }

        let in_scope = |model: &ModelEntry| -> bool {
            match scope {
                Scope::All => true,
                // Handled above.
                Scope::Recents => false,
                Scope::Favorites => favorites.contains(&model.provider, &model.id),
                Scope::Provider(provider) => &model.provider == provider,
            }
        };
        // Scope and text filter are independent: the Favorites scope still
        // honors the search box.
        let included = |model: &ModelEntry| -> bool {
            in_scope(model)
                && (matches(&model.name) || matches(&model.id) || matches(&model.provider))
        };

        // Recents and favorites pin above the provider groups — recents at the
        // very top (a recently used model is the likeliest pick), then
        // favorites. Both de-dup against the groups below, and recents win over
        // favorites, so no model appears twice.
        let mut pinned: Vec<usize> = Vec::new();

        // Recents lead the All scope; a provider scope pins that provider's own
        // recents (the chip already names the provider).
        if matches!(scope, Scope::All | Scope::Provider(_)) {
            let section: Vec<usize> = if *scope == Scope::All {
                recents
                    .entries()
                    .iter()
                    .filter_map(|(provider, id)| {
                        models
                            .iter()
                            .position(|model| &model.provider == provider && &model.id == id)
                    })
                    .filter(|&ix| included(&models[ix]))
                    .collect()
            } else {
                models
                    .iter()
                    .enumerate()
                    .filter(|(_, model)| {
                        recents.contains(&model.provider, &model.id) && included(model)
                    })
                    .map(|(ix, _)| ix)
                    .collect()
            };
            if !section.is_empty() {
                rows.push(Row::Header {
                    provider: String::new(),
                    favorites: false,
                    recents: true,
                });
                for ix in &section {
                    rows.push(Row::Model {
                        model_ix: *ix,
                        selected: selected_catalog_ix == Some(*ix),
                    });
                }
                pinned.extend(section);
            }
        }

        if *scope == Scope::All && !favorites.is_empty() {
            let section: Vec<usize> = models
                .iter()
                .enumerate()
                .filter(|(ix, model)| {
                    favorites.contains(&model.provider, &model.id)
                        && included(model)
                        && !pinned.contains(ix)
                })
                .map(|(ix, _)| ix)
                .collect();
            if !section.is_empty() {
                rows.push(Row::Header {
                    provider: String::new(),
                    favorites: true,
                    recents: false,
                });
                for ix in &section {
                    rows.push(Row::Model {
                        model_ix: *ix,
                        selected: selected_catalog_ix == Some(*ix),
                    });
                }
                pinned.extend(section);
            }
        }

        let mut providers: Vec<String> = Vec::new();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (ix, model) in models.iter().enumerate() {
            if !included(model) || pinned.contains(&ix) {
                continue;
            }
            match providers.iter().position(|p| p == &model.provider) {
                Some(pos) => groups[pos].push(ix),
                None => {
                    providers.push(model.provider.clone());
                    groups.push(vec![ix]);
                }
            }
        }
        for (provider, group) in providers.iter().zip(groups) {
            // The selected provider is already named by its tab; only the
            // All scope needs provider headings to distinguish the groups.
            if *scope == Scope::All {
                rows.push(Row::Header {
                    provider: provider.clone(),
                    favorites: false,
                    recents: false,
                });
            }
            for ix in group {
                rows.push(Row::Model {
                    model_ix: ix,
                    selected: selected_catalog_ix == Some(ix),
                });
            }
        }
        rows
    }

    fn rows(&self, needle: &str) -> Vec<Row> {
        Self::rows_for(
            self.kind,
            &self.models,
            &self.levels,
            &self.current_level,
            self.selected_catalog_ix,
            &self.scope,
            &crate::favorites::all(),
            &crate::recent_models::all(),
            needle,
        )
    }

    /// Distinct providers in catalog (first-appearance) order, for the rail.
    fn providers(&self) -> Vec<String> {
        let mut providers: Vec<String> = Vec::new();
        for model in &self.models {
            if !providers.contains(&model.provider) {
                providers.push(model.provider.clone());
            }
        }
        providers
    }

    /// Catalog counts remain stable while searching, and match each scope's
    /// available models rather than the visible subset of a pinned section.
    fn scope_count(
        scope: &Scope,
        models: &[ModelEntry],
        favorites: &Favorites,
        recents: &Recents,
    ) -> usize {
        match scope {
            Scope::All => models.len(),
            Scope::Favorites => models
                .iter()
                .filter(|model| favorites.contains(&model.provider, &model.id))
                .count(),
            Scope::Recents => recents
                .entries()
                .iter()
                .filter(|(provider, id)| {
                    models
                        .iter()
                        .any(|model| &model.provider == provider && &model.id == id)
                })
                .count(),
            Scope::Provider(provider) => models
                .iter()
                .filter(|model| &model.provider == provider)
                .count(),
        }
    }

    /// Apply one of the rail's scopes. Keeps the text filter, resets the
    /// scroll, and re-pins the highlight to the chosen model when it falls in
    /// scope.
    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.list_scroll.set_offset(point(px(0.), px(0.)));
        self.block_hover_highlight();
        self.needs_scroll = true;
        cx.notify();
    }

    /// Toggle a model as a favorite; the list and its pinned section refresh.
    fn toggle_favorite(&mut self, model_ix: usize, cx: &mut Context<Self>) {
        let Some(model) = self.models.get(model_ix) else {
            return;
        };
        let (provider, id) = (model.provider.clone(), model.id.clone());
        crate::favorites::toggle(&provider, &id);
        self.block_hover_highlight();
        self.needs_scroll = true;
        cx.notify();
    }

    /// The scope rail: All, Recents/Favorites when they exist, then each
    /// catalog provider. It is one horizontally scrollable row of compact
    /// icon + label chips; selecting a provider scrolls that chip to the
    /// centre, and the edge shadows grow with the scroll distance so the rail
    /// reads as continuous rather than cut off.
    fn scope_row(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let this = cx.weak_entity();
        let favorites = crate::favorites::all();
        let recents = crate::recent_models::all();
        let mut entries: Vec<(String, Scope)> = vec![(tr!("model_selector.all"), Scope::All)];
        if !recents.is_empty() {
            entries.push((tr!("model_selector.recents"), Scope::Recents));
        }
        if !favorites.is_empty() {
            entries.push((tr!("model_selector.favorites"), Scope::Favorites));
        }
        entries.extend(
            self.providers()
                .into_iter()
                .map(|provider| (provider_display_name(&provider), Scope::Provider(provider))),
        );
        // A lone "All" tab is a mode row with nothing to switch to.
        if entries.len() <= 1 {
            return None;
        }

        let mut row = div()
            .id("picker-scope")
            .debug_selector(|| "picker-scope".to_string())
            .w_full()
            .flex_none()
            .px(picker::search_padding_x(&theme))
            .py(DynamicSpacing::Base04.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .overflow_x_scroll()
            .track_scroll(&self.scope_scroll);
        for (ix, (label, scope)) in entries.into_iter().enumerate() {
            let count = Self::scope_count(&scope, &self.models, &favorites, &recents);
            let active = self.scope == scope;
            let ink = if active {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            // One iconography for every scope: the provider's mark when it has
            // one, and the shared mode glyphs for All/Recents/Favorites. The
            // mark stays brand-faithful; only the mono fallback follows the
            // active ink.
            let lead = match &scope {
                Scope::Provider(provider) => icon_dyn(
                    provider_icon(provider),
                    IconSize::Small.px(&theme),
                    provider_mark_color(
                        provider,
                        &theme,
                        if active {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        },
                    ),
                )
                .into_any_element(),
                Scope::Recents => icon(
                    "icons/clock.svg",
                    IconSize::Small.px(&theme),
                    if active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    },
                )
                .into_any_element(),
                Scope::Favorites => icon(
                    "icons/star.svg",
                    IconSize::Small.px(&theme),
                    if active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    },
                )
                .into_any_element(),
                Scope::All => icon(
                    "icons/extensions.svg",
                    IconSize::Small.px(&theme),
                    if active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    },
                )
                .into_any_element(),
            };
            let this = this.clone();
            row = row.child(
                button_frame(
                    div().id(ElementId::NamedInteger("scope-chip".into(), ix as u64)),
                    &theme,
                    ButtonSize::Medium,
                )
                .px(DynamicSpacing::Base08.px(&theme))
                .rounded(Radius::Large.px(&theme))
                .border_1()
                .border_color(theme.border)
                .text_size(TextSize::Small.px(&theme))
                .text_color(ink)
                .when(active, |tab| tab.bg(theme.surface_selected))
                .when(!active, |tab| tab.hover_quiet(&theme))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    this.update(cx, |selector, cx| {
                        selector.set_scope(scope.clone(), cx);
                        selector.center_scope_chip(ix, cx);
                    })
                    .ok();
                })
                .child(lead)
                .child(label)
                .child(div().text_color(theme.text_muted).child(count.to_string())),
            );
        }

        // The rail sits on the popup surface; a contact shadow at an edge,
        // faded by how far the rail has scrolled, is the "more content here"
        // cue. It grows over the first 28px of travel rather than switching on.
        let (left, right) = rail_shadow_opacities(
            f32::from(self.scope_scroll.offset().x),
            f32::from(self.scope_scroll.max_offset().width),
        );
        let shadow = |on_left: bool, opacity: f32| {
            let mut el = div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(28.))
                .bg(linear_gradient(
                    if on_left { 90. } else { 270. },
                    linear_color_stop(theme.shadow_contact, 0.),
                    linear_color_stop(theme.shadow_contact.opacity(0.), 1.),
                ))
                .opacity(opacity);
            el = if on_left { el.left_0() } else { el.right_0() };
            el.into_any_element()
        };

        Some(
            div()
                .relative()
                .w_full()
                .flex_none()
                .child(row)
                .child(shadow(true, left))
                .child(shadow(false, right))
                .into_any_element(),
        )
    }

    /// Bring the selected scope chip to the centre of the rail. The chip
    /// bounds are window-space from the last prepaint; the offset needed is
    /// the distance between its centre and the viewport's, clamped to the
    /// rail's own scroll range so an edge chip stops at the edge instead of
    /// overscrolling.
    fn center_scope_chip(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(bounds) = self.scope_scroll.bounds_for_item(ix) else {
            return;
        };
        let viewport = self.scope_scroll.bounds();
        let max = f32::from(self.scope_scroll.max_offset().width);
        if viewport.size.width <= px(0.) || max <= 0. {
            return;
        }
        let offset =
            (f32::from(viewport.center().x) - f32::from(bounds.center().x)).clamp(-max, 0.);
        self.scope_scroll
            .set_offset(point(px(offset), self.scope_scroll.offset().y));
        self.scope_rail_measured = true;
        cx.notify();
    }

    fn selected_row_index(rows: &[Row]) -> Option<usize> {
        rows.iter().position(|row| match row {
            Row::Model { selected, .. } | Row::Level { selected, .. } => *selected,
            Row::Header { .. } => false,
        })
    }

    fn first_selectable(rows: &[Row]) -> Option<usize> {
        rows.iter().position(|row| !row.is_header())
    }

    fn content_height(rows: &[Row], metrics: &ListMetrics) -> f32 {
        if rows.is_empty() {
            return 0.;
        }
        let rows_h: f32 = rows.iter().map(|row| row.height(metrics)).sum();
        rows_h + (rows.len() - 1) as f32 * metrics.row_gap
    }

    /// Pixel offset of a row's top edge, accounting for the gaps between list
    /// children.
    fn row_top(rows: &[Row], ix: usize, metrics: &ListMetrics) -> f32 {
        let mut y = 0.;
        for row in rows.iter().take(ix) {
            y += row.height(metrics) + metrics.row_gap;
        }
        y
    }

    fn apply_scroll_to_row(
        list_scroll: &mut ScrollHandle,
        rows: &[Row],
        ix: usize,
        metrics: &ListMetrics,
    ) {
        if ix >= rows.len() {
            return;
        }
        let content_h = Self::content_height(rows, metrics);
        let viewport_h = content_h.min(metrics.list_max_h());
        let row_top = Self::row_top(rows, ix, metrics);
        let row_h = rows[ix].height(metrics);
        let max_offset = (content_h - viewport_h).max(0.);
        let centered = row_top - (viewport_h - row_h) / 2.0;
        let offset = centered.clamp(0., max_offset);
        list_scroll.set_offset(point(px(0.), px(-offset)));
    }

    fn hover_highlight_blocked(&self) -> bool {
        self.suppress_hover_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn block_hover_highlight(&mut self) {
        self.suppress_hover_until = Some(Instant::now() + Duration::from_millis(400));
    }

    /// Live catalog refresh while the popup is open (pi re-reports these
    /// after `set_model`, session switches, etc.).
    #[allow(clippy::too_many_arguments)]
    pub fn set_catalog(
        &mut self,
        models: Vec<ModelEntry>,
        levels: Vec<String>,
        current_model: String,
        current_model_id: String,
        current_model_provider: String,
        current_level: String,
        cx: &mut Context<Self>,
    ) {
        // pi re-reports the catalog on every `get_state`, not only on a real
        // change. Re-pinning the highlight on each one would snap an ↑/↓ back
        // to the active model while the popup is open, so an unchanged sync
        // must leave the highlight, scroll and hover-block untouched.
        let unchanged = self.models == models
            && self.levels == levels
            && self.current_model == current_model
            && self.current_model_id == current_model_id
            && self.current_model_provider == current_model_provider
            && self.current_level == current_level;
        if unchanged {
            return;
        }
        self.models = models;
        self.levels = levels;
        self.current_model = current_model;
        self.current_model_id = current_model_id;
        self.current_model_provider = current_model_provider;
        self.current_level = current_level;
        self.refresh_selected_catalog_ix();
        // Drop a provider scope whose provider left the catalog, and fall
        // back from Recents/Favorites when the last entry was removed.
        match &self.scope {
            Scope::Provider(provider)
                if !self.models.iter().any(|model| &model.provider == provider) =>
            {
                self.scope = Scope::All;
            }
            Scope::Recents if crate::recent_models::all().is_empty() => {
                self.scope = Scope::All;
            }
            Scope::Favorites if crate::favorites::all().is_empty() => {
                self.scope = Scope::All;
            }
            _ => {}
        }
        let rows = self.rows("");
        if let Some(ix) = Self::selected_row_index(&rows) {
            let metrics = ListMetrics::new(self.kind, theme::get(cx));
            Self::apply_scroll_to_row(&mut self.list_scroll, &rows, ix, &metrics);
        }
        self.block_hover_highlight();
        self.needs_scroll = true;
        cx.notify();
    }

    // ── actions (Picker key context) ──────────────────────────────────────

    fn on_cancel(&mut self, _: &crate::PickerCancel, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(false, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter.read(cx).is_composing() {
            return;
        }
        let rows = self.rows(&self.last_filter);
        self.activate(self.highlighted, &rows, window, cx);
    }

    fn on_next(&mut self, _: &crate::PickerSelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn on_prev(&mut self, _: &crate::PickerSelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    /// Outside mouse-down anywhere dismisses the popup.
    fn on_outside_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(true, window, cx);
    }

    // ── internals ─────────────────────────────────────────────────────────

    /// Re-apply the scroll after the deferred popover has laid out (its
    /// `max_offset` is unknown until then). Targets the **current** highlight,
    /// not the row captured on open, so an ↑/↓ during the retry window isn't
    /// scrolled back to the active model.
    fn defer_scroll(&self, cx: &mut Context<Self>) {
        for delay in [16_u64, 50, 120, 250] {
            let this = cx.weak_entity();
            cx.spawn(async move |_, cx| {
                gpui::Timer::after(Duration::from_millis(delay)).await;
                this.update(cx, |selector, cx| {
                    let rows = selector.rows(&selector.last_filter);
                    let ix = selector.highlighted;
                    selector.scroll_to_row(ix, &rows, cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn scroll_to_row(&mut self, ix: usize, rows: &[Row], cx: &App) {
        let metrics = ListMetrics::new(self.kind, theme::get(cx));
        Self::apply_scroll_to_row(&mut self.list_scroll, rows, ix, &metrics);
    }

    /// Test-only: the row index the keyboard cursor sits on.
    #[cfg(test)]
    pub(crate) fn highlighted_row(&self) -> usize {
        self.highlighted
    }

    /// Move the highlight one option, skipping provider headers so the
    /// keyboard cursor never lands on a label. Clamps at the ends.
    fn step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let rows = self.rows(&self.last_filter);
        if rows.is_empty() {
            return;
        }
        let mut next = self.highlighted.min(rows.len() - 1);
        loop {
            let candidate = next as isize + dir;
            if candidate < 0 || candidate >= rows.len() as isize {
                break;
            }
            next = candidate as usize;
            if !rows[next].is_header() {
                break;
            }
        }
        self.highlighted = next;
        // A deliberate ↑/↓ owns the highlight. Cancel any still-armed open pin
        // (the popup may not have rendered its first frame yet, or a scope /
        // catalog change re-armed it), or the next render snaps the cursor back
        // to the active model.
        self.needs_scroll = false;
        self.scroll_to_row(next, &rows, cx);
        cx.notify();
    }

    fn activate(&mut self, ix: usize, rows: &[Row], window: &mut Window, cx: &mut Context<Self>) {
        match rows.get(ix) {
            Some(Row::Model { model_ix, .. }) => {
                let (id, provider) = {
                    let model = &self.models[*model_ix];
                    (model.id.clone(), model.provider.clone())
                };
                (self.on_select_model)(&id, &provider, window, cx);
            }
            Some(Row::Level { level, .. }) => {
                let level = level.clone();
                (self.on_select_level)(&level, window, cx);
            }
            _ => {}
        }
    }
}

impl Focusable for ModelSelector {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.read(cx).focus_handle(cx)
    }
}

impl Render for ModelSelector {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let needle = self.filter.read(cx).text().to_lowercase();
        let rows = self.rows(&needle);
        if needle != self.last_filter {
            self.last_filter = needle.clone();
            self.list_scroll.set_offset(point(px(0.), px(0.)));
            if needle.is_empty() {
                self.block_hover_highlight();
                self.needs_scroll = true;
            } else if let Some(ix) = Self::first_selectable(&rows) {
                self.highlighted = ix;
            }
        }

        // Only a genuine open/refresh re-pins the highlight to the active
        // model. The hover-block window must NOT: it stays armed for 400ms, so
        // re-pinning on every render would undo an ↑/↓ pressed right after the
        // popover opens (or after a scope change / catalog refresh).
        let pin_to_selection = self.needs_scroll;
        if pin_to_selection {
            if let Some(ix) = Self::selected_row_index(&rows) {
                self.highlighted = ix;
                self.scroll_to_row(ix, &rows, cx);
                if self.needs_scroll {
                    self.defer_scroll(cx);
                }
            } else if let Some(ix) = Self::first_selectable(&rows) {
                self.highlighted = ix;
            }
            self.needs_scroll = false;
        } else if !rows.is_empty() {
            self.highlighted = self.highlighted.min(rows.len() - 1);
            if rows[self.highlighted].is_header() {
                if let Some(ix) = Self::first_selectable(&rows) {
                    self.highlighted = ix;
                }
            }
        }

        let this = cx.entity();
        let theme = *theme::get(cx);
        let metrics = ListMetrics::new(self.kind, &theme);
        let width = match self.kind {
            PickerKind::Model => MODEL_POPOVER_W,
            PickerKind::Thinking => THINKING_POPOVER_W,
        };
        let option_count = rows.iter().filter(|row| !row.is_header()).count();
        let count_label = match self.kind {
            PickerKind::Thinking => tr!("model_selector.n_levels", count = option_count),
            PickerKind::Model => match self.scope {
                Scope::Recents => tr!("model_selector.n_recent", count = option_count),
                Scope::Favorites => tr!("model_selector.n_favorites", count = option_count),
                _ => tr!("model_selector.n_models", count = option_count),
            },
        };
        // Plain scrollable list — the same shape Zed's `ContextMenu` uses for
        // menu bodies. Rows are ordinary children, so nothing depends on
        // measured-layout sizing inside the deferred popover.
        let mut list = div()
            .id("picker-list")
            .debug_selector(|| "picker-list".to_string())
            .w_full()
            .max_h(px(metrics.list_max_h()))
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(px(metrics.row_gap));
        for ix in 0..rows.len() {
            list = list.child(render_row(
                &rows,
                &self.models,
                ix,
                ix == self.highlighted,
                &this,
                theme,
                &metrics,
            ));
        }

        // `max_offset` is only known after prepaint; one follow-up repaint lets
        // the rail's right edge shadow read the real overflow on open.
        if self.kind == PickerKind::Model && !self.scope_rail_measured {
            self.scope_rail_measured = true;
            let this = cx.weak_entity();
            cx.spawn(async move |_, cx| {
                gpui::Timer::after(Duration::from_millis(32)).await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
        }

        picker_surface(div(), &theme)
            .debug_selector(|| "picker-surface".to_string())
            .w(px(width))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            // A click inside the popup (a row, the search field, the scope
            // chips) is the popup's own business: stop it here so it never
            // bubbles out to the composer box, whose mouse-up refocuses the
            // composer input and would kill the popup's keyboard ownership.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // any mouse-down outside the popup dismisses it
            .on_mouse_down_out(cx.listener(Self::on_outside_down))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            // search field — grouped above the list with a divider
            .child(
                picker_search_frame(div(), &theme)
                    .debug_selector(|| "picker-search".to_string())
                    .child(icon(
                        "icons/search.svg",
                        input::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(div().flex_1().min_w_0().child(self.filter.clone())),
            )
            // The model picker's modes — quiet tabs, not a provider rail; a
            // fresh install with no recents or favorites has nothing to switch
            // between and keeps only the search row.
            .children(
                (self.kind == PickerKind::Model)
                    .then(|| self.scope_row(theme, cx))
                    .flatten(),
            )
            .child(if rows.is_empty() {
                empty_state(self.kind, theme).into_any_element()
            } else {
                list.into_any_element()
            })
            // A quiet total, not a keyboard cheatsheet: the list is the
            // interface, and the count is the one fact the rows cannot show.
            .child(footer(count_label, theme))
    }
}

/// Icon + tint for a pi thinking level (HugeIcons stroke set in
/// `assets/icons/`). Mapping: idea-01 (off/none), idea (low/minimal), brain
/// (medium), brain-02 (high), ai-brain-02 (xhigh/ultra), ai-brain-03 (max),
/// sparkles (auto). Unknown levels fall back to the plain spark.
pub(crate) fn thinking_icon(level: &str, theme: &Theme) -> (&'static str, gpui::Hsla) {
    match level.to_ascii_lowercase().as_str() {
        "off" | "none" => ("icons/thinking-none.svg", theme.text_muted),
        "minimal" | "low" => ("icons/thinking-low.svg", theme.accent_text),
        "medium" => ("icons/thinking-medium.svg", theme.accent_text),
        "high" => ("icons/thinking-high.svg", theme.accent_text),
        "xhigh" | "ultra" => ("icons/thinking-xhigh.svg", theme.accent_text),
        "max" => ("icons/thinking-max.svg", theme.accent_text),
        "auto" => ("icons/thinking-auto.svg", theme.accent_text),
        _ => ("icons/spark.svg", theme.accent_text),
    }
}

/// Brand glyph for a pi provider — mono SVGs from [theSVG.org]
/// (https://thesvg.org), embedded under `assets/icons/providers/` and named
/// by provider id. A few ids reuse another brand's mark (Ollama Cloud is
/// still Ollama). Every pi built-in provider has a mark; unknown or
/// user-defined providers (custom `models.json` entries) fall back to a
/// neutral cloud glyph.
pub(crate) fn provider_icon(provider: &str) -> SharedString {
    // Provider id → the id whose mark it shares. The leading dot on
    // `.manifest` is pi's catalog-index filename, not a real provider id.
    const ALIASES: &[(&str, &str)] = &[("ollama-cloud", "ollama"), (".manifest", "manifest")];
    if let Some((_, mark)) = ALIASES.iter().find(|(id, _)| *id == provider) {
        return format!("icons/providers/{mark}.svg").into();
    }
    const KNOWN: &[&str] = &[
        "amazon-bedrock",
        "ant-ling",
        "anthropic",
        "azure-openai-responses",
        "baseten",
        "cerebras",
        "clinepass",
        "cloudflare-ai-gateway",
        "cloudflare-workers-ai",
        "deepseek",
        "fireworks",
        "github-copilot",
        "google",
        "google-vertex",
        "groq",
        "huggingface",
        "kimi-coding",
        "llama.cpp",
        "manifest",
        "minimax",
        "minimax-cn",
        "mistral",
        "moonshotai",
        "moonshotai-cn",
        "nvidia",
        "ollama",
        "openai",
        "openai-codex",
        "opencode",
        "opencode-go",
        "openrouter",
        "qwen-token-plan",
        "qwen-token-plan-cn",
        "qwen-token-plan-individual",
        "radius",
        "together",
        "vercel-ai-gateway",
        "xai",
        "xiaomi",
        "xiaomi-token-plan-ams",
        "xiaomi-token-plan-cn",
        "xiaomi-token-plan-sgp",
        "zai",
        "zai-coding-cn",
    ];
    if KNOWN.contains(&provider) {
        format!("icons/providers/{provider}.svg").into()
    } else {
        "icons/cloud.svg".into()
    }
}

/// Display form of a pi thinking level: `high` → `High` (capitalization is
/// presentational; the value stays pi's).
pub(crate) fn thinking_display(level: &str) -> String {
    let mut chars = level.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Short hint shown under the thinking level label in the picker.
fn thinking_hint(level: &str) -> String {
    match level.to_ascii_lowercase().as_str() {
        "off" | "none" => tr!("model_selector.reasoning_none"),
        "minimal" | "low" => tr!("model_selector.reasoning_fast"),
        "medium" => tr!("model_selector.reasoning_balanced"),
        "high" => tr!("model_selector.reasoning_deeper"),
        "xhigh" | "ultra" => tr!("model_selector.reasoning_extensive"),
        "max" => tr!("model_selector.reasoning_maximum"),
        "auto" => tr!("model_selector.reasoning_auto"),
        _ => tr!("model_selector.reasoning_custom"),
    }
}

/// Leading tile shared by the model and thinking rows — sized like sidebar
/// session chips so HugeIcons glyphs read clearly (some levels use small
/// artwork in the 24×24 viewBox, e.g. minimal's dot). The glyph inside is a
/// picker-row icon (`context_menu::ICON`).
const LEADING_TILE: f32 = 28.;

fn trailing_check(selected: bool, theme: Theme) -> impl IntoElement + use<> {
    div()
        .w(context_menu::ICON.px(&theme))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(if selected {
            icon(
                "icons/check.svg",
                context_menu::ICON.px(&theme),
                theme.accent_text,
            )
            .into_any_element()
        } else {
            div().into_any_element()
        })
}

/// The neutral leading tile for the two-line pickers: a 28px raised square with
/// an 8px corner. It stays neutral in every state — selection is carried by the
/// row's own fill and the trailing accent check, so the tile never adds a second
/// accent mark to the same row (the One Accent Rule).
pub(crate) fn leading_tile(theme: Theme, glyph: AnyElement) -> impl IntoElement + use<> {
    div()
        .size(px(LEADING_TILE))
        .flex_none()
        .rounded(Radius::Large.px(&theme))
        .bg(theme.surface_elevated)
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
}

fn thinking_chip(level: &str, theme: Theme) -> impl IntoElement + use<> {
    let (path, color) = thinking_icon(level, &theme);
    leading_tile(
        theme,
        icon(path, context_menu::ICON.px(&theme), color).into_any_element(),
    )
}

/// The provider mark used by a provider header and by the mixed
/// Recents/Favorites sections. Coloured brands keep their hue; monochrome
/// wordmarks fall back to the muted ink. It uses the same mark scale as the
/// rail's chips, so the provider is recognisable in both places.
fn provider_mark(provider: &str, theme: Theme) -> AnyElement {
    div()
        .flex_none()
        .child(icon_dyn(
            provider_icon(provider),
            IconSize::Small.px(&theme),
            provider_mark_color(provider, &theme, theme.text_muted),
        ))
        .into_any_element()
}

/// Row action that toggles a model's favorite state. A favorited model keeps
/// its accent star visible at rest; a hovered row reveals the control for
/// every model, so any of them can be pinned.
fn favorite_button(
    favorited: bool,
    theme: Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    icon_button_frame(div(), &theme, ButtonSize::Default)
        .tip(if favorited {
            tr!("model_selector.unfavorite")
        } else {
            tr!("model_selector.favorite")
        })
        .cursor_pointer()
        .group_hover("picker-row", |s| s.opacity(1.))
        .when(!favorited, |button| button.opacity(0.))
        .hover_state(&theme)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            // Keep the row's own click (model selection) from also firing.
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(icon(
            "icons/star.svg",
            ButtonSize::Default.icon_size().px(&theme),
            if favorited {
                theme.accent_text
            } else {
                theme.text_muted
            },
        ))
        .into_any_element()
}

/// Group header: an optional leading glyph and an uppercase label.
/// Used for both provider groups and the pinned
/// Favorites section, so the two read as the same register.
///
/// Laid out on Zed's inset `ListSubHeader` metrics like
/// [`crate::app::menu_header`], which has no glyph or trailing-count slot;
/// its height is [`ListMetrics::header_h`].
fn group_header(label: &str, lead: Option<AnyElement>, theme: Theme) -> impl IntoElement + use<> {
    div()
        .w_full()
        .flex_none()
        .px(list::sub_header_padding_x(&theme))
        .pb(list::sub_header_padding_bottom(&theme))
        .child(
            div()
                .h(list::sub_header_height(&theme))
                .px(list::sub_header_inset_x(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .text_size(list::SUB_HEADER_TEXT.px(&theme))
                .text_color(theme.text_muted)
                .children(lead)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                ),
        )
}

fn empty_state(kind: PickerKind, theme: Theme) -> impl IntoElement + use<> {
    let (title, hint) = match kind {
        PickerKind::Model => (
            tr!("model_selector.no_matching_models"),
            tr!("model_selector.try_a_provider_model_or_id"),
        ),
        PickerKind::Thinking => (
            tr!("model_selector.no_matching_levels"),
            tr!("model_selector.try_a_reasoning_level"),
        ),
    };
    // Zed's no-match state: one muted picker entry in the list's place.
    div().py(picker::list_padding_y(&theme)).child(
        picker_entry(div(), &theme)
            // The empty state is a two-line entry, held to the same token
            // height as a real row (and `flex_none` like the palette's).
            .h(picker::two_line_entry_height(&theme))
            .flex_none()
            .text_color(theme.text_muted)
            .child(icon(
                "icons/search.svg",
                context_menu::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        div()
                            .text_size(picker::SECONDARY_TEXT.px(&theme))
                            .child(hint),
                    ),
            ),
    )
}

/// Edge-shadow opacity for the scope rail, 0..1 on each side.
///
/// The shadow grows over the first [`RAIL_SHADOW_FADE`] pixels of travel, so it
/// reads as the rail sliding under the frame rather than a hard on/off stripe.
/// `offset_x` is GPUI's scroll offset (negative when scrolled right); `max_x`
/// is the positive maximum offset.
fn rail_shadow_opacities(offset_x: f32, max_x: f32) -> (f32, f32) {
    let max = max_x.max(0.);
    let scrolled = (-offset_x).clamp(0., max);
    let left = (scrolled / RAIL_SHADOW_FADE).clamp(0., 1.);
    let right = ((max - scrolled) / RAIL_SHADOW_FADE).clamp(0., 1.);
    (left, right)
}

/// A quiet footer keeps the result count and keyboard controls discoverable.
fn footer(count_label: String, theme: Theme) -> gpui::Div {
    div()
        .debug_selector(|| "picker-footer".to_string())
        .flex_none()
        .px(picker::search_padding_x(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .border_t_1()
        .border_color(theme.border)
        .text_size(TextSize::XSmall.px(&theme))
        .text_color(theme.text_muted)
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(div().flex_1().child(count_label))
        .child(tr!("model_selector.navigate"))
        .child(tr!("model_selector.select"))
        .child(tr!("model_selector.esc_close"))
}

fn label_column<P: IntoElement, S: IntoElement>(
    primary: P,
    secondary: S,
    selected: bool,
    highlighted: bool,
    secondary_mono: bool,
    theme: Theme,
) -> impl IntoElement + use<P, S> {
    // Fixed Comfortable line boxes: the two lines are exactly what
    // `picker::two_line_entry_height` (the row height) measures.
    let line_box = |size: TextSize| BufferLineHeight::Comfortable.resolve(size.px(&theme));
    let secondary_text = div()
        .w_full()
        .truncate()
        .text_size(picker::SECONDARY_TEXT.px(&theme))
        .line_height(line_box(picker::SECONDARY_TEXT))
        .text_color(if selected || highlighted {
            theme.text_secondary
        } else {
            theme.text_muted
        })
        .child(secondary);
    let secondary_text = if secondary_mono {
        secondary_text.font_family(theme::code_font_family())
    } else {
        secondary_text
    };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .justify_center()
        .child(
            div()
                .w_full()
                .truncate()
                .line_height(line_box(picker::TEXT))
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if selected {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .child(primary),
        )
        .child(secondary_text)
}

fn render_row(
    rows: &[Row],
    models: &[ModelEntry],
    ix: usize,
    highlighted: bool,
    this: &Entity<ModelSelector>,
    theme: Theme,
    metrics: &ListMetrics,
) -> gpui::AnyElement {
    let row = &rows[ix];

    if let Row::Header {
        provider,
        favorites,
        recents,
    } = row
    {
        // The pinned Recents/Favorites sections lead the model list; render
        // each with its glyph and the same uppercase/metrics register as a
        // provider header.
        return if *recents {
            let label = tr!("model_selector.recents").to_uppercase();
            group_header(
                &label,
                Some(
                    icon(
                        "icons/clock.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_muted,
                    )
                    .into_any_element(),
                ),
                theme,
            )
            .into_any_element()
        } else if *favorites {
            let label = tr!("model_selector.favorites").to_uppercase();
            group_header(
                &label,
                Some(
                    icon(
                        "icons/star.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_muted,
                    )
                    .into_any_element(),
                ),
                theme,
            )
            .into_any_element()
        } else {
            group_header(
                &provider_display_name(provider).to_uppercase(),
                Some(provider_mark(provider, theme)),
                theme,
            )
            .into_any_element()
        };
    }

    let selected = match row {
        Row::Level { selected, .. } | Row::Model { selected, .. } => *selected,
        Row::Header { .. } => false,
    };
    let is_favorite = match row {
        Row::Model { model_ix, .. } => {
            let model = &models[*model_ix];
            crate::favorites::contains(&model.provider, &model.id)
        }
        _ => false,
    };
    let this = this.clone();

    let row_shell = |content: gpui::Div| {
        picker_entry(
            content.id(ElementId::NamedInteger("picker-row".into(), ix as u64)),
            &theme,
        )
        .debug_selector(move || format!("picker-row-{ix}"))
        .group("picker-row")
        // Fixed so every option row measures `ListMetrics::row_h`, and
        // `flex_none` so a capped list can't shrink it below that height.
        .h(px(metrics.row_h))
        .flex_none()
        .cursor_pointer()
        .pressed_state(&theme, PressedRamp::Wash)
        // Moving the pointer over a row moves the keyboard highlight; a click
        // activates it. `on_mouse_move` (not `on_hover`) so a scroll that
        // slides rows under a stationary pointer can't hijack ↑/↓ navigation.
        .on_mouse_move({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |selector, cx| {
                    if selector.hover_highlight_blocked() {
                        return;
                    }
                    if selector.highlighted != ix {
                        selector.highlighted = ix;
                        cx.notify();
                    }
                });
            }
        })
        .on_click({
            let this = this.clone();
            move |_, window, cx| {
                this.update(cx, |selector, cx| {
                    let rows = selector.rows(&selector.last_filter);
                    selector.activate(ix, &rows, window, cx);
                });
            }
        })
        // Keyboard/hover cursor and the chosen option are separate
        // states: the cursor is a wash, the choice keeps the fill.
        .when(highlighted, |row| row.bg(theme.surface_selected))
        .when(!highlighted && selected, |row| {
            row.bg(theme.surface_selected)
        })
        .when(!highlighted && !selected, |row| row.hover_state(&theme))
    };

    match row {
        Row::Level { level, .. } => row_shell(div())
            .child(thinking_chip(level, theme))
            .child(label_column(
                thinking_display(level),
                thinking_hint(level),
                selected,
                highlighted,
                false,
                theme,
            ))
            .child(trailing_check(selected, theme))
            .into_any_element(),
        Row::Model { model_ix, .. } => {
            let model = &models[*model_ix];
            // The provider id remains inspectable in the tooltip; the context
            // window is displayed under the name, where it is easy to scan.
            let name = model.name.clone();
            let details = {
                let provider = provider_display_name(&model.provider);
                let id = model.id.trim();
                if id.is_empty() {
                    provider
                } else {
                    format!("{provider} · {id}")
                }
            };
            let context = model
                .context_window
                .map(|tokens| {
                    tr!(
                        "model_selector.context_window",
                        count = format_tokens(tokens)
                    )
                })
                .unwrap_or_default();
            let model_ix = *model_ix;
            let this_fav = this.clone();
            row_shell(div())
                .child(leading_tile(
                    theme,
                    icon_dyn(
                        provider_icon(&model.provider),
                        context_menu::ICON.px(&theme),
                        provider_mark_color(&model.provider, &theme, theme.text_muted),
                    )
                    .into_any_element(),
                ))
                .child(div().flex_1().min_w_0().tip(details).child(label_column(
                    name,
                    context,
                    selected,
                    highlighted,
                    false,
                    theme,
                )))
                .child(favorite_button(is_favorite, theme, move |_, cx| {
                    this_fav.update(cx, |selector, cx| {
                        selector.toggle_favorite(model_ix, cx);
                    });
                }))
                .child(trailing_check(selected, theme))
                .into_any_element()
        }
        Row::Header { .. } => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_rows(n: usize) -> Vec<Row> {
        (0..n)
            .map(|model_ix| Row::Model {
                model_ix,
                selected: false,
            })
            .collect()
    }

    fn metrics() -> ListMetrics {
        ListMetrics::new(PickerKind::Model, &Theme::dark())
    }

    fn offset_for(ix: usize, row_count: usize) -> f32 {
        let rows = model_rows(row_count);
        let mut scroll = ScrollHandle::new();
        ModelSelector::apply_scroll_to_row(&mut scroll, &rows, ix, &metrics());
        scroll.offset().y.into()
    }

    #[test]
    fn metrics_follow_the_picker_tokens() {
        let theme = Theme::dark();
        let m = metrics();
        assert_eq!(
            m.row_h,
            f32::from(picker::two_line_entry_height(&theme) + DynamicSpacing::Base04.px(&theme))
        );
        assert_eq!(
            ListMetrics::new(PickerKind::Thinking, &theme).row_h,
            f32::from(picker::two_line_entry_height(&theme)),
            "thinking levels keep their label/hint pair"
        );
        assert_eq!(
            m.header_h,
            f32::from(list::sub_header_height(&theme) + list::sub_header_padding_bottom(&theme))
        );
        assert_eq!(m.row_gap, f32::from(DynamicSpacing::Base01.px(&theme)));
    }

    #[test]
    fn scroll_offset_is_negative_when_scrolled_down() {
        // gpui's `set_offset` is a negative value for a scrolled-down list
        // (clamped to [-max, 0]); a positive value would be clamped to 0.
        assert!(offset_for(20, 30) < 0.);
    }

    #[test]
    fn scroll_offset_zero_at_top() {
        assert_eq!(offset_for(0, 30), 0.);
    }

    #[test]
    fn scroll_offset_centers_selected_row() {
        let m = metrics();
        let offset = offset_for(20, 30);
        // Row 20's top sits below the centered 6-row viewport; the scrolled
        // offset places the row fully within it.
        let row_top = ModelSelector::row_top(&model_rows(30), 20, &m);
        let viewport_h = m.list_max_h();
        assert!(row_top + offset >= 0.);
        assert!(row_top + offset <= viewport_h - m.row_h + 0.5);
    }

    #[test]
    fn headers_shift_scroll_math() {
        let mut rows = vec![Row::Header {
            provider: "openai".into(),
            favorites: false,
            recents: false,
        }];
        rows.extend(model_rows(5));
        let m = metrics();
        // A header, then two model rows before index 3, each followed by one
        // gap.
        let top = ModelSelector::row_top(&rows, 3, &m);
        assert_eq!(top, m.header_h + 2. * m.row_h + 3. * m.row_gap);
    }

    #[test]
    fn rows_share_one_tight_rhythm() {
        let rows = model_rows(3);
        let m = metrics();
        assert_eq!(ModelSelector::row_top(&rows, 1, &m), m.row_h + m.row_gap);
        assert_eq!(
            ModelSelector::row_top(&rows, 2, &m),
            2. * m.row_h + 2. * m.row_gap
        );
        // Content height counts the two gaps between the three rows.
        assert_eq!(
            ModelSelector::content_height(&rows, &m),
            3. * m.row_h + 2. * m.row_gap
        );
    }

    fn entry(id: &str, name: &str, provider: &str, context_window: Option<u64>) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            name: name.into(),
            provider: provider.into(),
            context_window,
            thinking_levels: Vec::new(),
        }
    }

    fn providers(rows: &[Row]) -> Vec<&str> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Header { provider, .. } => Some(provider.as_str()),
                _ => None,
            })
            .collect()
    }

    fn no_favorites() -> Favorites {
        Favorites::default()
    }

    fn no_recents() -> Recents {
        Recents::default()
    }

    #[test]
    fn models_group_by_provider_with_headers() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "openai", None),
            entry("c", "C", "anthropic", None),
        ];
        let favs = no_favorites();
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            Some(2),
            &Scope::All,
            &favs,
            &no_recents(),
            "",
        );
        assert_eq!(providers(&rows), vec!["openai", "anthropic"]);
        assert!(matches!(rows[0], Row::Header { .. }));
        assert!(matches!(rows[1], Row::Model { model_ix: 0, .. }));
        // The selected model (catalog ix 2) is the third model, row 4.
        assert_eq!(ModelSelector::selected_row_index(&rows), Some(4));
        assert_eq!(ModelSelector::first_selectable(&rows), Some(1));
    }

    #[test]
    fn scope_counts_only_models_in_the_catalog() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "openai", None),
            entry("c", "C", "anthropic", None),
        ];
        let favorites = Favorites::from_pairs(&[("openai", "a"), ("missing", "x")]);
        let recents = Recents::from_pairs(&[("anthropic", "c"), ("missing", "x")]);
        let count = |scope| ModelSelector::scope_count(&scope, &models, &favorites, &recents);
        assert_eq!(count(Scope::All), 3);
        assert_eq!(count(Scope::Provider("openai".into())), 2);
        assert_eq!(count(Scope::Favorites), 1);
        assert_eq!(count(Scope::Recents), 1);
    }

    #[test]
    fn scoped_provider_shows_models_without_repeating_its_header() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "openai", None),
            entry("c", "C", "anthropic", None),
        ];
        let favs = no_favorites();
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::Provider("openai".into()),
            &favs,
            &no_recents(),
            "",
        );
        assert!(providers(&rows).is_empty());
        assert_eq!(rows.len(), 2);
        assert_eq!(ModelSelector::first_selectable(&rows), Some(0));
    }

    #[test]
    fn provider_scope_pins_its_own_recents() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "openai", None),
            entry("c", "C", "anthropic", None),
        ];
        let favs = no_favorites();
        let recents = Recents::from_pairs(&[("anthropic", "c"), ("openai", "b")]);
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::Provider("openai".into()),
            &favs,
            &recents,
            "",
        );
        // The provider's own recent (b) pins at the top; the other provider's
        // recent (c) is out of scope and never appears.
        assert!(matches!(rows[0], Row::Header { recents: true, .. }));
        let model_ixs: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Model { model_ix, .. } => Some(*model_ix),
                _ => None,
            })
            .collect();
        assert_eq!(model_ixs, vec![1, 0]);
    }

    #[test]
    fn favorites_pin_a_section_above_providers() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "anthropic", None),
            entry("c", "C", "google", None),
        ];
        let favs = Favorites::from_pairs(&[("google", "c")]);
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::All,
            &favs,
            &no_recents(),
            "",
        );
        // Favorites header first, then its model, then the provider groups.
        assert!(matches!(
            rows[0],
            Row::Header {
                favorites: true,
                ..
            }
        ));
        assert!(matches!(rows[1], Row::Model { model_ix: 2, .. }));
        // The favorited model is not repeated under its provider group.
        let model_ixs: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Model { model_ix, .. } => Some(*model_ix),
                _ => None,
            })
            .collect();
        assert_eq!(model_ixs, vec![2, 0, 1]);
    }

    #[test]
    fn favorites_scope_shows_only_favorites() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "anthropic", None),
        ];
        let favs = Favorites::from_pairs(&[("anthropic", "b")]);
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::Favorites,
            &favs,
            &no_recents(),
            "",
        );
        assert!(providers(&rows).is_empty());
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0], Row::Model { model_ix: 1, .. }));
    }

    #[test]
    fn recents_pin_a_section_above_favorites() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "anthropic", None),
            entry("c", "C", "google", None),
        ];
        let favs = Favorites::from_pairs(&[("google", "c")]);
        let recents = Recents::from_pairs(&[("anthropic", "b")]);
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::All,
            &favs,
            &recents,
            "",
        );
        // Recents lead, then favorites, then the provider groups.
        assert!(matches!(rows[0], Row::Header { recents: true, .. }));
        assert!(matches!(rows[1], Row::Model { model_ix: 1, .. }));
        assert!(matches!(
            rows[2],
            Row::Header {
                favorites: true,
                ..
            }
        ));
        assert!(matches!(rows[3], Row::Model { model_ix: 2, .. }));
        // Neither pinned model repeats under its provider group.
        let model_ixs: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Model { model_ix, .. } => Some(*model_ix),
                _ => None,
            })
            .collect();
        assert_eq!(model_ixs, vec![1, 2, 0]);
    }

    #[test]
    fn a_model_that_is_both_recent_and_favorite_pins_once() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "anthropic", None),
        ];
        let favs = Favorites::from_pairs(&[("openai", "a")]);
        let recents = Recents::from_pairs(&[("openai", "a")]);
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::All,
            &favs,
            &recents,
            "",
        );
        // Recent wins: a Recents pinned header only, no Favorites pinned
        // header, and the model appears once (provider headers are ignored).
        let pinned_headers: Vec<(bool, bool)> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Header {
                    favorites, recents, ..
                } if *recents || *favorites => Some((*recents, *favorites)),
                _ => None,
            })
            .collect();
        assert_eq!(pinned_headers, vec![(true, false)]);
        let model_ixs: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Model { model_ix, .. } => Some(*model_ix),
                _ => None,
            })
            .collect();
        assert_eq!(model_ixs, vec![0, 1]);
    }

    #[test]
    fn recents_scope_lists_most_recent_first() {
        let models = vec![
            entry("a", "A", "openai", None),
            entry("b", "B", "anthropic", None),
            entry("c", "C", "google", None),
        ];
        let favs = no_favorites();
        let recents = Recents::from_pairs(&[("google", "c"), ("openai", "a")]);
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::Recents,
            &favs,
            &recents,
            "",
        );
        // Flat, MRU order (c then a), never catalog order, and no headers.
        assert!(providers(&rows).is_empty());
        let model_ixs: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Model { model_ix, .. } => Some(*model_ix),
                _ => None,
            })
            .collect();
        assert_eq!(model_ixs, vec![2, 0]);
    }

    #[test]
    fn filter_drops_empty_providers() {
        let models = vec![
            entry("a", "Alpha", "openai", None),
            entry("b", "Beta", "anthropic", None),
        ];
        let favs = no_favorites();
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::All,
            &favs,
            &no_recents(),
            "alpha",
        );
        assert_eq!(providers(&rows), vec!["openai"]);
    }

    /// The provider chips are gone; typing a provider in the search box still
    /// narrows the catalog to that provider, so browsing by provider remains a
    /// one-word action.
    #[test]
    fn search_matches_provider_names() {
        let models = vec![
            entry("a", "Alpha", "openai", None),
            entry("b", "Beta", "anthropic", None),
        ];
        let rows = ModelSelector::rows_for(
            PickerKind::Model,
            &models,
            &[],
            "low",
            None,
            &Scope::All,
            &no_favorites(),
            &no_recents(),
            "anthropic",
        );
        assert_eq!(providers(&rows), vec!["anthropic"]);
        let model_ixs: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Model { model_ix, .. } => Some(*model_ix),
                _ => None,
            })
            .collect();
        assert_eq!(model_ixs, vec![1]);
    }

    /// Every built-in provider needs a mapped mark and an embedded asset —
    /// the whitelist and the `assets/icons/providers/` directory must not
    /// drift apart (a missing entry silently paints the cloud fallback).
    #[test]
    fn every_builtin_provider_has_an_embedded_brand_mark() {
        use gpui::AssetSource as _;
        for builtin in crate::providers::BUILTIN_PROVIDERS {
            let path = provider_icon(builtin.id);
            assert_ne!(
                path.as_ref(),
                "icons/cloud.svg",
                "{} has no KNOWN entry",
                builtin.id
            );
            assert!(
                crate::assets::Assets.load(&path).unwrap().is_some(),
                "{path} is not embedded"
            );
        }
    }

    #[test]
    fn custom_provider_marks_resolve_to_embedded_assets() {
        use gpui::AssetSource as _;
        for id in ["manifest", "llama.cpp", "clinepass"] {
            let path = provider_icon(id);
            assert_eq!(path.as_ref(), format!("icons/providers/{id}.svg"));
            assert!(
                crate::assets::Assets.load(&path).unwrap().is_some(),
                "{path} is not embedded"
            );
        }
    }

    #[test]
    fn aliased_provider_ids_reuse_their_brand_mark() {
        use gpui::AssetSource as _;
        for (id, mark) in [("ollama-cloud", "ollama"), (".manifest", "manifest")] {
            let path = provider_icon(id);
            assert_eq!(path.as_ref(), format!("icons/providers/{mark}.svg"));
            assert!(
                crate::assets::Assets.load(&path).unwrap().is_some(),
                "{path} is not embedded"
            );
        }
        assert_eq!(
            provider_icon("ollama").as_ref(),
            "icons/providers/ollama.svg"
        );
        assert_eq!(
            provider_icon("manifest").as_ref(),
            "icons/providers/manifest.svg"
        );
    }

    #[test]
    fn unknown_providers_fall_back_to_the_cloud_glyph() {
        assert_eq!(provider_icon("my-gateway").as_ref(), "icons/cloud.svg");
    }

    /// A capped list must scroll, not squeeze its rows: without `flex_none`
    /// the flex column shrinks each row toward its content height. Every model
    /// row keeps the single-line picker token instead.
    #[gpui::test]
    fn capped_list_keeps_rows_at_the_model_token_height(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;

        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let models: Vec<ModelEntry> = (0..12)
            .map(|ix| {
                entry(
                    &format!("m{ix}"),
                    &format!("Model {ix}"),
                    "opencode-go",
                    Some(1_000_000),
                )
            })
            .collect();
        let selector = cx.update(|window, cx| {
            let selector = cx.new(|cx| {
                ModelSelector::new(
                    PickerKind::Model,
                    models.clone(),
                    Vec::new(),
                    "Model 0".into(),
                    "m0".into(),
                    "opencode-go".into(),
                    "low".into(),
                    Box::new(|_, _, _, _| {}),
                    Box::new(|_, _, _| {}),
                    Box::new(|_, _, _| {}),
                    cx,
                )
            });
            window.focus(&selector.read(cx).focus_handle(cx));
            selector
        });
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(360.), px(700.)),
            |_, _| selector.clone(),
        );

        let theme = Theme::for_id(ThemeId::Orbit);
        let expected = px(ListMetrics::new(PickerKind::Model, &theme).row_h);
        // Row 0 is the provider header; the first model row is row 1.
        for (ix, selector) in ["picker-row-1", "picker-row-6", "picker-row-12"]
            .into_iter()
            .enumerate()
        {
            let row = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("row {ix} laid out"));
            assert_eq!(row.size.height, expected, "row {ix} kept its token height");
        }
    }

    /// Bind the picker arrows exactly as `main::bind_keys` does: the
    /// `Composer` arrows first, the `Picker` arrows after. Both ride on the
    /// same dispatch node, and gpui breaks the depth tie by registration
    /// order, so the later `Picker` bindings must win.
    fn bind_picker_arrows(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;
        use crate::{Down, PickerSelectNext, PickerSelectPrev, Up};
        use gpui::KeyBinding;

        cx.update(|cx| {
            cx.set_global(Theme::for_id(ThemeId::Orbit));
            cx.bind_keys([
                KeyBinding::new("up", Up, Some("Composer")),
                KeyBinding::new("down", Down, Some("Composer")),
                KeyBinding::new("up", PickerSelectPrev, Some("Picker")),
                KeyBinding::new("down", PickerSelectNext, Some("Picker")),
            ]);
        });
    }

    /// Thirty models under one provider, so the list holds a header and a
    /// scrollable run of option rows.
    fn test_models() -> Vec<ModelEntry> {
        (0..30)
            .map(|ix| {
                entry(
                    &format!("m{ix}"),
                    &format!("Model {ix}"),
                    "opencode-go",
                    Some(1_000_000),
                )
            })
            .collect()
    }

    /// A 30-model selector with its filter focused, ready to draw.
    fn open_test_selector(cx: &mut gpui::VisualTestContext) -> Entity<ModelSelector> {
        let models = test_models();
        cx.update(|window, cx| {
            let selector = cx.new(|cx| {
                ModelSelector::new(
                    PickerKind::Model,
                    models.clone(),
                    Vec::new(),
                    "Model 0".into(),
                    "m0".into(),
                    "opencode-go".into(),
                    "low".into(),
                    Box::new(|_, _, _, _| {}),
                    Box::new(|_, _, _| {}),
                    Box::new(|_, _, _| {}),
                    cx,
                )
            });
            window.focus(&selector.read(cx).focus_handle(cx));
            selector
        })
    }

    /// ↓/↑ pressed right after the picker opens must survive the open-time
    /// "pin the highlight to the selected model". That pin used to re-run on
    /// every render for the first 400ms (the hover-suppression window), so a
    /// ↓ in that window was silently snapped back to the active model.
    #[gpui::test]
    fn arrow_keys_survive_the_open_pin(cx: &mut gpui::TestAppContext) {
        bind_picker_arrows(cx);
        let cx = cx.add_empty_window();
        let selector = open_test_selector(cx);

        let draw = |cx: &mut gpui::VisualTestContext| {
            let _ = cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(360.), px(700.)),
                |_, _| selector.clone(),
            );
        };
        draw(cx);
        // Row 0 is the provider header, so the first selectable row is 1.
        assert_eq!(cx.update(|_, cx| selector.read(cx).highlighted), 1);

        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            2,
            "↓ sticks right after opening"
        );
        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            3,
            "↓ keeps moving"
        );
    }

    /// The rail's edge shadows are a function of scroll distance: they grow
    /// over the first 28px, are full at either end's opposite edge, and are
    /// gone at the end they belong to.
    #[test]
    fn rail_shadows_grow_incrementally_and_clear_at_the_edges() {
        assert_eq!(rail_shadow_opacities(0., 0.), (0., 0.));
        assert_eq!(rail_shadow_opacities(0., 100.), (0., 1.));
        let (left, right) = rail_shadow_opacities(-14., 100.);
        assert!(left > 0. && left < 1., "left fades in: {left}");
        assert_eq!(right, 1., "right stays full while far from its end");
        assert_eq!(rail_shadow_opacities(-100., 100.), (1., 0.));
        let (_, right) = rail_shadow_opacities(-90., 100.);
        assert!(right > 0. && right < 0.5, "right fades out: {right}");
        // A clamp on either side cannot produce a negative opacity.
        assert_eq!(rail_shadow_opacities(-200., 100.), (1., 0.));
        assert_eq!(rail_shadow_opacities(50., 100.), (0., 1.));
    }

    /// Selecting a provider scrolls that chip to the rail's centre. The
    /// entries are All/Recents/Favorites when present, then providers, so the
    /// index is offset by however many mode tabs the real config supplies.
    #[gpui::test]
    fn selecting_a_provider_centres_its_chip(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let models: Vec<ModelEntry> = (0..12)
            .map(|ix| {
                entry(
                    &format!("m{ix}"),
                    &format!("Model {ix}"),
                    &format!("provider-{ix}"),
                    Some(1_000_000),
                )
            })
            .collect();
        let cx = cx.add_empty_window();
        let selector = cx.update(|window, cx| {
            let selector = cx.new(|cx| {
                ModelSelector::new(
                    PickerKind::Model,
                    models.clone(),
                    Vec::new(),
                    "Model 0".into(),
                    "m0".into(),
                    "provider-0".into(),
                    "low".into(),
                    Box::new(|_, _, _, _| {}),
                    Box::new(|_, _, _| {}),
                    Box::new(|_, _, _| {}),
                    cx,
                )
            });
            window.focus(&selector.read(cx).focus_handle(cx));
            selector
        });
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(360.), px(700.)),
            |_, _| selector.clone(),
        );

        let base = 1
            + usize::from(!crate::recent_models::all().is_empty())
            + usize::from(!crate::favorites::all().is_empty());
        let ix = base + 8;
        let (bounds, viewport, max) = cx.update(|_, cx| {
            let selector = selector.read(cx);
            (
                selector
                    .scope_scroll
                    .bounds_for_item(ix)
                    .expect("the provider chip laid out"),
                selector.scope_scroll.bounds(),
                selector.scope_scroll.max_offset().width,
            )
        });
        assert!(max > px(0.), "twelve providers overflow a 360px rail");
        cx.update(|_, cx| {
            selector.update(cx, |selector, cx| selector.center_scope_chip(ix, cx));
        });
        let offset = cx.update(|_, cx| selector.read(cx).scope_scroll.offset().x);
        assert!(offset < px(0.), "the rail scrolled right: {offset:?}");
        let painted_center = bounds.center().x + offset;
        assert!(
            (painted_center - viewport.center().x).abs() <= px(1.),
            "chip centre {} vs rail centre {}",
            painted_center,
            viewport.center().x
        );
    }

    /// The app re-syncs the catalog on every `get_state` response, not only on
    /// a real change. An unchanged sync must not re-pin the highlight to the
    /// active model — that would undo an ↑/↓ pressed while the popup is open.
    #[gpui::test]
    fn an_unchanged_catalog_sync_keeps_the_highlight(cx: &mut gpui::TestAppContext) {
        bind_picker_arrows(cx);
        let cx = cx.add_empty_window();
        let selector = open_test_selector(cx);
        let draw = |cx: &mut gpui::VisualTestContext| {
            let _ = cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(360.), px(700.)),
                |_, _| selector.clone(),
            );
        };
        draw(cx);

        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(cx.update(|_, cx| selector.read(cx).highlighted), 2);

        // pi re-reports exactly the same catalog while the popup is open.
        cx.update(|_, cx| {
            selector.update(cx, |selector, cx| {
                selector.set_catalog(
                    test_models(),
                    Vec::new(),
                    "Model 0".into(),
                    "m0".into(),
                    "opencode-go".into(),
                    "low".into(),
                    cx,
                );
            });
        });
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            2,
            "an unchanged catalog sync must not snap the highlight back"
        );
    }

    /// A ↑/↓ that lands while the open-pin is still armed (before the popup's
    /// first frame, or right after a scope change / catalog refresh re-armed
    /// it) must win over the pin — otherwise the next render snaps the cursor
    /// back to the active model and the key looks dead.
    #[gpui::test]
    fn a_pending_open_pin_does_not_undo_a_press(cx: &mut gpui::TestAppContext) {
        bind_picker_arrows(cx);
        let cx = cx.add_empty_window();
        let selector = open_test_selector(cx);
        let draw = |cx: &mut gpui::VisualTestContext| {
            let _ = cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(360.), px(700.)),
                |_, _| selector.clone(),
            );
        };
        draw(cx);
        assert_eq!(cx.update(|_, cx| selector.read(cx).highlighted), 1);

        // Re-arm the pin the way a scope change or a catalog refresh does…
        cx.update(|_, cx| selector.update(cx, |selector, _| selector.needs_scroll = true));
        // …and press ↓ before that frame lands.
        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            2,
            "the press wins over the still-armed pin"
        );
    }

    /// The popup ships through `anchored` + `deferred` above the composer, not
    /// as a bare entity. The arrows must resolve to the `Picker` bindings
    /// there too, or ↑/↓ move the composer caret and the list never moves.
    #[gpui::test]
    fn arrow_keys_resolve_inside_the_anchored_popup(cx: &mut gpui::TestAppContext) {
        use gpui::{anchored, deferred, AnchoredPositionMode, Corner};

        struct Host(Entity<ModelSelector>);
        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().relative().size_full().child(
                    anchored()
                        .position_mode(AnchoredPositionMode::Local)
                        .anchor(Corner::BottomLeft)
                        .offset(point(px(0.), -px(8.)))
                        .snap_to_window_with_margin(px(8.))
                        .child(deferred(self.0.clone())),
                )
            }
        }

        bind_picker_arrows(cx);
        let cx = cx.add_empty_window();
        let selector = open_test_selector(cx);
        let host = cx.update(|_, cx| cx.new(|_| Host(selector.clone())));

        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(900.), px(700.)),
            |_, _| host.clone(),
        );
        assert_eq!(cx.update(|_, cx| selector.read(cx).highlighted), 1);
        cx.simulate_keystrokes("down");
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            2,
            "↓ resolves to the Picker binding inside the anchored popup"
        );
        cx.simulate_keystrokes("up");
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            1,
            "↑ resolves too"
        );
    }

    /// The real keymap, not a hand-rolled subset. `bind_keys` registers
    /// hundreds of bindings, and any later one that also matches the picker's
    /// context stack would win over `PickerSelectNext`/`Prev`.
    #[gpui::test]
    fn arrow_keys_work_with_the_real_keymap(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;

        cx.update(|cx| {
            cx.set_global(Theme::for_id(ThemeId::Orbit));
            crate::bind_keys(cx);
        });
        let cx = cx.add_empty_window();
        let selector = open_test_selector(cx);
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(360.), px(700.)),
            |_, _| selector.clone(),
        );
        assert_eq!(cx.update(|_, cx| selector.read(cx).highlighted), 1);
        cx.simulate_keystrokes("down");
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            2,
            "↓ with the real keymap"
        );
        cx.simulate_keystrokes("up");
        assert_eq!(
            cx.update(|_, cx| selector.read(cx).highlighted),
            1,
            "↑ with the real keymap"
        );
    }

    /// A thinking selector with the built-in levels, muted callbacks, focused.
    fn open_thinking_selector(cx: &mut gpui::VisualTestContext) -> Entity<ModelSelector> {
        cx.update(|window, cx| {
            let levels = [
                "off", "minimal", "low", "medium", "high", "xhigh", "max", "auto",
            ]
            .iter()
            .map(|level| level.to_string())
            .collect::<Vec<_>>();
            let selector = cx.new(|cx| {
                ModelSelector::new(
                    PickerKind::Thinking,
                    Vec::new(),
                    levels,
                    String::new(),
                    String::new(),
                    String::new(),
                    "low".into(),
                    Box::new(|_, _, _, _| {}),
                    Box::new(|_, _, _| {}),
                    Box::new(|_, _, _| {}),
                    cx,
                )
            });
            window.focus(&selector.read(cx).focus_handle(cx));
            selector
        })
    }

    /// The popover's authored size and rhythm: each picker keeps its fixed
    /// width; both pickers use two-line rows, with extra room for models.
    /// Consecutive rows sit one `Base01` gap apart, and the footer stays compact.
    #[gpui::test]
    fn popover_width_and_row_rhythm_are_stable(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let theme = Theme::for_id(ThemeId::Orbit);
        let metrics = ListMetrics::new(PickerKind::Model, &theme);

        let cx = cx.add_empty_window();
        let model = open_test_selector(cx);
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(900.), px(900.)),
            |_, _| model.clone(),
        );
        let model_surface = cx.debug_bounds("picker-surface").expect("model surface");
        assert_eq!(model_surface.size.width, px(MODEL_POPOVER_W));
        let model_footer = cx.debug_bounds("picker-footer").expect("model footer");
        assert!(
            f32::from(model_footer.size.height) < 40.,
            "the footer is a single quiet count: {:?}",
            model_footer.size.height
        );
        let row_1 = cx.debug_bounds("picker-row-1").expect("first model row");
        let row_2 = cx.debug_bounds("picker-row-2").expect("second model row");
        assert_eq!(row_1.size.height, px(metrics.row_h));
        assert_eq!(
            f32::from(row_2.top()) - f32::from(row_1.top()),
            metrics.row_h + metrics.row_gap
        );

        let thinking = open_thinking_selector(cx);
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(900.), px(900.)),
            |_, _| thinking.clone(),
        );
        let thinking_surface = cx.debug_bounds("picker-surface").expect("thinking surface");
        assert_eq!(thinking_surface.size.width, px(THINKING_POPOVER_W));
        let thinking_footer = cx.debug_bounds("picker-footer").expect("thinking footer");
        assert!(f32::from(thinking_footer.size.height) < 40.);
        let thinking_metrics = ListMetrics::new(PickerKind::Thinking, &theme);
        let level_0 = cx.debug_bounds("picker-row-0").expect("first level row");
        let level_1 = cx.debug_bounds("picker-row-1").expect("second level row");
        assert_eq!(level_0.size.height, px(thinking_metrics.row_h));
        assert_eq!(
            f32::from(level_1.top()) - f32::from(level_0.top()),
            thinking_metrics.row_h + thinking_metrics.row_gap
        );
    }
}
