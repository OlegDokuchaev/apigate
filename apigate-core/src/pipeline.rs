use std::future::Future;
use std::pin::Pin;

use axum::body::{Body, Bytes, to_bytes};
use http::Extensions;

use crate::PartsCtx;
use crate::error::{ApigateError, ApigatePipelineError};

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
    raw_body: Option<Bytes>,
    body_limit: usize,
}

impl<'a> RequestScope<'a> {
    pub(crate) fn new(shared: &'a Extensions, body: Body, body_limit: usize) -> Self {
        Self {
            shared,
            local: Extensions::new(),
            body: Some(body),
            raw_body: None,
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

    /// Reads the request body into memory (up to `body_limit`), caches it as the
    /// raw body, and returns the bytes.
    pub async fn read_body_bytes(&mut self) -> Result<Bytes, ApigateError> {
        let body = self
            .body
            .take()
            .ok_or_else(|| ApigateError::from(ApigatePipelineError::RequestBodyAlreadyConsumed))?;
        let bytes = to_bytes(body, self.body_limit).await.map_err(|err| {
            ApigateError::from(ApigatePipelineError::RequestBodyTooLarge(err.to_string()))
        })?;
        self.raw_body = Some(bytes.clone());
        Ok(bytes)
    }

    /// Returns the raw request body bytes cached.
    pub fn raw_body(&self) -> Option<&[u8]> {
        self.raw_body.as_deref()
    }

    /// Returns an owned [`RawBody`] handle.
    #[doc(hidden)]
    pub fn raw_body_cloned(&self) -> Option<RawBody> {
        self.raw_body.clone().map(RawBody)
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

// ---------------------------------------------------------------------------
// RawBody
// ---------------------------------------------------------------------------

/// Raw request body bytes.
///
/// Declare it by value in a map signature to access the exact request bytes before typed parsing.
#[derive(Clone, Debug)]
pub struct RawBody(Bytes);

impl RawBody {
    /// Returns the raw body bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns the number of bytes in the body.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the body is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::ops::Deref for RawBody {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for RawBody {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<RawBody> for Body {
    fn from(raw: RawBody) -> Self {
        Body::from(raw.0)
    }
}

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

    #[tokio::test]
    async fn request_scope_read_body_bytes_caches_raw_body() {
        let shared = Extensions::new();
        let mut scope = RequestScope::new(&shared, Body::from("raw-bytes"), 1024);

        assert!(scope.raw_body().is_none());
        assert!(scope.raw_body_cloned().is_none());

        let bytes = scope.read_body_bytes().await.expect("body read");
        assert_eq!(&bytes[..], b"raw-bytes");

        assert_eq!(scope.raw_body(), Some(&b"raw-bytes"[..]));
        let cloned = scope.raw_body_cloned().expect("raw body cached");
        assert_eq!(cloned.as_bytes(), b"raw-bytes");
        assert_eq!(&*cloned, b"raw-bytes");
        assert_eq!(cloned.len(), 9);
        assert!(!cloned.is_empty());

        // The body has been consumed.
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
