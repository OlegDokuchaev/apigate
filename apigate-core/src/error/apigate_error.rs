use std::borrow::Cow;

use axum::response::{IntoResponse, Response};
use http::StatusCode;

use super::{
    ApigateCoreError, ApigateFrameworkError, ApigatePipelineError, ErrorRenderer,
    default_error_renderer,
};

#[derive(Debug)]
enum ApigateErrorRepr {
    Framework(ApigateFrameworkError),
    Custom(Box<Response>),
}

#[derive(Debug)]
pub struct ApigateError {
    repr: ApigateErrorRepr,
}

impl ApigateError {
    fn framework(error: impl Into<ApigateFrameworkError>) -> Self {
        Self {
            repr: ApigateErrorRepr::Framework(error.into()),
        }
    }

    fn http(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self::framework(ApigateFrameworkError::Http {
            status,
            message: message.into(),
        })
    }

    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(status, message)
    }

    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::BAD_REQUEST, message)
    }

    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::FORBIDDEN, message)
    }

    pub fn payload_too_large(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::PAYLOAD_TOO_LARGE, message)
    }

    pub fn unsupported_media_type(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::UNSUPPORTED_MEDIA_TYPE, message)
    }

    pub fn bad_gateway(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::BAD_GATEWAY, message)
    }

    pub fn gateway_timeout(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::GATEWAY_TIMEOUT, message)
    }

    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Stores a fully custom HTTP response.
    /// Useful for domain-specific JSON error payloads.
    pub fn from_response(response: impl IntoResponse) -> Self {
        Self {
            repr: ApigateErrorRepr::Custom(Box::new(response.into_response())),
        }
    }

    /// Stores a JSON response with a custom HTTP status.
    pub fn json<T>(status: StatusCode, body: T) -> Self
    where
        axum::Json<T>: IntoResponse,
    {
        Self::from_response((status, axum::Json(body)))
    }

    pub fn bad_request_json<T>(body: T) -> Self
    where
        axum::Json<T>: IntoResponse,
    {
        Self::json(StatusCode::BAD_REQUEST, body)
    }

    pub fn unauthorized_json<T>(body: T) -> Self
    where
        axum::Json<T>: IntoResponse,
    {
        Self::json(StatusCode::UNAUTHORIZED, body)
    }

    pub fn forbidden_json<T>(body: T) -> Self
    where
        axum::Json<T>: IntoResponse,
    {
        Self::json(StatusCode::FORBIDDEN, body)
    }

    pub(crate) fn framework_error(&self) -> Option<&ApigateFrameworkError> {
        match &self.repr {
            ApigateErrorRepr::Framework(error) => Some(error),
            ApigateErrorRepr::Custom(_) => None,
        }
    }

    pub(crate) fn status_code_for_log(&self) -> StatusCode {
        match &self.repr {
            ApigateErrorRepr::Framework(error) => error.status_code(),
            ApigateErrorRepr::Custom(response) => response.status(),
        }
    }

    pub(crate) fn into_response_with(self, renderer: &ErrorRenderer) -> Response {
        match self.repr {
            ApigateErrorRepr::Framework(error) => renderer(error),
            ApigateErrorRepr::Custom(response) => *response,
        }
    }
}

impl From<ApigateFrameworkError> for ApigateError {
    fn from(value: ApigateFrameworkError) -> Self {
        Self::framework(value)
    }
}

impl From<ApigateCoreError> for ApigateError {
    fn from(value: ApigateCoreError) -> Self {
        Self::framework(value)
    }
}

impl From<ApigatePipelineError> for ApigateError {
    fn from(value: ApigatePipelineError) -> Self {
        Self::framework(value)
    }
}

impl IntoResponse for ApigateError {
    fn into_response(self) -> Response {
        let renderer = &default_error_renderer;
        self.into_response_with(renderer)
    }
}
