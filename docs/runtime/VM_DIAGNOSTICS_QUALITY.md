# Terlan VM Diagnostics Quality

This document defines the 0.0.7 diagnostics contract for VM-first Terlan.

## Contract

User-facing VM diagnostics must be Terlan diagnostics, not leaked implementation
errors. CLI output must not expose Rust panic output, raw backend errors,
internal stack dumps, or host-runtime stack traces.

Every VM failure surfaced through `terlc` or `terlan-vm` must carry a stable
diagnostic code. The required diagnostic families are:

- `vm_load` for malformed native images, descriptor mismatches, unsupported
  native targets, and incompatible runtime ABI requirements.
- `vm_execute` for missing exports, rejected native transitions, and VM runtime
  execution failures.
- `native_boundary` for NativeBoundary request, payload, resource, timeout,
  cancellation, and stale-handle failures.
- `debugger_source_map` for missing, invalid, or stale source-map/debug data.
- `project_migration` for post-OTP project migration failures.

The same failure must be representable as text diagnostics and JSON
diagnostics. Text diagnostics must use the `error[code]` shape or an equivalent
stable code prefix. JSON diagnostics must expose at least `kind`, `code`, and
`message`, and must remain parseable by editor and automation tooling.

When native debug data exists, VM runtime failures must prefer source spans over
native instruction offsets. Source-span diagnostics must identify the Terlan
module, function, file, and source range when those fields are present in the
native debug section.

Typed diagnostic probes may observe one exact stable code from an explicit
installation point. They must ignore earlier diagnostics, unrelated codes, and
message text that merely resembles a known failure. A matched probe stays
matched while its log is append-only. Probes are bound to one log generation,
and cross-log, closed, and duplicate-close queries fail deterministically.

Fatal VM diagnostics use the versioned `terlan.vm.fatal-diagnostic.v1` support
bundle rather than the ERTS crash-dump text format. Capture is explicitly
enabled or disabled, bounded by subject and serialized-byte limits, and
deterministic for one VM state. An enabled bundle records scheduler accounting,
every retained process, terminal-state kind, and explicitly observed missing
process identities. It records resource and mailbox counts but never resource
handles, mailbox payloads, exit payloads, or host-local source paths. A complete
synced private file is atomically published through a same-directory link that
cannot replace an existing destination; disabled, oversized, malformed, and
failed captures leave no public partial artifact.

## Adversarial Coverage

The release gate must keep adversarial tests for:

- malformed native images
- missing exports
- descriptor mismatches
- unsupported native targets
- stale resources
- NativeBoundary failures
- malformed NativeBoundary payloads
- duplicate NativeBoundary request ids
- missing or invalid source-map/debug metadata
- typed diagnostic probes for I/O resource lifecycle failures
- bounded fatal snapshots across runnable, blocked, exited, and missing process
  states
- disabled, oversized, malformed, collision, and atomic-publication fatal
  bundle behavior

These tests are release gates, not optional implementation notes. A failure
that can reach a user must either have a stable diagnostic today or remain
explicitly unsupported behind a stable diagnostic.
