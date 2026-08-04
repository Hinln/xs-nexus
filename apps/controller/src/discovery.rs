use std::{
    collections::{HashMap, VecDeque},
    io,
    net::IpAddr,
    time::{Duration, Instant},
};

use chrono::Utc;
use sqlx::Row as _;
use tokio::{net::UdpSocket, sync::watch};
use uuid::Uuid;
use xs_protocol::{DISCOVERY_REQUEST_LENGTH, discovery_response, verify_discovery_request};

use crate::state::AppState;

const MAX_TRACKED_SOURCES: usize = 1024;
const SOURCE_WINDOW: Duration = Duration::from_mins(1);
const MAX_REQUESTS_PER_SOURCE: u16 = 30;

struct SourceBudget {
    window_started: Instant,
    requests: u16,
}

struct RateLimiter {
    budgets: HashMap<IpAddr, SourceBudget>,
    order: VecDeque<IpAddr>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            budgets: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn allow(&mut self, source: IpAddr, now: Instant) -> bool {
        if let Some(budget) = self.budgets.get_mut(&source) {
            if now.duration_since(budget.window_started) >= SOURCE_WINDOW {
                budget.window_started = now;
                budget.requests = 1;
                return true;
            }
            if budget.requests >= MAX_REQUESTS_PER_SOURCE {
                return false;
            }
            budget.requests = budget.requests.saturating_add(1);
            return true;
        }
        if self.budgets.len() >= MAX_TRACKED_SOURCES
            && let Some(oldest) = self.order.pop_front()
        {
            self.budgets.remove(&oldest);
        }
        self.order.push_back(source);
        self.budgets.insert(
            source,
            SourceBudget {
                window_started: now,
                requests: 1,
            },
        );
        true
    }
}

/// Serves authenticated UDP mapping-discovery requests until shutdown.
///
/// Invalid and unauthenticated datagrams are silently dropped.
///
/// # Errors
///
/// Returns an I/O error only when the local UDP socket fails.
pub async fn serve(
    socket: UdpSocket,
    state: AppState,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let mut datagram = [0_u8; DISCOVERY_REQUEST_LENGTH];
    let mut limiter = RateLimiter::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
            }
            received = socket.recv_from(&mut datagram) => {
                let (length, source) = received?;
                if length != DISCOVERY_REQUEST_LENGTH
                    || !limiter.allow(source.ip(), Instant::now())
                {
                    continue;
                }
                let Ok(now) = u64::try_from(Utc::now().timestamp()) else {
                    continue;
                };
                let Ok(request) = verify_discovery_request(
                    &datagram[..length],
                    &state.credential_signing_key.verifying_key(),
                    now,
                ) else {
                    continue;
                };
                if !active_node(&state, request).await {
                    continue;
                }
                let Ok(response) = discovery_response(
                    request,
                    source,
                    now,
                    &state.config_signing_key,
                ) else {
                    continue;
                };
                let _ = socket.send_to(&response, source).await?;
            }
        }
    }
}

async fn active_node(state: &AppState, request: xs_protocol::VerifiedDiscoveryRequest) -> bool {
    let network_id = Uuid::from_bytes(request.network_id);
    sqlx::query(
        "SELECT id
         FROM nodes
         WHERE network_id = $1
           AND node_id = $2
           AND identity_public_key = $3
           AND credential_serial = $4
           AND revoked_at IS NULL
           AND credential_not_after > now()",
    )
    .bind(network_id)
    .bind(request.node_id.as_slice())
    .bind(request.identity_public_key.as_slice())
    .bind(i64::try_from(request.credential_serial).unwrap_or(i64::MAX))
    .fetch_optional(&state.pool)
    .await
    .is_ok_and(|row| row.is_some_and(|row| row.try_get::<Uuid, _>("id").is_ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_rate_limit_is_bounded_and_resets() {
        let source = IpAddr::from([192, 0, 2, 1]);
        let start = Instant::now();
        let mut limiter = RateLimiter::new();
        for _ in 0..MAX_REQUESTS_PER_SOURCE {
            assert!(limiter.allow(source, start));
        }
        assert!(!limiter.allow(source, start));
        assert!(limiter.allow(source, start + SOURCE_WINDOW));
    }
}
