#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - tests/std/RELEASE_API_TESTS.tsv, which identifies the stdlib
#   release-test files.
# - The `terlan_cli` Cargo package providing the `terlc test` command.
#
# Output:
# - Exit status 0 when every unique release-test file passes through
#   `terlc test --target erlang`.
# - Exit status 1 with file-specific diagnostics when any release test fails.
#
# Transformation:
# - Derives the unique stdlib release-test file set from the manifest and
#   executes those Terlan test modules through the compiler's test command.

manifest="tests/std/RELEASE_API_TESTS.tsv"
failures=0

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
  if ! cargo run -q -p terlan_cli -- test "$test_file" --target erlang; then
    failures=1
  fi
done < "$test_files"

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'stdlib release tests passed.\n'
