# CLI `emit-native-metadata` Command Internals

This directory owns the `terlc emit-native-metadata` command runtime. The
implementation in `mod.rs` is centered on command-local validation and routing
to the shared native artifact generator.

## Responsibilities

- Validate that exactly one source file path is supplied.
- Read the source and run the formal compile path before emission.
- Reject unsafe native declarations for this command path.
- Create the configured output directory and emit native metadata artifacts.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

Public methods or values exposed to callers include `run`.

## Core Model

The command itself owns CLI behavior and exit-code handling. Native metadata
extraction and SafeNative stub generation live in the `artifacts` submodule
because the regular `emit` command also calls them when native declarations are
present. Compiler-owned Rust-backed std operations use
`@compiler.native {operation}` annotations on ordinary declarations. Generated
Rust stubs must preserve the SafeNative actor-bridge shape: opaque handles,
typed replies, request ids, explicit disposal, stale-handle errors, and credit
reporting. Generated neutral artifact names use `*.safe_native.json` and
`*.safe_native.rs`. The generated BEAM loader stub uses
`TERLAN_SAFE_NATIVE_PATH` only as the future attachment hook; current generated
loaders return stable `safe_native.not_loaded` replies until a concrete port,
worker, or audited NIF transport is implemented.

The main flow is:

1. Validate command-local argument count.
2. Read the source file.
3. Compile through parse, resolve, and typecheck phases.
4. Reject unsafe native declarations.
5. Delegate artifact generation and write outputs.

Important invariants:

- Native metadata emission only happens after formal compile validation.
- Unsafe native declarations are rejected explicitly.
- Write failures return exit code `1`; malformed arguments return exit code `2`.

## Integration Points

- `main.rs`: routes the command.
- `artifacts`: extracts metadata, emits JSON/Erlang/Rust artifacts, and
  validates generated Rust stubs.
- `validation::native_policy`: detects unsafe native declarations.
- Formal compile helpers: validate syntax output before artifact generation.

## Edge Cases

- Missing or extra paths print global usage.
- Output-directory conflicts fail before artifact generation.
- Generated Rust stubs are validated by the shared native artifact emitter.
- Generated Rust stubs remain `unsafe`-free and expose only actor-bridge
  placeholders until a real adapter is supplied.
- Compiler-owned Rust-backed std metadata uses `@compiler.native {operation}`.
- General target/runtime SafeNative metadata uses ordinary typed
  `@native { ... }` contract-block annotations on ordinary declarations.
- Source-level `#[native(...)]`, `#[nif(...)]`, and `native core module` blocks
  are not canonical Terlan source syntax and should not be added to new source
  contracts.

## Testing Notes

- Command-local tests cover the public `emit-native-metadata` path for a real
  `@compiler.native` std module.
- Artifact tests cover compiler-native metadata extraction, SafeNative stub
  generation, and operation inventory preservation.
