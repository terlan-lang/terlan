#!/usr/bin/env bash
set -euo pipefail

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{ print $1 }'
  else
    echo "typed validator cache requires sha256sum or shasum" >&2
    exit 127
  fi
}

hash_tree() {
  local root="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    find "$root" \
      \( -path '*/.terlan' -o -path '*/target' \) -prune -o \
      -type f -exec sha256sum {} + | LC_ALL=C sort -k2
  elif command -v shasum >/dev/null 2>&1; then
    find "$root" \
      \( -path '*/.terlan' -o -path '*/target' \) -prune -o \
      -type f -exec shasum -a 256 {} + | LC_ALL=C sort -k2
  else
    echo "typed validator cache requires sha256sum or shasum" >&2
    exit 127
  fi
}

emit_input_hashes() {
  local input
  for input in "$@"; do
    if [[ -f "$input" ]]; then
      printf '%s  %s\n' "$(sha256_file "$input")" "$input"
    elif [[ -d "$input" ]]; then
      hash_tree "$input"
    else
      echo "typed validator cache input is missing: $input" >&2
      return 1
    fi
  done
}

common_fingerprint() {
  local manifest
  manifest="$(mktemp)"
  trap 'rm -f "$manifest"' RETURN
  {
    printf 'cache-schema=terlan-typed-validator-common-v1\n'
    printf 'cache-implementation=%s\n' "$(sha256_file "${BASH_SOURCE[0]}")"
    emit_input_hashes "$@"
  } > "$manifest"
  sha256_file "$manifest"
}

write_common_fingerprint() {
  local output fingerprint
  output="${1:-}"
  shift || true
  if [[ -z "$output" || "$#" -eq 0 ]]; then
    echo "usage: scripts/build_typed_validator.sh fingerprint <output> <input>..." >&2
    return 2
  fi
  fingerprint="$(common_fingerprint "$@")"
  if [[ -f "$output" && "$(cat "$output")" == "$fingerprint" ]]; then
    echo "reusing typed validator common fingerprint: $output"
    return 0
  fi
  mkdir -p "$(dirname "$output")"
  printf '%s\n' "$fingerprint" > "$output"
  echo "refreshed typed validator common fingerprint: $output"
}

check_common_fingerprint() {
  local output expected actual
  output="${1:-}"
  shift || true
  if [[ -z "$output" || "$#" -eq 0 ]]; then
    echo "usage: scripts/build_typed_validator.sh fingerprint-check <output> <input>..." >&2
    return 2
  fi
  if [[ ! -f "$output" ]]; then
    echo "typed validator common fingerprint is missing: $output" >&2
    return 1
  fi
  expected="$(cat "$output")"
  actual="$(common_fingerprint "$@")"
  if [[ "$actual" != "$expected" ]]; then
    echo "typed validator common fingerprint is stale: $output" >&2
    return 1
  fi
  echo "verified typed validator common fingerprint: $output"
}

run_self_test() {
  local fixture source output counter builder common first_common second_common
  fixture="$(mktemp -d)"
  trap 'rm -rf "$fixture"' RETURN
  source="$fixture/Input.terl"
  output="$fixture/output.tvm"
  counter="$fixture/count"
  builder="$fixture/build.sh"
  common="$fixture/common.sha256"
  printf 'module fixture.Input.\n' > "$source"
  printf '0\n' > "$counter"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'count=$(cat "$1")' \
    'printf "%s\n" "$((count + 1))" > "$1"' \
    'cp "$2" "$3"' > "$builder"
  chmod +x "$builder"

  "$0" fingerprint "$common" "$source"
  first_common="$(cat "$common")"
  "$0" fingerprint "$common" "$source"
  second_common="$(cat "$common")"
  if [[ "$first_common" != "$second_common" ]]; then
    echo "typed validator common fingerprint changed without an input change" >&2
    return 1
  fi
  "$0" fingerprint-check "$common" "$source"

  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  printf 'module fixture.Changed.\n' > "$source"
  if "$0" fingerprint-check "$common" "$source" >/dev/null 2>&1; then
    echo "typed validator fingerprint check accepted changed input" >&2
    return 1
  fi
  "$0" fingerprint "$common" "$source"
  if [[ "$(cat "$common")" == "$first_common" ]]; then
    echo "typed validator common fingerprint ignored an input change" >&2
    return 1
  fi
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  if [[ "$(cat "$counter")" != "2" ]]; then
    echo "typed validator cache self-test did not reuse and invalidate exactly once" >&2
    return 1
  fi
  echo "typed validator cache self-test passed"
}

if [[ "${1:-}" == "self-test" ]]; then
  run_self_test
  exit
fi
if [[ "${1:-}" == "fingerprint" ]]; then
  shift
  write_common_fingerprint "$@"
  exit
fi
if [[ "${1:-}" == "fingerprint-check" ]]; then
  shift
  check_common_fingerprint "$@"
  exit
fi

output="${1:-}"
if [[ -z "$output" ]]; then
  echo "usage: scripts/build_typed_validator.sh <output> <input>... -- <build-command>..." >&2
  exit 2
fi
shift

inputs=()
while [[ "$#" -gt 0 && "$1" != "--" ]]; do
  inputs+=("$1")
  shift
done
if [[ "$#" -eq 0 || "${#inputs[@]}" -eq 0 ]]; then
  echo "typed validator cache requires inputs and a build command" >&2
  exit 2
fi
shift
if [[ "$#" -eq 0 ]]; then
  echo "typed validator cache requires a build command" >&2
  exit 2
fi

stamp="$output.inputs.sha256"
manifest="$(mktemp)"
trap 'rm -f "$manifest"' EXIT
{
  printf 'cache-schema=terlan-typed-validator-v1\n'
  printf 'cache-implementation=%s\n' "$(sha256_file "${BASH_SOURCE[0]}")"
  printf 'output=%s\n' "$output"
  printf 'command='
  printf '%q ' "$@"
  printf '\n'
  emit_input_hashes "${inputs[@]}"
} > "$manifest"
fingerprint="$(sha256_file "$manifest")"

if [[ -s "$output" && -f "$stamp" && "$(cat "$stamp")" == "$fingerprint" ]]; then
  echo "reusing typed validator: $output"
  exit 0
fi

mkdir -p "$(dirname "$output")"
rm -f "$stamp"
"$@"
if [[ ! -s "$output" ]]; then
  echo "typed validator build did not create $output" >&2
  exit 1
fi
printf '%s\n' "$fingerprint" > "$stamp"
