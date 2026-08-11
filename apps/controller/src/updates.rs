use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Postgres, Transaction};
use url::Url;
use uuid::Uuid;
use xs_core::{
    AgentRuntimeReport, AgentUpdateState, LinuxReleaseManifest, ReleaseVersion, UpdateChannel,
    UpdateDecision, UpdateDirective, UpdateRolloutPolicy, agent_runtime_report_signing_input,
};

use crate::{
    auth::ManagementActor,
    error::ApiError,
    model::{
        CreateUpdateReleaseRequest, ReplaceUpdatePolicyRequest, RevokeUpdateReleaseRequest,
        UpdatePolicyResponse, UpdateReleaseResponse,
    },
    service::AuthenticatedNode,
    state::AppState,
};

#[derive(FromRow)]
struct ReleaseRow {
    id: Uuid,
    version: String,
    platform: String,
    architecture: String,
    target: String,
    archive_name: String,
    archive_size: i64,
    archive_sha256: Vec<u8>,
    archive_url: String,
    revoked_at: Option<DateTime<Utc>>,
    revocation_reason: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct PolicyRow {
    network_id: Uuid,
    channel: String,
    platform: String,
    architecture: String,
    release_id: Uuid,
    minimum_version: Option<String>,
    rollout_basis_points: i32,
    paused: bool,
    generation: i64,
    updated_at: DateTime<Utc>,
    version: String,
    target: String,
    archive_name: String,
    archive_size: i64,
    archive_sha256: Vec<u8>,
    archive_url: String,
    release_revoked_at: Option<DateTime<Utc>>,
    release_revocation_reason: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct DirectiveRow {
    release_id: Uuid,
    minimum_version: Option<String>,
    rollout_basis_points: i32,
    paused: bool,
    generation: i64,
    version: String,
    platform: String,
    architecture: String,
    archive_name: String,
    archive_size: i64,
    archive_sha256: Vec<u8>,
    archive_url: String,
    manifest: Vec<u8>,
    signature: Vec<u8>,
}

#[derive(FromRow)]
struct RuntimeReportRow {
    agent_version: String,
    platform: String,
    architecture: String,
    update_channel: String,
    update_state: String,
    observed_release_id: Option<Uuid>,
    last_error_code: Option<String>,
    generated_at: DateTime<Utc>,
}

struct PolicyMutation<'a> {
    network_id: Uuid,
    channel: UpdateChannel,
    platform: &'a str,
    architecture: &'a str,
    release_id: Uuid,
    minimum_version: Option<ReleaseVersion>,
    rollout_basis_points: u16,
    paused: bool,
    generation: i64,
}

struct PolicyResponseParts {
    network_id: Uuid,
    channel: UpdateChannel,
    platform: String,
    architecture: String,
    minimum_version: Option<String>,
    rollout_basis_points: u16,
    paused: bool,
    generation: i64,
    updated_at: DateTime<Utc>,
}

pub(crate) async fn create_release(
    state: &AppState,
    request: CreateUpdateReleaseRequest,
    actor: &ManagementActor,
) -> Result<UpdateReleaseResponse, ApiError> {
    let verifying_key = state
        .update_signing_public_key
        .as_ref()
        .ok_or_else(ApiError::unavailable)?;
    let manifest_bytes = decode_canonical(&request.manifest_base64, 4096)?;
    let signature_bytes = decode_canonical(&request.signature_base64, 64)?;
    let signature_array: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::validation())?;
    verifying_key
        .verify(&manifest_bytes, &Signature::from_bytes(&signature_array))
        .map_err(|_| ApiError::validation())?;
    let manifest =
        LinuxReleaseManifest::parse(&manifest_bytes).map_err(|_| ApiError::validation())?;
    let archive_url = validate_archive_url(&request.archive_url, &manifest.archive)?;
    let archive_hash = decode_lower_hex(&manifest.archive_sha256)?;
    let manifest_hash: [u8; 32] = Sha256::digest(&manifest_bytes).into();
    let id = Uuid::new_v4();
    let archive_size = i64::try_from(manifest.archive_size).map_err(|_| ApiError::validation())?;
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let inserted = sqlx::query_as::<_, ReleaseRow>(
        "INSERT INTO update_releases
         (id, version, platform, architecture, target, archive_name, archive_size,
          archive_sha256, archive_url, manifest, manifest_sha256, signature,
          created_by_type, created_by_id)
         VALUES ($1, $2, 'linux', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING id, version, platform, architecture, target, archive_name,
                   archive_size, archive_sha256, archive_url, revoked_at,
                   revocation_reason, created_at",
    )
    .bind(id)
    .bind(manifest.version.to_string())
    .bind(&manifest.architecture)
    .bind(&manifest.target)
    .bind(&manifest.archive)
    .bind(archive_size)
    .bind(&archive_hash)
    .bind(archive_url.as_str())
    .bind(&manifest_bytes)
    .bind(manifest_hash.as_slice())
    .bind(signature_bytes)
    .bind(actor.actor_type())
    .bind(actor.actor_id())
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_write_error)?;
    append_audit(
        &mut transaction,
        actor,
        None,
        "update.release.create",
        "update_release",
        id.to_string(),
        json!({
            "version": inserted.version,
            "platform": inserted.platform,
            "architecture": inserted.architecture,
            "archive_sha256": encode_lower_hex(&inserted.archive_sha256),
        }),
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    release_response(inserted)
}

pub(crate) async fn list_releases(
    state: &AppState,
) -> Result<Vec<UpdateReleaseResponse>, ApiError> {
    let rows = sqlx::query_as::<_, ReleaseRow>(
        "SELECT id, version, platform, architecture, target, archive_name,
                archive_size, archive_sha256, archive_url, revoked_at,
                revocation_reason, created_at
         FROM update_releases
         ORDER BY created_at DESC, id
         LIMIT 256",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_database)?;
    rows.into_iter().map(release_response).collect()
}

pub(crate) async fn revoke_release(
    state: &AppState,
    release_id: Uuid,
    request: RevokeUpdateReleaseRequest,
    actor: &ManagementActor,
) -> Result<UpdateReleaseResponse, ApiError> {
    if !valid_revocation_reason(&request.reason) {
        return Err(ApiError::validation());
    }
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let current = sqlx::query_as::<_, ReleaseRow>(
        "SELECT id, version, platform, architecture, target, archive_name,
                archive_size, archive_sha256, archive_url, revoked_at,
                revocation_reason, created_at
         FROM update_releases WHERE id = $1 FOR UPDATE",
    )
    .bind(release_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;
    if current.revoked_at.is_some() {
        return Err(ApiError::conflict());
    }

    let revoked = sqlx::query_as::<_, ReleaseRow>(
        "UPDATE update_releases
         SET revoked_at = now(), revoked_by_type = $2, revoked_by_id = $3,
             revocation_reason = $4
         WHERE id = $1 AND revoked_at IS NULL
         RETURNING id, version, platform, architecture, target, archive_name,
                   archive_size, archive_sha256, archive_url, revoked_at,
                   revocation_reason, created_at",
    )
    .bind(release_id)
    .bind(actor.actor_type())
    .bind(actor.actor_id())
    .bind(&request.reason)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_write_error)?;
    let mut affected_networks = sqlx::query_scalar::<_, Uuid>(
        "UPDATE update_rollout_policies
         SET paused = true, generation = generation + 1,
             updated_by_type = $2, updated_by_id = $3, updated_at = now()
         WHERE release_id = $1
         RETURNING network_id",
    )
    .bind(release_id)
    .bind(actor.actor_type())
    .bind(actor.actor_id())
    .fetch_all(&mut *transaction)
    .await
    .map_err(map_write_error)?;
    append_audit(
        &mut transaction,
        actor,
        None,
        "update.release.revoke",
        "update_release",
        release_id.to_string(),
        json!({
            "reason": request.reason,
            "paused_policy_count": affected_networks.len(),
        }),
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    affected_networks.sort_unstable();
    affected_networks.dedup();
    for network_id in affected_networks {
        state.notify_update_changed(network_id);
    }
    release_response(revoked)
}

pub(crate) async fn replace_policy(
    state: &AppState,
    network_id: Uuid,
    channel: &str,
    platform: &str,
    architecture: &str,
    request: ReplaceUpdatePolicyRequest,
    actor: &ManagementActor,
) -> Result<UpdatePolicyResponse, ApiError> {
    let channel = parse_channel(channel)?;
    validate_platform_architecture(platform, architecture)?;
    let minimum_version = request
        .minimum_version
        .as_deref()
        .map(str::parse::<ReleaseVersion>)
        .transpose()
        .map_err(|_| ApiError::validation())?;
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let network_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM networks WHERE id = $1 FOR UPDATE)",
    )
    .bind(network_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(internal_database)?;
    if !network_exists {
        return Err(ApiError::not_found());
    }
    let release =
        policy_release(&mut transaction, request.release_id, platform, architecture).await?;
    let target_version = release
        .version
        .parse::<ReleaseVersion>()
        .map_err(|_| ApiError::internal())?;
    let policy = UpdateRolloutPolicy {
        schema_version: 1,
        release_id: request.release_id,
        channel,
        target_version,
        minimum_version,
        rollout_basis_points: request.rollout_basis_points,
        paused: request.paused,
    };
    policy.validate().map_err(|_| ApiError::validation())?;
    let current_generation = sqlx::query_scalar::<_, i64>(
        "SELECT generation FROM update_rollout_policies
         WHERE network_id = $1 AND channel = $2 AND platform = $3 AND architecture = $4
         FOR UPDATE",
    )
    .bind(network_id)
    .bind(channel_name(channel))
    .bind(platform)
    .bind(architecture)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?;
    let expected_generation =
        i64::try_from(request.expected_generation).map_err(|_| ApiError::validation())?;
    if current_generation.unwrap_or(0) != expected_generation {
        return Err(ApiError::conflict());
    }
    let generation = current_generation
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(ApiError::conflict)?;
    let mutation = PolicyMutation {
        network_id,
        channel,
        platform,
        architecture,
        release_id: request.release_id,
        minimum_version,
        rollout_basis_points: request.rollout_basis_points,
        paused: request.paused,
        generation,
    };
    persist_policy(&mut transaction, actor, &mutation).await?;
    transaction.commit().await.map_err(internal_database)?;
    state.notify_update_changed(network_id);
    policy_response(
        PolicyResponseParts {
            network_id,
            channel,
            platform: platform.to_owned(),
            architecture: architecture.to_owned(),
            minimum_version: minimum_version.map(|version| version.to_string()),
            rollout_basis_points: request.rollout_basis_points,
            paused: request.paused,
            generation,
            updated_at: Utc::now(),
        },
        release,
    )
}

async fn policy_release(
    transaction: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
    platform: &str,
    architecture: &str,
) -> Result<ReleaseRow, ApiError> {
    let release = sqlx::query_as::<_, ReleaseRow>(
        "SELECT id, version, platform, architecture, target, archive_name,
                archive_size, archive_sha256, archive_url, revoked_at,
                revocation_reason, created_at
         FROM update_releases WHERE id = $1",
    )
    .bind(release_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;
    if release.platform != platform
        || release.architecture != architecture
        || release.revoked_at.is_some()
    {
        return Err(ApiError::validation());
    }
    Ok(release)
}

async fn persist_policy(
    transaction: &mut Transaction<'_, Postgres>,
    actor: &ManagementActor,
    mutation: &PolicyMutation<'_>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO update_rollout_policies
         (network_id, channel, platform, architecture, release_id, minimum_version,
          rollout_basis_points, paused, generation, updated_by_type, updated_by_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (network_id, channel, platform, architecture) DO UPDATE SET
             release_id = EXCLUDED.release_id,
             minimum_version = EXCLUDED.minimum_version,
             rollout_basis_points = EXCLUDED.rollout_basis_points,
             paused = EXCLUDED.paused,
             generation = EXCLUDED.generation,
             updated_by_type = EXCLUDED.updated_by_type,
             updated_by_id = EXCLUDED.updated_by_id,
             updated_at = now()",
    )
    .bind(mutation.network_id)
    .bind(channel_name(mutation.channel))
    .bind(mutation.platform)
    .bind(mutation.architecture)
    .bind(mutation.release_id)
    .bind(mutation.minimum_version.map(|version| version.to_string()))
    .bind(i32::from(mutation.rollout_basis_points))
    .bind(mutation.paused)
    .bind(mutation.generation)
    .bind(actor.actor_type())
    .bind(actor.actor_id())
    .execute(&mut **transaction)
    .await
    .map_err(map_write_error)?;
    append_audit(
        transaction,
        actor,
        Some(mutation.network_id),
        "update.policy.replace",
        "update_rollout_policy",
        format!(
            "{}:{}:{}",
            channel_name(mutation.channel),
            mutation.platform,
            mutation.architecture
        ),
        json!({
            "release_id": mutation.release_id,
            "generation": mutation.generation,
            "rollout_basis_points": mutation.rollout_basis_points,
            "paused": mutation.paused,
            "minimum_version": mutation.minimum_version.map(|version| version.to_string()),
        }),
    )
    .await
}

pub(crate) async fn list_policies(
    state: &AppState,
    network_id: Uuid,
) -> Result<Vec<UpdatePolicyResponse>, ApiError> {
    let rows = sqlx::query_as::<_, PolicyRow>(
        "SELECT p.network_id, p.channel, p.platform, p.architecture, p.release_id,
                p.minimum_version, p.rollout_basis_points, p.paused, p.generation,
                p.updated_at, r.version, r.target, r.archive_name, r.archive_size,
                r.archive_sha256, r.archive_url, r.revoked_at AS release_revoked_at,
                r.revocation_reason AS release_revocation_reason, r.created_at
         FROM update_rollout_policies p
         JOIN update_releases r ON r.id = p.release_id
         WHERE p.network_id = $1
         ORDER BY p.channel, p.platform, p.architecture",
    )
    .bind(network_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_database)?;
    rows.into_iter()
        .map(|row| {
            let channel = parse_channel(&row.channel).map_err(|_| ApiError::internal())?;
            let release = ReleaseRow {
                id: row.release_id,
                version: row.version,
                platform: row.platform.clone(),
                architecture: row.architecture.clone(),
                target: row.target,
                archive_name: row.archive_name,
                archive_size: row.archive_size,
                archive_sha256: row.archive_sha256,
                archive_url: row.archive_url,
                revoked_at: row.release_revoked_at,
                revocation_reason: row.release_revocation_reason,
                created_at: row.created_at,
            };
            policy_response(
                PolicyResponseParts {
                    network_id: row.network_id,
                    channel,
                    platform: row.platform,
                    architecture: row.architecture,
                    minimum_version: row.minimum_version,
                    rollout_basis_points: u16::try_from(row.rollout_basis_points)
                        .map_err(|_| ApiError::internal())?,
                    paused: row.paused,
                    generation: row.generation,
                    updated_at: row.updated_at,
                },
                release,
            )
        })
        .collect()
}

pub(crate) async fn record_runtime_report(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    report: AgentRuntimeReport,
    signature_base64: &str,
) -> Result<Option<UpdateDirective>, ApiError> {
    let assigned_channel = assigned_update_channel(state, authenticated).await?;
    validate_runtime_report(authenticated, assigned_channel, &report)?;
    let signature_bytes = decode_canonical(signature_base64, 64)?;
    let signature_array: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::validation())?;
    let verifying_key = VerifyingKey::from_bytes(&authenticated.identity_public_key)
        .map_err(|_| ApiError::validation())?;
    let signing_input =
        agent_runtime_report_signing_input(&report).map_err(|_| ApiError::validation())?;
    verifying_key
        .verify_strict(&signing_input, &Signature::from_bytes(&signature_array))
        .map_err(|_| ApiError::validation())?;

    let result = sqlx::query(
        "INSERT INTO node_update_reports
         (node_id, agent_version, platform, architecture, update_channel,
          update_state, observed_release_id, last_error_code, generated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (node_id) DO UPDATE SET
             agent_version = EXCLUDED.agent_version,
             platform = EXCLUDED.platform,
             architecture = EXCLUDED.architecture,
             update_channel = EXCLUDED.update_channel,
             update_state = EXCLUDED.update_state,
             observed_release_id = EXCLUDED.observed_release_id,
             last_error_code = EXCLUDED.last_error_code,
             generated_at = EXCLUDED.generated_at,
             received_at = now()
         WHERE node_update_reports.generated_at < EXCLUDED.generated_at",
    )
    .bind(authenticated.node_id.as_slice())
    .bind(report.agent_version.to_string())
    .bind(&report.platform)
    .bind(&report.architecture)
    .bind(channel_name(report.update_channel))
    .bind(update_state_name(report.update_state))
    .bind(report.observed_release_id)
    .bind(&report.last_error_code)
    .bind(report.generated_at)
    .execute(&state.pool)
    .await
    .map_err(map_write_error)?;
    if result.rows_affected() != 1 {
        return Err(ApiError::conflict());
    }
    directive_for_report(state, authenticated, &report).await
}

pub(crate) async fn current_update_directive(
    state: &AppState,
    authenticated: &AuthenticatedNode,
) -> Result<Option<UpdateDirective>, ApiError> {
    let report = sqlx::query_as::<_, RuntimeReportRow>(
        "SELECT agent_version, platform, architecture, update_channel, update_state,
                observed_release_id, last_error_code, generated_at
         FROM node_update_reports WHERE node_id = $1",
    )
    .bind(authenticated.node_id.as_slice())
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_database)?;
    let Some(report) = report else {
        return Ok(None);
    };
    let report = AgentRuntimeReport {
        schema_version: 1,
        network_id: authenticated.network_id,
        node_id_base64: authenticated.node_id_base64.clone(),
        agent_version: report
            .agent_version
            .parse()
            .map_err(|_| ApiError::internal())?,
        platform: report.platform,
        architecture: report.architecture,
        update_channel: parse_channel(&report.update_channel).map_err(|_| ApiError::internal())?,
        update_state: parse_update_state(&report.update_state)?,
        observed_release_id: report.observed_release_id,
        last_error_code: report.last_error_code,
        generated_at: report.generated_at,
    };
    if report.update_channel != assigned_update_channel(state, authenticated).await? {
        return Ok(None);
    }
    directive_for_report(state, authenticated, &report).await
}

async fn directive_for_report(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    report: &AgentRuntimeReport,
) -> Result<Option<UpdateDirective>, ApiError> {
    let row = sqlx::query_as::<_, DirectiveRow>(
        "SELECT p.release_id, p.minimum_version, p.rollout_basis_points, p.paused,
                p.generation, r.version, r.platform, r.architecture, r.archive_name,
                r.archive_size, r.archive_sha256, r.archive_url, r.manifest, r.signature
         FROM update_rollout_policies p
         JOIN update_releases r ON r.id = p.release_id
         WHERE p.network_id = $1 AND p.channel = $2
           AND p.platform = $3 AND p.architecture = $4
           AND r.revoked_at IS NULL",
    )
    .bind(authenticated.network_id)
    .bind(channel_name(report.update_channel))
    .bind(&report.platform)
    .bind(&report.architecture)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_database)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let target_version = row
        .version
        .parse::<ReleaseVersion>()
        .map_err(|_| ApiError::internal())?;
    let minimum_version = row
        .minimum_version
        .as_deref()
        .map(str::parse::<ReleaseVersion>)
        .transpose()
        .map_err(|_| ApiError::internal())?;
    let policy = UpdateRolloutPolicy {
        schema_version: 1,
        release_id: row.release_id,
        channel: report.update_channel,
        target_version,
        minimum_version,
        rollout_basis_points: u16::try_from(row.rollout_basis_points)
            .map_err(|_| ApiError::internal())?,
        paused: row.paused,
    };
    let decision = policy
        .decide(
            authenticated.network_id,
            authenticated.node_id,
            report.update_channel,
            report.agent_version,
        )
        .map_err(|_| ApiError::internal())?;
    if !matches!(
        decision,
        UpdateDecision::Eligible | UpdateDecision::Required
    ) {
        return Ok(None);
    }
    Ok(Some(UpdateDirective {
        schema_version: 1,
        policy_generation: u64::try_from(row.generation).map_err(|_| ApiError::internal())?,
        release_id: row.release_id,
        decision,
        channel: report.update_channel,
        version: target_version,
        minimum_version,
        platform: row.platform,
        architecture: row.architecture,
        archive_name: row.archive_name,
        archive_size: u64::try_from(row.archive_size).map_err(|_| ApiError::internal())?,
        archive_sha256: encode_lower_hex(&row.archive_sha256),
        archive_url: row.archive_url,
        manifest_base64: URL_SAFE_NO_PAD.encode(row.manifest),
        signature_base64: URL_SAFE_NO_PAD.encode(row.signature),
    }))
}

fn validate_runtime_report(
    authenticated: &AuthenticatedNode,
    assigned_channel: UpdateChannel,
    report: &AgentRuntimeReport,
) -> Result<(), ApiError> {
    let age_seconds = Utc::now()
        .signed_duration_since(report.generated_at)
        .num_seconds()
        .unsigned_abs();
    let valid_error = match report.update_state {
        AgentUpdateState::Failed => report
            .last_error_code
            .as_deref()
            .is_some_and(valid_error_code),
        _ => report.last_error_code.is_none(),
    };
    if report.schema_version != 1
        || report.network_id != authenticated.network_id
        || report.node_id_base64 != authenticated.node_id_base64
        || report.update_channel != assigned_channel
        || age_seconds > 300
        || !valid_platform_architecture(&report.platform, &report.architecture)
        || !valid_error
    {
        return Err(ApiError::validation());
    }
    Ok(())
}

async fn assigned_update_channel(
    state: &AppState,
    authenticated: &AuthenticatedNode,
) -> Result<UpdateChannel, ApiError> {
    let channel = sqlx::query_scalar::<_, String>(
        "SELECT update_channel FROM nodes
         WHERE network_id = $1 AND node_id = $2 AND revoked_at IS NULL",
    )
    .bind(authenticated.network_id)
    .bind(authenticated.node_id.as_slice())
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::unauthorized)?;
    parse_channel(&channel).map_err(|_| ApiError::internal())
}

fn valid_platform_architecture(platform: &str, architecture: &str) -> bool {
    matches!(
        (platform, architecture),
        ("linux" | "windows", "x86_64") | ("linux", "aarch64")
    )
}

fn valid_error_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || (index > 0 && (byte.is_ascii_digit() || byte == b'_'))
        })
}

fn valid_revocation_reason(value: &str) -> bool {
    matches!(
        value,
        "build_error" | "key_compromise" | "security_issue" | "superseded" | "withdrawn"
    )
}

const fn update_state_name(state: AgentUpdateState) -> &'static str {
    match state {
        AgentUpdateState::Idle => "idle",
        AgentUpdateState::Downloading => "downloading",
        AgentUpdateState::Staged => "staged",
        AgentUpdateState::Applying => "applying",
        AgentUpdateState::Failed => "failed",
    }
}

fn parse_update_state(value: &str) -> Result<AgentUpdateState, ApiError> {
    match value {
        "idle" => Ok(AgentUpdateState::Idle),
        "downloading" => Ok(AgentUpdateState::Downloading),
        "staged" => Ok(AgentUpdateState::Staged),
        "applying" => Ok(AgentUpdateState::Applying),
        "failed" => Ok(AgentUpdateState::Failed),
        _ => Err(ApiError::internal()),
    }
}

fn policy_response(
    parts: PolicyResponseParts,
    release: ReleaseRow,
) -> Result<UpdatePolicyResponse, ApiError> {
    Ok(UpdatePolicyResponse {
        network_id: parts.network_id,
        channel: parts.channel,
        platform: parts.platform,
        architecture: parts.architecture,
        release: release_response(release)?,
        minimum_version: parts.minimum_version,
        rollout_basis_points: parts.rollout_basis_points,
        paused: parts.paused,
        generation: u64::try_from(parts.generation).map_err(|_| ApiError::internal())?,
        updated_at: parts.updated_at,
    })
}

fn release_response(row: ReleaseRow) -> Result<UpdateReleaseResponse, ApiError> {
    Ok(UpdateReleaseResponse {
        id: row.id,
        version: row.version,
        platform: row.platform,
        architecture: row.architecture,
        target: row.target,
        archive_name: row.archive_name,
        archive_size: u64::try_from(row.archive_size).map_err(|_| ApiError::internal())?,
        archive_sha256: encode_lower_hex(&row.archive_sha256),
        archive_url: row.archive_url,
        revoked_at: row.revoked_at,
        revocation_reason: row.revocation_reason,
        created_at: row.created_at,
    })
}

fn parse_channel(value: &str) -> Result<UpdateChannel, ApiError> {
    match value {
        "stable" => Ok(UpdateChannel::Stable),
        "testing" => Ok(UpdateChannel::Testing),
        "development" => Ok(UpdateChannel::Development),
        _ => Err(ApiError::validation()),
    }
}

const fn channel_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Testing => "testing",
        UpdateChannel::Development => "development",
    }
}

fn validate_platform_architecture(platform: &str, architecture: &str) -> Result<(), ApiError> {
    if platform == "linux" && matches!(architecture, "x86_64" | "aarch64") {
        Ok(())
    } else {
        Err(ApiError::validation())
    }
}

fn validate_archive_url(value: &str, archive_name: &str) -> Result<Url, ApiError> {
    if value.len() > 2048 {
        return Err(ApiError::validation());
    }
    let url = Url::parse(value).map_err(|_| ApiError::validation())?;
    let final_segment = url
        .path_segments()
        .and_then(Iterator::last)
        .ok_or_else(ApiError::validation)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || final_segment != archive_name
    {
        return Err(ApiError::validation());
    }
    Ok(url)
}

fn decode_canonical(value: &str, maximum: usize) -> Result<Vec<u8>, ApiError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::validation())?;
    if decoded.is_empty() || decoded.len() > maximum || URL_SAFE_NO_PAD.encode(&decoded) != value {
        return Err(ApiError::validation());
    }
    Ok(decoded)
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>, ApiError> {
    if value.len() != 64 {
        return Err(ApiError::validation());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0]).ok_or_else(ApiError::validation)?;
            let low = hex_nibble(pair[1]).ok_or_else(ApiError::validation)?;
            Ok((high << 4) | low)
        })
        .collect()
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

async fn append_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor: &ManagementActor,
    network_id: Option<Uuid>,
    action: &str,
    target_type: &str,
    target_id: String,
    metadata: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_events
         (network_id, actor_type, actor_id, action, target_type, target_id, outcome, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, 'success', $7)",
    )
    .bind(network_id)
    .bind(actor.actor_type())
    .bind(actor.actor_id())
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .execute(&mut **transaction)
    .await
    .map_err(internal_database)?;
    Ok(())
}

fn internal_database(error: sqlx::Error) -> ApiError {
    tracing::error!(event = "database_operation_failed", error = %error);
    drop(error);
    ApiError::internal()
}

fn map_write_error(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| matches!(code.as_ref(), "23505" | "23514" | "23503"))
    {
        ApiError::conflict()
    } else {
        internal_database(error)
    }
}
