//! Crate prelude

// Re-export the crate Error.
pub use crate::error::Error;

// Alias Result to be the crate Result.
pub type Result<T> = color_eyre::Result<T, Error>;

// Re-export tracing macros for convenience
pub use tracing::{error, info, warn};
