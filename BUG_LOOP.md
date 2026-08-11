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

---

## M7.1 已闭环缺陷

- 三轮聚合回归捕获候选路径测试竞态：测试原先只等待 A 端完成对 B 新路径的 PathResponse，便立即发送双向 ICMP；协议按方向独立维护探测。首次失败证据为 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T192431Z`，双向等待后的失败证据为 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T194301Z`，缺陷记录为 `artifacts/bugs/XS-2026-0003.md`。测试已分别要求 A、B 都报告 `authenticated_path_probe`，并修正诊断以保留首个失败命令和受限临时证据；完整 M5.2 随后通过，但缺陷保持调查中，三轮计数仍为零。
- 提交 `52867cd` 后重新从零执行三轮完整聚合，三轮均通过 Linux 网络/安装/部署、M6.1 门禁、E2E/视觉和供应链处置，证据 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`。当前已记录缺陷 P0=0、P1=0，三个 P2 均关闭；`XS-2026-0003` 据此闭环。

---

## M6.1 Win32 transport 已闭环缺陷

- 首次完整 Agent MSVC check 成功构建 Windows core/std 后在 `ring` 找不到 `lib.exe`，指定 `clang-cl` 后进一步证明当时缺少 Windows SDK `assert.h`；未把该失败写成 transport 编译结果，也未安装或伪造 SDK。transport 被隔离为不依赖 TLS/C 头的最小 crate；后续 Windows 11 VM/WDK 驱动环境门禁已解除，但完整 Agent 仍未链接。
- 最小 crate 初次 `-Z build-std=std` 因发行版 rust-src 的 Windows std 内部 `windows_targets` 不完整失败；将 crate 收紧为 `no_std + alloc`，只构建实际需要的 core/alloc/panic_abort，随后 MSVC target check 通过。
- Linux Clippy 首次拒绝只在 Windows 使用的私有访问器 dead code；用 `cfg(windows)` 限定真实使用点、接口解析保留 `cfg(test)`，没有添加 allow。交叉 Clippy 随后拒绝两个隐式 borrow-to-pointer，改为 Rust 2024 `&raw mut` 后 warnings-as-errors 通过。
- 一次同步命令把 Agent 源文件误放到 `apps/agent/windows_xsnet.rs`；删除前逐字节 SHA-256 核对它等于本地待同步文件，再写入正确 `src/` 路径。错误文件未进入 Git，Agent 新增非 Windows 拒绝测试随后从 31 增至 32 个并实际执行。
- Windows 本地 IPC 首版把 OS pipe instance 上限和活动 handler 上限都设为 16；第 16 个活动连接会在创建下一 listener 时超过上限并终止服务。现固定 16 个活动许可加 1 个 listener，许可耗尽时 `connect` 分支不接收，源码门禁同时锁定 17 实例和 guard，未以提高无界上限掩盖问题。
- 安装 rustup Windows 标准库后，首次全量验证的 transport check 使用系统 Cargo，而 `cargo clippy` 被同名 rustup proxy 接管，两个步骤使用不同 sysroot 并在离线标准库依赖解析失败；失败证据保留在 `artifacts/qa/m6.1-agent-session-20260731T035905Z`。脚本现从同一工具目录固定 cargo、rustc 与 cargo-clippy，旧门禁和新 IPC 交叉门禁分别真实通过，最终全量证据为 `artifacts/qa/m6.1-agent-session-20260731T040415Z`。
- Windows 私有存储初版读路径验证了文件 exact DACL 与整条路径的 reparse 属性，但没有验证直接父目录 DACL；宽松父目录仍可能允许在检查与打开间替换文件。读路径现要求父目录为真实目录且 exact protected DACL 后才检查文件，源码门禁固定该调用，未把仅文件 ACL 当作完整替换防护。
- Windows Service 首版在注册 control handler 后无锁上报 START_PENDING/RUNNING；STOP 若在检查与 RUNNING 上报之间到达，STOP_PENDING 可能被过期 RUNNING 覆盖。现用独立状态互斥串行化启动、停止和最终 STOPPED，上报活动状态前在同一锁内复核原子 STOP，源码门禁固定状态锁，未通过延迟或轮询掩盖竞争。
- 新增 workspace crate 后首次服务专用测试保留 `--locked` 并正确因 `Cargo.lock` 尚无新 package 失败；使用离线 Cargo 正常解析后回同步锁文件，再原样执行 `--locked` 测试通过，没有移除可复现性门禁。
- workspace Clippy 首轮拒绝非 Windows Service stub 缺少 `# Errors`、使用下划线绑定和空 async，第二轮又拒绝只供测试使用的解析 wrapper dead code；分别补齐契约文档、真实 pending await，并把 helper 限定为 `cfg(test)`，未添加 lint allow，随后 workspace warnings-as-errors 与全量单测通过。
- LUID 设计审计发现路由准备层只有显式参数而没有可信生产者；没有采用接口别名、display name、全局适配器枚举或篡改 ABI v1 Hello 响应。新增同一独占 xsnet handle 的 identity schema v1 查询，驱动从自身 `NETADAPTER` 读取 LUID；C/Rust 负向测试覆盖错误版本、长度、reserved 和零 LUID，runtime 仍保持禁用。
- identity 扩展首轮 M6.1 聚合验证被 Clippy `doc_markdown` 拒绝，因为新增公开文档中的 `NetAdapterCx` 未使用代码标记；失败证据保留在 `artifacts/qa/m6.1-agent-session-20260731T173555Z`。修正文档标记后原样重跑全量门禁，没有添加 lint allow 或降低告警等级。
- identity query 首版虽提供同句柄 LUID，但网络准备公开函数仍允许任意调用方直接传入裸 `u64`，可信链条可被未来编排绕过。现把裸 LUID recover/prepare 降为私有，只公开 session-bound wrapper，并用源码门禁拒绝重新公开；runtime 保持未接入。
- 镜像 SBOM 首次真实解析 PostgreSQL Alpine rootfs 时，严格许可证门禁拒绝 generated `.postgresql-rundeps`，因为该 dot-prefixed virtual metapackage 没有 `L:` 字段。修复为只允许这类明确虚拟包缺失许可证并仍保留在 SBOM；普通 apk 包缺失许可证继续失败。四镜像试运行随后识别 344 个包，未为虚拟包伪造许可证。
- 当前提交四镜像首次正式供应链验证完成构建后，在把 `/tmp` 中的完整输出用 `os.replace` 发布到仓库证据目录时收到 `EXDEV`，失败证据保留在 `artifacts/qa/image-supply-chain-20260731T175829Z`。生成器现先复制到输出父目录中的私有 staging，再执行同文件系统原子替换；已有目标仍拒绝覆盖，失败 staging 会清理，不降低完整发布语义。
## 运行时镜像漏洞收敛已闭环缺陷

- 初始扫描发现 Controller/Relay 为容器健康检查安装完整 `curl` 依赖链，Console/db-tools 还保留仓库中已有修复版本的 Alpine 包。没有添加 ignore 或降低扫描级别。
- 修复为二进制固定 loopback readiness 探测，补齐命令解析、连接拒绝、超时、非 200、畸形和超大响应回归；同时在两个 Alpine runtime stage 应用安全升级。
- Docker 生命周期测试真实发现并验证新镜像中的健康命令；新扫描把 Critical 从 44 降为 22、High 从 115 降为 45，fixable Critical/High 从 4/42 降为 0/0。
- 残余无当前修复版本的发现转入 `KI-021`，未作为已修复或已接受关闭。
- 第二轮继续移除未使用包：Controller/Relay 改为固定 digest distroless，Console 删除无反向依赖的 curl 链。供应链生成器首次正确拒绝未知 distroless 包数据库，随后新增 `status.d` 精确解析和负向门禁；未绕过 SBOM。
- 最终扫描 Critical/High 为 2/6；相对最初 44/115 显著下降，剩余 glibc/TIFF 项继续由 `KI-021` 跟踪。
## 最新聚合回归已闭环缺陷

- M6.1 最新回归在高并发 workspace 测试中发现健康检查测试服务器假设单次 TCP `read` 返回完整请求；改为 1 秒超时、1 KiB 上限并读取到完整头终止符，产品探测器断言未改变。失败/通过证据分别为 `m6.1-agent-session-20260731T183943Z` 与 `m6.1-agent-session-20260731T184122Z`。
- M2.3 聚合验证在功能全部通过后因 Compose 强制变量新增而失败；验证器现在显式提供当前 HEAD、镜像、非秘密路径、schema、endpoint 和测试 Relay ID，只执行 config，不创建容器或网络。失败/通过证据分别为 `m2.3-20260731T185001Z` 与 `m2.3-20260731T185454Z`。

## 稳定性重启证据门禁已闭环缺陷

- `XS-2026-0004`：初版长测强化错误要求手动 `docker restart` 后 `.RestartCount` 增加；正在运行的 24 小时证据显示三个服务 PID 均按预期变化而计数始终为零，复核 Docker 语义后确认该字段只用于 restart policy 自动重启计数。脚本现要求重启输出精确匹配目标容器 ID、每个服务恰好两个 PID，并要求计数始终等于基线，以便额外崩溃重启失败关闭；弃用的 `--time` 同时改为 `--timeout`。

## 签名更新通道闭环缺陷

- `XS-2026-0005`：首版灰度 UI 可以创建 testing/development 策略，但节点通道只来自 Agent 本地静态配置，导致策略无法由控制面安全激活；长连接还缓存认证时通道，数据库手工修改后会错误拒绝新报告。
- 修复：节点通道进入 Controller 签名配置，管理 API 使用网络配置版本发布新配置和审计；Agent 从自身签名节点条目取值并立即重报；Controller 每次对照数据库当前分配，不再信任连接建立时快照。Console 对所有节点提供显式确认的通道切换，审计员只读。
- 回归：真实 PostgreSQL 覆盖成功、旧版本 409、签名 payload 与审计脱敏；Playwright 断言精确请求体和确认；严格 Clippy、Agent 控制面及 6 视口视觉测试通过。

## 认证遥测闭环缺陷

- 双 Relay 全链路首次加入 Reporter 后在 Direct 恢复末端出现测试假失败：活动端点已是经过认证的本地候选，但周期 PathResponse 建立的 `authenticated_path_probe` 原因被随后合法 AEAD 流量更新为 `authenticated_peer_traffic`。测试现只接受这两个认证直连状态，不接受普通候选、未认证流量或 Relay 状态；原样重跑通过 fallback、密文不可见、主备切换和双向 Direct 恢复。
- 首次 Agent 控制测试在 `/tmp` 的 tmpfs 内生成独立 Cargo target 并耗尽空间；确认精确生成目录后删除，只把隔离仓库的 `target` 链接到 `/root/.cache` 的共享构建缓存。没有删除项目证据、正式仓库、容器、网络或 1Panel 数据。

## 备份加密依赖漏洞闭环缺陷

- 备份功能加入 age 后，首次干净镜像扫描发现 Alpine 3.22 的 age 1.2.1 内嵌过期 Go 依赖，db-tools 单镜像出现 18 个 Critical、32 个 High 可修复项。门禁失败，未添加 ignore、VEX 或自动豁免。
- 改用 Alpine edge 的 age 1.3.1-r6 清除了大部分发现，但精确扫描仍检出 `golang.org/x/crypto v0.45.0` 的 `GHSA-w879-237q-wc7r`，修复版本为 0.52.0；门禁再次按预期失败。
- 最终改为从固定 age `v1.3.1` 提交构建，并强制验证 `x/crypto v0.52.0`；两个 CLI 均报告 `v1.3.1-xs1`。完整备份生命周期、SBOM/许可证闭包和漏洞 disposition 原样重跑通过，可修复项为 0。
- 一次供应链验证在证据生成前因 `/tmp` 空间不足失败，证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T085349Z` 未冒充产品失败或通过；确认并删除仅属于本任务的 1.7 GiB 临时克隆构建目录后，后续扫描固定使用 `/var/tmp`，正式仓库、QA 证据和 1Panel 资源未删除。

## CLI 完整性闭环缺陷

- `XS-2026-0006`：任务书要求九个本地 CLI 命令，但早期 M1.2 执行计划把范围缩成 `status`、`peers`、`diagnostics`，后续回归沿用该缩减清单，遗漏 `ping`、`path`、`routes`、`netcheck`、`reconnect` 和 `version` 子命令。
- 修复：Core 增加严格有界 IPC 请求/响应；`ping` 复用认证 XSP/1 PathChallenge/PathResponse 而非 ICMP、原始 socket 或明文；`path`、`routes`、`netcheck` 只返回运行时已知事实；`reconnect` 以 1 秒冷却、确认响应和有界 channel 立即重建 Controller WebSocket；`version` 与既有 `--version` 一致。失败 ping、非健康 netcheck 和被拒 reconnect 返回非零。
- 回归：CLI/Core/Agent 单测、严格 Clippy、真实 PostgreSQL Agent 控制面及双 Agent namespace 全链路通过；候选路径测试实际运行九个命令并验证重连、认证路径晋升和宿主/1Panel 不变量。正式证据为 `/srv/xs-nexus/artifacts/qa/m1.2-cli-completion-20260802T100106Z`，实现提交为远端 `e272114`。

- `XS-2026-0007`：总审计发现 `xs-cli` 无条件导入 Unix socket，Windows target 无法编译；同时共享服务器用 `read_to_end` 等待客户端写半边 EOF，而 Windows duplex Named Pipe 没有可依赖的 Unix 半关闭语义，即使补一个简单 client 也会形成双方等待。
- 修复：请求与响应统一改为 4 字节大端长度加严格 JSON，一连接一请求；server/client 分别拒绝零、超限、截断和错误类型。`xs-cli` 在 Windows 使用固定 Named Pipe client 且无 unsafe，Unix 路径保持私有 socket。Windows IPC 聚合门禁现在同时 MSVC check/Clippy server crate 和 CLI，不再只验证服务器半边。
- 回归：Agent IPC 增加零/超限帧负向测试；CLI 三组协议/退出码测试原样通过；Linux Agent/CLI Clippy、Windows server/client MSVC check 与交叉 Clippy均通过。当时没有 Windows 实机结果；后续 `xsnet` 驱动 VM 门禁不包含 Rust Named Pipe/DACL/SCM，它们仍保持未验证，不把交叉编译描述成实机结果。

## 公网测试部署绑定闭环缺陷

- `XS-2026-0008`：域名测试部署预检发现 HTTP、Discovery UDP 和 Relay UDP 共用 `XS_BIND_ADDRESS`。保持回环会阻止外部节点发现/中继，改为 `0.0.0.0` 又会把 Controller 与 Console 的明文 HTTP 端口一起暴露公网。
- 修复：HTTP 继续由 `XS_BIND_ADDRESS` 控制且部署预检强制为 `127.0.0.1`；新增独立、显式的 `XS_UDP_BIND_ADDRESS`，只允许回环或全 IPv4 监听，默认回环。Compose 仅把 Discovery/Relay 两个 UDP 发布切换到新变量。
- 回归：Compose 预检、完整 M5.2 部署生命周期、回环 HTTP 与公网 UDP 实际监听、HTTPS/WSS 反向代理及外部节点路径测试必须同时通过；不得把测试开放描述为生产防火墙验收。

- `XS-2026-0009`：首次 Edge 预检发现官方 Caddy 镜像的 `/usr/bin/caddy` 带 `cap_net_bind_service=ep` 文件能力；在项目要求的 `cap_drop: ALL` 和 `no-new-privileges` 下，tini 无法 exec 该二进制。直接增加 capability 会扩大运行时权限并违背 Edge 零 capability 目标。
- 修复：新增项目 Edge Dockerfile，基础镜像固定到官方 Caddy 2.10.2 Alpine 的 amd64 manifest digest；构建阶段升级安全补丁并显式移除不需要的文件能力，最终固定 UID 65532 和 revision 标签。Edge 在容器内只监听 8080/8443，宿主端口映射不需要进程获得低端口能力。
- 回归：相同 `cap_drop: ALL`、`no-new-privileges`、只读根和非 root UID 下，`caddy validate` 已真实执行通过；完整 HTTPS 容器启动和证书签发继续由本次域名集成证据闭环。

- `XS-2026-0010`：第一次正式域名激活在迁移完成后才发现默认 Controller 宿主端口 `18080` 已被无关容器绑定到 `0.0.0.0`。激活按设计清理了新容器，但可避免的端口冲突不应发生在迁移之后。
- 修复：应用栈预检现在校验四个 TCP/UDP 端口的格式、范围和同协议唯一性，并用 `ss` 检查实际监听；只有当前同一 Compose 项目已持有的端口可用于幂等重部署。Edge 对 80/443 执行相同检查且只允许当前 Edge 容器占用。
- 回归：M5.2 新增真实回环监听占用负向测试，要求在镜像构建、备份或迁移之前失败；已运行项目的原端口仍必须通过幂等预检。

- `XS-2026-0011`：真实 Linux 首次安装已从 Controller 成功取得签名状态，但 Agent 删除一次性令牌时返回 `agent_state_invalid`，安装器随后按设计回滚。根因是安装器把令牌复制到 root 拥有的 `/run` 根目录；服务用户可读文件但无权从父目录删除。
- 修复：安装器改为在 Agent 自有的私有状态目录中创建随机临时令牌文件，并在退出、信号、失败和成功路径统一清理；Agent 仍负责在成功持久化状态后先删除令牌。
- 回归：Linux 安装器测试的假 Agent 现在要求令牌位于随机私有状态路径、必须能自行删除，并在首次注册后留下节点状态；旧的 `/run/xs-nexus-enrollment-token` 路径会直接导致测试失败。

## Windows xsnet Verifier 闭环缺陷

- `XS-2026-0012`：首个 Windows 测试签名包可以完成普通 SYSTEM Tx/Rx 与 PnP restart，但启用 UMDF/Application Verifier 后 PnP restart 超时。WDF 记录 `WUDFVerifierFailure` 414，WER 将故障归类为 `LKD_0x15E_VRF_Mini_Nbl_Leak_IMAGE_netcxrd.sys`。
- 根因：TX queue cancel 回调错误改写 packet/fragment 的 producer ownership indices，使 NetAdapterCx 在取消完成路径无法正确结算 NBL。该问题不能由平台无关生命周期模型或普通 PnP restart 发现。
- 修复：TX cancel 只标记待取消 packet 并把 packet completion `BeginIndex` 推进到 `EndIndex`；不再改写 packet `NextIndex` 或 fragment `BeginIndex`/`NextIndex`。源码门禁现在单独抽取 cancel 函数并拒绝这些越权写入。
- 回归：修复包在 Windows 11 24H2 VM 通过普通 restart、standard Driver Verifier oneboot、UMDF/Application Verifier 三轮 restart + SYSTEM smoke，settling 后新增相关 WDF/LiveKernel dump、WER 和 error event 均为零，随后 clean uninstall 零设备、零 driver-store 包。可复核摘要见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`。

## Windows signed Wintun release packaging closed defects (2026-08-04)

- 首次发布构建把 Cargo 根目录和 CLI package 名称假定错误；构建脚本现显式传入工作区 `--manifest-path`，并构建 `xs-agent` 与 `xs-cli` 的正确 package 名称。
- Wintun Authenticode Subject 包含完整 DN，严格完整字符串相等会误拒绝有效发行方；现精确匹配 Subject 中独立的 `CN=WireGuard LLC` 字段，不接受任意包含关系。
- PowerShell 模板标记计数最初使用 `.Split()`，会把 marker 字符逐个作为分隔集合而产生错误计数；现使用精确正则 matches，且仅允许一个 manifest 摘要占位符。
- 初版安装器在解压后只校验预期文件，尚未拒绝额外内容或重解析点；现强制精确文件/目录集合、拒绝每一个 reparse point，并逐项复验 payload 清单哈希。上述修复后最终 r3 package 校验通过，未放宽任何完整性断言。

## 生产候选继续开发闭环缺陷（2026-08-08）

- `XS-2026-0013`：生产镜像使用历史提交 tag，但 OCI revision 和 active deployment 已指向新提交。修复为 RC 预检强制五个镜像 tag 精确等于发布 Git revision；正向和 stale-tag 负向测试通过，不再允许标签漂移。
- `XS-2026-0014`：生产服务器 Chrony 被停用，系统时钟比 RTC、外部 HTTPS Date 和 NTP 慢整整一天，影响证书、Token、配置 TTL 和审计时间。恢复既有 Chrony 服务后自动前跳 86400.300154 秒并同步；容器身份、网络、默认路由、nftables 和下载接口均保持。公网 525 仍存在，证明其还需要源站 TLS/反代门禁处理。
- `XS-2026-0015`：2026-08-08 重新构建时 npm audit 新增 `nanoid <3.3.17` 高危无限循环/DoS 公告。未忽略或豁免；锁文件升级到 `3.3.18`，npm high/critical 清零，Console build 和 4 项测试通过。

## 最终 M5.2 与生产部署缺陷闭环（2026-08-08）

- `XS-2026-0016`：握手 fallback 选择可达候选后，会话尚未 Established 时维护循环错误创建更高优先级 PathProbe 并终止 Agent。修复为仅在 Established 状态创建探测；单元回归和真实 fallback namespace 通过。
- `XS-2026-0017`：SIGTERM 时 IPC 正常退出可先关闭 runtime command channel，`tokio::select!` 将其误判为运行失败。关闭分支现在先检查 shutdown 状态；systemd 与完整子网生命周期通过。
- `XS-2026-0018`：候选路径并发中，一侧可能先通过 AEAD peer traffic 晋升，随后周期探测错误覆盖或无法稳定报告路径原因。PendingPathProbe 记录是否为晋升探测；匹配响应只在晋升探测时写 `authenticated_path_probe`。测试接受协议已有的两个认证原因，但强制至少一侧完成 PathResponse，并连续三轮通过。
- `XS-2026-0019`：NAT nft 计数使用 `nft | awk exit`，在输出超过管道缓冲时生产者收到 SIGPIPE，`pipefail` 返回 141。AWK 现在读取完整输出且只打印首个匹配；六场景矩阵通过。
- `XS-2026-0020`：one-click cleanup 仅由 trap 间接调用，ShellCheck 报 SC2317。成功路径现在显式清理、清空状态并撤销 trap；ShellCheck 和 one-click 安全测试通过，没有添加 ignore。
- `XS-2026-0021`：生产环境本来存在 `xs-nexus-rc`，旧验证却要求 dev/RC Compose 项目均为空；首次基线修复又把会变化的 `Up N hours` 写入快照。最终改为前后稳定 ID/name/image/state 比较，并保留全 Docker 运行集合检查。
- `XS-2026-0022`：仓库外 QA 镜像只有 AArch64 GCC，没有目标 libc 头文件，`ring` 交叉 C 编译失败。镜像补充 `libc6-dev-arm64-cross` 后实际生成 AArch64 Agent/CLI 签名包并通过 ELF 校验；产品运行镜像不受影响。
- `XS-2026-0023`：部署测试把随机 32 字节当作 Ed25519 公钥，部分随机值不是有效验证键。测试改为从确定性签名私钥推导真实公钥；部署生命周期通过，未放宽 Controller 公钥校验。
- 最终结果：精确提交 `ff9551d322067c934d2ac7d55a62af8896660bb3` 的 M5.2 全量为 `validation_status=0`，随后同 revision 生产部署与宿主基线复核通过。外部门禁继续记录在 `BLOCKERS.md`，没有把失败测试删除、跳过、ignore 或改为 Mock。

## Gate 16 生产部署验证器缺陷闭环（2026-08-09）

- `XS-2026-0024`：生产 PostgreSQL `/tmp` 是容器 tmpfs，Docker archive API 的 `docker cp` 无法以该路径为目标；改为以目标用户通过 `docker exec -i` 流式写入。首次尝试在数据库变更前失败，20 分钟回滚仍按计划执行并恢复旧镜像。
- `XS-2026-0025`：数据库容器删除了 `CAP_CHOWN`，即使容器内 UID 0 也不能改变 tmpfs 文件 owner；改为从目录创建、流式写入、权限设置到清理全部使用 `postgres` 用户。第二次尝试同样在角色变更前失败并自动回滚。
- `XS-2026-0026`：HTTP 版本端点输出 JSON，但二进制 `--version` 按既有契约输出文本；生产验证器错误把四者统一按 JSON 解析。修正为分别验证 JSON 字段与文本 `commit=/protocol=`，第三次已健康部署仍因验证器失败而自动回滚，没有人工取消保护。
- `XS-2026-0027`：Compose 重建会更新 Docker nftables handle、规则顺序和项目容器 IP，字节级比较错误拒绝预期规则刷新。修正为保留原始快照，删除 counter/handle，仅允许由前后 Docker inspect 认证的 XS Nexus 地址和实际发布端口对应规则变化，并要求所有非项目规则精确相等。旧/新真实快照预演通过后才重试。
- 最终第五次尝试通过即时验证、全新 SSH 会话、回滚取消后只读复核和 81 文件 SHA-256；四次失败、四次自动回滚及根因均保留在 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`，未删除或弱化任何门禁。

## Gate 23 缺陷与验证器闭环（2026-08-09）

- `XS-2026-0028`：Axum JSON extractor 将 serde 字段、类型、行列和语法错误直接写入未认证 400 响应。修复为固定通用 envelope，并以畸形 JSON、未知字段、错误类型和超限 body 证明不再泄露 parser detail；保留业务错误类型，不把所有失败吞成 400。
- `XS-2026-0029`：生产 PostgreSQL 容器没有 Docker log rotation 上限。首次受保护 rollout 的验证器失败后按计划回滚并保留证据；修正验证链后以加密备份、20 分钟自动回滚、独立 SSH 和宿主/1Panel 不变量部署 `10m`/`5`，未重建数据库卷或修改 `1panel-network`。
- `XS-2026-0030`：SQLx `0.8.6` 把未启用路径的 `rsa` 留在 lock，脚本通过 ignore 处理 `RUSTSEC-2023-0071`；`cargo-deny` 又缺少许可证 allowlist。升级 SQLx `0.9.0`/Rust `1.94`、显式标注动态 SQL 安全边界、删除 `rsa` 和 ignore，并让完整 cargo-deny 四类检查通过，没有用 feature 裁剪、skip 或弱化门禁。
- 全量验证的前三个 harness run 分别因 CI JSON BOM、无效 toolchain 命令和磁盘压力提前终止；第四个 run 正确暴露测试 harness、RSA 和 license policy 缺陷。修复后一个 run 的所有产品检查通过，但生产磁盘健康守卫在根分区 `91%` 时按设计拒绝最终 baseline。
- 仅删除两个旧任务镜像和三个可证明属于 XS Nexus 的精确 BuildKit cache record；首轮镜像清理后仍不足 12 GiB 的 run 继续保持失败。禁止且未运行 global Docker prune。最终验证从约 12.96 GB headroom 开始并全部通过。
- 九个失败/non-authoritative root（API 三个、全量/空间六个）均保留原始文件并新增不可覆盖 disposition/manifest；成功根为 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z`，封存复核根为 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z`。
- 结果边界：四个可自行修复 finding 关闭，但生产仍运行 `3d93656`，全局 Critical/High 与外部门禁未关闭；Gate 23 仍为 `FAIL`，没有把内部回归写成生产 GO。

## Gate 24 依赖与 CI 供应链闭环（2026-08-10）

- `XS-2026-0031`：精确提交 `45dbc196` 的四项 GitHub Actions job 全部通过，但运行记录明确警告 checkout、setup-node 和 upload-artifact 仍以 Node 20 action metadata 运行并被平台强制到 Node 24。未把绿色退出码当作无问题。
- 修复：从官方仓库解析最新 Node 24 release 到验证有效的精确 commit，分别升级到 checkout `v7.0.1`、setup-node `v7.0.0`、upload-artifact `v7.0.1`；所有 checkout 禁止 credential persistence。
- 防回退：新增无第三方 YAML 依赖的静态验证器和负向测试，拒绝 mutable tag、旧 Node 20 SHA、版本标签漂移、畸形 `uses`、非 40 位 SHA 和令牌持久化；纳入 `security-check`。没有把 warning 隐藏或降低级别。
- `KI-026` 同轮完成 exact lock 与上游拓扑复核；当前上游仍依赖 `paste`，因此采用可见、自动到期的 P2 disposition，而非维护私有 netlink fork。

## Gate 04 协议丢包恢复闭环（2026-08-10）

- `XS-2026-0032`：KeyUpdate 和 PathChallenge 的重试复用原始 AEAD 帧；接收方处理首次请求后会把逐字节重试判为重放，丢失 Ack/Response 时无法恢复。修复为保留逻辑 payload/Epoch/Path ID/token，但每次通过现有 DataSender 生成新序列和密文；回归同时断言旧帧仍被拒绝。
- `XS-2026-0033`：Server 发送 ServerFinish 后立即进入 Established 并丢弃编码，最终响应丢失时重复 ClientFinish 得不到响应。修复为按 ClientFinish 摘要有界缓存精确 ServerFinish；重复请求不重建 nonce/密钥且不授权路径迁移。
- `XS-2026-0034`：KeyUpdate 尝试耗尽后仅清空 pending，下一 Tick 会静默启动另一轮相同 Epoch 更新，与规范要求的完整重握手不符。修复为显式 `Rehandshake` 动作并清除会话路径探测状态；单元测试覆盖全部尝试和耗尽边界。
- `XS-2026-0035`：首个 CI run `31349380836` 被固定 rustfmt 门禁拒绝；格式修复后的 run `31349709018` 又由严格 Clippy 拒绝 105 行维护函数。按工具输出格式化并抽取会话维护函数，未添加 allow/ignore；最终 run `31350065978` 四项 job 全通过，失败运行保持可审计。

## Gate 06 Linux Agent 恢复测试闭环（2026-08-10）

- `XS-2026-0036`：原 systemd 回归验证了 transient unit、TUN 隔离和可信 cleanup，但 `SIGKILL` 后由脚本手工清理，没有证明正式 `Restart=on-failure` 行为。修复后测试要求不同 PID 的单次自动重启、状态保留、TUN 私有重建、链路变化存活和最终清理。
- `XS-2026-0037`：首次 CI 加固版从 runner home 直接执行 Agent，而 unit 保持 `ProtectHome=yes`，真实失败为 `203/EXEC`。没有关闭 `ProtectHome`；改为把精确构建产物复制到唯一的 `/usr/local/lib/xs-nexus-tests/<unit>` 测试路径，并只清理该路径。
- `XS-2026-0038`：运行路径修复后，`systemd-analyze verify` 仍检查正式 `/usr/local/lib/xs-nexus/current` 可执行文件，导致 `unit_verify` 失败。改为在临时 root 中复制正式 unit 和测试二进制，再以 `--root` 验证；主机正式安装路径不再触碰。
- 失败 runs `31351583134`、`31351658402`、`31352027606` 与失败 artifact `9049311530` 均保留。最终 exact-head run `31352258781` 五项 job 全通过，专项 artifact `9049384061` 的归档/内部哈希和无值秘密扫描通过。

## Gate 08 Relay 资源与恢复闭环（2026-08-10）

- `XS-2026-0039`：原 Relay 只有每来源注册和每 Lease 速率/队列限制；来源地址喷洒可在昂贵验签前增长状态，多个合法 Lease 可把总队列扩大到每节点上限之和。修复为验签前全局注册预算、共享 packet/byte 队列上限、交叉配置校验和 enqueue/dequeue/cleanup 精确计数；拒绝不提交 replay 或 traffic state。
- `XS-2026-0040`：首次双 Relay 重启回归在重启主 Relay 已成为实际认证活动端点时仍只接受字符串 `relay_failover`，把合法 `relay_fallback` 状态命名误报为失败。断言现只对该已认证重启端点接受协议已有的两种 Relay 原因，仍强制端点、建立会话、双向流量和后续 Direct 回切；失败 run `31354320136` 与 artifact `9050139049` 保留。
- `XS-2026-0041`：容量脚本创建相对 evidence 目录后，Cargo 从 crate 目录运行测试，报告写入路径失效。脚本先把目录解析为绝对路径；后续独立复核又发现内部 `SHA256SUMS` 保存 runner 绝对路径，下载后需改写才能校验，最终改为在 evidence 目录内生成原始相对清单。runs `31355385268`、`31358498444` 分别保留失败与可移植通过证据。
- `XS-2026-0042`：5,000,000 帧测试在约 60 秒后超时，根因不是 UDP 窗口丢包，而是只接收流量的目标 Lease 达到 idle timeout。测试没有延长 TTL 或关闭过期检查；两个节点现每 400,000 帧以同身份/端点和唯一签名请求真实续租，并为新 Lease 重置独立 sequence。失败 run `31355663342` 与 artifact `9050582896` 保留。
- `XS-2026-0043`：严格 CI 依次暴露报告数值转换、测试身份种子传递和 `needless_borrow`；对应 runs `31355195399`、`31356055876`、`31356167042` 及 Relay artifacts `9050386927`、`9050672375`、`9050798784` 保留。没有添加 allow/ignore、降低 5,000,000 帧、降低 10,000 包/s、允许丢包或跳过重启。
- 最终精确 revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` 的 run `31358498444` Relay job `93362562136` 通过；artifact `9051561308` 可下载后直接复核两层 SHA-256，秘密扫描 0 findings。真实公网、多地域、多实例和长时矩阵由 `KI-028` 保持开放，Gate 08 不提升为 `PASS`。

## Gate 09 ACL 强制执行闭环（2026-08-10）

- `XS-2026-0044`：Agent 只对外层 `configuration.version` 执行单调检查；合法签名、较高外层版本但较低 `policy_version` 的配置可回滚当前 ACL。修复在任何状态变更前独立拒绝策略版本下降；新增回归证明拒绝后当前签名状态逐字节不变，同时保留低外层版本、同版本异内容、无效签名和无效 ACL 的既有失败关闭测试。
- `XS-2026-0045`：首个通过 artifact `9052236354` 的网络断言有效，但日志只保留最终成功行，独立审查者无法从 artifact 逐项确认离线配置、发送端拒绝、接收端拒绝及 Relay/子网绕过。测试仅增加结构化成功标记并重新运行，没有改变流量、等待、端口、策略、失败条件或断言；最终 artifact `9052383034` 可逐项复核完整矩阵。
- 三节点 Direct 回归使用真实 Agent、TUN、bridge 和 namespace，Controller 明确不可用；覆盖 A→B 允许，A→C/C→B 拒绝，ICMP/TCP/UDP、异常端口、伪造虚拟源、经允许节点路由及接收端独立拒绝。协议回归同时篡改源/目标 Node ID，并证明非法帧不会消耗有效帧的重放状态。
- Relay 回归在认证 Relay 活动时要求拒绝流量既不产生匹配 XSR 数据帧也不抵达接收端；子网回归要求拒绝 TCP/UDP 既不产生 XSP 帧也不抵达 LAN 服务。没有把绕过测试替换为 Mock。
- 精确 revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` 的 run `31360862865` 七个 job 全通过；ACL job `93369332314`、artifact `9052383034`、归档 digest `fd5cbc219591264ae6f1376db2d5c4aa9949c33be4684196887a3703a9ef8e23`、内部清单和无值秘密扫描通过。Gate 09 提升为 `PASS`，但总体仍为 `NO_GO`。

## Gate 15 1Panel 共存闭环（2026-08-11）

- `XS-2026-0046`：首个 current-source Compose 门禁使用 `config --no-interpolate`，Docker Compose 将边缘端口的未解析 host IP 表达式作为非法地址拒绝。修复为在私有临时目录生成无秘密、完整的环境和挂载夹具，再执行正常插值的 JSON 渲染；失败 run `31364684902` 与 artifact `9053670838` 保留。
- `XS-2026-0047`：渲染修复后，migration、ops 和 baseline profile 服务未进入默认 Compose service set，严格预期集合按设计失败。夹具显式启用全部三个已审 profile，并在失败消息中保留 expected/actual 集合；失败 run `31366046340` 与 artifact `9054165902` 保留。
- `XS-2026-0048`：运行时外部网络、sentinel 和默认路由断言全部通过，但 EXIT cleanup 最初只返回非零而没有差异。加入有界清理重试和原始差异后，run `31504209932` 证明 hosted runner 在 Docker daemon restart 时只重建内置 `bridge` 的 ID；目标外部网络 ID 和 sentinel ID 均保持。最终比较稳定的 name/driver/scope 全局 inventory，同时继续对目标网络和 sentinel 执行精确 ID 检查，并把最终 PASS 移到 cleanup/baseline 验证之后。runs `31367632436`、`31504209932` 与 artifacts `9054770008`、`9106338810` 保留。
- 精确 revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6` 的 run `31504402285` 八个 job 全通过；共存 job `93822197946`、artifact `9106406005`、归档 SHA-256 `93e174890d19264398090dcb891ab434a3839d769c4f0f14854982245678a28f`、内部清单和无值秘密扫描通过。
- 生产 Gate 14/16 的宿主 reboot、1Panel/OpenResty/SSH 恢复、项目升级、四次自动回滚和网络/数据库不变量证据完成正式矩阵映射。Gate 15 提升为 `PASS`；本分支未部署且总体仍为 `NO_GO`。

## Gate 18 更新与发布供应链子矩阵（2026-08-12）

- `XS-2026-0049`：schema 2 Shell 校验最初使用字符串 `<` 比较版本/epoch，ShellCheck `SC2071` 正确拒绝可能的词法排序。改为先执行规范十进制/u32/u64 边界验证再算术比较，并增加构建器非法边界回归；失败 run `31510934548` 与 artifact `9109033398` 保留。
- `XS-2026-0050`：archive-hash 负向夹具同时改变 `source_commit`，无法证明归档身份门禁本身。夹具现只修改目标字段并保持其余已签名字段不变；失败 run `31511341568` 与 artifact `9109197337` 保留。
- `XS-2026-0051`：安装器生命周期在 hosted `iproute2` 文本格式差异处提前退出且缺少命令上下文。先增加 ERR 行号/命令诊断，再把断言改为严格语义前缀和 metric，而不是发行版相关的整行文本；runs `31511912254`、`31512364301` 与 artifacts `9109507310`、`9109688746` 保留。
- `XS-2026-0052`：撤销 rollback 检查位于读取当前已安装二进制版本之后，导致撤销目标仍可执行一段候选代码。检查已移至任何目标二进制执行之前，并以 marker 回归证明零执行；失败 run `31512767254` 与 artifact `9109857831` 保留。
- `XS-2026-0053`：Gate 18 首次通过后的全量 run 暴露 Relay 最终 Direct 状态只做一次采样，在合法收敛窗口内偶发读到旧状态。改为有界轮询严格 JSON 状态，端点/会话/流量和最终 Direct 条件均未放宽；run `31513216122` 与 artifact `9110017348` 保留。
- `XS-2026-0054`：Relay 修复后严格 baseline Clippy 拒绝 Controller 的 101 行函数。只抽取 rollout release validation，不改变事务、错误或授权语义；run `31513788232` 与 artifact `9110258489` 保留。
- `XS-2026-0055`：抽取后的 SQLx 0.9 helper 对 transaction wrapper 执行查询而非底层 connection，专项 job 因 executor trait 失败。改为在同一事务连接上执行，保持锁和原子性；run `31514717354` 与 artifact `9110599058` 保留。
- 最终 candidate `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1` 的 run `31515281011` 全九 job 通过；Gate 18 job `93858773221`、artifact `9110864186`、GitHub/独立下载一致的归档 SHA-256、八个内部 payload 和无值秘密扫描通过。Gate 18 仍因正式密钥仪式/分发、签名 RC、真实平台和生产发布链保持 `PARTIAL`，总体仍为 `NO_GO`。
