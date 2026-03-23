use crate::backend::BackendPool;

use super::{CandidateSet, RouteCtx, RouteStrategy, RoutingDecision};

#[derive(Debug, Default, Clone, Copy)]
pub struct NoRouteKey;

impl RouteStrategy for NoRouteKey {
    fn route<'a>(&self, _ctx: &'_ RouteCtx, _pool: &'a BackendPool) -> RoutingDecision<'a> {
        RoutingDecision {
            affinity: None,
            candidates: CandidateSet::All,
        }
    }
}
