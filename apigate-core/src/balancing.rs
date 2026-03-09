use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::backend::{Backend, BackendPool};
use crate::routing::{AffinityKey, CandidateSet};

pub struct BackendRef<'a> {
    pub index: usize,
    pub backend: &'a Backend,
}

pub struct BalanceCtx<'a> {
    pub service: &'a str,
    pub affinity: Option<&'a AffinityKey<'a>>,
    pub pool: &'a BackendPool,
    pub candidates: CandidateSet<'a>,
}

impl<'a> BalanceCtx<'a> {
    pub fn candidate_len(&self) -> usize {
        match self.candidates {
            CandidateSet::All => self.pool.len(),
            CandidateSet::Indices(indices) => indices.len(),
        }
    }

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

    pub fn candidate_backend(&self, nth: usize) -> Option<BackendRef<'a>> {
        let index = self.candidate_index(nth)?;
        let backend = self.pool.get(index)?;
        Some(BackendRef { index, backend })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProxyErrorKind {
    InvalidUpstreamUri,
    UpstreamRequest,
    NoBackends,
}

pub struct StartEvent<'a> {
    pub service: &'a str,
    pub backend_index: usize,
}

pub struct ResultEvent<'a> {
    pub service: &'a str,
    pub backend_index: usize,
    pub status: Option<http::StatusCode>,
    pub error: Option<ProxyErrorKind>,
    pub head_latency: Duration,
}

pub trait Balancer: Send + Sync + 'static {
    fn pick<'a>(&self, ctx: &'a BalanceCtx<'a>) -> Option<usize>;

    fn on_start(&self, _event: &StartEvent<'_>) {}

    fn on_result(&self, _event: &ResultEvent<'_>) {}
}

#[derive(Debug, Default)]
pub struct RoundRobin {
    next: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
        }
    }
}

impl Balancer for RoundRobin {
    fn pick<'a>(&self, ctx: &'a BalanceCtx<'a>) -> Option<usize> {
        let len = ctx.candidate_len();
        if len == 0 {
            return None;
        }

        let pos = self.next.fetch_add(1, Ordering::Relaxed) % len;
        ctx.candidate_index(pos)
    }
}
