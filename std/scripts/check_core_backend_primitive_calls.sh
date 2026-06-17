#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - std/core/**/*.terl source files.
# - std/core/BACKEND_PRIMITIVE_CALLS.tsv, the reviewed shrink-only
#   inventory of backend primitive calls still present in portable std.core.
#
# Output:
# - Exit status 0 when actual backend primitive calls match the reviewed
#   inventory exactly.
# - Exit status 1 with a diff when a new backend primitive call appears or a
#   stale inventory row remains after cleanup.
#
# Transformation:
# - Scans non-comment Terlan source lines, tracks the enclosing public function,
#   extracts calls to known backend primitive modules, and compares the actual
#   inventory with the reviewed manifest.

manifest="std/core/BACKEND_PRIMITIVE_CALLS.tsv"

if [[ ! -f "$manifest" ]]; then
  printf 'std.core backend primitive inventory is missing: %s\n' "$manifest" >&2
  exit 1
fi

actual_file="$(mktemp -t terlan-std-core-backend-actual.XXXXXX)"
expected_file="$(mktemp -t terlan-std-core-backend-expected.XXXXXX)"
trap 'rm -f "$actual_file" "$expected_file"' EXIT

# Inputs:
# - $1: Terlan source file path.
#
# Output:
# - Tab-separated rows: source_file, public_function, backend_call.
#
# Transformation:
# - Maintains the latest public function name and extracts backend primitive
#   calls from executable source lines while ignoring comments and docs.
scan_source_file() {
  local source_file="$1"

  awk -v source_file="$source_file" '
    /^[[:space:]]*(\/\/|\/\/!|\/\/\/)/ {
      next
    }

    /^[[:space:]]*pub[[:space:]]+[a-z_][A-Za-z0-9_]*[[:space:]]*\(/ {
      line = $0
      sub(/^[[:space:]]*pub[[:space:]]+/, "", line)
      sub(/[[:space:]]*\(.*/, "", line)
      current_function = line
    }

    {
      line = $0
      while (match(line, /(^|[^A-Za-z0-9_])(erlang|string|lists|unicode|binary|math|maps)\.[a-z_][A-Za-z0-9_]*[[:space:]]*\(/)) {
        call = substr(line, RSTART, RLENGTH)
        sub(/^[^A-Za-z0-9_]/, "", call)
        sub(/[[:space:]]*\($/, "", call)
        if (current_function == "") {
          current_function = "<module>"
        }
        print source_file "\t" current_function "\t" call
        line = substr(line, RSTART + RLENGTH)
      }
    }
  ' "$source_file"
}

while IFS= read -r source_file; do
  scan_source_file "$source_file"
done < <(find std/core -type f -name '*.terl' | sort) | sort > "$actual_file"

awk -F '\t' '
  /^[[:space:]]*#/ || /^[[:space:]]*$/ {
    next
  }

  NF != 3 {
    printf "%s:%d: malformed backend primitive inventory row\n", FILENAME, FNR > "/dev/stderr"
    malformed = 1
    next
  }

  {
    print $1 "\t" $2 "\t" $3
  }

  END {
    exit malformed ? 1 : 0
  }
' "$manifest" | sort > "$expected_file"

if ! diff -u "$expected_file" "$actual_file"; then
  printf 'std.core backend primitive call inventory is not current.\n' >&2
  printf 'Add no new backend primitive calls; replace existing rows with CoreIR/backend intrinsics.\n' >&2
  exit 1
fi

printf 'std.core backend primitive call inventory is current.\n'
