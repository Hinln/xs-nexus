use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::{Signer as _, SigningKey};
use futures_util::StreamExt as _;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::watch;
use xs_core::{
    RelayTelemetryMetrics, RelayTelemetryReport, SignedRelayTelemetryReport,
    relay_telemetry_report_signing_input,
};

use crate::{RelayMetrics, config::RelayConfig};

const MAX_ACK_BYTES: usize = 4096;

pub struct RelayTelemetryReporter {
    client: reqwest::Client,
    url: url::Url,
    relay_id_base64: String,
    boot_id_base64: String,
    signing_key: SigningKey,
    interval: Duration,
    metrics: RelayMetrics,
}

#[derive(Debug, Error)]
pub enum ReporterError {
    #[error("unable to create Relay telemetry client")]
    Client,
    #[error("Relay telemetry sequence exhausted")]
    Sequence,
    #[error("unable to serialize Relay telemetry")]
    Serialization,
    #[error("Controller rejected Relay telemetry")]
    Rejected,
    #[error("Controller Relay telemetry acknowledgement was invalid")]
    Acknowledgement,
    #[error("Controller Relay telemetry response exceeded the bounded size")]
    ResponseTooLarge,
    #[error("Relay telemetry request failed")]
    Request,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TelemetryAcknowledgement {
    accepted_sequence: u64,
}

impl RelayTelemetryReporter {
    /// Builds a bounded signed metrics reporter from the strict Relay configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ReporterError`] when the HTTP client or boot identity cannot be created.
    pub fn new(config: &RelayConfig, metrics: RelayMetrics) -> Result<Self, ReporterError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("xs-relay/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ReporterError::Client)?;
        let mut boot_id = [0_u8; 16];
        getrandom::fill(&mut boot_id).map_err(|_| ReporterError::Client)?;
        if boot_id == [0_u8; 16] {
            boot_id[0] = 1;
        }
        Ok(Self {
            client,
            url: config.controller_metrics_url.clone(),
            relay_id_base64: URL_SAFE_NO_PAD.encode(config.relay_id),
            boot_id_base64: URL_SAFE_NO_PAD.encode(boot_id),
            signing_key: config.identity_key.clone(),
            interval: Duration::from_secs(config.metrics_report_interval_seconds),
            metrics,
        })
    }

    /// Pushes signed cumulative Relay metrics until shutdown.
    ///
    /// # Errors
    ///
    /// Returns [`ReporterError::Sequence`] if the process-local report sequence is exhausted.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<(), ReporterError> {
        let mut sequence = 0_u64;
        let mut interval = tokio::time::interval(self.interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
                _ = interval.tick() => {
                    sequence = sequence.checked_add(1).ok_or(ReporterError::Sequence)?;
                    if let Err(error) = self.send(sequence).await {
                        tracing::warn!(event = "relay_telemetry_report_failed", error = %error);
                    }
                }
            }
        }
    }

    async fn send(&self, sequence: u64) -> Result<(), ReporterError> {
        let snapshot = self.metrics.snapshot();
        let report = RelayTelemetryReport {
            schema_version: 1,
            relay_id_base64: self.relay_id_base64.clone(),
            boot_id_base64: self.boot_id_base64.clone(),
            sequence,
            generated_at: Utc::now(),
            metrics: RelayTelemetryMetrics {
                active_leases: u64::try_from(snapshot.active_leases).unwrap_or(u64::MAX),
                packets_received: snapshot.packets_received,
                bytes_received: snapshot.bytes_received,
                registrations_accepted: snapshot.registrations_accepted,
                registration_retries: snapshot.registration_retries,
                registrations_rejected: snapshot.registrations_rejected,
                packets_forwarded: snapshot.packets_forwarded,
                bytes_forwarded: snapshot.bytes_forwarded,
                keepalives_accepted: snapshot.keepalives_accepted,
                invalid_drops: snapshot.invalid_drops,
                authentication_drops: snapshot.authentication_drops,
                replay_drops: snapshot.replay_drops,
                rate_limit_drops: snapshot.rate_limit_drops,
                queue_drops: snapshot.queue_drops,
                destination_drops: snapshot.destination_drops,
                send_drops: snapshot.send_drops,
                packets_dropped: snapshot.packets_dropped,
                io_errors: snapshot.io_errors,
                forwarding_latency_samples: snapshot.forwarding_latency_samples,
                forwarding_latency_microseconds_total: snapshot
                    .forwarding_latency_microseconds_total,
                forwarding_latency_microseconds_max: snapshot.forwarding_latency_microseconds_max,
            },
        };
        let signing_input = relay_telemetry_report_signing_input(&report)
            .map_err(|_| ReporterError::Serialization)?;
        let signature = self.signing_key.sign(&signing_input);
        let response = self
            .client
            .post(self.url.clone())
            .json(&SignedRelayTelemetryReport {
                report,
                signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
            })
            .send()
            .await
            .map_err(|_| ReporterError::Request)?;
        if !response.status().is_success() {
            return Err(ReporterError::Rejected);
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| ReporterError::Request)?;
            if body.len().saturating_add(chunk.len()) > MAX_ACK_BYTES {
                return Err(ReporterError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let acknowledgement: TelemetryAcknowledgement =
            serde_json::from_slice(&body).map_err(|_| ReporterError::Acknowledgement)?;
        if acknowledgement.accepted_sequence != sequence {
            return Err(ReporterError::Acknowledgement);
        }
        Ok(())
    }
}
