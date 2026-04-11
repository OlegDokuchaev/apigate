use http::header::InvalidHeaderValue;
use http::uri::InvalidUri;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BaseUriParseError {
    #[error("invalid uri `{input}`: {source}")]
    InvalidUri {
        input: String,
        #[source]
        source: InvalidUri,
    },
    #[error("uri `{input}` missing scheme (expected http://...)")]
    MissingScheme { input: String },
    #[error("uri `{input}` missing authority (expected http://host:port)")]
    MissingAuthority { input: String },
    #[error("invalid authority for Host header `{authority}`: {source}")]
    InvalidHostHeader {
        authority: String,
        #[source]
        source: InvalidHeaderValue,
    },
}

#[derive(Debug, Error)]
pub enum ApigateBuildError {
    #[error("backend `{service}` has invalid uri `{url}`: {source}")]
    InvalidBackendUri {
        service: String,
        url: String,
        #[source]
        source: BaseUriParseError,
    },
    #[error("backend for service `{service}` is not registered")]
    BackendNotRegistered { service: &'static str },
    #[error("policy `{name}` is not registered")]
    PolicyNotRegistered { name: &'static str },
}
