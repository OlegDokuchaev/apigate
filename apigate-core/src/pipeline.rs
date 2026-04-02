use std::future::Future;
use std::pin::Pin;

use axum::body::Body;
use http::Extensions;

use crate::PartsCtx;
use crate::error::ApigateError;

// ---------------------------------------------------------------------------
// RequestScope
// ---------------------------------------------------------------------------

/// Owns the request body and extracted data for a single pipeline invocation.
/// Consumed by the pipeline — body is read/transformed inside, result returned.
pub struct RequestScope {
    extensions: Extensions,
    body: Option<Body>,
    body_limit: usize,
}

impl RequestScope {
    pub fn new(body: Body, body_limit: usize) -> Self {
        Self {
            extensions: Extensions::new(),
            body: Some(body),
            body_limit,
        }
    }

    pub fn take_body(&mut self) -> Body {
        self.body.take().expect("request body already consumed")
    }

    pub fn body_limit(&self) -> usize {
        self.body_limit
    }

    pub fn get<T: Clone + Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
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
