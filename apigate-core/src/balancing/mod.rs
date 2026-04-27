mod consistent_hash;
mod least_request;
mod least_time;
mod round_robin;

use std::time::Duration;

use crate::backend::{Backend, BackendPool};
use crate::routing::{AffinityKey, CandidateSet};

pub use consistent_hash::ConsistentHash;
pub use least_request::LeastRequest;
pub use least_time::LeastTime;
pub use round_robin::RoundRobin;

/// Backend reference returned by [`BalanceCtx::candidate_backend`].
pub struct BackendRef<'a> {
    /// Stable backend index in the service pool.
    pub index: usize,
    /// Backend at `index`.
    pub backend: &'a Backend,
}

/// Context passed to a load balancer.
pub struct BalanceCtx<'a> {
    /// Logical service name.
    pub service: &'a str,
    /// Optional affinity key produced by a route strategy.
    pub affinity: Option<&'a AffinityKey<'a>>,
    /// Complete backend pool for the service.
    pub pool: &'a BackendPool,
    /// Candidate backends selected by a route strategy.
    pub candidates: CandidateSet<'a>,
}

impl<'a> BalanceCtx<'a> {
    /// Returns the number of candidate backends.
    pub fn candidate_len(&self) -> usize {
        match self.candidates {
            CandidateSet::All => self.pool.len(),
            CandidateSet::Indices(indices) => indices.len(),
        }
    }

    /// Returns the backend pool index for the `nth` candidate.
    pub fn candidate_index(&self, nth: usize) -> Option<usize> {
        match self.candidates {
            CandidateSet::All => {
                if nth < self.pool.len() {
                    Some(nth)
                } else {
                    None
                }
            }
            CandidateSet::Indices(indices) => indices.get(nth).copied(),
        }
    }

    /// Returns the `nth` candidate backend.
    pub fn candidate_backend(&self, nth: usize) -> Option<BackendRef<'a>> {
        let index = self.candidate_index(nth)?;
        let backend = self.pool.get(index)?;
        Some(BackendRef { index, backend })
    }

    /// Returns whether `backend_idx` is included in the candidate set.
    pub fn is_candidate(&self, backend_idx: usize) -> bool {
        match self.candidates {
            CandidateSet::All => backend_idx < self.pool.len(),
            CandidateSet::Indices(indices) => indices.contains(&backend_idx),
        }
    }
}

/// Proxy error kind reported to balancers and runtime observers.
#[derive(Debug, Clone, Copy)]
pub enum ProxyErrorKind {
    /// Failed to build a valid upstream URI.
    InvalidUpstreamUri,
    /// Upstream request failed before a response was received.
    UpstreamRequest,
    /// No upstream backends were available.
    NoBackends,
    /// Upstream request timed out.
    Timeout,
}

/// Event emitted to a balancer when an upstream request starts.
pub struct StartEvent<'a> {
    /// Logical service name.
    pub service: &'a str,
    /// Backend index selected for the request.
    pub backend_index: usize,
}

/// Event emitted to a balancer when an upstream request finishes.
pub struct ResultEvent<'a> {
    /// Logical service name.
    pub service: &'a str,
    /// Backend index used for the request.
    pub backend_index: usize,
    /// Upstream response status, if a response was received.
    pub status: Option<http::StatusCode>,
    /// Proxy error, if the request failed before a response was received.
    pub error: Option<ProxyErrorKind>,
    /// Time from dispatch start to response head or failure.
    pub head_latency: Duration,
}

/// Load-balancing strategy for selecting an upstream backend.
pub trait Balancer: Send + Sync + 'static {
    /// Selects a backend index from the candidate set.
    fn pick(&self, ctx: &BalanceCtx) -> Option<usize>;

    /// Called after a backend is selected and before the request is sent.
    fn on_start(&self, _event: &StartEvent) {}

    /// Called after the request succeeds or fails.
    fn on_result(&self, _event: &ResultEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::backend::{BackendPool, BaseUri};
    use crate::routing::{AffinityKey, CandidateSet};

    fn pool() -> BackendPool {
        BackendPool::new(vec![
            BaseUri::parse("http://127.0.0.1:8081").unwrap(),
            BaseUri::parse("http://127.0.0.1:8082").unwrap(),
            BaseUri::parse("http://127.0.0.1:8083").unwrap(),
        ])
    }

    fn ctx<'a>(
        pool: &'a BackendPool,
        candidates: CandidateSet<'a>,
        affinity: Option<&'a AffinityKey<'a>>,
    ) -> BalanceCtx<'a> {
        BalanceCtx {
            service: "sales",
            affinity,
            pool,
            candidates,
        }
    }

    #[test]
    fn balance_ctx_resolves_candidate_sets() {
        let pool = pool();
        let all = ctx(&pool, CandidateSet::All, None);

        assert_eq!(all.candidate_len(), 3);
        assert_eq!(all.candidate_index(2), Some(2));
        assert_eq!(all.candidate_index(3), None);
        assert_eq!(all.candidate_backend(1).unwrap().index, 1);
        assert!(all.is_candidate(2));
        assert!(!all.is_candidate(3));

        let indices = [2, 0];
        let filtered = ctx(&pool, CandidateSet::Indices(&indices), None);

        assert_eq!(filtered.candidate_len(), 2);
        assert_eq!(filtered.candidate_index(0), Some(2));
        assert_eq!(filtered.candidate_index(1), Some(0));
        assert_eq!(filtered.candidate_index(2), None);
        assert!(filtered.is_candidate(0));
        assert!(!filtered.is_candidate(1));
    }

    #[test]
    fn round_robin_cycles_over_candidates() {
        let pool = pool();
        let balancer = RoundRobin::new();
        let candidates = [2, 0];
        let ctx = ctx(&pool, CandidateSet::Indices(&candidates), None);

        assert_eq!(balancer.pick(&ctx), Some(2));
        assert_eq!(balancer.pick(&ctx), Some(0));
        assert_eq!(balancer.pick(&ctx), Some(2));
    }

    #[test]
    fn balancers_return_none_for_empty_candidate_sets() {
        let pool = pool();
        let empty = [];
        let ctx = ctx(&pool, CandidateSet::Indices(&empty), None);

        assert_eq!(RoundRobin::new().pick(&ctx), None);
        assert_eq!(ConsistentHash::new().pick(&ctx), None);
        assert_eq!(LeastRequest::new().pick(&ctx), None);
        assert_eq!(LeastTime::new().pick(&ctx), None);
    }

    #[test]
    fn consistent_hash_is_stable_for_same_affinity_and_respects_candidates() {
        let pool = pool();
        let balancer = ConsistentHash::new();
        let key = AffinityKey::borrowed("user-1");
        let candidates = [2, 0];
        let ctx = ctx(&pool, CandidateSet::Indices(&candidates), Some(&key));

        let first = balancer.pick(&ctx).unwrap();
        for _ in 0..10 {
            assert_eq!(balancer.pick(&ctx), Some(first));
        }
        assert!(candidates.contains(&first));
    }

    #[test]
    fn consistent_hash_without_affinity_falls_back_to_round_robin() {
        let pool = pool();
        let balancer = ConsistentHash::new();
        let candidates = [1, 2];
        let ctx = ctx(&pool, CandidateSet::Indices(&candidates), None);

        assert_eq!(balancer.pick(&ctx), Some(1));
        assert_eq!(balancer.pick(&ctx), Some(2));
        assert_eq!(balancer.pick(&ctx), Some(1));
    }

    #[test]
    fn least_request_prefers_backend_with_fewer_in_flight_requests() {
        let pool = pool();
        let balancer = LeastRequest::new();
        let ctx = ctx(&pool, CandidateSet::All, None);

        assert_eq!(balancer.pick(&ctx), Some(0));
        balancer.on_start(&StartEvent {
            service: "sales",
            backend_index: 0,
        });

        assert_eq!(balancer.pick(&ctx), Some(1));

        balancer.on_result(&ResultEvent {
            service: "sales",
            backend_index: 0,
            status: Some(http::StatusCode::OK),
            error: None,
            head_latency: Duration::from_millis(1),
        });

        assert_eq!(balancer.pick(&ctx), Some(2));
    }

    #[test]
    fn least_time_prefers_unobserved_or_lower_latency_backend() {
        let pool = pool();
        let balancer = LeastTime::new();
        let ctx = ctx(&pool, CandidateSet::All, None);

        assert_eq!(balancer.pick(&ctx), Some(0));
        balancer.on_result(&ResultEvent {
            service: "sales",
            backend_index: 0,
            status: Some(http::StatusCode::OK),
            error: None,
            head_latency: Duration::from_millis(50),
        });

        assert_eq!(balancer.pick(&ctx), Some(1));

        balancer.on_result(&ResultEvent {
            service: "sales",
            backend_index: 1,
            status: None,
            error: Some(ProxyErrorKind::Timeout),
            head_latency: Duration::from_millis(500),
        });

        assert_eq!(balancer.pick(&ctx), Some(2));
    }
}
