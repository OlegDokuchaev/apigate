use std::sync::atomic::{AtomicUsize, Ordering};

use super::{BalanceCtx, Balancer};

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
    fn pick(&self, ctx: &BalanceCtx) -> Option<usize> {
        let len = ctx.candidate_len();
        if len == 0 {
            return None;
        }

        let pos = self.next.fetch_add(1, Ordering::Relaxed) % len;
        ctx.candidate_index(pos)
    }
}
