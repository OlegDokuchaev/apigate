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

/// Error returned from hooks and maps.
///
/// Framework errors are rendered through the configured error renderer.
/// Custom responses created with [`Self::from_response`] bypass that renderer.
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

    /// Creates a framework-rendered HTTP error with a custom status and message.
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(status, message)
    }

    /// Creates a `400 Bad Request` framework-rendered error.
    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::BAD_REQUEST, message)
    }

    /// Creates a `401 Unauthorized` framework-rendered error.
    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::UNAUTHORIZED, message)
    }

    /// Creates a `403 Forbidden` framework-rendered error.
    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::FORBIDDEN, message)
    }

    /// Creates a `413 Payload Too Large` framework-rendered error.
    pub fn payload_too_large(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::PAYLOAD_TOO_LARGE, message)
    }

    /// Creates a `415 Unsupported Media Type` framework-rendered error.
    pub fn unsupported_media_type(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::UNSUPPORTED_MEDIA_TYPE, message)
    }

    /// Creates a `502 Bad Gateway` framework-rendered error.
    pub fn bad_gateway(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::BAD_GATEWAY, message)
    }

    /// Creates a `504 Gateway Timeout` framework-rendered error.
    pub fn gateway_timeout(message: impl Into<Cow<'static, str>>) -> Self {
        Self::http(StatusCode::GATEWAY_TIMEOUT, message)
    }

    /// Creates a `500 Internal Server Error` framework-rendered error.
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

    /// Stores a JSON response with `400 Bad Request`.
    pub fn bad_request_json<T>(body: T) -> Self
    where
        axum::Json<T>: IntoResponse,
    {
        Self::json(StatusCode::BAD_REQUEST, body)
    }

    /// Stores a JSON response with `401 Unauthorized`.
    pub fn unauthorized_json<T>(body: T) -> Self
    where
        axum::Json<T>: IntoResponse,
    {
        Self::json(StatusCode::UNAUTHORIZED, body)
    }

    /// Stores a JSON response with `403 Forbidden`.
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
