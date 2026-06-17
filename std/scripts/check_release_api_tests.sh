#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - tests/std/RELEASE_API_TESTS.tsv, a tab-separated release API manifest.
# - Terlan stdlib release tests under tests/std.
#
# Output:
# - Exit status 0 when every listed release API has an @test-annotated Terlan
#   test function.
# - Exit status 1 with API-specific diagnostics when a manifest entry is
#   missing, malformed, or not backed by an @test declaration.
#
# Transformation:
# - Reads the release API manifest, validates test-file paths, and checks that
#   each expected public test function is immediately introduced by @test.

manifest="tests/std/RELEASE_API_TESTS.tsv"
failures=0

if [[ ! -f "$manifest" ]]; then
  printf 'stdlib release API test manifest is missing: %s\n' "$manifest" >&2
  exit 1
fi

# Inputs:
# - $1: Terlan test file path.
# - $2: Public zero-argument test function name.
#
# Output:
# - Exit status 0 when the file contains `@test` followed by `pub name(`.
# - Exit status 1 when the annotated function is absent.
#
# Transformation:
# - Performs a small line-oriented scan over Terlan source text. Blank lines
#   after @test are tolerated; any non-blank non-target declaration clears the
#   pending annotation.
has_annotated_test_function() {
  local path="$1"
  local function_name="$2"

  awk -v function_name="$function_name" '
    /^[[:space:]]*@test[[:space:]]*$/ {
      pending_test = 1
      next
    }

    /^[[:space:]]*$/ {
      next
    }

    pending_test && $0 ~ "^[[:space:]]*pub[[:space:]]+" function_name "\\(" {
      found = 1
      exit
    }

    pending_test {
      pending_test = 0
    }

    END {
      exit found ? 0 : 1
    }
  ' "$path"
}

while IFS=$'\t' read -r api_id test_file test_function extra; do
  [[ -z "${api_id:-}" || "${api_id:0:1}" == "#" ]] && continue

  if [[ -n "${extra:-}" || -z "${test_file:-}" || -z "${test_function:-}" ]]; then
    printf '%s: malformed manifest row for API `%s`\n' "$manifest" "$api_id" >&2
    failures=1
    continue
  fi

  if [[ "$test_file" != tests/std/*/*_test.terl && "$test_file" != tests/std/*/*/*_test.terl ]]; then
    printf '%s: API `%s` test path must be tests/std/<feature>/*_test.terl, got `%s`\n' \
      "$manifest" "$api_id" "$test_file" >&2
    failures=1
    continue
  fi

  if [[ "$test_file" == tests/std/negative/* ]]; then
    printf '%s: API `%s` positive test path must not live under tests/std/negative, got `%s`\n' \
      "$manifest" "$api_id" "$test_file" >&2
    failures=1
    continue
  fi

  if [[ ! -f "$test_file" ]]; then
    printf '%s: API `%s` references missing test file `%s`\n' \
      "$manifest" "$api_id" "$test_file" >&2
    failures=1
    continue
  fi

  if ! has_annotated_test_function "$test_file" "$test_function"; then
    printf '%s: API `%s` is missing @test function `%s` in `%s`\n' \
      "$manifest" "$api_id" "$test_function" "$test_file" >&2
    failures=1
  fi
done < "$manifest"

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'stdlib release API tests are complete.\n'
