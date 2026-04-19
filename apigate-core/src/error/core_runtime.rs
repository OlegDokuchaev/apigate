use http::StatusCode;
use thiserror::Error;

/// Core runtime errors produced by request mutation, dispatch, or proxying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApigateCoreError {
    /// Hook attempted to set an invalid header name.
    #[error("invalid header name")]
    InvalidHeaderName,
    /// Hook attempted to set an invalid header value.
    #[error("invalid header value")]
    InvalidHeaderValue,
    /// Path parameter extraction failed.
    #[error("invalid path parameters")]
    InvalidPathParameters,
    /// Route strategy produced no candidate accepted by the balancer.
    #[error("no backends selected by balancer")]
    NoBackendsSelectedByBalancer,
    /// Balancer returned an index outside the backend pool.
    #[error("balancer returned invalid backend index")]
    InvalidBackendIndex,
    /// Service has no configured backends.
    #[error("no backends")]
    NoBackends,
    /// Failed to build a valid upstream URI.
    #[error("bad upstream uri")]
    InvalidUpstreamUri,
    /// Upstream request failed before a response was received.
    #[error("upstream request failed")]
    UpstreamRequestFailed,
    /// Upstream request timed out.
    #[error("upstream request timed out")]
    UpstreamRequestTimedOut,
}

impl ApigateCoreError {
    /// Returns a user-facing message safe for default HTTP responses.
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::InvalidHeaderName => "invalid header name",
            Self::InvalidHeaderValue => "invalid header value",
            Self::InvalidPathParameters => "invalid path parameters",
            Self::NoBackendsSelectedByBalancer => "no backends selected by balancer",
            Self::InvalidBackendIndex => "balancer returned invalid backend index",
            Self::NoBackends => "no backends",
            Self::InvalidUpstreamUri => "bad upstream uri",
            Self::UpstreamRequestFailed => "upstream request failed",
            Self::UpstreamRequestTimedOut => "upstream request timed out",
        }
    }

    /// Returns diagnostic details intended for logs, not default responses.
    pub fn debug_details(&self) -> Option<&str> {
        None
    }

    /// Returns the default HTTP status for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidHeaderName | Self::InvalidHeaderValue | Self::InvalidPathParameters => {
                StatusCode::BAD_REQUEST
            }
            Self::NoBackendsSelectedByBalancer
            | Self::InvalidBackendIndex
            | Self::NoBackends
            | Self::InvalidUpstreamUri
            | Self::UpstreamRequestFailed => StatusCode::BAD_GATEWAY,
            Self::UpstreamRequestTimedOut => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// Returns a stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidHeaderName => "invalid_header_name",
            Self::InvalidHeaderValue => "invalid_header_value",
            Self::InvalidPathParameters => "invalid_path_parameters",
            Self::NoBackendsSelectedByBalancer => "no_backends_selected",
            Self::InvalidBackendIndex => "invalid_backend_index",
            Self::NoBackends => "no_backends",
            Self::InvalidUpstreamUri => "invalid_upstream_uri",
            Self::UpstreamRequestFailed => "upstream_request_failed",
            Self::UpstreamRequestTimedOut => "upstream_timeout",
        }
    }
}
