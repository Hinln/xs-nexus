# BLOCKERS.md — 外部阻塞

只有无法由代码、配置、测试或本地环境自行解决的外部条件才能写入本文件。

---

## BLK-001 Windows 测试环境

- 状态：阻塞 Windows 实机验收
- 需要：
  - Windows 11 测试 VM；
  - 快照；
  - Visual Studio Build Tools；
  - WDK；
  - 测试签名模式；
  - Driver Verifier。
- 不阻塞：
  - Linux 核心；
  - Controller；
  - Relay；
  - Console；
  - 驱动代码和交叉构建准备。
- 解除步骤：用户提供可测试 VM，Codex执行安装和验证。

---

## BLK-002 NAS 首次接入

- 状态：阻塞真实 NAS 验收
- 原因：NAS 位于 `192.168.0.100` 私网，公网服务器不能直接访问。
- 需要：用户在 NAS 本地执行签名安装命令。
- 不阻塞：完整 NAS 安装包、文档和虚拟测试。
- 解除：NAS 主动注册成功。

---

## BLK-003 DNS 和生产防火墙

- 状态：阻塞正式域名上线
- 需要：用户批准 DNS 和端口。
- 不阻塞：IP 和临时端口测试。

---

## BLK-004 正式驱动签名

- 状态：阻塞正式 Windows 公布
- 不阻塞：测试签名和测试 VM。
- 解除：完成签名流程。

---

## 新阻塞模板

```markdown
## BLK-NNN 标题

- 状态：
- 首次发现：
- 外部条件：
- 证据：
- 已尝试：
- 不受影响工作：
- 解除步骤：
- 解除后验证：
```

---

## BLK-005 现有数据库公网暴露整改门禁

- 状态：阻塞最终 1Panel 安全验收和 Release Candidate
- 首次发现：2026-07-29
- 外部条件：用户批准修改现有 1Panel 端口映射或云防火墙规则，并安排临时凭据轮换。
- 证据：`/srv/xs-nexus-qa/baseline/20260729T094000Z/external-port-check.txt`
- 已尝试：完成只读 Docker、监听端口和外部连通性检查；未修改现有资源。
- 不受影响工作：仓库开发、隔离 namespace 实验、Controller/Agent/Relay/Console 实现和本地测试。
- 解除步骤：限制 TCP `5432`、`6379` 的公网访问，保留 `1panel-network` 容器内访问，轮换临时数据库和 Redis 密码。
- 解除后验证：外部端口不可达、容器 DNS 和内部端口可达、现有 1Panel 服务健康、项目数据库连接通过。
