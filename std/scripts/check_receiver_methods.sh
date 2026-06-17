#!/usr/bin/env bash
set -euo pipefail

# Inputs:
# - std/core/string.terl, the canonical source module for the Terlan String
#   primitive API.
#
# Output:
# - Exit status 0 when receiver-shaped public String operations are declared as
#   receiver methods.
# - Exit status 1 with line-specific diagnostics when a public String operation
#   is declared as a module function whose first parameter is `String`.
#
# Transformation:
# - Scans the String module source for public module-function declarations with
#   a first parameter annotated as `String`. `concat(values: List[String])`
#   remains valid because it has no String receiver; receiver-shaped operations
#   must use `pub (value: String) method(...)`.

source_file="std/core/string.terl"
failures=0

if [[ ! -f "$source_file" ]]; then
  printf 'std.core.String source is missing: %s\n' "$source_file" >&2
  exit 1
fi

# Checks one source line for a forbidden receiver-shaped module function.
#
# Inputs:
# - $1: one-based source line number.
# - $2: source line text.
#
# Output:
# - Prints a diagnostic and returns 1 when the line declares a public module
#   function whose first argument is typed as `String`.
# - Returns 0 when the line is not a forbidden declaration.
#
# Transformation:
# - Matches compact Terlan function headers of the form
#   `pub name(first: String, ...): Return ->`. Receiver-method declarations
#   start with `pub (` and are intentionally excluded by the pattern.
check_line() {
  local line_no="$1"
  local line="$2"

  local pattern='^[[:space:]]*pub[[:space:]]+[a-z_][a-zA-Z0-9_]*[[:space:]]*\([[:space:]]*[a-z_][a-zA-Z0-9_]*[[:space:]]*:[[:space:]]*String[[:space:]]*[,)].*$'

  if grep -Eq "$pattern" <<< "$line"; then
    printf '%s:%s: public String operation must be a receiver method: %s\n' \
      "$source_file" "$line_no" "$line" >&2
    return 1
  fi

  return 0
}

line_no=0
while IFS= read -r line || [[ -n "$line" ]]; do
  line_no=$((line_no + 1))
  if ! check_line "$line_no" "$line"; then
    failures=1
  fi
done < "$source_file"

if [[ "$failures" -ne 0 ]]; then
  printf 'std.core receiver-method gate failed.\n' >&2
  exit 1
fi

printf 'std.core receiver-method declarations are enforced.\n'
