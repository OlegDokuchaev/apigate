use http::StatusCode;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApigatePipelineError {
    #[error("missing value in request scope")]
    MissingFromScope(&'static str),
    #[error("request body already consumed")]
    RequestBodyAlreadyConsumed,
    #[error("request body is too large")]
    RequestBodyTooLarge(String),
    #[error("invalid json body")]
    InvalidJsonBody(String),
    #[error("failed to serialize mapped json")]
    FailedSerializeMappedJson(String),
    #[error("invalid query")]
    InvalidQuery(String),
    #[error("failed to serialize mapped query")]
    FailedSerializeMappedQuery(String),
    #[error("failed to rebuild uri")]
    FailedRebuildUri(String),
    #[error("expected application/x-www-form-urlencoded")]
    ExpectedFormUrlEncoded,
    #[error("invalid form query")]
    InvalidFormQuery(String),
    #[error("failed to serialize mapped form")]
    FailedSerializeMappedForm(String),
    #[error("invalid form body")]
    InvalidFormBody(String),
}

impl ApigatePipelineError {
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
