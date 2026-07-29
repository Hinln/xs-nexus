# 第三方依赖与许可证

状态：M0.2 初始清单  
日期：2026-07-29

`Cargo.lock` 和 `package-lock.json` 是当前版本锁定的机器可读来源。任何新增依赖必须同时更新本文件，并在发布前生成完整 SBOM 和许可证清单。

## 1. 当前运行时直接依赖

| 依赖 | 锁定版本 | 用途 | 许可证 | 进入产物 |
|---|---:|---|---|---|
| React | 19.2.8 | Console UI | MIT | 是，浏览器 bundle |
| React DOM | 19.2.8 | Console DOM 渲染 | MIT | 是，浏览器 bundle |

当前 Rust workspace 只使用 Rust 标准库，没有第三方 Rust crate 进入产物。

## 2. 当前开发和测试直接依赖

| 依赖 | 锁定版本 | 用途 | 许可证 | 进入产物 |
|---|---:|---|---|---|
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
| Node.js/npm | Console 构建 | 构建工具，不进入浏览器 bundle |
| GCC/Clang/CMake/Make | 本地和未来驱动构建 | 构建工具 |
| Docker Compose | 服务编排 | 运行环境工具 |
| Alpine `3.22` | 基线 profile 的网络配置探针 | 当前只解析配置，尚未拉取或发布 |
| `actions/checkout@v4` | CI 源码检出 | GitHub Actions |
| `actions/setup-node@v4` | CI Node 工具链 | GitHub Actions |
| `dtolnay/rust-toolchain@stable` | CI Rust 工具链 | GitHub Actions |

## 4. 传递依赖

npm 传递依赖锁定在 `package-lock.json`。M0.1 的 `npm audit --audit-level=high` 结果为 0 漏洞。完整传递许可证导出和 CycloneDX/SPDX SBOM 尚未实现，属于 M7.2/M9.1 前强制工作，不能以本表替代。

## 5. 计划中的密码学依赖

M1.3 将只选择成熟、维护活跃且许可证兼容的标准原语库。候选类别包括 Ed25519、X25519、HKDF-SHA-256、ChaCha20-Poly1305、常量时间比较和内存清零。正式加入前必须：

1. 锁定版本和校验和；
2. 记录许可证和维护状态；
3. 确认不包含现成组网协议实现；
4. 增加标准测试向量；
5. 更新本文件和 SBOM。

## 6. 禁止依赖

核心路径不得引入任务书列出的现成组网、VPN、穿透、中继或虚拟网卡产品，包括其可执行程序、内核模块、驱动、私有协议实现和包装层。
