# PROGRESS.md — 当前项目状态

最后更新时间：2026-07-29 13:06 UTC  
当前 Git 提交：M1.2 完成检查点（以本文件所在提交为准）  
当前总状态：`ACTIVE_AUTONOMOUS_DEVELOPMENT`  
当前里程碑：`M1.3 XSP/1 两节点加密链路`

---

## 已完成

- M0.1 系统、资源、网络、防火墙、Docker、1Panel、服务和工具链基线；
- 隔离 namespace 中的真实 TUN、nftables 和 capability 验证；
- 外部 Secret 文件和 Git 忽略保护；
- Rust workspace、Controller、Relay、Agent、CLI 最小组件；
- React/Vite 控制台 workspace；
- 统一 Make、CI、秘密扫描、组件集成和网络能力脚本；
- M0.1 完整验证和 Git 检查点 `c519e34`；
- M0.2 clean-room、总体架构、威胁模型、安全假设、XSP/1 字节级协议与测试向量；
- M0.2 证据 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log` 和 Git 检查点 `86f8cd9`；
- M1.1 PostgreSQL schema、迁移、稳定 IPAM、一次性 Enrollment Token、节点凭证、配置签名、严格 API 和 WebSocket challenge；
- M1.1 证据 `/srv/xs-nexus/artifacts/qa/m1.1-20260729T105009Z/validate-m11.log` 和 Git 检查点 `fe1f9db`；
- M1.2 Agent 严格配置、本地 Ed25519 身份、`0700`/`0600` 权限、原子状态持久化和中间检查点 `c4e8e44`；
- Agent 与真实 Controller/PostgreSQL enrollment、challenge 认证和配置同步集成测试；
- 非持久 Linux TUN FD 与 Netlink MTU、`/32` 地址、接口启用和地址池路由；
- 默认路由保护、同名接口拒绝、shutdown、Drop 和 stale manifest 恢复；
- 严格只读 Unix IPC、`xs status`、`xs peers`、`xs diagnostics` 和 JSON 输出；
- 最小权限 systemd 单元和 `PrivateNetwork=yes` transient service 生命周期验证；
- M1.2 全量证据 `/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`。

## 当前工作点

- M1.2 已完成全部计划内实现和验证，宿主机无 `xstest0`、`xssvc0`、transient unit 或项目路由残留；
- M1.3 已开始，当前需要把既有 XSP/1 规范收敛为可执行握手、会话密钥、AEAD、抗重放和 Key Epoch 状态机；
- 在 M1.3 数据循环完成前，Agent 只读取并丢弃 TUN 数据包，不提供明文或未认证回退。

## 下一步

1. 完整复核协议目录规则、XSP/1 规范、威胁模型和现有数据头向量；
2. 先补充握手 transcript、身份/版本绑定、方向密钥和重放窗口的失败测试与确定性向量；
3. 使用成熟 X25519、HKDF 和 AEAD 原语实现独立会话状态机；
4. 接入 UDP 传输与 TUN 双向循环，拒绝未认证、篡改、重放和伪造源虚拟 IP；
5. 在两个隔离 Linux namespace 间完成双向 Ping、断流恢复、密钥轮换和 Controller 短时失联验证。

## 下一条准确命令

```bash
sed -n '1,360p' crates/protocol/AGENTS.md docs/XSP1_PROTOCOL.md docs/THREAT_MODEL.md
```

## 最近测试

- 时间：2026-07-29 13:06 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：`./scripts/validate-m12.sh`；
- 结果：通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`；
- 覆盖：严格格式化与 Clippy、Rust/Node 构建和单测、真实 PostgreSQL、Agent enrollment/control、协议向量、Unix IPC、CLI、TUN/Netlink namespace 生命周期、transient systemd、ShellCheck、依赖漏洞检查和秘密扫描。

## 当前失败

无未解决测试失败。

## 外部阻塞

- Windows 驱动真实测试需要 Windows 11 测试 VM、快照和 WDK；
- NAS 接入需要用户在 NAS 本地执行安装；
- 正式驱动签名需要外部签名流程；
- DNS 和生产防火墙变更需要人工批准；
- 现有 PostgreSQL、Redis 公网暴露整改需要用户批准修改 1Panel 或云防火墙，见 `BLK-005`。

## 当前风险

- 自研协议凭证层已实现，但握手、AEAD 数据面和独立审计尚未完成；
- M1.3 前虚拟网络业务包会安全丢弃，见 `KI-008`；
- Windows 驱动尚未开发，真实设备尚未接入；
- PostgreSQL、Redis 现有公网端口仍可达；
- 服务器提示需要维护窗口重启；
- 临时凭据后续必须轮换。

## 恢复说明

```bash
cd /srv/xs-nexus
git status --short --branch
GIT_PAGER=cat git log --oneline -10
cat PROGRESS.md
./scripts/validate-m12.sh
sed -n '1,360p' crates/protocol/AGENTS.md docs/XSP1_PROTOCOL.md docs/THREAT_MODEL.md
```

然后读取：

- `AGENTS.md`
- `EXECUTION_PLAN.md`
- 当前目录级 `AGENTS.md`

## 临时服务

无项目临时服务。Controller smoke 和 transient systemd 进程已退出，测试 namespace 已自动清理，未启动 Compose 服务。

## 清理命令

```bash
make clean
```

该命令只删除项目 Rust 构建目录和控制台 `dist`，不操作 Docker、1Panel、网络或外部秘密。
