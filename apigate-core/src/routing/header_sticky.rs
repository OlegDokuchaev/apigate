use crate::backend::BackendPool;

use super::{AffinityKey, CandidateSet, RouteCtx, RouteStrategy, RoutingDecision};

/// Route strategy that uses a request header as the affinity key.
#[derive(Debug, Clone)]
pub struct HeaderSticky {
    header: &'static str,
}

impl HeaderSticky {
    /// Creates a header-sticky route strategy.
    pub fn new(header: &'static str) -> Self {
        Self { header }
    }
}

impl RouteStrategy for HeaderSticky {
    fn route<'a>(&self, ctx: &RouteCtx<'a>, _pool: &'a BackendPool) -> RoutingDecision<'a> {
        let affinity = ctx
            .headers
            .get(self.header)
            .and_then(|v| v.to_str().ok())
            .map(AffinityKey::borrowed);

        RoutingDecision {
            affinity,
            candidates: CandidateSet::All,
        }
    }
}
