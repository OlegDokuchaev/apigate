use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

use super::{BalanceCtx, Balancer, ResultEvent};

const EWMA_WEIGHT: u64 = 10;

pub struct LeastTime {
    ewma_us: OnceLock<Box<[AtomicU64]>>,
    offset: AtomicUsize,
}

impl LeastTime {
    pub fn new() -> Self {
        Self {
            ewma_us: OnceLock::new(),
            offset: AtomicUsize::new(0),
        }
    }

    fn latencies(&self, pool_len: usize) -> &[AtomicU64] {
        self.ewma_us.get_or_init(|| {
            (0..pool_len)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice()
        })
    }
}

impl Balancer for LeastTime {
    fn pick<'a>(&self, ctx: &'a BalanceCtx<'a>) -> Option<usize> {
        let len = ctx.candidate_len();
        if len == 0 {
            return None;
        }

        let latencies = self.latencies(ctx.pool.len());
        let offset = self.offset.fetch_add(1, Ordering::Relaxed);
        let mut best_index = None;
        let mut best_latency = u64::MAX;

        for i in 0..len {
            let nth = (offset + i) % len;
            if let Some(idx) = ctx.candidate_index(nth) {
                let latency = latencies
                    .get(idx)
                    .map(|a| a.load(Ordering::Relaxed))
                    .unwrap_or(0);
                if latency < best_latency {
                    best_latency = latency;
                    best_index = Some(idx);
                }
            }
        }

        best_index
    }

    fn on_result(&self, event: &ResultEvent<'_>) {
        if event.error.is_some() {
            return;
        }

        if let Some(latencies) = self.ewma_us.get() {
            if let Some(slot) = latencies.get(event.backend_index) {
                let sample = event.head_latency.as_micros() as u64;
                let mut old = slot.load(Ordering::Relaxed);
                loop {
                    let new = if old == 0 {
                        sample
                    } else {
                        old - old / EWMA_WEIGHT + sample / EWMA_WEIGHT
                    };
                    match slot.compare_exchange_weak(
                        old,
                        new,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(actual) => old = actual,
                    }
                }
            }
        }
    }
}
