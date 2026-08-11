use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use xs_core::{
    AclAction, AclDecision, AclDecisionReason, AclPolicy, AclProtocol, AclRule, AclSelector,
    AgentPathKind, AgentPeerTelemetry, AgentRuntimeReport, AgentTelemetryReport,
    CandidateAdvertisement, ConfigurationNode, ConfigurationPayload, ConfigurationSubnetRoute,
    ControlClientMessage, ControlServerMessage, EndpointCandidate, EndpointCandidateKind,
    EnrollRequest, EnrollResponse, PortRange, SignedConfiguration, SubnetRouteAdvertisement,
    SubnetRouteMode, SubnetRouteSuggestion, UpdateChannel, UpdateDirective,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateNetworkRequest {
    pub name: String,
    pub address_pool: String,
    #[serde(default = "default_reserved_addresses")]
    pub reserved_addresses: u32,
}

const fn default_reserved_addresses() -> u32 {
    16
}

#[derive(Debug, Serialize)]
pub struct NetworkResponse {
    pub id: Uuid,
    pub name: String,
    pub address_pool: String,
    pub reserved_addresses: u32,
    pub config_version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEnrollmentTokenRequest {
    pub network_id: Uuid,
    pub expires_in_seconds: u64,
    #[serde(default = "default_max_uses")]
    pub max_uses: u16,
    #[serde(default)]
    pub default_role_bitmap: u32,
    #[serde(default)]
    pub default_tags: Vec<String>,
    pub requested_virtual_ip: Option<String>,
}

const fn default_max_uses() -> u16 {
    1
}

#[derive(Debug, Serialize)]
pub struct EnrollmentTokenResponse {
    pub id: Uuid,
    pub network_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub max_uses: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AclGroupRequest {
    pub name: String,
    pub node_ids_base64: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceAclPolicyRequest {
    pub expected_policy_version: u64,
    #[serde(default)]
    pub groups: Vec<AclGroupRequest>,
    pub rules: Vec<AclRule>,
}

#[derive(Debug, Serialize)]
pub struct ReplaceAclPolicyResponse {
    pub network_id: Uuid,
    pub policy_version: u64,
    pub configuration_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplainAclRequest {
    pub source_node_id_base64: String,
    pub destination_node_id_base64: String,
    pub protocol: AclProtocol,
    pub destination_port: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct ExplainAclResponse {
    pub network_id: Uuid,
    pub policy_version: u64,
    pub decision: AclDecision,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeNodeRequest {
    #[serde(default = "default_ip_cooldown_seconds")]
    pub ip_cooldown_seconds: u64,
}

const fn default_ip_cooldown_seconds() -> u64 {
    3600
}

#[derive(Debug, Serialize)]
pub struct RevokeNodeResponse {
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub virtual_ip: String,
    pub cooldown_until: DateTime<Utc>,
    pub configuration_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceNodeUpdateChannelRequest {
    pub expected_configuration_version: u64,
    pub update_channel: UpdateChannel,
}

#[derive(Debug, Serialize)]
pub struct ReplaceNodeUpdateChannelResponse {
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub update_channel: UpdateChannel,
    pub configuration_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubnetRouteApprovalRequest {
    pub route_id: String,
    pub gateway_node_id_base64: String,
    pub prefix: String,
    pub interface_name: String,
    pub mode: SubnetRouteMode,
    pub priority: u32,
    #[serde(default = "default_route_enabled")]
    pub enabled: bool,
}

const fn default_route_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceSubnetRoutesRequest {
    pub expected_configuration_version: u64,
    pub routes: Vec<SubnetRouteApprovalRequest>,
}

#[derive(Debug, Serialize)]
pub struct ReplaceSubnetRoutesResponse {
    pub network_id: Uuid,
    pub configuration_version: u64,
    pub enabled_routes: usize,
    pub paused_routes: usize,
}

#[derive(Debug, Serialize)]
pub struct SubnetRouteSuggestionResponse {
    pub gateway_node_id_base64: String,
    pub generation: u64,
    pub expires_at: DateTime<Utc>,
    pub suggestions: Vec<SubnetRouteSuggestion>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUpdateReleaseRequest {
    pub manifest_base64: String,
    pub signature_base64: String,
    pub archive_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeUpdateReleaseRequest {
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateReleaseResponse {
    pub id: Uuid,
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub target: String,
    pub archive_name: String,
    pub archive_size: u64,
    pub archive_sha256: String,
    pub archive_url: String,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceUpdatePolicyRequest {
    pub expected_generation: u64,
    pub release_id: Uuid,
    pub minimum_version: Option<String>,
    pub rollout_basis_points: u16,
    pub paused: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdatePolicyResponse {
    pub network_id: Uuid,
    pub channel: UpdateChannel,
    pub platform: String,
    pub architecture: String,
    pub release: UpdateReleaseResponse,
    pub minimum_version: Option<String>,
    pub rollout_basis_points: u16,
    pub paused: bool,
    pub generation: u64,
    pub updated_at: DateTime<Utc>,
}
