use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;

use crate::Routes;
use crate::error::ErrorRenderer;
use crate::observability::RuntimeObserver;
use crate::policy::Policy;
use crate::route::RouteMeta;

mod build;
mod dispatch;

pub struct App {
    router: Router,
}

pub struct AppBuilder {
    backends: HashMap<String, Vec<String>>,
    mounted: Vec<Routes>,
    policies: HashMap<String, Arc<Policy>>,
    default_policy: Arc<Policy>,
    request_timeout: Duration,
    connect_timeout: Duration,
    pool_idle_timeout: Duration,
    map_body_limit: usize,
    state: http::Extensions,
    error_renderer: Arc<ErrorRenderer>,
    runtime_observer: Option<Arc<RuntimeObserver>>,
}

pub(super) struct Inner {
    client: Client<HttpConnector, Body>,
    state: http::Extensions,
    map_body_limit: usize,
    request_timeout: Duration,
    route_metas: Box<[RouteMeta]>,
    error_renderer: Arc<ErrorRenderer>,
    runtime_observer: Option<Arc<RuntimeObserver>>,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    /// Applies a transformation to the internal axum router.
    ///
    /// Useful for adding external tower/axum layers, for example:
    /// `.with_router(|r| r.layer(tower_http::trace::TraceLayer::new_for_http()))`.
    pub fn with_router<F>(mut self, transform: F) -> Self
    where
        F: FnOnce(Router) -> Router,
    {
        self.router = transform(self.router);
        self
    }

    /// Consumes app and returns the underlying axum router.
    ///
    /// This allows full manual composition with tower layers and custom serving.
    pub fn into_router(self) -> Router {
        self.router
    }
}

pub async fn run(addr: SocketAddr, app: App) -> std::io::Result<()> {
    run_router(addr, app.router).await
}

/// Runs a pre-built axum router.
///
/// Useful when you need full control over outer tower/axum middleware stack.
pub async fn run_router(addr: SocketAddr, router: Router) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // axum::serve intentionally simple (и это нам подходит как внутренняя обертка)
    axum::serve(listener, router).await
}
