//! The strongly typed event model.
//!
//! Every event is a closed enum variant with no free-form payload. Properties
//! are constructed explicitly by [`AnalyticsEvent::properties`] — application
//! state is never serialized wholesale into a payload.

use std::collections::BTreeMap;

use serde_json::Value;

/// Why an agent run failed, at product-useful granularity only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFailure {
    /// The provider/model returned an error message.
    Provider,
    /// The pi process exited unexpectedly.
    ProcessExited,
    /// A tool or extension reported an error.
    Tool,
    /// Anything else.
    Other,
}

impl AgentFailure {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::ProcessExited => "process_exit",
            Self::Tool => "tool",
            Self::Other => "other",
        }
    }
}

/// Coarse theme mode. Custom palette names are never sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// Which setting changed. The value is never sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingId {
    Appearance,
    Language,
    FontSize,
    Spacing,
    ReduceMotion,
    Analytics,
    Composer,
    AutoTitle,
}

impl SettingId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Language => "language",
            Self::FontSize => "font_size",
            Self::Spacing => "spacing",
            Self::ReduceMotion => "reduce_motion",
            Self::Analytics => "analytics",
            Self::Composer => "composer",
            Self::AutoTitle => "auto_title",
        }
    }
}

/// A product-telemetry event. Unit variants match the call-site API:
/// `analytics.track(AnalyticsEvent::AgentStarted)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsEvent {
    AppStarted,
    AppClosed,
    SessionStarted,
    SessionEnded,
    ProjectCreated,
    ProjectOpened,
    ProjectDeleted,
    TerminalOpened,
    TerminalSessionCreated,
    AgentStarted,
    AgentCompleted,
    AgentFailed { reason: AgentFailure },
    McpConnected,
    McpDisconnected,
    McpConnectionFailed,
    CommandPaletteOpened,
    SettingsOpened,
    SettingsChanged { setting: SettingId },
    ThemeChanged { mode: ThemeMode },
    UpdateAvailable,
    UpdateInstalled,
}

impl AnalyticsEvent {
    /// Countly event key. Stable across releases.
    pub fn name(self) -> &'static str {
        match self {
            Self::AppStarted => "app_started",
            Self::AppClosed => "app_closed",
            Self::SessionStarted => "session_started",
            Self::SessionEnded => "session_ended",
            Self::ProjectCreated => "project_created",
            Self::ProjectOpened => "project_opened",
            Self::ProjectDeleted => "project_deleted",
            Self::TerminalOpened => "terminal_opened",
            Self::TerminalSessionCreated => "terminal_session_created",
            Self::AgentStarted => "agent_started",
            Self::AgentCompleted => "agent_completed",
            Self::AgentFailed { .. } => "agent_failed",
            Self::McpConnected => "mcp_connected",
            Self::McpDisconnected => "mcp_disconnected",
            Self::McpConnectionFailed => "mcp_connection_failed",
            Self::CommandPaletteOpened => "command_palette_opened",
            Self::SettingsOpened => "settings_opened",
            Self::SettingsChanged { .. } => "settings_changed",
            Self::ThemeChanged { .. } => "theme_changed",
            Self::UpdateAvailable => "update_available",
            Self::UpdateInstalled => "update_installed",
        }
    }

    /// Event-specific, non-sensitive properties. Global metadata (version, OS,
    /// architecture, locale) is attached by the request builder.
    pub fn properties(self) -> BTreeMap<String, Value> {
        let mut properties = BTreeMap::new();
        match self {
            Self::AgentFailed { reason } => {
                properties.insert("reason".into(), Value::String(reason.as_str().into()));
            }
            Self::SettingsChanged { setting } => {
                properties.insert("setting".into(), Value::String(setting.as_str().into()));
            }
            Self::ThemeChanged { mode } => {
                properties.insert("mode".into(), Value::String(mode.as_str().into()));
            }
            // Every other event deliberately carries no properties.
            _ => {}
        }
        properties
    }

    /// Every variant, for exhaustive privacy and serialization tests.
    pub const ALL: &'static [AnalyticsEvent] = &[
        AnalyticsEvent::AppStarted,
        AnalyticsEvent::AppClosed,
        AnalyticsEvent::SessionStarted,
        AnalyticsEvent::SessionEnded,
        AnalyticsEvent::ProjectCreated,
        AnalyticsEvent::ProjectOpened,
        AnalyticsEvent::ProjectDeleted,
        AnalyticsEvent::TerminalOpened,
        AnalyticsEvent::TerminalSessionCreated,
        AnalyticsEvent::AgentStarted,
        AnalyticsEvent::AgentCompleted,
        AnalyticsEvent::AgentFailed {
            reason: AgentFailure::Provider,
        },
        AnalyticsEvent::AgentFailed {
            reason: AgentFailure::ProcessExited,
        },
        AnalyticsEvent::AgentFailed {
            reason: AgentFailure::Tool,
        },
        AnalyticsEvent::AgentFailed {
            reason: AgentFailure::Other,
        },
        AnalyticsEvent::McpConnected,
        AnalyticsEvent::McpDisconnected,
        AnalyticsEvent::McpConnectionFailed,
        AnalyticsEvent::CommandPaletteOpened,
        AnalyticsEvent::SettingsOpened,
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::Appearance,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::Language,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::FontSize,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::Spacing,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::ReduceMotion,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::Analytics,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::Composer,
        },
        AnalyticsEvent::SettingsChanged {
            setting: SettingId::AutoTitle,
        },
        AnalyticsEvent::ThemeChanged {
            mode: ThemeMode::Dark,
        },
        AnalyticsEvent::ThemeChanged {
            mode: ThemeMode::Light,
        },
        AnalyticsEvent::UpdateAvailable,
        AnalyticsEvent::UpdateInstalled,
    ];
}

/// An event plus the wall-clock time it was observed. Queued verbatim until a
/// flush succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedEvent {
    pub event: AnalyticsEvent,
    pub timestamp_ms: i64,
}

impl QueuedEvent {
    pub fn new(event: AnalyticsEvent, timestamp_ms: i64) -> Self {
        Self {
            event,
            timestamp_ms,
        }
    }

    pub fn name(self) -> &'static str {
        self.event.name()
    }

    pub fn properties(self) -> BTreeMap<String, Value> {
        self.event.properties()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_has_a_stable_name() {
        for event in AnalyticsEvent::ALL {
            let name = event.name();
            assert!(!name.is_empty());
            assert!(
                !name.starts_with("[CLY]_"),
                "{name} is a reserved Countly key"
            );
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{name} is not a safe event key"
            );
        }
    }

    #[test]
    fn no_event_properties_are_forbidden() {
        for event in AnalyticsEvent::ALL {
            let mut properties = event.properties();
            let removed = crate::privacy::sanitize(&mut properties);
            assert_eq!(removed, 0, "{} carries a forbidden property", event.name());
        }
    }

    #[test]
    fn properties_are_small_and_typed() {
        for event in AnalyticsEvent::ALL {
            // No event carries more than a couple of scalar properties.
            assert!(
                event.properties().len() <= 1,
                "{} carries too many properties",
                event.name()
            );
        }
    }
}
