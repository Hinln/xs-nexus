# EXECUTION_PLAN.md — 拾枢（XS Nexus）实际执行计划

当前总状态：`IN_PROGRESS`

本计划按依赖顺序推进。不得在 Linux 核心链路未稳定前接入真实 NAS，不得在 Windows 测试虚拟机通过前接入用户日常电脑。

---

## M0.1 仓库与环境基线

- 状态：`PASSED`
- 风险等级：中
- 人工门禁：无

### 目标

建立可恢复、可构建、可测试且不会破坏 1Panel 的开发基线。

### 实施步骤

1. 检查 OS、磁盘、CPU、内存、Docker、1Panel、网络、现有防火墙。
2. 记录现有 Docker 网络和容器，不修改未知资源。
3. 初始化 Git。
4. 创建目录结构。
5. 创建 Rust workspace、前端 workspace 和测试目录。
6. 配置 `.gitignore`、格式化、静态分析和基础 CI。
7. 验证 `1panel-network` 为外部网络。
8. 验证 `/dev/net/tun` 和 namespace 能力。
9. 建立仓库外 Secret 文件。
10. 生成环境基线报告。

### 验收标准

- 干净环境可执行最小构建；
- Git 无秘密；
- 1Panel 未受影响；
- 外部网络只引用、不重建；
- 恢复命令已记录。

### 验证命令

```bash
./scripts/validate-m01.sh
```

### 证据

- 环境基线：`/srv/xs-nexus-qa/baseline/20260729T094000Z`
- 验证日志：`/srv/xs-nexus/artifacts/qa/m0.1-20260729T095256Z/validate-m01.log`
- 环境报告：`docs/ENVIRONMENT_BASELINE.md`
- Git 检查点：`c519e34`

### 实际结果

- 完成系统、Docker、1Panel、网络、防火墙和工具链基线；
- 在隔离 namespace 中实际验证 TUN 和 nftables；
- 完成 Rust 与前端最小真实构建、单元测试和组件集成测试；
- 仓库秘密扫描和 npm 审计通过；
- Compose 仅引用外部 `1panel-network`，未修改现有 1Panel 资源；
- 发现既有 PostgreSQL、Redis 公网暴露，记录为 `KI-006` 和 `BLK-005`，不阻塞不受影响开发。

---

## M0.2 Clean-room、架构和威胁模型

- 状态：`PASSED`
- 风险等级：高

### 目标

在写核心协议前确定独立实现边界和安全假设。

### 产出

- `docs/ARCHITECTURE.md`
- `docs/REFERENCE_RESEARCH.md`
- `docs/CLEAN_ROOM_LOG.md`
- `docs/PROTOCOL_ORIGINALITY.md`
- `docs/THREAT_MODEL.md`
- `docs/SECURITY_ASSUMPTIONS.md`
- `docs/CRYPTOGRAPHIC_DESIGN.md`
- `docs/XSP1_PROTOCOL.md`
- `THIRD_PARTY.md`

### 验收标准

- 不含复制代码；
- 协议字段、握手状态机、密钥生命周期明确；
- 明确 Relay 不可见业务明文；
- 明确控制器失联行为；
- 明确未审计风险。

### 验证命令

```bash
./scripts/validate-m02.sh
```

### 证据

- 首次全量验证：`/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`
- XSP/1 编码向量：`tests/vectors/xsp1/data-header-v1.json`
- 协议一致性检查：`scripts/validate-m02.py`
- Git 检查点：`86f8cd9`

### 实际结果

- 完成 clean-room 边界、参考资料和第三方依赖清单；
- 完成控制面、数据面、部署边界、故障和恢复架构；
- 完成威胁模型、安全假设、密码学设计和 XSP/1 字节级规范；
- 固定 v1 密码套件、重放窗口、密钥 epoch、路径验证和失败关闭行为；
- Relay 仅转发带外层元数据的端到端密文，不持有数据面会话密钥；
- 编码向量、文档长度约束、全量回归和秘密扫描实际通过；
- 明确自研协议仍需独立安全审计，不作生产安全声明。

---

## M1.1 Controller 最小控制平面

- 状态：`PASSED`
- 前置：M0.1、M0.2

### 范围

- PostgreSQL 迁移；
- 网络、节点、Enrollment Token；
- IPAM；
- 注册 API；
- 配置版本；
- 基础 WebSocket；
- 审计日志；
- 健康检查。

### 验收

- 一次性 Token 哈希存储；
- 节点本地生成密钥；
- Controller 不保存节点私钥；
- 注册后稳定分配虚拟 IP；
- 重复注册、过期 Token、重复 Token 被拒绝；
- 配置带版本和签名。

### 实际结果

- 在独立项目 schema 中完成 PostgreSQL 迁移、约束、序列、活动 IP 唯一索引和追加式审计 trigger；
- 完成严格 JSON API、bootstrap 管理认证、网络创建、一次性 Enrollment Token 和节点本地公钥注册；
- Token hash、消费计数、IP lease、节点凭证、配置版本和审计在单事务内提交；
- 完成自动 IPAM、管理员指定 IP、网络级 advisory lock 和并发单次 Token 验证；
- 完成固定 200 字节 Ed25519 凭证、独立签名配置和 deterministic 测试向量；
- 完成 WebSocket challenge 认证、单调配置同步、消息上限和 ping/pong；
- 完成真实 PostgreSQL、HTTP router、loopback WebSocket、主进程健康检查和全量安全回归；
- Controller 只使用项目 schema，未修改 1Panel 容器、网络、端口、卷或宿主机防火墙。

---

## M1.2 Linux Agent 与 TUN

- 状态：`COMPLETE`
- 前置：M1.1

### 范围

- `/dev/net/tun`；
- Netlink；
- 节点注册；
- 本地安全存储；
- 控制连接；
- 虚拟 IP 和路由；
- `xs status`、`xs peers`、`xs diagnostics`。

### 验收

- Agent 作为 systemd 服务运行；
- 无需调用现成组网工具；
- 崩溃或卸载后恢复项目路由；
- 不修改默认路由；
- 权限最小化。

### 完成证据

- 非持久 Linux TUN FD、Netlink MTU/地址/路由和默认路由保护已在独立 network namespace 中验证；
- Agent enrollment/control、严格只读 Unix IPC、`xs status`、`xs peers`、`xs diagnostics` 和 SIGTERM 清理已验证；
- 最小权限 transient systemd 生命周期测试已验证，宿主机无 `xssvc0` 或项目路由泄漏；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`。

---

## M1.3 XSP/1 两节点加密链路

- 状态：`IN_PROGRESS`
- 前置：M1.2
- 风险等级：高

### 范围

- 独立握手；
- 身份验证；
- 会话密钥；
- AEAD；
- 序列号；
- 抗重放；
- Key Epoch；
- UDP 数据包；
- TUN 到 UDP 双向循环。

### 验收

在两个 Linux namespace 或独立测试节点间：

- 虚拟 IP 双向 Ping；
- TCP/UDP 业务可传输；
- 抓包中看不到原始虚拟 IP 业务包；
- 重放包被拒绝；
- 篡改包被拒绝；
- 伪造源虚拟 IP 被拒绝；
- Controller 重启时已有会话继续。

---

## M2.1 地址发现与候选管理

- 状态：`NOT_STARTED`
- 前置：M1.3

### 范围

- 本地候选；
- 公网映射发现；
- IPv6 候选；
- 候选签名和交换；
- 端点变化；
- 路径探测。

### 验收

- 不形成匿名反射服务；
- 探测包经认证；
- 多候选按优先级测试；
- 控制台和 CLI 显示路径原因。

---

## M2.2 UDP 打洞

- 状态：`NOT_STARTED`
- 前置：M2.1

### 验收

网络实验矩阵至少覆盖：

- 同 LAN；
- Full-cone 类行为；
- Restricted；
- Port-restricted；
- 两端 NAT；
- 对称 NAT；
- 公网 IP 变化；
- UDP 被封锁。

能直连时必须优先直连。

---

## M2.3 自研 Relay

- 状态：`NOT_STARTED`
- 前置：M2.2
- 风险等级：高

### 范围

- 认证会话；
- 限速；
- 转发队列；
- 心跳；
- 多 Relay；
- 故障切换；
- 直连回切；
- 监控。

### 验收

- Relay 无业务密钥；
- Relay 抓包不能恢复业务明文；
- 匿名转发被拒绝；
- Relay 故障后恢复；
- 直连恢复后可回切；
- 速率限制生效。

---

## M3.1 路由、ACL 与 IPAM 完整化

- 状态：`NOT_STARTED`
- 前置：M2.3

### 范围

- 默认拒绝 ACL；
- 节点/组/标签；
- TCP、UDP、ICMP、端口；
- 冲突检测；
- 地址冷却；
- 源身份绑定；
- 策略签名。

### 验收

- 发送端和接收端均执行；
- 旧配置不会被无签名或低版本覆盖；
- 控制器失联继续使用最近有效策略；
- 策略解释可用；
- 冲突不强行覆盖系统路由。

---

## M3.2 子网路由

- 状态：`NOT_STARTED`
- 前置：M3.1
- 人工门禁：真实 NAS 子网审批

### 范围

- 本地网段建议；
- 管理员审批；
- 纯路由和 NAT；
- 转发 ACL；
- 冲突和优先级；
- 网关离线撤销。

### 验收

先在 namespace 实验室完成，不接真实 NAS：

- 审批前不可达；
- 审批后按 ACL 可达；
- 未授权节点不可达；
- 网关离线后路由失效；
- 重叠网段被拒绝；
- 卸载后路由恢复。

---

## M4.1 Web 控制台功能

- 状态：`NOT_STARTED`
- 前置：M1.1，可与后端阶段并行

### 页面

- 登录；
- 首页；
- 节点；
- 网络；
- 地址池；
- Token；
- ACL；
- 路由审批；
- Relay；
- 拓扑；
- 用户权限；
- 更新；
- 审计；
- 告警；
- 系统设置；
- 备份恢复。

### 验收

- 不使用伪造状态；
- API 错误可见；
- 所有状态齐全；
- 权限控制；
- 审计；
- 响应式；
- 可访问性；
- `VISUAL_QA.md` 通过。

---

## M4.2 Playwright 端到端与视觉验收

- 状态：`NOT_STARTED`
- 前置：M4.1

### 验收

- 所有核心页面真实浏览；
- 固定测试数据；
- 关键流程可回放；
- 桌面和移动截图；
- 无溢出、遮挡和不可操作控件；
- 键盘导航；
- Console 无未解释错误；
- 网络请求无未解释 4xx/5xx。

---

## M5.1 Linux 安装、升级、回滚和卸载

- 状态：`NOT_STARTED`
- 前置：M3.2

### 验收

- x86_64、arm64 构建；
- 哈希和签名校验；
- systemd；
- 失败回滚；
- 升级保留身份；
- 卸载无项目路由残留；
- 诊断可用；
- 不泄露 Token。

---

## M5.2 Docker / 1Panel 部署

- 状态：`NOT_STARTED`
- 前置：M4.1、M5.1

### 验收

- 使用外部 `1panel-network`；
- 不创建数据库容器；
- Controller、Relay、Console 健康检查；
- 非 root；
- 数据持久化；
- 数据库迁移；
- 备份和恢复；
- 开发和 RC 隔离；
- 无公网数据库端口。

---

## M6.1 Windows 驱动设计和构建

- 状态：`NOT_STARTED`
- 前置：M1.3
- 人工门禁：Windows WDK 环境
- 风险等级：极高

### 验收

- 独立 `xsnet` 实现；
- 不使用 Wintun/TAP；
- 驱动仅负责 NIC 和安全 IPC；
- 所有不可信长度和状态验证；
- 测试签名构建；
- 安装和卸载脚本；
- 未实机测试时明确标记。

---

## M6.2 Windows 测试虚拟机验收

- 状态：`BLOCKED_EXTERNAL`
- 前置：M6.1
- 人工门禁：用户提供 Windows 11 测试 VM 和快照

### 验收

- 反复安装卸载；
- Windows/Linux 互通；
- 睡眠恢复；
- 网络切换；
- Agent 崩溃；
- 驱动异常输入；
- Driver Verifier；
- 无蓝屏；
- 卸载无残留；
- 测试签名状态明确。

---

## M7.1 Bug 集中修复

- 状态：`NOT_STARTED`
- 前置：核心功能完成

按 `BUG_LOOP.md` 执行，P0/P1 清零，连续三轮全量回归无新增失败。

---

## M7.2 安全复核

- 状态：`NOT_STARTED`
- 前置：M7.1

按 `SECURITY_REVIEW.md` 执行。

不得把内部复核替代第三方协议和驱动安全审计。

---

## M7.3 性能和稳定性

- 状态：`NOT_STARTED`
- 前置：M7.1

至少测试：

- 吞吐；
- 延迟；
- CPU；
- 内存；
- 长连接；
- 资源泄漏；
- 100/500/1000 节点控制面模拟；
- Relay 限速和拥塞；
- 24 小时稳定性；
- 日志增长；
- 数据库重连。

---

## M8.1 NAS 候选接入

- 状态：`BLOCKED_EXTERNAL`
- 前置：M5.1、M7.2、M7.3
- 人工门禁：用户在 NAS 本地执行安装

### 验收顺序

1. NAS 只作为普通节点；
2. 验证虚拟 IP；
3. 验证 Direct/Relay；
4. 验证升级和卸载；
5. 再提出 `192.168.0.0/24`；
6. 用户在控制台审批；
7. 验证子网 ACL；
8. 验证离线撤销。

---

## M8.2 本地 Windows 最终验收

- 状态：`BLOCKED_EXTERNAL`
- 前置：M6.2、M8.1
- 人工门禁：用户安装正式候选包

不得将日常电脑用于早期驱动试验。

---

## M9.1 Release Candidate

- 状态：`NOT_STARTED`
- 前置：所有可完成里程碑

### 验收

- 从干净 Git 提交构建；
- 产物签名和哈希；
- 数据迁移和恢复；
- 完整安装/升级/回滚/卸载；
- `RELEASE_CHECKLIST.md`；
- `FINAL_REPORT.md`；
- Git 干净；
- 明确未完成外部门禁和未审计风险。
