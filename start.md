# XS Nexus 全新 Codex 环境启动提示词

你正在接手一个没有历史对话记忆的 XS Nexus 开发任务。必须以最新私有 GitHub 仓库、仓库内正式文档、Git 历史和可读取的原始证据为事实来源；不得根据本提示词之外的猜测补写进度、测试结果或外部环境状态。

## 一、取得仓库

私有仓库：`https://github.com/Hinln/xs-nexus`

```bash
git clone https://github.com/Hinln/xs-nexus.git
cd xs-nexus
git checkout main
git pull --ff-only
git status --short --branch
git log -5 --oneline
git rev-parse HEAD
```

始终以最新 `origin/main` 为准，不要回退到本文件记录的旧提交。工作树必须先保持干净；发现未知改动时不得覆盖、删除或混入新提交。

## 二、正式执行指令

首先完整读取 `BOOTSTRAP_PROMPT.md`，并将其全部内容作为正式执行指令。同时完整读取并遵守：

- `AGENTS.md`
- `XS_Nexus_Codex_Development_Brief.md`
- `EXECUTION_PLAN.md`
- `IMPLEMENT.md`
- `ACCEPTANCE.md`
- `QA_MATRIX.md`
- `BUG_LOOP.md`
- `VISUAL_QA.md`
- `SECURITY_REVIEW.md`
- `ENVIRONMENT.md`
- `RECOVERY_RUNBOOK.md`
- `RELEASE_CHECKLIST.md`
- `PROGRESS.md`
- `DECISIONS.md`
- `KNOWN_ISSUES.md`
- `BLOCKERS.md`
- `FINAL_REPORT.md`

若文档、代码和本提示词冲突，以安全边界、禁止事项、验收规则和最新证据优先；必须调查并修正文档矛盾，不能选择更宽松的说法。

## 三、当前可验证状态

- 当前总状态：`BLOCKED_EXTERNAL`。
- 所有不受外部门禁影响的实现、回归、M5.2 全量验证和生产候选滚动部署已经完成。
- 不得标记 Release Candidate，不得描述为公网生产就绪。
- 已完成全量验证并部署的代码 revision：`ff9551d322067c934d2ac7d55a62af8896660bb3`。
- GitHub `main` 可能包含该 revision 之后的纯文档交接提交；这不代表容器内运行 revision 已变化。

最终 M5.2 证据：

```text
/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo/artifacts/qa/m5.2-20260808T100655Z
```

生产候选部署证据：

```text
/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z
```

数据库外部暴露验证：

```text
/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z
```

没有实际登录服务器并读取这些目录时，不得声称已经复核证据内容。

## 四、服务器恢复入口

生产服务器工作目录：

```text
/srv/xs-nexus
```

服务器登录凭据、私钥、数据库密码和其他秘密由项目所有者在新会话中单独提供。不得将它们写入 Git、Markdown、日志、命令历史、测试产物或构建缓存。

登录后先执行：

```bash
cd /srv/xs-nexus
pwd
git status --short --branch
git log -5 --oneline
git rev-parse HEAD
docker version
docker compose version
docker network inspect 1panel-network
```

生产常驻 Controller、Relay、Console 和 PostgreSQL 已部署；当前运行 OCI revision 应为上面的已验证代码 revision。仅文档提交前进时不得因此无意义地重建服务。

## 五、Windows 技术边界

项目所有者已批准首版 Windows 使用发行方签名的官方 Wintun `0.14.1` x64 DLL，但它只能作为 `xs-windows-wintun` 的 L3 虚拟网卡适配器：

- 必须固定官方归档哈希、DLL 哈希、Authenticode 签名主体和许可证。
- Wintun 不得实现或替代 Enrollment、节点身份、XSP/1、密钥协商、加密、重放保护、ACL、IPAM、路由授权、NAT 穿透、候选路径或 Relay。
- 自研 `drivers/windows-xsnet` 继续作为独立的测试签名实验路径，不得依赖 Wintun，也不得宣称获得正式签名或生产分发资格。
- Windows 在线结果必须来自受控 Windows 实机或 VM；交叉编译、静态检查、模型测试和 Linux 模拟不能冒充实机证据。

## 六、1Panel 与宿主保护

- `1panel-network` 只能作为 external network 引用。
- 禁止删除、重建、重命名、断开或修改 `1panel-network`。
- 禁止修改或删除任何无关 1Panel 容器、数据库、Redis、卷、路由、网站或生产资源。
- 网络实验只能在项目创建且可完整回收的 network namespace 中执行。
- 禁止修改生产防火墙、默认路由或 SSH 管理入口。
- 每次宿主验证前后都要保存并比较 Docker 网络、`1panel-network`、默认路由、nftables、namespace、TUN 和失败服务状态。

## 七、当前外部门禁

只有获得相应真实环境或人工授权后才能继续以下项目：

1. 受控 Windows VM 上的在线 Enrollment、SCM 服务、CLI、双向网络、卸载和重装闭环；
2. Windows 10 兼容性验证；
3. 真实 NAS 安装和业务验收；
4. `vpn.xiashikeji.cn` CDN/源站 TLS 修复及生产防火墙批准；
5. 正式离线发布签名密钥仪式、正式备份 identity 和真实异地主机恢复；
6. 所有临时凭据轮换；
7. 独立协议与密码学安全审计；
8. 真实跨地域和公网容量测试。

当前 `vpn.qinwen.co` 的健康入口、Linux/Windows 引导、10 个发布文件逐字节比较和未知文件 404 已有服务器证据；任务书计划域名 `vpn.xiashikeji.cn` 的功能路径仍返回 CDN 525。不得把边缘 TLS 成功或根路径 404 描述为该域名已恢复。

## 八、恢复后的执行规则

1. 对比 GitHub `main`、生产 `/srv/xs-nexus` 和运行容器 OCI revision。
2. 复核 `PROGRESS.md`、`BLOCKERS.md`、`FINAL_REPORT.md` 和上述证据目录。
3. 若没有新的外部门禁条件，只执行一致性审计和安全复核，不伪造实机或人工验收结果。
4. 若门禁解除，从 `BLOCKERS.md` 中最早可执行项开始，先建立失败测试或验收步骤，再实现、验证、记录证据和提交。
5. 测试失败时自行定位修复；不得删除、跳过、忽略或弱化失败测试。
6. 普通技术选择自行决定并记录到 `DECISIONS.md`。
7. 每个检查点更新全部受影响的进度、验收、QA、安全、问题和发布文档。
8. 提交前运行秘密扫描、`git diff --check` 和适用门禁，保证工作树干净，再安全 fast-forward 同步私有 GitHub。

不要只回复计划，不要等待逐阶段批准。持续完成所有不受人工门禁阻塞的工作；只有真实外部条件不足时才保持 `BLOCKED_EXTERNAL`，并如实记录解除条件和证据边界。
