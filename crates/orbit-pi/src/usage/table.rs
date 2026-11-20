//! Usage's tables: which table a column-width override belongs to, and the
//! failures table's sort keys, rows and cell text. The grid itself (frame,
//! header, sort, resize, row and cells) is `ui/table.rs`.

use gpui::Hsla;

use crate::theme::Theme;

/// Which table a column-width override belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TableKind {
    Sessions,
    Buckets,
    Failures,
    Breakdown,
}

/// Sort key for the failures table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FailureSort {
    When,
    Kind,
    Model,
    Session,
    Message,
}

impl FailureSort {
    pub fn id(self) -> &'static str {
        match self {
            Self::When => "When",
            Self::Kind => "Type",
            Self::Model => "Model",
            Self::Session => "Session",
            Self::Message => "message",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::When => tr!("usage.col_when"),
            Self::Kind => tr!("usage.col_type"),
            Self::Model => tr!("usage.col_model"),
            Self::Session => tr!("usage.col_session"),
            Self::Message => tr!("usage.col_message"),
        }
    }

    /// Hideable columns; When is always shown.
    pub const HIDEABLE: [Self; 4] = [Self::Kind, Self::Model, Self::Session, Self::Message];
}

/// One row of the failures table, already resolved against the index so the
/// cells do no lookups while painting.
#[derive(Clone, PartialEq, Debug)]
pub struct FailureRow {
    pub ts_ms: i64,
    pub session: u16,
    pub kind: super::model::ErrorKind,
    pub model: String,
    pub session_title: String,
    pub message: String,
}

/// The failure-kind label and its register: provider errors are critical, tool
/// failures a warning.
pub(super) fn failure_kind_register(kind: super::model::ErrorKind, theme: Theme) -> (String, Hsla) {
    match kind {
        super::model::ErrorKind::Provider => (kind.label(), theme.error),
        super::model::ErrorKind::Tool => (kind.label(), theme.warning),
    }
}

/// Format a failure timestamp for its column.
pub(super) fn failure_when(ts_ms: i64) -> String {
    super::model::local_datetime(ts_ms)
        .map(|dt| dt.format("%b %-d, %H:%M").to_string())
        .unwrap_or_default()
}
