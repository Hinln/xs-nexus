use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use xs_core::{
    ConfigurationNode, ConfigurationPayload, ControlClientMessage, ControlServerMessage,
    EnrollRequest, EnrollResponse, SignedConfiguration,
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

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
}
