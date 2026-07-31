# BUG_LOOP.md — 缺陷闭环

---

## 1. 严重级别

### P0

- 远程服务器失联；
- 默认路由被破坏；
- 密钥、密码或 Token 泄露；
- 未授权访问；
- 远程代码执行；
- Windows 蓝屏或不可启动；
- 数据库不可恢复损坏；
- 更新供应链可被绕过；
- Relay 可被匿名滥用；
- 明文业务数据泄露。

P0 发现后立即停止相关发布路径，保留证据并修复。

### P1

- 核心组网不可用；
- 直连/Relay 无法恢复；
- ACL 可绕过；
- 路由残留；
- 安装或卸载破坏系统网络；
- 大规模资源泄漏；
- 控制台关键操作错误；
- 节点吊销不生效。

### P2

- 非核心功能错误；
- UI 明显问题；
- 特定边界兼容性问题；
- 可规避的性能退化。

### P3

- 文案；
- 轻微样式；
- 低影响可用性问题。

---

## 2. Bug 文件格式

每个缺陷写入：

```text
artifacts/bugs/XS-YYYY-NNNN.md
```

内容：

- 标题；
- 严重级别；
- 发现时间；
- 环境；
- 版本和提交；
- 前置条件；
- 复现步骤；
- 期望；
- 实际；
- 日志；
- 截图；
- 根因；
- 修复；
- 回归测试；
- 相关提交；
- 状态。

---

## 3. 修复循环

1. 保存证据；
2. 稳定复现；
3. 缩小范围；
4. 建立自动化回归测试；
5. 定位根因；
6. 最小修复；
7. 运行受影响测试；
8. 运行完整回归；
9. 检查副作用；
10. 更新缺陷；
11. 提交 Git。

---

## 4. 禁止做法

- 删除失败测试；
- 将测试标记为跳过；
- 把错误降级成日志；
- 捕获异常后返回成功；
- 增加无限重试；
- 仅延长超时；
- 使用静态假数据；
- 修改测试以适应错误实现；
- 没有根因就进行大范围重写；
- 一个提交夹带多个不相关修复。

---

## 5. 退出条件

Bug 集中修复阶段只有满足以下条件才通过：

- P0 = 0；
- P1 = 0；
- 所有 P2 有修复、接受或延期理由；
- 连续三轮完整回归无新增失败；
- 没有未解释崩溃；
- 没有未解释安全告警；
- 没有未解释浏览器 Console 错误；
- 没有未解释路由或设备残留；
- `KNOWN_ISSUES.md` 与实际一致。

---

## M3.2 已闭环缺陷

- XSP/1 普通虚拟地址绑定最初拒绝合法子网内层地址；通过显式 routed API、协议负向测试和三 namespace 全链路测试修复，未放宽普通会话。
- 候选路径主动回切期间，普通 AEAD 流量可能先于 PathResponse 改写路径原因；修复为待验证来源只能由匹配 PathResponse 晋升，候选路径回归和全量 NAT/Relay 矩阵均通过。
- 新增子网实现触发的 Clippy 长函数、所有权和参数告警均通过拆分职责消除，未使用 `allow`、跳过或降级告警。
- 三次失败的 M3.2 验证证据保留在 `artifacts/qa/m3.2-20260730T210602Z`、`artifacts/qa/m3.2-20260730T211008Z` 和 `artifacts/qa/m3.2-20260730T211424Z`；最终通过证据为 `artifacts/qa/m3.2-20260730T211914Z`。

---

## M4.1/M4.2 已闭环缺陷

- Playwright 浏览器首次缺少 `libnspr4.so` 等系统库；安装固定 Chromium 的官方依赖后重新执行，未改用系统随机浏览器或跳过测试。
- 未登录会话恢复的预期 401 被浏览器记录为 Console error；测试现明确断言只有 `/v1/auth/session` 401 可出现，登录后 Console、Page Error 和其他失败响应必须为零。
- 侧栏分组标题与页面标题同名导致定位歧义；测试改为限定 `#main-content h1`，未放宽可见性断言。
- Vitest 最初收集 Playwright 文件；Vite/Vitest 配置现只收集 `src/**/*.test.ts`，两套测试仍分别完整执行。
- 视觉压力夹具阶段仅改变 hash，React 保留上一空态数据；每个夹具阶段改用唯一查询参数强制新文档和 API 读取，避免伪通过或陈旧截图。
- 节点详情底部操作栏在小屏遮挡最后一行；取消遮挡式固定栏并在全部 6 个视口重新生成截图。
- 秘密扫描器把 TypeScript 的变量传递和 JSX 属性误报为硬编码秘密；扫描器现对源码仅接受字符串字面量为赋值命中，并新增 TypeScript“字面量必须命中、动态值不得误报”回归，真实环境参考值扫描保持不变。
- 首次失败全量证据保留在 `artifacts/qa/m4.2-20260730T222641Z`；最终通过证据为 `artifacts/qa/m4.2-20260730T223401Z`。

---

## M5.1 已闭环缺陷

- M0.2 规范检查仍引用控制台 API 扩展前的旧章节号；只更新精确章节标记到当前 `6/8/9`，未删除任何规范断言。
- Shell 秘密扫描器把包含动态 `$...` 路径的私钥文件变量误报为硬编码秘密；修复为动态 shell 值不作为字面量命中，并保留真实硬编码密码回归。
- 安装器解析在多个调用间共享全局状态，导致组合生命周期测试相互污染；解析状态改为每次命令独立初始化并新增重复调用覆盖。
- 控制同步最短周期为 5 秒而测试边界也为 5 秒，负载下首次更新约 5.4 秒到达；首个认证后同步固定在 1 秒启动，后续恢复配置周期，控制集成测试连续五轮通过且未延长超时。
- Relay 2 的认证流量可能先于 Relay 1 超时维护到达，活动端点已经切换但原因仍为 `relay_fallback`；从一个 Relay 收到另一 Relay 的认证流量现直接分类为 `relay_failover`，Direct 到 Relay 仍为 `relay_fallback`，Relay 全链路连续三轮通过。
- 最终 ShellCheck 发现 `SC2251`；修正条件表达式后保持 ShellCheck 全量通过，未使用禁用注释。
- 五次失败全量证据保留在 `artifacts/qa/m5.1-20260730T230810Z`、`artifacts/qa/m5.1-20260730T230947Z`、`artifacts/qa/m5.1-20260730T231558Z`、`artifacts/qa/m5.1-20260730T231749Z` 和 `artifacts/qa/m5.1-20260730T232350Z`；最终通过证据为 `artifacts/qa/m5.1-20260730T232953Z`。

---

## M5.2 已闭环缺陷

- db-tools 最初只设置 `PGDATABASE` 为连接 URI，libpq 将其当普通数据库名并回退本地 socket；改为每次 `psql`、`pg_dump`、`pg_restore` 显式传入受限进程内连接 URI，未记录值。
- 最初运维镜像使用 PostgreSQL 17 客户端连接 18.4 服务端，`pg_dump` 正确拒绝主版本不匹配；升级为 `postgres:18-alpine3.22` 客户端镜像并保留版本断言，没有忽略错误。
- `pg_restore --schema` 不恢复 schema 容器本身，首次恢复和安全回滚均失败；恢复路径现先显式创建确认后的目标 schema，再只恢复该 schema 对象。
- 部署测试中的无效数据库 URI 含模拟 userinfo，被秘密扫描器按凭据字面量拒绝；测试改用无 userinfo 的本机拒绝端点，迁移失败语义和断言保持不变。
- 失败全量证据保留在 `artifacts/qa/m5.2-20260731T001021Z`；最终通过证据为 `artifacts/qa/m5.2-20260731T001922Z`。

---

## M6.1 当前已闭环缺陷

- 兼容差异审计发现安装状态新增 ABI 和 driver version 必填字段后仍沿用 schema 1，会让未来旧结构与新结构无法可靠区分；由于安装脚本从未在 Windows 执行，直接把首个可执行状态提升为 schema 2，并要求读取、安装器和兼容门禁同时拒绝其他 schema，没有编写猜测式迁移。
- 提交前差异审计发现测试包构建脚本先运行 Inf2Cat、随后才给 UMDF DLL 做嵌入签名，后签名会改变 DLL 并使 catalog 中的文件哈希失效；顺序改为先签 `xsnet.dll`、再生成 catalog、最后签 `xsnet.cat`，并在 Python 门禁中强制三者源码顺序，未把无效 catalog 留给 VM 阶段发现。
- 新增测试包构建脚本首次 PowerShell 解析失败，因为双引号字符串中的 `$LASTEXITCODE:` 被解释为无效驱动器变量；改为 `${LASTEXITCODE}:` 后重新对全部新增脚本执行本机 PowerShell AST 解析，并加入独立源码门禁，未绕过错误或降低严格模式。
- 首次 VM 工作流证据命令由 Windows PowerShell stdin 注入 UTF-8 BOM，远端 Bash 把 `set` 读成未知命令，且直接哈希 nftables 会因运行时计数器增长产生假变化；保留失败采证目录 `artifacts/qa/m6.1-vm-workflow-20260731T021028Z`，改用 base64 无 BOM 传输并对 nftables 删除 handle/packet/byte 计数后做结构比较，签名顺序修复后的最终证据 `artifacts/qa/m6.1-vm-workflow-20260731T021432Z` 通过。项目测试本身未跳过或放宽。
- Windows Agent 客户端完整回归首次直接调用 `cargo test -p xs-agent`，绕过项目为真实 PostgreSQL 集成测试注入仓库外连接的 `scripts/test-agent-control.sh`，因此按设计因缺少 `XS_TEST_DATABASE_URL` 失败；改用 `Makefile` 正式入口 `make test-unit` 与 `make test-agent-control` 后真实控制面测试通过，未跳过或修改测试。
- 新增生命周期测试首轮遗漏了仓库测试翻译单元统一使用的 `NDEBUG` 撤销，导致 Clang Release 假绿而 ASan/UBSan 实际执行断言并失败；补齐同一断言门禁并把模式加入源码验证器。随后穷举交错发现 queue cancel 胜出分支断链后仍保留活动请求；模型现将该分支单次取消，另保留 request completion 胜出分支验证不会重复取消，没有删除、跳过或弱化断言。
- ABI 测试首次以 Release 构建时 `NDEBUG` 移除了标准 `assert`，使断言变量变成未使用并由 `-Werror` 阻止构建；测试翻译单元现显式重新启用断言，随后同一 Release 配置和 ASan/UBSan 配置均必须执行测试，不降级为 Debug-only。
- 首轮平台判断只核对了“KMDF NetAdapterCx 从 Windows 10 2004 可用”和“UMDF 从 Windows 11 24H2 可用”，遗漏官方版本表中 Windows 10 NetAdapterCx 2.0 仅支持 MBBCx 的限制；重新核对后用 ADR-044 取代 ADR-043，首版改为 Windows 11 24H2 UMDF 2.33 + NetAdapterCx 2.5，并把 Windows 10 差距记录为 `KI-016`，未继续实现或宣称不受支持组合。
- 有界队列首轮严格构建发现未使用的字节读取辅助函数；删除死代码后保留 `-Werror`。随后“小输出不消费”用例错误地提供了足够容纳 32 字节包与 16 字节批次开销的缓冲区；按真实 48 字节边界修正为 47 字节，未修改实现或放宽断言，最终 Clang 与 ASan/UBSan 三组测试全部通过。
- 扩展 WDK 项目验证器时，首次把 `ItemDefinitionGroup` 中没有 `Include` 属性的 `ClCompile` 选项节点误当成文件项并触发 `KeyError`；改为只收集带 `Include` 的项目文件节点，仍要求 ABI、会话和数据平面源文件全部显式进入工程。
- direct-I/O 锁复核时发现一次编辑把 RX 协商深度检查误插入 TX 出队成功分支，且 TX 异常回滚快照位于 sequence 推进之后；在进入验证前将深度检查移回 RX 预检、快照移到状态机调用前，不以“后续分支理论不失败”作为正确性依据。
- 新增 TX 无副作用批次测量测试时，首次把 47 字节不足断言插入了 128 字节成功缓冲区用例，测试正确失败；将断言移动到真实小缓冲场景后重跑，未修改测量实现或容量规则。
- 官方 DDI 复核确认运行时 `NetAdapterSetLinkLayerMtuSize` 会重建 TX/RX queues；Attach 最初在 `session_lock` 内调用会让 queue stop/create 回调重入同一 wait lock。现只在锁内提交会话与私有 MTU，解锁后调用 NetAdapterCx MTU 更新，SetLink 仍需等待重建后的双队列 started。

---

## 源码供应链验收已闭环缺陷

- 首次 CycloneDX/SPDX 实际生成保留 Cargo metadata 中 `Apache-2.0/MIT` 与 `MIT/Apache-2.0` 历史写法；策略解析虽能判断，但斜杠不是规范 SPDX 运算符。生成器现只把这两种明确写法规范化为 `OR`，测试要求所有组件表达式不含 `/`，未丢弃原许可证选择。
- 独立实现扫描器首轮把 SBOM 生成器和测试自身的禁用词策略表判为运行依赖并失败；修复只排除三份策略工具本身，Agent、Controller、Relay、协议、驱动、安装器、部署及其他脚本仍全量扫描，三个真实负向引用继续按路径、词和精确数量固定。
- 早期证据 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731T024136Z` 保留，仅证明首版结构和主机基线；由于包含非规范斜杠许可证表达式，不作为验收证据。最终证据为 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。
