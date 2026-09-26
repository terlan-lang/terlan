# Terlan VM Binary Internals

This directory owns the `terlan-vm` binary as an internal compiler/runtime
implementation detail. It is not a separate public VM distribution. The binary
is packaged beside `terlc` so release builds can validate and exercise the
Rust-native runtime path directly.

## Responsibilities

- Parse `terlan-vm` command-line arguments.
- Load compiler-emitted native `.tvm` images and execute their entrypoints.
- Reject source execution and retired JSON artifacts; compilation belongs to
  `terlc`, with no runtime CoreIR interpreter or source fallback.
- Preserve text and test-evaluation output semantics for release validation.
- Keep local VM instrumentation providers independent from Terlan Cloud.
- Keep local and cloud dashboard providers aligned over shared logical
  components.
- Keep the first VM dashboard mode read-only.
- Define the future guarded operator action vocabulary without enabling it in
  v1.

## Public Surface

- `main.rs`: standalone binary entrypoint.
- `cli.rs` and `arguments.rs`: command dispatch and argument validation.
- `main/native_image_runner.rs`: native image execution and support bundles.
- `instrumentation.rs`: local-only VM instrumentation provider model plus
  provider-neutral dashboard component declarations.

## Core Model

The binary does not define a separate Terlan-to-VM compiler path. It loads
native images through `PureNativeExecutionShard` and executes them with
VM-owned runtime services.

Important invariants:

- The VM binary must not bypass compiler validation.
- Native image admission and package validation must succeed before execution;
  invalid images must not trigger source compilation or interpretation.
- `--test` (alias for `--test-eval`) accepts only boolean test results.
- User-facing errors must identify whether failure happened during read,
  compile, load, or execution.
- Local VM instrumentation must use local-process providers and must not
  require Terlan Cloud identity, deployment state, or network endpoints.
- Shared dashboard components must not encode provider transport assumptions;
  local and cloud providers should render the same component identities.
- Dashboard v1 must reject operator mode until guarded controls, policy, and
  audit semantics exist.
- Planned operator actions include hot reload, deploy, rollback, node drain,
  service restart, and replica promotion, but the v1 policy keeps them
  disabled and requires audit semantics before activation.

## Integration Points

- `runtime::native_image`: native image admission and package validation.
- `runtime::vm`: process ownership, scheduling, and native execution services.
- Release packaging: installs `terlan-vm` beside `terlc`.

## Testing Notes

- `main_test.rs` covers argument parsing, result contracts, and rejection of
  retired HTTP benchmark commands.
- Native image runner tests cover image execution and admission failures.
- HTTP performance belongs to the maintained `terlan-benchmark` AOT socket
  harness; the standalone VM retains only its in-memory framing benchmark.
