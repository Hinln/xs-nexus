#![forbid(unsafe_code)]

use std::{
    fmt,
    net::{Ipv4Addr, SocketAddr},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod acl;
mod control_transfer;
mod healthcheck;
mod routes;
mod telemetry;
mod updates;

pub use acl::{
    AclAction, AclDecision, AclDecisionReason, AclPolicy, AclProtocol, AclRule, AclSelector,
    AclValidationError, PortRange,
};
pub use control_transfer::{ControlTransferAssembler, ControlTransferError};
pub use healthcheck::{HttpHealthcheckError, check_local_http_health};
pub use routes::validate_subnet_route_suggestion;
pub use routes::{ResolvedSubnetRoute, SubnetRoutePolicy, SubnetRouteValidationError};
pub use telemetry::{
    AgentPathKind, AgentPeerTelemetry, AgentTelemetryReport, MAX_AGENT_TELEMETRY_PEERS,
    RelayTelemetryMetrics, RelayTelemetryReport, SignedRelayTelemetryReport,
    agent_telemetry_report_signing_input, relay_telemetry_report_signing_input,
};
pub use updates::{
    AgentRuntimeReport, AgentUpdateState, LinuxReleaseManifest, MAX_UPDATE_ARCHIVE_BYTES,
    ReleaseManifestError, ReleaseVersion, ReleaseVersionError, UpdateChannel, UpdateDecision,
    UpdateDirective, UpdatePolicyError, UpdateRolloutPolicy, agent_runtime_report_signing_input,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Component {
    Agent,
    Cli,
    Controller,
    Relay,
}

pub const PRODUCT_NAME: &str = "xs-nexus";
pub const XSP_PROTOCOL_VERSION: &str = "XSP/1";
pub const BUILD_GIT_COMMIT: &str = env!("XS_BUILD_GIT_COMMIT");
pub const BUILD_DATE_EPOCH: &str = env!("XS_BUILD_DATE_EPOCH");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct BuildIdentity {
    pub product: &'static str,
    pub component: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
    pub protocol_version: &'static str,
    pub build_date_epoch: &'static str,
}

impl BuildIdentity {
    #[must_use]
    pub const fn current(component: Component) -> Self {
        Self {
            product: PRODUCT_NAME,
            component: component.as_str(),
            version: env!("CARGO_PKG_VERSION"),
            commit: BUILD_GIT_COMMIT,
            protocol_version: XSP_PROTOCOL_VERSION,
            build_date_epoch: BUILD_DATE_EPOCH,
        }
    }
}

impl fmt::Display for BuildIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} version={} commit={} protocol={}",
            self.component, self.version, self.commit, self.protocol_version
        )
    }
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

pub const CONTROL_TRANSPORT_QUERY_NAME: &str = "transport";
pub const CONTROL_CHUNKED_TRANSPORT_V1: &str = "chunked-v1";
pub const CONTROL_MESSAGE_LIMIT_BYTES: usize = 512 * 1024;
pub const CONTROL_TRANSFER_THRESHOLD_BYTES: usize = 8 * 1024;
pub const CONTROL_TRANSFER_CHUNK_BYTES: usize = 6_000;

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
    #[serde(default = "default_policy_version")]
    pub policy_version: u64,
    pub generated_at: DateTime<Utc>,
    pub address_pool: String,
    #[serde(default)]
    pub discovery_endpoints: Vec<SocketAddr>,
    pub nodes: Vec<ConfigurationNode>,
    #[serde(default)]
    pub relays: Vec<ConfigurationRelay>,
    #[serde(default)]
    pub policies: Vec<AclRule>,
    #[serde(default)]
    pub subnet_routes: Vec<ConfigurationSubnetRoute>,
}

const fn default_policy_version() -> u64 {
    1
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
    #[serde(default)]
    pub groups: Vec<String>,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_channel: Option<UpdateChannel>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubnetRouteMode {
    Routed,
    Nat,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationSubnetRoute {
    pub route_id: String,
    pub prefix: String,
    pub gateway_node_id_base64: String,
    pub mode: SubnetRouteMode,
    pub interface_name: String,
    pub priority: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubnetRouteSuggestion {
    pub prefix: String,
    pub interface_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubnetRouteAdvertisement {
    pub schema_version: u8,
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub generation: u64,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub suggestions: Vec<SubnetRouteSuggestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRelay {
    pub relay_id_base64: String,
    pub endpoint: SocketAddr,
    pub identity_public_key_base64: String,
    pub priority: u32,
    pub expires_at: DateTime<Utc>,
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
    Ping { virtual_ip: Ipv4Addr },
    Path { virtual_ip: Ipv4Addr },
    Routes {},
    Netcheck {},
    Diagnostics {},
    Reconnect {},
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
    Ping {
        schema_version: u8,
        result: LocalPingResult,
    },
    Path {
        schema_version: u8,
        path: LocalPeerPath,
    },
    Routes {
        schema_version: u8,
        configuration_version: u64,
        routes: Vec<LocalRouteStatus>,
    },
    Netcheck {
        schema_version: u8,
        result: LocalNetcheckResult,
    },
    Diagnostics {
        schema_version: u8,
        diagnostics: LocalAgentDiagnostics,
    },
    Reconnect {
        schema_version: u8,
        accepted: bool,
        error_code: Option<String>,
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

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalPingResult {
    pub virtual_ip: Ipv4Addr,
    pub reachable: bool,
    pub latency_microseconds: Option<u64>,
    pub active_candidate_kind: Option<EndpointCandidateKind>,
    pub path_reason: Option<PathSelectionReason>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalPeerPath {
    pub node_id_base64: String,
    pub virtual_ip: Ipv4Addr,
    pub session_established: bool,
    pub active_endpoint: Option<SocketAddr>,
    pub active_candidate_kind: Option<EndpointCandidateKind>,
    pub path_reason: Option<PathSelectionReason>,
    pub last_latency_microseconds: Option<u64>,
    pub tx_packets_total: u64,
    pub tx_bytes_total: u64,
    pub rx_packets_total: u64,
    pub rx_bytes_total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalRouteStatus {
    pub route_id: String,
    pub prefix: String,
    pub gateway_node_id_base64: String,
    pub gateway_virtual_ip: Ipv4Addr,
    pub mode: SubnetRouteMode,
    pub interface_name: String,
    pub priority: u32,
    pub local_is_gateway: bool,
    pub gateway_reachable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalNetcheckResult {
    pub healthy: bool,
    pub controller_connected: bool,
    pub network_active: bool,
    pub local_candidate_count: u64,
    pub configured_peer_count: u64,
    pub established_peer_count: u64,
    pub direct_peer_count: u64,
    pub relay_peer_count: u64,
    pub last_error_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PathSelectionReason {
    HighestPriority,
    HandshakeFallback,
    RelayFallback,
    RelayFailover,
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
    AdvertiseSubnetRoutes {
        advertisement: SubnetRouteAdvertisement,
        signature_base64: String,
    },
    ReportRuntime {
        report: AgentRuntimeReport,
        signature_base64: String,
    },
    ReportTelemetry {
        report: AgentTelemetryReport,
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
    UpdateDirective {
        directive: Option<UpdateDirective>,
    },
    TelemetryAccepted {
        sequence: u64,
    },
    TransferStart {
        transfer_id_base64: String,
        total_bytes: u32,
        chunk_count: u16,
        sha256_base64: String,
    },
    TransferChunk {
        transfer_id_base64: String,
        index: u16,
        data_base64: String,
    },
    TransferEnd {
        transfer_id_base64: String,
    },
    Error {
        code: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{BaselineReport, BuildIdentity, Component, LocalAgentRequest};

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
    fn build_identity_is_stable_and_contains_provenance() {
        let identity = BuildIdentity::current(Component::Agent);
        assert_eq!(identity.product, "xs-nexus");
        assert_eq!(identity.component, "xs-agent");
        assert_eq!(identity.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(identity.protocol_version, "XSP/1");
        assert!(
            identity.commit == "unknown"
                || (identity.commit.len() == 40
                    && identity
                        .commit
                        .bytes()
                        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
        );
        assert!(
            !identity.build_date_epoch.is_empty()
                && identity
                    .build_date_epoch
                    .bytes()
                    .all(|byte| byte.is_ascii_digit())
        );
        assert_eq!(
            identity.to_string(),
            format!(
                "xs-agent version={} commit={} protocol=XSP/1",
                env!("CARGO_PKG_VERSION"),
                identity.commit
            )
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
