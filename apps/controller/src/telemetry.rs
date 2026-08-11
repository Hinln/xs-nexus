use std::collections::{HashMap, HashSet};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use sqlx::Row as _;
use xs_core::{
    AgentPathKind, AgentPeerTelemetry, AgentTelemetryReport, MAX_AGENT_TELEMETRY_PEERS,
    agent_telemetry_report_signing_input,
};

use crate::{error::ApiError, service::AuthenticatedNode, state::AppState};

const MAX_TELEMETRY_REPORT_BYTES: usize = 512 * 1024;
const MAX_LATENCY_MICROSECONDS: u64 = 10 * 60 * 1_000_000;
const MAX_SAMPLES_PER_NODE: i64 = 1_800;

#[derive(Clone, Copy)]
struct TelemetryTotals {
    tx_bytes: i64,
    rx_bytes: i64,
    handshake_attempts: i64,
    handshake_successes: i64,
    acl_drops: i64,
    replay_drops: i64,
    latency_samples: i64,
    latency_microseconds: i64,
}

#[derive(Default)]
struct PeerTelemetryTotals {
    tx_bytes: u64,
    rx_bytes: u64,
    handshake_attempts: u64,
    handshake_successes: u64,
    latency_samples: u64,
    latency_microseconds: u64,
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn record_agent_report(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    report: AgentTelemetryReport,
    signature_base64: &str,
) -> Result<u64, ApiError> {
    let (boot_id, totals, report_json) = validate_report(state, authenticated, &report)?;
    let signature = decode_array::<64>(signature_base64).ok_or_else(ApiError::unauthorized)?;
    let signing_input =
        agent_telemetry_report_signing_input(&report).map_err(|_| ApiError::validation())?;
    if signing_input.len() > MAX_TELEMETRY_REPORT_BYTES {
        return Err(ApiError::validation());
    }
    let verifying_key = VerifyingKey::from_bytes(&authenticated.identity_public_key)
        .map_err(|_| ApiError::unauthorized())?;
    verifying_key
        .verify_strict(&signing_input, &Signature::from_bytes(&signature))
        .map_err(|_| ApiError::unauthorized())?;

    let mut transaction = state.pool.begin().await.map_err(database_error)?;
    let active_peer_rows = sqlx::query(
        "SELECT node_id FROM nodes
         WHERE network_id = $1 AND node_id <> $2 AND revoked_at IS NULL",
    )
    .bind(authenticated.network_id)
    .bind(authenticated.node_id.as_slice())
    .fetch_all(&mut *transaction)
    .await
    .map_err(database_error)?;
    let active_peers = active_peer_rows
        .into_iter()
        .map(|row| {
            row.try_get::<Vec<u8>, _>("node_id")
                .map_err(database_error)?
                .try_into()
                .map_err(|_| ApiError::internal())
        })
        .collect::<Result<HashSet<[u8; 16]>, ApiError>>()?;
    let reported_peers = report
        .peers
        .iter()
        .map(|peer| decode_array::<16>(&peer.peer_node_id_base64).ok_or_else(ApiError::validation))
        .collect::<Result<HashSet<_>, _>>()?;
    if reported_peers != active_peers {
        return Err(ApiError::validation());
    }

    let previous = sqlx::query(
        "SELECT boot_id, sequence, report, tx_bytes_total, rx_bytes_total,
                handshake_attempts_total, handshake_successes_total,
                acl_drops_total, replay_drops_total,
                latency_samples_total, latency_microseconds_total, generated_at
         FROM node_telemetry_reports WHERE node_id = $1 FOR UPDATE",
    )
    .bind(authenticated.node_id.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?;
    if let Some(previous) = previous {
        let previous_boot_id = previous
            .try_get::<Vec<u8>, _>("boot_id")
            .map_err(database_error)?;
        let previous_generated_at = previous
            .try_get::<chrono::DateTime<Utc>, _>("generated_at")
            .map_err(database_error)?;
        if report.generated_at <= previous_generated_at {
            return Err(ApiError::conflict());
        }
        if previous_boot_id.as_slice() == boot_id {
            let previous_sequence = previous
                .try_get::<i64, _>("sequence")
                .map_err(database_error)?;
            let previous_report: AgentTelemetryReport = serde_json::from_value(
                previous
                    .try_get::<serde_json::Value, _>("report")
                    .map_err(database_error)?,
            )
            .map_err(|_| ApiError::internal())?;
            if totals_decreased(&previous, totals)
                || i64::try_from(report.sequence).map_err(|_| ApiError::validation())?
                    <= previous_sequence
                || !peer_totals_monotonic(&previous_report, &report)
            {
                return Err(ApiError::conflict());
            }
        }
    }

    sqlx::query(
        "INSERT INTO node_telemetry_samples
         (node_id, network_id, boot_id, sequence, tx_bytes_total, rx_bytes_total,
          handshake_attempts_total, handshake_successes_total,
          acl_drops_total, replay_drops_total, latency_samples_total,
          latency_microseconds_total, generated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(authenticated.node_id.as_slice())
    .bind(authenticated.network_id)
    .bind(boot_id.as_slice())
    .bind(i64::try_from(report.sequence).map_err(|_| ApiError::validation())?)
    .bind(totals.tx_bytes)
    .bind(totals.rx_bytes)
    .bind(totals.handshake_attempts)
    .bind(totals.handshake_successes)
    .bind(totals.acl_drops)
    .bind(totals.replay_drops)
    .bind(totals.latency_samples)
    .bind(totals.latency_microseconds)
    .bind(report.generated_at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiError::conflict())?;

    sqlx::query(
        "INSERT INTO node_telemetry_reports
         (node_id, network_id, boot_id, sequence, report, tx_bytes_total, rx_bytes_total,
          handshake_attempts_total, handshake_successes_total,
          acl_drops_total, replay_drops_total, latency_samples_total,
          latency_microseconds_total, generated_at, received_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, now())
         ON CONFLICT (node_id) DO UPDATE SET
           network_id = EXCLUDED.network_id, boot_id = EXCLUDED.boot_id,
           sequence = EXCLUDED.sequence, report = EXCLUDED.report,
           tx_bytes_total = EXCLUDED.tx_bytes_total,
           rx_bytes_total = EXCLUDED.rx_bytes_total,
           handshake_attempts_total = EXCLUDED.handshake_attempts_total,
           handshake_successes_total = EXCLUDED.handshake_successes_total,
           acl_drops_total = EXCLUDED.acl_drops_total,
           replay_drops_total = EXCLUDED.replay_drops_total,
           latency_samples_total = EXCLUDED.latency_samples_total,
           latency_microseconds_total = EXCLUDED.latency_microseconds_total,
           generated_at = EXCLUDED.generated_at, received_at = now()",
    )
    .bind(authenticated.node_id.as_slice())
    .bind(authenticated.network_id)
    .bind(boot_id.as_slice())
    .bind(i64::try_from(report.sequence).map_err(|_| ApiError::validation())?)
    .bind(report_json)
    .bind(totals.tx_bytes)
    .bind(totals.rx_bytes)
    .bind(totals.handshake_attempts)
    .bind(totals.handshake_successes)
    .bind(totals.acl_drops)
    .bind(totals.replay_drops)
    .bind(totals.latency_samples)
    .bind(totals.latency_microseconds)
    .bind(report.generated_at)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;

    sqlx::query(
        "DELETE FROM node_telemetry_samples
         WHERE node_id = $1 AND received_at < now() - interval '25 hours'",
    )
    .bind(authenticated.node_id.as_slice())
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "DELETE FROM node_telemetry_samples
         WHERE (node_id, boot_id, sequence) IN (
           SELECT node_id, boot_id, sequence FROM node_telemetry_samples
           WHERE node_id = $1 ORDER BY received_at DESC, sequence DESC OFFSET $2
         )",
    )
    .bind(authenticated.node_id.as_slice())
    .bind(MAX_SAMPLES_PER_NODE)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    transaction.commit().await.map_err(database_error)?;
    Ok(report.sequence)
}

fn validate_report(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    report: &AgentTelemetryReport,
) -> Result<([u8; 16], TelemetryTotals, serde_json::Value), ApiError> {
    validate_report_shape(authenticated, report)?;
    let boot_id = decode_array::<16>(&report.boot_id_base64).ok_or_else(ApiError::validation)?;
    if boot_id == [0_u8; 16] {
        return Err(ApiError::validation());
    }
    let peer_totals = validate_report_peers(state, authenticated, report)?;
    if !report_contains_peer_totals(report, &peer_totals) {
        return Err(ApiError::validation());
    }
    let totals = TelemetryTotals {
        tx_bytes: to_i64(report.tx_bytes_total)?,
        rx_bytes: to_i64(report.rx_bytes_total)?,
        handshake_attempts: to_i64(report.handshake_attempts_total)?,
        handshake_successes: to_i64(report.handshake_successes_total)?,
        acl_drops: to_i64(report.acl_drops_total)?,
        replay_drops: to_i64(report.replay_drops_total)?,
        latency_samples: to_i64(report.latency_samples_total)?,
        latency_microseconds: to_i64(report.latency_microseconds_total)?,
    };
    let report_json = serde_json::to_value(report).map_err(|_| ApiError::validation())?;
    Ok((boot_id, totals, report_json))
}

fn validate_report_shape(
    authenticated: &AuthenticatedNode,
    report: &AgentTelemetryReport,
) -> Result<(), ApiError> {
    let now = Utc::now();
    if !matches!(report.schema_version, 1 | 2)
        || (report.schema_version == 1
            && (report.acl_drops_total != 0 || report.replay_drops_total != 0))
        || report.network_id != authenticated.network_id
        || report.node_id_base64 != authenticated.node_id_base64
        || report.sequence == 0
        || report.peers.len() > MAX_AGENT_TELEMETRY_PEERS
        || report.generated_at < now - Duration::minutes(10)
        || report.generated_at > now + Duration::minutes(5)
        || report.handshake_successes_total > report.handshake_attempts_total
        || report.latency_microseconds_total
            > report
                .latency_samples_total
                .saturating_mul(MAX_LATENCY_MICROSECONDS)
    {
        return Err(ApiError::validation());
    }
    Ok(())
}

fn validate_report_peers(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    report: &AgentTelemetryReport,
) -> Result<PeerTelemetryTotals, ApiError> {
    let relay_ids = state
        .relays
        .iter()
        .filter_map(|relay| decode_array::<16>(&relay.relay_id_base64))
        .collect::<HashSet<_>>();
    let mut peer_ids = HashSet::with_capacity(report.peers.len());
    let mut totals = PeerTelemetryTotals::default();
    for peer in &report.peers {
        let peer_id =
            decode_array::<16>(&peer.peer_node_id_base64).ok_or_else(ApiError::validation)?;
        let relay_id = peer
            .relay_id_base64
            .as_deref()
            .map(|value| decode_array::<16>(value).ok_or_else(ApiError::validation))
            .transpose()?;
        let path_valid = match peer.path {
            AgentPathKind::Disconnected => !peer.session_established && relay_id.is_none(),
            AgentPathKind::Direct => peer.session_established && relay_id.is_none(),
            AgentPathKind::Relay => {
                peer.session_established && relay_id.is_some_and(|id| relay_ids.contains(&id))
            }
        };
        if !path_valid
            || peer_id == authenticated.node_id
            || !peer_ids.insert(peer_id)
            || peer.handshake_successes_total > peer.handshake_attempts_total
            || peer.last_latency_microseconds > Some(MAX_LATENCY_MICROSECONDS)
            || peer.latency_microseconds_total
                > peer
                    .latency_samples_total
                    .saturating_mul(MAX_LATENCY_MICROSECONDS)
        {
            return Err(ApiError::validation());
        }
        checked_accumulate(&mut totals.tx_bytes, peer.tx_bytes_total)?;
        checked_accumulate(&mut totals.rx_bytes, peer.rx_bytes_total)?;
        checked_accumulate(
            &mut totals.handshake_attempts,
            peer.handshake_attempts_total,
        )?;
        checked_accumulate(
            &mut totals.handshake_successes,
            peer.handshake_successes_total,
        )?;
        checked_accumulate(&mut totals.latency_samples, peer.latency_samples_total)?;
        checked_accumulate(
            &mut totals.latency_microseconds,
            peer.latency_microseconds_total,
        )?;
    }
    Ok(totals)
}

const fn report_contains_peer_totals(
    report: &AgentTelemetryReport,
    peers: &PeerTelemetryTotals,
) -> bool {
    report.tx_bytes_total >= peers.tx_bytes
        && report.rx_bytes_total >= peers.rx_bytes
        && report.handshake_attempts_total >= peers.handshake_attempts
        && report.handshake_successes_total >= peers.handshake_successes
        && report.latency_samples_total >= peers.latency_samples
        && report.latency_microseconds_total >= peers.latency_microseconds
}

fn checked_accumulate(total: &mut u64, value: u64) -> Result<(), ApiError> {
    *total = total.checked_add(value).ok_or_else(ApiError::validation)?;
    Ok(())
}

fn totals_decreased(row: &sqlx::postgres::PgRow, totals: TelemetryTotals) -> bool {
    [
        ("tx_bytes_total", totals.tx_bytes),
        ("rx_bytes_total", totals.rx_bytes),
        ("handshake_attempts_total", totals.handshake_attempts),
        ("handshake_successes_total", totals.handshake_successes),
        ("acl_drops_total", totals.acl_drops),
        ("replay_drops_total", totals.replay_drops),
        ("latency_samples_total", totals.latency_samples),
        ("latency_microseconds_total", totals.latency_microseconds),
    ]
    .into_iter()
    .any(|(column, current)| {
        row.try_get::<i64, _>(column)
            .map_or(true, |old| current < old)
    })
}

fn peer_totals_monotonic(previous: &AgentTelemetryReport, current: &AgentTelemetryReport) -> bool {
    let current_by_peer = current
        .peers
        .iter()
        .map(|peer| (peer.peer_node_id_base64.as_str(), peer))
        .collect::<HashMap<_, _>>();
    previous.peers.iter().all(|old| {
        current_by_peer
            .get(old.peer_node_id_base64.as_str())
            .is_none_or(|new| peer_totals_not_decreased(old, new))
    })
}

const fn peer_totals_not_decreased(
    previous: &AgentPeerTelemetry,
    current: &AgentPeerTelemetry,
) -> bool {
    current.tx_packets_total >= previous.tx_packets_total
        && current.tx_bytes_total >= previous.tx_bytes_total
        && current.rx_packets_total >= previous.rx_packets_total
        && current.rx_bytes_total >= previous.rx_bytes_total
        && current.handshake_attempts_total >= previous.handshake_attempts_total
        && current.handshake_successes_total >= previous.handshake_successes_total
        && current.latency_samples_total >= previous.latency_samples_total
        && current.latency_microseconds_total >= previous.latency_microseconds_total
}

fn to_i64(value: u64) -> Result<i64, ApiError> {
    i64::try_from(value).map_err(|_| ApiError::validation())
}

fn decode_array<const N: usize>(encoded: &str) -> Option<[u8; N]> {
    URL_SAFE_NO_PAD.decode(encoded).ok()?.try_into().ok()
}

fn database_error(_error: sqlx::Error) -> ApiError {
    ApiError::internal()
}
