# Terlan CLI Run Command

## Purpose

The `run` command is the user-facing shortcut for executing Terlan VM source,
package launchers, and compiler-emitted Wasm core artifacts.

## Responsibilities

- Delegate compilation to the existing `build` command.
- Read `terlan-package-build.json` from the selected output directory.
- Execute the package launcher recorded in build metadata.
- Validate a `.wasm` artifact against its adjacent compiler manifest before
  executing a selected scalar export through the maintained hosted runtime.
- Reject stale Wasm checksums, unsupported ABI values, memory/table boundaries,
  missing host imports, missing exports, traps, and execution timeouts with
  stable diagnostic families.
- Return the launched program's exit status.

## Boundaries

- `run` does not implement a separate compiler path.
- `run` exposes only the `terlan-vm` artifact lane in the public CLI.
- Hosted Wasm execution accepts only `i32`, `i64`, `f32`, and `f64` parameters
  and results. Host function fixtures use explicit
  `--host-return module.name=type:value` bindings; ambient imports are denied.
- Wasm memory and table imports/exports remain unavailable until their ownership
  and bounds contracts are implemented.
- Historical Erlang launcher support is implementation migration material, not
  a supported `terlc run` target.

## Wasm Example

```text
terlc build src/Math.terl --target wasm.core
terlc run _build/wasm/app_Math.wasm --export add \
  --arg i32:19 --arg i32:23 --expect i32:42 --repeat 3
```

## Validation

Tests cover command argument rejection, executable metadata parsing, generated
Wasm execution, all supported scalar result shapes, typed host returns,
repeatability, stale manifests, unsupported memory exports, missing imports and
exports, traps, and timeout cleanup.
