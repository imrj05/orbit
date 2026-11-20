//! Extra provider accounts. Every account is its own provider: the bundled
//! accounts extension (`contrib/orbit-accounts-extension/`) registers a copy of
//! the built-in provider under the account's alias id (`<provider>~<slug>`),
//! and pi keys credentials by provider id. So each account signs in, refreshes
//! and is stored separately in pi's own `auth.json`, and all accounts — the
//! Default one (the built-in provider itself) included — are live in every
//! session at once. This module only keeps the metadata: labels and aliases.
//!
//! The store is `~/.orbit-pi/accounts.json`. Version 1 (`provider-accounts.json`)
//! routed one account per provider per session through a pi patch. Loading it
//! migrates to version 2 in the new file: each profile gets an alias,
//! its credential moves under that alias in pi's `auth.json` (moved, never
//! copied, so an OAuth refresh token stays single), and the per-session
//! bindings are dropped. The old file is then rewritten as an empty version-1
//! store, so an older Orbit build still starts (on Default accounts) instead
//! of refusing to launch pi. The session plumbing still asks for a selection;
//! it is always empty now, so no route is ever applied.
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Provider id → account id. Retained for the session plumbing; always empty.
pub(crate) type AccountSelection = BTreeMap<String, String>;

/// What an account holds in pi's `auth.json`: only the kind, never a secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialKind {
    OAuth,
    ApiKey,
}

/// One account as the Settings controller caches it for painting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccountProfile {
    pub(crate) id: String,
    pub(crate) label: String,
    /// The provider id pi knows this account by (`openai-codex~work`).
    pub(crate) alias: String,
    /// `None` while the account has no sign-in.
    pub(crate) credential: Option<CredentialKind>,
}

const VERSION: u32 = 2;
const STORE: &str = "accounts.json";
const LEGACY_STORE: &str = "provider-accounts.json";

/// Alias → account name, published whenever the store is read, so a provider
/// label (`OpenAI Codex · Work`) can be painted without touching disk.
static LABELS: std::sync::RwLock<BTreeMap<String, String>> =
    std::sync::RwLock::new(BTreeMap::new());

/// `openai-codex~work` → `("openai-codex", "work")`; `None` for a plain id.
pub(crate) fn split_alias(id: &str) -> Option<(&str, &str)> {
    id.split_once('~')
        .filter(|(base, slug)| !base.is_empty() && !slug.is_empty())
}

/// The account name behind an alias, as last read from the store.
pub(crate) fn alias_label(alias: &str) -> Option<String> {
    LABELS.read().ok()?.get(alias).cloned()
}

fn publish_labels(data: &Metadata) {
    if let Ok(mut labels) = LABELS.write() {
        *labels = data
            .profiles
            .values()
            .map(|profile| (profile.alias.clone(), profile.label.clone()))
            .collect();
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    provider: String,
    label: String,
    /// Empty only in a version-1 store, before migration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    alias: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u32,
    profiles: BTreeMap<String, Profile>,
    /// Version-1 fields: read so an old store parses, never written again.
    #[serde(default, skip_serializing)]
    defaults: AccountSelection,
    #[serde(default, skip_serializing)]
    sessions: BTreeMap<PathBuf, AccountSelection>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            version: VERSION,
            profiles: BTreeMap::new(),
            defaults: BTreeMap::new(),
            sessions: BTreeMap::new(),
        }
    }
}

pub(crate) struct ProviderAccounts {
    home: PathBuf,
    /// A corrupt store must not look like an empty store.
    load_error: Option<String>,
    had_metadata: Cell<bool>,
}

impl ProviderAccounts {
    pub(crate) fn load() -> Self {
        // Tests build whole `OrbitApp`s; they must never touch the user's
        // real store or pi's real auth.json.
        #[cfg(test)]
        {
            let home = std::env::temp_dir().join(format!(
                "orbit-provider-accounts-test-{}",
                std::process::id()
            ));
            let _ = fs::create_dir_all(&home);
            Self::at(home)
        }
        #[cfg(not(test))]
        match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            Some(home) => Self::at(PathBuf::from(home)),
            None => Self {
                home: PathBuf::new(),
                load_error: Some("Cannot locate the account store".into()),
                had_metadata: Cell::new(false),
            },
        }
    }

    fn at(home: PathBuf) -> Self {
        let mut accounts = Self {
            home,
            load_error: None,
            had_metadata: Cell::new(false),
        };
        if let Err(error) = accounts.migrate() {
            accounts.load_error = Some(error.to_string());
        } else if let Ok(data) = accounts.metadata() {
            publish_labels(&data);
        }
        accounts
    }

    fn root(&self) -> PathBuf {
        self.home.join(".orbit-pi")
    }

    /// pi's own credential store, where every account's sign-in lives.
    fn pi_auth_path(&self) -> PathBuf {
        self.home.join(".pi").join("agent").join("auth.json")
    }

    fn metadata(&self) -> Result<Metadata> {
        if let Some(error) = &self.load_error {
            bail!("{error}");
        }
        let data = self.read_metadata_at(&self.root().join(STORE), true)?;
        if data.version != VERSION {
            bail!("Unsupported account metadata version");
        }
        Ok(data)
    }

    /// Read and validate a store file, any version this build understands.
    /// `current` marks the live store, whose disappearance is an error.
    fn read_metadata_at(&self, path: &Path, current: bool) -> Result<Metadata> {
        if !self.home.is_absolute() {
            bail!("Account store home must be an absolute path");
        }
        let data: Metadata = match fs::symlink_metadata(path) {
            Ok(info) => {
                if !info.is_file() || info.file_type().is_symlink() {
                    bail!("Account metadata is not a regular file");
                }
                if current {
                    self.had_metadata.set(true);
                }
                serde_json::from_slice(&fs::read(path)?).context("Invalid account metadata")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if current && self.had_metadata.get() {
                    bail!("Account metadata was deleted while Orbit was running");
                }
                Metadata::default()
            }
            Err(error) => return Err(error.into()),
        };
        if data.version != 1 && data.version != VERSION {
            bail!("Unsupported account metadata version");
        }
        let mut aliases = BTreeSet::new();
        for (id, profile) in &data.profiles {
            if !safe_id(id) || profile.provider.trim().is_empty() || profile.label.trim().is_empty()
            {
                bail!("Invalid account metadata");
            }
            if data.version == VERSION
                && (!valid_alias(&profile.alias, &profile.provider)
                    || !aliases.insert(profile.alias.clone()))
            {
                bail!("Invalid account metadata");
            }
        }
        Ok(data)
    }

    /// Bring a version-1 store to version 2. Idempotent; a no-op on a current
    /// or missing store. Each credential is moved before its folder is removed,
    /// and an alias that already has a credential is never overwritten.
    fn migrate(&self) -> Result<()> {
        let current = self.root().join(STORE);
        if fs::symlink_metadata(&current).is_ok() {
            self.read_metadata_at(&current, true)?;
            return Ok(());
        }
        let legacy = self.root().join(LEGACY_STORE);
        let mut data = self.read_metadata_at(&legacy, false)?;
        if data.profiles.is_empty() {
            return Ok(());
        }
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let ids: Vec<String> = data.profiles.keys().cloned().collect();
        for id in &ids {
            let profile = &data.profiles[id];
            // A store already carrying aliases (an early build of this one
            // wrote version 2 into the old file) keeps them.
            let alias = if valid_alias(&profile.alias, &profile.provider)
                && !taken.contains(&profile.alias)
            {
                profile.alias.clone()
            } else {
                unique_alias(&profile.provider, &profile.label, &taken)
            };
            taken.insert(alias.clone());
            if let Some(entry) = self.legacy_credential(id, &profile.provider) {
                providers_insert(&self.pi_auth_path(), &alias, entry)?;
            }
            if let Some(profile) = data.profiles.get_mut(id) {
                profile.alias = alias;
            }
        }
        data.version = VERSION;
        data.defaults.clear();
        data.sessions.clear();
        self.persist(&data)?;
        for id in &ids {
            self.remove_legacy_dir(id);
        }
        // What an older build reads: a valid, empty version-1 store.
        write_file_atomic(
            &self.root(),
            LEGACY_STORE,
            br#"{"version":1,"profiles":{},"defaults":{},"sessions":{}}"#,
        )?;
        Ok(())
    }

    /// A version-1 profile's credential for its provider, if it held one.
    fn legacy_credential(&self, id: &str, provider: &str) -> Option<serde_json::Value> {
        if !safe_id(id) {
            return None;
        }
        let auth = self
            .root()
            .join("provider-accounts")
            .join(id)
            .join("auth.json");
        let info = fs::symlink_metadata(&auth).ok()?;
        if !info.is_file() || info.file_type().is_symlink() || info.len() > 1_048_576 {
            return None;
        }
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&auth).ok()?).ok()?;
        value
            .get(provider)
            .filter(|entry| entry.is_object())
            .cloned()
    }

    fn remove_legacy_dir(&self, id: &str) {
        if !safe_id(id) {
            return;
        }
        let dir = self.root().join("provider-accounts").join(id);
        if let Ok(info) = fs::symlink_metadata(&dir) {
            if info.is_dir() && !info.file_type().is_symlink() {
                let _ = fs::remove_dir_all(&dir);
            }
        }
        // The parent folder is removed once it is empty; never forced.
        let _ = fs::remove_dir(self.root().join("provider-accounts"));
    }

    /// Every provider's accounts in one pass, for the Settings controller to
    /// cache. Rendering must use that cache, never this. Credential kinds come
    /// from pi's `auth.json` under each alias.
    pub(crate) fn all_profiles(&self) -> BTreeMap<String, Vec<AccountProfile>> {
        let mut by_provider: BTreeMap<String, Vec<AccountProfile>> = BTreeMap::new();
        let Ok(data) = self.metadata() else {
            return by_provider;
        };
        publish_labels(&data);
        let kinds: HashMap<String, CredentialKind> =
            crate::providers::read_auth_at(&self.pi_auth_path())
                .map(|auth| {
                    auth.into_iter()
                        .map(|(id, auth)| {
                            let kind = match auth.kind {
                                crate::providers::AuthKind::OAuth => CredentialKind::OAuth,
                                crate::providers::AuthKind::ApiKey => CredentialKind::ApiKey,
                            };
                            (id, kind)
                        })
                        .collect()
                })
                .unwrap_or_default();
        for (id, profile) in data.profiles {
            let credential = kinds.get(&profile.alias).copied();
            by_provider
                .entry(profile.provider)
                .or_default()
                .push(AccountProfile {
                    id,
                    label: profile.label,
                    alias: profile.alias,
                    credential,
                });
        }
        for profiles in by_provider.values_mut() {
            profiles.sort_by_key(|profile| profile.label.to_lowercase());
        }
        by_provider
    }

    /// Whether the store can be read. A corrupt store must not look like one
    /// with no accounts.
    pub(crate) fn is_healthy(&self) -> bool {
        self.metadata().is_ok()
    }

    /// Add an account and return its alias. Metadata only: the account signs
    /// in on its own provider row once pi has loaded it.
    pub(crate) fn add(&mut self, provider: &str, label: &str) -> Result<String> {
        let label = label.trim();
        if provider.trim().is_empty()
            || provider.contains('~')
            || label.is_empty()
            || label.chars().count() > 120
        {
            bail!("Account provider and name must be valid and nonempty");
        }
        let mut data = self.metadata()?;
        let taken: BTreeSet<String> = data
            .profiles
            .values()
            .map(|profile| profile.alias.clone())
            .collect();
        let alias = unique_alias(provider, label, &taken);
        let id = loop {
            let id = generated_id();
            if !data.profiles.contains_key(&id) {
                break id;
            }
        };
        data.profiles.insert(
            id,
            Profile {
                provider: provider.into(),
                label: label.into(),
                alias: alias.clone(),
            },
        );
        self.persist(&data)?;
        Ok(alias)
    }

    /// Change an account's name. Its alias (and so its credential) stays.
    pub(crate) fn rename(&mut self, id: &str, label: &str) -> Result<()> {
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 120 {
            bail!("Account name must be nonempty");
        }
        let mut data = self.metadata()?;
        let Some(profile) = data.profiles.get_mut(id) else {
            bail!("Account is missing");
        };
        profile.label = label.into();
        self.persist(&data)
    }

    /// Remove an account and sign it out: its credential is deleted from pi's
    /// `auth.json`. Returns the alias that was removed.
    pub(crate) fn remove(&mut self, id: &str) -> Result<String> {
        let mut data = self.metadata()?;
        let Some(profile) = data.profiles.remove(id) else {
            bail!("Account is missing");
        };
        crate::providers::remove_auth_at(&self.pi_auth_path(), &profile.alias)
            .map_err(|error| anyhow!(error))?;
        self.persist(&data)?;
        Ok(profile.alias)
    }

    // ── Session plumbing (one account per session was retired) ──
    //
    // Every account is live in every session, so a session never selects one.
    // These keep the spawn and session-switch paths' contract: an empty
    // selection, which routes nothing, and a failure only while the store
    // itself is unreadable.

    pub(crate) fn selected(&self, _provider: &str) -> Option<String> {
        None
    }

    pub(crate) fn selection(&self) -> AccountSelection {
        AccountSelection::new()
    }

    pub(crate) fn uses_defaults(&self) -> bool {
        true
    }

    pub(crate) fn begin_session(&mut self) -> Result<()> {
        Ok(())
    }

    pub(crate) fn activate_session(&mut self, _path: &Path) -> Result<()> {
        Ok(())
    }

    pub(crate) fn selection_for_session(&self, _path: &Path) -> Result<AccountSelection> {
        Ok(AccountSelection::new())
    }

    pub(crate) fn record_session(&self, _path: &Path, selection: &AccountSelection) -> Result<()> {
        if !selection.is_empty() {
            bail!("Per-session account selection is no longer supported");
        }
        Ok(())
    }

    /// pi's own credential file: every account (alias) is stored there.
    pub(crate) fn auth_path(&self, _provider: &str) -> Result<PathBuf> {
        Ok(self.pi_auth_path())
    }

    pub(crate) fn routes(&self) -> Result<String> {
        self.routes_for(&AccountSelection::new())
    }

    pub(crate) fn routes_for(&self, selection: &AccountSelection) -> Result<String> {
        if !selection.is_empty() {
            bail!("Per-session account selection is no longer supported");
        }
        Ok("{}".into())
    }

    fn persist(&self, data: &Metadata) -> Result<()> {
        let root = self.root();
        private_dir(&root)?;
        write_file_atomic(&root, STORE, &serde_json::to_vec_pretty(data)?)?;
        self.had_metadata.set(true);
        Ok(())
    }
}

/// Write `root/name` atomically with owner-only permissions.
fn write_file_atomic(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    private_dir(root)?;
    let temporary = root.join(format!("{}.tmp", generated_id()));
    write_new_private(&temporary, bytes)?;
    let result = fs::rename(&temporary, root.join(name)).context("Cannot save account metadata");
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn providers_insert(path: &Path, alias: &str, entry: serde_json::Value) -> Result<()> {
    crate::providers::insert_auth_entry_at(path, alias, entry)
        .map(|_| ())
        .map_err(|error| anyhow!(error))
}

/// `<provider>~<slug>`: a stable id pi can key a credential by. The slug comes
/// from the name the account was created with and never changes on rename.
fn unique_alias(provider: &str, label: &str, taken: &BTreeSet<String>) -> String {
    let mut slug = String::new();
    for ch in label.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= 32 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "account" } else { slug };
    let base = format!("{provider}~{slug}");
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|alias| !taken.contains(alias))
        .unwrap_or(base)
}

fn valid_alias(alias: &str, provider: &str) -> bool {
    let Some(slug) = alias
        .strip_prefix(provider)
        .and_then(|rest| rest.strip_prefix('~'))
    else {
        return false;
    };
    !slug.is_empty()
        && slug.len() <= 48
        && !slug.starts_with('-')
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn safe_id(id: &str) -> bool {
    id.starts_with("account-")
        && id.len() <= 100
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn generated_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "account-{time:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn private_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(info) if !info.is_dir() || info.file_type().is_symlink() => {
            bail!("Account store is not a real directory")
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(path)?,
        Err(error) => return Err(error.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_new_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let home = std::env::temp_dir().join(generated_id());
            fs::create_dir(&home).unwrap();
            Self(home)
        }
        fn accounts(&self) -> ProviderAccounts {
            ProviderAccounts::at(self.0.clone())
        }
        fn pi_auth(&self) -> serde_json::Value {
            serde_json::from_slice(&fs::read(self.0.join(".pi/agent/auth.json")).unwrap()).unwrap()
        }
        fn write_pi_auth(&self, value: &str) {
            fs::create_dir_all(self.0.join(".pi/agent")).unwrap();
            fs::write(self.0.join(".pi/agent/auth.json"), value).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn accounts_are_aliases_of_their_provider_and_all_live_at_once() {
        let fixture = Fixture::new();
        let mut accounts = fixture.accounts();
        let work = accounts.add("openai-codex", "Work").unwrap();
        let personal = accounts.add("openai-codex", "Personal Plan!").unwrap();
        let again = accounts.add("openai-codex", "work").unwrap();
        assert_eq!(work, "openai-codex~work");
        assert_eq!(personal, "openai-codex~personal-plan");
        assert_eq!(again, "openai-codex~work-2");
        assert!(accounts.add("a~b", "X").is_err());
        // No selection and no route: every account is its own provider.
        assert!(accounts.selection().is_empty());
        assert_eq!(accounts.routes().unwrap(), "{}");
        assert_eq!(accounts.all_profiles()["openai-codex"].len(), 3);
    }

    #[test]
    fn credential_kind_comes_from_the_alias_without_secrets() {
        let fixture = Fixture::new();
        let mut accounts = fixture.accounts();
        let alias = accounts.add("p", "Work").unwrap();
        fixture.write_pi_auth(&format!(
            r#"{{"p":{{"type":"api_key","key":"default"}},"{alias}":{{"type":"oauth","access":"secret"}}}}"#
        ));
        let all = accounts.all_profiles();
        assert_eq!(all["p"][0].credential, Some(CredentialKind::OAuth));
        assert!(!format!("{all:?}").contains("secret"));
    }

    #[test]
    fn rename_keeps_the_alias_and_remove_signs_the_alias_out() {
        let fixture = Fixture::new();
        let mut accounts = fixture.accounts();
        let alias = accounts.add("p", "Work").unwrap();
        fixture.write_pi_auth(&format!(
            r#"{{"p":{{"type":"api_key","key":"keep"}},"{alias}":{{"type":"api_key","key":"gone"}}}}"#
        ));
        let id = accounts.all_profiles()["p"][0].id.clone();
        accounts.rename(&id, "  Client ").unwrap();
        let profile = &accounts.all_profiles()["p"][0];
        assert_eq!(
            (profile.label.as_str(), profile.alias.as_str()),
            ("Client", alias.as_str())
        );
        assert!(accounts.rename(&id, " ").is_err());

        assert_eq!(accounts.remove(&id).unwrap(), alias);
        let auth = fixture.pi_auth();
        assert!(auth.get(&alias).is_none());
        assert_eq!(auth["p"]["key"], "keep");
        assert!(accounts.all_profiles().is_empty());
        assert!(accounts.remove(&id).is_err());
    }

    #[test]
    fn version_one_store_migrates_and_moves_each_credential_once() {
        let fixture = Fixture::new();
        let root = fixture.0.join(".orbit-pi");
        let dir = root.join("provider-accounts/account-1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("auth.json"),
            r#"{"openai-codex":{"type":"oauth","refresh":"r1"}}"#,
        )
        .unwrap();
        fs::write(
            root.join("provider-accounts.json"),
            r#"{"version":1,"profiles":{"account-1":{"provider":"openai-codex","label":"Work"}},
               "defaults":{"openai-codex":"account-1"},
               "sessions":{"/s.jsonl":{"openai-codex":"account-1"}}}"#,
        )
        .unwrap();
        fixture.write_pi_auth(r#"{"openai-codex":{"type":"oauth","refresh":"default"}}"#);

        let accounts = fixture.accounts();
        assert!(accounts.is_healthy());
        let auth = fixture.pi_auth();
        assert_eq!(auth["openai-codex~work"]["refresh"], "r1");
        assert_eq!(auth["openai-codex"]["refresh"], "default");
        assert!(!dir.exists(), "the old per-profile copy is gone");
        let saved = fs::read_to_string(root.join("accounts.json")).unwrap();
        assert!(saved.contains("\"version\": 2"));
        assert!(!saved.contains("sessions") && !saved.contains("defaults"));
        // An older build still finds a valid, empty version-1 store.
        let legacy: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("provider-accounts.json")).unwrap())
                .unwrap();
        assert_eq!(legacy["version"], 1);
        assert!(legacy["profiles"].as_object().unwrap().is_empty());
        let profile = &accounts.all_profiles()["openai-codex"][0];
        assert_eq!(profile.alias, "openai-codex~work");
        assert_eq!(profile.credential, Some(CredentialKind::OAuth));
        // Loading again changes nothing.
        let _ = fixture.accounts();
        assert_eq!(fixture.pi_auth(), auth);
    }

    #[test]
    fn migration_never_overwrites_an_existing_alias_credential() {
        let fixture = Fixture::new();
        let root = fixture.0.join(".orbit-pi");
        let dir = root.join("provider-accounts/account-1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("auth.json"),
            r#"{"p":{"type":"api_key","key":"old"}}"#,
        )
        .unwrap();
        fs::write(
            root.join("provider-accounts.json"),
            r#"{"version":1,"profiles":{"account-1":{"provider":"p","label":"W"}},"defaults":{},"sessions":{}}"#,
        )
        .unwrap();
        fixture.write_pi_auth(r#"{"p~w":{"type":"api_key","key":"live"}}"#);
        let _ = fixture.accounts();
        assert_eq!(fixture.pi_auth()["p~w"]["key"], "live");
    }

    #[test]
    fn corrupt_or_deleted_store_is_unhealthy_not_empty() {
        let fixture = Fixture::new();
        let mut accounts = fixture.accounts();
        accounts.add("p", "Work").unwrap();
        fs::write(fixture.0.join(".orbit-pi/accounts.json"), b"{}").unwrap();
        assert!(!fixture.accounts().is_healthy());
        fs::remove_file(fixture.0.join(".orbit-pi/accounts.json")).unwrap();
        assert!(!accounts.is_healthy());
    }

    #[test]
    fn session_plumbing_never_selects_or_routes() {
        let fixture = Fixture::new();
        let mut accounts = fixture.accounts();
        accounts.add("p", "Work").unwrap();
        let path = fixture.0.join("s.jsonl");
        accounts.activate_session(&path).unwrap();
        assert!(accounts.selected("p").is_none());
        assert!(accounts.selection_for_session(&path).unwrap().is_empty());
        assert!(accounts
            .record_session(&path, &AccountSelection::new())
            .is_ok());
        let stale = AccountSelection::from([("p".into(), "account-x".into())]);
        assert!(accounts.record_session(&path, &stale).is_err());
        assert!(accounts.routes_for(&stale).is_err());
    }
}
