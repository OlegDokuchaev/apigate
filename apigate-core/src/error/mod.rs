mod apigate_error;
mod build;
mod core_runtime;
mod framework;
mod pipeline_runtime;

pub use apigate_error::ApigateError;
pub use build::{ApigateBuildError, BaseUriParseError};
pub use core_runtime::ApigateCoreError;
pub use framework::{ApigateFrameworkError, ErrorRenderer, default_error_renderer};
pub use pipeline_runtime::ApigatePipelineError;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    use http::StatusCode;
    use serde::Serialize;

    async fn response_text(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn core_errors_expose_status_code_message_and_code() {
        let err = ApigateCoreError::InvalidHeaderName;
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.user_message(), "invalid header name");
        assert_eq!(err.code(), "invalid_header_name");
        assert_eq!(err.debug_details(), None);

        let err = ApigateCoreError::UpstreamRequestTimedOut;
        assert_eq!(err.status_code(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.user_message(), "upstream request timed out");
        assert_eq!(err.code(), "upstream_timeout");
    }

    #[test]
    fn pipeline_errors_separate_user_message_from_debug_details() {
        let err = ApigatePipelineError::InvalidJsonBody("expected value at line 1".to_owned());

        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.user_message(), "invalid json body");
        assert_eq!(err.debug_details(), Some("expected value at line 1"));
        assert_eq!(err.code(), "invalid_json_body");

        let err = ApigatePipelineError::MissingFromScope("RequestMeta");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.user_message(), "missing value in request scope");
        assert_eq!(err.debug_details(), Some("RequestMeta"));
    }

    #[test]
    fn framework_error_delegates_to_inner_error_or_http_status() {
        let core = ApigateFrameworkError::from(ApigateCoreError::NoBackends);
        assert_eq!(core.status_code(), StatusCode::BAD_GATEWAY);
        assert_eq!(core.user_message(), "no backends");
        assert_eq!(core.code(), "no_backends");

        let http = ApigateFrameworkError::Http {
            status: StatusCode::UNAUTHORIZED,
            message: "missing token".into(),
        };
        assert_eq!(http.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(http.user_message(), "missing token");
        assert_eq!(http.code(), "unauthorized");
        assert_eq!(http.debug_details(), None);
    }

    #[tokio::test]
    async fn default_error_renderer_returns_plain_text_user_message() {
        let response = default_error_renderer(ApigateFrameworkError::from(
            ApigatePipelineError::InvalidQuery("bad query details".to_owned()),
        ));

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response_text(response).await, "invalid query");
    }

    #[tokio::test]
    async fn apigate_error_framework_constructors_use_default_renderer() {
        let response = ApigateError::unauthorized("missing token").into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response_text(response).await, "missing token");
    }

    #[tokio::test]
    async fn apigate_error_json_returns_custom_json_response() {
        #[derive(Serialize)]
        struct Body {
            code: &'static str,
        }

        let response =
            ApigateError::json(StatusCode::FORBIDDEN, Body { code: "forbidden" }).into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(response_text(response).await, r#"{"code":"forbidden"}"#);
    }
}
