//! Persisted workbench layout — the panel sizes, which panels are open, and
//! which workspace groups are collapsed, so the arrangement survives a restart.
//!
//! Orbit-owned (`~/.orbit-pi/layout.json`), like the pins and favorites
//! stores. A panel records its live size here as it changes; the heartbeat
//! flushes once the size holds still ([`SETTLE`]), so a drag costs one write
//! rather than one per pixel. Readers run once, when each panel is built.
//!
//! # Presets and the custom layout
//!
//! A preset is a **starting point, not a mode**. Applying one writes a new
//! arrangement, but the arrangement the user built by hand is kept in
//! `custom` first, so "Back to custom" restores it. Touching any panel after a
//! preset clears the preset — the arrangement is the user's again, and there is
//! nothing left to go back to.
//!
//! # Schema tolerance
//!
//! Every field is read independently: a missing key leaves the built-in
//! default, an unknown key is ignored, and a value of the wrong type or outside
//! [`MIN_PX`]..=[`MAX_PX`] is dropped. One corrupt field must not wipe the rest
//! of a workspace.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// How long a size must hold still before it is written to disk.
const SETTLE: Duration = Duration::from_millis(500);

/// Sanity bounds for a stored size, px. Guards against a corrupted file;
/// each panel still clamps to its own min/max when it applies the value.
const MIN_PX: f32 = 80.;
const MAX_PX: f32 = 4000.;

/// A named workspace shape. A preset is a starting point, not a mode: the user
/// can drag any panel afterwards and the preset is not re-applied.
///
/// The three shapes answer the three ways this workbench is used: writing with
/// nothing in the way, building with files and a shell, and reading a change
/// with the review pane open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPreset {
    /// Sidebar, the workspace, and nothing else.
    Focus,
    /// Sidebar, the project tree, the workspace, and the terminal.
    Developer,
    /// Sidebar and the review pane, wide.
    Review,
}

impl LayoutPreset {
    pub const ALL: [Self; 3] = [Self::Focus, Self::Developer, Self::Review];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Developer => "developer",
            Self::Review => "review",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "focus" => Some(Self::Focus),
            "developer" => Some(Self::Developer),
            "review" => Some(Self::Review),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Focus => "sidebar.layout_focus",
            Self::Developer => "sidebar.layout_developer",
            Self::Review => "sidebar.layout_review",
        }
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            Self::Focus => "icons/window-maximize.svg",
            Self::Developer => "icons/terminal.svg",
            Self::Review => "icons/side-by-side.svg",
        }
    }

    /// The sizes and open panels this preset applies.
    pub fn shape(self) -> PresetShape {
        match self {
            Self::Focus => PresetShape {
                sidebar: 240.,
                explorer: 280.,
                sidepane: 420.,
                terminal: 220.,
                project_panel: false,
                side_pane: false,
                terminal_panel: false,
            },
            Self::Developer => PresetShape {
                sidebar: 248.,
                explorer: 300.,
                sidepane: 420.,
                terminal: 260.,
                project_panel: true,
                side_pane: false,
                terminal_panel: true,
            },
            Self::Review => PresetShape {
                sidebar: 220.,
                explorer: 280.,
                sidepane: 560.,
                terminal: 240.,
                project_panel: false,
                side_pane: true,
                terminal_panel: false,
            },
        }
    }
}

/// A preset's resolved sizes (px) and which panels it opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetShape {
    pub sidebar: f32,
    pub explorer: f32,
    pub sidepane: f32,
    pub terminal: f32,
    pub project_panel: bool,
    pub side_pane: bool,
    pub terminal_panel: bool,
}

impl PresetShape {
    /// Every size inside the range the store accepts, so a preset can never
    /// persist a value the loader would drop.
    pub fn is_storable(self) -> bool {
        [self.sidebar, self.explorer, self.sidepane, self.terminal]
            .iter()
            .all(|px| px.is_finite() && (MIN_PX..=MAX_PX).contains(px))
    }
}

/// The arrangement, every field optional: `None` means the user has never set
/// it, so the surface keeps its built-in default.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub sidebar_width: Option<f32>,
    pub sidepane_width: Option<f32>,
    pub explorer_width: Option<f32>,
    pub terminal_height: Option<f32>,
    /// Width of the GitHub list pane inside the Issues/Pulls split view.
    pub github_list_width: Option<f32>,
    pub sidebar_visible: Option<bool>,
    pub project_panel: Option<bool>,
    pub side_pane: Option<bool>,
    pub terminal_panel: Option<bool>,
    /// Workspace labels whose group header is collapsed, in no particular
    /// order — a set, stored as a list.
    pub collapsed_workspaces: Vec<String>,
    /// How many extra sessions each workspace group has revealed past its cap
    /// ("Show more" presses). A group absent here is at its cap.
    pub expanded_groups: Vec<(String, usize)>,
}

impl Layout {
    /// Parse a stored payload, dropping any field that is missing or bad.
    fn from_value(value: &Value) -> Self {
        Self {
            sidebar_width: field(value, "sidebar_width"),
            sidepane_width: field(value, "sidepane_width"),
            explorer_width: field(value, "explorer_width"),
            terminal_height: field(value, "terminal_height"),
            github_list_width: field(value, "github_list_width"),
            sidebar_visible: flag(value, "sidebar_visible"),
            project_panel: flag(value, "project_panel"),
            side_pane: flag(value, "side_pane"),
            terminal_panel: flag(value, "terminal_panel"),
            collapsed_workspaces: labels(value, "collapsed_workspaces"),
            expanded_groups: counts(value, "expanded_groups"),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "sidebar_width": self.sidebar_width,
            "sidepane_width": self.sidepane_width,
            "explorer_width": self.explorer_width,
            "terminal_height": self.terminal_height,
            "github_list_width": self.github_list_width,
            "sidebar_visible": self.sidebar_visible,
            "project_panel": self.project_panel,
            "side_pane": self.side_pane,
            "terminal_panel": self.terminal_panel,
            "collapsed_workspaces": self.collapsed_workspaces,
            "expanded_groups": self
                .expanded_groups
                .iter()
                .cloned()
                .collect::<std::collections::BTreeMap<_, _>>(),
        })
    }

    /// Adopt a preset's sizes and open panels.
    fn adopt(&mut self, preset: LayoutPreset) {
        let shape = preset.shape();
        self.sidebar_width = Some(shape.sidebar);
        self.explorer_width = Some(shape.explorer);
        self.sidepane_width = Some(shape.sidepane);
        self.terminal_height = Some(shape.terminal);
        self.project_panel = Some(shape.project_panel);
        self.side_pane = Some(shape.side_pane);
        self.terminal_panel = Some(shape.terminal_panel);
    }
}

/// The live arrangement plus the preset/custom pair that makes a preset
/// reversible.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Arrangement {
    pub layout: Layout,
    /// The arrangement the user built, saved when a preset was applied.
    pub custom: Option<Layout>,
    pub active_preset: Option<LayoutPreset>,
    /// Arrangements the user named. A named layout is theirs by definition, so
    /// applying one never overwrites `custom`.
    pub named: Vec<NamedLayout>,
}

/// One arrangement saved under a name.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedLayout {
    pub name: String,
    pub layout: Layout,
}

/// A user keeps a handful of shapes at most; past this the menu stops being a
/// menu.
pub const MAX_NAMED_LAYOUTS: usize = 8;

impl Arrangement {
    /// A user change: the arrangement is theirs again, so the preset stops
    /// being active and the custom slot follows the new arrangement.
    fn record(&mut self, set: impl FnOnce(&mut Layout)) -> bool {
        let before = self.layout.clone();
        set(&mut self.layout);
        if self.layout == before {
            return false;
        }
        self.active_preset = None;
        if self.custom.is_some() {
            self.custom = Some(self.layout.clone());
        }
        true
    }

    fn apply_preset(&mut self, preset: LayoutPreset) {
        if self.active_preset.is_none() {
            self.custom = Some(self.layout.clone());
        }
        self.layout.adopt(preset);
        self.active_preset = Some(preset);
    }

    /// Restore the saved custom arrangement. False when there is none.
    fn restore_custom(&mut self) -> bool {
        let Some(custom) = self.custom.clone() else {
            return false;
        };
        self.layout = custom;
        self.active_preset = None;
        true
    }

    fn from_value(value: &Value) -> Self {
        Self {
            named: value
                .get("named")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            let name = item.get("name")?.as_str()?.trim();
                            (!name.is_empty()).then(|| NamedLayout {
                                name: name.to_owned(),
                                layout: Layout::from_value(item),
                            })
                        })
                        .take(MAX_NAMED_LAYOUTS)
                        .collect()
                })
                .unwrap_or_default(),
            layout: Layout::from_value(value),
            custom: value
                .get("custom")
                .filter(|v| v.is_object())
                .map(Layout::from_value),
            active_preset: value
                .get("active_preset")
                .and_then(Value::as_str)
                .and_then(LayoutPreset::parse),
        }
    }

    fn to_value(&self) -> Value {
        let mut value = self.layout.to_value();
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "custom".into(),
                match &self.custom {
                    Some(custom) => custom.to_value(),
                    None => Value::Null,
                },
            );
            object.insert(
                "named".into(),
                Value::Array(
                    self.named
                        .iter()
                        .map(|named| {
                            let mut value = named.layout.to_value();
                            if let Some(object) = value.as_object_mut() {
                                object.insert("name".into(), Value::String(named.name.clone()));
                            }
                            value
                        })
                        .collect(),
                ),
            );
            object.insert(
                "active_preset".into(),
                match self.active_preset {
                    Some(preset) => Value::String(preset.as_str().into()),
                    None => Value::Null,
                },
            );
        }
        value
    }
}

/// One stored size, rejected when missing, non-finite, or out of bounds.
fn field(value: &Value, key: &str) -> Option<f32> {
    let px = value.get(key).and_then(Value::as_f64)? as f32;
    (px.is_finite() && (MIN_PX..=MAX_PX).contains(&px)).then_some(px)
}

/// One stored flag. Anything that is not a JSON boolean is dropped rather than
/// coerced: "1" is a typo, not an intent.
fn flag(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// A stored label → count map. Entries with a non-string key or a non-positive
/// count are dropped: a group with zero extra rows is at its cap.
fn counts(value: &Value, key: &str) -> Vec<(String, usize)> {
    value
        .get(key)
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter(|(label, _)| !label.trim().is_empty())
                .filter_map(|(label, count)| {
                    count
                        .as_u64()
                        .filter(|n| *n > 0)
                        .map(|n| (label.clone(), n as usize))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A stored list of labels, with non-strings and blanks dropped.
fn labels(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|label| !label.trim().is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the persisted payload. Exposed for the phase-9 baseline measurement,
/// which times the restore path a launch pays for.
#[cfg(test)]
pub(crate) fn from_json_for_baseline(raw: &str) -> Layout {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => Layout::from_value(&value),
        Err(_) => Layout::default(),
    }
}

struct Store {
    arrangement: Arrangement,
    /// When the arrangement last changed; the flush waits out [`SETTLE`].
    changed_at: Option<Instant>,
}

static STORE: RwLock<Option<Store>> = RwLock::new(None);

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut guard = STORE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(guard.get_or_insert_with(|| Store {
        arrangement: Arrangement::from_value(
            &std::fs::read_to_string(persist_path())
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .unwrap_or(Value::Null),
        ),
        changed_at: None,
    }))
}

/// The stored size for each panel, if the user has ever set one.
pub fn sidebar_width() -> Option<f32> {
    with_store(|store| store.arrangement.layout.sidebar_width)
}
pub fn sidepane_width() -> Option<f32> {
    with_store(|store| store.arrangement.layout.sidepane_width)
}
pub fn explorer_width() -> Option<f32> {
    with_store(|store| store.arrangement.layout.explorer_width)
}
pub fn terminal_height() -> Option<f32> {
    with_store(|store| store.arrangement.layout.terminal_height)
}

/// The GitHub list pane's width, if the user has ever dragged its divider.
pub fn github_list_width() -> Option<f32> {
    with_store(|store| store.arrangement.layout.github_list_width)
}

/// The stored arrangement, for the launch-time restore.
pub fn arrangement() -> Arrangement {
    with_store(|store| store.arrangement.clone())
}

/// Apply `set` to the arrangement and arm the settle timer only when the value
/// actually changed, so a stationary drag never re-arms the writer.
fn record(set: impl FnOnce(&mut Layout)) {
    let now = Instant::now();
    with_store(|store| {
        if store.arrangement.record(set) {
            store.changed_at = Some(now);
        }
    });
}

pub fn set_sidebar_width(px: f32) {
    record(|layout| layout.sidebar_width = Some(px));
}
pub fn set_sidepane_width(px: f32) {
    record(|layout| layout.sidepane_width = Some(px));
}
pub fn set_explorer_width(px: f32) {
    record(|layout| layout.explorer_width = Some(px));
}
pub fn set_terminal_height(px: f32) {
    record(|layout| layout.terminal_height = Some(px));
}
pub fn set_github_list_width(px: f32) {
    record(|layout| layout.github_list_width = Some(px));
}

/// Snapshot which panels are open (and whether the sidebar shows) after a
/// toggle, so the next launch lands in the same shape.
pub fn set_open_state(sidebar: bool, project: bool, side: bool, terminal: bool) {
    record(|layout| {
        layout.sidebar_visible = Some(sidebar);
        layout.project_panel = Some(project);
        layout.side_pane = Some(side);
        layout.terminal_panel = Some(terminal);
    });
}

pub fn set_collapsed_workspaces(labels: Vec<String>) {
    record(|layout| layout.collapsed_workspaces = labels);
}

/// How many extra sessions each group has revealed. A group at its cap is
/// simply absent from the map.
pub fn set_expanded_groups(groups: Vec<(String, usize)>) {
    record(|layout| layout.expanded_groups = groups);
}

/// Apply a preset. The arrangement the user built is saved first, so
/// [`restore_custom`] can bring it back.
pub fn apply_preset(preset: LayoutPreset) {
    with_store(|store| {
        store.arrangement.apply_preset(preset);
        store.arrangement.layout.persist(&store.arrangement);
        store.changed_at = None;
    });
}

/// Bring the user's own arrangement back after a preset. False when there is
/// nothing saved.
pub fn restore_custom() -> bool {
    with_store(|store| {
        if !store.arrangement.restore_custom() {
            return false;
        }
        store.arrangement.layout.persist(&store.arrangement);
        store.changed_at = None;
        true
    })
}

/// Save the live arrangement under `name`. A name that already exists is
/// replaced, so "save" on an existing layout overwrites it rather than
/// producing two entries the user cannot tell apart.
pub fn save_named(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return false;
    }
    with_store(|store| {
        let layout = store.arrangement.layout.clone();
        let named = &mut store.arrangement.named;
        if let Some(existing) = named.iter_mut().find(|entry| entry.name == name) {
            existing.layout = layout;
        } else {
            if named.len() >= MAX_NAMED_LAYOUTS {
                return false;
            }
            named.push(NamedLayout {
                name: name.to_owned(),
                layout,
            });
        }
        store.arrangement.layout.persist(&store.arrangement);
        store.changed_at = None;
        true
    })
}

pub fn named_layouts() -> Vec<NamedLayout> {
    with_store(|store| store.arrangement.named.clone())
}

/// Apply a saved arrangement. The live arrangement is replaced; the `custom`
/// slot is left alone, because a named layout is the user's own.
pub fn apply_named(name: &str) -> bool {
    with_store(|store| {
        let Some(named) = store
            .arrangement
            .named
            .iter()
            .find(|entry| entry.name == name)
            .cloned()
        else {
            return false;
        };
        store.arrangement.layout = named.layout;
        store.arrangement.active_preset = None;
        store.arrangement.layout.persist(&store.arrangement);
        store.changed_at = None;
        true
    })
}

pub fn delete_named(name: &str) -> bool {
    with_store(|store| {
        let before = store.arrangement.named.len();
        store.arrangement.named.retain(|entry| entry.name != name);
        if store.arrangement.named.len() == before {
            return false;
        }
        store.arrangement.layout.persist(&store.arrangement);
        store.changed_at = None;
        true
    })
}

pub fn custom_layout() -> Option<Layout> {
    with_store(|store| store.arrangement.custom.clone())
}

pub fn active_preset() -> Option<LayoutPreset> {
    with_store(|store| store.arrangement.active_preset)
}

impl Layout {
    fn persist(&self, arrangement: &Arrangement) {
        let path = persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, arrangement.to_value().to_string());
    }
}

/// Write the arrangement to disk once it has held still past [`SETTLE`].
/// Called from the heartbeat, so a drag costs a single write when it stops.
pub fn flush_if_settled() {
    with_store(|store| {
        let Some(changed_at) = store.changed_at else {
            return;
        };
        if changed_at.elapsed() < SETTLE {
            return;
        }
        store.arrangement.layout.persist(&store.arrangement);
        store.changed_at = None;
    });
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("layout.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every preset must resolve to values the store would keep, and the three
    /// shapes must actually differ — a preset that changes nothing is a menu
    /// item that lies.
    #[test]
    fn presets_are_storable_and_distinct() {
        let mut seen_sizes: Vec<(u32, u32, u32, u32)> = Vec::new();
        let mut seen_panels: Vec<(bool, bool, bool)> = Vec::new();
        for preset in LayoutPreset::ALL {
            let shape = preset.shape();
            assert!(shape.is_storable(), "{preset:?} leaves the storable range");
            seen_sizes.push((
                shape.sidebar as u32,
                shape.explorer as u32,
                shape.sidepane as u32,
                shape.terminal as u32,
            ));
            seen_panels.push((shape.project_panel, shape.side_pane, shape.terminal_panel));
        }
        seen_sizes.dedup();
        seen_panels.dedup();
        assert_eq!(seen_sizes.len(), 3, "two presets share a size set");
        assert_eq!(seen_panels.len(), 3, "two presets open the same panels");

        // The shapes mean what their names say.
        let focus = LayoutPreset::Focus.shape();
        assert!(!focus.project_panel && !focus.side_pane && !focus.terminal_panel);
        let developer = LayoutPreset::Developer.shape();
        assert!(developer.project_panel && developer.terminal_panel && !developer.side_pane);
        let review = LayoutPreset::Review.shape();
        assert!(review.side_pane && !review.project_panel && !review.terminal_panel);
        assert!(
            review.sidepane > developer.sidepane,
            "review is the wide one"
        );
    }

    #[test]
    fn preset_keys_and_ids_are_unique() {
        let mut ids: Vec<&str> = LayoutPreset::ALL.iter().map(|p| p.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), LayoutPreset::ALL.len());

        let mut keys: Vec<&str> = LayoutPreset::ALL.iter().map(|p| p.label_key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), LayoutPreset::ALL.len());
    }

    /// The minimum-size contract: the store's accepted range must bracket every
    /// panel's own clamp. Anything the store keeps, a panel can render and the
    /// user can drag back to a usable size — a stored layout is never
    /// unrecoverable.
    #[test]
    fn the_store_range_brackets_every_panel_clamp() {
        let clamps = [
            ("sidebar", crate::app::SIDEBAR_MIN_W, 320.),
            (
                "review pane",
                crate::sidepane::PANE_MIN_W,
                crate::sidepane::PANE_MAX_W,
            ),
            (
                "project tree",
                crate::explorer::PANEL_MIN_W,
                crate::explorer::PANEL_MAX_W,
            ),
            (
                "terminal",
                crate::terminal::PANEL_MIN_H,
                crate::terminal::PANEL_MAX_H,
            ),
            (
                "github list",
                crate::git_panel::GITHUB_LIST_MIN_W,
                crate::git_panel::GITHUB_LIST_MAX_W,
            ),
        ];
        for (panel, min, max) in clamps {
            assert!(
                MIN_PX <= min,
                "{panel}: store drops sizes the panel accepts"
            );
            assert!(MIN_PX <= min, "{panel}: min below the store floor");
            assert!(MAX_PX >= max, "{panel}: max above the store ceiling");
            assert!(min <= max * 1., "{panel}: inverted clamp");
        }
    }

    #[test]
    fn fields_round_trip() {
        let layout = Layout {
            sidebar_width: Some(248.),
            sidepane_width: Some(460.),
            explorer_width: Some(300.),
            terminal_height: Some(260.),
            github_list_width: Some(360.),
            sidebar_visible: Some(true),
            project_panel: Some(false),
            side_pane: Some(true),
            terminal_panel: Some(false),
            collapsed_workspaces: vec!["orbit".into(), "scratch".into()],
            expanded_groups: vec![("orbit".into(), 6)],
        };
        assert_eq!(Layout::from_value(&layout.to_value()), layout);
    }

    #[test]
    fn expanded_groups_round_trip_and_drop_junk() {
        let value = json!({
            "expanded_groups": { "orbit": 6, "scratch": 0, "": 4, "weird": "lots" }
        });
        let layout = Layout::from_value(&value);
        assert_eq!(
            layout.expanded_groups,
            vec![("orbit".to_string(), 6)],
            "a zero, blank or non-numeric entry is not a reveal"
        );
        assert_eq!(
            Layout::from_value(&layout.to_value()).expanded_groups,
            layout.expanded_groups
        );
    }

    #[test]
    fn missing_and_out_of_bounds_fields_are_dropped() {
        let value = json!({
            "sidebar_width": 248.,
            "sidepane_width": 10.,      // below MIN_PX
            "explorer_width": 99999.,   // above MAX_PX
        });
        let layout = Layout::from_value(&value);
        assert_eq!(layout.sidebar_width, Some(248.));
        assert_eq!(layout.sidepane_width, None);
        assert_eq!(layout.explorer_width, None);
        assert_eq!(layout.terminal_height, None);
        assert_eq!(layout.sidebar_visible, None);
    }

    #[test]
    fn non_numeric_field_is_dropped() {
        let value = json!({ "sidebar_width": "wide" });
        assert_eq!(Layout::from_value(&value), Layout::default());
    }

    #[test]
    fn a_non_boolean_flag_is_dropped_rather_than_coerced() {
        let value = json!({ "sidebar_visible": 1, "project_panel": "yes", "side_pane": true });
        let layout = Layout::from_value(&value);
        assert_eq!(layout.sidebar_visible, None);
        assert_eq!(layout.project_panel, None);
        assert_eq!(layout.side_pane, Some(true));
    }

    #[test]
    fn collapsed_labels_drop_non_strings_and_blanks() {
        let value = json!({ "collapsed_workspaces": ["orbit", 7, "  ", null, "scratch"] });
        assert_eq!(
            Layout::from_value(&value).collapsed_workspaces,
            vec!["orbit".to_string(), "scratch".to_string()]
        );
    }

    #[test]
    fn an_arrangement_round_trips_with_its_custom_slot() {
        let mut arrangement = Arrangement::default();
        arrangement.layout.sidebar_width = Some(210.);
        arrangement.apply_preset(LayoutPreset::Review);
        let parsed = Arrangement::from_value(&arrangement.to_value());
        assert_eq!(parsed, arrangement);
        assert_eq!(parsed.active_preset, Some(LayoutPreset::Review));
        assert_eq!(
            parsed.custom.and_then(|c| c.sidebar_width),
            Some(210.),
            "the hand-built arrangement survives the round trip"
        );
    }

    /// A preset is reversible: applying one saves the user's arrangement, and
    /// going back restores it exactly.
    #[test]
    fn a_preset_keeps_the_custom_arrangement_and_gives_it_back() {
        let mut arrangement = Arrangement::default();
        arrangement.layout.sidebar_width = Some(210.);
        arrangement.layout.side_pane = Some(false);

        arrangement.apply_preset(LayoutPreset::Review);
        assert_eq!(arrangement.active_preset, Some(LayoutPreset::Review));
        assert_eq!(arrangement.layout.sidepane_width, Some(560.));
        assert_eq!(arrangement.layout.side_pane, Some(true));

        assert!(arrangement.restore_custom());
        assert_eq!(arrangement.active_preset, None);
        assert_eq!(arrangement.layout.sidebar_width, Some(210.));
        assert_eq!(arrangement.layout.side_pane, Some(false));
    }

    /// Touching a panel after a preset makes the arrangement the user's again:
    /// the preset is no longer active and the custom slot follows the change,
    /// so a later "Back to custom" cannot resurrect a stale arrangement.
    #[test]
    fn a_user_change_after_a_preset_clears_the_preset() {
        let mut arrangement = Arrangement::default();
        arrangement.apply_preset(LayoutPreset::Developer);
        assert_eq!(arrangement.active_preset, Some(LayoutPreset::Developer));

        arrangement.record(|layout| layout.sidebar_width = Some(300.));
        assert_eq!(arrangement.active_preset, None);
        assert_eq!(
            arrangement.custom.and_then(|c| c.sidebar_width),
            Some(300.),
            "the custom slot follows the user's change"
        );
    }

    /// A named layout is a snapshot, not a live binding: editing the workspace
    /// afterwards must not rewrite it, and applying it must replace the live
    /// arrangement without touching the custom slot.
    #[test]
    fn a_named_layout_saves_applies_and_deletes() {
        let mut arrangement = Arrangement::default();
        arrangement.layout.sidebar_width = Some(300.);
        arrangement.layout.side_pane = Some(true);
        let saved = NamedLayout {
            name: "review".into(),
            layout: arrangement.layout.clone(),
        };
        arrangement.named.push(saved);

        // The workspace moves on.
        arrangement.layout.sidebar_width = Some(220.);
        arrangement.layout.side_pane = Some(false);
        assert_eq!(
            arrangement.named[0].layout.sidebar_width,
            Some(300.),
            "a saved layout does not follow later edits"
        );

        // Applying it restores the snapshot.
        let restored = arrangement.named[0].layout.clone();
        arrangement.layout = restored;
        assert_eq!(arrangement.layout.sidebar_width, Some(300.));
        assert_eq!(arrangement.layout.side_pane, Some(true));

        // The custom slot is untouched by named layouts.
        assert!(arrangement.custom.is_none());
    }

    #[test]
    fn named_layouts_round_trip_and_skip_unnamed_entries() {
        let value = json!({
            "named": [
                { "name": "review", "sidebar_width": 220.0, "side_pane": true },
                { "name": "   " },
                { "sidebar_width": 300.0 }
            ]
        });
        let parsed = Arrangement::from_value(&value);
        assert_eq!(
            parsed.named.len(),
            1,
            "a blank or absent name is not an entry"
        );
        assert_eq!(parsed.named[0].name, "review");
        assert_eq!(parsed.named[0].layout.sidebar_width, Some(220.));
        assert_eq!(Arrangement::from_value(&parsed.to_value()), parsed);
    }

    #[test]
    fn restoring_without_a_saved_custom_layout_reports_false() {
        let mut arrangement = Arrangement::default();
        assert!(!arrangement.restore_custom());
    }
}
