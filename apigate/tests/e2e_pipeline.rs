mod support;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::response::IntoResponse;
use http::{Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
struct EchoBody {
    uri: String,
    content_type: Option<String>,
    x_hook: Option<String>,
    body: String,
}

async fn echo(req: Request<Body>) -> impl IntoResponse {
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();

    axum::Json(EchoBody {
        uri: parts.uri.to_string(),
        content_type: parts
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        x_hook: parts
            .headers
            .get("x-hook")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    })
}

#[derive(Clone)]
struct AppState {
    source: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct SalePath {
    id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
struct LookupInput {
    q: String,
}

#[derive(Debug, Serialize)]
struct LookupService {
    query: String,
    source: &'static str,
}

#[derive(Debug, Deserialize)]
struct BuyInput {
    public_id: String,
}

#[derive(Debug, Serialize)]
struct BuyService {
    internal_id: String,
}

#[apigate::hook]
async fn inject_header(ctx: &mut apigate::PartsCtx<'_>, state: &AppState) -> apigate::HookResult {
    ctx.set_header("x-hook", state.source)?;
    Ok(())
}

#[apigate::hook]
async fn remap_lookup_query(
    input: &LookupInput,
    path: &SalePath,
    state: &AppState,
    ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::HookResult {
    ctx.set_query(&LookupService {
        query: format!("{}:{}", path.id, input.q.trim()),
        source: state.source,
    })?;
    Ok(())
}

#[apigate::map]
async fn remap_buy(input: BuyInput) -> apigate::MapResult<BuyService> {
    Ok(BuyService {
        internal_id: format!("svc-{}", input.public_id),
    })
}

// validate-only map: returns `()`, so the original request body must be
// forwarded byte-for-byte. It still parses the input (rejecting bad bodies)
// and mutates headers.
#[apigate::map]
async fn validate_buy(input: BuyInput, ctx: &mut apigate::PartsCtx<'_>) -> apigate::MapResult<()> {
    if input.public_id.is_empty() {
        return Err(apigate::ApigateError::bad_request("empty public_id"));
    }
    ctx.set_header("x-validated", "yes")?;
    Ok(())
}

// Schema-less validate-only map: inspects the raw bytes and returns `()`, so the
// original body is forwarded unchanged (no re-serialization).
#[apigate::map]
async fn inspect_raw(raw: apigate::RawBody) -> apigate::MapResult<()> {
    if raw.is_empty() {
        return Err(apigate::ApigateError::bad_request("empty body"));
    }
    Ok(())
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::get("/{id}/lookup", path = SalePath, query = LookupInput, before = [inject_header, remap_lookup_query])]
    async fn lookup() {}

    #[apigate::post("/buy", json = BuyInput, map = remap_buy)]
    async fn buy() {}

    #[apigate::post("/check", json = BuyInput, map = validate_buy)]
    async fn check() {}

    #[apigate::post("/inspect", map = inspect_raw)]
    async fn inspect() {}
}

async fn app(base_url: String) -> Router {
    apigate::App::builder()
        .mount_service(sales::routes(), [base_url])
        .state(AppState { source: "gateway" })
        .build()
        .unwrap()
        .into_router()
}

#[tokio::test]
async fn hooks_path_validation_and_query_rewrite_run_before_proxying() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;
    let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();

    let response = support::send(
        router,
        Method::GET,
        &format!("/sales/{id}/lookup?q=%20hello%20"),
        Body::empty(),
    )
    .await;

    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::OK);

    let echo: EchoBody = serde_json::from_str(&body).unwrap();
    assert_eq!(echo.x_hook.as_deref(), Some("gateway"));
    assert!(echo.uri.starts_with(&format!("/{id}/lookup?")));

    let query = echo.uri.split_once('?').unwrap().1;
    let query: HashMap<String, String> = serde_urlencoded::from_str(query).unwrap();
    assert_eq!(query.get("query"), Some(&format!("{id}:hello")));
    assert_eq!(query.get("source"), Some(&"gateway".to_owned()));
}

#[tokio::test]
async fn json_map_rewrites_body_and_content_type() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    let response = support::send_request(
        router,
        Request::builder()
            .method(Method::POST)
            .uri("/sales/buy")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"public_id":"1"}"#))
            .unwrap(),
    )
    .await;

    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::OK);

    let echo: EchoBody = serde_json::from_str(&body).unwrap();
    assert_eq!(echo.uri, "/buy");
    assert_eq!(echo.content_type.as_deref(), Some("application/json"));
    assert_eq!(echo.body, r#"{"internal_id":"svc-1"}"#);
}

#[tokio::test]
async fn validate_only_map_keeps_original_body() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    // Body has extra whitespace and field order a re-serialization would change;
    // a validate-only map must forward these exact bytes.
    let original = r#"{ "public_id" : "42" }"#;
    let response = support::send_request(
        router,
        Request::builder()
            .method(Method::POST)
            .uri("/sales/check")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(original))
            .unwrap(),
    )
    .await;

    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::OK);

    let echo: EchoBody = serde_json::from_str(&body).unwrap();
    assert_eq!(echo.uri, "/check");
    // Body forwarded byte-for-byte (NOT re-serialized: whitespace/order preserved).
    assert_eq!(echo.body, original);
}

#[tokio::test]
async fn validate_only_map_still_rejects_invalid_body() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    // Missing required field -> input parse fails before the map runs.
    let response = support::send_request(
        router,
        Request::builder()
            .method(Method::POST)
            .uri("/sales/check")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"wrong":"field"}"#))
            .unwrap(),
    )
    .await;

    let (status, _, _) = support::response_text(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn raw_validate_only_map_keeps_original_body() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    // Arbitrary (non-JSON) bytes: a schema-less validate-only map forwards them as-is.
    let original = "any-raw-bytes-\x00\x01-here";
    let response = support::send_request(
        router,
        Request::builder()
            .method(Method::POST)
            .uri("/sales/inspect")
            .body(Body::from(original))
            .unwrap(),
    )
    .await;

    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::OK);

    let echo: EchoBody = serde_json::from_str(&body).unwrap();
    assert_eq!(echo.uri, "/inspect");
    assert_eq!(echo.body, original);
}

#[tokio::test]
async fn raw_validate_only_map_rejects_empty_body() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    let response = support::send_request(
        router,
        Request::builder()
            .method(Method::POST)
            .uri("/sales/inspect")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    let (status, _, _) = support::response_text(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_path_parameters_return_framework_error() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    let response = support::send(
        router,
        Method::GET,
        "/sales/not-a-uuid/lookup?q=hello",
        Body::empty(),
    )
    .await;

    let (status, _, body) = support::response_text(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, "invalid path parameters");
}
