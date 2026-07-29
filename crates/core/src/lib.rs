#![forbid(unsafe_code)]

use std::{
    fmt,
    net::{Ipv4Addr, SocketAddr},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Component {
    Agent,
    Cli,
    Controller,
    Relay,
}

impl Component {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "xs-agent",
            Self::Cli => "xs-cli",
            Self::Controller => "xs-controller",
            Self::Relay => "xs-relay",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaselineReport {
    component: Component,
    version: &'static str,
}

impl BaselineReport {
    #[must_use]
    pub const fn new(component: Component) -> Self {
        Self {
            component,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

impl fmt::Display for BaselineReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} status=baseline-ready version={}",
            self.component.as_str(),
            self.version
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollRequest {
    pub token: String,
    pub name: String,
    pub device_type: String,
    pub identity_public_key_base64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SignedConfiguration {
    pub version: u64,
    pub payload_base64: String,
    pub signature_base64: String,
    pub signer_key_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollResponse {
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub virtual_ip: String,
    pub credential_base64: String,
    pub credential_key_id: u32,
    pub credential_signing_public_key_base64: String,
    pub configuration_signing_public_key_base64: String,
    pub configuration: SignedConfiguration,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationPayload {
    pub schema_version: u8,
    pub network_id: Uuid,
    pub version: u64,
    pub generated_at: DateTime<Utc>,
    pub address_pool: String,
    #[serde(default)]
    pub discovery_endpoints: Vec<SocketAddr>,
    pub nodes: Vec<ConfigurationNode>,
    pub relays: Vec<serde_json::Value>,
    pub policies: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationNode {
    pub node_id_base64: String,
    pub identity_public_key_base64: String,
    pub virtual_ip: String,
    #[serde(default)]
    pub direct_endpoints: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<EndpointCandidate>,
    pub credential_serial: u64,
    pub credential_not_after: DateTime<Utc>,
    pub role_bitmap: u32,
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointCandidateKind {
    Local,
    PublicIpv6,
    Mapped,
    Static,
    Relay,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EndpointCandidate {
    pub kind: EndpointCandidateKind,
    pub endpoint: SocketAddr,
    pub priority: u32,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CandidateAdvertisement {
    pub schema_version: u8,
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub generation: u64,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub candidates: Vec<EndpointCandidate>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalAgentRequest {
    Status {},
    Peers {},
    Diagnostics {},
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalAgentResponse {
    Status {
        schema_version: u8,
        status: LocalAgentStatus,
    },
    Peers {
        schema_version: u8,
        peers: Vec<LocalPeerStatus>,
        total: u64,
        truncated: bool,
    },
    Diagnostics {
        schema_version: u8,
        diagnostics: LocalAgentDiagnostics,
    },
    Error {
        schema_version: u8,
        code: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentStatus {
    pub node_id_base64: String,
    pub virtual_ip: Ipv4Addr,
    pub controller_connected: bool,
    pub network_active: bool,
    pub interface_name: String,
    pub interface_index: u32,
    pub configuration_version: u64,
    pub uptime_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalPeerStatus {
    pub node_id_base64: String,
    pub virtual_ip: Ipv4Addr,
    pub credential_not_after: DateTime<Utc>,
    pub role_bitmap: u32,
    pub tags: Vec<String>,
    pub candidates: Vec<EndpointCandidate>,
    pub active_endpoint: Option<SocketAddr>,
    pub active_candidate_kind: Option<EndpointCandidateKind>,
    pub path_reason: Option<PathSelectionReason>,
    pub session_established: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PathSelectionReason {
    HighestPriority,
    HandshakeFallback,
    AuthenticatedHandshake,
    AuthenticatedPeerTraffic,
    AuthenticatedPathProbe,
    ConfigurationUpdate,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentDiagnostics {
    pub status: LocalAgentStatus,
    pub configuration_generated_at: DateTime<Utc>,
    pub address_pool: String,
    pub configuration_sha256: String,
    pub tun_packets_received: u64,
    pub tun_packets_dropped: u64,
    pub last_error_code: Option<String>,
    pub local_candidates: Vec<EndpointCandidate>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlClientMessage {
    Authenticate {
        node_id_base64: String,
        credential_base64: String,
        signature_base64: String,
    },
    Sync {
        last_version: u64,
    },
    AdvertiseCandidates {
        advertisement: CandidateAdvertisement,
        signature_base64: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlServerMessage {
    Challenge {
        challenge_base64: String,
    },
    Authenticated {
        node_id_base64: String,
        configuration: SignedConfiguration,
    },
    Configuration {
        configuration: SignedConfiguration,
    },
    UpToDate {
        version: u64,
    },
    Error {
        code: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{BaselineReport, Component, LocalAgentRequest};

    #[test]
    fn report_is_stable_and_component_specific() {
        let report = BaselineReport::new(Component::Controller).to_string();
        assert_eq!(report, "xs-controller status=baseline-ready version=0.1.0");
    }

    #[test]
    fn component_names_are_unique() {
        let names = [
            Component::Agent.as_str(),
            Component::Cli.as_str(),
            Component::Controller.as_str(),
            Component::Relay.as_str(),
        ];
        assert_eq!(names.len(), 4);
        assert_eq!(
            names
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
    }

    #[test]
    fn local_request_is_strict_and_stable() {
        assert_eq!(
            serde_json::to_string(&LocalAgentRequest::Diagnostics {}).expect("serialize request"),
            r#"{"command":"diagnostics"}"#
        );
        assert!(
            serde_json::from_str::<LocalAgentRequest>(r#"{"command":"status","unexpected":true}"#)
                .is_err()
        );
    }
}
