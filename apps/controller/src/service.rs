use std::{
    collections::{HashMap, HashSet},
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use ipnet::Ipv4Net;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Postgres, Row, Transaction};
use uuid::Uuid;
use xs_core::{SubnetRoutePolicy, validate_subnet_route_suggestion};
use xs_protocol::{
    CredentialClaims, controller_key_id, node_id, role_set_digest, sign_credential,
    verify_credential,
};

use crate::{
    auth::ManagementActor,
    error::ApiError,
    model::{
        AclAction, AclGroupRequest, AclPolicy, AclProtocol, AclRule, AclSelector,
        CandidateAdvertisement, ConfigurationNode, ConfigurationPayload, ConfigurationSubnetRoute,
        CreateEnrollmentTokenRequest, CreateNetworkRequest, EndpointCandidate,
        EndpointCandidateKind, EnrollRequest, EnrollResponse, EnrollmentTokenResponse,
        ExplainAclRequest, ExplainAclResponse, NetworkResponse, PortRange, ReplaceAclPolicyRequest,
        ReplaceAclPolicyResponse, ReplaceSubnetRoutesRequest, ReplaceSubnetRoutesResponse,
        RevokeNodeRequest, RevokeNodeResponse, SignedConfiguration, SubnetRouteAdvertisement,
        SubnetRouteApprovalRequest, SubnetRouteMode, SubnetRouteSuggestionResponse,
    },
    state::AppState,
};

const ENROLLMENT_TOKEN_DOMAIN: &[u8] = b"XS Nexus enrollment token v1";
const CONFIGURATION_DOMAIN: &[u8] = b"XS Nexus configuration v1";
const CANDIDATE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus candidate advertisement v1";
const SUBNET_ROUTE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus subnet route advertisement v1";
const MAX_CANDIDATES_PER_NODE: usize = 16;
const MAX_CANDIDATE_ADVERTISEMENT_BYTES: usize = 16 * 1024;
const MAX_SUBNET_SUGGESTIONS_PER_NODE: usize = 32;
const MAX_SUBNET_ROUTE_ADVERTISEMENT_BYTES: usize = 64 * 1024;

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
struct NetworkListRow {
    id: Uuid,
    name: String,
    address_pool: String,
    reserved_addresses: i32,
    config_version: i64,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct ConfigurationNodeRow {
    database_id: Uuid,
    node_id: Vec<u8>,
    identity_public_key: Vec<u8>,
    virtual_ip: String,
    credential_serial: i64,
    credential_not_after: DateTime<Utc>,
    role_bitmap: i64,
    tags: Vec<String>,
    candidate_payload: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct PolicyNodeRow {
    database_id: Uuid,
    node_id: Vec<u8>,
    virtual_ip: String,
    tags: Vec<String>,
}

struct ConfigurationMetadata {
    version: u64,
    policy_version: u64,
    address_pool: String,
}

struct PersistedSubnetRoute {
    route: ConfigurationSubnetRoute,
    gateway_database_id: Uuid,
    enabled: bool,
}

struct PreparedSubnetRoutes {
    persisted: Vec<PersistedSubnetRoute>,
    enabled: Vec<ConfigurationSubnetRoute>,
}

pub(crate) async fn create_network(
    state: &AppState,
    request: CreateNetworkRequest,
    actor: &ManagementActor,
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
    lock_address_pools(&mut transaction).await?;
    let overlaps: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM networks WHERE address_pool && $1::cidr
         )",
    )
    .bind(pool.to_string())
    .fetch_one(&mut *transaction)
    .await
    .map_err(internal_database)?;
    if overlaps {
        return Err(ApiError::conflict());
    }

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
            actor_type: actor.actor_type(),
            actor_id: actor.actor_id(),
            action: "network.create",
            target_type: "network",
            target_id: Some(network_id.to_string()),
            outcome: "success",
            metadata: json!({"address_pool": pool.to_string()}),
        },
    )
    .await?;

    let configuration = publish_configuration(&mut transaction, network_id, state).await?;
    transaction.commit().await.map_err(internal_database)?;
    state.notify_configuration_changed(network_id);

    Ok(NetworkResponse {
        id: network_id,
        name,
        address_pool: pool.to_string(),
        reserved_addresses: request.reserved_addresses,
        config_version: configuration.version,
        created_at,
    })
}

pub(crate) async fn list_networks(state: &AppState) -> Result<Vec<NetworkResponse>, ApiError> {
    let rows = sqlx::query_as::<_, NetworkListRow>(
        "SELECT id, name, address_pool::text AS address_pool, reserved_addresses,
                config_version, created_at
         FROM networks
         ORDER BY lower(name), id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_database)?;
    rows.into_iter()
        .map(|row| {
            Ok(NetworkResponse {
                id: row.id,
                name: row.name,
                address_pool: row.address_pool,
                reserved_addresses: u32::try_from(row.reserved_addresses)
                    .map_err(|_| ApiError::internal())?,
                config_version: u64::try_from(row.config_version)
                    .map_err(|_| ApiError::internal())?,
                created_at: row.created_at,
            })
        })
        .collect()
}

pub(crate) async fn create_enrollment_token(
    state: &AppState,
    request: CreateEnrollmentTokenRequest,
    actor: &ManagementActor,
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
    .bind(actor.actor_id())
    .execute(&mut *transaction)
    .await;
    if let Err(error) = inserted {
        return Err(map_write_error(error));
    }

    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(request.network_id),
            actor_type: actor.actor_type(),
            actor_id: actor.actor_id(),
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

pub(crate) async fn replace_acl_policy(
    state: &AppState,
    network_id: Uuid,
    request: ReplaceAclPolicyRequest,
    actor: &ManagementActor,
) -> Result<ReplaceAclPolicyResponse, ApiError> {
    if request.expected_policy_version == 0
        || request.groups.len() > 256
        || request.rules.len() > 4096
    {
        return Err(ApiError::validation());
    }
    let groups = canonical_groups(request.groups)?;
    let rules = canonical_rules(request.rules);
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    lock_network(&mut transaction, network_id).await?;
    let network = sqlx::query(
        "SELECT policy_version, address_pool::text AS address_pool
         FROM networks WHERE id = $1 FOR UPDATE",
    )
    .bind(network_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;
    let current_policy_version = u64::try_from(
        network
            .try_get::<i64, _>("policy_version")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;
    let address_pool = network
        .try_get::<String, _>("address_pool")
        .map_err(internal_database)?;
    if current_policy_version != request.expected_policy_version {
        return Err(ApiError::conflict());
    }

    let policy_nodes = load_policy_nodes(&mut transaction, network_id).await?;
    let node_database_ids = policy_nodes
        .iter()
        .map(|node| (URL_SAFE_NO_PAD.encode(&node.node_id), node.database_id))
        .collect::<HashMap<_, _>>();
    let subnet_routes = load_subnet_routes(&mut transaction, network_id).await?;
    validate_policy_references(
        &rules,
        &groups,
        &node_database_ids,
        &policy_nodes,
        &subnet_routes,
    )?;
    let groups_by_node = groups_by_node(&groups, &node_database_ids)?;
    let next_policy_version = current_policy_version
        .checked_add(1)
        .ok_or_else(ApiError::conflict)?;
    let validation_payload = policy_validation_payload(
        network_id,
        next_policy_version,
        &policy_nodes,
        &groups_by_node,
        rules.clone(),
        address_pool,
        subnet_routes,
    );
    AclPolicy::compile(&validation_payload).map_err(|_| ApiError::validation())?;

    persist_acl_policy(
        &mut transaction,
        network_id,
        &groups,
        &rules,
        &node_database_ids,
        next_policy_version,
    )
    .await?;

    let configuration = publish_configuration(&mut transaction, network_id, state).await?;
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(network_id),
            actor_type: actor.actor_type(),
            actor_id: actor.actor_id(),
            action: "acl.replace",
            target_type: "network_acl",
            target_id: Some(network_id.to_string()),
            outcome: "success",
            metadata: json!({
                "policy_version": next_policy_version,
                "rule_count": rules.len(),
                "group_count": groups.len(),
            }),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    state.notify_configuration_changed(network_id);
    Ok(ReplaceAclPolicyResponse {
        network_id,
        policy_version: next_policy_version,
        configuration_version: configuration.version,
    })
}

pub(crate) async fn list_subnet_route_suggestions(
    state: &AppState,
    network_id: Uuid,
) -> Result<Vec<SubnetRouteSuggestionResponse>, ApiError> {
    let rows = sqlx::query(
        "SELECT n.node_id, a.generation, a.payload, a.expires_at
         FROM node_subnet_route_advertisements a
         JOIN nodes n ON n.id = a.node_id AND n.network_id = a.network_id
         WHERE a.network_id = $1
           AND a.expires_at > now()
           AND n.revoked_at IS NULL
         ORDER BY n.node_id",
    )
    .bind(network_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_database)?;
    rows.into_iter()
        .map(|row| {
            let node_id = row
                .try_get::<Vec<u8>, _>("node_id")
                .map_err(internal_database)?;
            let advertisement = serde_json::from_slice::<SubnetRouteAdvertisement>(
                &row.try_get::<Vec<u8>, _>("payload")
                    .map_err(internal_database)?,
            )
            .map_err(|_| ApiError::internal())?;
            let gateway_node_id_base64 = URL_SAFE_NO_PAD.encode(node_id);
            if advertisement.network_id != network_id
                || advertisement.node_id_base64 != gateway_node_id_base64
            {
                return Err(ApiError::internal());
            }
            Ok(SubnetRouteSuggestionResponse {
                gateway_node_id_base64,
                generation: u64::try_from(
                    row.try_get::<i64, _>("generation")
                        .map_err(internal_database)?,
                )
                .map_err(|_| ApiError::internal())?,
                expires_at: row
                    .try_get::<DateTime<Utc>, _>("expires_at")
                    .map_err(internal_database)?,
                suggestions: advertisement.suggestions,
            })
        })
        .collect()
}

pub(crate) async fn replace_subnet_routes(
    state: &AppState,
    network_id: Uuid,
    request: ReplaceSubnetRoutesRequest,
    actor: &ManagementActor,
) -> Result<ReplaceSubnetRoutesResponse, ApiError> {
    if request.expected_configuration_version == 0 || request.routes.len() > 256 {
        return Err(ApiError::validation());
    }
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    lock_network(&mut transaction, network_id).await?;
    let network = sqlx::query(
        "SELECT config_version, policy_version, address_pool::text AS address_pool
         FROM networks
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(network_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;
    let current_configuration_version = u64::try_from(
        network
            .try_get::<i64, _>("config_version")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;
    if current_configuration_version != request.expected_configuration_version {
        return Err(ApiError::conflict());
    }
    let address_pool = network
        .try_get::<String, _>("address_pool")
        .map_err(internal_database)?;
    let policy_version = u64::try_from(
        network
            .try_get::<i64, _>("policy_version")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;
    let prepared =
        prepare_subnet_routes(&mut transaction, network_id, &address_pool, request.routes).await?;

    let nodes = load_configuration_nodes(&mut transaction, network_id).await?;
    let policies = load_acl_policies(&mut transaction, network_id).await?;
    let validation_payload = ConfigurationPayload {
        schema_version: 1,
        network_id,
        version: current_configuration_version.saturating_add(1),
        policy_version,
        generated_at: Utc::now(),
        address_pool: address_pool.clone(),
        discovery_endpoints: state.discovery_public_endpoints.as_ref().clone(),
        nodes,
        relays: state.relays.as_ref().clone(),
        policies,
        subnet_routes: prepared.enabled.clone(),
    };
    SubnetRoutePolicy::compile(&validation_payload).map_err(|_| ApiError::validation())?;
    AclPolicy::compile(&validation_payload).map_err(|_| ApiError::validation())?;

    persist_subnet_routes(&mut transaction, network_id, &prepared.persisted).await?;
    let configuration = publish_configuration(&mut transaction, network_id, state).await?;
    let enabled_count = prepared
        .persisted
        .iter()
        .filter(|route| route.enabled)
        .count();
    let paused_count = prepared.persisted.len().saturating_sub(enabled_count);
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(network_id),
            actor_type: actor.actor_type(),
            actor_id: actor.actor_id(),
            action: "subnet_routes.replace",
            target_type: "network_subnet_routes",
            target_id: Some(network_id.to_string()),
            outcome: "success",
            metadata: json!({
                "enabled_routes": enabled_count,
                "paused_routes": paused_count,
                "configuration_version": configuration.version,
            }),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    state.notify_configuration_changed(network_id);
    Ok(ReplaceSubnetRoutesResponse {
        network_id,
        configuration_version: configuration.version,
        enabled_routes: enabled_count,
        paused_routes: paused_count,
    })
}

async fn prepare_subnet_routes(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    address_pool: &str,
    routes: Vec<SubnetRouteApprovalRequest>,
) -> Result<PreparedSubnetRoutes, ApiError> {
    let policy_nodes = load_policy_nodes(transaction, network_id).await?;
    let node_database_ids = policy_nodes
        .iter()
        .map(|node| (URL_SAFE_NO_PAD.encode(&node.node_id), node.database_id))
        .collect::<HashMap<_, _>>();
    let suggestions = load_valid_subnet_suggestions(transaction, network_id).await?;
    let mut route_ids = HashSet::with_capacity(routes.len());
    let mut active_scopes = HashSet::with_capacity(routes.len());
    let mut persisted = Vec::with_capacity(routes.len());
    let mut enabled = Vec::new();

    for route in routes {
        if !route_ids.insert(route.route_id.clone()) {
            return Err(ApiError::validation());
        }
        let gateway_database_id = node_database_ids
            .get(&route.gateway_node_id_base64)
            .copied()
            .ok_or_else(ApiError::validation)?;
        let prefix =
            validate_subnet_route_suggestion(&route.prefix, &route.interface_name, address_pool)
                .map_err(|_| ApiError::validation())?;
        let scope = (gateway_database_id, prefix, route.interface_name.clone());
        if !suggestions.contains(&scope) || !active_scopes.insert(scope) {
            return Err(ApiError::validation());
        }
        let configuration_route = ConfigurationSubnetRoute {
            route_id: route.route_id,
            prefix: prefix.to_string(),
            gateway_node_id_base64: route.gateway_node_id_base64,
            mode: route.mode,
            interface_name: route.interface_name,
            priority: route.priority,
        };
        if route.enabled {
            enabled.push(configuration_route.clone());
        }
        persisted.push(PersistedSubnetRoute {
            route: configuration_route,
            gateway_database_id,
            enabled: route.enabled,
        });
    }
    canonical_subnet_routes(&mut enabled)?;
    Ok(PreparedSubnetRoutes { persisted, enabled })
}

async fn persist_subnet_routes(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    routes: &[PersistedSubnetRoute],
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE subnet_routes
         SET state = 'revoked', updated_at = now()
         WHERE network_id = $1 AND state != 'revoked'",
    )
    .bind(network_id)
    .execute(&mut **transaction)
    .await
    .map_err(internal_database)?;
    for persisted in routes {
        let route = &persisted.route;
        sqlx::query(
            "INSERT INTO subnet_routes
             (network_id, route_id, gateway_node_id, prefix, interface_name, mode, priority, state)
             VALUES ($1, $2, $3, $4::cidr, $5, $6, $7, $8)
             ON CONFLICT (network_id, route_id) DO UPDATE
             SET gateway_node_id = EXCLUDED.gateway_node_id,
                 prefix = EXCLUDED.prefix,
                 interface_name = EXCLUDED.interface_name,
                 mode = EXCLUDED.mode,
                 priority = EXCLUDED.priority,
                 state = EXCLUDED.state,
                 updated_at = now()",
        )
        .bind(network_id)
        .bind(&route.route_id)
        .bind(persisted.gateway_database_id)
        .bind(&route.prefix)
        .bind(&route.interface_name)
        .bind(subnet_route_mode_name(route.mode))
        .bind(i64::from(route.priority))
        .bind(if persisted.enabled {
            "enabled"
        } else {
            "paused"
        })
        .execute(&mut **transaction)
        .await
        .map_err(map_write_error)?;
    }
    Ok(())
}

async fn load_valid_subnet_suggestions(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<HashSet<(Uuid, Ipv4Net, String)>, ApiError> {
    let rows = sqlx::query(
        "SELECT a.node_id, a.payload
         FROM node_subnet_route_advertisements a
         JOIN nodes n ON n.id = a.node_id AND n.network_id = a.network_id
         WHERE a.network_id = $1
           AND a.expires_at > now()
           AND n.revoked_at IS NULL
         FOR SHARE OF a, n",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;
    let mut suggestions = HashSet::new();
    for row in rows {
        let node_id = row
            .try_get::<Uuid, _>("node_id")
            .map_err(internal_database)?;
        let advertisement = serde_json::from_slice::<SubnetRouteAdvertisement>(
            &row.try_get::<Vec<u8>, _>("payload")
                .map_err(internal_database)?,
        )
        .map_err(|_| ApiError::internal())?;
        if advertisement.network_id != network_id {
            return Err(ApiError::internal());
        }
        for suggestion in advertisement.suggestions {
            let prefix = suggestion
                .prefix
                .parse::<Ipv4Net>()
                .map_err(|_| ApiError::internal())?;
            suggestions.insert((node_id, prefix, suggestion.interface_name));
        }
    }
    Ok(suggestions)
}

fn canonical_subnet_routes(routes: &mut [ConfigurationSubnetRoute]) -> Result<(), ApiError> {
    for route in routes.iter() {
        route
            .prefix
            .parse::<Ipv4Net>()
            .map_err(|_| ApiError::validation())?;
    }
    routes.sort_by(|left, right| {
        let left_prefix = left.prefix.parse::<Ipv4Net>().expect("validated prefix");
        let right_prefix = right.prefix.parse::<Ipv4Net>().expect("validated prefix");
        left_prefix
            .network()
            .cmp(&right_prefix.network())
            .then_with(|| left_prefix.prefix_len().cmp(&right_prefix.prefix_len()))
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.route_id.cmp(&right.route_id))
    });
    Ok(())
}

const fn subnet_route_mode_name(mode: SubnetRouteMode) -> &'static str {
    match mode {
        SubnetRouteMode::Routed => "routed",
        SubnetRouteMode::Nat => "nat",
    }
}

async fn persist_acl_policy(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    groups: &[AclGroupRequest],
    rules: &[AclRule],
    node_database_ids: &HashMap<String, Uuid>,
    policy_version: u64,
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM node_group_memberships WHERE network_id = $1")
        .bind(network_id)
        .execute(&mut **transaction)
        .await
        .map_err(internal_database)?;
    sqlx::query("DELETE FROM node_groups WHERE network_id = $1")
        .bind(network_id)
        .execute(&mut **transaction)
        .await
        .map_err(internal_database)?;
    sqlx::query("DELETE FROM acl_rules WHERE network_id = $1")
        .bind(network_id)
        .execute(&mut **transaction)
        .await
        .map_err(internal_database)?;

    for group in groups {
        sqlx::query("INSERT INTO node_groups (network_id, name) VALUES ($1, $2)")
            .bind(network_id)
            .bind(&group.name)
            .execute(&mut **transaction)
            .await
            .map_err(map_write_error)?;
        for node_id in &group.node_ids_base64 {
            let database_id = node_database_ids
                .get(node_id)
                .copied()
                .ok_or_else(ApiError::validation)?;
            sqlx::query(
                "INSERT INTO node_group_memberships
                 (network_id, group_name, node_id)
                 VALUES ($1, $2, $3)",
            )
            .bind(network_id)
            .bind(&group.name)
            .bind(database_id)
            .execute(&mut **transaction)
            .await
            .map_err(map_write_error)?;
        }
    }
    for rule in rules {
        sqlx::query(
            "INSERT INTO acl_rules
             (network_id, rule_id, priority, action, protocol, sources, destinations,
              destination_ports)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(network_id)
        .bind(&rule.id)
        .bind(i64::from(rule.priority))
        .bind(acl_action_name(rule.action))
        .bind(acl_protocol_name(rule.protocol))
        .bind(serde_json::to_value(&rule.sources).map_err(|_| ApiError::validation())?)
        .bind(serde_json::to_value(&rule.destinations).map_err(|_| ApiError::validation())?)
        .bind(serde_json::to_value(&rule.destination_ports).map_err(|_| ApiError::validation())?)
        .execute(&mut **transaction)
        .await
        .map_err(map_write_error)?;
    }
    sqlx::query("UPDATE networks SET policy_version = $2 WHERE id = $1")
        .bind(network_id)
        .bind(i64::try_from(policy_version).map_err(|_| ApiError::conflict())?)
        .execute(&mut **transaction)
        .await
        .map_err(internal_database)?;
    Ok(())
}

pub(crate) async fn explain_acl(
    state: &AppState,
    network_id: Uuid,
    request: ExplainAclRequest,
) -> Result<ExplainAclResponse, ApiError> {
    if matches!(request.protocol, AclProtocol::Any)
        || matches!(request.protocol, AclProtocol::Tcp | AclProtocol::Udp)
            != request.destination_port.is_some()
    {
        return Err(ApiError::validation());
    }
    decode_canonical_array::<16>(&request.source_node_id_base64)
        .map_err(|()| ApiError::validation())?;
    decode_canonical_array::<16>(&request.destination_node_id_base64)
        .map_err(|()| ApiError::validation())?;
    let payload = latest_configuration_payload(&state.pool, network_id).await?;
    let policy = AclPolicy::compile(&payload).map_err(|_| ApiError::internal())?;
    let source = payload
        .nodes
        .iter()
        .find(|node| node.node_id_base64 == request.source_node_id_base64)
        .ok_or_else(ApiError::not_found)?
        .virtual_ip
        .parse::<Ipv4Addr>()
        .map_err(|_| ApiError::internal())?;
    let destination = payload
        .nodes
        .iter()
        .find(|node| node.node_id_base64 == request.destination_node_id_base64)
        .ok_or_else(ApiError::not_found)?
        .virtual_ip
        .parse::<Ipv4Addr>()
        .map_err(|_| ApiError::internal())?;
    Ok(ExplainAclResponse {
        network_id,
        policy_version: payload.policy_version,
        decision: policy.evaluate(
            source,
            destination,
            request.protocol,
            request.destination_port,
        ),
    })
}

pub(crate) async fn revoke_node(
    state: &AppState,
    network_id: Uuid,
    node_id_base64: &str,
    request: RevokeNodeRequest,
    actor: &ManagementActor,
) -> Result<RevokeNodeResponse, ApiError> {
    if !(60..=604_800).contains(&request.ip_cooldown_seconds) {
        return Err(ApiError::validation());
    }
    let node_id =
        decode_canonical_array::<16>(node_id_base64).map_err(|()| ApiError::validation())?;
    let cooldown_until = Utc::now()
        + Duration::seconds(
            i64::try_from(request.ip_cooldown_seconds).map_err(|_| ApiError::validation())?,
        );
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    lock_network(&mut transaction, network_id).await?;
    let node = sqlx::query(
        "SELECT id, host(virtual_ip) AS virtual_ip
         FROM nodes
         WHERE network_id = $1 AND node_id = $2 AND revoked_at IS NULL
         FOR UPDATE",
    )
    .bind(network_id)
    .bind(node_id.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;
    let database_id = node.try_get::<Uuid, _>("id").map_err(internal_database)?;
    let virtual_ip = node
        .try_get::<String, _>("virtual_ip")
        .map_err(internal_database)?;
    sqlx::query("UPDATE nodes SET revoked_at = now(), updated_at = now() WHERE id = $1")
        .bind(database_id)
        .execute(&mut *transaction)
        .await
        .map_err(internal_database)?;
    sqlx::query(
        "UPDATE ip_leases
         SET state = 'cooling', cooldown_until = $2
         WHERE node_id = $1 AND state = 'active'",
    )
    .bind(database_id)
    .bind(cooldown_until)
    .execute(&mut *transaction)
    .await
    .map_err(internal_database)?;
    let configuration = publish_configuration(&mut transaction, network_id, state).await?;
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(network_id),
            actor_type: actor.actor_type(),
            actor_id: actor.actor_id(),
            action: "node.revoke",
            target_type: "node",
            target_id: Some(database_id.to_string()),
            outcome: "success",
            metadata: json!({
                "node_id_base64": node_id_base64,
                "virtual_ip": virtual_ip,
                "cooldown_until": cooldown_until,
            }),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    state.notify_configuration_changed(network_id);
    Ok(RevokeNodeResponse {
        network_id,
        node_id_base64: node_id_base64.to_owned(),
        virtual_ip,
        cooldown_until,
        configuration_version: configuration.version,
    })
}

fn canonical_groups(mut groups: Vec<AclGroupRequest>) -> Result<Vec<AclGroupRequest>, ApiError> {
    for group in &mut groups {
        if !valid_acl_name(&group.name)
            || group.node_ids_base64.is_empty()
            || group.node_ids_base64.len() > 1024
        {
            return Err(ApiError::validation());
        }
        for node_id in &group.node_ids_base64 {
            decode_canonical_array::<16>(node_id).map_err(|()| ApiError::validation())?;
        }
        group.node_ids_base64.sort();
        if group
            .node_ids_base64
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(ApiError::validation());
        }
    }
    groups.sort_by(|left, right| left.name.cmp(&right.name));
    if groups.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err(ApiError::validation());
    }
    Ok(groups)
}

fn canonical_rules(mut rules: Vec<AclRule>) -> Vec<AclRule> {
    for rule in &mut rules {
        rule.sources.sort_by_key(selector_key);
        rule.destinations.sort_by_key(selector_key);
        rule.destination_ports
            .sort_by_key(|range| (range.start, range.end));
    }
    rules.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.id.cmp(&right.id))
    });
    rules
}

fn selector_key(selector: &AclSelector) -> String {
    match selector {
        AclSelector::Any => "any".to_owned(),
        AclSelector::Node { node_id_base64 } => format!("node:{node_id_base64}"),
        AclSelector::Group { name } => format!("group:{name}"),
        AclSelector::Tag { name } => format!("tag:{name}"),
        AclSelector::Subnet { cidr } => format!("subnet:{cidr}"),
    }
}

fn groups_by_node(
    groups: &[AclGroupRequest],
    node_database_ids: &HashMap<String, Uuid>,
) -> Result<HashMap<Uuid, Vec<String>>, ApiError> {
    let mut result = HashMap::<Uuid, Vec<String>>::new();
    for group in groups {
        for node_id in &group.node_ids_base64 {
            let database_id = node_database_ids
                .get(node_id)
                .copied()
                .ok_or_else(ApiError::validation)?;
            result
                .entry(database_id)
                .or_default()
                .push(group.name.clone());
        }
    }
    Ok(result)
}

fn validate_policy_references(
    rules: &[AclRule],
    groups: &[AclGroupRequest],
    node_database_ids: &HashMap<String, Uuid>,
    nodes: &[PolicyNodeRow],
    subnet_routes: &[ConfigurationSubnetRoute],
) -> Result<(), ApiError> {
    let group_names = groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<HashSet<_>>();
    let tags = nodes
        .iter()
        .flat_map(|node| node.tags.iter().map(String::as_str))
        .collect::<HashSet<_>>();
    let subnets = subnet_routes
        .iter()
        .map(|route| route.prefix.as_str())
        .collect::<HashSet<_>>();
    for selector in rules
        .iter()
        .flat_map(|rule| rule.sources.iter().chain(&rule.destinations))
    {
        let valid = match selector {
            AclSelector::Any => true,
            AclSelector::Node { node_id_base64 } => node_database_ids.contains_key(node_id_base64),
            AclSelector::Group { name } => group_names.contains(name.as_str()),
            AclSelector::Tag { name } => tags.contains(name.as_str()),
            AclSelector::Subnet { cidr } => subnets.contains(cidr.as_str()),
        };
        if !valid {
            return Err(ApiError::validation());
        }
    }
    Ok(())
}

async fn load_policy_nodes(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<Vec<PolicyNodeRow>, ApiError> {
    sqlx::query_as::<_, PolicyNodeRow>(
        "SELECT id AS database_id, node_id, host(virtual_ip) AS virtual_ip, tags
         FROM nodes
         WHERE network_id = $1 AND revoked_at IS NULL
         ORDER BY node_id",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)
}

fn policy_validation_payload(
    network_id: Uuid,
    policy_version: u64,
    nodes: &[PolicyNodeRow],
    groups_by_node: &HashMap<Uuid, Vec<String>>,
    policies: Vec<AclRule>,
    address_pool: String,
    subnet_routes: Vec<ConfigurationSubnetRoute>,
) -> ConfigurationPayload {
    let nodes = nodes
        .iter()
        .map(|node| {
            let mut tags = node.tags.clone();
            tags.sort();
            let mut groups = groups_by_node
                .get(&node.database_id)
                .cloned()
                .unwrap_or_default();
            groups.sort();
            ConfigurationNode {
                node_id_base64: URL_SAFE_NO_PAD.encode(&node.node_id),
                identity_public_key_base64: String::new(),
                virtual_ip: node.virtual_ip.clone(),
                direct_endpoints: Vec::new(),
                candidates: Vec::new(),
                credential_serial: 1,
                credential_not_after: Utc::now(),
                role_bitmap: 0,
                groups,
                tags,
            }
        })
        .collect();
    ConfigurationPayload {
        schema_version: 1,
        network_id,
        version: policy_version,
        policy_version,
        generated_at: Utc::now(),
        address_pool,
        discovery_endpoints: Vec::new(),
        nodes,
        relays: Vec::new(),
        policies,
        subnet_routes,
    }
}

async fn latest_configuration_payload(
    pool: &sqlx::PgPool,
    network_id: Uuid,
) -> Result<ConfigurationPayload, ApiError> {
    let payload = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT payload
         FROM configuration_versions
         WHERE network_id = $1
         ORDER BY version DESC
         LIMIT 1",
    )
    .bind(network_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::not_found)?;
    serde_json::from_slice(&payload).map_err(|_| ApiError::internal())
}

const fn acl_action_name(action: AclAction) -> &'static str {
    match action {
        AclAction::Allow => "allow",
        AclAction::Deny => "deny",
    }
}

const fn acl_protocol_name(protocol: AclProtocol) -> &'static str {
    match protocol {
        AclProtocol::Any => "any",
        AclProtocol::Tcp => "tcp",
        AclProtocol::Udp => "udp",
        AclProtocol::Icmp => "icmp",
    }
}

fn parse_acl_action(value: &str) -> Result<AclAction, ApiError> {
    match value {
        "allow" => Ok(AclAction::Allow),
        "deny" => Ok(AclAction::Deny),
        _ => Err(ApiError::internal()),
    }
}

fn parse_acl_protocol(value: &str) -> Result<AclProtocol, ApiError> {
    match value {
        "any" => Ok(AclProtocol::Any),
        "tcp" => Ok(AclProtocol::Tcp),
        "udp" => Ok(AclProtocol::Udp),
        "icmp" => Ok(AclProtocol::Icmp),
        _ => Err(ApiError::internal()),
    }
}

fn valid_acl_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
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

    let configuration = publish_configuration(&mut transaction, token.network_id, state).await?;
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
    state.notify_configuration_changed(token.network_id);

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

async fn lock_address_pools(transaction: &mut Transaction<'_, Postgres>) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('xs-nexus-address-pools', 0))")
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
         VALUES ($1, $2::inet, $3, 'active')
         ON CONFLICT (network_id, virtual_ip) DO UPDATE
         SET node_id = EXCLUDED.node_id,
             state = 'active',
             cooldown_until = NULL,
             allocated_at = now()
         WHERE ip_leases.state = 'cooling'
           AND ip_leases.cooldown_until <= now()",
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
    pub node_id: [u8; 16],
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
        node_id: claimed_node_id,
    })
}

pub(crate) async fn record_control_connected(
    state: &AppState,
    node_id: &[u8; 16],
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE nodes SET last_control_connected_at = now(), updated_at = now()
         WHERE node_id = $1 AND revoked_at IS NULL",
    )
    .bind(node_id.as_slice())
    .execute(&state.pool)
    .await
    .map_err(internal_database)?;
    Ok(())
}

pub(crate) async fn record_control_disconnected(
    state: &AppState,
    node_id: &[u8; 16],
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE nodes SET last_control_disconnected_at = now(), updated_at = now()
         WHERE node_id = $1",
    )
    .bind(node_id.as_slice())
    .execute(&state.pool)
    .await
    .map_err(internal_database)?;
    Ok(())
}

pub(crate) async fn advertise_candidates(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    advertisement: CandidateAdvertisement,
    signature_base64: &str,
) -> Result<SignedConfiguration, ApiError> {
    validate_candidate_advertisement(authenticated, &advertisement)?;
    let payload = serde_json::to_vec(&advertisement).map_err(|_| ApiError::validation())?;
    if payload.len() > MAX_CANDIDATE_ADVERTISEMENT_BYTES {
        return Err(ApiError::validation());
    }
    let signature = decode_array::<64>(signature_base64).map_err(|()| ApiError::unauthorized())?;
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let node = sqlx::query(
        "SELECT id, identity_public_key
         FROM nodes
         WHERE network_id = $1 AND node_id = $2 AND revoked_at IS NULL
         FOR UPDATE",
    )
    .bind(authenticated.network_id)
    .bind(authenticated.node_id.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::unauthorized)?;
    let node_database_id = node.try_get::<Uuid, _>("id").map_err(internal_database)?;
    let public_key = node
        .try_get::<Vec<u8>, _>("identity_public_key")
        .map_err(internal_database)?;
    let verifying_key = VerifyingKey::from_bytes(
        &public_key
            .as_slice()
            .try_into()
            .map_err(|_| ApiError::internal())?,
    )
    .map_err(|_| ApiError::internal())?;
    let mut signing_input =
        Vec::with_capacity(CANDIDATE_ADVERTISEMENT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(CANDIDATE_ADVERTISEMENT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    verifying_key
        .verify_strict(&signing_input, &Signature::from_bytes(&signature))
        .map_err(|_| ApiError::unauthorized())?;

    if candidate_advertisement_is_duplicate(
        &mut transaction,
        node_database_id,
        advertisement.generation,
        &payload,
        &signature,
    )
    .await?
    {
        transaction.rollback().await.map_err(internal_database)?;
        return latest_configuration(state, authenticated.network_id).await;
    }

    sqlx::query(
        "INSERT INTO node_candidate_advertisements
         (node_id, generation, payload, signature, expires_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (node_id) DO UPDATE
         SET generation = EXCLUDED.generation,
             payload = EXCLUDED.payload,
             signature = EXCLUDED.signature,
             expires_at = EXCLUDED.expires_at,
             updated_at = now()",
    )
    .bind(node_database_id)
    .bind(i64::try_from(advertisement.generation).map_err(|_| ApiError::validation())?)
    .bind(&payload)
    .bind(signature.as_slice())
    .bind(advertisement.expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(map_write_error)?;

    let configuration =
        publish_configuration(&mut transaction, authenticated.network_id, state).await?;
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(authenticated.network_id),
            actor_type: "node",
            actor_id: &authenticated.node_id_base64,
            action: "candidates.advertise",
            target_type: "node",
            target_id: Some(authenticated.node_id_base64.clone()),
            outcome: "success",
            metadata: json!({
                "generation": advertisement.generation,
                "candidate_count": advertisement.candidates.len()
            }),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    state.notify_configuration_changed(authenticated.network_id);
    Ok(configuration)
}

pub(crate) async fn advertise_subnet_routes(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    advertisement: SubnetRouteAdvertisement,
    signature_base64: &str,
) -> Result<SignedConfiguration, ApiError> {
    let payload = serde_json::to_vec(&advertisement).map_err(|_| ApiError::validation())?;
    if payload.len() > MAX_SUBNET_ROUTE_ADVERTISEMENT_BYTES {
        return Err(ApiError::validation());
    }
    let signature = decode_array::<64>(signature_base64).map_err(|()| ApiError::unauthorized())?;
    let mut transaction = state.pool.begin().await.map_err(internal_database)?;
    let node = sqlx::query(
        "SELECT n.id, n.identity_public_key, net.address_pool::text AS address_pool
         FROM nodes n
         JOIN networks net ON net.id = n.network_id
         WHERE n.network_id = $1 AND n.node_id = $2 AND n.revoked_at IS NULL
         FOR UPDATE OF n",
    )
    .bind(authenticated.network_id)
    .bind(authenticated.node_id.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_database)?
    .ok_or_else(ApiError::unauthorized)?;
    let node_database_id = node.try_get::<Uuid, _>("id").map_err(internal_database)?;
    let address_pool = node
        .try_get::<String, _>("address_pool")
        .map_err(internal_database)?;
    validate_subnet_route_advertisement(authenticated, &advertisement, &address_pool)?;
    let public_key = node
        .try_get::<Vec<u8>, _>("identity_public_key")
        .map_err(internal_database)?;
    let verifying_key = VerifyingKey::from_bytes(
        &public_key
            .as_slice()
            .try_into()
            .map_err(|_| ApiError::internal())?,
    )
    .map_err(|_| ApiError::internal())?;
    let mut signing_input =
        Vec::with_capacity(SUBNET_ROUTE_ADVERTISEMENT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(SUBNET_ROUTE_ADVERTISEMENT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    verifying_key
        .verify_strict(&signing_input, &Signature::from_bytes(&signature))
        .map_err(|_| ApiError::unauthorized())?;

    if subnet_route_advertisement_is_duplicate(
        &mut transaction,
        node_database_id,
        advertisement.generation,
        &payload,
        &signature,
    )
    .await?
    {
        transaction.rollback().await.map_err(internal_database)?;
        return latest_configuration(state, authenticated.network_id).await;
    }
    sqlx::query(
        "INSERT INTO node_subnet_route_advertisements
         (node_id, network_id, generation, payload, signature, expires_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, now())
         ON CONFLICT (node_id) DO UPDATE
         SET network_id = EXCLUDED.network_id,
             generation = EXCLUDED.generation,
             payload = EXCLUDED.payload,
             signature = EXCLUDED.signature,
             expires_at = EXCLUDED.expires_at,
             updated_at = now()",
    )
    .bind(node_database_id)
    .bind(authenticated.network_id)
    .bind(i64::try_from(advertisement.generation).map_err(|_| ApiError::validation())?)
    .bind(&payload)
    .bind(signature.as_slice())
    .bind(advertisement.expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(map_write_error)?;
    append_audit(
        &mut transaction,
        AuditEvent {
            network_id: Some(authenticated.network_id),
            actor_type: "node",
            actor_id: &authenticated.node_id_base64,
            action: "subnet_routes.suggest",
            target_type: "node",
            target_id: Some(authenticated.node_id_base64.clone()),
            outcome: "success",
            metadata: json!({
                "generation": advertisement.generation,
                "suggestion_count": advertisement.suggestions.len(),
                "expires_at": advertisement.expires_at,
            }),
        },
    )
    .await?;
    transaction.commit().await.map_err(internal_database)?;
    latest_configuration(state, authenticated.network_id).await
}

async fn subnet_route_advertisement_is_duplicate(
    transaction: &mut Transaction<'_, Postgres>,
    node_id: Uuid,
    generation: u64,
    payload: &[u8],
    signature: &[u8; 64],
) -> Result<bool, ApiError> {
    let current = sqlx::query(
        "SELECT generation, payload, signature
         FROM node_subnet_route_advertisements
         WHERE node_id = $1
         FOR UPDATE",
    )
    .bind(node_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(internal_database)?;
    let Some(current) = current else {
        return Ok(false);
    };
    let current_generation = u64::try_from(
        current
            .try_get::<i64, _>("generation")
            .map_err(internal_database)?,
    )
    .map_err(|_| ApiError::internal())?;
    let duplicate = current_generation == generation
        && current
            .try_get::<Vec<u8>, _>("payload")
            .map_err(internal_database)?
            == payload
        && current
            .try_get::<Vec<u8>, _>("signature")
            .map_err(internal_database)?
            == signature;
    if current_generation >= generation && !duplicate {
        return Err(ApiError::conflict());
    }
    Ok(duplicate)
}

fn validate_subnet_route_advertisement(
    authenticated: &AuthenticatedNode,
    advertisement: &SubnetRouteAdvertisement,
    address_pool: &str,
) -> Result<(), ApiError> {
    let now = Utc::now();
    if advertisement.schema_version != 1
        || advertisement.network_id != authenticated.network_id
        || advertisement.node_id_base64 != authenticated.node_id_base64
        || advertisement.generation == 0
        || advertisement.suggestions.len() > MAX_SUBNET_SUGGESTIONS_PER_NODE
        || advertisement.generated_at < now - Duration::minutes(5)
        || advertisement.generated_at > now + Duration::minutes(5)
        || advertisement.expires_at <= now + Duration::seconds(30)
        || advertisement.expires_at > now + Duration::minutes(15)
        || advertisement.suggestions.windows(2).any(|pair| {
            (&pair[0].prefix, &pair[0].interface_name) >= (&pair[1].prefix, &pair[1].interface_name)
        })
    {
        return Err(ApiError::validation());
    }
    let mut scopes = HashSet::with_capacity(advertisement.suggestions.len());
    for suggestion in &advertisement.suggestions {
        let prefix = validate_subnet_route_suggestion(
            &suggestion.prefix,
            &suggestion.interface_name,
            address_pool,
        )
        .map_err(|_| ApiError::validation())?;
        if !scopes.insert((prefix, suggestion.interface_name.as_str())) {
            return Err(ApiError::validation());
        }
    }
    Ok(())
}

async fn candidate_advertisement_is_duplicate(
    transaction: &mut Transaction<'_, Postgres>,
    node_id: Uuid,
    generation: u64,
    payload: &[u8],
    signature: &[u8; 64],
) -> Result<bool, ApiError> {
    let current = sqlx::query(
        "SELECT generation, payload, signature
         FROM node_candidate_advertisements
         WHERE node_id = $1
         FOR UPDATE",
    )
    .bind(node_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(internal_database)?;
    let Some(current) = current else {
        return Ok(false);
    };
    let current_generation = current
        .try_get::<i64, _>("generation")
        .map_err(internal_database)?;
    let current_generation = u64::try_from(current_generation).map_err(|_| ApiError::internal())?;
    let duplicate = current_generation == generation
        && current
            .try_get::<Vec<u8>, _>("payload")
            .map_err(internal_database)?
            == payload
        && current
            .try_get::<Vec<u8>, _>("signature")
            .map_err(internal_database)?
            == signature;
    if current_generation >= generation && !duplicate {
        return Err(ApiError::conflict());
    }
    Ok(duplicate)
}

fn validate_candidate_advertisement(
    authenticated: &AuthenticatedNode,
    advertisement: &CandidateAdvertisement,
) -> Result<(), ApiError> {
    let now = Utc::now();
    if advertisement.schema_version != 1
        || advertisement.network_id != authenticated.network_id
        || advertisement.node_id_base64 != authenticated.node_id_base64
        || advertisement.generation == 0
        || advertisement.candidates.len() > MAX_CANDIDATES_PER_NODE
        || advertisement.generated_at < now - Duration::minutes(5)
        || advertisement.generated_at > now + Duration::minutes(5)
        || advertisement.expires_at <= now + Duration::seconds(30)
        || advertisement.expires_at > now + Duration::minutes(15)
        || advertisement
            .candidates
            .windows(2)
            .any(|pair| pair[0].priority <= pair[1].priority)
    {
        return Err(ApiError::validation());
    }

    let mut endpoints = HashSet::with_capacity(advertisement.candidates.len());
    let mut priorities = HashSet::with_capacity(advertisement.candidates.len());
    for candidate in &advertisement.candidates {
        if candidate.priority == 0
            || candidate.expires_at <= now
            || candidate.expires_at > advertisement.expires_at
            || candidate.kind == EndpointCandidateKind::Relay
            || !valid_candidate_endpoint(candidate)
            || !endpoints.insert((candidate.kind, candidate.endpoint))
            || !priorities.insert(candidate.priority)
        {
            return Err(ApiError::validation());
        }
    }
    Ok(())
}

fn valid_candidate_endpoint(candidate: &EndpointCandidate) -> bool {
    if candidate.endpoint.port() == 0 {
        return false;
    }
    match candidate.endpoint {
        std::net::SocketAddr::V4(endpoint) => {
            let address = *endpoint.ip();
            if address.is_unspecified()
                || address.is_loopback()
                || address.is_multicast()
                || address == Ipv4Addr::BROADCAST
            {
                return false;
            }
            candidate.kind != EndpointCandidateKind::PublicIpv6
        }
        std::net::SocketAddr::V6(endpoint) => {
            let address = *endpoint.ip();
            if endpoint.flowinfo() != 0
                || address.is_unspecified()
                || address.is_loopback()
                || address.is_multicast()
                || (address.is_unicast_link_local() && endpoint.scope_id() == 0)
                || (!address.is_unicast_link_local() && endpoint.scope_id() != 0)
            {
                return false;
            }
            match candidate.kind {
                EndpointCandidateKind::PublicIpv6 => {
                    public_ipv6(address) && endpoint.scope_id() == 0
                }
                EndpointCandidateKind::Local
                | EndpointCandidateKind::Mapped
                | EndpointCandidateKind::Static => true,
                EndpointCandidateKind::Relay => false,
            }
        }
    }
}

fn public_ipv6(address: Ipv6Addr) -> bool {
    !address.is_unique_local() && !address.is_unicast_link_local()
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
    state: &AppState,
) -> Result<SignedConfiguration, ApiError> {
    let metadata = next_configuration_metadata(transaction, network_id).await?;
    let nodes = load_configuration_nodes(transaction, network_id).await?;
    let policies = load_acl_policies(transaction, network_id).await?;
    let subnet_routes = load_subnet_routes(transaction, network_id).await?;
    let configuration_payload = ConfigurationPayload {
        schema_version: 1,
        network_id,
        version: metadata.version,
        policy_version: metadata.policy_version,
        generated_at: Utc::now(),
        address_pool: metadata.address_pool,
        discovery_endpoints: state.discovery_public_endpoints.as_ref().clone(),
        nodes,
        relays: state.relays.as_ref().clone(),
        policies,
        subnet_routes,
    };
    AclPolicy::compile(&configuration_payload).map_err(|_| ApiError::internal())?;
    let (payload, signature, key_id) = encode_configuration(state, &configuration_payload)?;
    persist_configuration(
        transaction,
        network_id,
        metadata.version,
        &payload,
        &signature,
        key_id,
    )
    .await?;

    Ok(SignedConfiguration {
        version: metadata.version,
        payload_base64: URL_SAFE_NO_PAD.encode(payload),
        signature_base64: URL_SAFE_NO_PAD.encode(signature),
        signer_key_id: key_id,
    })
}

async fn next_configuration_metadata(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<ConfigurationMetadata, ApiError> {
    let network = sqlx::query(
        "UPDATE networks
         SET config_version = config_version + 1
         WHERE id = $1
         RETURNING config_version, policy_version, address_pool::text AS address_pool",
    )
    .bind(network_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal_database)?;
    Ok(ConfigurationMetadata {
        version: u64::try_from(
            network
                .try_get::<i64, _>("config_version")
                .map_err(internal_database)?,
        )
        .map_err(|_| ApiError::internal())?,
        address_pool: network
            .try_get::<String, _>("address_pool")
            .map_err(internal_database)?,
        policy_version: u64::try_from(
            network
                .try_get::<i64, _>("policy_version")
                .map_err(internal_database)?,
        )
        .map_err(|_| ApiError::internal())?,
    })
}

async fn load_configuration_nodes(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<Vec<ConfigurationNode>, ApiError> {
    let rows = sqlx::query_as::<_, ConfigurationNodeRow>(
        "SELECT n.id AS database_id, n.node_id, n.identity_public_key,
                host(n.virtual_ip) AS virtual_ip,
                n.credential_serial, n.credential_not_after, n.role_bitmap, n.tags,
                c.payload AS candidate_payload
         FROM nodes n
         LEFT JOIN node_candidate_advertisements c
           ON c.node_id = n.id AND c.expires_at > now()
         WHERE n.network_id = $1 AND n.revoked_at IS NULL
         ORDER BY n.node_id",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;

    let group_rows = sqlx::query(
        "SELECT node_id, group_name
         FROM node_group_memberships
         WHERE network_id = $1
         ORDER BY node_id, group_name",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;
    let mut groups_by_node = HashMap::<Uuid, Vec<String>>::new();
    for row in group_rows {
        groups_by_node
            .entry(
                row.try_get::<Uuid, _>("node_id")
                    .map_err(internal_database)?,
            )
            .or_default()
            .push(
                row.try_get::<String, _>("group_name")
                    .map_err(internal_database)?,
            );
    }

    rows.into_iter()
        .map(|mut row| {
            let candidates = row
                .candidate_payload
                .as_deref()
                .map(|payload| {
                    serde_json::from_slice::<CandidateAdvertisement>(payload)
                        .map(|advertisement| advertisement.candidates)
                        .map_err(|_| ApiError::internal())
                })
                .transpose()?
                .unwrap_or_default();
            row.tags.sort();
            Ok(ConfigurationNode {
                node_id_base64: URL_SAFE_NO_PAD.encode(row.node_id),
                identity_public_key_base64: URL_SAFE_NO_PAD.encode(row.identity_public_key),
                virtual_ip: row.virtual_ip,
                direct_endpoints: Vec::new(),
                candidates,
                credential_serial: u64::try_from(row.credential_serial)
                    .map_err(|_| ApiError::internal())?,
                credential_not_after: row.credential_not_after,
                role_bitmap: u32::try_from(row.role_bitmap).map_err(|_| ApiError::internal())?,
                groups: groups_by_node.remove(&row.database_id).unwrap_or_default(),
                tags: row.tags,
            })
        })
        .collect()
}

async fn load_acl_policies(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<Vec<AclRule>, ApiError> {
    let rows = sqlx::query(
        "SELECT rule_id, priority, action, protocol, sources, destinations,
                destination_ports
         FROM acl_rules
         WHERE network_id = $1
         ORDER BY priority DESC, rule_id",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;
    rows.into_iter()
        .map(|row| {
            Ok(AclRule {
                id: row
                    .try_get::<String, _>("rule_id")
                    .map_err(internal_database)?,
                priority: u32::try_from(
                    row.try_get::<i64, _>("priority")
                        .map_err(internal_database)?,
                )
                .map_err(|_| ApiError::internal())?,
                action: parse_acl_action(
                    &row.try_get::<String, _>("action")
                        .map_err(internal_database)?,
                )?,
                protocol: parse_acl_protocol(
                    &row.try_get::<String, _>("protocol")
                        .map_err(internal_database)?,
                )?,
                sources: serde_json::from_value(
                    row.try_get::<serde_json::Value, _>("sources")
                        .map_err(internal_database)?,
                )
                .map_err(|_| ApiError::internal())?,
                destinations: serde_json::from_value(
                    row.try_get::<serde_json::Value, _>("destinations")
                        .map_err(internal_database)?,
                )
                .map_err(|_| ApiError::internal())?,
                destination_ports: serde_json::from_value::<Vec<PortRange>>(
                    row.try_get::<serde_json::Value, _>("destination_ports")
                        .map_err(internal_database)?,
                )
                .map_err(|_| ApiError::internal())?,
            })
        })
        .collect()
}

async fn load_subnet_routes(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
) -> Result<Vec<ConfigurationSubnetRoute>, ApiError> {
    let rows = sqlx::query(
        "SELECT r.route_id, r.prefix::text AS prefix, n.node_id,
                r.interface_name, r.mode, r.priority
         FROM subnet_routes r
         JOIN nodes n
           ON n.network_id = r.network_id AND n.id = r.gateway_node_id
         WHERE r.network_id = $1
           AND r.state = 'enabled'
           AND n.revoked_at IS NULL
         ORDER BY network(r.prefix), masklen(r.prefix), r.priority DESC, r.route_id",
    )
    .bind(network_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal_database)?;
    rows.into_iter()
        .map(|row| {
            Ok(ConfigurationSubnetRoute {
                route_id: row
                    .try_get::<String, _>("route_id")
                    .map_err(internal_database)?,
                prefix: row
                    .try_get::<String, _>("prefix")
                    .map_err(internal_database)?,
                gateway_node_id_base64: URL_SAFE_NO_PAD.encode(
                    row.try_get::<Vec<u8>, _>("node_id")
                        .map_err(internal_database)?,
                ),
                interface_name: row
                    .try_get::<String, _>("interface_name")
                    .map_err(internal_database)?,
                mode: parse_subnet_route_mode(
                    &row.try_get::<String, _>("mode")
                        .map_err(internal_database)?,
                )?,
                priority: u32::try_from(
                    row.try_get::<i64, _>("priority")
                        .map_err(internal_database)?,
                )
                .map_err(|_| ApiError::internal())?,
            })
        })
        .collect()
}

fn parse_subnet_route_mode(value: &str) -> Result<SubnetRouteMode, ApiError> {
    match value {
        "routed" => Ok(SubnetRouteMode::Routed),
        "nat" => Ok(SubnetRouteMode::Nat),
        _ => Err(ApiError::internal()),
    }
}

fn encode_configuration(
    state: &AppState,
    configuration_payload: &ConfigurationPayload,
) -> Result<(Vec<u8>, [u8; 64], u32), ApiError> {
    let payload = serde_json::to_vec(configuration_payload).map_err(|_| ApiError::internal())?;
    if payload.len() > 1_048_576 {
        return Err(ApiError::internal());
    }

    let mut signing_input = Vec::with_capacity(CONFIGURATION_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(CONFIGURATION_DOMAIN);
    signing_input.extend_from_slice(&payload);
    let signature = state.config_signing_key.sign(&signing_input).to_bytes();
    let key_id = controller_key_id(&state.config_signing_key.verifying_key());
    Ok((payload, signature, key_id))
}

async fn persist_configuration(
    transaction: &mut Transaction<'_, Postgres>,
    network_id: Uuid,
    version: u64,
    payload: &[u8],
    signature: &[u8; 64],
    key_id: u32,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO configuration_versions
         (network_id, version, payload, signature, signer_key_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(network_id)
    .bind(i64::try_from(version).map_err(|_| ApiError::internal())?)
    .bind(payload)
    .bind(signature.as_slice())
    .bind(i64::from(key_id))
    .execute(&mut **transaction)
    .await
    .map_err(map_write_error)?;
    Ok(())
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

fn decode_canonical_array<const LENGTH: usize>(encoded: &str) -> Result<[u8; LENGTH], ()> {
    let decoded = decode_array::<LENGTH>(encoded)?;
    if URL_SAFE_NO_PAD.encode(decoded) == encoded {
        Ok(decoded)
    } else {
        Err(())
    }
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
