use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Serialize;
use sqlx::Row as _;
use xs_core::{
    RelayTelemetryMetrics, RelayTelemetryReport, SignedRelayTelemetryReport,
    relay_telemetry_report_signing_input,
};

use crate::{error::ApiError, state::AppState};

const MAX_REPORT_BYTES: usize = 64 * 1024;
const MAX_LATENCY_MICROSECONDS: u64 = 10 * 60 * 1_000_000;
// The fastest allowed 10-second interval produces 9,000 reports in 25 hours.
const MAX_SAMPLES_PER_RELAY: i64 = 9_000;

#[derive(Serialize)]
pub(crate) struct RelayTelemetryAcknowledgement {
    accepted_sequence: u64,
}

#[derive(Clone, Copy)]
struct StoredMetrics {
    packets_received: i64,
    bytes_received: i64,
    packets_forwarded: i64,
    bytes_forwarded: i64,
    packets_dropped: i64,
    io_errors: i64,
    latency_samples: i64,
    latency_microseconds: i64,
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn record(
    state: &AppState,
    signed: SignedRelayTelemetryReport,
) -> Result<RelayTelemetryAcknowledgement, ApiError> {
    let report = signed.report;
    let now = Utc::now();
    let relay_id =
        decode_array::<16>(&report.relay_id_base64).ok_or_else(ApiError::unauthorized)?;
    let boot_id = decode_array::<16>(&report.boot_id_base64).ok_or_else(ApiError::validation)?;
    let relay = state
        .relays
        .iter()
        .find(|relay| relay.relay_id_base64 == report.relay_id_base64)
        .filter(|relay| relay.expires_at > now)
        .ok_or_else(ApiError::unauthorized)?;
    if report.schema_version != 1
        || report.sequence == 0
        || boot_id == [0_u8; 16]
        || report.generated_at < now - Duration::minutes(10)
        || report.generated_at > now + Duration::minutes(5)
    {
        return Err(ApiError::validation());
    }
    validate_metrics(&report.metrics)?;
    let signing_input =
        relay_telemetry_report_signing_input(&report).map_err(|_| ApiError::validation())?;
    if signing_input.len() > MAX_REPORT_BYTES {
        return Err(ApiError::validation());
    }
    let signature =
        decode_array::<64>(&signed.signature_base64).ok_or_else(ApiError::unauthorized)?;
    let public_key =
        decode_array::<32>(&relay.identity_public_key_base64).ok_or_else(ApiError::internal)?;
    VerifyingKey::from_bytes(&public_key)
        .map_err(|_| ApiError::internal())?
        .verify_strict(&signing_input, &Signature::from_bytes(&signature))
        .map_err(|_| ApiError::unauthorized())?;
    let metrics = stored_metrics(&report.metrics)?;
    let report_json = serde_json::to_value(&report).map_err(|_| ApiError::validation())?;

    let mut transaction = state.pool.begin().await.map_err(database_error)?;
    let previous = sqlx::query(
        "SELECT boot_id, sequence, report, generated_at
         FROM relay_telemetry_reports WHERE relay_id = $1 FOR UPDATE",
    )
    .bind(relay_id.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?;
    if let Some(previous) = previous {
        let previous_generated_at = previous
            .try_get::<chrono::DateTime<Utc>, _>("generated_at")
            .map_err(database_error)?;
        if report.generated_at <= previous_generated_at {
            return Err(ApiError::conflict());
        }
        let previous_boot_id = previous
            .try_get::<Vec<u8>, _>("boot_id")
            .map_err(database_error)?;
        if previous_boot_id.as_slice() == boot_id {
            let previous_sequence = previous
                .try_get::<i64, _>("sequence")
                .map_err(database_error)?;
            let previous_report: RelayTelemetryReport = serde_json::from_value(
                previous
                    .try_get::<serde_json::Value, _>("report")
                    .map_err(database_error)?,
            )
            .map_err(|_| ApiError::internal())?;
            if i64::try_from(report.sequence).map_err(|_| ApiError::validation())?
                <= previous_sequence
                || !metrics_monotonic(&previous_report.metrics, &report.metrics)
            {
                return Err(ApiError::conflict());
            }
        }
    }

    insert_sample(
        &mut transaction,
        relay_id,
        boot_id,
        report.sequence,
        metrics,
        report.generated_at,
    )
    .await?;
    upsert_latest(
        &mut transaction,
        relay_id,
        boot_id,
        report.sequence,
        report_json,
        metrics,
        report.generated_at,
    )
    .await?;
    prune_samples(&mut transaction, relay_id).await?;
    transaction.commit().await.map_err(database_error)?;
    Ok(RelayTelemetryAcknowledgement {
        accepted_sequence: report.sequence,
    })
}

#[allow(clippy::too_many_arguments)]
async fn insert_sample(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    relay_id: [u8; 16],
    boot_id: [u8; 16],
    sequence: u64,
    metrics: StoredMetrics,
    generated_at: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO relay_telemetry_samples
         (relay_id, boot_id, sequence, packets_received, bytes_received,
          packets_forwarded, bytes_forwarded, packets_dropped, io_errors,
          forwarding_latency_samples, forwarding_latency_microseconds_total, generated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(relay_id.as_slice())
    .bind(boot_id.as_slice())
    .bind(i64::try_from(sequence).map_err(|_| ApiError::validation())?)
    .bind(metrics.packets_received)
    .bind(metrics.bytes_received)
    .bind(metrics.packets_forwarded)
    .bind(metrics.bytes_forwarded)
    .bind(metrics.packets_dropped)
    .bind(metrics.io_errors)
    .bind(metrics.latency_samples)
    .bind(metrics.latency_microseconds)
    .bind(generated_at)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApiError::conflict())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_latest(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    relay_id: [u8; 16],
    boot_id: [u8; 16],
    sequence: u64,
    report_json: serde_json::Value,
    metrics: StoredMetrics,
    generated_at: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO relay_telemetry_reports
         (relay_id, boot_id, sequence, report, packets_received, bytes_received,
          packets_forwarded, bytes_forwarded, packets_dropped, io_errors,
          forwarding_latency_samples, forwarding_latency_microseconds_total,
          generated_at, received_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())
         ON CONFLICT (relay_id) DO UPDATE SET
           boot_id = EXCLUDED.boot_id, sequence = EXCLUDED.sequence,
           report = EXCLUDED.report, packets_received = EXCLUDED.packets_received,
           bytes_received = EXCLUDED.bytes_received,
           packets_forwarded = EXCLUDED.packets_forwarded,
           bytes_forwarded = EXCLUDED.bytes_forwarded,
           packets_dropped = EXCLUDED.packets_dropped,
           io_errors = EXCLUDED.io_errors,
           forwarding_latency_samples = EXCLUDED.forwarding_latency_samples,
           forwarding_latency_microseconds_total = EXCLUDED.forwarding_latency_microseconds_total,
           generated_at = EXCLUDED.generated_at, received_at = now()",
    )
    .bind(relay_id.as_slice())
    .bind(boot_id.as_slice())
    .bind(i64::try_from(sequence).map_err(|_| ApiError::validation())?)
    .bind(report_json)
    .bind(metrics.packets_received)
    .bind(metrics.bytes_received)
    .bind(metrics.packets_forwarded)
    .bind(metrics.bytes_forwarded)
    .bind(metrics.packets_dropped)
    .bind(metrics.io_errors)
    .bind(metrics.latency_samples)
    .bind(metrics.latency_microseconds)
    .bind(generated_at)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn prune_samples(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    relay_id: [u8; 16],
) -> Result<(), ApiError> {
    sqlx::query(
        "DELETE FROM relay_telemetry_samples
         WHERE relay_id = $1 AND received_at < now() - interval '25 hours'",
    )
    .bind(relay_id.as_slice())
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "DELETE FROM relay_telemetry_samples
         WHERE (relay_id, boot_id, sequence) IN (
           SELECT relay_id, boot_id, sequence FROM relay_telemetry_samples
           WHERE relay_id = $1 ORDER BY received_at DESC, sequence DESC OFFSET $2
         )",
    )
    .bind(relay_id.as_slice())
    .bind(MAX_SAMPLES_PER_RELAY)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

fn validate_metrics(metrics: &RelayTelemetryMetrics) -> Result<(), ApiError> {
    let drops = metrics
        .invalid_drops
        .checked_add(metrics.authentication_drops)
        .and_then(|value| value.checked_add(metrics.replay_drops))
        .and_then(|value| value.checked_add(metrics.rate_limit_drops))
        .and_then(|value| value.checked_add(metrics.queue_drops))
        .and_then(|value| value.checked_add(metrics.destination_drops))
        .and_then(|value| value.checked_add(metrics.send_drops))
        .ok_or_else(ApiError::validation)?;
    if metrics.active_leases > 65_536
        || metrics.packets_dropped != drops
        || metrics.forwarding_latency_microseconds_max > MAX_LATENCY_MICROSECONDS
        || metrics.forwarding_latency_microseconds_total
            > metrics
                .forwarding_latency_samples
                .saturating_mul(MAX_LATENCY_MICROSECONDS)
    {
        return Err(ApiError::validation());
    }
    Ok(())
}

fn stored_metrics(metrics: &RelayTelemetryMetrics) -> Result<StoredMetrics, ApiError> {
    Ok(StoredMetrics {
        packets_received: to_i64(metrics.packets_received)?,
        bytes_received: to_i64(metrics.bytes_received)?,
        packets_forwarded: to_i64(metrics.packets_forwarded)?,
        bytes_forwarded: to_i64(metrics.bytes_forwarded)?,
        packets_dropped: to_i64(metrics.packets_dropped)?,
        io_errors: to_i64(metrics.io_errors)?,
        latency_samples: to_i64(metrics.forwarding_latency_samples)?,
        latency_microseconds: to_i64(metrics.forwarding_latency_microseconds_total)?,
    })
}

fn metrics_monotonic(previous: &RelayTelemetryMetrics, current: &RelayTelemetryMetrics) -> bool {
    current.packets_received >= previous.packets_received
        && current.bytes_received >= previous.bytes_received
        && current.registrations_accepted >= previous.registrations_accepted
        && current.registration_retries >= previous.registration_retries
        && current.registrations_rejected >= previous.registrations_rejected
        && current.packets_forwarded >= previous.packets_forwarded
        && current.bytes_forwarded >= previous.bytes_forwarded
        && current.keepalives_accepted >= previous.keepalives_accepted
        && current.invalid_drops >= previous.invalid_drops
        && current.authentication_drops >= previous.authentication_drops
        && current.replay_drops >= previous.replay_drops
        && current.rate_limit_drops >= previous.rate_limit_drops
        && current.queue_drops >= previous.queue_drops
        && current.destination_drops >= previous.destination_drops
        && current.send_drops >= previous.send_drops
        && current.packets_dropped >= previous.packets_dropped
        && current.io_errors >= previous.io_errors
        && current.forwarding_latency_samples >= previous.forwarding_latency_samples
        && current.forwarding_latency_microseconds_total
            >= previous.forwarding_latency_microseconds_total
        && current.forwarding_latency_microseconds_max
            >= previous.forwarding_latency_microseconds_max
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
