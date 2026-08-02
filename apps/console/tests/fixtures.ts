import type { Page, Route } from "@playwright/test";

import type {
  ConsoleRole,
  ConsoleSnapshot,
  ConsoleUser,
  SessionPayload,
  UpdatePolicy,
  UpdateRelease,
} from "../src/types";

const now = "2026-07-30T21:45:00Z";

export function userFixture(role: ConsoleRole = "administrator"): ConsoleUser {
  return {
    id: role === "administrator" ? "00000000-0000-4000-8000-000000000001" : "00000000-0000-4000-8000-000000000002",
    username: role === "administrator" ? "admin" : "audit.user",
    display_name: role === "administrator" ? "系统管理员" : "安全审计员",
    role,
    enabled: true,
    created_at: "2026-07-29T08:00:00Z",
    last_login_at: now,
  };
}

export function sessionFixture(role: ConsoleRole = "administrator"): SessionPayload {
  return {
    user: userFixture(role),
    csrf_token: ["fixture", "csrf", "token", "with-more-than-32-characters"].join("-"),
    expires_at: "2026-07-31T05:45:00Z",
  };
}

export const usersFixture: ConsoleUser[] = [
  userFixture("administrator"),
  {
    ...userFixture("operator"),
    id: "00000000-0000-4000-8000-000000000003",
    username: "network.operator",
    display_name: "网络运维员（华西灾备与边缘节点联合值班组）",
  },
  userFixture("auditor"),
];

const unavailable = <T>(reason: string) => ({
  status: "unavailable" as const,
  value: null as T | null,
  reason,
});

const available = <T>(value: T) => ({
  status: "available" as const,
  value,
  reason: null,
});

export const snapshotFixture: ConsoleSnapshot = {
  collected_at: now,
  dashboard: {
    online_nodes: 1,
    offline_nodes: 1,
    direct_nodes: available(1),
    relay_nodes: available(0),
    relay_health: available("healthy"),
    traffic_bytes_24h: available(3_670_016),
    connection_success_percent_24h: available(99.8),
    average_latency_ms_24h: available(12.4),
    pending_route_suggestions: 1,
    security_alerts_24h: 2,
    recent_activity: [],
  },
  networks: [
    {
      id: "10000000-0000-4000-8000-000000000001",
      name: "生产互联网络（成都总部—异地灾备—边缘采集长名称验证）",
      address_pool: "100.88.0.0/24",
      reserved_addresses: 16,
      configuration_version: 12,
      policy_version: 4,
      active_nodes: 2,
      online_nodes: 1,
      active_leases: 2,
      created_at: "2026-07-29T09:00:00Z",
    },
  ],
  nodes: [
    {
      id: "20000000-0000-4000-8000-000000000001",
      network_id: "10000000-0000-4000-8000-000000000001",
      network_name: "生产互联网络（成都总部—异地灾备—边缘采集长名称验证）",
      node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ",
      name: "成都总部核心网关-超长设备名称用于验证表格截断与详情提示",
      virtual_ip: "100.88.0.16/32",
      device_type: "linux",
      architecture: available("x86_64"),
      agent_version: available("0.1.0"),
      public_endpoint: "203.0.113.200:42001",
      local_endpoints: ["192.168.100.254:42001", "10.200.30.40:42001"],
      current_path: available("direct"),
      relay: available("none"),
      latency_ms: available(12.4),
      traffic_bytes_24h: available(3_670_016),
      state: "online",
      last_seen_at: now,
      groups: ["gateway", "production"],
      tags: ["linux", "subnet-router", "chengdu-headquarters"],
      published_subnets: ["192.168.100.0/24"],
      credential_expires_at: "2026-08-30T00:00:00Z",
      credential_state: "active",
      update_channel: available("stable" as const),
      update_state: available("idle"),
      update_release_id: null,
      update_error_code: null,
      update_reported_at: now,
    },
    {
      id: "20000000-0000-4000-8000-000000000002",
      network_id: "10000000-0000-4000-8000-000000000001",
      network_name: "生产互联网络（成都总部—异地灾备—边缘采集长名称验证）",
      node_id_base64: "AgICAgICAgICAgICAgICAg",
      name: "异地灾备应用节点",
      virtual_ip: "100.88.0.17/32",
      device_type: "linux",
      architecture: unavailable<string>("Agent 尚未上报系统架构"),
      agent_version: unavailable<string>("Agent 尚未上报版本"),
      public_endpoint: null,
      local_endpoints: [],
      current_path: unavailable<string>("Agent 尚未上报当前路径"),
      relay: unavailable<string>("Agent 尚未上报当前 Relay"),
      latency_ms: unavailable<number>("Agent 尚未上报路径延迟"),
      traffic_bytes_24h: unavailable<number>("Agent 尚未上报流量"),
      state: "offline",
      last_seen_at: "2026-07-30T19:20:00Z",
      groups: ["application"],
      tags: ["linux"],
      published_subnets: [],
      credential_expires_at: "2026-08-30T00:00:00Z",
      credential_state: "active",
      update_channel: available("stable" as const),
      update_state: unavailable<string>("Agent 尚未上报更新状态"),
      update_release_id: null,
      update_error_code: null,
      update_reported_at: null,
    },
  ],
  enrollment_tokens: [
    {
      id: "30000000-0000-4000-8000-000000000001",
      network_id: "10000000-0000-4000-8000-000000000001",
      network_name: "生产互联网络（成都总部—异地灾备—边缘采集长名称验证）",
      expires_at: "2026-07-31T00:00:00Z",
      max_uses: 3,
      use_count: 1,
      remaining_uses: 2,
      state: "active",
      default_role_bitmap: 0,
      default_tags: ["linux", "production"],
      requested_virtual_ip: null,
      created_at: "2026-07-30T20:00:00Z",
      created_by: "00000000-0000-4000-8000-000000000001",
    },
  ],
  groups: [
    {
      network_id: "10000000-0000-4000-8000-000000000001",
      name: "gateway",
      node_ids_base64: ["AQEBAQEBAQEBAQEBAQEBAQ"],
    },
  ],
  acl_rules: [
    {
      network_id: "10000000-0000-4000-8000-000000000001",
      network_name: "生产互联网络（成都总部—异地灾备—边缘采集长名称验证）",
      rule_id: "allow-production-https-from-application-group",
      priority: 300,
      action: "allow",
      protocol: "tcp",
      sources: [{ type: "group", name: "application" }],
      destinations: [{ type: "group", name: "gateway" }],
      destination_ports: [{ start: 443, end: 443 }],
    },
  ],
  subnet_route_suggestions: [
    {
      network_id: "10000000-0000-4000-8000-000000000001",
      gateway_node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ",
      gateway_name: "成都总部核心网关-超长设备名称用于验证表格截断与详情提示",
      generation: 8,
      expires_at: "2026-07-30T22:00:00Z",
      suggestions: [{ prefix: "192.168.101.0/24", interface_name: "ens192" }],
    },
  ],
  subnet_routes: [
    {
      network_id: "10000000-0000-4000-8000-000000000001",
      network_name: "生产互联网络（成都总部—异地灾备—边缘采集长名称验证）",
      route_id: "chengdu-production-lan",
      gateway_node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ",
      gateway_name: "成都总部核心网关-超长设备名称用于验证表格截断与详情提示",
      prefix: "192.168.100.0/24",
      interface_name: "ens192",
      mode: "routed",
      priority: 200,
      state: "enabled",
      updated_at: now,
    },
  ],
  relays: [
    {
      relay_id_base64: "EREREREREREREREREREREQ",
      endpoint: "198.51.100.20:42002",
      priority: 100,
      expires_at: "2026-08-30T00:00:00Z",
      health: available("healthy"),
      metrics: available({
        reported_at: now,
        current: {
          active_leases: 2,
          packets_received: 18_402,
          bytes_received: 12_845_120,
          packets_forwarded: 18_390,
          bytes_forwarded: 12_830_720,
          packets_dropped: 12,
          io_errors: 0,
          forwarding_latency_samples: 18_390,
          forwarding_latency_microseconds_total: 119_535_000,
        },
        window_24h: {
          packets_received: 18_000,
          bytes_received: 12_582_912,
          packets_forwarded: 17_990,
          bytes_forwarded: 12_570_624,
          packets_dropped: 10,
          io_errors: 0,
          forwarding_latency_samples: 17_990,
          forwarding_latency_microseconds_average: 6_500,
        },
      }),
    },
  ],
  topology: {
    nodes: [
      { node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ", name: "成都总部核心网关-超长设备名称用于验证表格截断与详情提示", virtual_ip: "100.88.0.16/32", state: "online" },
      { node_id_base64: "AgICAgICAgICAgICAgICAg", name: "异地灾备应用节点", virtual_ip: "100.88.0.17/32", state: "offline" },
    ],
    links: [{ source_node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ", destination_node_id_base64: "AgICAgICAgICAgICAgICAg", path: "direct", latency_ms: 12.4 }],
    subnets: [{ gateway_node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ", prefix: "192.168.100.0/24", state: "enabled" }],
    link_telemetry: available("agent_signed_v1"),
  },
  audit_events: [
    {
      id: 22,
      occurred_at: now,
      network_id: "10000000-0000-4000-8000-000000000001",
      actor_type: "console_user",
      actor_id: "00000000-0000-4000-8000-000000000001",
      action: "subnet_routes.replace",
      target_type: "network_subnet_routes",
      target_id: "10000000-0000-4000-8000-000000000001",
      outcome: "success",
      metadata: {},
    },
    {
      id: 21,
      occurred_at: "2026-07-30T21:40:00Z",
      network_id: null,
      actor_type: "anonymous",
      actor_id: "redacted-username-hash",
      action: "auth.login",
      target_type: "console_session",
      target_id: null,
      outcome: "rejected",
      metadata: { reason_class: "invalid_credentials" },
    },
  ],
  alerts: [
    {
      audit_event_id: 21,
      occurred_at: "2026-07-30T21:40:00Z",
      severity: "medium",
      action: "auth.login",
      outcome: "rejected",
      reason_class: "invalid_credentials",
    },
  ],
  system: {
    database: { status: "available", value: "ok", reason: null },
    credential_signing_key_id: 41231023,
    configuration_signing_key_id: 98230111,
    update_management: available("signed_release_v1"),
    backup_restore: unavailable<string>("备份恢复流程将在 M8.1 实现"),
    relay_metrics: available("relay_signed_v1_25h_bounded"),
    path_telemetry: available("agent_signed_v1_25h_bounded"),
  },
};

snapshotFixture.dashboard.recent_activity = snapshotFixture.audit_events;

export const updateReleasesFixture: UpdateRelease[] = [
  {
    id: "70000000-0000-4000-8000-000000000001",
    version: "0.2.0",
    platform: "linux",
    architecture: "x86_64",
    target: "x86_64-unknown-linux-gnu",
    archive_name: "xs-nexus-0.2.0-x86_64-unknown-linux-gnu.tar.gz",
    archive_size: 18_874_368,
    archive_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    archive_url: "https://updates.example.test/xs-nexus-0.2.0-x86_64-unknown-linux-gnu.tar.gz",
    created_at: now,
  },
];

export const updatePoliciesFixture: UpdatePolicy[] = [
  {
    network_id: snapshotFixture.networks[0].id,
    channel: "stable",
    platform: "linux",
    architecture: "x86_64",
    release: updateReleasesFixture[0],
    minimum_version: "0.1.0",
    rollout_basis_points: 2500,
    paused: false,
    generation: 3,
    updated_at: now,
  },
];

export function emptySnapshotFixture(): ConsoleSnapshot {
  return {
    ...structuredClone(snapshotFixture),
    dashboard: {
      ...structuredClone(snapshotFixture.dashboard),
      online_nodes: 0,
      offline_nodes: 0,
      pending_route_suggestions: 0,
      security_alerts_24h: 0,
      recent_activity: [],
    },
    networks: [],
    nodes: [],
    enrollment_tokens: [],
    groups: [],
    acl_rules: [],
    subnet_route_suggestions: [],
    subnet_routes: [],
    relays: [],
    topology: { nodes: [], links: [], subnets: [], link_telemetry: unavailable<string>("Agent 尚未上报已选链路") },
    audit_events: [],
    alerts: [],
  };
}

export function stressSnapshotFixture(): ConsoleSnapshot {
  const snapshot = structuredClone(snapshotFixture);
  snapshot.nodes = Array.from({ length: 40 }, (_, index) => {
    const source = snapshotFixture.nodes[index % snapshotFixture.nodes.length];
    return {
      ...structuredClone(source),
      id: `20000000-0000-4000-8000-${String(index + 100).padStart(12, "0")}`,
      node_id_base64: `fixed-visual-node-${String(index).padStart(3, "0")}`,
      name: `${source.name}（固定大量数据第 ${index + 1} 项）`,
      virtual_ip: index % 3 === 0
        ? `fd7a:115c:a1e0:ab12:4843:cd96:62${String(index).padStart(2, "0")}:${String(index + 1).padStart(4, "0")}/128`
        : `100.88.0.${16 + index}/32`,
      public_endpoint: index % 3 === 0
        ? `[2001:db8:1234:5678:90ab:cdef:${String(index).padStart(4, "0")}:${String(index + 1).padStart(4, "0")}]:42001`
        : source.public_endpoint,
    };
  });
  snapshot.topology.nodes = snapshot.nodes.map((node) => ({
    node_id_base64: node.node_id_base64,
    name: node.name,
    virtual_ip: node.virtual_ip,
    state: node.state,
  }));
  snapshot.dashboard.online_nodes = snapshot.nodes.filter((node) => node.state === "online").length;
  snapshot.dashboard.offline_nodes = snapshot.nodes.filter((node) => node.state === "offline").length;
  return snapshot;
}

export async function mockAuthenticatedApi(
  page: Page,
  options: { role?: ConsoleRole; snapshot?: ConsoleSnapshot } = {},
) {
  const role = options.role ?? "administrator";
  const snapshot = options.snapshot ?? snapshotFixture;
  const releases = snapshot.networks.length === 0 ? [] : updateReleasesFixture;
  await page.route("**/v1/auth/session", (route) => fulfillJson(route, 200, sessionFixture(role)));
  await page.route("**/v1/admin/console", (route) => fulfillJson(route, 200, snapshot));
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, usersFixture));
  await page.route("**/v1/admin/update-releases", (route) => fulfillJson(route, 200, releases));
  await page.route("**/v1/admin/networks/*/update-policies", (route) => fulfillJson(route, 200, updatePoliciesFixture));
}

export function fulfillJson(route: Route, status: number, body: unknown) {
  return route.fulfill({ status, contentType: "application/json", body: JSON.stringify(body) });
}
