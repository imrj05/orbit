//! Account-level provider quota, balance, and spend for Settings → Providers.
//!
//! Orbit performs no provider HTTP and never sees a credential. The pi RPC
//! server resolves each connected provider's auth, queries the provider's own
//! usage endpoint, and returns normalized, non-secret reports (see
//! `crates/orbit-rpc/docs/quota-rpc.md`). This module is a small reducer over
//! the `quota.list` response:
//!
//! - It records whether the running pi build exposes `quota.*` at all.
//! - It caches one [`QuotaReport`] per provider for the UI to render.
//! - It performs no I/O and touches no network; the GPUI layer owns the RPC.
//!
//! The report carries only display facts — percentages, counts, reset times,
//! and monetary balances — so there is no secret boundary to police here.

use std::collections::HashMap;

use orbit_rpc::{parse_quota_reports, QuotaBalance, QuotaKind, QuotaReport, QuotaWindow};
use serde_json::Value;

/// Custom session-entry type the Orbit quota bridge appends. The payload is
/// the `quota.list` response body: `{"providers": [QuotaReport, …]}`.
pub const BRIDGE_ENTRY_TYPE: &str = "orbit:quota";

/// Whether pi exposes the quota RPC namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaSupport {
    /// Not probed yet (or the process restarted; probe again).
    Unknown,
    /// `quota.list` answered successfully.
    Supported,
    /// pi rejected `quota.list`; the Providers page hides quota meters.
    Unsupported,
}

/// What the top-bar quota chip should show, resolved from the cached reports
/// by [`QuotaManager::headline`].
pub enum QuotaHeadline<'a> {
    /// A metered window: draw the rounded bar and its percentage.
    Window {
        report: &'a QuotaReport,
        window: &'a QuotaWindow,
    },
    /// A provider with balances but no metered window.
    Balance {
        report: &'a QuotaReport,
        balance: &'a QuotaBalance,
    },
    /// Reports exist but none carries a number to meter (notes, errors).
    Quiet,
}

/// The non-secret quota cache, keyed by provider id.
pub struct QuotaManager {
    support: QuotaSupport,
    reports: HashMap<String, QuotaReport>,
}

/// The id a shared surface should treat as the account: the alias when
/// `provider` names a sibling of another provider id, otherwise the id itself.
/// `ollama` and `ollama-cloud` are one ollama.com account, so the popover keys
/// both to `ollama` and the hide list stores that id — a card hidden on one id
/// stays hidden when the active model moves to the other.
pub fn canonical_provider(provider: &str) -> &str {
    crate::providers::provider_alias(provider).unwrap_or(provider)
}

/// Collapse provider ids that resolve to one account into a single report.
/// `ollama` and `ollama-cloud` are answered by the same ollama.com account (the
/// adapter borrows a key stored under either id), so their two reports carry
/// the same windows and balances and would render twice.
///
/// Within a group the report carrying real data wins, so an errored sibling
/// cannot blank a working card; ties go to the active provider's id so the
/// card names the model in use. A group with nothing but errors keeps one
/// member, letting the failure surface instead of vanishing into the collapse.
fn collapse_accounts<'a>(reports: Vec<&'a QuotaReport>, active: &str) -> Vec<&'a QuotaReport> {
    let mut collapsed: Vec<&'a QuotaReport> = Vec::with_capacity(reports.len());
    let mut groups: HashMap<&'a str, usize> = HashMap::new();
    for report in reports {
        let account = canonical_provider(&report.provider);
        let Some(&slot) = groups.get(account) else {
            groups.insert(account, collapsed.len());
            collapsed.push(report);
            continue;
        };
        let kept = collapsed[slot];
        if (report.has_data(), report.provider == active)
            > (kept.has_data(), kept.provider == active)
        {
            collapsed[slot] = report;
        }
    }
    collapsed
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaManager {
    pub fn new() -> Self {
        Self {
            support: QuotaSupport::Unknown,
            reports: HashMap::new(),
        }
    }

    pub fn support(&self) -> QuotaSupport {
        self.support
    }

    /// The cached report for a provider, if one has been fetched.
    pub fn report(&self, id: &str) -> Option<&QuotaReport> {
        self.reports.get(id)
    }

    /// Every cached report, ordered by provider id for a stable UI. Reports
    /// with nothing to render (no windows, balances, note, or error) are
    /// omitted so the top-bar summary never counts an empty provider.
    pub fn reports(&self) -> Vec<&QuotaReport> {
        let mut reports: Vec<&QuotaReport> = self
            .reports
            .values()
            .filter(|report| has_something(report))
            .collect();
        reports.sort_by(|a, b| a.provider.cmp(&b.provider));
        reports
    }

    /// Reports shown in the usage popover, with the active provider first.
    /// Ids that resolve to one account collapse to a single card (see
    /// [`canonical_provider`]), so neither the list nor the header count read
    /// from it can show the same account twice. Unsupported providers remain
    /// available through `report` for settings.
    pub fn usage_reports(&self, active_provider: &str) -> Vec<&QuotaReport> {
        let mut reports = self.reports();
        reports.retain(|report| report.kind != QuotaKind::Unsupported);
        let mut reports = collapse_accounts(reports, active_provider);
        reports.sort_by_key(|report| report.provider != active_provider);
        reports
    }

    /// What the top-bar pill should meter for the provider the active model
    /// is using, in priority order:
    ///
    /// 1. the active provider's rolling 5-hour window;
    /// 2. the active provider's most-constrained window;
    /// 3. the active provider's first balance;
    /// 4. the 5-hour window closest to its limit on any other provider;
    /// 5. the most-constrained window on any other provider.
    ///
    /// Steps 4–5 keep the pill useful while the active model runs on a
    /// provider with no usage surface (a local model, say) and a metered
    /// account is connected. Errored reports never meter — their windows may
    /// be stale, and the popover carries the error.
    pub fn headline(&self, active_provider: &str) -> QuotaHeadline<'_> {
        let active = self
            .reports
            .get(active_provider)
            .filter(|report| report.error.is_none() && has_something(report));
        if let Some(report) = active {
            if let Some(window) = best_window(&report.windows) {
                return QuotaHeadline::Window { report, window };
            }
            if let Some(balance) = report.balances.first() {
                return QuotaHeadline::Balance { report, balance };
            }
        }
        if let Some((report, window)) = self
            .peak_window_matching(is_five_hour_window)
            .or_else(|| self.peak_window())
        {
            return QuotaHeadline::Window { report, window };
        }
        QuotaHeadline::Quiet
    }

    /// The single most-constraining window across every provider: the highest
    /// fraction used, preferring a window without an error. `None` when no
    /// provider reports a percentage or a used/limit pair. The pill's
    /// fallback when the active provider has nothing to meter.
    pub fn peak_window(&self) -> Option<(&QuotaReport, &QuotaWindow)> {
        self.peak_window_matching(|_| true)
    }

    /// [`Self::peak_window`] restricted to windows matching `accept`.
    fn peak_window_matching(
        &self,
        accept: impl Fn(&QuotaWindow) -> bool,
    ) -> Option<(&QuotaReport, &QuotaWindow)> {
        let mut peak: Option<(&QuotaReport, &QuotaWindow)> = None;
        for report in self.reports() {
            if report.error.is_some() {
                continue;
            }
            for window in report.windows.iter().filter(|window| accept(window)) {
                let Some(fraction) = window.fraction() else {
                    continue;
                };
                let better = peak
                    .as_ref()
                    .map(|(_, best)| fraction > best.fraction().unwrap_or(0.0))
                    .unwrap_or(true);
                if better {
                    peak = Some((report, window));
                }
            }
        }
        peak
    }

    /// Apply a `quota.list` response. A namespace-level failure marks quota
    /// unsupported; a successful response merges each report by provider id,
    /// so a single-provider refresh does not drop the others.
    pub fn on_response(
        &mut self,
        success: bool,
        data: Option<&serde_json::Value>,
        error: Option<&str>,
    ) {
        if !success {
            if error.is_some_and(is_unsupported_error) {
                self.support = QuotaSupport::Unsupported;
            }
            return;
        }
        self.support = QuotaSupport::Supported;
        let Some(data) = data else {
            return;
        };
        self.merge(data);
    }

    /// Merge the newest bridge snapshot from a `get_entries` response. The
    /// bridge appends one `orbit:quota` custom entry per changed snapshot, so
    /// only the last matching entry in the slice matters. Returns `true` when
    /// a snapshot was applied — the caller repaints. Bridge data never
    /// touches [`QuotaSupport`]: it arrives regardless of whether pi exposes
    /// the `quota.*` RPC namespace.
    pub fn on_entries(&mut self, entries: &[Value]) -> bool {
        let snapshot = entries.iter().rev().find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("custom")
                && entry.get("customType").and_then(Value::as_str) == Some(BRIDGE_ENTRY_TYPE)
        });
        let Some(data) = snapshot.and_then(|entry| entry.get("data")) else {
            return false;
        };
        self.merge(data)
    }

    /// Insert every report carried by `data`, keyed by provider id.
    fn merge(&mut self, data: &Value) -> bool {
        let reports = parse_quota_reports(data);
        if reports.is_empty() {
            return false;
        }
        for report in reports {
            self.reports.insert(report.provider.clone(), report);
        }
        true
    }

    /// The pi process went away: forget the capability probe so the next
    /// process is re-checked, but keep the last reports so cards do not blink
    /// empty during a restart.
    pub fn on_disconnect(&mut self) {
        self.support = QuotaSupport::Unknown;
    }
}

fn is_unsupported_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("unknown command") || lower.contains("unsupported")
}

/// Whether a report's `note` should render. A note normally accompanies a
/// report with nothing else to show (an unsupported provider, a balance-only
/// account). It also renders beneath metered windows when any of them carries
/// no reset time, so a provider whose usage API omits one countdown (Ollama
/// Cloud's monthly credits reset on the plan anniversary, which the payload
/// never carries) explains its absence instead of leaving a bare meter.
/// Errored reports render their error, never the note.
pub fn note_should_render(report: &QuotaReport) -> bool {
    report.error.is_none()
        && report.note.is_some()
        && (!report.has_data()
            || report
                .windows
                .iter()
                .any(|window| window.resets_at.is_none()))
}

/// Whether a report carries anything the UI can render.
fn has_something(report: &QuotaReport) -> bool {
    report.has_data() || report.error.is_some() || report.note.is_some()
}

/// Whether a window is a provider's rolling 5-hour limit, whatever the
/// adapter names it: id `five_hour` / `5h` / `primary` / `rolling`, label
/// `5-hour`, `Rolling 5h`, `5-hour session`. Separators and the spelled-out
/// `five` are normalized, so a new adapter's wording needs no UI change.
pub fn is_five_hour_window(window: &QuotaWindow) -> bool {
    let matches = |text: &str| {
        let normalized = text
            .to_ascii_lowercase()
            .replace("five", "5")
            .replace(['-', '_'], " ");
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        tokens.contains(&"5h") || tokens.windows(2).any(|pair| pair == ["5", "hour"])
    };
    matches(&window.id) || matches(&window.label)
}

/// The window a provider's meter should headline: its 5-hour limit when one
/// is metered, otherwise the window closest to its limit.
fn best_window(windows: &[QuotaWindow]) -> Option<&QuotaWindow> {
    windows
        .iter()
        .find(|window| is_five_hour_window(window) && window.fraction().is_some())
        .or_else(|| {
            windows
                .iter()
                .filter(|window| window.fraction().is_some())
                .max_by(|a, b| {
                    a.fraction()
                        .unwrap_or(0.0)
                        .total_cmp(&b.fraction().unwrap_or(0.0))
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> serde_json::Value {
        json!({
            "providers": [
                {"provider":"anthropic","kind":"subscription","plan":"Max",
                 "windows":[{"id":"five_hour","label":"5-hour","usedPercent":6.0,"resetsAt":1738300000000i64}]},
                {"provider":"deepseek","kind":"balance","windows":[],
                 "balances":[{"label":"Available","amount":110.0,"currency":"CNY"}]}
            ]
        })
    }

    #[test]
    fn parses_and_caches_reports() {
        let mut quota = QuotaManager::new();
        assert_eq!(quota.support(), QuotaSupport::Unknown);
        quota.on_response(true, Some(&payload()), None);
        assert_eq!(quota.support(), QuotaSupport::Supported);
        let anthropic = quota.report("anthropic").expect("anthropic report");
        assert_eq!(anthropic.plan.as_deref(), Some("Max"));
        assert_eq!(anthropic.windows.len(), 1);
        assert!(quota.report("deepseek").unwrap().has_data());
    }

    #[test]
    fn merges_single_provider_refresh() {
        let mut quota = QuotaManager::new();
        quota.on_response(true, Some(&payload()), None);
        quota.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"anthropic","kind":"subscription",
                "windows":[{"id":"weekly","label":"Weekly","usedPercent":50.0}]}]}),
            ),
            None,
        );
        // The refreshed provider is replaced; the untouched one survives.
        assert_eq!(quota.report("anthropic").unwrap().windows[0].id, "weekly");
        assert!(quota.report("deepseek").is_some());
    }

    #[test]
    fn unsupported_pi_marks_fallback_and_ignores_errors() {
        let mut quota = QuotaManager::new();
        quota.on_response(false, None, Some("Unknown command: quota.list"));
        assert_eq!(quota.support(), QuotaSupport::Unsupported);
        // An unrelated command error must not flip support.
        let mut quota = QuotaManager::new();
        quota.on_response(false, None, Some("network hiccup"));
        assert_eq!(quota.support(), QuotaSupport::Unknown);
    }

    #[test]
    fn disconnect_keeps_reports_but_reprobes() {
        let mut quota = QuotaManager::new();
        quota.on_response(true, Some(&payload()), None);
        quota.on_disconnect();
        assert_eq!(quota.support(), QuotaSupport::Unknown);
        assert!(quota.report("anthropic").is_some());
    }

    #[test]
    fn note_renders_under_windows_that_expose_no_reset() {
        // A metered report with no reset time still surfaces its note, so an
        // API-only Ollama account is told how to see the countdown.
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"ollama","kind":"subscription",
                "windows":[{"id":"session","label":"5-hour session","usedPercent":1.0},
                           {"id":"weekly","label":"Weekly","usedPercent":38.1}],
                "note":"add a session cookie"}]}),
            ),
            None,
        );
        assert!(note_should_render(quota.report("ollama").unwrap()));

        // Once a window carries a reset, the note steps aside for the meter.
        let mut dated = QuotaManager::new();
        dated.on_response(
            true,
            Some(&json!({"providers":[{"provider":"ollama","kind":"subscription",
                "windows":[{"id":"session","label":"5-hour session","usedPercent":1.0,"resetsAt":1}],
                "note":"add a session cookie"}]})),
            None,
        );
        assert!(!note_should_render(dated.report("ollama").unwrap()));

        // An error owns the row; the note never renders beside it.
        let mut errored = QuotaManager::new();
        errored.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"ollama","kind":"subscription",
                "windows":[],"note":"add a session cookie","error":"429"}]}),
            ),
            None,
        );
        assert!(!note_should_render(errored.report("ollama").unwrap()));

        // One dated window is not enough to silence a note that explains a
        // *different* window's missing countdown (Ollama's monthly credits).
        let mut mixed = QuotaManager::new();
        mixed.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"ollama","kind":"subscription",
                "windows":[{"id":"session","label":"5-hour session","usedPercent":1.0,"resetsAt":1},
                           {"id":"monthly","label":"Monthly credits","usedPercent":28.0}],
                "note":"Monthly usage credits reset on your plan's anniversary."}]}),
            ),
            None,
        );
        assert!(note_should_render(mixed.report("ollama").unwrap()));

        // Every window dated: the note would be a stale caveat, so it drops.
        let mut fully_dated = QuotaManager::new();
        fully_dated.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"ollama","kind":"subscription",
                "windows":[{"id":"session","label":"5-hour session","usedPercent":1.0,"resetsAt":1},
                           {"id":"weekly","label":"Weekly","usedPercent":38.0,"resetsAt":2}],
                "note":"Ollama Cloud did not report reset times for every window."}]}),
            ),
            None,
        );
        assert!(!note_should_render(fully_dated.report("ollama").unwrap()));

        // The classic note case: nothing else to show.
        let mut empty = QuotaManager::new();
        empty.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"groq","kind":"unsupported",
                "windows":[],"balances":[],"note":"Groq exposes no usage API."}]}),
            ),
            None,
        );
        assert!(note_should_render(empty.report("groq").unwrap()));
    }

    #[test]
    fn reports_are_sorted_and_skip_empty_entries() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"zai","kind":"subscription",
                 "windows":[{"id":"5h","label":"5-hour","usedPercent":10.0,"resetsAt":1}]},
                {"provider":"anthropic","kind":"subscription",
                 "windows":[{"id":"7d","label":"Weekly","usedPercent":20.0,"resetsAt":2}]},
                {"provider":"groq","kind":"unsupported","windows":[],"balances":[]}
            ]})),
            None,
        );
        let providers: Vec<&str> = quota
            .reports()
            .iter()
            .map(|r| r.provider.as_str())
            .collect();
        // Sorted by id; the empty groq report is omitted.
        assert_eq!(providers, vec!["anthropic", "zai"]);
    }

    #[test]
    fn usage_reports_put_active_provider_first_and_hide_unsupported() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"anthropic","kind":"subscription","note":"Usage unavailable"},
                {"provider":"groq","kind":"unsupported","note":"No usage API"},
                {"provider":"zai","kind":"subscription","error":"Rate limited"}
            ]})),
            None,
        );
        let providers: Vec<_> = quota
            .usage_reports("zai")
            .into_iter()
            .map(|r| r.provider.as_str())
            .collect();
        assert_eq!(providers, ["zai", "anthropic"]);
        assert!(quota.report("groq").is_some());
    }

    #[test]
    fn usage_reports_keep_alphabetical_order_when_active_provider_is_unsupported() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"zai","kind":"subscription","note":"Unavailable"},
                {"provider":"groq","kind":"unsupported","note":"No usage API"},
                {"provider":"anthropic","kind":"subscription","error":"Unavailable"}
            ]})),
            None,
        );
        let providers: Vec<_> = quota
            .usage_reports("groq")
            .into_iter()
            .map(|r| r.provider.as_str())
            .collect();
        assert_eq!(providers, ["anthropic", "zai"]);
    }

    /// One ollama.com account, two provider ids: the popover must show one
    /// card, named after the id the active model is using.
    fn ollama_pair() -> serde_json::Value {
        json!({"providers":[
            {"provider":"ollama","kind":"subscription",
             "windows":[{"id":"session","label":"5-hour session","usedPercent":2.0,"resetsAt":1}]},
            {"provider":"ollama-cloud","kind":"subscription",
             "windows":[{"id":"session","label":"5-hour session","usedPercent":2.0,"resetsAt":1}]}
        ]})
    }

    #[test]
    fn usage_reports_collapse_the_ollama_sibling_pair() {
        let mut quota = QuotaManager::new();
        quota.on_response(true, Some(&ollama_pair()), None);

        // The active id wins, so the card names the model in use.
        for active in ["ollama", "ollama-cloud"] {
            let reports = quota.usage_reports(active);
            assert_eq!(reports.len(), 1, "one card for {active}");
            assert_eq!(reports[0].provider, active);
        }

        // Settings still addresses each id individually.
        assert!(quota.report("ollama").is_some());
        assert!(quota.report("ollama-cloud").is_some());
    }

    #[test]
    fn collapsed_group_prefers_data_over_an_errored_sibling() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"ollama","kind":"subscription","windows":[],"error":"429"},
                {"provider":"ollama-cloud","kind":"subscription",
                 "windows":[{"id":"session","label":"5-hour session","usedPercent":12.0,"resetsAt":1}]}
            ]})),
            None,
        );
        // The active id errors, but the account's numbers still render: a
        // failure must not blank a working card.
        let reports = quota.usage_reports("ollama");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].provider, "ollama-cloud");
        assert!(reports[0].has_data());
    }

    #[test]
    fn collapsed_group_keeps_one_report_when_both_fail() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"ollama","kind":"subscription","windows":[],"error":"429"},
                {"provider":"ollama-cloud","kind":"subscription","windows":[],"error":"503"}
            ]})),
            None,
        );
        // Collapsing must not swallow the failure: one card keeps its error.
        let reports = quota.usage_reports("deepseek");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].error.is_some());
    }

    #[test]
    fn peak_window_picks_the_highest_fraction() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"anthropic","kind":"subscription",
                 "windows":[{"id":"5h","label":"5-hour","usedPercent":6.0,"resetsAt":1}]},
                {"provider":"zai","kind":"subscription",
                 "windows":[{"id":"weekly","label":"Weekly","usedPercent":82.0,"resetsAt":2}]}
            ]})),
            None,
        );
        let (report, window) = quota.peak_window().expect("a peak");
        assert_eq!(report.provider, "zai");
        assert_eq!(window.id, "weekly");
        assert!((window.fraction().unwrap() - 0.82).abs() < 1e-6);
    }

    #[test]
    fn peak_window_ignores_errored_reports() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"anthropic","kind":"subscription","error":"429",
                 "windows":[{"id":"5h","label":"5-hour","usedPercent":99.0,"resetsAt":1}]},
                {"provider":"zai","kind":"subscription",
                 "windows":[{"id":"weekly","label":"Weekly","usedPercent":40.0,"resetsAt":2}]}
            ]})),
            None,
        );
        let (report, _) = quota.peak_window().expect("a peak");
        assert_eq!(report.provider, "zai");
    }

    #[test]
    fn peak_window_is_none_for_balance_only() {
        let mut quota = QuotaManager::new();
        quota.on_response(true, Some(&payload()), None);
        // Only deepseek has a balance and anthropic has a percentage, so the
        // peak is anthropic's window, not None.
        assert_eq!(quota.peak_window().unwrap().0.provider, "anthropic");

        let mut balances = QuotaManager::new();
        balances.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"deepseek","kind":"balance",
                "balances":[{"label":"Available","amount":110.0,"currency":"CNY"}]}]}),
            ),
            None,
        );
        assert!(balances.peak_window().is_none());
        assert_eq!(balances.reports().len(), 1);
    }

    #[test]
    fn headline_prefers_the_active_providers_five_hour_window() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"anthropic","kind":"subscription",
                 "windows":[
                    {"id":"five_hour","label":"5-hour","usedPercent":6.0,"resetsAt":1},
                    {"id":"weekly","label":"Weekly","usedPercent":82.0,"resetsAt":2}]},
                {"provider":"openai-codex","kind":"subscription",
                 "windows":[{"id":"primary","label":"5-hour","usedPercent":99.0,"resetsAt":3}]}
            ]})),
            None,
        );
        // The active provider's 5-hour window, not its higher weekly window
        // and not the other account's busier one.
        match quota.headline("anthropic") {
            QuotaHeadline::Window { report, window } => {
                assert_eq!(report.provider, "anthropic");
                assert_eq!(window.id, "five_hour");
            }
            _ => panic!("expected a window headline"),
        }
    }

    #[test]
    fn headline_falls_back_within_the_active_provider() {
        // No 5-hour window: the window closest to its limit headlines.
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"zai","kind":"subscription",
                 "windows":[
                    {"id":"monthly","label":"Monthly tools","usedPercent":12.0,"resetsAt":1},
                    {"id":"credits","label":"Credits","usedPercent":41.0,"resetsAt":2}]}
            ]})),
            None,
        );
        match quota.headline("zai") {
            QuotaHeadline::Window { window, .. } => assert_eq!(window.id, "credits"),
            _ => panic!("expected a window headline"),
        }

        // No window at all: the balance headlines.
        let mut balances = QuotaManager::new();
        balances.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"deepseek","kind":"balance",
                "balances":[{"label":"Available","amount":110.0,"currency":"CNY"}]}]}),
            ),
            None,
        );
        match balances.headline("deepseek") {
            QuotaHeadline::Balance { report, balance } => {
                assert_eq!(report.provider, "deepseek");
                assert_eq!(balance.amount, 110.0);
            }
            _ => panic!("expected a balance headline"),
        }
    }

    #[test]
    fn headline_falls_back_to_the_accounts_five_hour_window() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"anthropic","kind":"subscription",
                 "windows":[
                    {"id":"five_hour","label":"5-hour","usedPercent":6.0,"resetsAt":1},
                    {"id":"weekly","label":"Weekly","usedPercent":82.0,"resetsAt":2}]}
            ]})),
            None,
        );
        // The active provider (`ollama`) reports nothing, so the account's
        // 5-hour window wins over the more-used weekly window.
        match quota.headline("ollama") {
            QuotaHeadline::Window { report, window } => {
                assert_eq!(report.provider, "anthropic");
                assert_eq!(window.id, "five_hour");
            }
            _ => panic!("expected a window headline"),
        }
    }

    #[test]
    fn headline_skips_errored_reports_and_notes_only_providers() {
        let mut quota = QuotaManager::new();
        quota.on_response(
            true,
            Some(&json!({"providers":[
                {"provider":"anthropic","kind":"subscription","error":"429",
                 "windows":[{"id":"5h","label":"5-hour","usedPercent":99.0,"resetsAt":1}]},
                {"provider":"zai","kind":"subscription",
                 "windows":[{"id":"weekly","label":"Weekly","usedPercent":40.0,"resetsAt":2}]}
            ]})),
            None,
        );
        match quota.headline("anthropic") {
            QuotaHeadline::Window { report, .. } => assert_eq!(report.provider, "zai"),
            _ => panic!("expected the healthy account's window"),
        }

        // Reports exist but none meters: the quiet label keeps the popover
        // reachable.
        let mut notes = QuotaManager::new();
        notes.on_response(
            true,
            Some(
                &json!({"providers":[{"provider":"groq","kind":"unsupported",
                "windows":[],"balances":[],"note":"Groq exposes no usage API."}]}),
            ),
            None,
        );
        assert!(matches!(notes.headline("groq"), QuotaHeadline::Quiet));
    }

    #[test]
    fn five_hour_matcher_covers_adapter_wording() {
        let window = |id: &str, label: &str| QuotaWindow {
            id: id.into(),
            label: label.into(),
            used_percent: None,
            used: None,
            limit: None,
            unit: None,
            resets_at: None,
        };
        // The ids and labels the bundled adapters emit.
        for (id, label) in [
            ("five_hour", "5-hour"),
            ("5h", "5-hour"),
            ("primary", "5-hour"),
            ("rolling", "Rolling 5h"),
            ("session", "5-hour session"),
            ("time_limit", "5-hour"),
        ] {
            assert!(is_five_hour_window(&window(id, label)), "{id}/{label}");
        }
        // Weekly, monthly, and a hypothetical 15-hour window stay out.
        for (id, label) in [
            ("weekly", "Weekly"),
            ("monthly", "Monthly tools"),
            ("credits", "Credits"),
            ("15h", "15-hour"),
        ] {
            assert!(!is_five_hour_window(&window(id, label)), "{id}/{label}");
        }
    }

    #[test]
    fn bridge_entries_merge_without_flipping_support() {
        let mut quota = QuotaManager::new();
        let entries = vec![
            json!({"type":"message","id":"m1"}),
            json!({"type":"custom","customType":"other","data":{}}),
            json!({"type":"custom","customType":BRIDGE_ENTRY_TYPE,"data":payload()}),
        ];
        assert!(quota.on_entries(&entries));
        // Bridge data is not the `quota.*` namespace: support stays unknown
        // so a patched pi is still probed, and the report renders either way.
        assert_eq!(quota.support(), QuotaSupport::Unknown);
        assert_eq!(
            quota.report("anthropic").unwrap().plan.as_deref(),
            Some("Max")
        );
        assert!(quota.report("deepseek").is_some());
    }

    #[test]
    fn bridge_entries_use_the_newest_snapshot() {
        let mut quota = QuotaManager::new();
        let entries = vec![
            json!({"type":"custom","customType":BRIDGE_ENTRY_TYPE,"data":payload()}),
            json!({"type":"custom","customType":BRIDGE_ENTRY_TYPE,"data":{
                "providers":[{"provider":"anthropic","kind":"subscription",
                "windows":[{"id":"weekly","label":"Weekly","usedPercent":50.0}]}]
            }}),
        ];
        assert!(quota.on_entries(&entries));
        assert_eq!(quota.report("anthropic").unwrap().windows[0].id, "weekly");
        // The older snapshot is ignored entirely: its deepseek report is not
        // merged alongside the newer payload.
        assert!(quota.report("deepseek").is_none());
    }

    #[test]
    fn bridge_entries_absent_or_empty_are_a_noop() {
        let mut quota = QuotaManager::new();
        assert!(!quota.on_entries(&[]));
        assert!(!quota.on_entries(&[json!({"type":"message"})]));
        assert!(!quota.on_entries(&[
            json!({"type":"custom","customType":BRIDGE_ENTRY_TYPE,"data":{"providers":[]}})
        ]));
        assert!(quota.reports().is_empty());
    }
}
