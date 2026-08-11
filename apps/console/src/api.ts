import type {
  AclExplanation,
  ConsoleSnapshot,
  ConsoleUser,
  EnrollmentTokenCreated,
  SessionPayload,
  SubnetRouteSummary,
  UpdateChannel,
  UpdatePolicy,
  UpdateRelease,
} from "./types";

interface ErrorEnvelope {
  error?: {
    code?: string;
    message?: string;
  };
}

export class ConsoleApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ConsoleApiError";
    this.status = status;
    this.code = code;
  }
}

async function request<T>(
  path: string,
  options: RequestInit = {},
  csrfToken?: string,
): Promise<T> {
  const headers = new Headers(options.headers);
  headers.set("Accept", "application/json");
  if (options.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (csrfToken !== undefined) {
    headers.set("X-CSRF-Token", csrfToken);
  }

  let response: Response;
  try {
    response = await fetch(path, {
      ...options,
      headers,
      credentials: "same-origin",
    });
  } catch {
    throw new ConsoleApiError(0, "network_error", "无法连接 Controller");
  }

  if (response.status === 204) {
    return undefined as T;
  }
  const payload = await readJson(response);
  if (!response.ok) {
    const envelope = payload as ErrorEnvelope;
    throw new ConsoleApiError(
      response.status,
      envelope.error?.code ?? "request_failed",
      publicErrorMessage(response.status),
    );
  }
  return payload as T;
}

async function readJson(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    if (response.ok) {
      throw new ConsoleApiError(
        response.status,
        "invalid_response",
        "Controller 返回了无法解析的响应",
      );
    }
    return {};
  }
}

function publicErrorMessage(status: number): string {
  if (status === 400) return "提交内容未通过验证";
  if (status === 401) return "登录已失效，请重新登录";
  if (status === 403) return "当前用户没有执行此操作的权限";
  if (status === 404) return "请求的资源不存在";
  if (status === 409) return "资源已变化或与当前配置冲突";
  if (status === 429) return "请求过于频繁，请稍后再试";
  if (status >= 500) return "Controller 暂时无法完成请求";
  return "请求未能完成";
}

export async function restoreSession(): Promise<SessionPayload> {
  return request<SessionPayload>("/v1/auth/session");
}

export async function login(
  username: string,
  password: string,
): Promise<SessionPayload> {
  return request<SessionPayload>("/v1/auth/login", {
    method: "POST",
    body: JSON.stringify({ username, password }),
  });
}

export async function logout(csrfToken: string): Promise<void> {
  await request<void>("/v1/auth/logout", { method: "POST" }, csrfToken);
}

export async function loadConsoleSnapshot(): Promise<ConsoleSnapshot> {
  const snapshot = await request<ConsoleSnapshot>("/v1/admin/console");
  if (!validSnapshot(snapshot)) {
    throw new ConsoleApiError(
      200,
      "invalid_response",
      "Controller 返回了不完整的控制台数据",
    );
  }
  return snapshot;
}

function validSnapshot(snapshot: ConsoleSnapshot): boolean {
  return (
    typeof snapshot === "object" &&
    snapshot !== null &&
    typeof snapshot.collected_at === "string" &&
    typeof snapshot.dashboard === "object" &&
    Array.isArray(snapshot.networks) &&
    Array.isArray(snapshot.nodes) &&
    Array.isArray(snapshot.enrollment_tokens) &&
    Array.isArray(snapshot.acl_rules) &&
    Array.isArray(snapshot.subnet_routes) &&
    Array.isArray(snapshot.audit_events)
  );
}

export async function loadUsers(): Promise<ConsoleUser[]> {
  return request<ConsoleUser[]>("/v1/admin/users");
}

export async function loadUpdateReleases(): Promise<UpdateRelease[]> {
  return request<UpdateRelease[]>("/v1/admin/update-releases");
}

export async function loadUpdatePolicies(networkId: string): Promise<UpdatePolicy[]> {
  return request<UpdatePolicy[]>(
    `/v1/admin/networks/${encodeURIComponent(networkId)}/update-policies`,
  );
}

export async function createUpdateRelease(
  input: {
    manifest_base64: string;
    signature_base64: string;
    archive_url: string;
  },
  csrfToken: string,
): Promise<UpdateRelease> {
  return request<UpdateRelease>(
    "/v1/admin/update-releases",
    { method: "POST", body: JSON.stringify(input) },
    csrfToken,
  );
}

export async function revokeUpdateRelease(
  releaseId: string,
  reason: "build_error" | "key_compromise" | "security_issue" | "superseded" | "withdrawn",
  csrfToken: string,
): Promise<UpdateRelease> {
  return request<UpdateRelease>(
    `/v1/admin/update-releases/${encodeURIComponent(releaseId)}/revoke`,
    { method: "POST", body: JSON.stringify({ reason }) },
    csrfToken,
  );
}

export async function replaceUpdatePolicy(
  networkId: string,
  channel: UpdateChannel,
  platform: "linux",
  architecture: "x86_64" | "aarch64",
  input: {
    expected_generation: number;
    release_id: string;
    minimum_version: string | null;
    rollout_basis_points: number;
    paused: boolean;
  },
  csrfToken: string,
): Promise<UpdatePolicy> {
  const path = [
    "/v1/admin/networks",
    encodeURIComponent(networkId),
    "update-policies",
    encodeURIComponent(channel),
    platform,
    architecture,
  ].join("/");
  return request<UpdatePolicy>(
    path,
    { method: "PUT", body: JSON.stringify(input) },
    csrfToken,
  );
}

export async function replaceNodeUpdateChannel(
  networkId: string,
  nodeIdBase64: string,
  input: {
    expected_configuration_version: number;
    update_channel: UpdateChannel;
  },
  csrfToken: string,
): Promise<{
  network_id: string;
  node_id_base64: string;
  update_channel: UpdateChannel;
  configuration_version: number;
}> {
  const path = [
    "/v1/admin/networks",
    encodeURIComponent(networkId),
    "nodes",
    encodeURIComponent(nodeIdBase64),
    "update-channel",
  ].join("/");
  return request(
    path,
    { method: "PUT", body: JSON.stringify(input) },
    csrfToken,
  );
}

export async function createUser(
  input: {
    username: string;
    display_name: string;
    password: string;
    role: ConsoleUser["role"];
  },
  csrfToken: string,
): Promise<ConsoleUser> {
  return request<ConsoleUser>(
    "/v1/admin/users",
    { method: "POST", body: JSON.stringify(input) },
    csrfToken,
  );
}

export async function createNetwork(
  input: { name: string; address_pool: string; reserved_addresses: number },
  csrfToken: string,
): Promise<void> {
  await request(
    "/v1/admin/networks",
    { method: "POST", body: JSON.stringify(input) },
    csrfToken,
  );
}

export async function createEnrollmentToken(
  input: {
    network_id: string;
    expires_in_seconds: number;
    max_uses: number;
    default_role_bitmap: number;
    default_tags: string[];
    requested_virtual_ip?: string;
  },
  csrfToken: string,
): Promise<EnrollmentTokenCreated> {
  return request<EnrollmentTokenCreated>(
    "/v1/admin/enrollment-tokens",
    { method: "POST", body: JSON.stringify(input) },
    csrfToken,
  );
}

export async function revokeNode(
  networkId: string,
  nodeIdBase64: string,
  csrfToken: string,
): Promise<void> {
  await request(
    `/v1/admin/networks/${networkId}/nodes/${encodeURIComponent(nodeIdBase64)}/revoke`,
    { method: "POST", body: JSON.stringify({ ip_cooldown_seconds: 3600 }) },
    csrfToken,
  );
}

export async function explainAcl(
  networkId: string,
  input: {
    source_node_id_base64: string;
    destination_node_id_base64: string;
    protocol: "tcp" | "udp" | "icmp";
    destination_port?: number;
  },
): Promise<AclExplanation> {
  return request<AclExplanation>(
    `/v1/admin/networks/${networkId}/acl/explain`,
    { method: "POST", body: JSON.stringify(input) },
  );
}

export async function replaceSubnetRoutes(
  networkId: string,
  configurationVersion: number,
  routes: Array<
    Pick<
      SubnetRouteSummary,
      | "route_id"
      | "gateway_node_id_base64"
      | "prefix"
      | "interface_name"
      | "mode"
      | "priority"
    > & { enabled: boolean }
  >,
  csrfToken: string,
): Promise<void> {
  await request(
    `/v1/admin/networks/${networkId}/subnet-routes`,
    {
      method: "PUT",
      body: JSON.stringify({
        expected_configuration_version: configurationVersion,
        routes,
      }),
    },
    csrfToken,
  );
}
