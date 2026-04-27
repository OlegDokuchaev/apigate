use std::sync::Arc;

use crate::balancing::{Balancer, ConsistentHash, LeastRequest, LeastTime, RoundRobin};
use crate::routing::{HeaderSticky, NoRouteKey, PathSticky, RouteStrategy};

/// Routing and balancing policy applied to one or more routes.
pub struct Policy {
    pub(crate) router: Arc<dyn RouteStrategy>,
    pub(crate) balancer: Arc<dyn Balancer>,
}

impl Policy {
    /// Default policy: `NoRouteKey + RoundRobin`.
    pub fn new() -> Self {
        Self {
            router: Arc::new(NoRouteKey),
            balancer: Arc::new(RoundRobin::new()),
        }
    }

    /// Built-in sticky policy by request header + consistent hash.
    pub fn header_sticky(header: &'static str) -> Self {
        Self::new()
            .router(HeaderSticky::new(header))
            .balancer(ConsistentHash::new())
    }

    /// Built-in sticky policy by path parameter + consistent hash.
    pub fn path_sticky(param: &'static str) -> Self {
        Self::new()
            .router(PathSticky::new(param))
            .balancer(ConsistentHash::new())
    }

    /// Built-in policy: `NoRouteKey + ConsistentHash`.
    pub fn consistent_hash() -> Self {
        Self::new().balancer(ConsistentHash::new())
    }

    /// Built-in policy: `NoRouteKey + LeastRequest`.
    pub fn least_request() -> Self {
        Self::new().balancer(LeastRequest::new())
    }

    /// Built-in policy: `NoRouteKey + LeastTime`.
    pub fn least_time() -> Self {
        Self::new().balancer(LeastTime::new())
    }

    /// Built-in policy: `NoRouteKey + RoundRobin`.
    pub fn round_robin() -> Self {
        Self::new().balancer(RoundRobin::new())
    }

    /// Sets a custom routing strategy.
    pub fn router<R>(mut self, router: R) -> Self
    where
        R: RouteStrategy,
    {
        self.router = Arc::new(router);
        self
    }

    /// Sets a custom load balancer.
    pub fn balancer<B>(mut self, balancer: B) -> Self
    where
        B: Balancer,
    {
        self.balancer = Arc::new(balancer);
        self
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendPool, BaseUri};
    use crate::balancing::BalanceCtx;
    use crate::routing::{CandidateSet, RouteCtx};

    fn pool() -> BackendPool {
        BackendPool::new(vec![
            BaseUri::parse("http://127.0.0.1:8081").unwrap(),
            BaseUri::parse("http://127.0.0.1:8082").unwrap(),
        ])
    }

    fn route_ctx<'a>(
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

    fn pick(policy: &Policy, pool: &BackendPool, candidates: CandidateSet) -> Option<usize> {
        policy.balancer.pick(&BalanceCtx {
            service: "sales",
            affinity: None,
            pool,
            candidates,
        })
    }

    #[test]
    fn built_in_policies_select_from_available_backends() {
        let pool = pool();

        assert!(pick(&Policy::new(), &pool, CandidateSet::All).is_some());
        assert!(pick(&Policy::default(), &pool, CandidateSet::All).is_some());
        assert!(pick(&Policy::round_robin(), &pool, CandidateSet::All).is_some());
        assert!(pick(&Policy::least_request(), &pool, CandidateSet::All).is_some());
        assert!(pick(&Policy::least_time(), &pool, CandidateSet::All).is_some());
        assert!(pick(&Policy::consistent_hash(), &pool, CandidateSet::All).is_some());
    }

    #[test]
    fn sticky_policy_sugars_extract_expected_affinity_keys() {
        let pool = pool();
        let uri: http::Uri = "/sales/items/42".parse().unwrap();
        let mut headers = http::HeaderMap::new();
        headers.insert("x-user-id", "user-7".parse().unwrap());

        let header_policy = Policy::header_sticky("x-user-id");
        let header_decision = header_policy
            .router
            .route(&route_ctx("/items/{id}", "/sales", &uri, &headers), &pool);
        assert_eq!(header_decision.affinity.unwrap().as_str(), "user-7");

        let path_policy = Policy::path_sticky("id");
        let path_decision = path_policy
            .router
            .route(&route_ctx("/items/{id}", "/sales", &uri, &headers), &pool);
        assert_eq!(path_decision.affinity.unwrap().as_str(), "42");
    }
}
