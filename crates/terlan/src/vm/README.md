# Terlan VM Binary Internals

This directory owns the `terlan-vm` binary as an internal compiler/runtime
implementation detail. It is not a separate public VM distribution. The binary
is packaged beside `terlc` so release builds can validate and exercise the
Rust-native runtime path directly.

## Responsibilities

- Parse `terlan-vm` command-line arguments.
- Compile one Terlan source file through the normal compiler frontend.
- Load the resulting CoreIR into the Rust VM and execute a zero-arity
  entrypoint.
- Load only the transitive artifact dependencies needed by source fallback;
  unrelated sibling artifacts must not affect execution or startup cost.
- Preserve text and test-evaluation output semantics for release validation.
- Keep local VM instrumentation providers independent from Terlan Cloud.
- Keep local and cloud dashboard providers aligned over shared logical
  components.
- Keep the first VM dashboard mode read-only.
- Define the future guarded operator action vocabulary without enabling it in
  v1.

## Public Surface

- `main.rs`: standalone binary entrypoint.
- `commands.rs`: command helpers shared by the VM binary surface.
- `instrumentation.rs`: local-only VM instrumentation provider model plus
  provider-neutral dashboard component declarations.

## Core Model

The binary does not define a separate Terlan-to-VM compiler path. It reuses the
formal compiler pipeline, loads CoreIR into `runtime::vm::TerlanVm`, and runs
the requested function.

Important invariants:

- The VM binary must not bypass compiler validation.
- Artifact source fallback follows structured imports and qualified CoreIR
  module references recorded in the optional `extensions.module_dependencies`
  field. It never discovers dependencies by parsing source text.
- Optional extension metadata selects which sibling artifacts to inspect. It
  is outside the executable checksum for backward compatibility, while each
  selected sibling still passes its own complete schema and checksum
  validation before compilation.
- Missing sibling artifacts are allowed because standard-library and
  runtime-native modules may not have package-local artifact files. Present
  but malformed or misidentified dependencies fail explicitly.
- `--test-eval` accepts only boolean test results.
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

- `formal_pipeline`: source-to-CoreIR compilation.
- `runtime::vm`: CoreIR execution.
- Release packaging: installs `terlan-vm` beside `terlc`.

## Testing Notes

- `main_test.rs` covers argument parsing and source execution.
- Artifact fallback tests prove transitive dependency loading, rejection of a
  malformed selected dependency, and isolation from malformed unrelated
  siblings.
- Release preflight checks compare `terlc run` output with `terlan-vm run`
  output for the bridge fixture.
