//! Per-command and per-agent permission rules — the Orbital half of the guard.
//!
//! `~/.orbit-pi/access.json` holds one of three access modes, which is the
//! whole policy a mode can express: *ask before commands*, or *do not*. This
//! module owns the finer-grained layer the mode cannot: a rule names a **tool
//! family**, an optional **argument glob**, a **scope** (every session, main
//! sessions only, or one named subagent) and an **action** (`allow` / `ask` /
//! `deny`). That is what makes "`bash` may run, but `rm` asks" expressible.
//!
//! The rules live in `~/.orbit-pi/access-rules.json`, which the bundled guard
//! extension (`contrib/orbit-guard-extension/policy.js`) reads **fresh on every
//! tool call**, so a change here re-arms live sessions without a restart. This
//! module is the only writer; the extension is the only enforcer. Nothing here
//! decides anything at runtime — a duplicated decision table would be a second
//! policy to drift, which is exactly what the mode/allowlist split avoided.
//!
//! Two invariants the file keeps, both lenient in the safe direction:
//!
//! - Anything unreadable, partial, or of an unknown shape loads as **no rules**,
//!   which falls back to the mode default (`ask` unless the mode allows it).
//! - A rule can only adjust the guard's prompt, and only in a confined mode:
//!   **Full access short-circuits the whole table** (nothing asks, nothing is
//!   refused), because that is what the mode promises. A rule also can never
//!   raise a child past the ceiling its agent's tool list implies — the guard
//!   clamps that first.
//!
//! The one table this module *does* mirror is [`Target::default_action`], which
//! Settings → Permissions shows as the baseline for each family. `policy.js`
//! holds the enforcing copy (`defaultAction`); both have tests asserting their
//! own table, and a change to one is a change to both.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::access::AccessMode;

/// Hard cap on stored rules. The extension scans the whole list on every tool
/// call and the Settings page renders all of them, so the file stays bounded.
pub(crate) const MAX_RULES: usize = 200;

/// A pattern longer than this is truncated: a glob needs no more, and a bounded
/// field keeps the file readable and the row layout stable.
pub(crate) const MAX_PATTERN_LEN: usize = 200;

/// The tool families a rule may name. Exactly the guard's `classify()` kinds
/// plus `Any`; kept in sync with `RULE_TARGETS` in the extension's `policy.js`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// `read`, `grep`, `find`, `ls`, `glob`, `list`.
    Read,
    /// `edit`, `write`, `multiedit`, `apply_patch`, `str_replace_editor`.
    Edit,
    /// `bash`, `powershell`, `shell`, `exec`, `terminal`.
    Exec,
    /// `webfetch`, `websearch` — mutates nothing locally, but leaves the machine.
    Net,
    /// Every `mcp__<server>__<tool>`.
    Mcp,
    /// The `subagent` delegation tool.
    Delegate,
    /// Extension and custom tools the guard does not recognise.
    Other,
    /// Every tool, whatever its family. Rules only — never a default row.
    Any,
}

impl Target {
    /// The families the page renders, in display order. `Any` is deliberately
    /// absent: it exists for hand-written rules that apply to every tool, and
    /// the page's per-family rows already cover "all tool calls".
    pub(crate) const FAMILIES: [Target; 7] = [
        Self::Read,
        Self::Edit,
        Self::Exec,
        Self::Net,
        Self::Mcp,
        Self::Delegate,
        Self::Other,
    ];

    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Exec => "exec",
            Self::Net => "net",
            Self::Mcp => "mcp",
            Self::Delegate => "delegate",
            Self::Other => "other",
            Self::Any => "any",
        }
    }

    /// Parse a stored id. `None` for anything unknown, so a hand-edited file
    /// drops the rule instead of loading half of it.
    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "edit" => Some(Self::Edit),
            "exec" => Some(Self::Exec),
            "net" => Some(Self::Net),
            "mcp" => Some(Self::Mcp),
            "delegate" => Some(Self::Delegate),
            "other" => Some(Self::Other),
            "any" => Some(Self::Any),
            _ => None,
        }
    }

    /// Short label for the rule list and the family picker.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Read => tr!("permissions.target.read"),
            Self::Edit => tr!("permissions.target.edit"),
            Self::Exec => tr!("permissions.target.exec"),
            Self::Net => tr!("permissions.target.net"),
            Self::Mcp => tr!("permissions.target.mcp"),
            Self::Delegate => tr!("permissions.target.delegate"),
            Self::Other => tr!("permissions.target.other"),
            Self::Any => tr!("permissions.target.any"),
        }
    }

    /// What belongs to this family, under the label in the picker.
    pub(crate) fn hint(self) -> String {
        match self {
            Self::Read => tr!("permissions.target.read_hint"),
            Self::Edit => tr!("permissions.target.edit_hint"),
            Self::Exec => tr!("permissions.target.exec_hint"),
            Self::Net => tr!("permissions.target.net_hint"),
            Self::Mcp => tr!("permissions.target.mcp_hint"),
            Self::Delegate => tr!("permissions.target.delegate_hint"),
            Self::Other => tr!("permissions.target.other_hint"),
            Self::Any => tr!("permissions.target.any_hint"),
        }
    }

    /// A representative glob for the pattern field, when the family has one.
    pub(crate) fn pattern_example(self) -> &'static str {
        match self {
            Self::Exec => "git status*",
            Self::Edit | Self::Read => "src/*",
            Self::Net => "https://*",
            Self::Mcp => "mcp__github__*",
            Self::Delegate | Self::Other | Self::Any => "",
        }
    }

    /// The text a pattern is matched against, named for the editor's hint.
    pub(crate) fn matches_text(self) -> String {
        match self {
            Self::Exec => tr!("permissions.text.command"),
            Self::Edit | Self::Read => tr!("permissions.text.path"),
            Self::Net => tr!("permissions.text.url"),
            Self::Mcp | Self::Other => tr!("permissions.text.tool_and_arguments"),
            Self::Delegate => tr!("permissions.text.task"),
            Self::Any => tr!("permissions.text.any_argument"),
        }
    }

    /// The mode's baseline for this family — the table the page renders as
    /// "Default". Mirrors `defaultAction()` in `contrib/orbit-guard-extension/policy.js`,
    /// which is the copy the guard actually enforces.
    pub(crate) fn default_action(self, mode: AccessMode) -> Action {
        match self {
            // A delegation is allowed in every mode: its child is clamped by
            // the same mode, and the subagent extension refuses runs that could
            // not finish. Only a deny rule stops one.
            Self::Delegate => Action::Allow,
            // Reads and fetches are non-mutating, so every mode allows them.
            // A rule is the only way to gate a fetch.
            Self::Read | Self::Net => Action::Allow,
            Self::Edit if mode == AccessMode::AutoAcceptEdits => Action::Allow,
            _ if mode == AccessMode::FullAccess => Action::Allow,
            _ => Action::Ask,
        }
    }
}

/// What a rule does to a matching call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Run it without asking.
    Allow,
    /// Confirm it, whatever the mode would have done.
    Ask,
    /// Refuse it without a prompt; the model is told a rule denied it.
    Deny,
}

impl Action {
    pub(crate) const ALL: [Action; 3] = [Self::Allow, Self::Ask, Self::Deny];

    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }

    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Allow => tr!("permissions.action.allow"),
            Self::Ask => tr!("permissions.action.ask"),
            Self::Deny => tr!("permissions.action.deny"),
        }
    }
}

/// Who a rule applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Every session, main and child.
    Any,
    /// Main sessions only — the agent a person is talking to.
    Session,
    /// One subagent, by the name in its agent file.
    Agent(String),
}

impl Scope {
    pub(crate) fn as_wire(&self) -> String {
        match self {
            Self::Any => "any".to_string(),
            Self::Session => "session".to_string(),
            Self::Agent(name) => format!("agent:{name}"),
        }
    }

    /// Parse a stored scope. An `agent:` prefix needs a name: an empty one is
    /// not a scope anyone can use, so the rule is dropped rather than widened.
    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "any" => Some(Self::Any),
            "session" => Some(Self::Session),
            other => other
                .strip_prefix("agent:")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| Self::Agent(name.to_string())),
        }
    }
}

/// One stored rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rule {
    pub scope: Scope,
    pub target: Target,
    /// A glob (`*` = any run of characters) matched against the call's salient
    /// text. Empty matches every call of the family.
    pub pattern: String,
    pub action: Action,
}

impl Rule {
    /// A rule with a normalised pattern, ready to store.
    pub(crate) fn new(scope: Scope, target: Target, pattern: &str, action: Action) -> Self {
        let pattern = pattern.trim();
        let pattern: String = pattern.chars().take(MAX_PATTERN_LEN).collect();
        Self {
            scope,
            target,
            pattern,
            action,
        }
    }
}

/// Every rule, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// The action explicitly set for a family as a whole in `scope` (the
    /// empty-pattern rule), or `None` when the family inherits.
    pub(crate) fn family_rule(&self, scope: &Scope, target: Target) -> Option<Action> {
        self.rules
            .iter()
            .find(|rule| &rule.scope == scope && rule.target == target && rule.pattern.is_empty())
            .map(|rule| rule.action)
    }

    /// The pattern rules stored for one family in one scope, in file order.
    pub(crate) fn pattern_rules(&self, scope: &Scope, target: Target) -> Vec<&Rule> {
        self.rules
            .iter()
            .filter(|rule| {
                &rule.scope == scope && rule.target == target && !rule.pattern.is_empty()
            })
            .collect()
    }

    /// What a call to `target` actually resolves to for `scope`: the most
    /// specific scope with a matching family-level rule decides (deny > ask >
    /// allow), then the mode's default. Mirrors the guard's precedence so the
    /// page's "→ Ask" is the same answer the extension gives.
    pub(crate) fn effective_action(
        &self,
        scope: &Scope,
        target: Target,
        mode: AccessMode,
    ) -> Action {
        // Full access is resolved before any rule, exactly as the guard does.
        if mode == AccessMode::FullAccess {
            return Action::Allow;
        }
        for candidate in scope_chain(scope) {
            let matching: Vec<Action> = self
                .rules
                .iter()
                .filter(|rule| {
                    rule.scope == candidate
                        && rule.pattern.is_empty()
                        && (rule.target == target || rule.target == Target::Any)
                })
                .map(|rule| rule.action)
                .collect();
            if matching.is_empty() {
                continue;
            }
            if matching.contains(&Action::Deny) {
                return Action::Deny;
            }
            if matching.contains(&Action::Ask) {
                return Action::Ask;
            }
            return Action::Allow;
        }
        target.default_action(mode)
    }

    /// Set a family's action in `scope`, or remove its rule so it inherits.
    pub(crate) fn set_family_action(
        &mut self,
        scope: &Scope,
        target: Target,
        action: Option<Action>,
    ) {
        self.rules.retain(|rule| {
            !(&rule.scope == scope && rule.target == target && rule.pattern.is_empty())
        });
        if let Some(action) = action {
            self.upsert(Rule::new(scope.clone(), target, "", action));
        }
    }

    /// Forget one pattern rule. Returns whether anything was removed.
    pub(crate) fn remove_pattern(&mut self, scope: &Scope, target: Target, pattern: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|rule| {
            !(&rule.scope == scope && rule.target == target && rule.pattern == pattern)
        });
        self.rules.len() != before
    }

    /// Add a rule, replacing the action of an identical `(scope, target,
    /// pattern)` — one row per shape, so the page cannot show two rules that
    /// contradict each other by accident. Returns `false` when the file is at
    /// its cap and the rule was not stored.
    pub(crate) fn upsert(&mut self, rule: Rule) -> bool {
        if let Some(existing) = self.rules.iter_mut().find(|existing| {
            // Patterns are matched case-sensitively (a command is a command), so
            // two patterns that differ only in case are two different rules.
            existing.scope == rule.scope
                && existing.target == rule.target
                && existing.pattern == rule.pattern
        }) {
            existing.action = rule.action;
            return true;
        }
        if self.rules.len() >= MAX_RULES {
            return false;
        }
        self.rules.push(rule);
        true
    }

    /// Parse a document, dropping anything this build does not understand.
    /// A dropped rule falls back to the mode default, which asks.
    pub(crate) fn from_json(raw: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Self::default();
        };
        Self::from_value(&value)
    }

    fn from_value(value: &Value) -> Self {
        let rules = value
            .get("rules")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(parse_rule)
                    .take(MAX_RULES)
                    .collect()
            })
            .unwrap_or_default();
        Self { rules }
    }

    fn to_value(&self) -> Value {
        json!({
            "version": 1,
            "rules": self
                .rules
                .iter()
                .map(|rule| json!({
                    "scope": rule.scope.as_wire(),
                    "target": rule.target.as_wire(),
                    "pattern": rule.pattern,
                    "action": rule.action.as_wire(),
                }))
                .collect::<Vec<_>>(),
        })
    }

    /// The JSON document written to disk.
    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.to_value()).unwrap_or_else(|_| "{}".to_string())
    }
}

/// One stored entry, or `None` when a field is unknown.
fn parse_rule(value: &Value) -> Option<Rule> {
    let scope = Scope::from_wire(value.get("scope")?.as_str()?)?;
    let target = Target::from_wire(value.get("target")?.as_str()?)?;
    let action = Action::from_wire(value.get("action")?.as_str()?)?;
    let pattern = value
        .get("pattern")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(Rule::new(scope, target, pattern, action))
}

/// The scopes that apply to `scope`, most specific first. Only consulted in a
/// confined mode: Full access never reaches here. A child session sees
/// its own agent scope and then `any`; a main session sees `session` and `any`.
/// The guard resolves in the same order.
fn scope_chain(scope: &Scope) -> Vec<Scope> {
    match scope {
        Scope::Any => vec![Scope::Any],
        Scope::Session => vec![Scope::Session, Scope::Any],
        Scope::Agent(_) => vec![scope.clone(), Scope::Any],
    }
}

/// `~/.orbit-pi/access-rules.json` — written here, read by the guard extension.
pub(crate) fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("access-rules.json")
}

impl Rules {
    /// Read the store; any failure is no rules.
    pub(crate) fn load() -> Self {
        Self::load_at(&store_path())
    }

    fn load_at(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    /// Write the store atomically (temp + rename) and owner-only, so a crash
    /// cannot leave a truncated file for the guard to trip over and another
    /// local user cannot rewrite the user's permissions.
    pub(crate) fn save(&self) -> Result<(), String> {
        self.save_at(&store_path())
    }

    fn save_at(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).map_err(|error| {
            tr!(
                "errors.could_not_create",
                path = parent.display().to_string(),
                error = error
            )
        })?;
        let temporary = parent.join(format!(".access-rules.json.{}.tmp", std::process::id()));
        let write = || -> Result<(), String> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| {
                    tr!(
                        "errors.could_not_write",
                        path = temporary.display().to_string(),
                        error = error
                    )
                })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(0o600))
                    .map_err(|error| {
                        tr!(
                            "errors.could_not_write",
                            path = temporary.display().to_string(),
                            error = error
                        )
                    })?;
            }
            use std::io::Write;
            file.write_all(self.to_json().as_bytes()).map_err(|error| {
                tr!(
                    "errors.could_not_write",
                    path = temporary.display().to_string(),
                    error = error
                )
            })?;
            std::fs::rename(&temporary, path).map_err(|error| {
                tr!(
                    "errors.could_not_replace",
                    path = path.display().to_string(),
                    error = error
                )
            })
        };
        let result = write();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orbit-permissions-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rule(scope: Scope, target: Target, pattern: &str, action: Action) -> Rule {
        Rule::new(scope, target, pattern, action)
    }

    #[test]
    fn the_default_table_matches_the_guards_decision_table() {
        use AccessMode::*;
        // Mirrors `defaultAction()` in `contrib/orbit-guard-extension/policy.js`
        // (the enforcing copy). Changing one is changing both.
        let expected = |mode: AccessMode, target: Target| match (mode, target) {
            (_, Target::Read | Target::Net | Target::Delegate) => Action::Allow,
            (FullAccess, _) => Action::Allow,
            (AutoAcceptEdits, Target::Edit) => Action::Allow,
            _ => Action::Ask,
        };
        for mode in AccessMode::ALL {
            for target in Target::FAMILIES.into_iter().chain([Target::Any]) {
                assert_eq!(
                    target.default_action(mode),
                    expected(mode, target),
                    "{} / {}",
                    mode.as_wire(),
                    target.as_wire()
                );
            }
        }
        // Delegation is never prompted in any mode.
        for mode in AccessMode::ALL {
            assert_eq!(Target::Delegate.default_action(mode), Action::Allow);
        }
    }

    #[test]
    fn wire_ids_round_trip_and_unknowns_are_rejected() {
        for target in Target::FAMILIES.into_iter().chain([Target::Any]) {
            assert_eq!(Target::from_wire(target.as_wire()), Some(target));
            assert!(!target.label().is_empty());
            assert!(!target.matches_text().is_empty());
        }
        for action in Action::ALL {
            assert_eq!(Action::from_wire(action.as_wire()), Some(action));
            assert!(!action.label().is_empty());
        }
        for scope in [Scope::Any, Scope::Session, Scope::Agent("worker".into())] {
            assert_eq!(Scope::from_wire(&scope.as_wire()), Some(scope));
        }
        assert_eq!(Target::from_wire("shell"), None);
        assert_eq!(Action::from_wire("block"), None);
        assert_eq!(Scope::from_wire("agent:"), None);
        assert_eq!(Scope::from_wire("all"), None);
    }

    #[test]
    fn the_page_lists_every_family_but_any() {
        assert_eq!(Target::FAMILIES.len(), 7);
        assert!(!Target::FAMILIES.contains(&Target::Any));
    }

    #[test]
    fn load_drops_unknown_rules_and_never_widens() {
        let dir = temp_dir("load");
        let path = dir.join("access-rules.json");
        let none = |rules: Rules| rules.pattern_rules(&Scope::Any, Target::Exec).is_empty();
        assert!(none(Rules::load_at(&path)), "missing file");

        std::fs::write(&path, "{not json").unwrap();
        assert!(none(Rules::load_at(&path)), "corrupt file");

        std::fs::write(
            &path,
            r#"{"rules":[
                {"scope":"any","target":"exec","pattern":"rm *","action":"deny"},
                {"scope":"agent:worker","target":"any","pattern":"","action":"ask"},
                {"scope":"any","target":"shell","pattern":"","action":"deny"},
                {"scope":"any","target":"exec","pattern":"","action":"maybe"},
                {"scope":"everyone","target":"exec","pattern":"","action":"deny"},
                {"scope":"any"},
                "not a rule"
            ]}"#,
        )
        .unwrap();
        let loaded = Rules::load_at(&path);
        assert_eq!(
            loaded.pattern_rules(&Scope::Any, Target::Exec),
            vec![&rule(Scope::Any, Target::Exec, "rm *", Action::Deny)],
            "only the well-formed rules survive"
        );
        assert_eq!(
            loaded.family_rule(&Scope::Agent("worker".into()), Target::Any),
            Some(Action::Ask)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_round_trips_atomically_and_owner_only() {
        let dir = temp_dir("save");
        let path = dir.join("access-rules.json");
        let mut rules = Rules::default();
        rules.upsert(rule(Scope::Any, Target::Exec, "rm *", Action::Deny));
        rules.upsert(rule(
            Scope::Agent("worker".into()),
            Target::Net,
            "",
            Action::Ask,
        ));
        rules.save_at(&path).unwrap();

        assert_eq!(Rules::load_at(&path), rules);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "permissions are owner-only");
        }
        // Rewriting leaves no temp file for the guard to trip over.
        rules.save_at(&path).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "stray temp files: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_keeps_one_row_per_shape_and_replaces_its_action() {
        let mut rules = Rules::default();
        assert!(rules.upsert(rule(Scope::Any, Target::Exec, "git status*", Action::Ask)));
        assert!(rules.upsert(rule(Scope::Any, Target::Exec, "git status*", Action::Allow)));
        assert_eq!(
            rules.pattern_rules(&Scope::Any, Target::Exec),
            vec![&rule(
                Scope::Any,
                Target::Exec,
                "git status*",
                Action::Allow
            )],
            "same scope/target/pattern"
        );

        // A different scope, target or pattern is a different rule.
        assert!(rules.upsert(rule(
            Scope::Session,
            Target::Exec,
            "git status*",
            Action::Ask
        )));
        assert!(rules.upsert(rule(Scope::Any, Target::Edit, "git status*", Action::Ask)));
        assert!(rules.upsert(rule(Scope::Any, Target::Exec, "git log*", Action::Ask)));
        assert_eq!(rules.pattern_rules(&Scope::Any, Target::Exec).len(), 2);
        assert_eq!(rules.pattern_rules(&Scope::Session, Target::Exec).len(), 1);
        assert_eq!(rules.pattern_rules(&Scope::Any, Target::Edit).len(), 1);
    }

    #[test]
    fn the_cap_and_pattern_length_are_enforced() {
        let mut rules = Rules::default();
        for ix in 0..MAX_RULES {
            assert!(rules.upsert(rule(
                Scope::Any,
                Target::Exec,
                &format!("cmd{ix}"),
                Action::Ask
            )));
        }
        assert!(
            !rules.upsert(rule(Scope::Any, Target::Exec, "one-too-many", Action::Ask)),
            "the file stays bounded"
        );
        assert_eq!(
            rules.pattern_rules(&Scope::Any, Target::Exec).len(),
            MAX_RULES
        );

        let long = "x".repeat(MAX_PATTERN_LEN + 50);
        let stored = rule(Scope::Any, Target::Exec, &long, Action::Deny);
        assert_eq!(stored.pattern.chars().count(), MAX_PATTERN_LEN);
        assert!(rule(Scope::Any, Target::Exec, "  rm *  ", Action::Deny).pattern == "rm *");
    }

    #[test]
    fn a_family_inherits_until_it_is_set() {
        let mut rules = Rules::default();
        assert_eq!(rules.family_rule(&Scope::Any, Target::Exec), None);
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Exec, AccessMode::Supervised),
            Action::Ask
        );
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Read, AccessMode::Supervised),
            Action::Allow
        );

        rules.set_family_action(&Scope::Any, Target::Exec, Some(Action::Allow));
        assert_eq!(
            rules.family_rule(&Scope::Any, Target::Exec),
            Some(Action::Allow)
        );
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Exec, AccessMode::Supervised),
            Action::Allow
        );

        // Choosing "inherit" again drops the rule rather than storing the default.
        rules.set_family_action(&Scope::Any, Target::Exec, None);
        assert_eq!(rules.family_rule(&Scope::Any, Target::Exec), None);
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Exec, AccessMode::Supervised),
            Action::Ask
        );
    }

    #[test]
    fn the_effective_action_follows_the_guard_s_scope_precedence() {
        let mut rules = Rules::default();
        rules.set_family_action(&Scope::Any, Target::Exec, Some(Action::Allow));
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Exec, AccessMode::Supervised),
            Action::Allow
        );

        // A child inherits `any` until its own scope says otherwise.
        let worker = Scope::Agent("worker".into());
        assert_eq!(
            rules.effective_action(&worker, Target::Exec, AccessMode::Supervised),
            Action::Allow
        );
        rules.set_family_action(&worker, Target::Exec, Some(Action::Deny));
        assert_eq!(
            rules.effective_action(&worker, Target::Exec, AccessMode::Supervised),
            Action::Deny
        );
        // ...and a main session never sees the child's rule.
        assert_eq!(
            rules.effective_action(&Scope::Session, Target::Exec, AccessMode::Supervised),
            Action::Allow
        );
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Exec, AccessMode::Supervised),
            Action::Allow
        );
    }

    #[test]
    fn full_access_ignores_every_stored_rule() {
        // The mode is answered before the table, so the page must not claim a
        // rule is in force while Full access would run the call anyway.
        let mut rules = Rules::default();
        rules.set_family_action(&Scope::Any, Target::Exec, Some(Action::Deny));
        rules.set_family_action(
            &Scope::Agent("worker".into()),
            Target::Edit,
            Some(Action::Ask),
        );
        for mode in [AccessMode::FullAccess] {
            for target in Target::FAMILIES {
                assert_eq!(
                    rules.effective_action(&Scope::Any, target, mode),
                    Action::Allow,
                    "{}",
                    target.as_wire()
                );
            }
        }
        // The same rules apply the moment the mode is confined again.
        assert_eq!(
            rules.effective_action(&Scope::Any, Target::Exec, AccessMode::Supervised),
            Action::Deny
        );
        assert_eq!(
            rules.effective_action(
                &Scope::Agent("worker".into()),
                Target::Edit,
                AccessMode::AutoAcceptEdits
            ),
            Action::Ask
        );
    }

    #[test]
    fn a_broader_rule_still_applies_inside_a_specific_scope() {
        // The page must show what will actually happen: an `any`-scope rule is
        // inherited by every scope that says nothing itself.
        let mut rules = Rules::default();
        rules.set_family_action(&Scope::Any, Target::Edit, Some(Action::Allow));
        assert_eq!(
            rules.effective_action(
                &Scope::Agent("worker".into()),
                Target::Edit,
                AccessMode::Supervised
            ),
            Action::Allow
        );
        // An `any`-target rule (hand-written or advanced) covers every family.
        rules.upsert(rule(Scope::Session, Target::Any, "", Action::Deny));
        assert_eq!(
            rules.effective_action(&Scope::Session, Target::Read, AccessMode::Supervised),
            Action::Deny
        );
    }

    #[test]
    fn pattern_rules_are_listed_and_removed_per_family() {
        let mut rules = Rules::default();
        let worker = Scope::Agent("worker".into());
        rules.set_family_action(&worker, Target::Exec, Some(Action::Allow));
        rules.upsert(rule(worker.clone(), Target::Exec, "rm *", Action::Deny));
        rules.upsert(rule(worker.clone(), Target::Exec, "git *", Action::Allow));
        rules.upsert(rule(worker.clone(), Target::Edit, "*.lock", Action::Ask));

        let patterns: Vec<&str> = rules
            .pattern_rules(&worker, Target::Exec)
            .iter()
            .map(|rule| rule.pattern.as_str())
            .collect();
        assert_eq!(
            patterns,
            vec!["rm *", "git *"],
            "family-level rule is not a pattern"
        );
        assert_eq!(rules.pattern_rules(&worker, Target::Edit).len(), 1);
        assert_eq!(rules.pattern_rules(&Scope::Any, Target::Exec).len(), 0);

        assert!(rules.remove_pattern(&worker, Target::Exec, "rm *"));
        assert_eq!(rules.pattern_rules(&worker, Target::Exec).len(), 1);
        assert!(
            !rules.remove_pattern(&worker, Target::Exec, "rm *"),
            "idempotent"
        );
        // The family-level action survives removing its patterns.
        assert_eq!(
            rules.family_rule(&worker, Target::Exec),
            Some(Action::Allow)
        );
    }

    #[test]
    fn the_document_shape_is_the_one_the_extension_reads() {
        let mut rules = Rules::default();
        rules.upsert(rule(Scope::Any, Target::Exec, "rm *", Action::Deny));
        let value: Value = serde_json::from_str(&rules.to_json()).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["rules"][0]["scope"], "any");
        assert_eq!(value["rules"][0]["target"], "exec");
        assert_eq!(value["rules"][0]["pattern"], "rm *");
        assert_eq!(value["rules"][0]["action"], "deny");
    }
}
