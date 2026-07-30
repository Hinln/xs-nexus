use std::collections::{HashMap, HashSet};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;
use xs_core::{CandidateAdvertisement, SubnetRouteAdvertisement};
use xs_protocol::controller_key_id;

use crate::{error::ApiError, state::AppState};

#[derive(Serialize)]
pub(crate) struct ConsoleSnapshot {
    collected_at: DateTime<Utc>,
    dashboard: DashboardSummary,
    networks: Vec<NetworkSummary>,
    nodes: Vec<NodeSummary>,
    enrollment_tokens: Vec<EnrollmentTokenSummary>,
    groups: Vec<GroupSummary>,
    acl_rules: Vec<AclRuleSummary>,
    subnet_route_suggestions: Vec<SubnetRouteSuggestionSummary>,
    subnet_routes: Vec<SubnetRouteSummary>,
    relays: Vec<RelaySummary>,
    topology: TopologySummary,
    audit_events: Vec<AuditEventSummary>,
    alerts: Vec<AlertSummary>,
    system: SystemSummary,
}

#[derive(Clone, Serialize)]
struct Availability<T> {
    status: &'static str,
    value: Option<T>,
    reason: Option<&'static str>,
}

impl<T> Availability<T> {
    const fn available(value: T) -> Self {
        Self {
            status: "available",
            value: Some(value),
            reason: None,
        }
    }

    const fn unavailable(reason: &'static str) -> Self {
        Self {
            status: "unavailable",
            value: None,
            reason: Some(reason),
        }
    }
}

#[derive(Serialize)]
struct DashboardSummary {
    online_nodes: usize,
    offline_nodes: usize,
    direct_nodes: Availability<usize>,
    relay_nodes: Availability<usize>,
    relay_health: Availability<&'static str>,
    traffic_bytes_24h: Availability<u64>,
    connection_success_percent_24h: Availability<f64>,
    average_latency_ms_24h: Availability<f64>,
    pending_route_suggestions: usize,
    security_alerts_24h: u64,
    recent_activity: Vec<AuditEventSummary>,
}

#[derive(Serialize)]
struct NetworkSummary {
    id: Uuid,
    name: String,
    address_pool: String,
    reserved_addresses: u32,
    configuration_version: u64,
    policy_version: u64,
    active_nodes: usize,
    online_nodes: usize,
    active_leases: u64,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct NodeSummary {
    id: Uuid,
    network_id: Uuid,
    network_name: String,
    node_id_base64: String,
    name: String,
    virtual_ip: String,
    device_type: String,
    architecture: Availability<String>,
    agent_version: Availability<String>,
    public_endpoint: Option<String>,
    local_endpoints: Vec<String>,
    current_path: Availability<String>,
    relay: Availability<String>,
    latency_ms: Availability<f64>,
    traffic_bytes_24h: Availability<u64>,
    state: &'static str,
    last_seen_at: Option<DateTime<Utc>>,
    groups: Vec<String>,
    tags: Vec<String>,
    published_subnets: Vec<String>,
    credential_expires_at: DateTime<Utc>,
    credential_state: &'static str,
    update_state: Availability<String>,
}

#[derive(Serialize)]
struct EnrollmentTokenSummary {
    id: Uuid,
    network_id: Uuid,
    network_name: String,
    expires_at: DateTime<Utc>,
    max_uses: u32,
    use_count: u32,
    remaining_uses: u32,
    state: &'static str,
    default_role_bitmap: u32,
    default_tags: Vec<String>,
    requested_virtual_ip: Option<String>,
    created_at: DateTime<Utc>,
    created_by: String,
}

#[derive(Serialize)]
struct GroupSummary {
    network_id: Uuid,
    name: String,
    node_ids_base64: Vec<String>,
}

#[derive(Serialize)]
struct AclRuleSummary {
    network_id: Uuid,
    network_name: String,
    rule_id: String,
    priority: u32,
    action: String,
    protocol: String,
    sources: Value,
    destinations: Value,
    destination_ports: Value,
}

#[derive(Serialize)]
struct SubnetRouteSuggestionSummary {
    network_id: Uuid,
    gateway_node_id_base64: String,
    gateway_name: String,
    generation: u64,
    expires_at: DateTime<Utc>,
    suggestions: Vec<xs_core::SubnetRouteSuggestion>,
}

#[derive(Clone, Serialize)]
struct SubnetRouteSummary {
    network_id: Uuid,
    network_name: String,
    route_id: String,
    gateway_node_id_base64: String,
    gateway_name: String,
    prefix: String,
    interface_name: String,
    mode: String,
    priority: u32,
    state: String,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct RelaySummary {
    relay_id_base64: String,
    endpoint: String,
    priority: u32,
    expires_at: DateTime<Utc>,
    health: Availability<&'static str>,
    metrics: Availability<Value>,
}

#[derive(Serialize)]
struct TopologySummary {
    nodes: Vec<TopologyNode>,
    links: Vec<TopologyLink>,
    subnets: Vec<TopologySubnet>,
    link_telemetry: Availability<&'static str>,
}

#[derive(Serialize)]
struct TopologyNode {
    node_id_base64: String,
    name: String,
    virtual_ip: String,
    state: &'static str,
}

#[derive(Serialize)]
struct TopologyLink {
    source_node_id_base64: String,
    destination_node_id_base64: String,
    path: String,
    latency_ms: Option<f64>,
}

#[derive(Serialize)]
struct TopologySubnet {
    gateway_node_id_base64: String,
    prefix: String,
    state: String,
}

#[derive(Clone, Serialize)]
struct AuditEventSummary {
    id: i64,
    occurred_at: DateTime<Utc>,
    network_id: Option<Uuid>,
    actor_type: String,
    actor_id: String,
    action: String,
    target_type: String,
    target_id: Option<String>,
    outcome: String,
    metadata: Value,
}

#[derive(Serialize)]
struct AlertSummary {
    audit_event_id: i64,
    occurred_at: DateTime<Utc>,
    severity: &'static str,
    action: String,
    outcome: String,
    reason_class: Option<String>,
}

#[derive(Serialize)]
struct SystemSummary {
    database: Availability<&'static str>,
    credential_signing_key_id: u32,
    configuration_signing_key_id: u32,
    update_management: Availability<&'static str>,
    backup_restore: Availability<&'static str>,
    relay_metrics: Availability<&'static str>,
    path_telemetry: Availability<&'static str>,
}

#[derive(FromRow)]
struct NetworkRow {
    id: Uuid,
    name: String,
    address_pool: String,
    reserved_addresses: i32,
    config_version: i64,
    policy_version: i64,
    active_leases: i64,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct NodeRow {
    id: Uuid,
    network_id: Uuid,
    network_name: String,
    node_id: Vec<u8>,
    name: String,
    device_type: String,
    virtual_ip: String,
    tags: Vec<String>,
    credential_not_after: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    last_control_connected_at: Option<DateTime<Utc>>,
    last_control_disconnected_at: Option<DateTime<Utc>>,
    candidate_payload: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct MembershipRow {
    network_id: Uuid,
    group_name: String,
    node_database_id: Uuid,
    node_id: Vec<u8>,
}

#[derive(FromRow)]
struct TokenRow {
    id: Uuid,
    network_id: Uuid,
    network_name: String,
    expires_at: DateTime<Utc>,
    max_uses: i32,
    use_count: i32,
    default_role_bitmap: i64,
    default_tags: Vec<String>,
    requested_virtual_ip: Option<String>,
    revoked_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    created_by: String,
}

#[derive(FromRow)]
struct AclRow {
    network_id: Uuid,
    network_name: String,
    rule_id: String,
    priority: i64,
    action: String,
    protocol: String,
    sources: Value,
    destinations: Value,
    destination_ports: Value,
}

#[derive(FromRow)]
struct SuggestionRow {
    network_id: Uuid,
    gateway_node_id: Vec<u8>,
    gateway_name: String,
    payload: Vec<u8>,
}

#[derive(FromRow)]
struct RouteRow {
    network_id: Uuid,
    network_name: String,
    route_id: String,
    gateway_node_id: Vec<u8>,
    gateway_name: String,
    prefix: String,
    interface_name: String,
    mode: String,
    priority: i64,
    state: String,
    updated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct AuditRow {
    id: i64,
    occurred_at: DateTime<Utc>,
    network_id: Option<Uuid>,
    actor_type: String,
    actor_id: String,
    action: String,
    target_type: String,
    target_id: Option<String>,
    outcome: String,
    metadata: Value,
}

pub(crate) async fn snapshot(state: &AppState) -> Result<ConsoleSnapshot, ApiError> {
    let collected_at = Utc::now();
    let online_node_ids: HashSet<[u8; 16]> = state.online_node_ids().await.into_iter().collect();
    let network_rows = load_networks(state).await?;
    let node_rows = load_nodes(state).await?;
    let memberships = load_memberships(state).await?;
    let routes = load_routes(state).await?;
    let suggestions = load_suggestions(state).await?;
    let audit_events = load_audit_events(state).await?;
    let groups = groups_from_memberships(&memberships)?;
    let nodes = nodes_from_rows(
        node_rows,
        &online_node_ids,
        &memberships,
        &routes,
        collected_at,
    )?;
    let networks = networks_from_rows(network_rows, &nodes)?;
    let enrollment_tokens = load_tokens(state, collected_at).await?;
    let acl_rules = load_acl_rules(state).await?;
    let security_alerts_24h = load_security_alert_count(state).await?;
    let dashboard = dashboard_summary(
        &nodes,
        &suggestions,
        &routes,
        &audit_events,
        security_alerts_24h,
    )?;
    let relays = relay_summaries(state);
    let topology = topology_summary(&nodes, &routes);
    let alerts = alerts_from_audit(&audit_events);
    let system = system_summary(state);

    Ok(ConsoleSnapshot {
        collected_at,
        dashboard,
        networks,
        nodes,
        enrollment_tokens,
        groups,
        acl_rules,
        subnet_route_suggestions: suggestions,
        subnet_routes: routes,
        relays,
        topology,
        audit_events,
        alerts,
        system,
    })
}

fn dashboard_summary(
    nodes: &[NodeSummary],
    suggestions: &[SubnetRouteSuggestionSummary],
    routes: &[SubnetRouteSummary],
    audit_events: &[AuditEventSummary],
    security_alerts_24h: i64,
) -> Result<DashboardSummary, ApiError> {
    Ok(DashboardSummary {
        online_nodes: nodes.iter().filter(|node| node.state == "online").count(),
        offline_nodes: nodes.iter().filter(|node| node.state == "offline").count(),
        direct_nodes: Availability::unavailable("Agent 尚未上报当前路径遥测"),
        relay_nodes: Availability::unavailable("Agent 尚未上报当前路径遥测"),
        relay_health: Availability::unavailable("Relay 目录未包含健康指标端点"),
        traffic_bytes_24h: Availability::unavailable("Agent 尚未上报流量遥测"),
        connection_success_percent_24h: Availability::unavailable("Agent 尚未上报连接结果遥测"),
        average_latency_ms_24h: Availability::unavailable("Agent 尚未上报路径延迟遥测"),
        pending_route_suggestions: count_pending_suggestions(suggestions, routes),
        security_alerts_24h: u64::try_from(security_alerts_24h)
            .map_err(|_| ApiError::internal())?,
        recent_activity: audit_events.iter().take(12).cloned().collect(),
    })
}

fn relay_summaries(state: &AppState) -> Vec<RelaySummary> {
    state
        .relays
        .iter()
        .map(|relay| RelaySummary {
            relay_id_base64: relay.relay_id_base64.clone(),
            endpoint: relay.endpoint.to_string(),
            priority: relay.priority,
            expires_at: relay.expires_at,
            health: Availability::unavailable("Relay 健康指标端点未配置"),
            metrics: Availability::unavailable("Relay 指标端点未配置"),
        })
        .collect()
}

fn topology_summary(nodes: &[NodeSummary], routes: &[SubnetRouteSummary]) -> TopologySummary {
    TopologySummary {
        nodes: nodes
            .iter()
            .map(|node| TopologyNode {
                node_id_base64: node.node_id_base64.clone(),
                name: node.name.clone(),
                virtual_ip: node.virtual_ip.clone(),
                state: node.state,
            })
            .collect(),
        links: Vec::new(),
        subnets: routes
            .iter()
            .map(|route| TopologySubnet {
                gateway_node_id_base64: route.gateway_node_id_base64.clone(),
                prefix: route.prefix.clone(),
                state: route.state.clone(),
            })
            .collect(),
        link_telemetry: Availability::unavailable("Agent 尚未上报已选链路"),
    }
}

fn alerts_from_audit(audit_events: &[AuditEventSummary]) -> Vec<AlertSummary> {
    audit_events
        .iter()
        .filter(|event| event.outcome != "success")
        .map(|event| AlertSummary {
            audit_event_id: event.id,
            occurred_at: event.occurred_at,
            severity: if event.outcome == "failure" {
                "high"
            } else {
                "medium"
            },
            action: event.action.clone(),
            outcome: event.outcome.clone(),
            reason_class: event
                .metadata
                .get("reason_class")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect()
}

fn system_summary(state: &AppState) -> SystemSummary {
    SystemSummary {
        database: Availability::available("ok"),
        credential_signing_key_id: controller_key_id(&state.credential_signing_key.verifying_key()),
        configuration_signing_key_id: controller_key_id(&state.config_signing_key.verifying_key()),
        update_management: Availability::unavailable("更新签名与发布通道将在 M6.1 实现"),
        backup_restore: Availability::unavailable("备份恢复流程将在 M8.1 实现"),
        relay_metrics: Availability::unavailable("Relay 指标端点未配置"),
        path_telemetry: Availability::unavailable("Agent 路径遥测尚未接入控制面"),
    }
}

async fn load_security_alert_count(state: &AppState) -> Result<i64, ApiError> {
    sqlx::query_scalar(
        "SELECT count(*) FROM audit_events
         WHERE occurred_at >= now() - interval '24 hours' AND outcome != 'success'",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_networks(state: &AppState) -> Result<Vec<NetworkRow>, ApiError> {
    sqlx::query_as::<_, NetworkRow>(
        "SELECT n.id, n.name, n.address_pool::text AS address_pool,
                n.reserved_addresses, n.config_version, n.policy_version,
                count(l.node_id) FILTER (WHERE l.state = 'active') AS active_leases,
                n.created_at
         FROM networks n
         LEFT JOIN ip_leases l ON l.network_id = n.id
         GROUP BY n.id
         ORDER BY lower(n.name), n.id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_nodes(state: &AppState) -> Result<Vec<NodeRow>, ApiError> {
    sqlx::query_as::<_, NodeRow>(
        "SELECT n.id, n.network_id, w.name AS network_name, n.node_id, n.name,
                n.device_type, n.virtual_ip::text AS virtual_ip, n.tags,
                n.credential_not_after, n.revoked_at, n.last_control_connected_at,
                n.last_control_disconnected_at, c.payload AS candidate_payload
         FROM nodes n
         JOIN networks w ON w.id = n.network_id
         LEFT JOIN node_candidate_advertisements c
           ON c.node_id = n.id AND c.expires_at > now()
         ORDER BY lower(n.name), n.id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_memberships(state: &AppState) -> Result<Vec<MembershipRow>, ApiError> {
    sqlx::query_as::<_, MembershipRow>(
        "SELECT m.network_id, m.group_name, m.node_id AS node_database_id, n.node_id
         FROM node_group_memberships m
         JOIN nodes n ON n.id = m.node_id
         ORDER BY m.network_id, m.group_name, n.node_id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)
}

async fn load_tokens(
    state: &AppState,
    now: DateTime<Utc>,
) -> Result<Vec<EnrollmentTokenSummary>, ApiError> {
    let rows = sqlx::query_as::<_, TokenRow>(
        "SELECT t.id, t.network_id, n.name AS network_name, t.expires_at,
                t.max_uses, t.use_count, t.default_role_bitmap, t.default_tags,
                t.requested_virtual_ip::text AS requested_virtual_ip, t.revoked_at,
                t.created_at, t.created_by
         FROM enrollment_tokens t
         JOIN networks n ON n.id = t.network_id
         ORDER BY t.created_at DESC
         LIMIT 1000",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(|row| {
            let max_uses = u32::try_from(row.max_uses).map_err(|_| ApiError::internal())?;
            let use_count = u32::try_from(row.use_count).map_err(|_| ApiError::internal())?;
            let state = if row.revoked_at.is_some() {
                "revoked"
            } else if row.expires_at <= now {
                "expired"
            } else if use_count >= max_uses {
                "exhausted"
            } else {
                "active"
            };
            Ok(EnrollmentTokenSummary {
                id: row.id,
                network_id: row.network_id,
                network_name: row.network_name,
                expires_at: row.expires_at,
                max_uses,
                use_count,
                remaining_uses: max_uses.saturating_sub(use_count),
                state,
                default_role_bitmap: u32::try_from(row.default_role_bitmap)
                    .map_err(|_| ApiError::internal())?,
                default_tags: row.default_tags,
                requested_virtual_ip: row.requested_virtual_ip,
                created_at: row.created_at,
                created_by: row.created_by,
            })
        })
        .collect()
}

async fn load_acl_rules(state: &AppState) -> Result<Vec<AclRuleSummary>, ApiError> {
    let rows = sqlx::query_as::<_, AclRow>(
        "SELECT r.network_id, n.name AS network_name, r.rule_id, r.priority,
                r.action, r.protocol, r.sources, r.destinations, r.destination_ports
         FROM acl_rules r
         JOIN networks n ON n.id = r.network_id
         ORDER BY n.name, r.priority DESC, r.rule_id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(|row| {
            Ok(AclRuleSummary {
                network_id: row.network_id,
                network_name: row.network_name,
                rule_id: row.rule_id,
                priority: u32::try_from(row.priority).map_err(|_| ApiError::internal())?,
                action: row.action,
                protocol: row.protocol,
                sources: row.sources,
                destinations: row.destinations,
                destination_ports: row.destination_ports,
            })
        })
        .collect()
}

async fn load_suggestions(state: &AppState) -> Result<Vec<SubnetRouteSuggestionSummary>, ApiError> {
    let rows = sqlx::query_as::<_, SuggestionRow>(
        "SELECT a.network_id, n.node_id AS gateway_node_id, n.name AS gateway_name,
                a.payload
         FROM node_subnet_route_advertisements a
         JOIN nodes n ON n.id = a.node_id
         WHERE a.expires_at > now() AND n.revoked_at IS NULL
         ORDER BY n.name, n.node_id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(|row| {
            let advertisement: SubnetRouteAdvertisement =
                serde_json::from_slice(&row.payload).map_err(|_| ApiError::internal())?;
            Ok(SubnetRouteSuggestionSummary {
                network_id: row.network_id,
                gateway_node_id_base64: encode_node_id(&row.gateway_node_id)?,
                gateway_name: row.gateway_name,
                generation: advertisement.generation,
                expires_at: advertisement.expires_at,
                suggestions: advertisement.suggestions,
            })
        })
        .collect()
}

async fn load_routes(state: &AppState) -> Result<Vec<SubnetRouteSummary>, ApiError> {
    let rows = sqlx::query_as::<_, RouteRow>(
        "SELECT r.network_id, w.name AS network_name, r.route_id,
                n.node_id AS gateway_node_id, n.name AS gateway_name,
                r.prefix::text AS prefix, r.interface_name, r.mode, r.priority,
                r.state, r.updated_at
         FROM subnet_routes r
         JOIN networks w ON w.id = r.network_id
         JOIN nodes n ON n.id = r.gateway_node_id
         ORDER BY w.name, r.prefix, r.priority DESC, r.route_id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(|row| {
            Ok(SubnetRouteSummary {
                network_id: row.network_id,
                network_name: row.network_name,
                route_id: row.route_id,
                gateway_node_id_base64: encode_node_id(&row.gateway_node_id)?,
                gateway_name: row.gateway_name,
                prefix: row.prefix,
                interface_name: row.interface_name,
                mode: row.mode,
                priority: u32::try_from(row.priority).map_err(|_| ApiError::internal())?,
                state: row.state,
                updated_at: row.updated_at,
            })
        })
        .collect()
}

async fn load_audit_events(state: &AppState) -> Result<Vec<AuditEventSummary>, ApiError> {
    let rows = sqlx::query_as::<_, AuditRow>(
        "SELECT id, occurred_at, network_id, actor_type, actor_id, action,
                target_type, target_id, outcome, metadata
         FROM audit_events
         ORDER BY occurred_at DESC, id DESC
         LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    Ok(rows
        .into_iter()
        .map(|row| AuditEventSummary {
            id: row.id,
            occurred_at: row.occurred_at,
            network_id: row.network_id,
            actor_type: row.actor_type,
            actor_id: row.actor_id,
            action: row.action,
            target_type: row.target_type,
            target_id: row.target_id,
            outcome: row.outcome,
            metadata: row.metadata,
        })
        .collect())
}

fn groups_from_memberships(memberships: &[MembershipRow]) -> Result<Vec<GroupSummary>, ApiError> {
    let mut groups = HashMap::<(Uuid, String), Vec<String>>::new();
    for membership in memberships {
        groups
            .entry((membership.network_id, membership.group_name.clone()))
            .or_default()
            .push(encode_node_id(&membership.node_id)?);
    }
    let mut groups: Vec<_> = groups
        .into_iter()
        .map(|((network_id, name), mut node_ids_base64)| {
            node_ids_base64.sort();
            GroupSummary {
                network_id,
                name,
                node_ids_base64,
            }
        })
        .collect();
    groups.sort_by(|left, right| {
        left.network_id
            .cmp(&right.network_id)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(groups)
}

fn nodes_from_rows(
    rows: Vec<NodeRow>,
    online_node_ids: &HashSet<[u8; 16]>,
    memberships: &[MembershipRow],
    routes: &[SubnetRouteSummary],
    now: DateTime<Utc>,
) -> Result<Vec<NodeSummary>, ApiError> {
    rows.into_iter()
        .map(|row| {
            let node_id: [u8; 16] = row
                .node_id
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::internal())?;
            let node_id_base64 = URL_SAFE_NO_PAD.encode(node_id);
            let online = row.revoked_at.is_none() && online_node_ids.contains(&node_id);
            let state = if row.revoked_at.is_some() {
                "revoked"
            } else if online {
                "online"
            } else {
                "offline"
            };
            let (public_endpoint, local_endpoints) = candidate_endpoints(row.candidate_payload)?;
            let last_seen_at = latest_time(
                row.last_control_connected_at,
                row.last_control_disconnected_at,
            );
            let groups = memberships
                .iter()
                .filter(|membership| membership.node_database_id == row.id)
                .map(|membership| membership.group_name.clone())
                .collect();
            let published_subnets = routes
                .iter()
                .filter(|route| {
                    route.gateway_node_id_base64 == node_id_base64 && route.state != "revoked"
                })
                .map(|route| route.prefix.clone())
                .collect();
            let credential_state = if row.revoked_at.is_some() {
                "revoked"
            } else if row.credential_not_after <= now {
                "expired"
            } else {
                "active"
            };
            Ok(NodeSummary {
                id: row.id,
                network_id: row.network_id,
                network_name: row.network_name,
                node_id_base64,
                name: row.name,
                virtual_ip: row.virtual_ip,
                device_type: row.device_type,
                architecture: Availability::unavailable("Agent 尚未上报系统架构"),
                agent_version: Availability::unavailable("Agent 尚未上报版本"),
                public_endpoint,
                local_endpoints,
                current_path: Availability::unavailable("Agent 尚未上报当前路径"),
                relay: Availability::unavailable("Agent 尚未上报当前 Relay"),
                latency_ms: Availability::unavailable("Agent 尚未上报路径延迟"),
                traffic_bytes_24h: Availability::unavailable("Agent 尚未上报流量"),
                state,
                last_seen_at,
                groups,
                tags: row.tags,
                published_subnets,
                credential_expires_at: row.credential_not_after,
                credential_state,
                update_state: Availability::unavailable("更新管理将在 M6.1 实现"),
            })
        })
        .collect()
}

fn networks_from_rows(
    rows: Vec<NetworkRow>,
    nodes: &[NodeSummary],
) -> Result<Vec<NetworkSummary>, ApiError> {
    rows.into_iter()
        .map(|row| {
            Ok(NetworkSummary {
                id: row.id,
                name: row.name,
                address_pool: row.address_pool,
                reserved_addresses: u32::try_from(row.reserved_addresses)
                    .map_err(|_| ApiError::internal())?,
                configuration_version: u64::try_from(row.config_version)
                    .map_err(|_| ApiError::internal())?,
                policy_version: u64::try_from(row.policy_version)
                    .map_err(|_| ApiError::internal())?,
                active_nodes: nodes
                    .iter()
                    .filter(|node| node.network_id == row.id && node.state != "revoked")
                    .count(),
                online_nodes: nodes
                    .iter()
                    .filter(|node| node.network_id == row.id && node.state == "online")
                    .count(),
                active_leases: u64::try_from(row.active_leases)
                    .map_err(|_| ApiError::internal())?,
                created_at: row.created_at,
            })
        })
        .collect()
}

fn candidate_endpoints(
    payload: Option<Vec<u8>>,
) -> Result<(Option<String>, Vec<String>), ApiError> {
    let Some(payload) = payload else {
        return Ok((None, Vec::new()));
    };
    let advertisement: CandidateAdvertisement =
        serde_json::from_slice(&payload).map_err(|_| ApiError::internal())?;
    let mut public_endpoint = None;
    let mut local_endpoints = Vec::new();
    for candidate in advertisement.candidates {
        match candidate.kind {
            xs_core::EndpointCandidateKind::Mapped | xs_core::EndpointCandidateKind::PublicIpv6 => {
                public_endpoint.get_or_insert_with(|| candidate.endpoint.to_string());
            }
            xs_core::EndpointCandidateKind::Local => {
                local_endpoints.push(candidate.endpoint.to_string());
            }
            xs_core::EndpointCandidateKind::Static | xs_core::EndpointCandidateKind::Relay => {}
        }
    }
    Ok((public_endpoint, local_endpoints))
}

fn count_pending_suggestions(
    suggestions: &[SubnetRouteSuggestionSummary],
    routes: &[SubnetRouteSummary],
) -> usize {
    suggestions
        .iter()
        .flat_map(|advertisement| {
            advertisement.suggestions.iter().filter(|suggestion| {
                !routes.iter().any(|route| {
                    route.network_id == advertisement.network_id
                        && route.gateway_node_id_base64 == advertisement.gateway_node_id_base64
                        && route.prefix == suggestion.prefix
                        && route.interface_name == suggestion.interface_name
                        && route.state != "revoked"
                })
            })
        })
        .count()
}

fn latest_time(left: Option<DateTime<Utc>>, right: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn encode_node_id(node_id: &[u8]) -> Result<String, ApiError> {
    let node_id: [u8; 16] = node_id.try_into().map_err(|_| ApiError::internal())?;
    Ok(URL_SAFE_NO_PAD.encode(node_id))
}

fn database_error(_error: sqlx::Error) -> ApiError {
    ApiError::internal()
}
