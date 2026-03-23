use crate::backend::BackendPool;

use super::{AffinityKey, CandidateSet, RouteCtx, RouteStrategy, RoutingDecision};

#[derive(Debug, Clone)]
pub struct HeaderSticky {
    header: &'static str,
}

impl HeaderSticky {
    pub fn new(header: &'static str) -> Self {
        Self { header }
    }
}

impl RouteStrategy for HeaderSticky {
    fn route<'a>(&self, ctx: &'_ RouteCtx, _pool: &'a BackendPool) -> RoutingDecision<'a> {
        let affinity = ctx
            .headers
            .get(self.header)
            .and_then(|v| v.to_str().ok())
            .map(AffinityKey::owned);

        RoutingDecision {
            affinity,
            candidates: CandidateSet::All,
        }
    }
}
