use std::borrow::Cow;

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use thiserror::Error;

use super::{ApigateCoreError, ApigatePipelineError};

#[derive(Debug, Error)]
pub enum ApigateFrameworkError {
    #[error(transparent)]
    Core(#[from] ApigateCoreError),
    #[error(transparent)]
    Pipeline(#[from] ApigatePipelineError),
    #[error("{message}")]
    Http {
        status: StatusCode,
        message: Cow<'static, str>,
    },
}

impl ApigateFrameworkError {
    pub fn user_message(&self) -> &str {
        match self {
            Self::Core(err) => err.user_message(),
            Self::Pipeline(err) => err.user_message(),
            Self::Http { message, .. } => message.as_ref(),
        }
    }

    pub fn debug_details(&self) -> Option<&str> {
        match self {
            Self::Core(err) => err.debug_details(),
            Self::Pipeline(err) => err.debug_details(),
            Self::Http { .. } => None,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Core(err) => err.status_code(),
            Self::Pipeline(err) => err.status_code(),
            Self::Http { status, .. } => *status,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(err) => err.code(),
            Self::Pipeline(err) => err.code(),
            Self::Http { status, .. } => match *status {
                StatusCode::BAD_REQUEST => "bad_request",
                StatusCode::UNAUTHORIZED => "unauthorized",
                StatusCode::FORBIDDEN => "forbidden",
                StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
                StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media_type",
                StatusCode::BAD_GATEWAY => "bad_gateway",
                StatusCode::GATEWAY_TIMEOUT => "gateway_timeout",
                StatusCode::INTERNAL_SERVER_ERROR => "internal",
                _ if status.is_client_error() => "client_error",
                _ if status.is_server_error() => "server_error",
                _ => "http_error",
            },
        }
    }
}

pub type ErrorRenderer = dyn Fn(ApigateFrameworkError) -> Response + Send + Sync + 'static;

pub fn default_error_renderer(error: ApigateFrameworkError) -> Response {
    (
        error.status_code(),
        [("content-type", "text/plain; charset=utf-8")],
        error.user_message().to_owned(),
    )
        .into_response()
}
