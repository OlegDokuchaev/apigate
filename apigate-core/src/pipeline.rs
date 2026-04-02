use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::body::Body;
use http::Extensions;

use crate::PartsCtx;
use crate::error::ApigateError;

// ---------------------------------------------------------------------------
// RequestScope
// ---------------------------------------------------------------------------

/// Owns the request body and extracted data for a single pipeline invocation.
///
/// Two-layer extensions:
/// - `shared` — app-level state from `AppBuilder::state()`, shared via `Arc` (zero-copy per request)
/// - `extensions` — per-request data (path params, parsed input, etc.)
pub struct RequestScope {
    shared: Arc<Extensions>,
    extensions: Extensions,
    body: Option<Body>,
    body_limit: usize,
}

impl RequestScope {
    pub fn new(body: Body, body_limit: usize) -> Self {
        Self {
            shared: Arc::new(Extensions::new()),
            extensions: Extensions::new(),
            body: Some(body),
            body_limit,
        }
    }

    pub fn with_state(shared: Arc<Extensions>, body: Body, body_limit: usize) -> Self {
        Self {
            shared,
            extensions: Extensions::new(),
            body: Some(body),
            body_limit,
        }
    }

    pub fn take_body(&mut self) -> Option<Body> {
        self.body.take()
    }

    pub fn body_limit(&self) -> usize {
        self.body_limit
    }

    pub fn get<T: Clone + Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions
            .get::<T>()
            .or_else(|| self.shared.get::<T>())
    }

    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) {
        self.extensions.insert(val);
    }
}

// ---------------------------------------------------------------------------
// Pipeline types
// ---------------------------------------------------------------------------

/// Single function that orchestrates all request processing:
/// parse path params → before hooks → validate/parse body → map → return body.
pub type PipelineFn = for<'a> fn(PartsCtx<'a>, RequestScope) -> PipelineFuture<'a>;
pub type PipelineFuture<'a> = Pin<Box<dyn Future<Output = PipelineResult> + Send + 'a>>;
pub type PipelineResult = Result<Body, ApigateError>;

// User-facing return types for hook/map functions
pub type HookResult = Result<(), ApigateError>;
pub type MapResult<T> = Result<T, ApigateError>;
