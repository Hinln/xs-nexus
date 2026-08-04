# 第三方依赖与许可证

状态：源码与四个运行镜像依赖/许可证闭包已验证  
日期：2026-08-02

`Cargo.lock` 和 `package-lock.json` 是当前版本锁定的机器可读来源。`supply-chain/npm-licenses.json` 固定 npm 官方注册表中与 lock integrity 一致的精确版本许可证；Cargo 许可证由 `cargo metadata --locked` 读取并与 `Cargo.lock` SHA-256 checksum 逐项绑定。`make source-sbom SBOM_OUTPUT=<绝对新目录> SOURCE_DATE_EPOCH=<时间>` 离线生成完整源码传递依赖清单；任何新增、删除、版本、checksum、integrity 或许可证变化都必须同步通过门禁。四个容器的 exact image ID、113 个已安装 OS 包、逐包许可证材料、Dockerfile/revision 和 provenance 已在 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z` 闭环；RC 仍需从最终固定提交重新生成和复核，而不是把本次开发证据当成生产签名。

## 1. 当前运行时直接依赖

### Rust 服务与协议

| 依赖 | 锁定版本 | 用途 | 许可证 |
|---|---:|---|---|
| axum | 0.8.9 | Controller HTTP/WebSocket | MIT |
| argon2 | 0.5.3 | 控制台密码 Argon2id 哈希与验证 | Apache-2.0 OR MIT |
| base64 | 0.23.0 | Base64URL wire encoding | MIT OR Apache-2.0 |
| chacha20poly1305 | 0.10.1 | XSP/1 Finish 与数据 AEAD | Apache-2.0 OR MIT |
| chrono | 0.4.45 | UTC 时间与数据库时间 | MIT OR Apache-2.0 |
| ed25519-dalek | 3.0.0 | 标准 Ed25519 签名与验证 | BSD-3-Clause |
| futures-util | 0.3.33 | WebSocket stream/sink | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | 操作系统 CSPRNG | MIT OR Apache-2.0 |
| hkdf | 0.13.0 | XSP/1 HKDF-SHA-256 密钥派生 | MIT OR Apache-2.0 |
| ipnet | 2.12.0 | IPv4 prefix 和 IPAM 边界 | MIT OR Apache-2.0 |
| reqwest | 0.12.28 | Agent HTTPS enrollment，禁用系统原生 TLS | MIT OR Apache-2.0 |
| rtnetlink | 0.21.0 | Linux Agent 使用 Netlink 管理接口、地址和路由 | MIT |
| serde | 1.0.229 | 严格请求与配置序列化 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | JSON API 和精确配置 payload | MIT OR Apache-2.0 |
| sha2 | 0.11.0 | SHA-256、Node ID 与域分离摘要 | MIT OR Apache-2.0 |
| sqlx | 0.8.6 | PostgreSQL 连接、迁移与事务 | MIT OR Apache-2.0 |
| subtle | 2.6.1 | 常量时间 Token hash 比较 | BSD-3-Clause |
| thiserror | 2.0.19 | 内部错误类型 | MIT OR Apache-2.0 |
| tokio | 1.53.1 | 异步运行时、信号与定时器 | MIT |
| tokio-tun | 0.15.2 | Linux-only `/dev/net/tun` 非持久设备 FD 封装 | MIT OR Apache-2.0 |
| tokio-tungstenite | 0.29.0 | Agent WSS 控制连接与 loopback 集成测试 | MIT |
| tower-http | 0.7.0 | request ID 与脱敏 HTTP tracing | MIT |
| tracing | 0.1.44 | 结构化运行日志 | MIT |
| tracing-subscriber | 0.3.23 | 日志订阅与过滤 | MIT |
| uuid | 1.24.0 | 资源标识和 request ID | Apache-2.0 OR MIT |
| x25519-dalek | 2.0.1 | XSP/1 临时 X25519 密钥协商 | BSD-3-Clause |
| zeroize | 1.9.0 | 临时密钥和 Token 内存清零 | Apache-2.0 OR MIT |
| windows-sys | 0.61.2 | 隔离 Win32 设备枚举、独占 handle 与同步 `DeviceIoControl` 绑定 | MIT OR Apache-2.0 |

这些 crate 只提供通用 Web、数据库、序列化、标准密码学原语、Linux Netlink 和 `/dev/net/tun` 文件描述符封装，不包含现成组网、VPN、穿透或 Relay 实现。`tokio-tun` 仅用于 Linux TUN 系统调用封装，不启用持久设备，也不用于 Windows。

### Windows adapter exception (user-approved)

Windows release `0.1.0` distributes the official Wintun `0.14.1` x64 prebuilt `wintun.dll` only as a signed, hash-pinned L3 adapter dependency. The release builder verifies the upstream archive SHA-256 `07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51`, DLL SHA-256 `e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce`, and an Authenticode signer subject of `CN=WireGuard LLC`; the installer repeats the DLL checks before loading it. The upstream prebuilt-binary license is copied into every Windows archive as `THIRD_PARTY/WINTUN-LICENSE.txt`. Wintun is restricted to adapter/session creation and bounded IPv4 packet I/O; it does not replace XS Nexus control, identity, XSP/1, encryption, peer selection, ACL, routing policy, NAT traversal, or relay code.

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
| `@playwright/test` | 1.62.1 | 固定 Chromium 的端到端与视觉回归 | Apache-2.0 | 否 |
| Vitest | 4.1.10 | 前端单元测试 | MIT | 否 |
| `@types/react` | 19.2.17 | TypeScript 类型 | MIT | 否 |
| `@types/react-dom` | 19.2.3 | TypeScript 类型 | MIT | 否 |

### 容器基础镜像与运行工具

| 镜像/组件 | 固定系列 | 用途 | 许可说明 |
|---|---|---|---|
| Distroless `cc-debian12:nonroot` | 固定 SHA-256 digest | Controller/Relay 非 root 运行时 | Debian/distroless 包逐包许可证闭包；无 shell/package manager |
| NGINX unprivileged | `1.29-alpine` | Console 非 root 静态服务和反向代理 | NGINX BSD-2-Clause 与 Alpine 包逐包许可证闭包 |
| PostgreSQL Alpine | `18-alpine3.22` | `pg_dump`/`pg_restore`/`psql` 与 age 加密备份运维镜像 | PostgreSQL License、Alpine 包及固定 age 源码许可证闭包 |
| Alpine | `3.22` | 外部网络只读配置探针和错误镜像测试 | Alpine 包各自许可证 |

Controller/Relay 的 Rust `1.93.0-bookworm`、Console 的 Node `24-bookworm-slim` 和 db-tools 的 digest-pinned Go builder 只作为构建阶段，不进入最终运行镜像。db-tools 从 age `v1.3.1` 精确提交构建并强制 `golang.org/x/crypto v0.52.0`，只复制静态 CLI 和上游许可证。当前镜像 SBOM、113/113 许可证闭包、provenance、Grype 报告和有界 disposition 已验证；结果为 Critical 2、High 4、Medium 16、Negligible 24、当前可修复项 0。剩余 glibc 风险由 `KI-021` 跟踪，不能因无当前修复版本而隐藏或自动接受。

## 3. 工具与 CI

| 工具 | 用途 | 分发状态 |
|---|---|---|
| Rust/Cargo/rustfmt/Clippy | 构建、格式和 lint | 服务器工具链，不进入应用产物 |
| Node.js/npm | Console 构建和依赖审计 | 构建工具，不进入浏览器 bundle |
| PostgreSQL 18 Alpine | CI/本地迁移与事务集成测试 | 临时 service，不进入应用产物 |
| GCC/Clang/CMake/Make | 本地和未来驱动构建 | 构建工具 |
| `gcc-aarch64-linux-gnu` / `libc6-dev-arm64-cross` | Linux aarch64 交叉编译和链接 | 构建工具，不进入应用产物 |
| Rust `rust-src` / Cargo `build-std` | 为 aarch64 目标构建匹配标准库 | 构建工具，不进入应用产物 |
| OpenSSL CLI | Linux 发布清单 Ed25519 签名和验证 | 构建/安装工具，不链接进入应用产物 |
| GNU tar/gzip/coreutils/binutils | 确定性归档、SHA-256、文件处理和 ELF Machine 验证 | 构建/安装工具，不进入应用产物 |
| Docker Compose | 服务编排 | 运行环境工具 |
| Alpine `3.22` | 基线 profile 的网络配置探针 | 当前只解析配置，尚未发布 |
| Chromium for Testing 151.0.7922.34 | Playwright 固定浏览器渲染 | 仅测试缓存，不进入应用产物 |
| `actions/checkout@v4` | CI 源码检出 | GitHub Actions |
| `actions/setup-node@v4` | CI Node 工具链 | GitHub Actions |
| `dtolnay/rust-toolchain@stable` | CI Rust 工具链 | GitHub Actions |

## 4. 传递依赖

Rust 与 npm 传递依赖分别锁定在 `Cargo.lock` 和 `package-lock.json`。`scripts/generate-source-sbom.py` 对 321 个第三方 Cargo crate 和 110 个唯一 npm 包生成逐组件名称、版本、PURL、许可证与锁定摘要；输出同时包含 CycloneDX 1.6、SPDX 2.3 和输入/输出 SHA-256 manifest。生成过程禁止网络访问、拒绝未知或不允许的许可证、拒绝禁用产品依赖、拒绝快照与锁文件集合不一致，并使用 `SOURCE_DATE_EPOCH` 与输入摘要产生字节级可复现输出。`scripts/test-source-sbom.py` 验证双次输出一致及缺失许可证、拒绝许可证、既有输出目录和禁用依赖负向路径。完整证据为 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。

## 5. 密码学依赖边界

M1.3 通过 `ed25519-dalek`、`x25519-dalek`、`hkdf`、`chacha20poly1305`、`sha2`、`getrandom`、`subtle` 和 `zeroize` 调用成熟原语与安全辅助能力，不自行实现 Ed25519、X25519、HKDF、ChaCha20-Poly1305、SHA-256、CSPRNG 或常量时间比较。

新增密码学 crate 已锁定版本和 Cargo 校验和，许可证通过 `cargo metadata --locked` 核对；它们只提供标准原语，不包含现成组网协议、NAT 穿透、Relay 或虚拟网卡实现。RFC 原语向量、项目独立 session/data 向量及 Fuzz seed corpus 已纳入自动化回归。源码与当前运行镜像 SBOM/许可证文本已完成；维护状态复核、最终 RC 重建和第三方协议/密码学审计仍是发布前强制工作。

## 6. 禁止依赖

核心路径不得引入任务书列出的现成组网、VPN、穿透、中继或虚拟网卡产品，包括其可执行程序、内核模块、驱动、私有协议实现和包装层。
