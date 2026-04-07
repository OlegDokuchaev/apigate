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
///
/// Pre-populated with cloned app state, then enriched with per-request data
/// (parsed input, path params, etc.) during the pipeline.
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

    pub(crate) fn with_state(state: Extensions, body: Body, body_limit: usize) -> Self {
        Self {
            extensions: state,
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

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.extensions.get_mut::<T>()
    }

    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) {
        self.extensions.insert(val);
    }

    pub fn take<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.extensions.remove::<T>()
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

pub type HookResult = Result<(), ApigateError>;
pub type MapResult<T> = Result<T, ApigateError>;
