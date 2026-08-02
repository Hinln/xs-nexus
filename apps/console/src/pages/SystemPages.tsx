import { useEffect, useMemo, useState, type FormEvent } from "react";

import {
  ConsoleApiError,
  createUpdateRelease,
  loadUpdatePolicies,
  loadUpdateReleases,
  replaceNodeUpdateChannel,
  replaceUpdatePolicy,
} from "../api";
import {
  EmptyState,
  ErrorState,
  ForbiddenState,
  LoadingState,
  Notice,
  PageHeader,
  StatusPill,
} from "../components/StateView";
import { formatDateTime } from "../format";
import type {
  Availability,
  ConsoleSnapshot,
  ConsoleUser,
  UpdateChannel,
  UpdatePolicy,
  UpdateRelease,
} from "../types";

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

interface UpdatesPageProps {
  snapshot: ConsoleSnapshot;
  user: ConsoleUser;
  csrfToken: string;
  onSnapshotChanged: () => Promise<void>;
}

interface UpdateDataState {
  releases: UpdateRelease[];
  policies: UpdatePolicy[];
  loading: boolean;
  error: string | null;
  forbidden: boolean;
}

const emptyUpdateData: UpdateDataState = {
  releases: [],
  policies: [],
  loading: true,
  error: null,
  forbidden: false,
};

export function UpdatesPage({ snapshot, user, csrfToken, onSnapshotChanged }: UpdatesPageProps) {
  const [data, setData] = useState<UpdateDataState>(emptyUpdateData);
  const [showReleaseForm, setShowReleaseForm] = useState(false);
  const [showPolicyForm, setShowPolicyForm] = useState(false);
  const [releaseBusy, setReleaseBusy] = useState(false);
  const [policyBusy, setPolicyBusy] = useState(false);
  const [releaseError, setReleaseError] = useState<string | null>(null);
  const [policyError, setPolicyError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [policyNetworkId, setPolicyNetworkId] = useState(snapshot.networks[0]?.id ?? "");
  const [policyChannel, setPolicyChannel] = useState<UpdateChannel>("stable");
  const [policyArchitecture, setPolicyArchitecture] = useState<"x86_64" | "aarch64">("x86_64");
  const networkKey = snapshot.networks.map((network) => network.id).join(",");
  const canManage = user.role !== "auditor";
  const updateAvailable = snapshot.system.update_management.status === "available";

  const refresh = async () => {
    setData((current) => ({ ...current, loading: true, error: null, forbidden: false }));
    try {
      const [releases, policyGroups] = await Promise.all([
        loadUpdateReleases(),
        Promise.all(snapshot.networks.map((network) => loadUpdatePolicies(network.id))),
      ]);
      setData({
        releases,
        policies: policyGroups.flat(),
        loading: false,
        error: null,
        forbidden: false,
      });
    } catch (cause) {
      setData((current) => ({
        ...current,
        loading: false,
        error: cause instanceof Error ? cause.message : "更新数据读取失败",
        forbidden: cause instanceof ConsoleApiError && cause.status === 403,
      }));
    }
  };

  useEffect(() => {
    void refresh();
  }, [networkKey]);

  useEffect(() => {
    if (!snapshot.networks.some((network) => network.id === policyNetworkId)) {
      setPolicyNetworkId(snapshot.networks[0]?.id ?? "");
    }
  }, [networkKey, policyNetworkId, snapshot.networks]);

  const compatibleReleases = useMemo(
    () => data.releases.filter((release) => release.platform === "linux" && release.architecture === policyArchitecture),
    [data.releases, policyArchitecture],
  );
  const currentPolicy = data.policies.find((policy) =>
    policy.network_id === policyNetworkId &&
    policy.channel === policyChannel &&
    policy.platform === "linux" &&
    policy.architecture === policyArchitecture
  );
  const policyFormKey = [
    policyNetworkId,
    policyChannel,
    policyArchitecture,
    currentPolicy?.generation ?? 0,
  ].join(":");

  const submitRelease = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    const formData = new FormData(form);
    const manifest = formData.get("manifest");
    const signature = formData.get("signature");
    if (!(manifest instanceof File) || !(signature instanceof File)) {
      setReleaseError("请选择签名清单和签名文件");
      return;
    }
    setReleaseBusy(true);
    setReleaseError(null);
    setNotice(null);
    try {
      const created = await createUpdateRelease(
        {
          manifest_base64: await fileToBase64Url(manifest, 4096, "签名清单"),
          signature_base64: await fileToBase64Url(signature, 64, "Ed25519 签名", true),
          archive_url: String(formData.get("archive_url") ?? "").trim(),
        },
        csrfToken,
      );
      form.reset();
      setShowReleaseForm(false);
      setNotice(`已导入不可变发布 ${created.version}（${created.architecture}）`);
      await refresh();
    } catch (cause) {
      setReleaseError(cause instanceof Error ? cause.message : "发布导入失败");
    } finally {
      setReleaseBusy(false);
    }
  };

  const submitPolicy = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const formData = new FormData(event.currentTarget);
    const releaseId = String(formData.get("release_id") ?? "");
    const release = compatibleReleases.find((item) => item.id === releaseId);
    const percentage = Number(formData.get("rollout_percentage") ?? 0);
    const paused = formData.get("paused") === "on";
    const minimumVersion = String(formData.get("minimum_version") ?? "").trim() || null;
    const networkName = snapshot.networks.find((network) => network.id === policyNetworkId)?.name ?? policyNetworkId;
    if (!release || !Number.isFinite(percentage) || percentage < 0 || percentage > 100) {
      setPolicyError("请选择匹配架构的发布并填写 0 至 100 的灰度比例");
      return;
    }
    const impact = paused
      ? "暂停后不会向任何节点下发该通道更新。"
      : `将允许稳定分桶中的 ${percentage}% 节点接收 ${release.version}；低于最低版本的节点会被强制纳入。`;
    if (!window.confirm(`确认更新“${networkName}”的 ${channelLabel(policyChannel)}策略？\n\n${impact}`)) {
      return;
    }
    setPolicyBusy(true);
    setPolicyError(null);
    setNotice(null);
    try {
      const updated = await replaceUpdatePolicy(
        policyNetworkId,
        policyChannel,
        "linux",
        policyArchitecture,
        {
          expected_generation: currentPolicy?.generation ?? 0,
          release_id: release.id,
          minimum_version: minimumVersion,
          rollout_basis_points: Math.round(percentage * 100),
          paused,
        },
        csrfToken,
      );
      setNotice(`策略已保存为第 ${updated.generation} 代${updated.paused ? "，当前暂停" : ""}`);
      await refresh();
    } catch (cause) {
      setPolicyError(cause instanceof Error ? cause.message : "更新策略保存失败");
    } finally {
      setPolicyBusy(false);
    }
  };

  return (
    <>
      <PageHeader
        title="更新管理"
        description="导入离线签名的不可变发布，并按网络、通道和架构控制暂停、最低版本与确定性灰度。"
        actions={<button className="button button--secondary" type="button" onClick={() => void refresh()} disabled={data.loading}>刷新更新数据</button>}
      />
      <CapabilityCard title="签名更新通道" capability={snapshot.system.update_management} availableLabel="可用" />
      {!updateAvailable ? <Notice tone="warning">Controller 未加载离线签名公钥，不能导入新发布；已有策略仍可暂停或收紧。</Notice> : null}
      {notice ? <Notice>{notice}</Notice> : null}

      {data.loading && data.releases.length === 0 && data.policies.length === 0 ? (
        <LoadingState label="正在读取签名发布与灰度策略" />
      ) : data.forbidden ? (
        <ForbiddenState />
      ) : data.error ? (
        <ErrorState message={data.error} onRetry={() => void refresh()} />
      ) : (
        <>
          {canManage ? (
            <div className="update-actions">
              <button className="button button--primary" type="button" disabled={!updateAvailable} onClick={() => setShowReleaseForm((open) => !open)}>
                {showReleaseForm ? "取消导入" : "导入签名发布"}
              </button>
              <button className="button button--secondary" type="button" disabled={snapshot.networks.length === 0} onClick={() => setShowPolicyForm((open) => !open)}>
                {showPolicyForm ? "关闭策略表单" : "配置灰度策略"}
              </button>
            </div>
          ) : <Notice>审计员可查看发布、策略和节点更新状态，但不能执行写操作。</Notice>}

          {showReleaseForm ? (
            <form className="panel form-panel" onSubmit={(event) => void submitRelease(event)}>
              <div className="panel-heading"><div><h2>导入离线签名发布</h2><p>浏览器只上传公开清单与签名；离线私钥不得进入 Controller 或控制台。</p></div></div>
              <div className="form-grid">
                <label>发布清单<input name="manifest" type="file" accept=".manifest,text/plain" required /></label>
                <label>Ed25519 签名<input name="signature" type="file" accept=".sig,application/octet-stream" required /></label>
                <label>HTTPS 归档地址<input name="archive_url" type="url" required placeholder="https://updates.example/xs-nexus-1.2.3-x86_64-unknown-linux-gnu.tar.gz" autoComplete="off" /></label>
              </div>
              <Notice>Controller 会再次验证签名、九字段清单、归档文件名和 HTTPS 地址；导入后内容不可修改。</Notice>
              {releaseError ? <Notice tone="danger">{releaseError}</Notice> : null}
              <button className="button button--primary" type="submit" disabled={releaseBusy}>{releaseBusy ? "正在验证并导入" : "验证并导入"}</button>
            </form>
          ) : null}

          {showPolicyForm ? (
            <form className="panel form-panel" key={policyFormKey} onSubmit={(event) => void submitPolicy(event)}>
              <div className="panel-heading"><div><h2>灰度与最低版本策略</h2><p>保存使用当前代次进行并发冲突检测；策略暂停始终覆盖最低版本。</p></div>{currentPolicy ? <StatusPill state={currentPolicy.paused ? "paused" : "active"} label={`第 ${currentPolicy.generation} 代`} /> : null}</div>
              <div className="form-grid">
                <label>网络<select value={policyNetworkId} onChange={(event) => setPolicyNetworkId(event.target.value)} required>{snapshot.networks.map((network) => <option key={network.id} value={network.id}>{network.name}</option>)}</select></label>
                <label>通道<select value={policyChannel} onChange={(event) => setPolicyChannel(event.target.value as UpdateChannel)}><option value="stable">稳定</option><option value="testing">测试</option><option value="development">开发</option></select></label>
                <label>架构<select value={policyArchitecture} onChange={(event) => setPolicyArchitecture(event.target.value as "x86_64" | "aarch64")}><option value="x86_64">Linux x86_64</option><option value="aarch64">Linux aarch64</option></select></label>
                <label>目标发布<select name="release_id" defaultValue={currentPolicy?.release.id ?? compatibleReleases[0]?.id ?? ""} required disabled={compatibleReleases.length === 0}><option value="" disabled>请选择发布</option>{compatibleReleases.map((release) => <option key={release.id} value={release.id}>{release.version} · {release.archive_name}</option>)}</select></label>
                <label>最低版本（可选）<input name="minimum_version" defaultValue={currentPolicy?.minimum_version ?? ""} pattern="[0-9]+\.[0-9]+\.[0-9]+" placeholder="1.2.0" autoComplete="off" /></label>
                <label>灰度比例（%）<input name="rollout_percentage" type="number" min={0} max={100} step={0.01} defaultValue={(currentPolicy?.rollout_basis_points ?? 0) / 100} required /></label>
              </div>
              <label className="checkbox-field"><input name="paused" type="checkbox" defaultChecked={currentPolicy?.paused ?? true} />暂停该范围的全部更新下发</label>
              {compatibleReleases.length === 0 ? <Notice tone="warning">尚无匹配 {policyArchitecture} 的 Linux 签名发布，不能保存该范围。</Notice> : null}
              {policyError ? <Notice tone="danger">{policyError}</Notice> : null}
              <button className="button button--primary" type="submit" disabled={policyBusy || compatibleReleases.length === 0}>{policyBusy ? "正在保存" : "确认影响并保存"}</button>
            </form>
          ) : null}

          <UpdateTables
            snapshot={snapshot}
            releases={data.releases}
            policies={data.policies}
            canManage={canManage}
            csrfToken={csrfToken}
            onSnapshotChanged={onSnapshotChanged}
          />
        </>
      )}
    </>
  );
}

function UpdateTables({
  snapshot,
  releases,
  policies,
  canManage,
  csrfToken,
  onSnapshotChanged,
}: {
  snapshot: ConsoleSnapshot;
  releases: UpdateRelease[];
  policies: UpdatePolicy[];
  canManage: boolean;
  csrfToken: string;
  onSnapshotChanged: () => Promise<void>;
}) {
  const channelKey = snapshot.nodes
    .map((node) => `${node.id}:${node.update_channel.value ?? "stable"}`)
    .join(",");
  const [draftChannels, setDraftChannels] = useState<Record<string, UpdateChannel>>({});
  const [channelBusy, setChannelBusy] = useState<string | null>(null);
  const [channelError, setChannelError] = useState<string | null>(null);
  const [channelNotice, setChannelNotice] = useState<string | null>(null);

  useEffect(() => {
    setDraftChannels(Object.fromEntries(
      snapshot.nodes.map((node) => [node.id, node.update_channel.value ?? "stable"]),
    ));
  }, [channelKey]);

  const saveNodeChannel = async (nodeId: string) => {
    const node = snapshot.nodes.find((candidate) => candidate.id === nodeId);
    const channel = draftChannels[nodeId];
    if (!node || !channel || channel === node.update_channel.value) return;
    const network = snapshot.networks.find((candidate) => candidate.id === node.network_id);
    if (!network) {
      setChannelError("节点所属网络不存在，请刷新后重试。");
      return;
    }
    if (!window.confirm(`确认将“${node.name}”切换到${channelLabel(channel)}通道？\n\nController 将发布新的签名配置，Agent 收到后会立即按新通道重新上报。`)) {
      return;
    }
    setChannelBusy(nodeId);
    setChannelError(null);
    setChannelNotice(null);
    try {
      const result = await replaceNodeUpdateChannel(
        node.network_id,
        node.node_id_base64,
        {
          expected_configuration_version: network.configuration_version,
          update_channel: channel,
        },
        csrfToken,
      );
      await onSnapshotChanged();
      setChannelNotice(`节点“${node.name}”已切换到${channelLabel(channel)}通道，配置版本为 ${result.configuration_version}。`);
    } catch (cause) {
      setChannelError(cause instanceof Error ? cause.message : "节点升级通道保存失败");
    } finally {
      setChannelBusy(null);
    }
  };

  return (
    <div className="update-sections">
      <section aria-labelledby="release-list-title">
        <h2 id="release-list-title">已验证发布</h2>
        {releases.length === 0 ? <EmptyState title="尚无签名发布" description="先由离线发布流程生成清单和签名，再导入公开材料。" /> : (
          <div className="table-shell"><table><caption>已验证更新发布</caption><thead><tr><th scope="col">版本</th><th scope="col">目标</th><th scope="col">归档</th><th scope="col">大小</th><th scope="col">SHA-256</th><th scope="col">导入时间</th></tr></thead><tbody>{releases.map((release) => <tr key={release.id}><td><strong>{release.version}</strong><span className="cell-secondary">{release.platform} · {release.architecture}</span></td><td><code>{release.target}</code></td><td><a href={release.archive_url} target="_blank" rel="noreferrer">{release.archive_name}</a></td><td>{formatBytes(release.archive_size)}</td><td><code className="long-value" title={release.archive_sha256}>{release.archive_sha256}</code></td><td>{formatDateTime(release.created_at)}</td></tr>)}</tbody></table></div>
        )}
      </section>
      <section aria-labelledby="policy-list-title">
        <h2 id="policy-list-title">当前灰度策略</h2>
        {policies.length === 0 ? <EmptyState title="尚无更新策略" description="没有策略时 Controller 不会向节点下发更新。" /> : (
          <div className="table-shell"><table><caption>当前更新灰度策略</caption><thead><tr><th scope="col">网络</th><th scope="col">范围</th><th scope="col">目标</th><th scope="col">最低版本</th><th scope="col">灰度</th><th scope="col">状态</th><th scope="col">代次</th></tr></thead><tbody>{policies.map((policy) => <tr key={`${policy.network_id}-${policy.channel}-${policy.platform}-${policy.architecture}`}><td>{snapshot.networks.find((network) => network.id === policy.network_id)?.name ?? policy.network_id}</td><td>{channelLabel(policy.channel)}<span className="cell-secondary">{policy.platform} · {policy.architecture}</span></td><td><strong>{policy.release.version}</strong></td><td>{policy.minimum_version ?? "—"}</td><td>{(policy.rollout_basis_points / 100).toFixed(2)}%</td><td><StatusPill state={policy.paused ? "paused" : "active"} label={policy.paused ? "已暂停" : "下发中"} /></td><td>{policy.generation}<span className="cell-secondary">{formatDateTime(policy.updated_at)}</span></td></tr>)}</tbody></table></div>
        )}
      </section>
      <section aria-labelledby="node-update-title">
        <h2 id="node-update-title">节点更新状态与通道</h2>
        <p>通道由 Controller 签入节点配置；版本、架构和处理状态由 Agent 身份密钥签名上报。</p>
        {channelNotice ? <Notice>{channelNotice}</Notice> : null}
        {channelError ? <Notice tone="danger">{channelError}</Notice> : null}
        {snapshot.nodes.length === 0 ? <EmptyState title="尚无节点" description="节点完成注册后可在此分配升级通道并查看签名运行时上报。" /> : (
          <div className="table-shell">
            <table>
              <caption>节点升级通道与签名更新上报</caption>
              <thead><tr><th scope="col">节点</th><th scope="col">版本</th><th scope="col">分配通道</th><th scope="col">状态</th><th scope="col">关联发布</th><th scope="col">上报时间</th></tr></thead>
              <tbody>{snapshot.nodes.map((node) => {
                const assignedChannel = node.update_channel.value ?? "stable";
                const draftChannel = draftChannels[node.id] ?? assignedChannel;
                return <tr key={node.id}>
                  <td><strong>{node.name}</strong><span className="cell-secondary">{node.network_name} · {node.architecture.value ?? "未上报架构"}</span></td>
                  <td>{node.agent_version.value ?? "未上报"}</td>
                  <td>{canManage && node.state !== "revoked" ? <div className="inline-control">
                    <select aria-label={`节点 ${node.name} 的升级通道`} value={draftChannel} disabled={channelBusy === node.id} onChange={(event) => setDraftChannels((current) => ({ ...current, [node.id]: event.target.value as UpdateChannel }))}>
                      <option value="stable">稳定</option><option value="testing">测试</option><option value="development">开发</option>
                    </select>
                    <button className="button button--secondary" type="button" disabled={channelBusy === node.id || draftChannel === assignedChannel} onClick={() => void saveNodeChannel(node.id)}>{channelBusy === node.id ? "保存中" : "保存"}</button>
                  </div> : channelLabel(assignedChannel)}</td>
                  <td><StatusPill state={node.update_state.value ?? "unknown"} label={updateStateLabel(node.update_state.value, node.update_error_code)} /></td>
                  <td><code>{node.update_release_id ?? "—"}</code></td>
                  <td>{node.update_reported_at ? formatDateTime(node.update_reported_at) : "未上报"}</td>
                </tr>;
              })}</tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

async function fileToBase64Url(file: File, maximum: number, label: string, exact = false): Promise<string> {
  if (file.size === 0 || file.size > maximum || (exact && file.size !== maximum)) {
    throw new Error(exact ? `${label}必须恰好为 ${maximum} 字节` : `${label}大小必须在 1 至 ${maximum} 字节之间`);
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  const binary = String.fromCharCode(...bytes);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "");
}

function channelLabel(channel: UpdateChannel): string {
  if (channel === "stable") return "稳定";
  if (channel === "testing") return "测试";
  return "开发";
}

function updateStateLabel(state: string | null, errorCode: string | null): string {
  if (state === "idle") return "空闲";
  if (state === "downloading") return "下载中";
  if (state === "staged") return "已暂存";
  if (state === "applying") return "应用中";
  if (state === "failed") return errorCode ? `失败：${errorCode}` : "失败";
  return "未上报";
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
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
