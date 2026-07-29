use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
pub struct EnrollRequest {
    pub token: String,
    pub name: String,
    pub device_type: String,
    pub identity_public_key_base64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedConfiguration {
    pub version: u64,
    pub payload_base64: String,
    pub signature_base64: String,
    pub signer_key_id: u32,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ConfigurationPayload {
    pub schema_version: u8,
    pub network_id: Uuid,
    pub version: u64,
    pub generated_at: DateTime<Utc>,
    pub address_pool: String,
    pub nodes: Vec<ConfigurationNode>,
    pub relays: Vec<serde_json::Value>,
    pub policies: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ConfigurationNode {
    pub node_id_base64: String,
    pub identity_public_key_base64: String,
    pub virtual_ip: String,
    pub credential_serial: u64,
    pub credential_not_after: DateTime<Utc>,
    pub role_bitmap: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
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
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
        code: &'static str,
    },
}
