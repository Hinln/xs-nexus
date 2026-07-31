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

- 状态：`COMPLETE`
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

### 完成证据

- 协议单元、集成、RFC 原语和确定性向量测试全部通过；
- 两个隔离 Linux namespace 中完成双向 ICMP、TCP、UDP、加密抓包、自动 Key Epoch、Tag 篡改、重放、伪造源地址和 Controller 端点中断验证；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`。

---

## M2.1 地址发现与候选管理

- 状态：`DONE`
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

### 完成证据

- 固定长度认证 `XSD/1` 发现请求/响应、活动凭证检查、每来源限速和响应小于请求验证通过；
- Agent Netlink IPv4/IPv6 候选、同数据面 UDP socket 映射发现、节点签名广告、单调 generation、短期过期和动态配置应用验证通过；
- 两个隔离 namespace 中完成首选候选不可达后的握手回退，以及建立会话后更高优先级路径的 AEAD PathChallenge/PathResponse 晋升；
- CLI/IPC 显示候选、活动端点、候选种类、会话状态和真实路径原因；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.1-20260729T175243Z`。

---

## M2.2 UDP 打洞

- 状态：`DONE`
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

### 完成证据

- Idle Peer 无需等待 TUN 业务包即可双方主动发送认证 XSP/1 ClientHello；全局并发、每 Tick 启动量、每 Peer 候选数、重传次数和指数退避均有界；
- 已建立会话发送 AEAD Keepalive 维护 UDP 映射，生产默认 15 秒、特权网络测试 1 秒；
- 未知 UDP 来源只在 XSP/1 Header 绑定当前 Network、目标 Node、已知 Peer 和已建立 Session ID，且 AEAD/重放验证成功后才作为 NAT rebinding 晋升；
- `scripts/test-agent-proactive-punch.sh` 证明无 TUN 流量时自动建立认证 Direct 会话、周期保活和双向 ICMP；
- `scripts/test-agent-nat-matrix.sh` 在独立 namespace/nftables 中覆盖同 LAN、Full-cone 类、Restricted、Port-restricted、双端 NAT、公网 IP 重绑定、对称 NAT 无法直连、UDP 封锁后恢复；
- M2.1 的候选优先级、`handshake_fallback` 和 AEAD 路径晋升语义保持回归通过；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.2-20260730T092547Z`。

---

## M2.3 自研 Relay

- 状态：`COMPLETED`
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

- 状态：`PASSED`
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

### 验证命令

```bash
./scripts/validate-m31.sh
```

### 证据

- 全量验证：`/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`
- Git 检查点：以本节所在提交为准

### 实际结果

- 完成严格默认拒绝 ACL，支持节点、组、标签、Allow/Deny、优先级、TCP、UDP、ICMP、端口范围和解释结果；
- Agent 在加密前发送路径和解密后接收路径均执行同一签名策略，并将认证 Peer 身份绑定到虚拟源地址；
- Agent 只在配置签名、网络、节点、版本和 ACL 全部验证后原子替换状态，低版本、同版本异内容、无签名和无效策略均保留最近有效配置；
- Controller 使用 PostgreSQL 持久化组、成员和 ACL，策略原子替换并使用乐观版本防止并发覆盖；
- 活动地址唯一、节点吊销和地址冷却复用已验证；重叠地址池和 Agent 本机系统路由冲突失败关闭；
- 两个隔离 namespace 已验证允许 ICMP/TCP/UDP、发送端拒绝、接收端拒绝和明文负载不泄露；
- 全量回归、真实 PostgreSQL、ShellCheck、秘密扫描、npm audit、Docker、`1panel-network`、默认路由和 nftables 前后基线均通过。

---

## M3.2 子网路由

- 状态：`PASSED`
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

### 验证命令

```bash
./scripts/validate-m32.sh
```

### 证据

- 全量验证：`/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z`
- Git 检查点：以本节所在提交为准

### 实际结果

- Agent 通过 Netlink 发现本地直连私网，仅生成节点签名建议，未经管理员审批不发布路由；
- Controller 使用 PostgreSQL 持久化建议和审批状态，支持启用、暂停、撤销、乐观配置版本、纯路由/NAT、优先级和审计；
- 重叠、默认、保留、虚拟地址池冲突、无建议和过期建议均失败关闭，只将启用路由写入 Controller 签名配置；
- Agent 客户端路由和网关转发使用 Netlink 管理，NAT 使用项目自有 nftables 表，转发和系统状态均由 manifest 幂等回滚；
- XSP/1 路由包使用显式策略守卫接口，发送端加密前和接收端认证解密后均绑定 Peer、子网和 ACL，伪造虚拟源地址被拒绝；
- 三个隔离 namespace 已验证审批前不可达、纯路由/NAT ICMP/TCP、未授权源拒绝、网关离线撤销、暂停和崩溃恢复；
- 全量回归、真实 PostgreSQL、ShellCheck、秘密扫描、npm audit、Docker、`1panel-network`、默认路由和 nftables 前后基线均通过。

---

## M4.1 Web 控制台功能

- 状态：`COMPLETED`
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

### 完成记录

- Controller 使用 PostgreSQL 持久化控制台用户和会话，密码为 Argon2id PHC 哈希，会话与 CSRF 只保存域分离摘要；
- 管理员、操作员、审计员权限在服务端强制执行，登录限速、会话上限、注销撤销和管理审计已覆盖；
- 控制台接入真实网络、节点、Token、组、ACL、子网、Relay、拓扑、用户、审计和告警数据；未接入遥测明确显示“未采集”，不推断健康、路径、流量或延迟；
- 节点在线状态只来源于已认证的活跃控制连接，断开后从实时状态移除并记录时间；
- 响应式、空态、错误、无权限、长名称、节点详情、404 和键盘交互均已实现；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m4.2-20260730T223401Z`。

---

## M4.2 Playwright 端到端与视觉验收

- 状态：`COMPLETED`
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

### 完成记录

- Playwright 固定为 `1.62.1` 和 Chromium for Testing `151.0.7922.34`；
- 6 项主流程覆盖登录、16 个管理页面、空/错误/无权限、审计员只读、跳转链接、节点详情焦点恢复和 404；
- 6 个固定视口生成 135 张截图，另覆盖大量节点、长 IPv6、离线和部分服务不可用；
- 每轮监测 Console、Page Error 和所有 4xx/5xx，只有登录 401、权限 403 和预期服务 503 被逐项解释；
- 人工抽查登录、首页、节点、节点详情、拓扑、压力数据和错误页后修复了详情栏遮挡，再次完整回归通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m4.2-20260730T223401Z`，截图：`/srv/xs-nexus/artifacts/visual/m4.1`。

---

## M5.1 Linux 安装、升级、回滚和卸载

- 状态：`COMPLETED`
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

### 完成记录

- `x86_64-unknown-linux-gnu` 与 `aarch64-unknown-linux-gnu` 均完成真实 release 构建并以 ELF Machine 校验架构；
- 外部 Ed25519 清单在解析前验证，随后绑定产品、版本、平台、架构、目标、文件名、长度与 SHA-256；包内成员采用精确 allowlist、类型检查和逐文件哈希；
- 安装器固定首次可信公钥，使用版本目录和原子 `current`/`previous` 符号链接，拒绝外部降级，只允许回滚到仍通过签名与哈希验证的已安装版本；
- 服务启动失败自动恢复原单元、版本链接和活动状态；升级保留配置、节点身份和签名状态；
- 默认卸载先执行可信状态清理并保留身份，`--purge` 才删除项目配置与状态；清理失败会中止卸载并恢复先前活动服务；
- Enrollment Token 只接受受限文件路径，不进入进程命令行、包、状态输出或日志；
- 真实 systemd 崩溃清理、安装/重复安装/升级/失败回滚/显式回滚/卸载/重装/篡改拒绝均已自动化；
- 完成证据：`/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`；该证据使用临时测试签名密钥，不代表正式离线发布签名或真实 arm64/NAS 运行验收。

---

## M5.2 Docker / 1Panel 部署

- 状态：`COMPLETED`
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

### 完成记录

- Controller、Relay、Console 和一次性 migration/db-tools 均以多阶段镜像构建，运行时非 root、只读根文件系统、丢弃全部 capability 并启用 `no-new-privileges`；
- Compose 只引用既有外部 `1panel-network`，不定义数据库服务、不发布 MySQL/PostgreSQL/Redis 端口，也不创建项目网络；
- Controller 支持严格 `_FILE` Secret 和独立 `migrate` 命令，迁移在服务激活前执行；开发与 RC 使用独立项目名、schema、端口、Secret、备份和状态目录；
- 部署脚本在迁移前备份既有 schema，激活失败自动恢复上一个镜像集合；RC 要求干净 Git 和与 HEAD 一致的镜像 revision；
- PostgreSQL 18 运维镜像提供自定义格式逻辑备份、严格五字段完整性清单、校验、显式 schema 确认、恢复前安全备份和失败数据库回滚；
- 实际生命周期测试覆盖镜像构建、迁移、三服务健康、容器安全属性、数据持久化、备份篡改拒绝、恢复、迁移失败不替换服务和错误镜像自动回滚；
- 完成证据：`/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`；既有 1Panel PostgreSQL/Redis 公网暴露仍为 `KI-006`/`BLK-005`，项目未修改该外部资源。

---

## M6.1 Windows 驱动设计和构建

- 状态：`IN_PROGRESS`
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
