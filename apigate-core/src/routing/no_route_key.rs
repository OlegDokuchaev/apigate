use crate::backend::BackendPool;

use super::{CandidateSet, RouteCtx, RouteStrategy, RoutingDecision};

/// Route strategy that produces no affinity key and leaves all backends eligible.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoRouteKey;

impl RouteStrategy for NoRouteKey {
    fn route<'a>(&self, _ctx: &RouteCtx<'a>, _pool: &'a BackendPool) -> RoutingDecision<'a> {
        RoutingDecision {
            affinity: None,
            candidates: CandidateSet::All,
        }
    }
}
