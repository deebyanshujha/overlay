//! One error type for the whole backend. Every Tauri command returns
//! `Result<T, OverlayError>`, which serialises to a tagged object the frontend
//! can branch on instead of pattern-matching English strings.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("{0}")]
    Registry(String),

    #[error("{0}")]
    Win32(String),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("module `{0}` is not registered")]
    UnknownModule(String),

    #[error("module `{id}` is unavailable on this system: {reason}")]
    Unavailable { id: String, reason: String },

    #[error("`{id}` conflicts with `{other}`: {reason}")]
    Conflict {
        id: String,
        other: String,
        reason: String,
    },

    #[error("`{0}` requires `{1}` to be enabled first")]
    MissingDependency(String, String),

    #[error("profile `{0}` was not found")]
    UnknownProfile(String),

    #[error("profile `{0}` is built in and cannot be changed")]
    ProfileImmutable(String),

    #[error("verification failed for `{target}`: expected {expected}, found {found}")]
    VerificationFailed {
        target: String,
        expected: String,
        found: String,
    },

    #[error("`{target}` was changed outside OVERLAY since we recorded it (expected {expected}, found {found})")]
    OwnershipConflict {
        target: String,
        expected: String,
        found: String,
    },

    #[error("{0}")]
    Integration(String),

    #[error("{0}")]
    Other(String),
}

impl OverlayError {
    /// Machine-readable discriminant for the frontend.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Registry(_) => "registry",
            Self::Win32(_) => "win32",
            Self::Db(_) => "database",
            Self::Io(_) => "io",
            Self::Config(_) => "config",
            Self::UnknownModule(_) => "unknown_module",
            Self::Unavailable { .. } => "unavailable",
            Self::Conflict { .. } => "conflict",
            Self::MissingDependency(..) => "missing_dependency",
            Self::UnknownProfile(_) => "unknown_profile",
            Self::ProfileImmutable(_) => "profile_immutable",
            Self::VerificationFailed { .. } => "verification_failed",
            Self::OwnershipConflict { .. } => "ownership_conflict",
            Self::Integration(_) => "integration",
            Self::Other(_) => "other",
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

impl From<anyhow::Error> for OverlayError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e.to_string())
    }
}

impl From<serde_json::Error> for OverlayError {
    fn from(e: serde_json::Error) -> Self {
        Self::Config(e.to_string())
    }
}

impl From<tauri::Error> for OverlayError {
    fn from(e: tauri::Error) -> Self {
        Self::Win32(e.to_string())
    }
}

impl Serialize for OverlayError {
    // Spelled out rather than using this module's `Result` alias, which binds
    // the error type to `OverlayError` and would not match the trait.
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("OverlayError", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type Result<T> = std::result::Result<T, OverlayError>;
