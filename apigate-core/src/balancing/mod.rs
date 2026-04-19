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
