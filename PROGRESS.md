# PROGRESS.md — 当前项目状态

最后更新时间：2026-07-29 11:11 UTC  
当前 Git 提交：`c4e8e44`  
当前总状态：`PAUSED_AT_SAFE_CHECKPOINT`  
当前里程碑：`M1.2 Linux Agent 与 TUN`

---

## 已完成

- M0.1 系统、资源、网络、防火墙、Docker、1Panel、服务和工具链基线；
- 隔离 namespace 中的真实 TUN、nftables 和 capability 验证；
- 外部 Secret 文件和 Git 忽略保护；
- Rust workspace、Controller、Relay、Agent、CLI 最小组件；
- React/Vite 控制台 workspace；
- 统一 Make、CI、秘密扫描、组件集成和网络能力脚本；
- M0.1 完整验证；
- 首个 Git 检查点 `c519e34`；
- M0.2 clean-room、总体架构、威胁模型和安全假设；
- XSP/1 字节级协议、密码学设计、测试向量和一致性验证；
- M0.2 全量验证证据 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`；
- M0.2 Git 检查点 `86f8cd9`；
- M1.1 PostgreSQL schema、迁移、约束、稳定 IPAM 和追加式审计；
- 一次性 Enrollment Token 哈希存储、原子消费和并发拒绝；
- 节点本地 Ed25519 身份、固定 200 字节凭证、配置独立签名和锁定向量；
- Controller 健康检查、严格 JSON API、WebSocket challenge 认证与单调配置同步；
- 真实 PostgreSQL、HTTP、loopback WebSocket、主进程和网络能力全量验证；
- M1.1 证据 `/srv/xs-nexus/artifacts/qa/m1.1-20260729T105009Z/validate-m11.log`；
- M1.1 Git 检查点 `fe1f9db`。
- M1.2 Agent 严格配置、本地 Ed25519 身份、`0700`/`0600` 权限和原子状态持久化；
- Enrollment 凭证与独立配置签名的完整信任链验证、非规范编码和篡改拒绝；
- WSS 控制 challenge、消息边界、单调配置同步、最后有效配置保留和退避重连；
- M1.2 中间 Git 检查点 `c4e8e44`。

## 暂停点

- 已安全停在 M1.2 的 Agent 本地身份、Enrollment 与控制面信任链边界；尚未创建 TUN、路由、本地 IPC 或 systemd 服务。

## 下一步

1. 增加 Agent 与本地 Controller 的 enrollment/control 集成测试；
2. 通过 Netlink 创建、配置和回收项目 TUN 与路由；
3. 实现 systemd 服务和崩溃/卸载恢复；
4. 实现本地只读 IPC、`xs status`、`xs peers` 和 `xs diagnostics`；
5. 在隔离 namespace 中完成真实生命周期测试。

## 下一条准确命令

```bash
sed -n '1,260p' docs/AGENT_TRUST_FOUNDATION.md apps/agent/src/state.rs
```

## 最近测试

- 时间：2026-07-29 11:11 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：`cargo clippy -p xs-core -p xs-controller -p xs-agent --all-targets -- -D warnings`、`cargo test -p xs-core -p xs-protocol -p xs-agent --lib`、`./scripts/test-controller-db.sh`、`make security-check`；
- 结果：通过；
- 覆盖：严格 Clippy、Agent 配置与权限、完整 Enrollment 信任链、配置签名篡改拒绝、协议凭证向量、真实 PostgreSQL Controller 回归和秘密扫描。

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
- Windows 驱动尚未开发；
- 真实设备尚未接入；
- PostgreSQL、Redis 现有公网端口仍可达；
- 服务器提示需要维护窗口重启；
- 临时凭据后续必须轮换。

## 恢复说明

```bash
cd /srv/xs-nexus
git status --short --branch
GIT_PAGER=cat git log --oneline -10
cat PROGRESS.md
./scripts/validate-m11.sh
cargo test -p xs-agent --lib
```

然后读取：

- `AGENTS.md`
- `EXECUTION_PLAN.md`
- 当前目录级 `AGENTS.md`

## 临时服务

无项目临时服务。Controller smoke 进程已优雅退出；未启动 Compose 服务，隔离 namespace 测试退出后自动清理。

## 清理命令

```bash
make clean
```

该命令只删除项目 Rust 构建目录和控制台 `dist`，不操作 Docker、1Panel、网络或外部秘密。
