#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - The `terlan_cli` Cargo package providing the public `terlc` command.
# - A scratch filesystem where temporary Terlan projects can be created.
#
# Output:
# - Exit status 0 when the current Polars/native package boundary is stable.
# - Exit status 1 when `terlc` accepts unsupported native package shapes,
#   emits artifacts after a rejected native dependency, or changes the stable
#   diagnostic text this gate depends on.
#
# Transformation:
# - Creates temporary user-style Terlan projects and invokes only public `terlc`
#   command behavior. No internal compiler modules, validation artifacts, or
#   package-generation scripts are called directly.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d /tmp/terlan-0-0-3-polars-blackbox.XXXXXX)"

cleanup() {
  # Inputs:
  # - `$tmp_dir`: temporary directory created for black-box projects.
  #
  # Output:
  # - No reported output.
  #
  # Transformation:
  # - Removes throwaway project files and compiler outputs after the gate exits.
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

run_terlc() {
  # Inputs:
  # - Command arguments to pass after `terlc`.
  #
  # Output:
  # - The public `terlc` process status and command output.
  #
  # Transformation:
  # - Runs the scratch compiler binary through Cargo so this black-box gate does
  #   not depend on a previously installed `terlc`.
  cargo run -q --manifest-path "$repo_root/Cargo.toml" -p terlan_cli -- "$@"
}

write_buildable_project() {
  # Inputs:
  # - `$1`: project directory to create.
  # - `$2`: extra manifest text appended after the base package section.
  #
  # Output:
  # - A minimal manifest-backed Terlan project on disk.
  #
  # Transformation:
  # - Writes the smallest user project that can reach manifest validation before
  #   backend emission when unsupported native package metadata is present.
  local project_dir="$1"
  local manifest_tail="$2"

  mkdir -p "$project_dir/src/app"
  cat >"$project_dir/terlan.toml" <<EOF
[package]
name = "app"
version = "0.0.3"

[build]
source_roots = ["src"]
artifact = "beam-thin"

${manifest_tail}
EOF

  cat >"$project_dir/src/app/Main.terl" <<'EOF'
module app.Main.

pub main(): Unit ->
    Unit.
EOF
}

assert_contains() {
  # Inputs:
  # - `$1`: text blob.
  # - `$2`: required substring.
  #
  # Output:
  # - Exit status 0 when the substring is present.
  # - Exit status 1 with a diagnostic when it is absent.
  #
  # Transformation:
  # - Performs stable shell substring matching for user-facing CLI diagnostics.
  local haystack="$1"
  local needle="$2"

  if [[ "$haystack" != *"$needle"* ]]; then
    printf 'expected diagnostic substring missing:\n  %s\n\nactual output:\n%s\n' \
      "$needle" "$haystack" >&2
    exit 1
  fi
}

assert_path_absent() {
  # Inputs:
  # - `$1`: filesystem path that must not exist.
  #
  # Output:
  # - Exit status 0 when the path is absent.
  # - Exit status 1 when unsupported package metadata caused artifacts to leak.
  #
  # Transformation:
  # - Guards black-box failure-before-emission behavior.
  local path="$1"

  if [[ -e "$path" ]]; then
    printf 'unexpected artifact exists after rejected native package boundary: %s\n' \
      "$path" >&2
    exit 1
  fi
}

assert_files_equal() {
  # Inputs:
  # - `$1`: expected file path.
  # - `$2`: actual file path.
  #
  # Output:
  # - Exit status 0 when both files exist and have identical bytes.
  # - Exit status 1 with a diagnostic when the generated skeleton drifts from
  #   the scratch package reference.
  #
  # Transformation:
  # - Performs byte-level comparison without relying on git diff.
  local expected="$1"
  local actual="$2"

  if [[ ! -f "$expected" ]]; then
    printf 'expected reference file missing: %s\n' "$expected" >&2
    exit 1
  fi

  if [[ ! -f "$actual" ]]; then
    printf 'expected generated file missing: %s\n' "$actual" >&2
    exit 1
  fi

  if ! cmp -s "$expected" "$actual"; then
    printf 'generated file differs from reference:\n  expected: %s\n  actual:   %s\n' \
      "$expected" "$actual" >&2
    exit 1
  fi
}

assert_file_contains() {
  # Inputs:
  # - `$1`: file path to inspect.
  # - `$2`: required text fragment.
  #
  # Output:
  # - Exit status 0 when the file contains the fragment.
  # - Exit status 1 with a diagnostic when required ABI metadata is absent.
  #
  # Transformation:
  # - Performs fixed-string matching for generated package contract metadata.
  local path="$1"
  local needle="$2"

  if [[ ! -f "$path" ]]; then
    printf 'expected file missing: %s\n' "$path" >&2
    exit 1
  fi

  if ! grep -Fq "$needle" "$path"; then
    printf 'expected file fragment missing from %s:\n  %s\n' "$path" "$needle" >&2
    exit 1
  fi
}

unsupported_bind_out="$tmp_dir/unsupported_bind"
set +e
unsupported_bind_output="$(
  run_terlc bind rust --crate serde --out "$unsupported_bind_out" 2>&1
)"
unsupported_bind_status=$?
set -e

if [[ "$unsupported_bind_status" -eq 0 ]]; then
  printf 'expected unsupported Rust binding crate to fail.\n' >&2
  exit 1
fi

assert_contains "$unsupported_bind_output" \
  'unsupported rust binding crate `serde`; supported crates: polars'
assert_path_absent "$unsupported_bind_out"

polars_dependency_project="$tmp_dir/polars_dependency_project"
polars_dependency_out="$tmp_dir/polars_dependency_out"
write_buildable_project "$polars_dependency_project" \
  '[target.rust.dependencies]
polars = { cargo = "polars", version = "0.54.4", features = ["lazy", "csv", "strings"] }'

set +e
polars_dependency_output="$(
  run_terlc --out-dir "$polars_dependency_out" build "$polars_dependency_project" --target erlang 2>&1
)"
polars_dependency_status=$?
set -e

if [[ "$polars_dependency_status" -eq 0 ]]; then
  printf 'expected Polars cargo dependency metadata to fail before backend emission.\n' >&2
  exit 1
fi

assert_contains "$polars_dependency_output" \
  'declares unsupported rust dependency `polars` from cargo package `polars` version `0.54.4`'
assert_contains "$polars_dependency_output" \
  'package-manager integration is not available'
assert_path_absent "$polars_dependency_out/src/app_main.erl"
assert_path_absent "$polars_dependency_out/terlan-debug-map.json"
assert_path_absent "$polars_dependency_out/terlan-package-build.json"

polars_skeleton_project="$repo_root/scratch/packages/std/native/polars"
polars_skeleton_out="$tmp_dir/polars_skeleton_out"
has_polars_reference=0

if [[ -f "$polars_skeleton_project/terlan.toml" && -f "$polars_skeleton_project/src/std/native/polars/DataFrame.terl" ]]; then
  has_polars_reference=1
fi

set +e
if [[ "$has_polars_reference" -eq 1 ]]; then
  polars_skeleton_output="$(
    run_terlc --out-dir "$polars_skeleton_out" build "$polars_skeleton_project" --target erlang 2>&1
  )"
else
  generated_reference_project="$tmp_dir/generated_polars_reference"
  run_terlc bind rust --crate polars --out "$generated_reference_project" >/dev/null
  polars_skeleton_output="$(
    run_terlc --out-dir "$polars_skeleton_out" build "$generated_reference_project" --target erlang 2>&1
  )"
fi
polars_skeleton_status=$?
set -e

if [[ "$polars_skeleton_status" -eq 0 ]]; then
  printf 'expected Polars package skeleton to stop at unsupported cargo dependency metadata.\n' >&2
  exit 1
fi

assert_contains "$polars_skeleton_output" \
  'terlc build package `std-native-polars` declares unsupported rust dependency `polars` from cargo package `polars` version `0.54.4`'
assert_contains "$polars_skeleton_output" \
  'package-manager integration is not available'
assert_path_absent "$polars_skeleton_out/src/std_native_polars_dataframe.erl"
assert_path_absent "$polars_skeleton_out/terlan-debug-map.json"
assert_path_absent "$polars_skeleton_out/terlan-package-build.json"

generated_polars_project="$tmp_dir/generated_polars"
run_terlc bind rust --crate polars --out "$generated_polars_project" >/dev/null

if [[ "$has_polars_reference" -eq 1 ]]; then
  assert_files_equal \
    "$polars_skeleton_project/terlan.toml" \
    "$generated_polars_project/terlan.toml"
  assert_files_equal \
    "$polars_skeleton_project/src/std/native/polars/DataFrame.terl" \
    "$generated_polars_project/src/std/native/polars/DataFrame.terl"
  assert_files_equal \
    "$polars_skeleton_project/bindings/polars.mapping.toml" \
    "$generated_polars_project/bindings/polars.mapping.toml"
  assert_files_equal \
    "$polars_skeleton_project/native/terlan-native.toml" \
    "$generated_polars_project/native/terlan-native.toml"
  assert_files_equal \
    "$polars_skeleton_project/docs/std.native.polars.md" \
    "$generated_polars_project/docs/std.native.polars.md"
  assert_files_equal \
    "$polars_skeleton_project/examples/read_csv.terl" \
    "$generated_polars_project/examples/read_csv.terl"
  assert_files_equal \
    "$polars_skeleton_project/summaries/std.native.polars.DataFrame.typi" \
    "$generated_polars_project/summaries/std.native.polars.DataFrame.typi"
  assert_files_equal \
    "$polars_skeleton_project/native/rust/Cargo.toml" \
    "$generated_polars_project/native/rust/Cargo.toml"
  assert_files_equal \
    "$polars_skeleton_project/native/rust/src/lib.rs" \
    "$generated_polars_project/native/rust/src/lib.rs"
  assert_files_equal \
    "$polars_skeleton_project/native/rust/src/bridge.rs" \
    "$generated_polars_project/native/rust/src/bridge.rs"
fi

assert_file_contains "$generated_polars_project/native/rust/src/lib.rs" \
  '#![forbid(unsafe_code)]'
assert_file_contains "$generated_polars_project/native/rust/src/bridge.rs" \
  '#![forbid(unsafe_code)]'

native_abi="$generated_polars_project/native/terlan-native.toml"
assert_file_contains "$native_abi" 'namespace = "std.native.polars"'
assert_file_contains "$native_abi" 'adapter = "rust"'
assert_file_contains "$native_abi" '[runtime]'
assert_file_contains "$native_abi" 'bridge = "supervised_actor"'
assert_file_contains "$native_abi" 'worker = "rust_thread_probe"'
assert_file_contains "$native_abi" 'ownership = "opaque_handles"'
assert_file_contains "$native_abi" 'backpressure = "credit"'
assert_file_contains "$native_abi" 'handle_generation_tokens = true'
assert_file_contains "$native_abi" '[runtime.commands]'
assert_file_contains "$native_abi" 'start = "start_worker"'
assert_file_contains "$native_abi" 'call = "typed_request"'
assert_file_contains "$native_abi" 'stop = "stop_worker"'
assert_file_contains "$native_abi" '[runtime.beam]'
assert_file_contains "$native_abi" 'supervision = "std.beam.NativeBridge.NativeBridgeRuntime"'
assert_file_contains "$native_abi" 'process = "std.beam.Process.Process"'
assert_file_contains "$native_abi" 'message = "std.beam.Message.MessageCodec"'
assert_file_contains "$native_abi" 'backpressure = "std.beam.Backpressure.Backpressure"'
assert_file_contains "$native_abi" 'credit = "std.beam.Backpressure.Credit"'
assert_file_contains "$native_abi" '[types."std.native.polars.DataFrame.DataFrame"]'
assert_file_contains "$native_abi" 'rust = "TerlanPolarsDataFrame"'
assert_file_contains "$native_abi" '[errors."std.core.Error.Error"]'
assert_file_contains "$native_abi" 'rust = "TerlanPolarsError"'
assert_file_contains "$native_abi" 'conversion = "code_message"'
assert_file_contains "$native_abi" 'native_unavailable_code = "native_unavailable"'
assert_file_contains "$native_abi" '[functions."std.native.polars.DataFrame.read_csv"]'
assert_file_contains "$native_abi" 'rust = "read_csv"'
assert_file_contains "$native_abi" '[result_conversions."std.native.polars.DataFrame.read_csv"]'
assert_file_contains "$native_abi" '[methods."std.native.polars.DataFrame.height"]'
assert_file_contains "$native_abi" 'rust = "height"'
assert_file_contains "$native_abi" '[methods."std.native.polars.DataFrame.width"]'
assert_file_contains "$native_abi" 'rust = "width"'
assert_file_contains "$native_abi" '[methods."std.native.polars.DataFrame.columns"]'
assert_file_contains "$native_abi" 'rust = "columns"'
assert_file_contains "$native_abi" '[methods."std.native.polars.DataFrame.select"]'
assert_file_contains "$native_abi" 'rust = "select"'
assert_file_contains "$native_abi" '[result_conversions."std.native.polars.DataFrame.select"]'

package_doc="$generated_polars_project/docs/std.native.polars.md"
assert_file_contains "$package_doc" 'std.native.polars'
assert_file_contains "$package_doc" 'terlc bind rust --crate polars'
assert_file_contains "$package_doc" 'Real Polars execution requires the future Rust/native target capability.'

set +e
polars_example_output="$(
  run_terlc --out-dir "$tmp_dir/polars_example_out" build "$generated_polars_project/examples/read_csv.terl" --target erlang 2>&1
)"
polars_example_status=$?
set -e

if [[ "$polars_example_status" -eq 0 ]]; then
  printf 'expected generated Polars example to be target-gated on Erlang.\n' >&2
  exit 1
fi

assert_contains "$polars_example_output" \
  'terlc build --target erlang cannot import native package'
assert_contains "$polars_example_output" \
  '`std.native` packages require the Rust/native target capability'
assert_path_absent "$tmp_dir/polars_example_out/src/examples_polars_readcsv.erl"
assert_path_absent "$tmp_dir/polars_example_out/terlan-debug-map.json"

generated_summary_check_out="$tmp_dir/generated_summary_check"
run_terlc \
  --out-dir "$generated_summary_check_out" \
  interface "$generated_polars_project/summaries/std.native.polars.DataFrame.typi" \
  >/dev/null

cargo test -q --manifest-path "$generated_polars_project/native/rust/Cargo.toml"
cargo clippy -q --manifest-path "$generated_polars_project/native/rust/Cargo.toml" \
  --all-targets -- -D warnings

polars_docs_out="$tmp_dir/polars_docs"
run_terlc \
  --out-dir "$polars_docs_out" \
  doc "$generated_polars_project/src" --format markdown \
  >/dev/null
run_terlc \
  --out-dir "$tmp_dir/polars_doc_check" \
  doc "$generated_polars_project/src" --check \
  >/dev/null
polars_dataframe_doc="$polars_docs_out/std.native.polars.DataFrame.md"
assert_file_contains "$polars_dataframe_doc" 'Native Polars DataFrame contract'
assert_file_contains "$polars_dataframe_doc" 'read_csv'
assert_file_contains "$polars_dataframe_doc" 'height'
assert_file_contains "$polars_dataframe_doc" 'select'

rust_target_project="$tmp_dir/rust_target_project"
rust_target_out="$tmp_dir/rust_target_out"
write_buildable_project "$rust_target_project" ""

set +e
rust_target_output="$(
  run_terlc --out-dir "$rust_target_out" build "$rust_target_project" --target rust 2>&1
)"
rust_target_status=$?
set -e

if [[ "$rust_target_status" -eq 0 ]]; then
  printf 'expected `terlc build --target rust` to fail until native build support exists.\n' >&2
  exit 1
fi

assert_contains "$rust_target_output" 'unsupported build target `rust`; supported targets: erlang'
assert_path_absent "$rust_target_out/src/app_main.erl"
assert_path_absent "$rust_target_out/terlan-debug-map.json"

native_import_project="$tmp_dir/native_import_project"
native_import_out="$tmp_dir/native_import_out"
mkdir -p "$native_import_project"
cat >"$native_import_project/Main.terl" <<'EOF'
module app.Main.

import std.native.polars.DataFrame.

pub main(): Unit ->
    Unit.
EOF

set +e
native_import_output="$(
  run_terlc --out-dir "$native_import_out" build "$native_import_project/Main.terl" --target erlang 2>&1
)"
native_import_status=$?
set -e

if [[ "$native_import_status" -eq 0 ]]; then
  printf 'expected Erlang build to reject std.native.polars import.\n' >&2
  exit 1
fi

assert_contains "$native_import_output" \
  'terlc build --target erlang cannot import native package'
assert_contains "$native_import_output" \
  '`std.native` packages require the Rust/native target capability'
assert_path_absent "$native_import_out/src/app_main.erl"
assert_path_absent "$native_import_out/terlan-debug-map.json"

printf '0.0.3 Polars black-box package boundary passed.\n'
