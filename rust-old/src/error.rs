//! Error types for the nidhi sampler engine.

use core::fmt;

/// Errors that can occur in nidhi operations.
#[derive(Debug, Clone)]
#[non_exhaustive]
#[must_use]
pub enum NidhiError {
    /// Sample not found in the bank.
    SampleNotFound(SampleId),
    /// Invalid zone configuration.
    InvalidZone(alloc::string::String),
    /// Invalid parameter value.
    InvalidParameter {
        name: alloc::string::String,
        reason: alloc::string::String,
    },
    /// Playback error.
    Playback(alloc::string::String),
    /// Format import error.
    ImportError(alloc::string::String),
}

use crate::sample::SampleId;

impl fmt::Display for NidhiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleNotFound(id) => write!(f, "sample not found: {}", id.0),
            Self::InvalidZone(msg) => write!(f, "invalid zone: {msg}"),
            Self::InvalidParameter { name, reason } => {
                write!(f, "invalid parameter '{name}': {reason}")
            }
            Self::Playback(msg) => write!(f, "playback error: {msg}"),
            Self::ImportError(msg) => write!(f, "import error: {msg}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NidhiError {}

/// Result type alias for nidhi operations.
pub type Result<T> = core::result::Result<T, NidhiError>;
