use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use serde::Serialize;

#[derive(Clone, Default)]
pub struct RelayMetrics {
    inner: Arc<MetricsInner>,
}

#[derive(Default)]
struct MetricsInner {
    active_leases: AtomicUsize,
    registrations_accepted: AtomicU64,
    registration_retries: AtomicU64,
    registrations_rejected: AtomicU64,
    packets_forwarded: AtomicU64,
    bytes_forwarded: AtomicU64,
    keepalives_accepted: AtomicU64,
    invalid_drops: AtomicU64,
    authentication_drops: AtomicU64,
    replay_drops: AtomicU64,
    rate_limit_drops: AtomicU64,
    queue_drops: AtomicU64,
    destination_drops: AtomicU64,
    send_drops: AtomicU64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RelayMetricsSnapshot {
    pub active_leases: usize,
    pub registrations_accepted: u64,
    pub registration_retries: u64,
    pub registrations_rejected: u64,
    pub packets_forwarded: u64,
    pub bytes_forwarded: u64,
    pub keepalives_accepted: u64,
    pub invalid_drops: u64,
    pub authentication_drops: u64,
    pub replay_drops: u64,
    pub rate_limit_drops: u64,
    pub queue_drops: u64,
    pub destination_drops: u64,
    pub send_drops: u64,
}

impl RelayMetrics {
    pub(crate) fn set_active_leases(&self, value: usize) {
        self.inner.active_leases.store(value, Ordering::Relaxed);
    }

    pub(crate) fn registration_accepted(&self) {
        increment(&self.inner.registrations_accepted);
    }

    pub(crate) fn registration_retry(&self) {
        increment(&self.inner.registration_retries);
    }

    pub(crate) fn registration_rejected(&self) {
        increment(&self.inner.registrations_rejected);
    }

    pub(crate) fn forwarded(&self, bytes: usize) {
        increment(&self.inner.packets_forwarded);
        self.inner
            .bytes_forwarded
            .fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    pub(crate) fn keepalive_accepted(&self) {
        increment(&self.inner.keepalives_accepted);
    }

    pub(crate) fn invalid_drop(&self) {
        increment(&self.inner.invalid_drops);
    }

    pub(crate) fn authentication_drop(&self) {
        increment(&self.inner.authentication_drops);
    }

    pub(crate) fn replay_drop(&self) {
        increment(&self.inner.replay_drops);
    }

    pub(crate) fn rate_limit_drop(&self) {
        increment(&self.inner.rate_limit_drops);
    }

    pub(crate) fn queue_drop(&self) {
        increment(&self.inner.queue_drops);
    }

    pub(crate) fn destination_drop(&self) {
        increment(&self.inner.destination_drops);
    }

    pub(crate) fn send_drop(&self) {
        increment(&self.inner.send_drops);
    }

    #[must_use]
    pub fn snapshot(&self) -> RelayMetricsSnapshot {
        RelayMetricsSnapshot {
            active_leases: self.inner.active_leases.load(Ordering::Relaxed),
            registrations_accepted: self.inner.registrations_accepted.load(Ordering::Relaxed),
            registration_retries: self.inner.registration_retries.load(Ordering::Relaxed),
            registrations_rejected: self.inner.registrations_rejected.load(Ordering::Relaxed),
            packets_forwarded: self.inner.packets_forwarded.load(Ordering::Relaxed),
            bytes_forwarded: self.inner.bytes_forwarded.load(Ordering::Relaxed),
            keepalives_accepted: self.inner.keepalives_accepted.load(Ordering::Relaxed),
            invalid_drops: self.inner.invalid_drops.load(Ordering::Relaxed),
            authentication_drops: self.inner.authentication_drops.load(Ordering::Relaxed),
            replay_drops: self.inner.replay_drops.load(Ordering::Relaxed),
            rate_limit_drops: self.inner.rate_limit_drops.load(Ordering::Relaxed),
            queue_drops: self.inner.queue_drops.load(Ordering::Relaxed),
            destination_drops: self.inner.destination_drops.load(Ordering::Relaxed),
            send_drops: self.inner.send_drops.load(Ordering::Relaxed),
        }
    }
}

fn increment(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}
