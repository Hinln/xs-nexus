# External Gate Handoff

## Purpose

Repository code and safely automatable validation are complete. Formal production remains `NO_GO` until every external gate has real no-secret evidence. This handoff provides one fail-closed evidence format for all remaining manual work; it does not convert a checklist into test evidence.

The current evidence-kit schema is v2. Windows receipts must explicitly identify `virtual-machine` or `physical-machine`; v1 kits cannot be reused for a changed candidate.

## Prepare The Kit

Use the final full 40-character commit and a directory outside the repository, source checkout, shell-synchronized folder, and production host:

```bash
python3 scripts/external-gate-kit.py init \
  --revision <FULL_SHA> \
  --output /secure/offline/xs-nexus-external-gates-<FULL_SHA>
```

The kit creates twelve receipts and separate evidence directories for:

1. host identity;
2. credential rotation;
3. formal key ceremony;
4. independent security audit;
5. Windows real-system testing on a recoverable VM or dedicated physical target;
6. NAS real-system testing;
7. real WAN, Relay, subnet and capacity testing;
8. planned-domain strict TLS/CDN;
9. independent alert delivery and on-call closure;
10. offsite clean-server recovery;
11. the formal minimum 24-hour soak;
12. owner-signed independent release and production rehearsal.

Never store passwords, tokens, cookies, private keys, complete credential URIs, unredacted authorization headers, Enrollment Tokens, or raw production data in the kit. Public fingerprints, redacted receipts, non-secret identifiers, hashes, exit statuses, inventories and sanitized logs are allowed.

## Execute A Gate

1. Confirm the receipt still names the exact candidate revision.
2. Set `status` to `IN_PROGRESS`, identify the operator without personal secrets, and record UTC start time.
3. Execute the matching existing runbook in this directory. Do not set checks from planned work or screenshots of commands that were not executed.
4. Put sanitized raw evidence only below `evidence/<gate>/`.
5. List every required evidence kind in the receipt with a relative path and `sha256: null`.
6. After every check is factually true, set `status` to `COMPLETE`, set UTC completion time, fill permitted public identifiers/metrics, then seal:

```bash
python3 scripts/external-gate-kit.py seal \
  --root /secure/offline/xs-nexus-external-gates-<FULL_SHA> \
  --expected-revision <FULL_SHA> \
  --gate <GATE_NAME>
```

The sealer rejects missing evidence, false checks, secret-like fields/content, unsafe paths, symlinks, wrong revisions, insufficient soak duration, non-independent audit/release operators, TOFU host identity and mismatched host fingerprints.

## Verify And Resume

After moving the kit through an approved channel, independently verify it:

```bash
python3 scripts/external-gate-kit.py verify \
  --root /secure/offline/xs-nexus-external-gates-<FULL_SHA> \
  --expected-revision <FULL_SHA>
```

When all twelve receipts are complete, require the entire set:

```bash
python3 scripts/external-gate-kit.py seal \
  --root /secure/offline/xs-nexus-external-gates-<FULL_SHA> \
  --expected-revision <FULL_SHA> \
  --require-complete

python3 scripts/external-gate-kit.py verify \
  --root /secure/offline/xs-nexus-external-gates-<FULL_SHA> \
  --expected-revision <FULL_SHA> \
  --require-complete
```

`READY_FOR_FRESH_AUDIT` means only that the external evidence kit is complete and internally consistent. It does not mean `GO`. Create a new `audit/production-readiness-final/`, independently rerun all 25 Hard Gates from the owner-signed exact RC, and issue a new explicit `GO`, `CONDITIONAL_GO`, or `NO_GO` decision.

## First Manual Action

Confirm the production server's ED25519 host fingerprint through the cloud provider console or another genuinely independent management channel. Do not authenticate first, disable strict checking, overwrite `known_hosts`, or accept repeated TOFU. The `host-identity` receipt requires equal approved/observed fingerprints and rejects `ssh`, `tofu`, `same-session`, `chat`, or `email-only` as the out-of-band channel.

Only after host identity is confirmed may an approved session perform read-only inventory or later owner-authorized production work. Destructive soak fault injection belongs on an independent disposable QA host, not on production.
