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
/// App-level state lives in a shared `Arc<Extensions>` (zero-copy per request).
/// Per-request data (path params, hook insertions) goes into a local `Extensions`
/// that starts empty and allocates only on first `insert`.
pub struct RequestScope<'a> {
    shared: &'a Extensions,
    local: Extensions,
    body: Option<Body>,
    body_limit: usize,
}

impl<'a> RequestScope<'a> {
    pub(crate) fn new(shared: &'a Extensions, body: Body, body_limit: usize) -> Self {
        Self {
            shared,
            local: Extensions::new(),
            body: Some(body),
            body_limit,
        }
    }

    /// Takes ownership of the request body.
    ///
    /// Generated pipelines use this when validating or mapping request bodies.
    pub fn take_body(&mut self) -> Option<Body> {
        self.body.take()
    }

    /// Returns the maximum number of bytes generated pipelines may read.
    pub fn body_limit(&self) -> usize {
        self.body_limit
    }

    /// Returns a shared reference to `T`.
    /// Checks local (per-request) extensions first, then shared (app) state.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.local.get::<T>().or_else(|| self.shared.get::<T>())
    }

    /// Returns a mutable reference to `T` from local (per-request) extensions only.
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.local.get_mut::<T>()
    }

    /// Inserts a value into per-request (local) extensions.
    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) {
        self.local.insert(val);
    }

    /// Takes a value from local extensions first; if absent, clones from shared state.
    pub fn take<T: Clone + Send + Sync + 'static>(&mut self) -> Option<T> {
        self.local
            .remove::<T>()
            .or_else(|| self.shared.get::<T>().cloned())
    }
}

// ---------------------------------------------------------------------------
// Pipeline types
// ---------------------------------------------------------------------------

/// Single function that orchestrates all request processing:
/// parse path params, run before hooks, validate/parse body, map, and return body.
pub type PipelineFn = for<'a> fn(PartsCtx<'a>, RequestScope<'a>) -> PipelineFuture<'a>;
/// Boxed future returned by a generated pipeline.
pub type PipelineFuture<'a> = Pin<Box<dyn Future<Output = PipelineResult> + Send + 'a>>;
/// Result returned by a generated pipeline.
pub type PipelineResult = Result<Body, ApigateError>;

/// Result type returned by `#[apigate::hook]` functions.
pub type HookResult = Result<(), ApigateError>;
/// Result type returned by `#[apigate::map]` functions.
pub type MapResult<T> = Result<T, ApigateError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct SharedState(&'static str);

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct LocalState(&'static str);

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Overridden(&'static str);

    #[tokio::test]
    async fn request_scope_body_can_be_taken_once() {
        let shared = Extensions::new();
        let mut scope = RequestScope::new(&shared, Body::from("hello"), 1024);

        assert_eq!(scope.body_limit(), 1024);

        let body = scope.take_body().expect("body is present");
        let bytes = to_bytes(body, 1024).await.unwrap();
        assert_eq!(&bytes[..], b"hello");

        assert!(scope.take_body().is_none());
    }

    #[test]
    fn request_scope_get_reads_local_before_shared() {
        let mut shared = Extensions::new();
        shared.insert(SharedState("shared"));
        shared.insert(Overridden("shared"));

        let mut scope = RequestScope::new(&shared, Body::empty(), 1024);
        scope.insert(LocalState("local"));
        scope.insert(Overridden("local"));

        assert_eq!(scope.get::<SharedState>(), Some(&SharedState("shared")));
        assert_eq!(scope.get::<LocalState>(), Some(&LocalState("local")));
        assert_eq!(scope.get::<Overridden>(), Some(&Overridden("local")));
    }

    #[test]
    fn request_scope_get_mut_only_targets_local_values() {
        let mut shared = Extensions::new();
        shared.insert(SharedState("shared"));

        let mut scope = RequestScope::new(&shared, Body::empty(), 1024);
        scope.insert(LocalState("local"));

        scope.get_mut::<LocalState>().unwrap().0 = "changed";

        assert_eq!(scope.get::<LocalState>(), Some(&LocalState("changed")));
        assert!(scope.get_mut::<SharedState>().is_none());
    }

    #[test]
    fn request_scope_take_removes_local_or_clones_shared() {
        let mut shared = Extensions::new();
        shared.insert(SharedState("shared"));

        let mut scope = RequestScope::new(&shared, Body::empty(), 1024);
        scope.insert(LocalState("local"));

        assert_eq!(scope.take::<LocalState>(), Some(LocalState("local")));
        assert_eq!(scope.take::<LocalState>(), None);

        assert_eq!(scope.take::<SharedState>(), Some(SharedState("shared")));
        assert_eq!(scope.take::<SharedState>(), Some(SharedState("shared")));
    }
}
