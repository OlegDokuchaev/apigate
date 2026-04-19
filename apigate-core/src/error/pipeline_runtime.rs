use http::StatusCode;
use thiserror::Error;

/// Errors produced by generated request pipelines.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApigatePipelineError {
    /// A generated hook/map wrapper could not find a required value in scope.
    #[error("missing value in request scope")]
    MissingFromScope(&'static str),
    /// Request body was requested after already being consumed.
    #[error("request body already consumed")]
    RequestBodyAlreadyConsumed,
    /// Request body exceeded the configured map body limit.
    #[error("request body is too large")]
    RequestBodyTooLarge(String),
    /// JSON request body could not be deserialized.
    #[error("invalid json body")]
    InvalidJsonBody(String),
    /// Mapped JSON output could not be serialized.
    #[error("failed to serialize mapped json")]
    FailedSerializeMappedJson(String),
    /// Query string could not be deserialized.
    #[error("invalid query")]
    InvalidQuery(String),
    /// Mapped query output could not be serialized.
    #[error("failed to serialize mapped query")]
    FailedSerializeMappedQuery(String),
    /// URI could not be rebuilt after query/form mapping.
    #[error("failed to rebuild uri")]
    FailedRebuildUri(String),
    /// Expected `application/x-www-form-urlencoded` body.
    #[error("expected application/x-www-form-urlencoded")]
    ExpectedFormUrlEncoded,
    /// Form values from the query string could not be deserialized.
    #[error("invalid form query")]
    InvalidFormQuery(String),
    /// Mapped form output could not be serialized.
    #[error("failed to serialize mapped form")]
    FailedSerializeMappedForm(String),
    /// Form body could not be deserialized.
    #[error("invalid form body")]
    InvalidFormBody(String),
}

impl ApigatePipelineError {
    /// Returns a user-facing message safe for default HTTP responses.
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::MissingFromScope(_) => "missing value in request scope",
            Self::RequestBodyAlreadyConsumed => "request body already consumed",
            Self::RequestBodyTooLarge(_) => "request body is too large",
            Self::InvalidJsonBody(_) => "invalid json body",
            Self::FailedSerializeMappedJson(_) => "failed to serialize mapped json",
            Self::InvalidQuery(_) => "invalid query",
            Self::FailedSerializeMappedQuery(_) => "failed to serialize mapped query",
            Self::FailedRebuildUri(_) => "failed to rebuild uri",
            Self::ExpectedFormUrlEncoded => "expected application/x-www-form-urlencoded",
            Self::InvalidFormQuery(_) => "invalid form query",
            Self::FailedSerializeMappedForm(_) => "failed to serialize mapped form",
            Self::InvalidFormBody(_) => "invalid form body",
        }
    }

    /// Returns diagnostic details intended for logs, not default responses.
    pub fn debug_details(&self) -> Option<&str> {
        match self {
            Self::MissingFromScope(ty) => Some(ty),
            Self::RequestBodyTooLarge(detail)
            | Self::InvalidJsonBody(detail)
            | Self::FailedSerializeMappedJson(detail)
            | Self::InvalidQuery(detail)
            | Self::FailedSerializeMappedQuery(detail)
            | Self::FailedRebuildUri(detail)
            | Self::InvalidFormQuery(detail)
            | Self::FailedSerializeMappedForm(detail)
            | Self::InvalidFormBody(detail) => Some(detail.as_str()),
            Self::RequestBodyAlreadyConsumed | Self::ExpectedFormUrlEncoded => None,
        }
    }

    /// Returns the default HTTP status for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingFromScope(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RequestBodyAlreadyConsumed
            | Self::FailedSerializeMappedJson(_)
            | Self::FailedSerializeMappedQuery(_)
            | Self::FailedRebuildUri(_)
            | Self::FailedSerializeMappedForm(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RequestBodyTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ExpectedFormUrlEncoded => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::InvalidJsonBody(_)
            | Self::InvalidQuery(_)
            | Self::InvalidFormQuery(_)
            | Self::InvalidFormBody(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// Returns a stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingFromScope(_) => "missing_from_scope",
            Self::RequestBodyAlreadyConsumed => "request_body_already_consumed",
            Self::RequestBodyTooLarge(_) => "request_body_too_large",
            Self::InvalidJsonBody(_) => "invalid_json_body",
            Self::FailedSerializeMappedJson(_) => "serialize_mapped_json_failed",
            Self::InvalidQuery(_) => "invalid_query",
            Self::FailedSerializeMappedQuery(_) => "serialize_mapped_query_failed",
            Self::FailedRebuildUri(_) => "rebuild_uri_failed",
            Self::ExpectedFormUrlEncoded => "expected_form_urlencoded",
            Self::InvalidFormQuery(_) => "invalid_form_query",
            Self::FailedSerializeMappedForm(_) => "serialize_mapped_form_failed",
            Self::InvalidFormBody(_) => "invalid_form_body",
        }
    }
}
