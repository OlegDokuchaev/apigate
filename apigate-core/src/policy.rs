use std::sync::Arc;

use crate::balancing::{Balancer, RoundRobin};
use crate::routing::{NoRouteKey, RouteStrategy};

pub struct Policy {
    pub router: Arc<dyn RouteStrategy>,
    pub balancer: Arc<dyn Balancer>,
}

impl Policy {
    pub fn new() -> Self {
        Self {
            router: Arc::new(NoRouteKey),
            balancer: Arc::new(RoundRobin::new()),
        }
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
