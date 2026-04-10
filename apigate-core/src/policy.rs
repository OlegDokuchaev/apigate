use std::sync::Arc;

use crate::balancing::{Balancer, ConsistentHash, LeastRequest, LeastTime, RoundRobin};
use crate::routing::{HeaderSticky, NoRouteKey, PathSticky, RouteStrategy};

pub struct Policy {
    pub router: Arc<dyn RouteStrategy>,
    pub balancer: Arc<dyn Balancer>,
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

    pub fn router<R>(mut self, router: R) -> Self
    where
        R: RouteStrategy,
    {
        self.router = Arc::new(router);
        self
    }

    pub fn balancer<B>(mut self, balancer: B) -> Self
    where
        B: Balancer,
    {
        self.balancer = Arc::new(balancer);
        self
    }
}
