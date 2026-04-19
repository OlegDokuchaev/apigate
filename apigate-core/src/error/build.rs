use http::header::InvalidHeaderValue;
use http::uri::InvalidUri;
use thiserror::Error;

/// Error returned when parsing an upstream backend base URI.
#[derive(Debug, Error)]
pub enum BaseUriParseError {
    /// URI parser rejected the input.
    #[error("invalid uri `{input}`: {source}")]
    InvalidUri {
        /// Original URI string.
        input: String,
        /// Parser source error.
        #[source]
        source: InvalidUri,
    },
    /// Backend URI has no scheme.
    #[error("uri `{input}` missing scheme (expected http://...)")]
    MissingScheme {
        /// Original URI string.
        input: String,
    },
    /// Backend URI has no authority.
    #[error("uri `{input}` missing authority (expected http://host:port)")]
    MissingAuthority {
        /// Original URI string.
        input: String,
    },
    /// URI authority cannot be used as an HTTP `Host` header.
    #[error("invalid authority for Host header `{authority}`: {source}")]
    InvalidHostHeader {
        /// URI authority string.
        authority: String,
        /// Header value source error.
        #[source]
        source: InvalidHeaderValue,
    },
}

/// Error returned while building an [`crate::App`].
#[derive(Debug, Error)]
pub enum ApigateBuildError {
    /// A registered backend URL is invalid.
    #[error("backend `{service}` has invalid uri `{url}`: {source}")]
    InvalidBackendUri {
        /// Service name owning the backend URL.
        service: String,
        /// Invalid backend URL.
        url: String,
        /// URI parse source error.
        #[source]
        source: BaseUriParseError,
    },
    /// Routes were mounted for a service without registered backends.
    #[error("backend for service `{service}` is not registered")]
    BackendNotRegistered {
        /// Service name without registered backends.
        service: &'static str,
    },
    /// A route or service references an unknown named policy.
    #[error("policy `{name}` is not registered")]
    PolicyNotRegistered {
        /// Missing policy name.
        name: &'static str,
    },
}
