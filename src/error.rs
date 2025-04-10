//! Main Crate Error

use reqwest::StatusCode;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Request failed: {0}")]
    RequestFailed(StatusCode),

    #[error("Empty response")]
    EmptyResponse,

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    TracingDispatcherSetGlobalDefault(#[from] tracing::dispatcher::SetGlobalDefaultError),

    #[error(transparent)]
    RegexPattern(#[from] regex::Error),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}
