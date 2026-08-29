# Terlan Web Service Foundation

SF1.0 defines one Terlan-owned service contract. Cloudflare Foundations is an
optional native telemetry adapter; it is not the public API, server lifecycle,
configuration model, or listener.

## Canonical ownership

| Concern | Owner |
|---|---|
| Source APIs | `std.log` and `std.service` |
| Portable values, validation, sinks, lifecycle | `terlan-service-foundation` |
| Implicit VM handler context | `terlan::service_foundation` |
| Existing VM HTTP metrics and trace artifacts | `commands::serve::observability` |
| Existing local route/request logs | `commands::serve::logging` |
| Native reference telemetry | `terlan-foundations-adapter` |
| Listener, startup, signals, reload, admission, drain | embedding server/runtime |
| Application config and secret declarations | `terlan.toml` and typed Terlan declarations |

The implementation reuses the existing serve attribution and W3C boundary.
VM program output now enters a local JSON service sink with implicit request,
connection, route, handler, source, release, actor, and trace identity.

## Portable contract

Structured fields accept only booleans, signed/unsigned integers, finite
floats, and bounded strings. Event names are lowercase ASCII identities; field
count, name length, and value length are bounded. Names associated with
passwords, authorization, cookies, keys, tokens, or secrets are rejected.
`SecretRef` deliberately has no structured-field conversion.

Metric declarations fix the instrument name, kind, label keys, and cardinality
limit before samples are accepted. Raw URLs/paths, secret-like dimensions,
runtime-created names, and mismatched label sets are rejected.

`traceparent` is parsed at the HTTP boundary. Missing or malformed input yields
no parent. Unsampled identity may propagate but need not be exported. Nested
work inherits request context only while its parent remains active. A cancelled
or timed-out parent cannot admit nested work. Detached work retains
service/release/source identity but clears request and trace identity.

Readiness becomes false before draining begins. A draining service rejects new
admission and must bound in-flight requests, actors, native resources, and
telemetry flush time. Incomplete bounded drain ends failed rather than claiming
a clean stop.

## Host ABI and failure behavior

`ServiceSink` accepts logs, metrics, spans, health, drain, and config/secret
resolution notices. Calls are bounded acceptance operations and never grant a
sink authority to fail a valid customer request. The disabled, bounded-memory,
local human, and local JSON sinks require no collector or Cloud account.

The reference adapter pins Foundations 5.9.2 with
`default-features = false` and explicitly enables only `logging`, `metrics`,
and `testing`. It excludes tracing and OTLP until their upstream OpenTelemetry
dependency closure is patched, as well as settings, CLI, platform defaults,
telemetry bundles/server, Sentry, security/seccomp, jemalloc, and memory
profiling. It creates no Tokio runtime or public listener.

The adapter's BSD-3-Clause metadata, exact dependency closure, selected and
excluded features, semantic corpus, platform policy, security decisions,
rollback path, and source hashes are recorded in
`web-service-foundation-report.json`.

## Platform and rollback policy

The native reference adapter is admitted for Linux x86-64 and AArch64 service
hosts. Embedded Linux and Zephyr consume only the portable contract unless a
target-specific plan independently admits an adapter. Seccomp is deferred until
the complete steady-state syscall set is measured. Jemalloc and memory
profiling are deferred until a long-running service measurement justifies them.

Rollback removes selection of `terlan-foundations-adapter`; disabled and local
sinks preserve the public behavior. Upgrades change the exact version pin only
with feature-closure, dependency, semantic-corpus, pruning, and report gates.

## Executable gates

```text
make web-service-foundation-contract-check
make web-service-foundation-runtime-check
make web-service-foundation-adapter-check
make web-service-foundation-pruning-check
make web-service-foundation-check
```
