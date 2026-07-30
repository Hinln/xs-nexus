import { PageHeader, Notice, StatusPill } from "../components/StateView";
import type { Availability, ConsoleSnapshot } from "../types";

function CapabilityCard({
  title,
  capability,
  availableLabel,
}: {
  title: string;
  capability: Availability<string>;
  availableLabel: string;
}) {
  const available = capability.status === "available";
  return (
    <article className="capability-card">
      <div>
        <span className="eyebrow">能力状态</span>
        <h2>{title}</h2>
      </div>
      <StatusPill state={available ? "online" : "unknown"} label={available ? availableLabel : "尚未接入"} />
      <p>{available ? capability.value : capability.reason}</p>
    </article>
  );
}

export function UpdatesPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="更新管理" description="只展示 Controller 当前具备的更新能力，不伪造版本或发布通道。" />
      <CapabilityCard title="签名更新通道" capability={snapshot.system.update_management} availableLabel="可用" />
      <Notice>节点版本尚未由 Agent 上报；M6.1 完成签名清单、灰度、回滚和失败恢复前，本页不会提供无效操作。</Notice>
    </>
  );
}

export function SettingsPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="系统设置" description="敏感配置不会返回浏览器；此处仅展示可公开的运行标识。" />
      <section className="settings-grid">
        <article className="panel">
          <div className="panel-heading"><div><h2>Controller</h2><p>当前请求已成功读取数据库快照。</p></div><StatusPill state="online" label="可读" /></div>
          <dl className="detail-list">
            <div><dt>数据库</dt><dd>{snapshot.system.database.value ?? "不可用"}</dd></div>
            <div><dt>凭证签名 Key ID</dt><dd><code>{snapshot.system.credential_signing_key_id}</code></dd></div>
            <div><dt>配置签名 Key ID</dt><dd><code>{snapshot.system.configuration_signing_key_id}</code></dd></div>
          </dl>
        </article>
        <CapabilityCard title="Relay 指标" capability={snapshot.system.relay_metrics} availableLabel="已接入" />
        <CapabilityCard title="路径遥测" capability={snapshot.system.path_telemetry} availableLabel="已接入" />
      </section>
      <Notice tone="warning">管理 API Token、数据库连接串和签名私钥不会下发到控制台。</Notice>
    </>
  );
}

export function BackupPage({ snapshot }: { snapshot: ConsoleSnapshot }) {
  return (
    <>
      <PageHeader title="备份与恢复" description="备份操作属于高风险流程，必须在加密、完整性校验和恢复演练完成后启用。" />
      <CapabilityCard title="备份恢复流程" capability={snapshot.system.backup_restore} availableLabel="可用" />
      <Notice tone="warning">当前没有可执行的备份按钮，避免让未实现的流程看起来可用。M8.1 将补齐加密备份、恢复验证和审计。</Notice>
    </>
  );
}
