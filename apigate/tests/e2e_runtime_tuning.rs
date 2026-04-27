mod support;

use axum::Router;
use http::{Method, StatusCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[apigate::service(name = "runtime", prefix = "/runtime")]
mod runtime_routes {
    #[apigate::get("/ping")]
    async fn ping() {}
}

async fn app(base_url: String, upstream: apigate::UpstreamConfig) -> Router {
    apigate::App::builder()
        .mount_service(runtime_routes::routes(), [base_url])
        .request_timeout(Duration::from_secs(5))
        .upstream(upstream)
        .build()
        .unwrap()
        .into_router()
}

#[test]
fn serve_config_builder_exposes_socket_tuning() {
    let _config = apigate::ServeConfig::default()
        .backlog(128)
        .reuse_address(true)
        .recv_buffer_size(64 * 1024)
        .send_buffer_size(64 * 1024)
        .tcp_nodelay(true);
}

#[tokio::test]
async fn upstream_config_tuning_is_used_by_public_app_builder() {
    let upstream_server = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let client_configured = Arc::new(AtomicBool::new(false));
    let connector_configured = Arc::new(AtomicBool::new(false));

    let client_flag = Arc::clone(&client_configured);
    let connector_flag = Arc::clone(&connector_configured);

    let upstream = apigate::UpstreamConfig::default()
        .connect_timeout(Duration::from_secs(1))
        .pool_idle_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(4)
        .tcp_nodelay(true)
        .configure_client(move |client| {
            client.retry_canceled_requests(true);
            client_flag.store(true, Ordering::SeqCst);
        })
        .configure_connector(move |connector| {
            connector.set_keepalive(Some(Duration::from_secs(5)));
            connector_flag.store(true, Ordering::SeqCst);
        });

    let router = app(upstream_server.url(), upstream).await;

    assert!(client_configured.load(Ordering::SeqCst));
    assert!(connector_configured.load(Ordering::SeqCst));

    let response = support::send(router, Method::GET, "/runtime/ping", "").await;
    let (status, _, body) = support::response_text(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}
