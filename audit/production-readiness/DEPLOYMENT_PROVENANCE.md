# Deployment Provenance

## Chain

| Layer | Value | Verification |
|---|---|---|
| GitHub main | `8745b5804312587534c1e91980dfb11720952ed1` | GitHub API 与本地 fetch |
| Production source | `ff9551d322067c934d2ac7d55a62af8896660bb3` | `/srv/xs-nexus` 与独立 worktree |
| Controller image | `sha256:38df60f4348be5a51c2866fd6cd6ff1717aa26282428916c34664704e1a1f269` | Docker inspect 与 OCI revision |
| Relay image | `sha256:6d51f8c1d763cb7e46e683ff2834cebf49157d443e4303452cd8dfd2a463a8d9` | Docker inspect 与 OCI revision |
| Console image | `sha256:67d3ce972c355c850f489cc3534c6d7572d07ca4dbbaaa8c8aaaf9f814a42b99` | Docker inspect 与 OCI revision |
| Controller binary | `9fb2cc69698feacee62fbc195b0e242f6a88df9718f877c5431a4098a229d397` | 运行镜像与 clean rebuild 相同 |
| Relay binary | `2af178212437d1076f45c298765c3d6bbec48aab667fe1f45bd9314579022cb0` | 运行镜像与 clean rebuild 相同 |

## Main Versus Deploy

`ff9551d...8745b580` 之间仅 16 个 Markdown/交接文件变化，没有 Rust、TypeScript、Dockerfile、Compose、SQL、Shell、systemd 或 CI 产品代码差异。因此 main 代码与部署代码在功能上等价，但生产仍没有直接部署 main 的证明。

## Rebuild Result

- 从干净 `ff9551d` checkout 使用 `--no-cache --pull` 重建。
- Controller/Relay 二进制逐字节一致，Console 三个静态文件逐字节一致。
- 完整镜像 ID 均不一致；可变 Rust/Node/Nginx/Postgres 基础引用、在线 `apk upgrade/add` 和输出时间戳破坏了完整可复现性。
- 没有发布 tag 或 Git 签名；GitHub main 最新 CI 因 mutable stable Rust/Clippy 变化失败。

## Migration And Compatibility

生产 `_sqlx_migrations` 1-9 的版本和 SHA-384 与 `ff9551d` 源迁移完全匹配。M5.2 重新通过协议向量、Agent/Controller、Relay、ACL、Subnet Router、安装和部署生命周期。

## Result

`FAIL`。源码到二进制的链较强，但源码到完整镜像、发布 tag、签名和 CI 的可复现 provenance 不完整。

## Final Remediation Reassessment

- 修复分支 `8532eb6` 的 Edge、Console、Controller、Relay、db-tools 均通过两次无缓存 OCI 逐字节复现；完整摘要见 `EVIDENCE_INDEX.md`。
- 最终 GitHub run `31270487478` 四个 job 全部成功，修复了首轮 mutable toolchain/action/base、SBOM 漂移和镜像时间不确定性。
- 生产仍运行 `ff9551d`，GitHub main 仍为 `8745b580`，修复分支未合并、签名、tag 或部署。因此该改善不能转写成生产 provenance PASS。

最终结果仍为 `FAIL`。
