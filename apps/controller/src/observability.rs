use std::collections::HashSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::FromRow;
use xs_core::{AgentPathKind, AgentTelemetryReport, BuildIdentity, Component};

use crate::{error::ApiError, state::AppState};

const AGENT_FRESHNESS_MINUTES: i64 = 3;
const RELAY_FRESHNESS_MINUTES: i64 = 2;

#[derive(Serialize)]
pub(crate) struct ObservabilitySnapshot {
    schema_version: u8,
    collected_at: DateTime<Utc>,
    controller: ControllerSummary,
    database: DependencySummary,
    redis: DependencySummary,
    nodes: NodeSummary,
    relays: RelaySummary,
    security: SecuritySummary,
    routing: RoutingSummary,
    updates: UpdateSummary,
    audit: AuditSummary,
}

#[derive(Serialize)]
struct ControllerSummary {
    status: &'static str,
    build: BuildIdentity,
    control_auth_failures_since_start: u64,
}

#[derive(Serialize)]
struct DependencySummary {
    status: &'static str,
    reason: Option<&'static str>,
}

#[derive(Serialize)]
struct NodeSummary {
    managed: u64,
    online: u64,
    fresh_telemetry: u64,
    unknown_path_nodes: u64,
    disconnected_nodes: u64,
    path_telemetry_complete: bool,
    path_observations: u64,
    direct_path_observations: u64,
    relay_path_observations: u64,
    direct_ratio_basis_points: Option<u16>,
    relay_ratio_basis_points: Option<u16>,
    telemetry_nodes_24h: u64,
    traffic_bytes_24h: u64,
    handshake_attempts_24h: u64,
    handshake_failures_24h: u64,
    acl_drops_24h: u64,
    replay_drops_24h: u64,
}

#[derive(Serialize)]
struct RelaySummary {
    configured: u64,
    unexpired_configured: u64,
    fresh: u64,
    telemetry_complete: bool,
    telemetry_relays_24h: u64,
    packets_received_24h: u64,
    packets_forwarded_24h: u64,
    registration_retries_24h: u64,
    registrations_rejected_24h: u64,
    invalid_drops_24h: u64,
    authentication_drops_24h: u64,
    replay_drops_24h: u64,
    rate_limit_drops_24h: u64,
    queue_drops_24h: u64,
    destination_drops_24h: u64,
    send_drops_24h: u64,
    packets_dropped_24h: u64,
    io_errors_24h: u64,
}

#[derive(Serialize)]
struct SecuritySummary {
    management_auth_failures_24h: u64,
    control_auth_failures_since_start: u64,
    acl_drops_24h: u64,
    replay_drops_24h: u64,
}

#[derive(Serialize)]
struct RoutingSummary {
    enabled_routes: u64,
    successful_changes_24h: u64,
}

#[derive(Serialize)]
struct UpdateSummary {
    failed_nodes: u64,
}

#[derive(Serialize)]
struct AuditSummary {
    events_24h: u64,
    total_events: u64,
    oldest_event_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct NodeLatestRow {
    node_id: Vec<u8>,
    report: serde_json::Value,
    generated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct NodeWindowRow {
    telemetry_nodes: i64,
    tx_bytes: i64,
    rx_bytes: i64,
    handshake_attempts: i64,
    handshake_successes: i64,
    acl_drops: i64,
    replay_drops: i64,
}

#[derive(FromRow)]
struct RelayLatestRow {
    relay_id: Vec<u8>,
    generated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct RelayWindowRow {
    relay_id: Vec<u8>,
    packets_received: i64,
    packets_forwarded: i64,
    registration_retries: i64,
    registrations_rejected: i64,
    invalid_drops: i64,
    authentication_drops: i64,
    replay_drops: i64,
    rate_limit_drops: i64,
    queue_drops: i64,
    destination_drops: i64,
    send_drops: i64,
    packets_dropped: i64,
    io_errors: i64,
}

#[derive(FromRow)]
struct AuditRow {
    events_24h: i64,
    total_events: i64,
    oldest_event_at: Option<DateTime<Utc>>,
    management_auth_failures_24h: i64,
    route_changes_24h: i64,
}

#[derive(FromRow)]
struct OperationalRow {
    enabled_routes: i64,
    failed_updates: i64,
}

pub(crate) async fn snapshot(state: &AppState) -> Result<ObservabilitySnapshot, ApiError> {
    let collected_at = Utc::now();
    let active_node_ids = load_active_node_ids(state).await?;
    let latest_nodes = load_latest_nodes(state).await?;
    let node_window = load_node_window(state).await?;
    let latest_relays = load_latest_relays(state).await?;
    let relay_window = load_relay_window(state).await?;
    let audit = load_audit(state).await?;
    let operational = load_operational(state).await?;

    let active_nodes = active_node_ids
        .iter()
        .map(|node_id| decode_id(node_id))
        .collect::<Result<HashSet<_>, _>>()?;
    let online_nodes = state
        .online_node_ids()
        .await
        .into_iter()
        .filter(|node_id| active_nodes.contains(node_id))
        .count();
    let mut fresh_telemetry = 0_u64;
    let mut disconnected_nodes = 0_u64;
    let mut direct_paths = 0_u64;
    let mut relay_paths = 0_u64;
    for row in latest_nodes {
        let node_id = decode_id(&row.node_id)?;
        if !active_nodes.contains(&node_id)
            || row.generated_at < collected_at - Duration::minutes(AGENT_FRESHNESS_MINUTES)
        {
            continue;
        }
        let report = serde_json::from_value::<AgentTelemetryReport>(row.report)
            .map_err(|_| ApiError::internal())?;
        fresh_telemetry = fresh_telemetry.saturating_add(1);
        let mut established = false;
        for peer in report
            .peers
            .into_iter()
            .filter(|peer| peer.session_established)
        {
            match peer.path {
                AgentPathKind::Direct => {
                    established = true;
                    direct_paths = direct_paths.saturating_add(1);
                }
                AgentPathKind::Relay => {
                    established = true;
                    relay_paths = relay_paths.saturating_add(1);
                }
                AgentPathKind::Disconnected => {}
            }
        }
        if !established {
            disconnected_nodes = disconnected_nodes.saturating_add(1);
        }
    }
    let managed = usize_u64(active_nodes.len())?;
    let path_observations = direct_paths
        .checked_add(relay_paths)
        .ok_or_else(ApiError::internal)?;
    let traffic_bytes = nonnegative(node_window.tx_bytes)?
        .checked_add(nonnegative(node_window.rx_bytes)?)
        .ok_or_else(ApiError::internal)?;
    let handshake_attempts = nonnegative(node_window.handshake_attempts)?;
    let handshake_successes = nonnegative(node_window.handshake_successes)?;
    let handshake_failures = handshake_attempts
        .checked_sub(handshake_successes)
        .ok_or_else(ApiError::internal)?;
    let node_acl_drops = nonnegative(node_window.acl_drops)?;
    let node_replay_drops = nonnegative(node_window.replay_drops)?;
    let node_summary = NodeSummary {
        managed,
        online: usize_u64(online_nodes)?,
        fresh_telemetry,
        unknown_path_nodes: managed.saturating_sub(fresh_telemetry),
        disconnected_nodes,
        path_telemetry_complete: fresh_telemetry == managed,
        path_observations,
        direct_path_observations: direct_paths,
        relay_path_observations: relay_paths,
        direct_ratio_basis_points: ratio_basis_points(direct_paths, path_observations)?,
        relay_ratio_basis_points: ratio_basis_points(relay_paths, path_observations)?,
        telemetry_nodes_24h: nonnegative(node_window.telemetry_nodes)?,
        traffic_bytes_24h: traffic_bytes,
        handshake_attempts_24h: handshake_attempts,
        handshake_failures_24h: handshake_failures,
        acl_drops_24h: node_acl_drops,
        replay_drops_24h: node_replay_drops,
    };

    let configured_relays = relay_ids(state.relays.iter().map(|relay| &relay.relay_id_base64))?;
    let unexpired_relays = relay_ids(
        state
            .relays
            .iter()
            .filter(|relay| relay.expires_at > collected_at)
            .map(|relay| &relay.relay_id_base64),
    )?;
    let mut fresh_relays = 0_usize;
    for row in &latest_relays {
        let relay_id = decode_id(&row.relay_id)?;
        if unexpired_relays.contains(&relay_id)
            && row.generated_at >= collected_at - Duration::minutes(RELAY_FRESHNESS_MINUTES)
        {
            fresh_relays = fresh_relays.saturating_add(1);
        }
    }
    let relay_totals = relay_totals(&relay_window, &unexpired_relays)?;
    let relay_summary = RelaySummary {
        configured: usize_u64(configured_relays.len())?,
        unexpired_configured: usize_u64(unexpired_relays.len())?,
        fresh: usize_u64(fresh_relays)?,
        telemetry_complete: fresh_relays == unexpired_relays.len(),
        telemetry_relays_24h: relay_totals.telemetry_relays,
        packets_received_24h: relay_totals.packets_received,
        packets_forwarded_24h: relay_totals.packets_forwarded,
        registration_retries_24h: relay_totals.registration_retries,
        registrations_rejected_24h: relay_totals.registrations_rejected,
        invalid_drops_24h: relay_totals.invalid_drops,
        authentication_drops_24h: relay_totals.authentication_drops,
        replay_drops_24h: relay_totals.replay_drops,
        rate_limit_drops_24h: relay_totals.rate_limit_drops,
        queue_drops_24h: relay_totals.queue_drops,
        destination_drops_24h: relay_totals.destination_drops,
        send_drops_24h: relay_totals.send_drops,
        packets_dropped_24h: relay_totals.packets_dropped,
        io_errors_24h: relay_totals.io_errors,
    };

    let control_auth_failures = state.control_auth_failures_total();
    let security = SecuritySummary {
        management_auth_failures_24h: nonnegative(audit.management_auth_failures_24h)?,
        control_auth_failures_since_start: control_auth_failures,
        acl_drops_24h: node_acl_drops,
        replay_drops_24h: node_replay_drops
            .checked_add(relay_totals.replay_drops)
            .ok_or_else(ApiError::internal)?,
    };
    Ok(ObservabilitySnapshot {
        schema_version: 1,
        collected_at,
        controller: ControllerSummary {
            status: "ok",
            build: BuildIdentity::current(Component::Controller),
            control_auth_failures_since_start: control_auth_failures,
        },
        database: DependencySummary {
            status: "ok",
            reason: None,
        },
        redis: DependencySummary {
            status: "not_applicable",
            reason: Some("controller_has_no_redis_dependency"),
        },
        nodes: node_summary,
        relays: relay_summary,
        security,
        routing: RoutingSummary {
            enabled_routes: nonnegative(operational.enabled_routes)?,
            successful_changes_24h: nonnegative(audit.route_changes_24h)?,
        },
        updates: UpdateSummary {
            failed_nodes: nonnegative(operational.failed_updates)?,
        },
        audit: AuditSummary {
            events_24h: nonnegative(audit.events_24h)?,
            total_events: nonnegative(audit.total_events)?,
            oldest_event_at: audit.oldest_event_at,
        },
    })
}

async fn load_active_node_ids(state: &AppState) -> Result<Vec<Vec<u8>>, ApiError> {
    sqlx::query_scalar("SELECT node_id FROM nodes WHERE revoked_at IS NULL ORDER BY node_id")
        .fetch_all(&state.pool)
        .await
        .map_err(database_error)
}

async fn load_latest_nodes(state: &AppState) -> Result<Vec<NodeLatestRow>, ApiError> {
    sqlx::query_as(
        "SELECT t.node_id, t.report, t.generated_at
         FROM node_telemetry_reports t
         JOIN nodes n ON n.node_id = t.node_id
         WHERE n.revoked_at IS NULL",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_node_window(state: &AppState) -> Result<NodeWindowRow, ApiError> {
    sqlx::query_as(
        "WITH per_boot AS (
           SELECT s.node_id, s.boot_id,
                  greatest(max(s.tx_bytes_total) - min(s.tx_bytes_total), 0) AS tx_bytes,
                  greatest(max(s.rx_bytes_total) - min(s.rx_bytes_total), 0) AS rx_bytes,
                  greatest(max(s.handshake_attempts_total) - min(s.handshake_attempts_total), 0)
                    AS handshake_attempts,
                  greatest(max(s.handshake_successes_total) - min(s.handshake_successes_total), 0)
                    AS handshake_successes,
                  greatest(max(s.acl_drops_total) - min(s.acl_drops_total), 0) AS acl_drops,
                  greatest(max(s.replay_drops_total) - min(s.replay_drops_total), 0)
                    AS replay_drops
           FROM node_telemetry_samples s
           JOIN nodes n ON n.node_id = s.node_id
           WHERE n.revoked_at IS NULL AND s.received_at >= now() - interval '24 hours'
           GROUP BY s.node_id, s.boot_id
         )
         SELECT count(DISTINCT node_id)::bigint AS telemetry_nodes,
                coalesce(sum(tx_bytes), 0)::bigint AS tx_bytes,
                coalesce(sum(rx_bytes), 0)::bigint AS rx_bytes,
                coalesce(sum(handshake_attempts), 0)::bigint AS handshake_attempts,
                coalesce(sum(handshake_successes), 0)::bigint AS handshake_successes,
                coalesce(sum(acl_drops), 0)::bigint AS acl_drops,
                coalesce(sum(replay_drops), 0)::bigint AS replay_drops
         FROM per_boot",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_latest_relays(state: &AppState) -> Result<Vec<RelayLatestRow>, ApiError> {
    sqlx::query_as("SELECT relay_id, generated_at FROM relay_telemetry_reports")
        .fetch_all(&state.pool)
        .await
        .map_err(database_error)
}

async fn load_relay_window(state: &AppState) -> Result<Vec<RelayWindowRow>, ApiError> {
    sqlx::query_as(
        "WITH per_boot AS (
           SELECT relay_id, boot_id,
                  greatest(max(packets_received) - min(packets_received), 0)
                    AS packets_received,
                  greatest(max(packets_forwarded) - min(packets_forwarded), 0)
                    AS packets_forwarded,
                  greatest(max(registration_retries) - min(registration_retries), 0)
                    AS registration_retries,
                  greatest(max(registrations_rejected) - min(registrations_rejected), 0)
                    AS registrations_rejected,
                  greatest(max(invalid_drops) - min(invalid_drops), 0) AS invalid_drops,
                  greatest(max(authentication_drops) - min(authentication_drops), 0)
                    AS authentication_drops,
                  greatest(max(replay_drops) - min(replay_drops), 0) AS replay_drops,
                  greatest(max(rate_limit_drops) - min(rate_limit_drops), 0)
                    AS rate_limit_drops,
                  greatest(max(queue_drops) - min(queue_drops), 0) AS queue_drops,
                  greatest(max(destination_drops) - min(destination_drops), 0)
                    AS destination_drops,
                  greatest(max(send_drops) - min(send_drops), 0) AS send_drops,
                  greatest(max(packets_dropped) - min(packets_dropped), 0)
                    AS packets_dropped,
                  greatest(max(io_errors) - min(io_errors), 0) AS io_errors
           FROM relay_telemetry_samples
           WHERE received_at >= now() - interval '24 hours'
           GROUP BY relay_id, boot_id
         )
         SELECT relay_id,
                sum(packets_received)::bigint AS packets_received,
                sum(packets_forwarded)::bigint AS packets_forwarded,
                sum(registration_retries)::bigint AS registration_retries,
                sum(registrations_rejected)::bigint AS registrations_rejected,
                sum(invalid_drops)::bigint AS invalid_drops,
                sum(authentication_drops)::bigint AS authentication_drops,
                sum(replay_drops)::bigint AS replay_drops,
                sum(rate_limit_drops)::bigint AS rate_limit_drops,
                sum(queue_drops)::bigint AS queue_drops,
                sum(destination_drops)::bigint AS destination_drops,
                sum(send_drops)::bigint AS send_drops,
                sum(packets_dropped)::bigint AS packets_dropped,
                sum(io_errors)::bigint AS io_errors
         FROM per_boot GROUP BY relay_id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_audit(state: &AppState) -> Result<AuditRow, ApiError> {
    sqlx::query_as(
        "SELECT count(*) FILTER (WHERE occurred_at >= now() - interval '24 hours')::bigint
                  AS events_24h,
                count(*)::bigint AS total_events,
                min(occurred_at) AS oldest_event_at,
                count(*) FILTER (
                  WHERE occurred_at >= now() - interval '24 hours'
                    AND action = 'auth.login' AND outcome != 'success'
                )::bigint AS management_auth_failures_24h,
                count(*) FILTER (
                  WHERE occurred_at >= now() - interval '24 hours'
                    AND action = 'subnet_routes.replace' AND outcome = 'success'
                )::bigint AS route_changes_24h
         FROM audit_events",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_operational(state: &AppState) -> Result<OperationalRow, ApiError> {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM subnet_routes WHERE state = 'enabled')::bigint
                  AS enabled_routes,
                (SELECT count(*) FROM node_update_reports WHERE update_state = 'failed')::bigint
                  AS failed_updates",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(database_error)
}

#[derive(Default)]
struct RelayTotals {
    telemetry_relays: u64,
    packets_received: u64,
    packets_forwarded: u64,
    registration_retries: u64,
    registrations_rejected: u64,
    invalid_drops: u64,
    authentication_drops: u64,
    replay_drops: u64,
    rate_limit_drops: u64,
    queue_drops: u64,
    destination_drops: u64,
    send_drops: u64,
    packets_dropped: u64,
    io_errors: u64,
}

fn relay_totals(
    rows: &[RelayWindowRow],
    configured: &HashSet<[u8; 16]>,
) -> Result<RelayTotals, ApiError> {
    let mut totals = RelayTotals::default();
    for row in rows {
        if !configured.contains(&decode_id(&row.relay_id)?) {
            continue;
        }
        totals.telemetry_relays = totals.telemetry_relays.saturating_add(1);
        add(&mut totals.packets_received, row.packets_received)?;
        add(&mut totals.packets_forwarded, row.packets_forwarded)?;
        add(&mut totals.registration_retries, row.registration_retries)?;
        add(
            &mut totals.registrations_rejected,
            row.registrations_rejected,
        )?;
        add(&mut totals.invalid_drops, row.invalid_drops)?;
        add(&mut totals.authentication_drops, row.authentication_drops)?;
        add(&mut totals.replay_drops, row.replay_drops)?;
        add(&mut totals.rate_limit_drops, row.rate_limit_drops)?;
        add(&mut totals.queue_drops, row.queue_drops)?;
        add(&mut totals.destination_drops, row.destination_drops)?;
        add(&mut totals.send_drops, row.send_drops)?;
        add(&mut totals.packets_dropped, row.packets_dropped)?;
        add(&mut totals.io_errors, row.io_errors)?;
    }
    Ok(totals)
}

fn relay_ids<'a>(values: impl Iterator<Item = &'a String>) -> Result<HashSet<[u8; 16]>, ApiError> {
    values
        .map(|value| {
            URL_SAFE_NO_PAD
                .decode(value)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(ApiError::internal)
        })
        .collect()
}

fn add(total: &mut u64, value: i64) -> Result<(), ApiError> {
    *total = total
        .checked_add(nonnegative(value)?)
        .ok_or_else(ApiError::internal)?;
    Ok(())
}

fn decode_id(bytes: &[u8]) -> Result<[u8; 16], ApiError> {
    bytes.try_into().map_err(|_| ApiError::internal())
}

fn nonnegative(value: i64) -> Result<u64, ApiError> {
    u64::try_from(value).map_err(|_| ApiError::internal())
}

fn usize_u64(value: usize) -> Result<u64, ApiError> {
    u64::try_from(value).map_err(|_| ApiError::internal())
}

fn ratio_basis_points(numerator: u64, denominator: u64) -> Result<Option<u16>, ApiError> {
    if denominator == 0 {
        return Ok(None);
    }
    let ratio = u128::from(numerator)
        .checked_mul(10_000)
        .ok_or_else(ApiError::internal)?
        / u128::from(denominator);
    Ok(Some(
        u16::try_from(ratio).map_err(|_| ApiError::internal())?,
    ))
}

fn database_error(_error: sqlx::Error) -> ApiError {
    ApiError::internal()
}
