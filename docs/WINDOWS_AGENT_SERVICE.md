# Windows Agent Service

## Scope

The Windows Agent exposes a dedicated `service --config <absolute-path>` entry point and connects to the Service Control Manager under the fixed service name `XsNexusAgent`. The runtime entry point does not create, delete, reconfigure, or install a service. Those operations remain the responsibility of the future signed Windows installer.

The service host is isolated in `crates/windows-service`. Shared Agent code remains safe Rust and invokes only the crate's safe `run_service` and shutdown interfaces.

## Lifecycle Contract

- `StartServiceCtrlDispatcherW` is called on the process main path rather than from a detached worker.
- `RegisterServiceCtrlHandlerExW` installs one control handler for the fixed service name.
- The host reports `START_PENDING`, then `RUNNING` with only STOP and SHUTDOWN accepted.
- The first STOP or SHUTDOWN atomically transitions to `STOP_PENDING` and wakes the Agent exactly once.
- INTERROGATE is acknowledged without changing service state.
- Agent completion reports `STOPPED`; configuration, runtime, or panic failure uses `ERROR_SERVICE_SPECIFIC_ERROR` with service code 1.
- A status mutex prevents a concurrent STOP from being overwritten by a later `RUNNING` report.

The SCM callback runs the existing Tokio Agent runtime through its current multi-thread runtime handle. A Tokio watch channel bridges the one-shot SCM shutdown notification into the same `run_agent` shutdown contract used by console mode. The service host does not poll, sleep, retry controls, or perform I/O from a destructor.

## Failure Boundaries

- Empty, path-like, NUL-containing, or over-256-UTF-16-unit service names are rejected before the dispatcher call.
- Repeated process-local initialization is rejected.
- Dispatcher connection failure is mapped to the Agent runtime error boundary.
- A poisoned status or handler mutex fails closed.
- Panics from the Agent callback are caught at the FFI boundary and reported as service-specific failure.
- The non-Windows implementation rejects service hosting and never invokes the callback.

Exactly four auditable unsafe blocks contain the dispatcher, handler registration, and status-reporting FFI calls. Agent code continues to contain no unsafe block.

## Validation

`scripts/test-windows-agent-service.sh` performs:

1. source invariant validation, including the fixed name, accepted controls, lifecycle states, status lock, one-shot notification, unsafe count, and absence of service installation APIs;
2. portable service-name and Linux rejection tests;
3. Agent command parser tests proving `service` is accepted only for the Windows platform mode and rejects enrollment-only arguments;
4. `x86_64-pc-windows-msvc` check and Clippy warnings-as-errors for the isolated service crate;
5. inclusion in the M6.1 Agent session validation.

## External Gates

Cross-target compilation does not prove SCM execution. M6.1 and M6.2 remain incomplete until a controlled Windows 11 VM verifies:

- installation under the intended LocalSystem identity;
- start, stop, shutdown, repeated start, crash, and SCM timeout behavior;
- effective named-pipe and ProgramData ACLs under the final service token;
- event logging and diagnostic behavior without an interactive console;
- complete Agent linking with the pinned Windows SDK and driver toolchain;
- uninstall, upgrade, rollback, and zero-residual service registration.

The current runtime deliberately contains no `CreateServiceW`, `DeleteService`, or `ChangeServiceConfig` call and must not be described as a Windows installer.
