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
    details: Option<String>,
}

fn json_error_renderer(error: apigate::ApigateFrameworkError) -> Response {
    let body = ErrorBody {
        code: error.code().to_owned(),
        status: error.status_code().as_u16(),
        message: error.user_message().to_owned(),
        details: error.debug_details().map(str::to_owned),
    };

    (error.status_code(), axum::Json(body)).into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Input {
    value: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Order {
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Item {
    count: u32,
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
            details: None,
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

    #[apigate::post("/order", json = Order)]
    async fn order() {}

    #[apigate::post("/form", form = Item)]
    async fn form() {}

    #[apigate::get("/form", form = Item)]
    async fn form_query() {}
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

#[tokio::test]
async fn body_parse_errors_name_the_offending_field() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let router = app(upstream.url()).await;

    // nested JSON field: the path is prepended to serde's message
    let response = support::send_request(
        router.clone(),
        http::Request::post("/errors/order")
            .header("content-type", "application/json")
            .body(r#"{"items":[{"count":1},{"count":"two"}]}"#.into())
            .unwrap(),
    )
    .await;
    let (status, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "invalid_json_body");
    let details = body.details.unwrap();
    assert!(details.starts_with("items[1].count: "), "{details}");

    // a failure outside the value keeps serde's own message
    let response = support::send_request(
        router.clone(),
        http::Request::post("/errors/json")
            .header("content-type", "application/json")
            .body(r#"{"value":"ok"} trailing"#.into())
            .unwrap(),
    )
    .await;
    let (_, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();
    assert!(
        body.details
            .as_deref()
            .unwrap()
            .starts_with("trailing characters"),
        "{body:?}"
    );

    // form body and form query use the same rule
    let response = support::send_request(
        router.clone(),
        http::Request::post("/errors/form")
            .header("content-type", "application/x-www-form-urlencoded")
            .body("count=two".into())
            .unwrap(),
    )
    .await;
    let (_, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();
    assert_eq!(body.code, "invalid_form_body");
    assert!(
        body.details.as_deref().unwrap().starts_with("count: "),
        "{body:?}"
    );

    let response = support::send(router, Method::GET, "/errors/form?count=two", "").await;
    let (_, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();
    assert_eq!(body.code, "invalid_form_query");
    assert!(
        body.details.as_deref().unwrap().starts_with("count: "),
        "{body:?}"
    );
}

#[tokio::test]
async fn unrouted_requests_use_configured_renderer() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let router = app(upstream.url()).await;

    for uri in ["/errors/nope", "/nope"] {
        let response = support::send(router.clone(), Method::GET, uri, "").await;
        let (status, _, body) = support::response_text(response).await;
        let body: ErrorBody = serde_json::from_str(&body).unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body.code, "route_not_found");
        assert_eq!(body.message, "route not found");
    }

    let response = support::send(router, Method::DELETE, "/errors/json", "").await;
    let (status, _, body) = support::response_text(response).await;
    let body: ErrorBody = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body.code, "method_not_allowed");
    assert_eq!(body.message, "method not allowed");
}

#[tokio::test]
async fn unrouted_requests_use_default_renderer_and_can_be_overridden() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;

    let router = apigate::App::builder()
        .mount_service(errors::routes(), [upstream.url()])
        .build()
        .unwrap()
        .into_router();
    let response = support::send(router.clone(), Method::GET, "/nope", "").await;
    let (status, headers, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers["content-type"], "text/plain; charset=utf-8");
    assert_eq!(body, "route not found");

    let response = support::send(router, Method::DELETE, "/errors/json", "").await;
    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body, "method not allowed");

    // `with_router` still lets the application take over the fallback
    let router = apigate::App::builder()
        .mount_service(errors::routes(), [upstream.url()])
        .build()
        .unwrap()
        .with_router(|router| router.fallback(|| async { (StatusCode::GONE, "custom") }))
        .into_router();
    let response = support::send(router, Method::GET, "/nope", "").await;
    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(body, "custom");
}
