# Independent Security Audit Gate

## Status

`BLOCKED_EXTERNAL`. Internal Codex review cannot close this gate.

## Audit Package

Prepare `third-party-audit-package/` containing an exact clean commit, architecture, XSP/1 specification and vectors, cryptographic design, threat model, Controller/API, Relay, Linux Agent, Windows paths, supply-chain design, SBOM/license evidence, fuzz corpus/results, deployment model, known issues, and reproduction instructions.

## Minimum Independent Scope

- Protocol and cryptographic composition.
- Controller/API authentication, authorization, sessions, CSRF and storage.
- Relay abuse, amplification, metadata, resource exhaustion and isolation.
- Linux Agent privilege/network cleanup.
- Windows driver/adapter, service, storage, route and installer boundaries.
- Build/update/release supply chain.

## Completion

An independent assessor must issue a report, severity-ranked findings, and a retest statement after fixes. All Critical/High and production-blocking findings must be closed or explicitly rejected by the product owner with a documented scope reduction. The audit firm, report identifier, exact revision, and public-safe result are indexed; private report content remains in controlled storage.
