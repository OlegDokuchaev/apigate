mod support;

use axum::Router;
use axum::response::{IntoResponse, Response};
use http::{Method, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ErrorBody {
    code: String,
    status: u16,
    message: String,
}

fn json_error_renderer(error: apigate::ApigateFrameworkError) -> Response {
    let body = ErrorBody {
        code: error.code().to_owned(),
        status: error.status_code().as_u16(),
        message: error.user_message().to_owned(),
    };

    (error.status_code(), axum::Json(body)).into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Input {
    value: String,
}

#[apigate::hook]
async fn deny_framework() -> apigate::HookResult {
    Err(apigate::ApigateError::unauthorized("missing token"))
}

#[apigate::hook]
async fn deny_custom_json() -> apigate::HookResult {
    Err(apigate::ApigateError::json(
        StatusCode::FORBIDDEN,
        ErrorBody {
            code: "custom_forbidden".to_owned(),
            status: 403,
            message: "custom denial".to_owned(),
        },
    ))
}

#[apigate::service(name = "errors", prefix = "/errors")]
mod errors {
    use super::*;

    #[apigate::get("/framework", before = [deny_framework])]
    async fn framework() {}

    #[apigate::get("/custom", before = [deny_custom_json])]
    async fn custom() {}

    #[apigate::post("/json", json = Input)]
    async fn json() {}
}

async fn app(base_url: String) -> Router {
    apigate::App::builder()
        .mount_service(errors::routes(), [base_url])
        .error_renderer(json_error_renderer)
        .build()
        .unwrap()
        .into_router()
}

#[tokio::test]
async fn framework_errors_use_configured_renderer() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let router = app(upstream.url()).await;

    let response = support::send(router, Method::GET, "/errors/framework", "").await;
    let (status, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body.code, "unauthorized");
    assert_eq!(body.status, 401);
    assert_eq!(body.message, "missing token");
}

#[tokio::test]
async fn custom_json_errors_bypass_configured_renderer() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let router = app(upstream.url()).await;

    let response = support::send(router, Method::GET, "/errors/custom", "").await;
    let (status, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body.code, "custom_forbidden");
    assert_eq!(body.status, 403);
    assert_eq!(body.message, "custom denial");
}

#[tokio::test]
async fn pipeline_errors_use_configured_renderer() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let router = app(upstream.url()).await;

    let response = support::send(router, Method::POST, "/errors/json", "not json").await;
    let (status, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "invalid_json_body");
    assert_eq!(body.message, "invalid json body");
}
