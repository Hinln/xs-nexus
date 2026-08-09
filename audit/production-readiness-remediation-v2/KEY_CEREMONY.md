# Formal Key Ceremony Gate

## Status

`BLOCKED_EXTERNAL`. Codex can prepare scripts, validation, manifests, and runbooks, but cannot self-close a human-controlled root/update/recovery key ceremony.

## Required Human-Controlled Ceremony

1. Use an offline, freshly prepared and independently verified environment.
2. Generate separate root, release/update, backup-recovery, and any required TLS identities.
3. Record public identifiers and fingerprints only; never record private material in Git, production hosts, chat, logs, screenshots, or audit artifacts.
4. Split authorization and backup custody between designated operators.
5. Distribute verification public keys through at least one independent authenticated channel.
6. Sign a fixed clean revision and verify signatures on a separate machine.
7. Exercise rotation, revocation, emergency withdrawal, backup recovery, and loss procedures.
8. Archive signed ceremony minutes and public evidence outside the repository.

## Expected Result

Production hosts contain only the public material and short-lived operational keys they require. Old keys are rejected after rotation, and recovery succeeds from the controlled backup.

## Risk And Rollback

Incorrect key publication can block all updates or authorize the wrong artifacts. Keep the previous trusted public key active only for a documented overlap window, test dual verification, and publish revocation only after new-path validation.

## Resume Condition

Provide the public ceremony record, public-key fingerprints, signed test manifest, rotation/revocation result, and recovery result. Never provide private keys.
