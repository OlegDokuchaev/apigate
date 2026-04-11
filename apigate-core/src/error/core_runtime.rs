use http::StatusCode;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApigateCoreError {
    #[error("invalid header name")]
    InvalidHeaderName,
    #[error("invalid header value")]
    InvalidHeaderValue,
    #[error("invalid path parameters")]
    InvalidPathParameters,
    #[error("no backends selected by balancer")]
    NoBackendsSelectedByBalancer,
    #[error("balancer returned invalid backend index")]
    InvalidBackendIndex,
    #[error("no backends")]
    NoBackends,
    #[error("bad upstream uri")]
    InvalidUpstreamUri,
    #[error("upstream request failed")]
    UpstreamRequestFailed,
    #[error("upstream request timed out")]
    UpstreamRequestTimedOut,
}

impl ApigateCoreError {
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

    pub fn debug_details(&self) -> Option<&str> {
        None
    }

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
