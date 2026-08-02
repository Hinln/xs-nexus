export type ConsoleRole = "administrator" | "operator" | "auditor";
export type UpdateChannel = "stable" | "testing" | "development";

export interface ConsoleUser {
  id: string;
  username: string;
  display_name: string;
  role: ConsoleRole;
  enabled: boolean;
  created_at: string;
  last_login_at: string | null;
}

export interface SessionPayload {
  user: ConsoleUser;
  csrf_token: string;
  expires_at: string;
}

export interface Availability<T> {
  status: "available" | "unavailable";
  value: T | null;
  reason: string | null;
}

export interface AuditEvent {
  id: number;
  occurred_at: string;
  network_id: string | null;
  actor_type: string;
  actor_id: string;
  action: string;
  target_type: string;
  target_id: string | null;
  outcome: "success" | "rejected" | "failure";
  metadata: Record<string, unknown>;
}

export interface DashboardSummary {
  online_nodes: number;
  offline_nodes: number;
  direct_nodes: Availability<number>;
  relay_nodes: Availability<number>;
  relay_health: Availability<string>;
  traffic_bytes_24h: Availability<number>;
  connection_success_percent_24h: Availability<number>;
  average_latency_ms_24h: Availability<number>;
  pending_route_suggestions: number;
  security_alerts_24h: number;
  recent_activity: AuditEvent[];
}

export interface NetworkSummary {
  id: string;
  name: string;
  address_pool: string;
  reserved_addresses: number;
  configuration_version: number;
  policy_version: number;
  active_nodes: number;
  online_nodes: number;
  active_leases: number;
  created_at: string;
}

export interface NodeSummary {
  id: string;
  network_id: string;
  network_name: string;
  node_id_base64: string;
  name: string;
  virtual_ip: string;
  device_type: string;
  architecture: Availability<string>;
  agent_version: Availability<string>;
  public_endpoint: string | null;
  local_endpoints: string[];
  current_path: Availability<string>;
  relay: Availability<string>;
  latency_ms: Availability<number>;
  traffic_bytes_24h: Availability<number>;
  state: "online" | "offline" | "revoked";
  last_seen_at: string | null;
  groups: string[];
  tags: string[];
  published_subnets: string[];
  credential_expires_at: string;
  credential_state: "active" | "expired" | "revoked";
  update_channel: Availability<UpdateChannel>;
  update_state: Availability<string>;
  update_release_id: string | null;
  update_error_code: string | null;
  update_reported_at: string | null;
}

export interface EnrollmentTokenSummary {
  id: string;
  network_id: string;
  network_name: string;
  expires_at: string;
  max_uses: number;
  use_count: number;
  remaining_uses: number;
  state: "active" | "expired" | "exhausted" | "revoked";
  default_role_bitmap: number;
  default_tags: string[];
  requested_virtual_ip: string | null;
  created_at: string;
  created_by: string;
}

export interface GroupSummary {
  network_id: string;
  name: string;
  node_ids_base64: string[];
}

export interface AclRuleSummary {
  network_id: string;
  network_name: string;
  rule_id: string;
  priority: number;
  action: "allow" | "deny";
  protocol: "any" | "tcp" | "udp" | "icmp";
  sources: unknown[];
  destinations: unknown[];
  destination_ports: unknown[];
}

export interface RouteSuggestion {
  prefix: string;
  interface_name: string;
}

export interface RouteSuggestionAdvertisement {
  network_id: string;
  gateway_node_id_base64: string;
  gateway_name: string;
  generation: number;
  expires_at: string;
  suggestions: RouteSuggestion[];
}

export interface SubnetRouteSummary {
  network_id: string;
  network_name: string;
  route_id: string;
  gateway_node_id_base64: string;
  gateway_name: string;
  prefix: string;
  interface_name: string;
  mode: "routed" | "nat";
  priority: number;
  state: "enabled" | "paused" | "revoked";
  updated_at: string;
}

export interface RelaySummary {
  relay_id_base64: string;
  endpoint: string;
  priority: number;
  expires_at: string;
  health: Availability<string>;
  metrics: Availability<Record<string, number>>;
}

export interface TopologySummary {
  nodes: Array<{
    node_id_base64: string;
    name: string;
    virtual_ip: string;
    state: NodeSummary["state"];
  }>;
  links: Array<{
    source_node_id_base64: string;
    destination_node_id_base64: string;
    path: string;
    latency_ms: number | null;
  }>;
  subnets: Array<{
    gateway_node_id_base64: string;
    prefix: string;
    state: string;
  }>;
  link_telemetry: Availability<string>;
}

export interface AlertSummary {
  audit_event_id: number;
  occurred_at: string;
  severity: "high" | "medium";
  action: string;
  outcome: string;
  reason_class: string | null;
}

export interface SystemSummary {
  database: Availability<string>;
  credential_signing_key_id: number;
  configuration_signing_key_id: number;
  update_management: Availability<string>;
  backup_restore: Availability<string>;
  relay_metrics: Availability<string>;
  path_telemetry: Availability<string>;
}

export interface ConsoleSnapshot {
  collected_at: string;
  dashboard: DashboardSummary;
  networks: NetworkSummary[];
  nodes: NodeSummary[];
  enrollment_tokens: EnrollmentTokenSummary[];
  groups: GroupSummary[];
  acl_rules: AclRuleSummary[];
  subnet_route_suggestions: RouteSuggestionAdvertisement[];
  subnet_routes: SubnetRouteSummary[];
  relays: RelaySummary[];
  topology: TopologySummary;
  audit_events: AuditEvent[];
  alerts: AlertSummary[];
  system: SystemSummary;
}

export interface EnrollmentTokenCreated {
  id: string;
  network_id: string;
  token: string;
  expires_at: string;
  max_uses: number;
}

export interface AclExplanation {
  network_id: string;
  policy_version: number;
  decision: {
    allowed: boolean;
    matched_rule_id: string | null;
    reason: string;
  };
}

export interface UpdateRelease {
  id: string;
  version: string;
  platform: "linux";
  architecture: "x86_64" | "aarch64";
  target: string;
  archive_name: string;
  archive_size: number;
  archive_sha256: string;
  archive_url: string;
  created_at: string;
}

export interface UpdatePolicy {
  network_id: string;
  channel: UpdateChannel;
  platform: "linux";
  architecture: "x86_64" | "aarch64";
  release: UpdateRelease;
  minimum_version: string | null;
  rollout_basis_points: number;
  paused: boolean;
  generation: number;
  updated_at: string;
}
