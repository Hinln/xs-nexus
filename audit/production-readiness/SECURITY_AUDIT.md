# Security Audit

## Secrets

- Git 历史、worktree、生产状态、运行日志和三个运行镜像扫描为零。
- QA artifacts 首轮发现一个审计工具返回的临时 GitHub clone token。它没有进入 Git 或生产状态；证据文件和扫描报告已脱敏，复扫为零。该即时处置发生在首轮报告前并在 `gitleaks-initial-finding-disposition.txt` 留痕。
- 无法证明开发期已披露的服务器、数据库、NAS、Console、Controller、Enrollment 和恢复凭据全部失效。正式 Gate 02 失败。

## Host Authentication

- `PermitRootLogin yes`、`PasswordAuthentication yes`、X11 和 TCP forwarding 开启。
- root 与 ubuntu 账户具有密码；未见强制 allowlist/MFA/仅密钥策略。
- 生产登录凭据与此前开发沟通中披露的凭据存在复用风险；审计禁止擅自轮换，保持 `BLOCKED_EXTERNAL`。

## Protocol

- M5.2 重新通过 transcript/身份绑定、credential、AEAD、replay window、key epoch、malformed corpus、Relay framing 和 namespace 数据面测试。
- `fuzz/` 只有 corpus，没有 `cargo-fuzz` manifest/target；没有本轮持续 fuzz 结果。
- 没有独立协议/密码学 review。Hard Gate 04 FAIL，Gate 05 BLOCKED_EXTERNAL。

## Keys And Updates

- Controller 未包含 update private key；测试 manifest/hash/signature/tamper/downgrade/rollback 流程通过。
- 没有正式离线 root/update key ceremony、双人控制、rotation、revoke、backup/recovery 证据。

## Database And Runtime

- Controller、Relay、Console 和 PostgreSQL 容器非 root、非 privileged、drop ALL、no-new-privileges；前三者只读 rootfs 和日志轮换有效。
- PostgreSQL 应用角色是 superuser/createdb/createrole/replication/bypassrls，属于 Security High。
- PostgreSQL `ssl=off`，但未发布数据库端口；这不能抵消过大权限和同网段横向风险。

## Dependencies

- `cargo audit`：RUSTSEC-2023-0071 `rsa` 高危但 target-all 不可达；RUSTSEC-2024-0436 `paste` unmaintained，实际经 rtnetlink 可达。
- `cargo deny` 无配置并失败；npm audit 为 0。
- 实际镜像有 2 Critical/4 High glibc `wont-fix`，当前二进制不导入处置 API；这是限时风险处置，不是漏洞消失。

## Result

`NO_GO`。Security Critical 运行时可达性未被发现，但 P1/Security High、正式密钥和第三方审计均未关闭。

## Final Remediation Reassessment

- 仓库、最终 CI artifacts 和生产复核文件的秘密扫描通过。
- XSP/1 新增可执行 fuzz targets；最终 artifact `9025496390` 证明 credential、handshake、data、discovery 和 relay fuzz 运行通过。
- 固定 `cargo audit`、`cargo deny`、SBOM 和依赖策略在最终 baseline 通过。
- 这些结果关闭“无 fuzz harness”和内部依赖门禁，但不能关闭已披露凭据轮换、SSH/root/password、正式密钥、bootstrap DB superuser 或独立第三方审计。

最终结果：`NO_GO`；Gate 04 `PARTIAL`，Gate 05 `BLOCKED_EXTERNAL`，Gate 02/16 `FAIL`。
