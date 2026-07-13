//! Shared monotonic freshness tracking for include cache entries.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct RevalidationClock {
    interval: Duration,
    epoch: Instant,
}

impl RevalidationClock {
    pub(super) fn new(interval: Duration) -> Self {
        Self {
            interval,
            epoch: Instant::now(),
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.interval > Duration::ZERO
    }

    pub(super) fn stamp(&self) -> ValidationStamp {
        ValidationStamp(AtomicU64::new(self.now_nanos()))
    }

    pub(super) fn is_fresh(&self, stamp: &ValidationStamp) -> bool {
        self.now_nanos()
            .saturating_sub(stamp.0.load(Ordering::Relaxed))
            < self.interval.as_nanos() as u64
    }

    pub(super) fn touch(&self, stamp: &ValidationStamp) {
        stamp.0.store(self.now_nanos(), Ordering::Relaxed);
    }

    fn now_nanos(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }
}

#[derive(Debug)]
pub(super) struct ValidationStamp(AtomicU64);
