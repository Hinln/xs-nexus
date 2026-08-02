import { useMemo, useState, type FormEvent } from "react";

import {
  createEnrollmentToken,
  createNetwork,
  replaceSubnetRoutes,
} from "../api";
import { availabilityText, formatBytes, formatDateTime, tokenStateLabel } from "../format";
import type {
  ConsoleSnapshot,
  ConsoleUser,
  EnrollmentTokenCreated,
  RouteSuggestion,
  RouteSuggestionAdvertisement,
} from "../types";
import {
  EmptyState,
  Notice,
  PageHeader,
  StatusPill,
  TagList,
} from "../components/StateView";

interface WritablePageProps {
  snapshot: ConsoleSnapshot;
  user: ConsoleUser;
  csrfToken: string;
  onChanged: () => Promise<void>;
}

function canManage(user: ConsoleUser): boolean {
  return user.role === "administrator" || user.role === "operator";
}

export function NetworksPage({ snapshot, user, csrfToken, onChanged }: WritablePageProps) {
  const [showForm, setShowForm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    setBusy(true);
    setError(null);
    try {
      await createNetwork(
        {
          name: String(data.get("name") ?? ""),
          address_pool: String(data.get("address_pool") ?? ""),
          reserved_addresses: Number(data.get("reserved_addresses") ?? 16),
        },
        csrfToken,
      );
      setShowForm(false);
      await onChanged();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "创建网络失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader
        title="网络"
        description="网络、配置版本和节点计数均来自 Controller 数据库。"
        actions={canManage(user) ? (
          <button className="button button--primary" type="button" onClick={() => setShowForm((open) => !open)}>
            {showForm ? "取消创建" : "创建网络"}
          </button>
        ) : null}
      />
      {showForm ? (
        <form className="panel form-panel" onSubmit={(event) => void submit(event)}>
          <div className="panel-heading"><h2>新建虚拟网络</h2></div>
          <div className="form-grid">
            <label>网络名称<input name="name" required minLength={1} maxLength={80} autoComplete="off" /></label>
            <label>IPv4 地址池<input name="address_pool" required placeholder="100.88.0.0/24" autoComplete="off" /></label>
            <label>预留地址数<input name="reserved_addresses" required type="number" min={2} max={4096} defaultValue={16} /></label>
          </div>
          <Notice>地址池会在 Controller 事务中检查与现有网络重叠，不会静默覆盖。</Notice>
          {error ? <Notice tone="danger">{error}</Notice> : null}
          <button className="button button--primary" type="submit" disabled={busy}>{busy ? "正在创建" : "确认创建"}</button>
        </form>
      ) : null}
      {snapshot.networks.length === 0 ? (
        <EmptyState title="尚无网络" description="管理员可以创建第一个虚拟网络。" />
      ) : (
        <div className="card-grid">
          {snapshot.networks.map((network) => (
            <article className="network-card" key={network.id}>
              <div className="network-card__head">
                <div><span className="eyebrow">虚拟网络</span><h2 title={network.name}>{network.name}</h2></div>
                <StatusPill state={network.online_nodes > 0 ? "online" : "offline"} label={`${network.online_nodes} 在线`} />
              </div>
              <code className="address-block">{network.address_pool}</code>
              <dl className="compact-stats">
                <div><dt>活动节点</dt><dd>{network.active_nodes}</dd></div>
                <div><dt>活动租约</dt><dd>{network.active_leases}</dd></div>
                <div><dt>配置版本</dt><dd>{network.configuration_version}</dd></div>
                <div><dt>策略版本</dt><dd>{network.policy_version}</dd></div>
              </dl>
              <p className="muted">创建于 {formatDateTime(network.created_at)}</p>
            </article>
          ))}
        </div>
      )}
    </>
  );
}

export function AddressPoolsPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="地址池" description="展示真实地址池、预留边界和当前活动租约，不估算未分配地址。" />
      {snapshot.networks.length === 0 ? (
        <EmptyState title="暂无地址池" description="创建网络后会自动建立对应地址池。" />
      ) : (
        <div className="table-shell">
          <table>
            <caption>网络地址池</caption>
            <thead><tr><th scope="col">网络</th><th scope="col">CIDR</th><th scope="col">预留</th><th scope="col">活动租约</th><th scope="col">版本</th></tr></thead>
            <tbody>
              {snapshot.networks.map((network) => (
                <tr key={network.id}>
                  <td><strong title={network.name}>{network.name}</strong></td>
                  <td><code>{network.address_pool}</code></td>
                  <td>{network.reserved_addresses}</td>
                  <td>{network.active_leases}</td>
                  <td>配置 {network.configuration_version}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

export function TokensPage({ snapshot, user, csrfToken, onChanged }: WritablePageProps) {
  const [showForm, setShowForm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<EnrollmentTokenCreated | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const requestedIp = String(data.get("requested_virtual_ip") ?? "").trim();
    setBusy(true);
    setError(null);
    try {
      const token = await createEnrollmentToken(
        {
          network_id: String(data.get("network_id") ?? ""),
          expires_in_seconds: Number(data.get("expires_in_seconds") ?? 3600),
          max_uses: Number(data.get("max_uses") ?? 1),
          default_role_bitmap: 0,
          default_tags: String(data.get("default_tags") ?? "")
            .split(",")
            .map((tag) => tag.trim())
            .filter(Boolean),
          ...(requestedIp ? { requested_virtual_ip: requestedIp } : {}),
        },
        csrfToken,
      );
      setCreated(token);
      setShowForm(false);
      await onChanged();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "创建令牌失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader
        title="Enrollment Token"
        description="令牌明文只在创建响应中显示一次；列表只包含元数据和使用状态。"
        actions={canManage(user) && snapshot.networks.length > 0 ? (
          <button className="button button--primary" type="button" onClick={() => setShowForm((open) => !open)}>
            {showForm ? "取消创建" : "创建令牌"}
          </button>
        ) : null}
      />
      {created ? (
        <Notice tone="warning">
          <strong>请立即安全保存一次性令牌：</strong>
          <code className="secret-once">{created.token}</code>
          <button className="button button--small button--secondary" type="button" onClick={() => setCreated(null)}>我已保存并关闭</button>
        </Notice>
      ) : null}
      {showForm ? (
        <form className="panel form-panel" onSubmit={(event) => void submit(event)}>
          <div className="form-grid">
            <label>所属网络<select name="network_id" required>{snapshot.networks.map((network) => <option key={network.id} value={network.id}>{network.name}</option>)}</select></label>
            <label>有效期（秒）<input name="expires_in_seconds" type="number" min={60} max={604800} defaultValue={3600} required /></label>
            <label>最大使用次数<input name="max_uses" type="number" min={1} max={100} defaultValue={1} required /></label>
            <label>默认标签<input name="default_tags" placeholder="linux, gateway" autoComplete="off" /></label>
            <label>指定虚拟 IP（可选）<input name="requested_virtual_ip" placeholder="100.88.0.30" autoComplete="off" /></label>
          </div>
          {error ? <Notice tone="danger">{error}</Notice> : null}
          <button className="button button--primary" type="submit" disabled={busy}>{busy ? "正在创建" : "确认创建"}</button>
        </form>
      ) : null}
      {snapshot.enrollment_tokens.length === 0 ? (
        <EmptyState title="暂无注册令牌" description="创建令牌后，明文只显示一次，数据库仅保存哈希。" />
      ) : (
        <div className="table-shell">
          <table>
            <caption>注册令牌元数据</caption>
            <thead><tr><th scope="col">网络</th><th scope="col">状态</th><th scope="col">使用次数</th><th scope="col">默认标签</th><th scope="col">指定 IP</th><th scope="col">过期时间</th></tr></thead>
            <tbody>{snapshot.enrollment_tokens.map((token) => (
              <tr key={token.id}>
                <td><strong>{token.network_name}</strong><span className="cell-secondary"><code>{token.id}</code></span></td>
                <td><StatusPill state={token.state} label={tokenStateLabel(token.state)} /></td>
                <td>{token.use_count} / {token.max_uses}<span className="cell-secondary">剩余 {token.remaining_uses}</span></td>
                <td><TagList values={token.default_tags} /></td>
                <td>{token.requested_virtual_ip ? <code>{token.requested_virtual_ip}</code> : <span className="muted">自动分配</span>}</td>
                <td>{formatDateTime(token.expires_at)}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      )}
    </>
  );
}

export function GroupsPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  const nodesById = new Map(snapshot.nodes.map((node) => [node.node_id_base64, node]));
  return (
    <>
      <PageHeader title="分组与标签" description="ACL 分组来自 Controller 持久化策略；标签来自已签发节点配置。" />
      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel-heading"><div><h2>节点分组</h2><p>按网络隔离的 ACL 选择器。</p></div></div>
          {snapshot.groups.length === 0 ? <EmptyState title="暂无分组" description="提交 ACL 策略时可建立节点分组。" /> : (
            <ul className="record-list">{snapshot.groups.map((group) => (
              <li key={`${group.network_id}-${group.name}`}><strong>{group.name}</strong><span>{group.node_ids_base64.map((id) => nodesById.get(id)?.name ?? id).join("、")}</span></li>
            ))}</ul>
          )}
        </article>
        <article className="panel">
          <div className="panel-heading"><div><h2>节点标签</h2><p>只展示节点真实签发标签。</p></div></div>
          {snapshot.nodes.every((node) => node.tags.length === 0) ? <EmptyState title="暂无标签" description="创建注册令牌时可以设置默认标签。" /> : (
            <ul className="record-list">{snapshot.nodes.filter((node) => node.tags.length > 0).map((node) => (
              <li key={node.id}><strong title={node.name}>{node.name}</strong><TagList values={node.tags} /></li>
            ))}</ul>
          )}
        </article>
      </section>
    </>
  );
}

export function RoutesPage({ snapshot, user, csrfToken, onChanged }: WritablePageProps) {
  const [modes, setModes] = useState<Record<string, "routed" | "nat">>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pending = useMemo(
    () => snapshot.subnet_route_suggestions.flatMap((advertisement) =>
      advertisement.suggestions
        .filter((suggestion) => !routeAlreadyApproved(snapshot, advertisement, suggestion))
        .map((suggestion) => ({ advertisement, suggestion })),
    ),
    [snapshot],
  );

  const approve = async (advertisement: RouteSuggestionAdvertisement, suggestion: RouteSuggestion) => {
    const network = snapshot.networks.find((item) => item.id === advertisement.network_id);
    if (!network) return;
    const key = suggestionKey(advertisement, suggestion);
    const mode = modes[key] ?? "routed";
    const accepted = window.confirm(
      `确认审批 ${suggestion.prefix} 吗？\n\n网关：${advertisement.gateway_name}\n接口：${suggestion.interface_name}\n模式：${mode === "nat" ? "NAT" : "纯路由"}\n\n审批后 Controller 会发布新的签名配置。`,
    );
    if (!accepted) return;
    const existing = snapshot.subnet_routes
      .filter((route) => route.network_id === advertisement.network_id && route.state !== "revoked")
      .map((route) => ({
        route_id: route.route_id,
        gateway_node_id_base64: route.gateway_node_id_base64,
        prefix: route.prefix,
        interface_name: route.interface_name,
        mode: route.mode,
        priority: route.priority,
        enabled: route.state === "enabled",
      }));
    const routeId = makeRouteId(advertisement, suggestion);
    setBusy(key);
    setError(null);
    try {
      await replaceSubnetRoutes(
        advertisement.network_id,
        network.configuration_version,
        [...existing, {
          route_id: routeId,
          gateway_node_id_base64: advertisement.gateway_node_id_base64,
          prefix: suggestion.prefix,
          interface_name: suggestion.interface_name,
          mode,
          priority: 100,
          enabled: true,
        }],
        csrfToken,
      );
      await onChanged();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "审批路由失败");
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <PageHeader title="子网路由审批" description="Agent 仅提交签名建议；只有管理员审批后才会进入签名配置。" />
      {error ? <Notice tone="danger">{error}</Notice> : null}
      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel-heading"><div><h2>待审批建议</h2><p>{pending.length} 条未匹配现有审批。</p></div></div>
          {pending.length === 0 ? <EmptyState title="没有待审批路由" description="新的 Agent 子网建议会显示在这里。" /> : (
            <ul className="route-list">{pending.map(({ advertisement, suggestion }) => {
              const key = suggestionKey(advertisement, suggestion);
              return (
                <li key={key}>
                  <div><strong><code>{suggestion.prefix}</code></strong><span>{advertisement.gateway_name} · {suggestion.interface_name}</span><small>建议过期：{formatDateTime(advertisement.expires_at)}</small></div>
                  {canManage(user) ? <div className="inline-actions">
                    <label><span className="sr-only">路由模式</span><select value={modes[key] ?? "routed"} onChange={(event) => setModes((current) => ({ ...current, [key]: event.target.value as "routed" | "nat" }))}><option value="routed">纯路由</option><option value="nat">NAT</option></select></label>
                    <button className="button button--primary button--small" type="button" disabled={busy === key} onClick={() => void approve(advertisement, suggestion)}>{busy === key ? "审批中" : "审批"}</button>
                  </div> : <span className="muted">只读</span>}
                </li>
              );
            })}</ul>
          )}
        </article>
        <article className="panel">
          <div className="panel-heading"><div><h2>已配置路由</h2><p>包含启用、暂停和已撤销记录。</p></div></div>
          {snapshot.subnet_routes.length === 0 ? <EmptyState title="暂无已审批路由" description="完成审批后会显示路由生命周期状态。" /> : (
            <ul className="route-list">{snapshot.subnet_routes.map((route) => (
              <li key={`${route.network_id}-${route.route_id}`}><div><strong><code>{route.prefix}</code></strong><span>{route.gateway_name} · {route.interface_name}</span><small>{route.mode} · 优先级 {route.priority}</small></div><StatusPill state={route.state} label={route.state} /></li>
            ))}</ul>
          )}
        </article>
      </section>
    </>
  );
}

function suggestionKey(advertisement: RouteSuggestionAdvertisement, suggestion: RouteSuggestion): string {
  return `${advertisement.gateway_node_id_base64}-${suggestion.prefix}-${suggestion.interface_name}`;
}

function routeAlreadyApproved(
  snapshot: ConsoleSnapshot,
  advertisement: RouteSuggestionAdvertisement,
  suggestion: RouteSuggestion,
): boolean {
  return snapshot.subnet_routes.some((route) =>
    route.network_id === advertisement.network_id &&
    route.gateway_node_id_base64 === advertisement.gateway_node_id_base64 &&
    route.prefix === suggestion.prefix &&
    route.interface_name === suggestion.interface_name &&
    route.state !== "revoked",
  );
}

function makeRouteId(advertisement: RouteSuggestionAdvertisement, suggestion: RouteSuggestion): string {
  const scope = `${advertisement.gateway_node_id_base64.slice(0, 8)}-${suggestion.prefix}-${suggestion.interface_name}`
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `route-${scope}`.slice(0, 64);
}

export function RelaysPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="Relay" description="展示 Controller 签名目录与 Relay 身份签名的运行指标。" />
      {snapshot.relays.length === 0 ? <EmptyState title="Relay 目录为空" description="当前签名配置没有可用 Relay。" /> : (
        <div className="card-grid">{snapshot.relays.map((relay) => {
          const metrics = relay.metrics.value;
          const window = metrics?.window_24h;
          return (
            <article className="network-card" key={relay.relay_id_base64}>
              <div className="network-card__head"><div><span className="eyebrow">XSR/1 Relay</span><h2><code>{relay.endpoint}</code></h2></div><StatusPill state={relay.health.value === "healthy" ? "online" : "unknown"} label={availabilityText(relay.health)} /></div>
              <dl className="detail-list">
                <div><dt>优先级</dt><dd>{relay.priority}</dd></div>
                <div><dt>活跃租约</dt><dd>{metrics?.current.active_leases ?? "未采集"}</dd></div>
                <div><dt>24 小时接收 / 转发</dt><dd>{window ? `${formatBytes(window.bytes_received)} / ${formatBytes(window.bytes_forwarded)}` : "未采集"}</dd></div>
                <div><dt>24 小时转发包 / 丢弃包</dt><dd>{window ? `${window.packets_forwarded.toLocaleString("zh-CN")} / ${window.packets_dropped.toLocaleString("zh-CN")}` : "未采集"}</dd></div>
                <div><dt>24 小时平均转发延迟</dt><dd>{window?.forwarding_latency_microseconds_average === null || window === undefined ? "未采集" : `${(window.forwarding_latency_microseconds_average / 1000).toFixed(2)} ms`}</dd></div>
                <div><dt>最近上报</dt><dd>{metrics ? formatDateTime(metrics.reported_at) : "未采集"}</dd></div>
                <div><dt>目录过期</dt><dd>{formatDateTime(relay.expires_at)}</dd></div>
                <div><dt>健康原因</dt><dd>{relay.health.reason ?? "签名指标新鲜"}</dd></div>
              </dl>
            </article>
          );
        })}</div>
      )}
    </>
  );
}
