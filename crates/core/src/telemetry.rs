use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const AGENT_TELEMETRY_DOMAIN: &[u8] = b"XS Nexus agent telemetry report v1";
const RELAY_TELEMETRY_DOMAIN: &[u8] = b"XS Nexus relay telemetry report v1";

pub const MAX_AGENT_TELEMETRY_PEERS: usize = 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPathKind {
    Disconnected,
    Direct,
    Relay,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPeerTelemetry {
    pub peer_node_id_base64: String,
    pub path: AgentPathKind,
    pub relay_id_base64: Option<String>,
    pub session_established: bool,
    pub last_latency_microseconds: Option<u64>,
    pub tx_packets_total: u64,
    pub tx_bytes_total: u64,
    pub rx_packets_total: u64,
    pub rx_bytes_total: u64,
    pub handshake_attempts_total: u64,
    pub handshake_successes_total: u64,
    pub latency_samples_total: u64,
    pub latency_microseconds_total: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTelemetryReport {
    pub schema_version: u8,
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub boot_id_base64: String,
    pub sequence: u64,
    pub generated_at: DateTime<Utc>,
    pub tx_bytes_total: u64,
    pub rx_bytes_total: u64,
    pub handshake_attempts_total: u64,
    pub handshake_successes_total: u64,
    #[serde(default)]
    pub acl_drops_total: u64,
    #[serde(default)]
    pub replay_drops_total: u64,
    pub latency_samples_total: u64,
    pub latency_microseconds_total: u64,
    pub peers: Vec<AgentPeerTelemetry>,
}

#[derive(Serialize)]
struct AgentTelemetryReportV1<'a> {
    schema_version: u8,
    network_id: &'a Uuid,
    node_id_base64: &'a str,
    boot_id_base64: &'a str,
    sequence: u64,
    generated_at: &'a DateTime<Utc>,
    tx_bytes_total: u64,
    rx_bytes_total: u64,
    handshake_attempts_total: u64,
    handshake_successes_total: u64,
    latency_samples_total: u64,
    latency_microseconds_total: u64,
    peers: &'a [AgentPeerTelemetry],
}

/// Encodes the domain-separated bytes signed by an Agent telemetry report.
///
/// # Errors
///
/// Returns [`serde_json::Error`] if the bounded report cannot be serialized.
pub fn agent_telemetry_report_signing_input(
    report: &AgentTelemetryReport,
) -> Result<Vec<u8>, serde_json::Error> {
    let payload = if report.schema_version == 1 {
        serde_json::to_vec(&AgentTelemetryReportV1 {
            schema_version: report.schema_version,
            network_id: &report.network_id,
            node_id_base64: &report.node_id_base64,
            boot_id_base64: &report.boot_id_base64,
            sequence: report.sequence,
            generated_at: &report.generated_at,
            tx_bytes_total: report.tx_bytes_total,
            rx_bytes_total: report.rx_bytes_total,
            handshake_attempts_total: report.handshake_attempts_total,
            handshake_successes_total: report.handshake_successes_total,
            latency_samples_total: report.latency_samples_total,
            latency_microseconds_total: report.latency_microseconds_total,
            peers: &report.peers,
        })?
    } else {
        serde_json::to_vec(report)?
    };
    let mut signing_input = Vec::with_capacity(AGENT_TELEMETRY_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(AGENT_TELEMETRY_DOMAIN);
    signing_input.extend_from_slice(&payload);
    Ok(signing_input)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayTelemetryMetrics {
    pub active_leases: u64,
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
    pub forwarding_latency_microseconds_total: u64,
    pub forwarding_latency_microseconds_max: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayTelemetryReport {
    pub schema_version: u8,
    pub relay_id_base64: String,
    pub boot_id_base64: String,
    pub sequence: u64,
    pub generated_at: DateTime<Utc>,
    pub metrics: RelayTelemetryMetrics,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRelayTelemetryReport {
    pub report: RelayTelemetryReport,
    pub signature_base64: String,
}

/// Encodes the domain-separated bytes signed by a Relay telemetry report.
///
/// # Errors
///
/// Returns [`serde_json::Error`] if the bounded report cannot be serialized.
pub fn relay_telemetry_report_signing_input(
    report: &RelayTelemetryReport,
) -> Result<Vec<u8>, serde_json::Error> {
    let payload = serde_json::to_vec(report)?;
    let mut signing_input = Vec::with_capacity(RELAY_TELEMETRY_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(RELAY_TELEMETRY_DOMAIN);
    signing_input.extend_from_slice(&payload);
    Ok(signing_input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn report() -> AgentTelemetryReport {
        AgentTelemetryReport {
            schema_version: 2,
            network_id: Uuid::from_u128(1),
            node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ".to_owned(),
            boot_id_base64: "AgICAgICAgICAgICAgICAg".to_owned(),
            sequence: 7,
            generated_at: Utc.timestamp_opt(1_700_000_000, 0).single().expect("time"),
            tx_bytes_total: 300,
            rx_bytes_total: 200,
            handshake_attempts_total: 1,
            handshake_successes_total: 1,
            acl_drops_total: 2,
            replay_drops_total: 3,
            latency_samples_total: 4,
            latency_microseconds_total: 48_000,
            peers: vec![AgentPeerTelemetry {
                peer_node_id_base64: "AwMDAwMDAwMDAwMDAwMDAw".to_owned(),
                path: AgentPathKind::Direct,
                relay_id_base64: None,
                session_established: true,
                last_latency_microseconds: Some(12_000),
                tx_packets_total: 3,
                tx_bytes_total: 300,
                rx_packets_total: 2,
                rx_bytes_total: 200,
                handshake_attempts_total: 1,
                handshake_successes_total: 1,
                latency_samples_total: 4,
                latency_microseconds_total: 48_000,
            }],
        }
    }

    #[test]
    fn signing_input_is_domain_separated_and_deterministic() {
        let report = report();
        let first = agent_telemetry_report_signing_input(&report).expect("signing input");
        let second = agent_telemetry_report_signing_input(&report).expect("signing input");
        assert_eq!(first, second);
        assert!(first.starts_with(AGENT_TELEMETRY_DOMAIN));
        assert_eq!(
            &first[AGENT_TELEMETRY_DOMAIN.len()..],
            serde_json::to_vec(&report).expect("json")
        );
    }

    #[test]
    fn report_rejects_unknown_fields() {
        let mut value = serde_json::to_value(report()).expect("json");
        value
            .as_object_mut()
            .expect("object")
            .insert("unexpected".to_owned(), serde_json::json!(true));
        assert!(serde_json::from_value::<AgentTelemetryReport>(value).is_err());
    }

    #[test]
    fn version_one_defaults_new_counters_and_preserves_legacy_signing_shape() {
        let mut value = serde_json::to_value(report()).expect("json");
        let object = value.as_object_mut().expect("object");
        object.insert("schema_version".to_owned(), serde_json::json!(1));
        object.remove("acl_drops_total");
        object.remove("replay_drops_total");
        let legacy: AgentTelemetryReport = serde_json::from_value(value).expect("legacy report");
        assert_eq!(legacy.acl_drops_total, 0);
        assert_eq!(legacy.replay_drops_total, 0);

        let signing_input = agent_telemetry_report_signing_input(&legacy).expect("signing input");
        let payload = &signing_input[AGENT_TELEMETRY_DOMAIN.len()..];
        let payload = std::str::from_utf8(payload).expect("UTF-8 JSON");
        assert!(!payload.contains("acl_drops_total"));
        assert!(!payload.contains("replay_drops_total"));
    }

    #[test]
    fn relay_signing_input_is_domain_separated() {
        let report = RelayTelemetryReport {
            schema_version: 1,
            relay_id_base64: "BAQEBAQEBAQEBAQEBAQEBA".to_owned(),
            boot_id_base64: "BQUFBQUFBQUFBQUFBQUFBQ".to_owned(),
            sequence: 9,
            generated_at: Utc.timestamp_opt(1_700_000_000, 0).single().expect("time"),
            metrics: RelayTelemetryMetrics {
                active_leases: 2,
                packets_received: 10,
                bytes_received: 1_000,
                registrations_accepted: 2,
                registration_retries: 0,
                registrations_rejected: 1,
                packets_forwarded: 8,
                bytes_forwarded: 800,
                keepalives_accepted: 4,
                invalid_drops: 1,
                authentication_drops: 0,
                replay_drops: 0,
                rate_limit_drops: 0,
                queue_drops: 0,
                destination_drops: 0,
                send_drops: 0,
                packets_dropped: 1,
                io_errors: 0,
                forwarding_latency_samples: 8,
                forwarding_latency_microseconds_total: 400,
                forwarding_latency_microseconds_max: 80,
            },
        };
        let input = relay_telemetry_report_signing_input(&report).expect("signing input");
        assert!(input.starts_with(RELAY_TELEMETRY_DOMAIN));
    }
}
