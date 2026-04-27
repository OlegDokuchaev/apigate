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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendPool, BaseUri};

    fn pool() -> BackendPool {
        BackendPool::new(vec![
            BaseUri::parse("http://127.0.0.1:8081").unwrap(),
            BaseUri::parse("http://127.0.0.1:8082").unwrap(),
        ])
    }

    fn ctx<'a>(
        route_path: &'a str,
        prefix: &'a str,
        uri: &'a http::Uri,
        headers: &'a http::HeaderMap,
    ) -> RouteCtx<'a> {
        RouteCtx {
            service: "sales",
            prefix,
            route_path,
            method: &http::Method::GET,
            uri,
            headers,
        }
    }

    #[test]
    fn no_route_key_leaves_all_backends_eligible() {
        let uri = "/sales/123".parse().unwrap();
        let headers = http::HeaderMap::new();
        let pool = pool();
        let decision = NoRouteKey.route(&ctx("/{id}", "/sales", &uri, &headers), &pool);

        assert!(decision.affinity.is_none());
        assert!(matches!(decision.candidates, CandidateSet::All));
    }

    #[test]
    fn header_sticky_uses_utf8_header_as_affinity() {
        let uri = "/sales".parse().unwrap();
        let mut headers = http::HeaderMap::new();
        headers.insert("x-user-id", "user-1".parse().unwrap());
        let pool = pool();

        let decision =
            HeaderSticky::new("x-user-id").route(&ctx("/", "/sales", &uri, &headers), &pool);

        assert_eq!(
            decision.affinity.as_ref().map(AffinityKey::as_str),
            Some("user-1")
        );
        assert!(matches!(decision.candidates, CandidateSet::All));
    }

    #[test]
    fn header_sticky_ignores_missing_or_non_utf8_header() {
        let uri = "/sales".parse().unwrap();
        let mut headers = http::HeaderMap::new();
        headers.insert("x-user-id", http::HeaderValue::from_bytes(b"\xff").unwrap());
        let pool = pool();

        let decision =
            HeaderSticky::new("x-missing").route(&ctx("/", "/sales", &uri, &headers), &pool);
        assert!(decision.affinity.is_none());

        let decision =
            HeaderSticky::new("x-user-id").route(&ctx("/", "/sales", &uri, &headers), &pool);
        assert!(decision.affinity.is_none());
    }

    #[test]
    fn path_sticky_extracts_named_parameter_after_prefix() {
        let uri = "/api/sales/111/items/222".parse().unwrap();
        let headers = http::HeaderMap::new();
        let pool = pool();

        let decision = PathSticky::new("item_id").route(
            &ctx("/{sale_id}/items/{item_id}", "/api/sales", &uri, &headers),
            &pool,
        );

        assert_eq!(
            decision.affinity.as_ref().map(AffinityKey::as_str),
            Some("222")
        );
        assert!(matches!(decision.candidates, CandidateSet::All));
    }

    #[test]
    fn path_sticky_returns_none_when_parameter_is_absent() {
        let uri = "/api/sales/111".parse().unwrap();
        let headers = http::HeaderMap::new();
        let pool = pool();

        let decision = PathSticky::new("item_id")
            .route(&ctx("/{sale_id}", "/api/sales", &uri, &headers), &pool);

        assert!(decision.affinity.is_none());
    }

    #[test]
    fn affinity_key_can_be_borrowed_or_owned() {
        let borrowed = AffinityKey::borrowed("abc");
        let owned = AffinityKey::owned("abc");

        assert_eq!(borrowed, owned);
        assert_eq!(borrowed.as_str(), "abc");
        assert_eq!(owned.into_owned(), "abc");
    }
}
