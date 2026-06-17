#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - tests/std/NEGATIVE_API_TESTS.tsv, which identifies constrained
#   public stdlib APIs, invalid fixtures, and expected diagnostics.
# - The `terlan_cli` Cargo package providing `terlc test`.
#
# Output:
# - Exit status 0 when every invalid stdlib fixture fails with its expected
#   diagnostic substring and does not report a successful test summary.
# - Exit status 1 with fixture-specific diagnostics when a fixture is missing,
#   malformed, succeeds unexpectedly, or fails with the wrong diagnostic.
#
# Transformation:
# - Reads manifest rows, runs each invalid Terlan source file through
#   `terlc test --target erlang`, captures combined compiler output, and checks
#   that constrained API misuse remains rejected.

manifest="tests/std/NEGATIVE_API_TESTS.tsv"
failures=0

if [[ ! -f "$manifest" ]]; then
  printf 'stdlib negative API test manifest is missing: %s\n' "$manifest" >&2
  exit 1
fi

while IFS=$'\t' read -r api_id fixture expected extra; do
  [[ -z "${api_id:-}" || "${api_id:0:1}" == "#" ]] && continue

  if [[ -n "${extra:-}" || -z "${fixture:-}" || -z "${expected:-}" ]]; then
    printf '%s: malformed manifest row for API `%s`\n' "$manifest" "$api_id" >&2
    failures=1
    continue
  fi

  if [[ "$fixture" != tests/std/negative/*/*_test.terl && "$fixture" != tests/std/negative/*/*/*_test.terl ]]; then
    printf '%s: API `%s` negative fixture path must be tests/std/negative/<feature>/*_test.terl, got `%s`\n' \
      "$manifest" "$api_id" "$fixture" >&2
    failures=1
    continue
  fi

  if [[ ! -f "$fixture" ]]; then
    printf '%s: API `%s` references missing negative fixture `%s`\n' \
      "$manifest" "$api_id" "$fixture" >&2
    failures=1
    continue
  fi

  printf '[stdlib-negative-api-test] %s\n' "$fixture"
  output_file="$(mktemp -t terlan-std-negative-api.XXXXXX)"
  if cargo run -q -p terlan_cli -- test "$fixture" --target erlang >"$output_file" 2>&1; then
    printf '%s: expected terlc test to fail for API `%s`\n' "$fixture" "$api_id" >&2
    cat "$output_file" >&2
    failures=1
  elif ! grep -Fq "$expected" "$output_file"; then
    printf '%s: expected diagnostic substring not found for API `%s`: %s\n' \
      "$fixture" "$api_id" "$expected" >&2
    cat "$output_file" >&2
    failures=1
  elif grep -Fq 'test result: ok.' "$output_file"; then
    printf '%s: invalid stdlib fixture reported successful test execution for API `%s`\n' \
      "$fixture" "$api_id" >&2
    cat "$output_file" >&2
    failures=1
  fi
  rm -f "$output_file"
done < "$manifest"

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'stdlib negative API diagnostics are stable.\n'
