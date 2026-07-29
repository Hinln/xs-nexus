use std::{collections::HashSet, net::Ipv4Addr, str::FromStr};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use ipnet::Ipv4Net;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Postgres, Row, Transaction};
use uuid::Uuid;
use xs_protocol::{
    CredentialClaims, controller_key_id, node_id, role_set_digest, sign_credential,
    verify_credential,
};

use crate::{
    error::ApiError,
    model::{
        ConfigurationNode, ConfigurationPayload, CreateEnrollmentTokenRequest,
        CreateNetworkRequest, EnrollRequest, EnrollResponse, EnrollmentTokenResponse,
        NetworkResponse, SignedConfiguration,
    },
    state::AppState,
};

const ENROLLMENT_TOKEN_DOMAIN: &[u8] = b"XS Nexus enrollment token v1";
const CONFIGURATION_DOMAIN: &[u8] = b"XS Nexus configuration v1";

#[derive(FromRow)]
struct TokenRow {
    network_id: Uuid,
    expires_at: DateTime<Utc>,
    max_uses: i32,
    use_count: i32,
    default_role_bitmap: i64,
    default_tags: Vec<String>,
    requested_virtual_ip: Option<String>,
}

#[derive(FromRow)]
struct NetworkAllocationRow {
    address_pool: String,
    reserved_addresses: i32,
}

#[derive(FromRow)]
struct ConfigurationNodeRow {
    node_id: Vec<u8>,
    identity_public_key: Vec<u8>,
    virtual_ip: String,
    credential_serial: i64,
    credential_not_after: DateTime<Utc>,
    role_bitmap: i64,
    tags: Vec<String>,
}

pub(crate) async fn create_network(
    state: &AppState,
    request: CreateNetworkRequest,
) -> Result<NetworkResponse, ApiError> {
    let name = validate_name(&request.name)?;
    let pool = Ipv4Net::from_str(&request.address_pool).map_err(|_| ApiError::validation())?;
    if !(8..=30).contains(&pool.prefix_len()) {
        return Err(ApiError::validation());
    }

    let usable_addresses =
        u64::from(u32::from(pool.broadcast())).saturating_sub(u64::from(u32::from(pool.network())));
    if request.reserved_addresses < 2
        || request.reserved_addresses > 4096
        || u64::from(request.reserved_addresses) >= usable_addresses
    {
        return Err(ApiError::validation());
    }

    let network_id = Uuid::new_v4();
    let created_at = Utc::now();
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;

    let inserted = sqlx::query(
        "INSERT INTO networks
         (id, name, address_pool, reserved_addresses, created_at)
         VALUES ($1, $2, $3::cidr, $4, $5)",
    )
    .bind(network_id)
    .bind(&name)
    .bind(pool.to_string())
    .bind(i32::try_from(request.reserved_addresses).map_err(|_| ApiError::validation())?)
    .bind(created_at)
    .execute(&mut *transaction)
    .await;

    if let Err(error) = inserted {
        return Err(map_write_error(error));
    }

    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(network_id),
            actor_type: "admin",
            actor_id: "bootstrap-admin",
            action: "network.create",
            target_type: "network",
            target_id: Some(network_id.to_string()),
            outcome: "success",
            metadata: json!({"address_pool": pool.to_string()}),
        },
    )
    .await?;

    let configuration =
        publish_configuration(&mut transaction, network_id, &state.config_signing_key).await?;
    transaction.commit().await.map_err(internal_database)?;

    Ok(NetworkResponse {
        id: network_id,
        name,
        address_pool: pool.to_string(),
        reserved_addresses: request.reserved_addresses,
        config_version: configuration.version,
        created_at,
    })
}

pub(crate) async fn create_enrollment_token(
    state: &AppState,
    request: CreateEnrollmentTokenRequest,
) -> Result<EnrollmentTokenResponse, ApiError> {
    if !(60..=604_800).contains(&request.expires_in_seconds)
        || !(1..=100).contains(&request.max_uses)
        || role_set_digest(request.default_role_bitmap, &request.default_tags).is_err()
    {
        return Err(ApiError::validation());
    }

    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let network = sqlx::query_as::<_, NetworkAllocationRow>(
        "SELECT address_pool::text AS address_pool, reserved_addresses
         FROM networks
         WHERE id = $1",
    )
    .bind(request.network_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;

    lock_network(&mut transaction, request.network_id).await?;
    let requested_virtual_ip = validate_requested_virtual_ip(
        &mut transaction,
        request.network_id,
        &network,
        request.requested_virtual_ip.as_deref(),
    )
    .await?;

    let token_id = Uuid::new_v4();
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|_| ApiError::internal())?;
    let token = format!("xsenr1_{}", URL_SAFE_NO_PAD.encode(random));
    let token_hash = enrollment_token_hash(&token);
    let expires_at = Utc::now()
        + Duration::seconds(
            i64::try_from(request.expires_in_seconds).map_err(|_| ApiError::validation())?,
        );

    let inserted = sqlx::query(
        "INSERT INTO enrollment_tokens
         (id, network_id, token_hash, expires_at, max_uses, default_role_bitmap,
          default_tags, requested_virtual_ip, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8::inet, $9)",
    )
    .bind(token_id)
    .bind(request.network_id)
    .bind(token_hash.as_slice())
    .bind(expires_at)
    .bind(i32::from(request.max_uses))
    .bind(i64::from(request.default_role_bitmap))
    .bind(&request.default_tags)
    .bind(requested_virtual_ip.map(|address| address.to_string()))
    .bind("bootstrap-admin")
    .execute(&mut *transaction)
    .await;
    if let Err(error) = inserted {
        return Err(map_write_error(error));
    }

    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(request.network_id),
            actor_type: "admin",
            actor_id: "bootstrap-admin",
            action: "enrollment_token.create",
            target_type: "enrollment_token",
            target_id: Some(token_id.to_string()),
            outcome: "success",
            metadata: json!({
                "expires_at": expires_at,
                "max_uses": request.max_uses,
                "requested_virtual_ip": requested_virtual_ip,
            }),
        },
    )
    .await?;

    transaction.commit().await.map_err(internal_database)?;
    Ok(EnrollmentTokenResponse {
        id: token_id,
        network_id: request.network_id,
        token,
        expires_at,
        max_uses: request.max_uses,
    })
}

async fn validate_requested_virtual_ip(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    network: &NetworkAllocationRow,
    requested: Option<&str>,
) -> Result<Option<Ipv4Addr>, ApiError> {
    let address = requested
        .map(Ipv4Addr::from_str)
        .transpose()
        .map_err(|_| ApiError::validation())?;
    let Some(address) = address else {
        return Ok(None);
    };
    validate_allocatable_address(
        Ipv4Net::from_str(&network.address_pool).map_err(|_| ApiError::internal())?,
        u32::try_from(network.reserved_addresses).map_err(|_| ApiError::internal())?,
        address,
    )?;
    let occupied: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM ip_leases
            WHERE network_id = $1 AND virtual_ip = $2::inet
              AND (state = 'active' OR cooldown_until > now())
         )",
    )
    .bind(network_id)
    .bind(address.to_string())
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal_database)?;
    if occupied {
        Err(ApiError::conflict())
    } else {
        Ok(Some(address))
    }
}

pub(crate) async fn enroll_node(
    state: &AppState,
    request: EnrollRequest,
) -> Result<EnrollResponse, ApiError> {
    let enrollment = validate_enrollment(&request)?;
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let token = locked_token(&mut transaction, &enrollment.token_hash).await?;

    let Some(token) = token else {
        transaction.rollback().await.map_err(internal_database)?;
        record_rejected_enrollment(&state.pool, None, "token").await;
        return Err(ApiError::invalid_enrollment());
    };
    if token.expires_at <= Utc::now() || token.use_count >= token.max_uses {
        transaction.rollback().await.map_err(internal_database)?;
        record_rejected_enrollment(&state.pool, Some(token.network_id), "token").await;
        return Err(ApiError::invalid_enrollment());
    }

    lock_network(&mut transaction, token.network_id).await?;
    if identity_exists(
        &mut transaction,
        &enrollment.node_id,
        &enrollment.identity_public_key,
    )
    .await?
    {
        transaction.rollback().await.map_err(internal_database)?;
        record_rejected_enrollment(&state.pool, Some(token.network_id), "duplicate").await;
        return Err(ApiError::conflict());
    }

    let network = load_network_allocation(&mut transaction, token.network_id).await?;
    let virtual_ip = allocate_virtual_ip(
        &mut transaction,
        token.network_id,
        &network,
        token.requested_virtual_ip.as_deref(),
    )
    .await?;
    let issued = issue_credential(
        &mut transaction,
        state,
        &token,
        enrollment.identity_public_key,
        virtual_ip,
    )
    .await?;
    let node_database_id = Uuid::new_v4();

    persist_node(
        &mut transaction,
        &PersistedNode {
            database_id: node_database_id,
            network_id: token.network_id,
            node_id: enrollment.node_id,
            name: &enrollment.name,
            device_type: &enrollment.device_type,
            identity_public_key: enrollment.identity_public_key,
            virtual_ip,
            tags: &token.default_tags,
            issued: &issued,
        },
    )
    .await?;
    consume_token(&mut transaction, &enrollment.token_hash).await?;

    let configuration = publish_configuration(
        &mut transaction,
        token.network_id,
        &state.config_signing_key,
    )
    .await?;
    let enrollment_actor_id = URL_SAFE_NO_PAD.encode(enrollment.node_id);
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(token.network_id),
            actor_type: "node",
            actor_id: &enrollment_actor_id,
            action: "node.enroll",
            target_type: "node",
            target_id: Some(node_database_id.to_string()),
            outcome: "success",
            metadata: json!({
                "virtual_ip": virtual_ip,
                "credential_serial": issued.serial,
                "device_type": enrollment.device_type,
            }),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;

    Ok(EnrollResponse {
        network_id: token.network_id,
        node_id_base64: URL_SAFE_NO_PAD.encode(enrollment.node_id),
        virtual_ip: virtual_ip.to_string(),
        credential_base64: URL_SAFE_NO_PAD.encode(issued.credential),
        credential_key_id: issued.credential_key_id,
        credential_signing_public_key_base64: URL_SAFE_NO_PAD
            .encode(state.credential_signing_key.verifying_key().as_bytes()),
        configuration_signing_public_key_base64: URL_SAFE_NO_PAD
            .encode(state.config_signing_key.verifying_key().as_bytes()),
        configuration,
    })
}

struct ValidatedEnrollment {
    name: String,
    device_type: String,
    identity_public_key: [u8; 32],
    node_id: [u8; 16],
    token_hash: [u8; 32],
}

fn validate_enrollment(request: &EnrollRequest) -> Result<ValidatedEnrollment, ApiError> {
    let name = validate_name(&request.name)?;
    let device_type = validate_device_type(&request.device_type)?.to_owned();
    if request.token.len() > 96 || !request.token.starts_with("xsenr1_") {
        return Err(ApiError::invalid_enrollment());
    }
    let identity_public_key = decode_array::<32>(&request.identity_public_key_base64)
        .map_err(|()| ApiError::validation())?;
    ed25519_dalek::VerifyingKey::from_bytes(&identity_public_key)
        .map_err(|_| ApiError::validation())?;

    Ok(ValidatedEnrollment {
        name,
        device_type,
        identity_public_key,
        node_id: node_id(&identity_public_key),
        token_hash: enrollment_token_hash(&request.token),
    })
}

async fn locked_token(
    transaction: &mut Transaction<'_, Postgres>,
    token_hash: &[u8; 32],
) -> Result<Option<TokenRow>, ApiError> {
    sqlx::query_as::<_, TokenRow>(
        "SELECT network_id, expires_at, max_uses, use_count, default_role_bitmap,
                default_tags, host(requested_virtual_ip) AS requested_virtual_ip
         FROM enrollment_tokens
         WHERE token_hash = $1 AND revoked_at IS NULL
         FOR UPDATE",
    )
    .bind(token_hash.as_slice())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(internal_database)
}

async fn lock_network(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(network_id.to_string())
        .execute(&mut **transaction)
        .await
        .map_err(internal_database)?;
    Ok(())
}

async fn identity_exists(
    transaction: &mut Transaction<'_, Postgres>,
    node_id: &[u8; 16],
    identity_public_key: &[u8; 32],
) -> Result<bool, ApiError> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM nodes
            WHERE node_id = $1 OR identity_public_key = $2
         )",
    )
    .bind(node_id.as_slice())
    .bind(identity_public_key.as_slice())
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal_database)
}

async fn load_network_allocation(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<NetworkAllocationRow, ApiError> {
    sqlx::query_as::<_, NetworkAllocationRow>(
        "SELECT address_pool::text AS address_pool, reserved_addresses
         FROM networks WHERE id = $1",
    )
    .bind(network_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal_database)
}

struct IssuedCredential {
    credential: [u8; 200],
    serial: u64,
    role_bitmap: u32,
    credential_key_id: u32,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

async fn issue_credential(
    transaction: &mut Transaction<'_, Postgres>,
    state: &AppState,
    token: &TokenRow,
    identity_public_key: [u8; 32],
    virtual_ip: Ipv4Addr,
) -> Result<IssuedCredential, ApiError> {
    let serial: i64 = sqlx::query_scalar("SELECT nextval('credential_serial')")
        .fetch_one(&mut **transaction)
        .await
        .map_err(internal_database)?;
    let serial = u64::try_from(serial).map_err(|_| ApiError::internal())?;
    let role_bitmap = u32::try_from(token.default_role_bitmap).map_err(|_| ApiError::internal())?;
    let role_digest =
        role_set_digest(role_bitmap, &token.default_tags).map_err(|_| ApiError::internal())?;
    let issued_at = Utc::now();
    let not_before = u64::try_from(issued_at.timestamp()).map_err(|_| ApiError::internal())?;
    let not_after = not_before
        .checked_add(state.credential_ttl_seconds)
        .ok_or_else(ApiError::internal)?;
    let expires_at = DateTime::from_timestamp(
        i64::try_from(not_after).map_err(|_| ApiError::internal())?,
        0,
    )
    .ok_or_else(ApiError::internal)?;
    let credential = sign_credential(
        CredentialClaims {
            network_id: *token.network_id.as_bytes(),
            identity_public_key,
            virtual_ipv4: virtual_ip,
            serial,
            not_before,
            not_after,
            role_bitmap,
            role_set_digest: role_digest,
        },
        &state.credential_signing_key,
    );

    Ok(IssuedCredential {
        credential,
        serial,
        role_bitmap,
        credential_key_id: controller_key_id(&state.credential_signing_key.verifying_key()),
        issued_at,
        expires_at,
    })
}

struct PersistedNode<'a> {
    database_id: Uuid,
    network_id: Uuid,
    node_id: [u8; 16],
    name: &'a str,
    device_type: &'a str,
    identity_public_key: [u8; 32],
    virtual_ip: Ipv4Addr,
    tags: &'a [String],
    issued: &'a IssuedCredential,
}

async fn persist_node(
    transaction: &mut Transaction<'_, Postgres>,
    node: &PersistedNode<'_>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO nodes
         (id, network_id, node_id, name, device_type, identity_public_key,
          virtual_ip, role_bitmap, tags, credential, credential_serial,
          credential_key_id, credential_not_before, credential_not_after)
         VALUES ($1, $2, $3, $4, $5, $6, $7::inet, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(node.database_id)
    .bind(node.network_id)
    .bind(node.node_id.as_slice())
    .bind(node.name)
    .bind(node.device_type)
    .bind(node.identity_public_key.as_slice())
    .bind(node.virtual_ip.to_string())
    .bind(i64::from(node.issued.role_bitmap))
    .bind(node.tags)
    .bind(node.issued.credential.as_slice())
    .bind(i64::try_from(node.issued.serial).map_err(|_| ApiError::internal())?)
    .bind(i64::from(node.issued.credential_key_id))
    .bind(node.issued.issued_at)
    .bind(node.issued.expires_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_write_error)?;

    sqlx::query(
        "INSERT INTO ip_leases (network_id, virtual_ip, node_id, state)
         VALUES ($1, $2::inet, $3, 'active')",
    )
    .bind(node.network_id)
    .bind(node.virtual_ip.to_string())
    .bind(node.database_id)
    .execute(&mut **transaction)
    .await
    .map_err(map_write_error)?;
    Ok(())
}

async fn consume_token(
    transaction: &mut Transaction<'_, Postgres>,
    token_hash: &[u8; 32],
) -> Result<(), ApiError> {
    let consumed = sqlx::query(
        "UPDATE enrollment_tokens
         SET use_count = use_count + 1
         WHERE token_hash = $1 AND revoked_at IS NULL
           AND expires_at > now() AND use_count < max_uses",
    )
    .bind(token_hash.as_slice())
    .execute(&mut **transaction)
    .await
    .map_err(internal_database)?;
    if consumed.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ApiError::invalid_enrollment())
    }
}

#[derive(Debug)]
pub(crate) struct AuthenticatedNode {
    pub network_id: Uuid,
    pub node_id_base64: String,
}

pub(crate) async fn authenticate_control(
    state: &AppState,
    challenge: &[u8; 32],
    node_id_base64: &str,
    credential_base64: &str,
    signature_base64: &str,
) -> Result<AuthenticatedNode, ApiError> {
    let claimed_node_id =
        decode_array::<16>(node_id_base64).map_err(|()| ApiError::unauthorized())?;
    let credential =
        decode_array::<200>(credential_base64).map_err(|()| ApiError::unauthorized())?;
    let signature = decode_array::<64>(signature_base64).map_err(|()| ApiError::unauthorized())?;
    let now = u64::try_from(Utc::now().timestamp()).map_err(|_| ApiError::unauthorized())?;
    let verified = verify_credential(
        &credential,
        &state.credential_signing_key.verifying_key(),
        now,
    )
    .map_err(|_| ApiError::unauthorized())?;
    if verified.node_id != claimed_node_id {
        return Err(ApiError::unauthorized());
    }

    let stored = sqlx::query(
        "SELECT network_id, credential, identity_public_key
         FROM nodes
         WHERE node_id = $1 AND revoked_at IS NULL",
    )
    .bind(claimed_node_id.as_slice())
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::unauthorized)?;
    let network_id = stored
        .try_get::<Uuid, _>("network_id")
        .map_err(internal_database)?;
    let stored_credential = stored
        .try_get::<Vec<u8>, _>("credential")
        .map_err(internal_database)?;
    let stored_public_key = stored
        .try_get::<Vec<u8>, _>("identity_public_key")
        .map_err(internal_database)?;
    if stored_credential.as_slice() != credential
        || stored_public_key.as_slice() != verified.identity_public_key
        || *network_id.as_bytes() != verified.network_id
    {
        return Err(ApiError::unauthorized());
    }

    let verifying_key = VerifyingKey::from_bytes(&verified.identity_public_key)
        .map_err(|_| ApiError::unauthorized())?;
    let signature = Signature::from_bytes(&signature);
    let mut authentication_input =
        Vec::with_capacity(b"XS Nexus control authentication v1".len() + 48);
    authentication_input.extend_from_slice(b"XS Nexus control authentication v1");
    authentication_input.extend_from_slice(challenge);
    authentication_input.extend_from_slice(&claimed_node_id);
    verifying_key
        .verify_strict(&authentication_input, &signature)
        .map_err(|_| ApiError::unauthorized())?;

    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(network_id),
            actor_type: "node",
            actor_id: node_id_base64,
            action: "control.authenticate",
            target_type: "control_session",
            target_id: None,
            outcome: "success",
            metadata: json!({}),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;

    Ok(AuthenticatedNode {
        network_id,
        node_id_base64: node_id_base64.to_owned(),
    })
}

pub(crate) async fn latest_configuration(
    state: &AppState,
    network_id: Uuid,
) -> Result<SignedConfiguration, ApiError> {
    let row = sqlx::query(
        "SELECT version, payload, signature, signer_key_id
         FROM configuration_versions
         WHERE network_id = $1
         ORDER BY version DESC
         LIMIT 1",
    )
    .bind(network_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;

    signed_configuration_from_row(&row)
}

async fn allocate_virtual_ip(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    network: &NetworkAllocationRow,
    requested: Option<&str>,
) -> Result<Ipv4Addr, ApiError> {
    let pool = Ipv4Net::from_str(&network.address_pool).map_err(|_| ApiError::internal())?;
    let reserved = u32::try_from(network.reserved_addresses).map_err(|_| ApiError::internal())?;
    let rows = sqlx::query(
        "SELECT host(virtual_ip) AS virtual_ip
         FROM ip_leases
         WHERE network_id = $1
           AND (state = 'active' OR cooldown_until > now())",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;
    let occupied = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("virtual_ip").ok())
        .filter_map(|address| Ipv4Addr::from_str(&address).ok())
        .collect::<HashSet<_>>();

    if let Some(requested) = requested {
        let address = Ipv4Addr::from_str(requested).map_err(|_| ApiError::internal())?;
        validate_allocatable_address(pool, reserved, address)?;
        if occupied.contains(&address) {
            return Err(ApiError::conflict());
        }
        return Ok(address);
    }

    let first = u32::from(pool.network())
        .checked_add(reserved)
        .ok_or_else(ApiError::address_pool_exhausted)?;
    let last = u32::from(pool.broadcast()).saturating_sub(1);
    for raw in first..=last {
        let candidate = Ipv4Addr::from(raw);
        if !occupied.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(ApiError::address_pool_exhausted())
}

fn validate_allocatable_address(
    pool: Ipv4Net,
    reserved: u32,
    address: Ipv4Addr,
) -> Result<(), ApiError> {
    let first = u32::from(pool.network())
        .checked_add(reserved)
        .ok_or_else(ApiError::validation)?;
    let last = u32::from(pool.broadcast()).saturating_sub(1);
    let candidate = u32::from(address);
    if pool.contains(&address) && (first..=last).contains(&candidate) {
        Ok(())
    } else {
        Err(ApiError::validation())
    }
}

async fn publish_configuration(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    signing_key: &SigningKey,
) -> Result<SignedConfiguration, ApiError> {
    let network = sqlx::query(
        "UPDATE networks
         SET config_version = config_version + 1
         WHERE id = $1
         RETURNING config_version, address_pool::text AS address_pool",
    )
    .bind(network_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal_database)?;
    let version = u64::try_from(
        network
            .try_get::<i64, _>("config_version")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;
    let address_pool = network
        .try_get::<String, _>("address_pool")
        .map_err(internal_database)?;

    let rows = sqlx::query_as::<_, ConfigurationNodeRow>(
        "SELECT node_id, identity_public_key, host(virtual_ip) AS virtual_ip,
                credential_serial, credential_not_after, role_bitmap, tags
         FROM nodes
         WHERE network_id = $1 AND revoked_at IS NULL
         ORDER BY node_id",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;

    let mut nodes = Vec::with_capacity(rows.len());
    for row in rows {
        nodes.push(ConfigurationNode {
            node_id_base64: URL_SAFE_NO_PAD.encode(row.node_id),
            identity_public_key_base64: URL_SAFE_NO_PAD.encode(row.identity_public_key),
            virtual_ip: row.virtual_ip,
            direct_endpoints: Vec::new(),
            credential_serial: u64::try_from(row.credential_serial)
                .map_err(|_| ApiError::internal())?,
            credential_not_after: row.credential_not_after,
            role_bitmap: u32::try_from(row.role_bitmap).map_err(|_| ApiError::internal())?,
            tags: row.tags,
        });
    }

    let payload = serde_json::to_vec(&ConfigurationPayload {
        schema_version: 1,
        network_id,
        version,
        generated_at: Utc::now(),
        address_pool,
        nodes,
        relays: Vec::new(),
        policies: Vec::new(),
    })
    .map_err(|_| ApiError::internal())?;
    if payload.len() > 1_048_576 {
        return Err(ApiError::internal());
    }

    let mut signing_input = Vec::with_capacity(CONFIGURATION_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(CONFIGURATION_DOMAIN);
    signing_input.extend_from_slice(&payload);
    let signature = signing_key.sign(&signing_input).to_bytes();
    let key_id = controller_key_id(&signing_key.verifying_key());

    sqlx::query(
        "INSERT INTO configuration_versions
         (network_id, version, payload, signature, signer_key_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(network_id)
    .bind(i64::try_from(version).map_err(|_| ApiError::internal())?)
    .bind(&payload)
    .bind(signature.as_slice())
    .bind(i64::from(key_id))
    .execute(&mut **transaction)
    .await
    .map_err(map_write_error)?;

    Ok(SignedConfiguration {
        version,
        payload_base64: URL_SAFE_NO_PAD.encode(payload),
        signature_base64: URL_SAFE_NO_PAD.encode(signature),
        signer_key_id: key_id,
    })
}

fn signed_configuration_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SignedConfiguration, ApiError> {
    let version = u64::try_from(
        row.try_get::<i64, _>("version")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;
    let payload = row
        .try_get::<Vec<u8>, _>("payload")
        .map_err(internal_database)?;
    let signature = row
        .try_get::<Vec<u8>, _>("signature")
        .map_err(internal_database)?;
    let signer_key_id = u32::try_from(
        row.try_get::<i64, _>("signer_key_id")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;

    Ok(SignedConfiguration {
        version,
        payload_base64: URL_SAFE_NO_PAD.encode(payload),
        signature_base64: URL_SAFE_NO_PAD.encode(signature),
        signer_key_id,
    })
}

struct AuditEvent<'a> {
    network_id: Option<Uuid>,
    actor_type: &'a str,
    actor_id: &'a str,
    action: &'a str,
    target_type: &'a str,
    target_id: Option<String>,
    outcome: &'a str,
    metadata: serde_json::Value,
}

async fn append_audit(
    transaction: &mut Transaction<'_, Postgres>,
    event: AuditEvent<'_>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_events
         (network_id, actor_type, actor_id, action, target_type, target_id, outcome, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(event.network_id)
    .bind(event.actor_type)
    .bind(event.actor_id)
    .bind(event.action)
    .bind(event.target_type)
    .bind(event.target_id)
    .bind(event.outcome)
    .bind(event.metadata)
    .execute(&mut **transaction)
    .await
    .map_err(internal_database)?;
    Ok(())
}

async fn record_rejected_enrollment(
    pool: &sqlx::PgPool,
    network_id: Option<Uuid>,
    reason_class: &'static str,
) {
    let result = sqlx::query(
        "INSERT INTO audit_events
         (network_id, actor_type, actor_id, action, target_type, outcome, metadata)
         VALUES ($1, 'anonymous', 'untrusted', 'node.enroll', 'node', 'rejected', $2)",
    )
    .bind(network_id)
    .bind(json!({"reason_class": reason_class}))
    .execute(pool)
    .await;
    if result.is_err() {
        tracing::warn!(event = "audit_write_failed", action = "node.enroll");
    }
}

fn enrollment_token_hash(token: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(ENROLLMENT_TOKEN_DOMAIN);
    hash.update(token.as_bytes());
    hash.finalize().into()
}

fn validate_name(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 64 || trimmed.chars().any(char::is_control) {
        Err(ApiError::validation())
    } else {
        Ok(trimmed.to_owned())
    }
}

fn validate_device_type(device_type: &str) -> Result<&str, ApiError> {
    match device_type {
        "linux" | "windows" | "nas" | "test" => Ok(device_type),
        _ => Err(ApiError::validation()),
    }
}

fn decode_array<const LENGTH: usize>(encoded: &str) -> Result<[u8; LENGTH], ()> {
    let decoded = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| ())?;
    decoded.try_into().map_err(|_| ())
}

fn internal_database(_error: sqlx::Error) -> ApiError {
    tracing::error!(event = "database_operation_failed");
    ApiError::internal()
}

fn map_write_error(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
    {
        ApiError::conflict()
    } else {
        internal_database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocatable_addresses_respect_reserved_and_broadcast_bounds() {
        let pool = Ipv4Net::from_str("100.88.0.0/24").expect("valid network");
        assert!(validate_allocatable_address(pool, 16, Ipv4Addr::new(100, 88, 0, 16)).is_ok());
        assert!(validate_allocatable_address(pool, 16, Ipv4Addr::new(100, 88, 0, 15)).is_err());
        assert!(validate_allocatable_address(pool, 16, Ipv4Addr::new(100, 88, 0, 255)).is_err());
        assert!(validate_allocatable_address(pool, 16, Ipv4Addr::new(100, 88, 1, 16)).is_err());
    }

    #[test]
    fn token_hash_is_domain_separated_and_deterministic() {
        assert_eq!(
            enrollment_token_hash("example"),
            enrollment_token_hash("example")
        );
        assert_ne!(
            enrollment_token_hash("example"),
            <[u8; 32]>::from(Sha256::digest(b"example"))
        );
    }

    #[test]
    fn fixed_length_base64_decoder_rejects_wrong_length() {
        assert!(decode_array::<16>(&URL_SAFE_NO_PAD.encode([1_u8; 16])).is_ok());
        assert!(decode_array::<16>(&URL_SAFE_NO_PAD.encode([1_u8; 15])).is_err());
        assert!(decode_array::<16>("***").is_err());
    }
}
