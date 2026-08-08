# Supply Chain Audit

## Locks And CI

- `Cargo.lock` 与 `package-lock.json` 存在；npm `ci` 和 audit 为 0 vulnerabilities。
- GitHub CI 使用 mutable action tags、`stable` Rust、`ubuntu-latest` 和未固定 digest 的 Postgres service；main 当前 CI 因 Rust/Clippy 漂移失败。
- 无发布 tag、Git 签名或 GitHub release；私有仓库当前账户无法启用分支保护 API。

## Rust

- `cargo audit` 报 RUSTSEC-2023-0071 `rsa` High 和 RUSTSEC-2024-0436 `paste` unmaintained。
- target-all dependency tree证明 `rsa` 不可达；`paste` 经 `rtnetlink -> netlink-packet-core` 可达。
- `cargo deny` 因无 `deny.toml` 和 unmaintained advisory 失败。

## Source SBOM

- 本轮 `test-source-sbom.py` 失败：锁文件含 `nanoid@3.3.18`，许可证快照缺失该版本并残留 `nanoid@3.3.16`。
- 因此 THIRD_PARTY/SBOM 不能作为当前版本完整证据。

## Images

- 实际生产 Controller/Relay/Console/db-tools SBOM 生成通过。
- Grype 0.116.1 最新数据库：Critical 2、High 4、Medium 16、Negligible 24，无 advertised fix。
- 2 Critical/4 High 是 Controller/Relay glibc；当前二进制和源码不使用被处置 API，但仍是限时风险接受。
- 完整 image ID clean rebuild 不一致；Rust/Node/Nginx/Postgres base 和 Alpine package 操作未完全固定。

## Result

`FAIL`。源 SBOM、CI、advisory policy、基础镜像和完整可复现性未闭环。

## Final Remediation Reassessment

- Rust `1.93.0`、Windows target、GitHub actions、Ubuntu runner、PostgreSQL service 和产品基础镜像均固定。
- `cargo audit`、`cargo deny`、npm audit、源 SBOM、许可证和依赖门禁在 run `31270487478` baseline 中通过。
- 当前源 SBOM 为 Cargo 325、npm 110、总计 435；旧 321 计数未被静默放宽，而是核对出新增的 DER/PEM/PKCS#8/SPKI 锁定依赖后更新快照。
- 五个最终 OCI 镜像各自两次无缓存构建逐字节一致；Edge 的唯一差异被定位为 `/var/log/apk.log` 安装时间并从运行时删除，Console 使用固定 digest slim runtime 和隔离许可证阶段。
- 生产镜像仍为旧 revision，修复版本未签名/tag/部署；实际生产 glibc Critical/High 仍只具有限时不可达性处置。

最终结果：`PARTIAL`，不是正式供应链 PASS。
