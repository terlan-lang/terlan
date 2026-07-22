# 0.0.7 Test And Pipeline OTP Exit Inventory

This inventory classifies remaining OTP, Erlang, and BEAM references in test
and pipeline surfaces during the 0.0.7 VM migration.

No default release gate may require stock OTP. No new OTP-dependent test or
pipeline may be added without this inventory. Remaining OTP references are
either explicit migration-lane checks, reference-only baselines, or removal
work. The default compiler/runtime lane is `terlan-vm` with the `CoreV0`
profile.

Classification labels:

- `default-release-gate`: must run without requiring stock OTP.
- `migration-lane`: explicit transitional path retained until replaced by
  Terlan VM execution.
- `reference-only`: historical or measurement material used to extract
  Terlan-owned behavior.
- `remove`: active 0.0.7 cleanup item.
- `historical`: retained documentation or compatibility context only.

| Path | Surface | Current OTP use | Classification | Required replacement/removal |
| --- | --- | --- | --- | --- |
| `Makefile` | release and quality gates | Runs current release and quality gates without stock OTP execution; remaining Erlang/BEAM wording is limited to Rust classification gates, reference inventories, and removed-spelling rejection assertions. | default-release-gate | Keep root Make targets free of OTP installation, runtime matrix execution, and stock `erl`/`erlc` release dependencies. |
| `.github/workflows/ci.yml` | CI pipeline | Runs Rust, stdlib, editor, and release-scale checks without installing OTP or running an Erlang runtime matrix. | default-release-gate | Keep CI free of OTP installation and runtime matrix steps. |
| `.github/workflows/release.yml` | release pipeline | Runs release validation without installing OTP or running an Erlang runtime matrix. | default-release-gate | Keep release validation free of OTP installation and runtime matrix steps. |
| `crates/terlan/cli.mk` | compiler test/release Make fragment | Provides current compiler, VM artifact, browser/static/web, stdlib, and formal gates without public `erlc` smoke targets; remaining Erlang-named formal gates are backend-contract migration tests only. | default-release-gate | Keep public CLI make targets free of `erl`/`erlc` execution; remove Erlang backend-contract tests when the backend is deleted. |
| `std/stdlib.mk` | standard library release Make fragment | Runs stdlib release tests through the VM-default test lane, with JS tests explicitly routed to the browser profile. | default-release-gate | Keep stdlib release tests on `terlc test` VM default and prevent implicit Erlang target arguments from returning. |
| `std/scripts/check_native_artifacts.py` | native artifact checker | Compares NativeBoundary JSON/Rust artifacts and compiles generated Rust skeletons without invoking `erlc` or accepting generated Erlang loader intermediates. | default-release-gate | Keep the checker independent of stock OTP and reject any return of generated Erlang loader artifacts. |
| `crates/terlan/src/commands/emit_native_metadata` | NativeBoundary metadata tests | Validates NativeBoundary JSON and generated Rust skeletons without generating Erlang loader text or invoking `erl`, `erlc`, or stock OTP. | default-release-gate | Keep this surface on JSON/Rust artifact validation only. |
| `scripts/check_release_boundary.sh` | release boundary checker | Rejects scratch `.beam`, `.erl`, and `.hrl` artifacts in release contents. | default-release-gate | Keep as a no-OTP leak guard. |
| `crates/terlan/src/commands/build/build_test/tests` | build command test directory guard | Every Rust test file in this directory must have its own explicit inventory row; concrete migration tests are classified individually below. | default-release-gate | Keep the directory-level row as an inventory guard only, not as a hiding place for migration tests. |
| `crates/terlan/src/commands/build/build_test/tests/artifact_test.rs` | VM, JS, and browser artifact tests | Covers Terlan VM artifact emission, JavaScript artifact emission, declarations, browser package assets, and negative no-Erlang/no-BEAM output assertions without stock OTP execution. | default-release-gate | Keep artifact tests VM/browser-first and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/args_test.rs` | build argument parser tests | Covers VM-default argument parsing, JavaScript/mobile target parsing, reserved target diagnostics, and removed Erlang target rejection without backend execution. | default-release-gate | Keep argument parsing VM/default-first and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/dependency_test.rs` | VM package dependency tests | Covers local path dependency diagnostics, dependency cycles, and unsupported package-manager metadata through the VM build target without backend execution. | default-release-gate | Keep dependency tests VM/default-first and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/executable_vm_artifact_test.rs` | VM executable artifact tests | Covers VM artifact entrypoint, constructor, and receiver-shape diagnostics without producing Erlang or BEAM artifacts. | default-release-gate | Keep executable VM artifact tests isolated from Erlang launcher/reference tests and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/import_constructor_test.rs` | VM imported-constructor closure tests | Records current VM import-closure gaps without producing Erlang, BEAM, or consumer VM artifacts. | default-release-gate | Keep imported-constructor gap coverage on the VM target and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs` | JavaScript target-family diagnostics | Covers JS/shared-JS rejection diagnostics for VM process, native, Postgres, and browser DOM import families without stock OTP execution. | default-release-gate | Keep JavaScript target diagnostics independent of removed backend checks and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/mobile_build_test.rs` | mobile build planning tests | Emits Android/iOS planning manifests and asserts no source/ebin build output is produced. | default-release-gate | Keep mobile planning tests independent of stock OTP, BEAM output, and stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/project_layout_test.rs` | VM project-layout tests | Covers manifest source roots, package-root validation, library artifacts, and VM project layout diagnostics without producing Erlang or BEAM artifacts. | default-release-gate | Keep project-layout tests VM-first and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/shape_js_test.rs` | imported-shape JavaScript build tests | Builds imported literal, tuple, nested, string-capture, and guarded shapes through CoreIR and Oxc, then executes emitted modules with Node when available without stock OTP execution. | default-release-gate | Keep imported-shape JavaScript tests independent of Erlang and BEAM artifacts and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/std_runtime_test.rs` | std.vm removed-target rejection test | Proves old std runtime fixtures cannot reopen the removed public `--target erlang` build spelling. | default-release-gate | Keep this as rejection-only VM/default coverage and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/wasm_artifact_metadata_test.rs` | Wasm/WASI package metadata tests | Covers VM-default, Wasm browser, and WASI package metadata projection without legacy BEAM metadata or runtime execution. | default-release-gate | Keep Wasm/WASI metadata tests target-neutral and reject stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/build/build_test/tests/wasm_build_target_test.rs` | reserved Wasm/WASI build target tests | Rejects reserved Wasm/WASI target families before artifact emission. | default-release-gate | Keep reserved target diagnostics independent of stock OTP and stale `beam-thin` fixtures. |
| `crates/terlan/src/commands/run` | run command tests | Defaults to `terlan-vm`, rejects explicit `--target erlang`, and covers removed runtime spellings as diagnostics only. | default-release-gate | Keep run command tests VM-first and keep removed Erlang target spellings as rejection coverage. |
| `crates/terlan/src/commands/test` | test command tests | Defaults to `terlan-vm`, rejects explicit `--target erlang`, and keeps JavaScript tests on the explicit JS profile. | default-release-gate | Keep test command tests VM-first and keep removed Erlang target spellings as rejection coverage. |
| `crates/terlan/src/commands/repl` | REPL tests | Defaults to `--runtime vm`, rejects `--runtime beam`, and verifies VM evaluation does not emit Erlang artifacts. | default-release-gate | Keep REPL tests VM-first and keep removed BEAM runtime spellings as rejection coverage. |
| `crates/terlan/src/commands/serve` | web/server tests | Keeps static, file, WebSocket, route-matching, VM-handler-unavailable, and native response validation tests without active BEAM handler execution. | default-release-gate | Keep `terlc serve` independent of OTP while replacing the current VM-handler-unavailable diagnostic with VM handler execution. |
| `crates/terlan/src/validation/target_profile` | target-profile tests | Tests VM/A0 profile progression, JavaScript profile gating, retired core-v0 proof-subset validation, and removed-target diagnostics without stock OTP execution. | default-release-gate | Keep target-profile validation on VM/JS/core vocabulary and prevent Erlang profile descriptions from returning. |
| `tools/check_http_runtime_stack.py` | HTTP runtime stack checker | Checks the approved Hyper/Tokio/http stack, native MIME boundary, native cookie boundary, and live-reload watcher integration. | default-release-gate | Keep rejecting manual HTTP/TCP parsing and any return of local cookie/header parsing outside the native boundary. |

Release closure rule: every `migration-lane` and `remove` row must either be
converted to a Terlan VM/default release gate or deleted before 0.0.7 is
declared complete.
