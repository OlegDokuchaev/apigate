mod support;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::response::IntoResponse;
use http::header::{CONNECTION, HOST};
use http::{Method, Request, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct EchoBody {
    method: String,
    uri: String,
    host: Option<String>,
    x_remove: Option<String>,
    body: String,
}

async fn echo(req: Request<Body>) -> impl IntoResponse {
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    let echo = EchoBody {
        method: parts.method.to_string(),
        uri: parts.uri.to_string(),
        host: parts
            .headers
            .get(HOST)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        x_remove: parts
            .headers
            .get("x-remove")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        body,
    };

    let mut response = axum::Json(echo).into_response();
    response
        .headers_mut()
        .insert(CONNECTION, "x-response-hop".parse().unwrap());
    response
        .headers_mut()
        .insert("x-response-hop", "remove-me".parse().unwrap());
    response
        .headers_mut()
        .insert("x-upstream", "ok".parse().unwrap());
    response
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    #[apigate::get("/plain")]
    async fn plain() {}

    #[apigate::post("/static", to = "/internal/static")]
    async fn static_rewrite() {}

    #[apigate::get("/items/{id}", to = "/internal/items/{id}")]
    async fn template_rewrite() {}
}

async fn app(base_url: String) -> Router {
    apigate::App::builder()
        .mount_service(sales::routes(), [base_url])
        .build()
        .unwrap()
        .into_router()
}

#[tokio::test]
async fn proxy_strips_prefix_and_hop_headers() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    let response = support::send_request(
        router,
        Request::builder()
            .method(Method::GET)
            .uri("/sales/plain?q=abc")
            .header(CONNECTION, "x-remove")
            .header("x-remove", "remove-me")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(CONNECTION).is_none());
    assert!(response.headers().get("x-response-hop").is_none());
    assert_eq!(response.headers().get("x-upstream").unwrap(), "ok");

    let (_, _, body) = support::response_text(response).await;
    let echo: EchoBody = serde_json::from_str(&body).unwrap();

    assert_eq!(echo.method, "GET");
    assert_eq!(echo.uri, "/plain?q=abc");
    assert_eq!(echo.x_remove, None);
    assert!(echo.host.unwrap().starts_with("127.0.0.1:"));
}

#[tokio::test]
async fn proxy_applies_static_rewrite_and_preserves_body() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    let response = support::send(router, Method::POST, "/sales/static?q=abc", "payload").await;
    let (_, _, body) = support::response_text(response).await;
    let echo: EchoBody = serde_json::from_str(&body).unwrap();

    assert_eq!(echo.method, "POST");
    assert_eq!(echo.uri, "/internal/static?q=abc");
    assert_eq!(echo.body, "payload");
}

#[tokio::test]
async fn proxy_applies_template_rewrite() {
    let upstream = support::spawn_upstream(Router::new().fallback(echo)).await;
    let router = app(upstream.url()).await;

    let response = support::send(router, Method::GET, "/sales/items/42?q=abc", Body::empty()).await;
    let (_, _, body) = support::response_text(response).await;
    let echo: EchoBody = serde_json::from_str(&body).unwrap();

    assert_eq!(echo.uri, "/internal/items/42?q=abc");
}
