import { useMemo, useState, type FormEvent } from "react";

import { createUser, explainAcl } from "../api";
import { formatDateTime, outcomeLabel, roleLabel } from "../format";
import type {
  AclExplanation,
  ConsoleSnapshot,
  ConsoleUser,
} from "../types";
import {
  EmptyState,
  ErrorState,
  LoadingState,
  Notice,
  PageHeader,
  StatusPill,
} from "../components/StateView";

export function AclPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  const [networkId, setNetworkId] = useState(snapshot.networks[0]?.id ?? "");
  const [result, setResult] = useState<AclExplanation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const nodes = useMemo(
    () => snapshot.nodes.filter((node) => node.network_id === networkId && node.state !== "revoked"),
    [networkId, snapshot.nodes],
  );

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const protocol = String(data.get("protocol")) as "tcp" | "udp" | "icmp";
    const port = String(data.get("destination_port") ?? "").trim();
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await explainAcl(networkId, {
        source_node_id_base64: String(data.get("source_node_id_base64")),
        destination_node_id_base64: String(data.get("destination_node_id_base64")),
        protocol,
        ...(port ? { destination_port: Number(port) } : {}),
      }));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "ACL 解释失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader title="访问控制 ACL" description="规则按优先级展示；解释请求由 Controller 使用同一策略引擎计算。" />
      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel-heading"><div><h2>策略规则</h2><p>未匹配任何允许规则时默认拒绝。</p></div></div>
          {snapshot.acl_rules.length === 0 ? <EmptyState title="没有显式 ACL 规则" description="当前网络按默认拒绝策略执行。" /> : (
            <ul className="acl-list">{snapshot.acl_rules.map((rule) => (
              <li key={`${rule.network_id}-${rule.rule_id}`}>
                <div className="acl-priority">{rule.priority}</div>
                <div><strong title={rule.rule_id}>{rule.rule_id}</strong><span>{rule.network_name} · {rule.protocol.toUpperCase()}</span></div>
                <StatusPill state={rule.action} label={rule.action === "allow" ? "允许" : "拒绝"} />
                <details><summary>选择器详情</summary><pre>{JSON.stringify({ sources: rule.sources, destinations: rule.destinations, destination_ports: rule.destination_ports }, null, 2)}</pre></details>
              </li>
            ))}</ul>
          )}
        </article>
        <article className="panel">
          <div className="panel-heading"><div><h2>连接解释</h2><p>验证某条连接为何允许或拒绝。</p></div></div>
          {snapshot.networks.length === 0 ? <EmptyState title="暂无网络" description="创建网络和节点后才能解释连接。" /> : (
            <form className="stack-form" onSubmit={(event) => void submit(event)}>
              <label>网络<select value={networkId} onChange={(event) => { setNetworkId(event.target.value); setResult(null); }}><option value="" disabled>请选择</option>{snapshot.networks.map((network) => <option key={network.id} value={network.id}>{network.name}</option>)}</select></label>
              <label>源节点<select name="source_node_id_base64" required>{nodes.map((node) => <option key={node.id} value={node.node_id_base64}>{node.name} · {node.virtual_ip}</option>)}</select></label>
              <label>目标节点<select name="destination_node_id_base64" required>{nodes.map((node) => <option key={node.id} value={node.node_id_base64}>{node.name} · {node.virtual_ip}</option>)}</select></label>
              <div className="form-grid form-grid--compact">
                <label>协议<select name="protocol" defaultValue="tcp"><option value="tcp">TCP</option><option value="udp">UDP</option><option value="icmp">ICMP</option></select></label>
                <label>目标端口<input name="destination_port" type="number" min={1} max={65535} placeholder="443" /></label>
              </div>
              <button className="button button--primary" type="submit" disabled={busy || nodes.length < 2}>{busy ? "计算中" : "解释连接"}</button>
              {nodes.length < 2 ? <Notice>至少需要同一网络中的两个活动节点。</Notice> : null}
              {error ? <Notice tone="danger">{error}</Notice> : null}
              {result ? (
                <div className={`decision decision--${result.decision.allowed ? "allow" : "deny"}`} role="status">
                  <strong>{result.decision.allowed ? "允许" : "拒绝"}</strong>
                  <span>策略版本 {result.policy_version}</span>
                  <p>匹配规则：{result.decision.matched_rule_id ?? "无"}</p>
                  <code>{result.decision.reason}</code>
                </div>
              ) : null}
            </form>
          )}
        </article>
      </section>
    </>
  );
}

export function UsersPage({
  currentUser,
  users,
  loading,
  error,
  csrfToken,
  onChanged,
}: {
  currentUser: ConsoleUser;
  users: ConsoleUser[];
  loading: boolean;
  error: string | null;
  csrfToken: string;
  onChanged: () => Promise<void>;
}) {
  const [showForm, setShowForm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    setBusy(true);
    setFormError(null);
    try {
      await createUser(
        {
          username: String(data.get("username") ?? ""),
          display_name: String(data.get("display_name") ?? ""),
          password: String(data.get("password") ?? ""),
          role: String(data.get("role")) as ConsoleUser["role"],
        },
        csrfToken,
      );
      setShowForm(false);
      await onChanged();
    } catch (cause) {
      setFormError(cause instanceof Error ? cause.message : "创建用户失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader
        title="用户与权限"
        description="角色授权由 Controller 服务端执行；隐藏按钮不是权限边界。"
        actions={currentUser.role === "administrator" ? <button className="button button--primary" type="button" onClick={() => setShowForm((open) => !open)}>{showForm ? "取消创建" : "创建用户"}</button> : null}
      />
      {showForm ? (
        <form className="panel form-panel" onSubmit={(event) => void submit(event)}>
          <div className="form-grid">
            <label>用户名<input name="username" required minLength={3} maxLength={64} pattern="[a-z0-9](?:[a-z0-9._]|-){2,63}" autoComplete="off" /></label>
            <label>显示名称<input name="display_name" required maxLength={80} autoComplete="off" /></label>
            <label>初始密码<input name="password" type="password" required minLength={12} maxLength={128} autoComplete="new-password" /></label>
            <label>角色<select name="role" defaultValue="auditor"><option value="administrator">管理员</option><option value="operator">运维员</option><option value="auditor">审计员</option></select></label>
          </div>
          <Notice tone="warning">管理员可管理用户和全部网络；运维员可变更网络；审计员只读。</Notice>
          {formError ? <Notice tone="danger">{formError}</Notice> : null}
          <button className="button button--primary" type="submit" disabled={busy}>{busy ? "正在创建" : "确认创建"}</button>
        </form>
      ) : null}
      {loading ? <LoadingState label="正在读取用户与角色" /> : error ? <ErrorState message={error} onRetry={() => void onChanged()} /> : users.length === 0 ? <EmptyState title="暂无控制台用户" description="配置引导管理员后会显示用户。" /> : (
        <div className="table-shell"><table><caption>控制台用户</caption><thead><tr><th scope="col">用户</th><th scope="col">角色</th><th scope="col">状态</th><th scope="col">最后登录</th><th scope="col">创建时间</th></tr></thead><tbody>{users.map((user) => (
          <tr key={user.id}><td><strong title={user.display_name}>{user.display_name}</strong><span className="cell-secondary">@{user.username}</span></td><td>{roleLabel(user.role)}</td><td><StatusPill state={user.enabled ? "online" : "disabled"} label={user.enabled ? "已启用" : "已禁用"} /></td><td>{formatDateTime(user.last_login_at)}</td><td>{formatDateTime(user.created_at)}</td></tr>
        ))}</tbody></table></div>
      )}
    </>
  );
}

export function AuditPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="审计日志" description="审计表由数据库触发器保护为追加写入；页面最多显示最新 500 条。" />
      {snapshot.audit_events.length === 0 ? <EmptyState title="暂无审计事件" description="登录和管理操作会生成审计记录。" /> : (
        <div className="table-shell"><table><caption>最新审计事件</caption><thead><tr><th scope="col">时间</th><th scope="col">操作</th><th scope="col">主体</th><th scope="col">目标</th><th scope="col">结果</th></tr></thead><tbody>{snapshot.audit_events.map((event) => (
          <tr key={event.id}><td>{formatDateTime(event.occurred_at)}</td><td><code>{event.action}</code></td><td><strong>{event.actor_type}</strong><span className="cell-secondary long-value" title={event.actor_id}>{event.actor_id}</span></td><td>{event.target_type}<span className="cell-secondary long-value" title={event.target_id ?? undefined}>{event.target_id ?? "—"}</span></td><td><StatusPill state={event.outcome} label={outcomeLabel(event.outcome)} /></td></tr>
        ))}</tbody></table></div>
      )}
    </>
  );
}

export function AlertsPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="安全告警" description="当前告警直接来自被拒绝或失败的真实审计事件，不使用演示数据。" />
      {snapshot.alerts.length === 0 ? <EmptyState title="当前没有安全告警" description="此状态仅表示查询窗口内没有失败或拒绝事件。" /> : (
        <ul className="alert-list">{snapshot.alerts.map((alert) => (
          <li key={alert.audit_event_id} className={`alert-item alert-item--${alert.severity}`}><span className="alert-severity">{alert.severity === "high" ? "高" : "中"}</span><div><strong>{alert.action}</strong><span>{alert.reason_class ?? alert.outcome}</span></div><time dateTime={alert.occurred_at}>{formatDateTime(alert.occurred_at)}</time></li>
        ))}</ul>
      )}
    </>
  );
}
