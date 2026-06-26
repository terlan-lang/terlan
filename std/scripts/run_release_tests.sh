#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - tests/std/RELEASE_API_TESTS.tsv, which identifies adjacent stdlib
#   release-test files.
# - The `terlan_cli` Cargo package providing the `terlc test` command.
# - TERLAN_STD_TEST_TIMEOUT_SECONDS, optionally overriding the per-file timeout.
#
# Output:
# - Exit status 0 when every unique release-test file passes through
#   `terlc test --target erlang`.
# - Exit status 1 with file-specific diagnostics when any release test fails
#   or exceeds the timeout.
#
# Transformation:
# - Derives the unique stdlib release-test file set from the manifest and
#   executes those Terlan test modules through the compiler's test command with
#   the owning target profile and a bounded runtime so a stuck test cannot hang
#   release automation.

manifest="tests/std/RELEASE_API_TESTS.tsv"
test_timeout_seconds="${TERLAN_STD_TEST_TIMEOUT_SECONDS:-30}"
failures=0

# Inputs:
# - $1: stdlib release-test file path.
#
# Output:
# - Prints command arguments that select the target and target profile for that
#   release-test file.
#
# Transformation:
# - Treats generated JavaScript standard-library tests as JavaScript browser
#   profile tests and keeps all other current stdlib tests on the Erlang
#   profile.
test_target_args() {
  case "$1" in
    std/js/*)
      printf '%s\n' '--target js --target-profile js.browser'
      ;;
    *)
      printf '%s\n' '--target erlang'
      ;;
  esac
}

if [[ ! -f "$manifest" ]]; then
  printf 'stdlib release API test manifest is missing: %s\n' "$manifest" >&2
  exit 1
fi

test_files="$(mktemp -t terlan-std-tests.XXXXXX)"
trap 'rm -f "$test_files"' EXIT

awk -F '\t' '
  /^[[:space:]]*#/ || /^[[:space:]]*$/ {
    next
  }

  NF >= 3 {
    print $2
  }
' "$manifest" | sort -u > "$test_files"

while IFS= read -r test_file; do
  if [[ ! -f "$test_file" ]]; then
    printf 'stdlib release test file is missing: %s\n' "$test_file" >&2
    failures=1
    continue
  fi

  printf '[stdlib-release-test] %s\n' "$test_file"
  status=0
  read -r -a target_args <<< "$(test_target_args "$test_file")"
  timeout "${test_timeout_seconds}s" cargo run -q -p terlan_cli -- test "$test_file" "${target_args[@]}" || status="$?"
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
done < "$test_files"

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'stdlib release tests passed.\n'
