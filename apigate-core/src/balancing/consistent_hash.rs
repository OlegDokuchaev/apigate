use std::sync::atomic::{AtomicUsize, Ordering};

use super::{BalanceCtx, Balancer};

pub struct ConsistentHash {
    offset: AtomicUsize,
}

impl ConsistentHash {
    pub fn new() -> Self {
        Self {
            offset: AtomicUsize::new(0),
        }
    }
}

impl Default for ConsistentHash {
    fn default() -> Self {
        Self::new()
    }
}

impl Balancer for ConsistentHash {
    #[inline]
    fn pick(&self, ctx: &BalanceCtx) -> Option<usize> {
        let candidate_len = ctx.candidate_len();
        if candidate_len == 0 {
            return None;
        }

        match ctx.affinity {
            Some(key) => {
                let hash = xxhash_rust::xxh3::xxh3_64(key.as_str().as_bytes());
                pick_candidate(hash, ctx)
            }
            None => {
                let n = self.offset.fetch_add(1, Ordering::Relaxed);

                let nth = if candidate_len.is_power_of_two() {
                    n & (candidate_len - 1)
                } else {
                    n % candidate_len
                };

                ctx.candidate_index(nth)
            }
        }
    }
}

#[inline]
fn pick_candidate(hash: u64, ctx: &BalanceCtx<'_>) -> Option<usize> {
    let bucket = jump_consistent_hash(hash, ctx.candidate_len())?;
    ctx.candidate_index(bucket)
}

#[inline]
fn jump_consistent_hash(mut key: u64, buckets: usize) -> Option<usize> {
    if buckets == 0 {
        return None;
    }

    let mut b: i64 = -1;
    let mut j: i64 = 0;
    let buckets = buckets as i64;

    while j < buckets {
        b = j;
        key = key.wrapping_mul(2862933555777941757).wrapping_add(1);
        j = ((b + 1) * (1_i64 << 31)) / (((key >> 33) + 1) as i64);
    }

    Some(b as usize)
}
