import { useEffect, useRef, useState } from "react";

import { revokeNode } from "../api";
import {
  availabilityText,
  formatBytes,
  formatDateTime,
  nodeStateLabel,
  outcomeLabel,
} from "../format";
import type { ConsoleSnapshot, ConsoleUser, NodeSummary } from "../types";
import {
  EmptyState,
  Notice,
  PageHeader,
  StatusPill,
  TagList,
} from "../components/StateView";

export function DashboardPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  const dashboard = snapshot.dashboard;
  const metrics = [
    { label: "在线节点", value: String(dashboard.online_nodes), tone: "good" },
    { label: "离线节点", value: String(dashboard.offline_nodes), tone: "neutral" },
    {
      label: "直连节点",
      value: availabilityText(dashboard.direct_nodes),
      reason: dashboard.direct_nodes.reason,
      tone: "info",
    },
    {
      label: "中继节点",
      value: availabilityText(dashboard.relay_nodes),
      reason: dashboard.relay_nodes.reason,
      tone: "info",
    },
    { label: "待审批路由", value: String(dashboard.pending_route_suggestions), tone: "warning" },
    { label: "24 小时安全告警", value: String(dashboard.security_alerts_24h), tone: "danger" },
  ];

  return (
    <>
      <PageHeader
        title="运行概览"
        description="所有计数来自 Controller 当前会话与数据库；未接入的遥测会明确标记为未采集。"
      />
      <section className="metric-grid" aria-label="核心指标">
        {metrics.map((metric) => (
          <article className={`metric-card metric-card--${metric.tone}`} key={metric.label}>
            <span>{metric.label}</span>
            <strong>{metric.value}</strong>
            {metric.reason ? <small>{metric.reason}</small> : null}
          </article>
        ))}
      </section>

      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel-heading">
            <div>
              <h2>链路与服务</h2>
              <p>没有数据时不推断健康或连接方式。</p>
            </div>
          </div>
          <dl className="detail-list">
            <div>
              <dt>Relay 健康</dt>
              <dd title={dashboard.relay_health.reason ?? undefined}>
                {availabilityText(dashboard.relay_health)}
              </dd>
            </div>
            <div>
              <dt>24 小时流量</dt>
              <dd title={dashboard.traffic_bytes_24h.reason ?? undefined}>
                {availabilityText(dashboard.traffic_bytes_24h, formatBytes)}
              </dd>
            </div>
            <div>
              <dt>连接成功率</dt>
              <dd title={dashboard.connection_success_percent_24h.reason ?? undefined}>
                {availabilityText(
                  dashboard.connection_success_percent_24h,
                  (value) => `${value.toFixed(1)}%`,
                )}
              </dd>
            </div>
            <div>
              <dt>平均延迟</dt>
              <dd title={dashboard.average_latency_ms_24h.reason ?? undefined}>
                {availabilityText(
                  dashboard.average_latency_ms_24h,
                  (value) => `${value.toFixed(1)} ms`,
                )}
              </dd>
            </div>
          </dl>
        </article>

        <article className="panel">
          <div className="panel-heading">
            <div>
              <h2>最近活动</h2>
              <p>Controller 追加写入的最新审计事件。</p>
            </div>
          </div>
          {dashboard.recent_activity.length === 0 ? (
            <EmptyState title="暂无活动" description="完成管理操作后，审计事件会显示在这里。" />
          ) : (
            <ol className="activity-list">
              {dashboard.recent_activity.map((event) => (
                <li key={event.id}>
                  <span className={`activity-marker activity-marker--${event.outcome}`} />
                  <div>
                    <strong>{event.action}</strong>
                    <span>{event.actor_type} · {outcomeLabel(event.outcome)}</span>
                  </div>
                  <time dateTime={event.occurred_at}>{formatDateTime(event.occurred_at)}</time>
                </li>
              ))}
            </ol>
          )}
        </article>
      </section>
    </>
  );
}

export function NodesPage({
  snapshot,
  user,
  csrfToken,
  onChanged,
}: {
  snapshot: ConsoleSnapshot;
  user: ConsoleUser;
  csrfToken: string;
  onChanged: () => Promise<void>;
}) {
  const [busyNode, setBusyNode] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<NodeSummary | null>(null);
  const canManage = user.role !== "auditor";

  const revoke = async (node: NodeSummary) => {
    const accepted = window.confirm(
      `确认吊销节点“${node.name}”吗？\n\n影响：现有凭证立即失效，虚拟 IP 进入 1 小时冷却，节点需要重新注册。`,
    );
    if (!accepted) return;
    setBusyNode(node.id);
    setError(null);
    try {
      await revokeNode(node.network_id, node.node_id_base64, csrfToken);
      await onChanged();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "吊销节点失败");
    } finally {
      setBusyNode(null);
    }
  };

  return (
    <>
      <PageHeader title="节点管理" description="在线状态仅由已认证的活跃控制连接决定。" />
      {error ? <Notice tone="danger">{error}</Notice> : null}
      {snapshot.nodes.length === 0 ? (
        <EmptyState title="尚无节点" description="创建一次性注册令牌并完成 Agent 注册后，节点会出现在这里。" />
      ) : (
        <div className="table-shell">
          <table>
            <caption>已注册节点</caption>
            <thead>
              <tr>
                <th scope="col">节点</th>
                <th scope="col">状态</th>
                <th scope="col">虚拟 IP</th>
                <th scope="col">端点</th>
                <th scope="col">路径 / 延迟</th>
                <th scope="col">分组与标签</th>
                <th scope="col">最后在线</th>
                <th scope="col">操作</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.nodes.map((node) => (
                <tr key={node.id}>
                  <td>
                    <strong className="long-value" title={node.name}>{node.name}</strong>
                    <span className="cell-secondary">{node.device_type} · {node.network_name}</span>
                  </td>
                  <td>
                    <StatusPill state={node.state} label={nodeStateLabel(node.state)} />
                    <span className="cell-secondary">凭证：{node.credential_state}</span>
                  </td>
                  <td><code>{node.virtual_ip}</code></td>
                  <td>
                    <span className="long-value" title={node.public_endpoint ?? "未发现映射端点"}>
                      {node.public_endpoint ?? "未发现"}
                    </span>
                    <span className="cell-secondary">
                      {node.local_endpoints.length > 0 ? node.local_endpoints.join(" · ") : "无本地候选"}
                    </span>
                  </td>
                  <td title={node.current_path.reason ?? undefined}>
                    {availabilityText(node.current_path)}
                    <span className="cell-secondary" title={node.latency_ms.reason ?? undefined}>
                      {availabilityText(node.latency_ms, (value) => `${value.toFixed(1)} ms`)}
                    </span>
                  </td>
                  <td>
                    <TagList values={node.groups} empty="无分组" />
                    <TagList values={node.tags} empty="无标签" />
                  </td>
                  <td>{formatDateTime(node.last_seen_at)}</td>
                  <td>
                    <div className="action-stack">
                      <button
                        className="button button--secondary button--small"
                        type="button"
                        onClick={() => setSelectedNode(node)}
                      >
                        查看详情
                      </button>
                      {canManage && node.state !== "revoked" ? (
                        <button
                          className="button button--danger button--small"
                          type="button"
                          disabled={busyNode === node.id}
                          onClick={() => void revoke(node)}
                        >
                          {busyNode === node.id ? "处理中" : "吊销凭证"}
                        </button>
                      ) : (
                        <span className="muted">{canManage ? "已吊销" : "只读"}</span>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {selectedNode ? <NodeDetailDialog node={selectedNode} onClose={() => setSelectedNode(null)} /> : null}
    </>
  );
}

function NodeDetailDialog({ node, onClose }: { node: NodeSummary; onClose: () => void }) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  return (
    <dialog className="detail-dialog" ref={dialogRef} onClose={onClose} aria-labelledby="node-detail-title">
      <header className="detail-dialog__header">
        <div>
          <p className="eyebrow">节点详情</p>
          <h2 id="node-detail-title">{node.name}</h2>
          <p>{node.network_name}</p>
        </div>
        <form method="dialog">
          <button className="icon-button" type="submit" aria-label="关闭节点详情">×</button>
        </form>
      </header>
      <dl className="detail-list detail-list--dialog">
        <div><dt>状态</dt><dd><StatusPill state={node.state} label={nodeStateLabel(node.state)} /></dd></div>
        <div><dt>虚拟 IP</dt><dd><code>{node.virtual_ip}</code></dd></div>
        <div><dt>设备类型</dt><dd>{node.device_type}</dd></div>
        <div><dt>架构</dt><dd>{availabilityText(node.architecture)}</dd></div>
        <div><dt>Agent 版本</dt><dd>{availabilityText(node.agent_version)}</dd></div>
        <div><dt>公网端点</dt><dd>{node.public_endpoint ?? "未发现"}</dd></div>
        <div><dt>本地候选</dt><dd>{node.local_endpoints.length > 0 ? node.local_endpoints.join(" · ") : "无"}</dd></div>
        <div><dt>当前路径</dt><dd>{availabilityText(node.current_path)}</dd></div>
        <div><dt>Relay</dt><dd>{availabilityText(node.relay)}</dd></div>
        <div><dt>延迟</dt><dd>{availabilityText(node.latency_ms, (value) => `${value.toFixed(1)} ms`)}</dd></div>
        <div><dt>24 小时流量</dt><dd>{availabilityText(node.traffic_bytes_24h, formatBytes)}</dd></div>
        <div><dt>已发布子网</dt><dd>{node.published_subnets.length > 0 ? node.published_subnets.join(" · ") : "无"}</dd></div>
        <div><dt>最后在线</dt><dd>{formatDateTime(node.last_seen_at)}</dd></div>
        <div><dt>凭证到期</dt><dd>{formatDateTime(node.credential_expires_at)}</dd></div>
      </dl>
      <div className="detail-dialog__tags">
        <div><strong>分组</strong><TagList values={node.groups} empty="无分组" /></div>
        <div><strong>标签</strong><TagList values={node.tags} empty="无标签" /></div>
      </div>
      <form method="dialog" className="detail-dialog__footer">
        <button className="button button--secondary" type="submit">关闭</button>
      </form>
    </dialog>
  );
}

export function TopologyPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  const topology = snapshot.topology;
  return (
    <>
      <PageHeader
        title="网络拓扑"
        description="只展示 Controller 已知节点、已审批子网和真实链路遥测；当前没有链路遥测时不会绘制虚假连线。"
      />
      {topology.link_telemetry.status === "unavailable" ? (
        <Notice>{topology.link_telemetry.reason}</Notice>
      ) : null}
      {topology.nodes.length === 0 ? (
        <EmptyState title="暂无拓扑节点" description="节点完成注册后会加入拓扑。" />
      ) : (
        <section className="topology-canvas" aria-label="网络拓扑图">
          <div className="topology-controller">
            <span>Controller</span>
            <strong>拾枢控制平面</strong>
          </div>
          <div className="topology-node-grid">
            {topology.nodes.map((node) => (
              <article className={`topology-node topology-node--${node.state}`} key={node.node_id_base64}>
                <span className="topology-state">{nodeStateLabel(node.state)}</span>
                <strong title={node.name}>{node.name}</strong>
                <code>{node.virtual_ip}</code>
                <div className="topology-subnets">
                  {topology.subnets
                    .filter((subnet) => subnet.gateway_node_id_base64 === node.node_id_base64)
                    .map((subnet) => (
                      <span key={`${subnet.prefix}-${subnet.state}`}>{subnet.prefix} · {subnet.state}</span>
                    ))}
                </div>
              </article>
            ))}
          </div>
          {topology.links.length === 0 ? (
            <p className="topology-empty-links">当前未收到节点已选路径遥测，因此不绘制节点间连线。</p>
          ) : null}
        </section>
      )}
    </>
  );
}
