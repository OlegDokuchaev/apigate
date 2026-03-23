use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use super::{BalanceCtx, Balancer, ResultEvent, StartEvent};

pub struct LeastRequest {
    in_flight: OnceLock<Box<[AtomicUsize]>>,
    offset: AtomicUsize,
}

impl LeastRequest {
    pub fn new() -> Self {
        Self {
            in_flight: OnceLock::new(),
            offset: AtomicUsize::new(0),
        }
    }

    fn counters(&self, pool_len: usize) -> &[AtomicUsize] {
        self.in_flight.get_or_init(|| {
            (0..pool_len)
                .map(|_| AtomicUsize::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice()
        })
    }
}

impl Balancer for LeastRequest {
    fn pick<'a>(&self, ctx: &'a BalanceCtx<'a>) -> Option<usize> {
        let len = ctx.candidate_len();
        if len == 0 {
            return None;
        }

        let counters = self.counters(ctx.pool.len());
        let offset = self.offset.fetch_add(1, Ordering::Relaxed);
        let mut best_index = None;
        let mut best_count = usize::MAX;

        for i in 0..len {
            let nth = (offset + i) % len;
            if let Some(idx) = ctx.candidate_index(nth) {
                let count = counters
                    .get(idx)
                    .map(|a| a.load(Ordering::Relaxed))
                    .unwrap_or(0);
                if count < best_count {
                    best_count = count;
                    best_index = Some(idx);
                }
            }
        }

        best_index
    }

    fn on_start(&self, event: &StartEvent<'_>) {
        if let Some(counters) = self.in_flight.get() {
            if let Some(counter) = counters.get(event.backend_index) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn on_result(&self, event: &ResultEvent<'_>) {
        if let Some(counters) = self.in_flight.get() {
            if let Some(counter) = counters.get(event.backend_index) {
                counter.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}
