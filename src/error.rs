//! Main Crate Error

use reqwest::StatusCode;

#[derive(Debug)]
pub enum Error {
    RequestFailed(StatusCode),
    EmptyResponse,
    InvalidUrl(String),

    // -- External --
    IO(std::io::Error),
    Reqwest(reqwest::Error),
    TracingDispatcherSetGlobalDefault(tracing::dispatcher::SetGlobalDefaultError),
    RegexPattern(regex::Error),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequestFailed(status_code) => {
                write!(f, "Request failed with status code: {status_code}")
            }
            Self::EmptyResponse => write!(f, "Received empty response"),
            Self::InvalidUrl(url) => write!(f, "Invalid URL: {url}"),

            // -- External --
            Self::IO(err) => write!(f, "IO error: {err}"),
            Self::Reqwest(err) => write!(f, "Reqwest error: {err}"),
            Self::TracingDispatcherSetGlobalDefault(err) => {
                write!(f, "Tracing dispatcher error: {err}")
            }
            Self::RegexPattern(err) => write!(f, "Regex pattern error: {err}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IO(value)
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Reqwest(value)
    }
}

impl From<tracing::dispatcher::SetGlobalDefaultError> for Error {
    fn from(value: tracing::dispatcher::SetGlobalDefaultError) -> Self {
        Self::TracingDispatcherSetGlobalDefault(value)
    }
}

impl From<regex::Error> for Error {
    fn from(value: regex::Error) -> Self {
        Self::RegexPattern(value)
    }
}
