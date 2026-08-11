# Gate 19 Console and Visual UX Evidence

## Result

`PARTIAL`

当前源码的内部生产形态子矩阵为 `PASS`：真实 PostgreSQL、真实 Controller、生产构建后的 Vite preview 和 Chromium Playwright 全部通过，测试没有拦截或伪造 API。Gate 19 仍不是 `PASS`，因为计划域名的严格源站 TLS、CDN、正式公网 Console/API/WebSocket 和外部浏览器路径仍由 Gate 13、`PRV2-005`、`BLK-003` 与 `KI-025` 阻塞。

## Exact Scope

| Item | Evidence |
|---|---|
| Candidate revision | `5505893710ab1d15e06495603dff08bf5c1e035f` |
| Exact-head CI | GitHub Actions run [`31529393933`](https://github.com/Hinln/xs-nexus/actions/runs/31529393933), all 9 jobs passed |
| Console job | [`93905489516`](https://github.com/Hinln/xs-nexus/actions/runs/31529393933/job/93905489516), `PASS` |
| Artifact | `9116327161`, `console-real-e2e-evidence` |
| Archive digest | `f6d4903febab15e6c92bd46ee91451bfbea849fae84a166afd4aa1c0b67632d6`, identical in GitHub metadata and independent ZIP download |
| Inner manifest | `SHA256SUMS` digest `abe90dbf2525ee4587c1d521208253c262c3f3418d2ffee354b10cf7fcd78b31`; all 139 listed files exist, all hashes match, and no downloaded file is unlisted |
| Test result | 8 expected, 0 unexpected, 0 skipped, 0 flaky; 7 production-matrix scenarios plus the original real Controller flow |
| Browser observations | 14 observation sets, 0 page errors, 0 HTTP 5xx; only the two explicitly injected offline requests fail |
| Screenshots | 132 PNG files: 6 viewports × 19 states, 16 initial real-data pages, and 2 loading/offline states |
| Secret surface | No-value scan `PASS`, 0 findings; successful artifact contains no trace, video, credential URL, private key, authorization header, cookie header, or credential assignment |

The tested deployment does not currently consume Redis in the Console authentication or data path; no Redis implementation or mock was substituted. PostgreSQL remains the tested fact source. This is an explicit architecture boundary, not a claim that every historical Redis design statement is exercised.

## Real Runtime Matrix

| Scenario | Required evidence | Result |
|---|---|---|
| Bootstrap, loading, login, empty state and initialization | Stop/resume the real Controller, observe loading, reject a bad login, authenticate, visit every initial management page, create a network once under rapid double activation, create a token, enroll two Ed25519 nodes, create an auditor, and observe real audit data | PASS |
| Node, ACL and API boundaries | Keyboard-accessible node detail, default-deny ACL, malformed JSON `400`, missing CSRF `403`, and unknown endpoint `404` | PASS |
| Concurrent stale state | Two authenticated tabs retain valid CSRF state; a stale configuration version reaches and receives the required `409` instead of an unrelated authorization failure | PASS |
| Visual and accessibility matrix | Login plus all 16 management pages, node dialog and not-found state at `1920x1080`, `1440x900`, `1280x720`, `1024x768`, `768x1024`, and `390x844`; document overflow, accessible names, keyboard skip link and bounded table scrolling asserted | PASS |
| Destructive action | Node revoke cancellation sends no request; confirmation sends exactly one request and validates the canonical `200` response body and revoked state | PASS |
| Auditor boundary | Write controls are absent, a direct forged write receives the canonical `403` envelope, logout returns `204`, and the same session subsequently receives `401` | PASS |
| Offline and recovery | Browser network is actually disabled; old data is replaced by the explicit error state; only the expected Console/users reads fail; reconnect and reload restore real data | PASS |

The source guard rejects `page.route`, `context.route`, and HAR routing anywhere under `apps/console/tests-real`. The matrix uses generated test-only credentials stored outside the evidence directory, a dedicated PostgreSQL schema, a real Controller binary, a production Console build, and no test-only health state.

## Visual Review

- Desktop dashboard and ACL layouts preserve hierarchy, factual unavailable states, readable controls, and full-width management content without document-level horizontal overflow.
- Mobile login, dashboard, audit and management views retain readable spacing and named controls. Wide tables remain inside an explicit horizontal-scroll shell instead of expanding the document.
- Loading and offline screenshots show distinct truthful states. The offline state does not retain or imply stale health data and exposes a clear reload action.
- The matrix validates every interactive control at the reference desktop viewport, every login viewport, every node dialog, and the keyboard skip-link focus transfer.
- Manual review sampled desktop dashboard/ACL, mobile login/dashboard/audit, and the offline state. No clipped primary action, false healthy value, secret value, broken glyph, or unbounded page overflow was found.

## Security and Defect Closure

- Network creation now has a synchronous in-flight guard. The real matrix originally observed two POST requests before this product defect was fixed; the final run requires exactly one created network.
- Browser-compatible username validation replaces a pattern rejected by Chromium's Unicode `v` regular-expression rules. A local regression validates the constraint.
- Session CSRF state is now stable across tabs by deriving the public CSRF token through a domain-separated SHA-256 digest of the 256-bit random HttpOnly session token. The session token remains HttpOnly, only domain-separated hashes are stored, and two tabs now reach the intended optimistic-concurrency `409` boundary.
- Playwright traces are disabled for the real matrix because traces record filled form values. Two failed trace-bearing artifacts were deleted after diagnosis; the immutable failed workflow logs remain. The successful artifact contains screenshots only.
- Chromium reports a successful fetch returning `204` as `requestfailed/net::ERR_ABORTED`. The observer suppresses that browser quirk only when the exact same Playwright request object also has a real `204` response; a true abort without a response remains a failure. Logout status and post-logout `401` are both asserted.
- Evidence checksums are generated only for non-hidden files because GitHub artifact upload excludes hidden files by default. The final independently downloaded package has an exact 139/139 manifest-to-file correspondence.

## Retained Failure Chain

| Run | Failure or finding | Disposition |
|---|---|---|
| `31521405820` | ShellCheck rejected evidence checksum output aliasing | Generate through a private temporary file |
| `31521600690` | Real rapid activation sent two network-create requests | Product in-flight guard and local regression |
| `31521968920` | Chromium rejected the username pattern; failed traces exposed generated test values | Valid pattern regression; traces disabled; affected artifacts deleted |
| `31522425606` | Opening a second tab rotated the session CSRF token and caused unrelated `403` | Stable domain-separated per-session CSRF derivation |
| `31522820229` | Test expected revoke `204`, while the canonical API is `200` with a typed body | Assert the real contract and response fields |
| `31523146155` | Revoked-state locator was ambiguous because two correct labels existed | Assert status and action regions separately |
| `31523454612` | First attempt hit a transient hosted package-feed error; rerun exposed an incorrect expected `403` message | Infrastructure rerun retained; exact canonical error envelope asserted |
| `31525584275` | Successful logout `204` was misclassified as an unexplained Chromium abort | Pair request failure with the same successful response object |
| `31527592960` | All nine jobs passed, but downloaded evidence referenced an uploader-filtered hidden file | Manifest generation aligned with the upload boundary; run retained as non-final evidence |
| `31529393933` | Exact-head final validation | All nine jobs and the independently verified Console evidence package pass |

No test was skipped, marked allowed-failure, retried for product nondeterminism, or weakened. Every final behavior assertion remains strict.

## Hard-Gate Boundary

This evidence closes the self-solvable Gate 19 gap that previously relied on API mocks and a shallow real flow. It does not prove the planned public domain, strict origin certificate/SNI, CDN routing, external browser path, current production deployment, credential rotation, signed release provenance, real Windows/NAS operation, offsite recovery, or independent security audit.

Gate 19 therefore remains `PARTIAL`; Gate 13 remains `FAIL`; the overall production decision remains `NO_GO`.
