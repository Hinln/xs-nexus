# 第三方依赖与许可证

状态：M1.2 Agent 信任链基础依赖清单  
日期：2026-07-29

`Cargo.lock` 和 `package-lock.json` 是当前版本锁定的机器可读来源。版本与许可证字段已通过 `cargo metadata` 核对；任何新增依赖必须同步更新本文件。发布前仍须生成完整 SBOM、许可证文本集合和构建来源证明。

## 1. 当前运行时直接依赖

### Rust 服务与协议

| 依赖 | 锁定版本 | 用途 | 许可证 |
|---|---:|---|---|
| axum | 0.8.9 | Controller HTTP/WebSocket | MIT |
| base64 | 0.23.0 | Base64URL wire encoding | MIT OR Apache-2.0 |
| chrono | 0.4.45 | UTC 时间与数据库时间 | MIT OR Apache-2.0 |
| ed25519-dalek | 3.0.0 | 标准 Ed25519 签名与验证 | BSD-3-Clause |
| futures-util | 0.3.33 | WebSocket stream/sink | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | 操作系统 CSPRNG | MIT OR Apache-2.0 |
| ipnet | 2.12.0 | IPv4 prefix 和 IPAM 边界 | MIT OR Apache-2.0 |
| reqwest | 0.12.28 | Agent HTTPS enrollment，禁用系统原生 TLS | MIT OR Apache-2.0 |
| serde | 1.0.229 | 严格请求与配置序列化 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | JSON API 和精确配置 payload | MIT OR Apache-2.0 |
| sha2 | 0.11.0 | SHA-256、Node ID 与域分离摘要 | MIT OR Apache-2.0 |
| sqlx | 0.8.6 | PostgreSQL 连接、迁移与事务 | MIT OR Apache-2.0 |
| subtle | 2.6.1 | 常量时间 Token hash 比较 | BSD-3-Clause |
| thiserror | 2.0.19 | 内部错误类型 | MIT OR Apache-2.0 |
| tokio | 1.53.1 | 异步运行时、信号与定时器 | MIT |
| tokio-tungstenite | 0.29.0 | Agent WSS 控制连接与 loopback 集成测试 | MIT |
| tower-http | 0.7.0 | request ID 与脱敏 HTTP tracing | MIT |
| tracing | 0.1.44 | 结构化运行日志 | MIT |
| tracing-subscriber | 0.3.23 | 日志订阅与过滤 | MIT |
| uuid | 1.24.0 | 资源标识和 request ID | Apache-2.0 OR MIT |
| zeroize | 1.9.0 | 临时密钥和 Token 内存清零 | Apache-2.0 OR MIT |

这些 crate 只提供通用 Web、数据库、序列化和标准密码学原语，不包含现成组网、VPN、穿透、Relay 或虚拟网卡实现。

### Console 浏览器产物

| 依赖 | 锁定版本 | 用途 | 许可证 |
|---|---:|---|---|
| React | 19.2.8 | Console UI | MIT |
| React DOM | 19.2.8 | Console DOM 渲染 | MIT |

## 2. 当前开发和测试直接依赖

| 依赖 | 锁定版本 | 用途 | 许可证 | 进入产物 |
|---|---:|---|---|---|
| http-body-util | 0.1.4 | HTTP handler 集成测试 | MIT | 否 |
| tempfile | 3.24.0 | Agent 原子状态持久化测试 | MIT OR Apache-2.0 | 否 |
| tower | 0.5.3 | Router one-shot 测试 | MIT | 否 |
| TypeScript | 7.0.2 | 类型检查 | Apache-2.0 | 否 |
| Vite | 8.1.5 | Console 构建 | MIT | 否 |
| `@vitejs/plugin-react` | 6.0.4 | React 构建插件 | MIT | 否 |
| Vitest | 4.1.10 | 前端单元测试 | MIT | 否 |
| `@types/react` | 19.2.17 | TypeScript 类型 | MIT | 否 |
| `@types/react-dom` | 19.2.3 | TypeScript 类型 | MIT | 否 |

## 3. 工具与 CI

| 工具 | 用途 | 分发状态 |
|---|---|---|
| Rust/Cargo/rustfmt/Clippy | 构建、格式和 lint | 服务器工具链，不进入应用产物 |
| Node.js/npm | Console 构建和依赖审计 | 构建工具，不进入浏览器 bundle |
| PostgreSQL 17 Alpine | CI 迁移与事务集成测试 | 临时 CI service，不进入应用产物 |
| GCC/Clang/CMake/Make | 本地和未来驱动构建 | 构建工具 |
| Docker Compose | 服务编排 | 运行环境工具 |
| Alpine `3.22` | 基线 profile 的网络配置探针 | 当前只解析配置，尚未发布 |
| `actions/checkout@v4` | CI 源码检出 | GitHub Actions |
| `actions/setup-node@v4` | CI Node 工具链 | GitHub Actions |
| `dtolnay/rust-toolchain@stable` | CI Rust 工具链 | GitHub Actions |

## 4. 传递依赖

Rust 与 npm 传递依赖分别锁定在 `Cargo.lock` 和 `package-lock.json`。M0.1 的 `npm audit --audit-level=high` 结果为 0 漏洞；M1.1 依赖变更后必须重新执行。完整传递许可证导出和 CycloneDX/SPDX SBOM 属于 M7.2/M9.1 前强制工作，不能以本表替代。

## 5. 密码学依赖边界

M1.1 仅通过 `ed25519-dalek`、`sha2`、`getrandom`、`subtle` 和 `zeroize` 调用成熟原语与安全辅助能力，不自行实现 Ed25519、SHA-256、CSPRNG 或常量时间比较。M1.3 引入 X25519、HKDF-SHA-256 和 ChaCha20-Poly1305 前必须：

1. 锁定版本和校验和；
2. 记录许可证和维护状态；
3. 确认不包含现成组网协议实现；
4. 增加标准及项目独立测试向量；
5. 更新本文件、威胁模型和 SBOM。

## 6. 禁止依赖

核心路径不得引入任务书列出的现成组网、VPN、穿透、中继或虚拟网卡产品，包括其可执行程序、内核模块、驱动、私有协议实现和包装层。
