mod header_sticky;
mod no_route_key;
mod path_sticky;

use std::borrow::Cow;

use crate::backend::BackendPool;

pub use header_sticky::HeaderSticky;
pub use no_route_key::NoRouteKey;
pub use path_sticky::PathSticky;

/// Request context passed to route-key strategies.
pub struct RouteCtx<'a> {
    /// Logical service name.
    pub service: &'a str,
    /// Mounted service prefix.
    pub prefix: &'a str,
    /// Route path relative to the service prefix.
    pub route_path: &'a str,
    /// Incoming HTTP method.
    pub method: &'a http::Method,
    /// Incoming request URI.
    pub uri: &'a http::Uri,
    /// Incoming request headers.
    pub headers: &'a http::HeaderMap,
}

/// Affinity key used by balancers such as consistent hashing.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AffinityKey<'a>(Cow<'a, str>);

impl<'a> AffinityKey<'a> {
    /// Creates an affinity key borrowed from request data.
    pub fn borrowed(value: &'a str) -> Self {
        Self(Cow::Borrowed(value))
    }

    /// Creates an owned affinity key.
    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    /// Returns the key as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Converts the key into an owned string.
    pub fn into_owned(self) -> String {
        self.0.into_owned()
    }
}

/// Backend candidate set selected by a route strategy.
#[derive(Clone, Debug)]
pub enum CandidateSet<'a> {
    /// All backends in the service pool are candidates.
    All,
    /// Only the listed backend indices are candidates.
    Indices(&'a [usize]),
}

/// Result of route-key extraction and candidate filtering.
#[derive(Clone, Debug)]
pub struct RoutingDecision<'a> {
    /// Optional key used by affinity-aware balancers.
    pub affinity: Option<AffinityKey<'a>>,
    /// Candidate backend set for the balancer.
    pub candidates: CandidateSet<'a>,
}

/// Strategy that extracts affinity and candidate information from a request.
pub trait RouteStrategy: Send + Sync + 'static {
    /// Produces a routing decision for the request.
    fn route<'a>(&self, ctx: &RouteCtx<'a>, pool: &'a BackendPool) -> RoutingDecision<'a>;
}
