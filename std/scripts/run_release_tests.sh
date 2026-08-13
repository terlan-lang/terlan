#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - tests/std/RELEASE_API_TESTS.tsv, which identifies adjacent stdlib
#   release-test files.
# - The Cargo package providing the `terlc test` command.
# - TERLC_BIN, optionally overriding the compiler binary used for each test.
# - TERLAN_STD_TEST_TIMEOUT_SECONDS, optionally overriding the per-file timeout.
#
# Output:
# - Exit status 0 when every unique release-test file passes through
#   `terlc test` on the default VM lane.
# - Exit status 1 with file-specific diagnostics when any release test fails
#   or exceeds the timeout.
#
# Transformation:
# - Executes each manifest-listed Terlan @test function through the compiler's
#   test command with a bounded runtime so a stuck test cannot hang release
#   automation.
# - Generated JS interface rows are contract-surface checks validated by
#   `stdlib-release-api-tests-check`, not executable VM-default tests.

manifest="tests/std/RELEASE_API_TESTS.tsv"
test_timeout_seconds="${TERLAN_STD_TEST_TIMEOUT_SECONDS:-120}"
terlc_bin="${TERLC_BIN:-${CARGO_TARGET_DIR:-target}/debug/terlc}"
failures=0
release_cache_home=""
declare -A executed_test_files=()

# Inputs:
# - $1: stdlib release-test file path.
#
# Output:
# - Populates `target_args` with any target/profile arguments required for the
#   release-test file.
#
# Transformation:
# - Treats generated JavaScript standard-library tests as JavaScript browser
#   profile tests and keeps all other stdlib tests on the default VM test lane.
target_args_for_test() {
  target_args=()
  case "$1" in
    std/js/*)
      target_args=(--target js --target-profile js.browser)
      ;;
  esac
}

if [[ ! -f "$manifest" ]]; then
  printf 'stdlib release API test manifest is missing: %s\n' "$manifest" >&2
  exit 1
fi

if [[ -z "${TERLC_BIN:-}" ]]; then
  printf 'building terlc for stdlib release tests: %s\n' "$terlc_bin"
  cargo build -q -p terlan --bin terlc
fi

if [[ ! -x "$terlc_bin" ]]; then
  printf 'terlc binary is missing or not executable after build: %s\n' "$terlc_bin" >&2
  exit 1
fi

test_rows="$(mktemp -t terlan-std-tests.XXXXXX)"
if [[ -z "${XDG_CACHE_HOME:-}" ]]; then
  release_cache_home="$(mktemp -d /tmp/terlan-std-release-cache.XXXXXX)"
  export XDG_CACHE_HOME="$release_cache_home"
fi
trap 'rm -f "$test_rows"; if [[ -n "$release_cache_home" ]]; then rm -rf "$release_cache_home"; fi' EXIT

awk -F '\t' '
  /^[[:space:]]*#/ || /^[[:space:]]*$/ {
    next
  }

  NF >= 3 {
    kind = ($1 ~ /^std\.js\..*\.generated_surface$/) ? "contract" : "test"
    print kind "\t" $2 "\t" $3
  }
' "$manifest" | sort -u > "$test_rows"

while IFS=$'\t' read -r row_kind test_file test_function; do
  if [[ ! -f "$test_file" ]]; then
    printf 'stdlib release test file is missing: %s\n' "$test_file" >&2
    failures=1
    continue
  fi

  if [[ "$row_kind" == contract ]]; then
    printf '[stdlib-release-contract] %s %s\n' "$test_file" "$test_function"
    continue
  fi

  if [[ -n "${executed_test_files[$test_file]:-}" ]]; then
    continue
  fi
  executed_test_files["$test_file"]=1

  printf '[stdlib-release-test] %s\n' "$test_file"
  status=0
  target_args_for_test "$test_file"
  timeout "${test_timeout_seconds}s" "$terlc_bin" test "$test_file" "${target_args[@]}" || status="$?"
  if [[ "$status" -eq 0 ]]; then
    continue
  fi

  if [[ "$status" -eq 124 ]]; then
    printf 'stdlib release test timed out after %ss: %s\n' \
      "$test_timeout_seconds" "$test_file" >&2
    failures=1
  else
    failures=1
  fi
done < "$test_rows"

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'stdlib release tests passed.\n'
