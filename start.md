# XS Nexus 全新 Codex 环境启动提示词

你正在接手一个没有任何历史对话记忆的 XS Nexus 开发任务。必须以仓库文件和 Git 历史为唯一事实来源，不得根据本提示词之外的猜测补写进度、测试结果或外部环境状态。

## 一、取得仓库并确认检查点

私有仓库：`https://github.com/Hinln/xs-nexus`

```bash
git clone https://github.com/Hinln/xs-nexus.git
cd xs-nexus
git checkout main
git pull --ff-only
git status --short --branch
git rev-parse HEAD
```

首次接手时，预期基线检查点为：

```text
cca33fa8c4ea0e2cf99955821f82c0692b7e84d5
```

如果 GitHub `main` 已经前进，应先阅读新增提交和文档，以最新 `origin/main` 为准，不得强制回退到上述旧检查点。工作树必须先保持干净；若发现未知改动，不得覆盖、删除或混入新提交。

## 二、正式执行指令

首先完整读取 `BOOTSTRAP_PROMPT.md`，并将其全部内容作为本次任务的正式执行指令。

同时完整读取并遵守：

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

这些文件中的更具体规则优先于本交接摘要。不得只阅读标题、节选或旧对话摘要。

## 三、当前正式开发状态

- 当前分支：`main`。
- 最新已验证检查点：`cca33fa feat(windows): add protected service host boundary`。
- 当前里程碑：M6.1 `IN_PROGRESS`。
- M6.2：`BLOCKED_EXTERNAL`。
- 最新 M6.1 全量验证证据：`/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`。
- 证据目录位于原开发服务器，不在 Git 仓库中；没有实际读取该目录时，不得声称复核了证据内容。

M6.1 已形成检查点的工作包括：

- 跨平台 xsnet ABI、状态机、数据面和生命周期模型；
- Windows 11 24H2 x64 UMDF 2.33 / NetAdapterCx 2.5 驱动源码；
- 严格测试安装器、测试包构建脚本和六阶段 VM 采证流程；
- exact ABI v1、DriverVer、driver-store 和安装状态一致性门禁；
- Rust `XsnetTransport` 与无后台重试的 `XsnetDeviceSession`；
- 固定名称、首实例、拒绝远程、受限 DACL 的 Windows 本地命名管道；
- exact protected DACL、reparse 拒绝和 write-through 替换的 Windows 私有存储；
- 固定 `XsNexusAgent`、四阶段状态机、一次性 STOP/SHUTDOWN 桥接的 Windows Service/SCM 边界；
- 对应源码门禁、MSVC target check、交叉 Clippy、Linux 回归和文档记录。

M6.1 仍未完成，原因包括：

- 完整 Windows Agent 仍因缺少 Windows SDK 工具链停在 `ring/lib.exe`；
- 没有 WDK、Windows 11 测试 VM、测试签名和 Driver Verifier 实机结果；
- 命名管道、私有存储、SCM 和设备 I/O 尚未在 Windows 实际运行；
- 空 TX、满 RX、取消和移除状态尚无权威 no-commit 映射；
- Windows 路由管理、完整 Agent 数据面接入、正式安装升级和生产签名尚未完成。

## 四、恢复工作的准确断点

下一项不受 Windows VM 阻塞的正式工作是：**Windows 路由管理准备**。

暂停前只完成了需求审计，没有提交可用实现。不得假定存在可复用的 Windows 路由草案，也不得把其他工作目录中的未验证文件直接复制进仓库。

开始实现前必须重新核对：

1. `apps/agent/src/network.rs`、`lifecycle.rs` 和 `runtime.rs` 的 Linux 网络生命周期；
2. `crates/core/src/routes.rs` 的子网路由安全规则；
3. Microsoft IP Helper 的地址、路由、LUID、DAD、创建和精确删除语义；
4. Windows 地址由 `CreateUnicastIpAddressEntry` 创建后是非持久的，并且完成 DAD 前不可宣称可用；
5. `GetIpForwardTable2` 返回表必须有数量上限并由 `FreeMibTable` 释放；
6. 项目路由必须使用明确 LUID、规范前缀、固定 on-link 下一跳和受控 metric；
7. 默认路由不得被项目修改，任何非项目系统路由重叠必须失败关闭；
8. 变更必须有 additions-first、失败补偿、回滚失败显式上报和可信 manifest 所有权边界；
9. 在 DAD、PnP、睡眠恢复和精确错误映射获得 VM 证据前，不得接入真实 Agent runtime。

普通技术选择自行决定，并记录到 `DECISIONS.md`。测试失败时自行定位并修复，不得删除、跳过、忽略或弱化失败测试。

## 五、原开发服务器恢复规则

原远程工作目录：

```text
/srv/xs-nexus
```

服务器登录凭据、数据库密码、Redis 密码和私钥必须由用户在新会话中临时提供，不得从仓库猜测，也不得写入 Git、日志、文档、命令历史或测试产物。

连接服务器后先执行：

```bash
cd /srv/xs-nexus
pwd
git status --short --branch
git log -5 --oneline
git rev-parse HEAD
```

如果服务器 HEAD 落后于 GitHub，必须先判断是否有未提交改动和服务器专用文件，再使用安全的 fast-forward 流程同步；不得直接 `reset --hard` 覆盖未知状态。

## 六、宿主与 1Panel 保护边界

- `1panel-network` 只能作为 external network 引用。
- 不得删除、重建、重命名、断开或修改 `1panel-network`。
- 不得删除、重建或修改任何现有 1Panel 容器、数据库、Redis、卷、路由或生产资源。
- 网络实验只能在项目创建并可完整回收的 network namespace 中执行。
- 每次涉及网络、容器或系统服务的验证前后，都要保存并比较 Docker、`1panel-network`、默认路由、nftables、namespace、TUN 和失败服务状态。
- 不得修改生产防火墙；正式 DNS 和生产端口仍属于人工门禁。

## 七、安全与提交规则

- 不提交真实 `.env`、密码、token、私钥、证书私钥、数据库连接串、测试产物、构建缓存或 `artifacts/qa`。
- 提交前运行仓库秘密扫描、`git diff --check` 和相关源码门禁。
- 所有新增 unsafe 必须隔离、最小化、计数并由源码验证器固定；不得降低 workspace 的 unsafe 规则。
- 不使用 Wintun、TAP 或第三方组网实现替代自研协议和 `xsnet`。
- 不把交叉编译、源码扫描、模型测试或 mock 测试描述为 Windows 实机结果。
- NAS、Windows VM、日常 Windows 电脑、正式 DNS、正式驱动签名和生产防火墙属于人工门禁；到达门禁时更新 `BLOCKERS.md`，继续完成所有不受阻塞影响的工作。
- 每个独立子阶段都要更新 `PROGRESS.md`、`DECISIONS.md`、`QA_MATRIX.md`、`SECURITY_REVIEW.md`、`KNOWN_ISSUES.md`、`BUG_LOOP.md` 及受影响的验收文档。
- 运行实际验证后创建 Git 检查点，并保证工作树干净。

## 八、开始执行

完成上述核对后，从 Windows 路由管理准备继续实际开发。先审计、写测试和源码门禁，再实现最小隔离边界；未验证前不得接入 Agent runtime。

不要只回复计划，不要等待逐阶段批准。持续完成所有不受人工门禁阻塞的工作；只有在用户明确要求暂停时才停止。
