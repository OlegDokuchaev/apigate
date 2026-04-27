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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use serde::Serialize;

    #[test]
    fn framework_constructor_sugars_preserve_status_message_and_log_status() {
        let cases = [
            (
                ApigateError::bad_request("bad"),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "bad",
            ),
            (
                ApigateError::unauthorized("missing"),
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing",
            ),
            (
                ApigateError::forbidden("no"),
                StatusCode::FORBIDDEN,
                "forbidden",
                "no",
            ),
            (
                ApigateError::payload_too_large("large"),
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "large",
            ),
            (
                ApigateError::unsupported_media_type("media"),
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "media",
            ),
            (
                ApigateError::bad_gateway("upstream"),
                StatusCode::BAD_GATEWAY,
                "bad_gateway",
                "upstream",
            ),
            (
                ApigateError::gateway_timeout("slow"),
                StatusCode::GATEWAY_TIMEOUT,
                "gateway_timeout",
                "slow",
            ),
            (
                ApigateError::internal("boom"),
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "boom",
            ),
            (
                ApigateError::new(StatusCode::TOO_MANY_REQUESTS, "rate"),
                StatusCode::TOO_MANY_REQUESTS,
                "client_error",
                "rate",
            ),
            (
                ApigateError::new(StatusCode::SERVICE_UNAVAILABLE, "down"),
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "down",
            ),
        ];

        for (error, status, code, message) in cases {
            let framework = error.framework_error().unwrap();
            assert_eq!(framework.status_code(), status);
            assert_eq!(framework.code(), code);
            assert_eq!(framework.user_message(), message);
            assert_eq!(error.status_code_for_log(), status);
        }
    }

    #[tokio::test]
    async fn custom_response_bypasses_framework_renderer() {
        let error = ApigateError::from_response((StatusCode::ACCEPTED, "custom"));

        assert!(error.framework_error().is_none());
        assert_eq!(error.status_code_for_log(), StatusCode::ACCEPTED);

        let response = error.into_response_with(&default_error_renderer);
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), b"custom");
    }

    #[tokio::test]
    async fn json_constructor_sugars_return_custom_json_statuses() {
        #[derive(Serialize)]
        struct Body {
            code: &'static str,
        }

        let cases = [
            (
                ApigateError::bad_request_json(Body { code: "bad" }),
                StatusCode::BAD_REQUEST,
                r#"{"code":"bad"}"#,
            ),
            (
                ApigateError::unauthorized_json(Body { code: "auth" }),
                StatusCode::UNAUTHORIZED,
                r#"{"code":"auth"}"#,
            ),
            (
                ApigateError::forbidden_json(Body { code: "forbidden" }),
                StatusCode::FORBIDDEN,
                r#"{"code":"forbidden"}"#,
            ),
        ];

        for (error, status, expected_body) in cases {
            assert!(error.framework_error().is_none());
            assert_eq!(error.status_code_for_log(), status);

            let response = error.into_response_with(&default_error_renderer);
            assert_eq!(response.status(), status);
            assert_eq!(
                response.headers().get(http::header::CONTENT_TYPE).unwrap(),
                "application/json"
            );
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(body.as_ref(), expected_body.as_bytes());
        }
    }
}
