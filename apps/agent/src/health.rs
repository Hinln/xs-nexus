use std::{
    sync::{
        RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

pub struct AgentHealth {
    started_at: Instant,
    controller_connected: AtomicBool,
    tun_packets_received: AtomicU64,
    tun_packets_dropped: AtomicU64,
    last_error_code: RwLock<Option<String>>,
}

impl AgentHealth {
    #[must_use]
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            controller_connected: AtomicBool::new(false),
            tun_packets_received: AtomicU64::new(0),
            tun_packets_dropped: AtomicU64::new(0),
            last_error_code: RwLock::new(None),
        }
    }

    pub fn set_controller_connected(&self, connected: bool) {
        self.controller_connected
            .store(connected, Ordering::Release);
        if connected {
            self.set_last_error(None);
        }
    }

    pub fn set_last_error(&self, code: Option<&str>) {
        let mut guard = self
            .last_error_code
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = code.map(str::to_owned);
    }

    pub fn record_tun_packet(&self, dropped: bool) {
        self.tun_packets_received.fetch_add(1, Ordering::Relaxed);
        if dropped {
            self.tun_packets_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[must_use]
    pub fn controller_connected(&self) -> bool {
        self.controller_connected.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    #[must_use]
    pub fn tun_packets_received(&self) -> u64 {
        self.tun_packets_received.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn tun_packets_dropped(&self) -> u64 {
        self.tun_packets_dropped.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn last_error_code(&self) -> Option<String> {
        self.last_error_code
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Default for AgentHealth {
    fn default() -> Self {
        Self::new()
    }
}
