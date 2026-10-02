//! Privacy-first, provider-independent product analytics for Orbit.
//!
//! The application only ever talks to [`AnalyticsClient`]; the Countly HTTP
//! transport lives behind the [`Transport`] seam so a different backend can be
//! swapped in without touching call sites. The public surface is the
//! [`Analytics`] trait:
//!
//! ```
//! use orbit_analytics::{Analytics, AnalyticsClient, AnalyticsEvent};
//! # fn demo(client: &AnalyticsClient) {
//! client.track(AnalyticsEvent::AppStarted);
//! client.track(AnalyticsEvent::ProjectOpened);
//! client.track(AnalyticsEvent::AgentStarted);
//! # }
//! ```
//!
//! Design rules enforced here:
//!
//! * `track` never blocks on the network. Events go into a bounded in-memory
//!   queue and a single background worker flushes them in batches.
//! * Payloads are built explicitly from a closed event enum. Application state
//!   is never serialized wholesale, and every event's properties are vetted by
//!   [`privacy`].
//! * The installation id is a random UUID persisted locally; the session id is
//!   a fresh UUID per launch. Neither is derived from hardware or account data.
//! * Network failures are bounded and non-fatal: retry with exponential backoff
//!   up to a queue limit, then drop. Analytics never panics.
//! * Only HTTPS endpoints are accepted (loopback HTTP is allowed for tests).

mod client;
mod config;
mod countly;
mod event;
mod identity;
mod privacy;
mod queue;

pub use client::{Analytics, AnalyticsClient, NoopAnalytics};
pub use config::{
    config_from_env, AnalyticsConfig, AppMetadata, ConfigError, Environment, PRODUCTION_ENDPOINT,
};
pub use countly::{build_request, CountlyRequest, CountlyTransport, Transport, TransportError};
pub use event::{AgentFailure, AnalyticsEvent, QueuedEvent, SettingId, ThemeMode};
pub use identity::{FileStore, InMemoryStore, StateStore, StoredState};
pub use privacy::{
    is_forbidden_key, looks_sensitive, sanitize, FORBIDDEN_KEYS, FORBIDDEN_VALUE_MARKERS,
};
