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
    packets_received: AtomicU64,
    bytes_received: AtomicU64,
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
    io_errors: AtomicU64,
    forwarding_latency_samples: AtomicU64,
    forwarding_latency_microseconds_total: AtomicU64,
    forwarding_latency_microseconds_max: AtomicU64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RelayMetricsSnapshot {
    pub active_leases: usize,
    pub packets_received: u64,
    pub bytes_received: u64,
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
    pub packets_dropped: u64,
    pub io_errors: u64,
    pub forwarding_latency_samples: u64,
    pub forwarding_latency_microseconds_average: Option<u64>,
    pub forwarding_latency_microseconds_max: u64,
}

impl RelayMetrics {
    pub(crate) fn received(&self, bytes: usize) {
        increment(&self.inner.packets_received);
        add_bytes(&self.inner.bytes_received, bytes);
    }

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

    pub(crate) fn forwarded(&self, bytes: usize, latency: std::time::Duration) {
        increment(&self.inner.packets_forwarded);
        add_bytes(&self.inner.bytes_forwarded, bytes);
        let micros = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        increment(&self.inner.forwarding_latency_samples);
        self.inner
            .forwarding_latency_microseconds_total
            .fetch_add(micros, Ordering::Relaxed);
        self.inner
            .forwarding_latency_microseconds_max
            .fetch_max(micros, Ordering::Relaxed);
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
        increment(&self.inner.io_errors);
    }

    #[must_use]
    pub fn snapshot(&self) -> RelayMetricsSnapshot {
        let invalid_drops = self.inner.invalid_drops.load(Ordering::Relaxed);
        let authentication_drops = self.inner.authentication_drops.load(Ordering::Relaxed);
        let replay_drops = self.inner.replay_drops.load(Ordering::Relaxed);
        let rate_limit_drops = self.inner.rate_limit_drops.load(Ordering::Relaxed);
        let queue_drops = self.inner.queue_drops.load(Ordering::Relaxed);
        let destination_drops = self.inner.destination_drops.load(Ordering::Relaxed);
        let send_drops = self.inner.send_drops.load(Ordering::Relaxed);
        let forwarding_latency_samples = self
            .inner
            .forwarding_latency_samples
            .load(Ordering::Relaxed);
        let forwarding_latency_microseconds_total = self
            .inner
            .forwarding_latency_microseconds_total
            .load(Ordering::Relaxed);
        RelayMetricsSnapshot {
            active_leases: self.inner.active_leases.load(Ordering::Relaxed),
            packets_received: self.inner.packets_received.load(Ordering::Relaxed),
            bytes_received: self.inner.bytes_received.load(Ordering::Relaxed),
            registrations_accepted: self.inner.registrations_accepted.load(Ordering::Relaxed),
            registration_retries: self.inner.registration_retries.load(Ordering::Relaxed),
            registrations_rejected: self.inner.registrations_rejected.load(Ordering::Relaxed),
            packets_forwarded: self.inner.packets_forwarded.load(Ordering::Relaxed),
            bytes_forwarded: self.inner.bytes_forwarded.load(Ordering::Relaxed),
            keepalives_accepted: self.inner.keepalives_accepted.load(Ordering::Relaxed),
            invalid_drops,
            authentication_drops,
            replay_drops,
            rate_limit_drops,
            queue_drops,
            destination_drops,
            send_drops,
            packets_dropped: invalid_drops
                .saturating_add(authentication_drops)
                .saturating_add(replay_drops)
                .saturating_add(rate_limit_drops)
                .saturating_add(queue_drops)
                .saturating_add(destination_drops)
                .saturating_add(send_drops),
            io_errors: self.inner.io_errors.load(Ordering::Relaxed),
            forwarding_latency_samples,
            forwarding_latency_microseconds_average: (forwarding_latency_samples != 0)
                .then(|| forwarding_latency_microseconds_total / forwarding_latency_samples),
            forwarding_latency_microseconds_max: self
                .inner
                .forwarding_latency_microseconds_max
                .load(Ordering::Relaxed),
        }
    }
}

fn increment(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

fn add_bytes(counter: &AtomicU64, bytes: usize) {
    counter.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_exposes_bytes_drops_errors_and_latency() {
        let metrics = RelayMetrics::default();
        metrics.received(120);
        metrics.invalid_drop();
        metrics.queue_drop();
        metrics.send_drop();
        metrics.forwarded(80, std::time::Duration::from_micros(25));
        metrics.forwarded(40, std::time::Duration::from_micros(75));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.packets_received, 1);
        assert_eq!(snapshot.bytes_received, 120);
        assert_eq!(snapshot.packets_forwarded, 2);
        assert_eq!(snapshot.bytes_forwarded, 120);
        assert_eq!(snapshot.packets_dropped, 3);
        assert_eq!(snapshot.io_errors, 1);
        assert_eq!(snapshot.forwarding_latency_samples, 2);
        assert_eq!(snapshot.forwarding_latency_microseconds_average, Some(50));
        assert_eq!(snapshot.forwarding_latency_microseconds_max, 75);
    }
}
