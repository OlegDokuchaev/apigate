mod support;

use axum::Router;
use http::{Method, StatusCode};
use std::sync::{Arc, Mutex};

#[apigate::hook]
async fn fail_hook() -> apigate::HookResult {
    Err(apigate::ApigateError::bad_request("blocked"))
}

#[apigate::service(name = "obs", prefix = "/obs")]
mod obs {
    use super::*;

    #[apigate::get("/plain")]
    async fn plain() {}

    #[apigate::get("/fail", before = [fail_hook])]
    async fn fail() {}
}

async fn app(base_url: String, events: Arc<Mutex<Vec<&'static str>>>) -> Router {
    apigate::App::builder()
        .mount_service(obs::routes(), [base_url])
        .runtime_observer(move |event| {
            let name = match event.kind {
                apigate::RuntimeEventKind::RequestStart { .. } => "request_start",
                apigate::RuntimeEventKind::PipelineFailedFramework { .. } => {
                    "pipeline_failed_framework"
                }
                apigate::RuntimeEventKind::PipelineFailedCustom { .. } => "pipeline_failed_custom",
                apigate::RuntimeEventKind::DispatchFailed { .. } => "dispatch_failed",
                apigate::RuntimeEventKind::BackendSelected { .. } => "backend_selected",
                apigate::RuntimeEventKind::UpstreamSucceeded { .. } => "upstream_succeeded",
                apigate::RuntimeEventKind::UpstreamFailed { .. } => "upstream_failed",
                _ => "unknown",
            };
            events.lock().unwrap().push(name);
        })
        .build()
        .unwrap()
        .into_router()
}

#[tokio::test]
async fn observer_receives_successful_proxy_events() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let router = app(upstream.url(), events.clone()).await;

    let response = support::send(router, Method::GET, "/obs/plain", "").await;
    assert_eq!(response.status(), StatusCode::OK);

    let events = events.lock().unwrap().clone();
    assert_eq!(events[0], "request_start");
    assert!(events.contains(&"backend_selected"));
    assert!(events.contains(&"upstream_succeeded"));
}

#[tokio::test]
async fn observer_receives_pipeline_failure_without_backend_selection() {
    let upstream = support::spawn_upstream(Router::new().fallback(|| async { "ok" })).await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let router = app(upstream.url(), events.clone()).await;

    let response = support::send(router, Method::GET, "/obs/fail", "").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let events = events.lock().unwrap().clone();
    assert_eq!(events[0], "request_start");
    assert!(events.contains(&"pipeline_failed_framework"));
    assert!(!events.contains(&"backend_selected"));
}
