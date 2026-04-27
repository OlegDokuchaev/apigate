#![allow(dead_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use http::{Method, Request, Response, StatusCode};
use tokio::task::JoinHandle;
use tower::ServiceExt;

pub struct Upstream {
    addr: std::net::SocketAddr,
    handle: JoinHandle<()>,
}

impl Upstream {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for Upstream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn spawn_upstream(router: Router) -> Upstream {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    Upstream { addr, handle }
}

pub async fn send(
    router: Router,
    method: Method,
    uri: &str,
    body: impl Into<Body>,
) -> Response<Body> {
    send_request(
        router,
        Request::builder()
            .method(method)
            .uri(uri)
            .body(body.into())
            .unwrap(),
    )
    .await
}

pub async fn send_request(router: Router, request: Request<Body>) -> Response<Body> {
    router.oneshot(request).await.unwrap()
}

pub async fn response_text(response: Response<Body>) -> (StatusCode, http::HeaderMap, String) {
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    (status, headers, body)
}
