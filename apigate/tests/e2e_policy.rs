mod support;

use axum::Router;
use axum::body::Body;
use http::{Method, Request, StatusCode};

#[apigate::service(name = "policy", prefix = "/policy")]
mod policy_routes {
    #[apigate::get("/item")]
    async fn item() {}
}

async fn upstream(label: &'static str) -> support::Upstream {
    support::spawn_upstream(Router::new().fallback(move || async move { label })).await
}

#[tokio::test]
async fn default_round_robin_balances_between_backends() {
    let a = upstream("a").await;
    let b = upstream("b").await;
    let router = apigate::App::builder()
        .mount_service(policy_routes::routes(), [a.url(), b.url()])
        .build()
        .unwrap()
        .into_router();

    let first = support::send(router.clone(), Method::GET, "/policy/item", "").await;
    let second = support::send(router, Method::GET, "/policy/item", "").await;

    let (first_status, _, first_body) = support::response_text(first).await;
    let (second_status, _, second_body) = support::response_text(second).await;

    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(second_status, StatusCode::OK);
    assert_ne!(first_body, second_body);
}

#[tokio::test]
async fn header_sticky_policy_keeps_same_affinity_on_same_backend() {
    let a = upstream("a").await;
    let b = upstream("b").await;
    let router = apigate::App::builder()
        .default_policy(apigate::Policy::header_sticky("x-user"))
        .mount_service(policy_routes::routes(), [a.url(), b.url()])
        .build()
        .unwrap()
        .into_router();

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/policy/item")
            .header("x-user", "user-1")
            .body(Body::empty())
            .unwrap()
    };

    let first = support::send_request(router.clone(), request()).await;
    let second = support::send_request(router, request()).await;

    let (_, _, first_body) = support::response_text(first).await;
    let (_, _, second_body) = support::response_text(second).await;

    assert_eq!(first_body, second_body);
}
