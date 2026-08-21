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
  local output fingerprint temporary
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
  temporary="$(mktemp "$output.tmp.XXXXXX")"
  printf '%s\n' "$fingerprint" > "$temporary"
  mv "$temporary" "$output"
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
    'if [[ -n "${SLOW_BUILD_SECONDS:-}" ]]; then sleep "$SLOW_BUILD_SECONDS"; elif [[ "${SLOW_BUILD:-0}" == "1" ]]; then sleep 0.2; fi' \
    'if [[ "${FAIL_BUILD:-0}" == "1" ]]; then printf "partial\n" > "$3"; exit 9; fi' \
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
  printf 'mutated output\n' > "$output"
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
  if [[ "$(cat "$counter")" != "3" ]]; then
    echo "typed validator cache self-test did not reuse, detect mutation, and invalidate exactly once" >&2
    return 1
  fi
  printf 'module fixture.Failed.\n' > "$source"
  if FAIL_BUILD=1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
    echo "typed validator cache self-test accepted a failed build" >&2
    return 1
  fi
  if [[ -e "$output" || -e "$output.inputs.sha256" || -e "$output.partial" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test retained a failed build" >&2
    return 1
  fi
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  if [[ "$(cat "$counter")" != "5" ]]; then
    echo "typed validator cache self-test did not recover from a failed build" >&2
    return 1
  fi
  printf 'module fixture.Concurrent.\n' > "$source"
  SLOW_BUILD=1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output" &
  first_writer="$!"
  sleep 0.05
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output" &
  second_writer="$!"
  wait "$first_writer"
  wait "$second_writer"
  if [[ "$(cat "$counter")" != "6" ]]; then
    echo "typed validator cache self-test admitted concurrent equivalent builders" >&2
    return 1
  fi
  printf 'module fixture.Interrupted.\n' > "$source"
  SLOW_BUILD=1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output" &
  interrupted_writer="$!"
  attempts=0
  while [[ ! -e "$output.partial" && "$attempts" -lt 100 ]]; do
    sleep 0.01
    attempts="$((attempts + 1))"
  done
  if [[ ! -e "$output.partial" ]]; then
    echo "typed validator cache self-test could not observe an active writer" >&2
    return 1
  fi
  kill -TERM "$interrupted_writer"
  if wait "$interrupted_writer"; then
    echo "typed validator cache self-test reported an interrupted writer as successful" >&2
    return 1
  fi
  if [[ -e "$output" || -e "$output.inputs.sha256" || -e "$output.partial" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test retained an interrupted build" >&2
    return 1
  fi
  if [[ -e "$output.partial" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test left lifecycle state behind" >&2
    return 1
  fi
  printf 'module fixture.TimedOut.\n' > "$source"
  if TERLAN_TYPED_VALIDATOR_TIMEOUT_SECONDS=1 SLOW_BUILD_SECONDS=2 \
    "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
    echo "typed validator cache self-test accepted a timed-out build" >&2
    return 1
  fi
  if [[ -e "$output" || -e "$output.inputs.sha256" || -e "$output.partial" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test retained a timed-out build" >&2
    return 1
  fi
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
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
partial="$output.partial"
lock="$output.lock"
manifest="$(mktemp)"
lock_acquired=0
build_in_progress=0
build_pid=0

cleanup() {
  local status="$?"
  trap - EXIT INT TERM HUP
  if [[ "$build_pid" -gt 0 ]] && kill -0 "$build_pid" 2>/dev/null; then
    kill -TERM "$build_pid" 2>/dev/null || true
    wait "$build_pid" 2>/dev/null || true
  fi
  rm -f "$manifest"
  if [[ "$build_in_progress" -eq 1 ]]; then
    rm -f "$output" "$stamp" "$partial"
  fi
  if [[ "$lock_acquired" -eq 1 ]]; then
    rm -f "$lock/pid"
    rmdir "$lock" 2>/dev/null || true
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

run_bounded_build() {
  local timeout_seconds started status
  timeout_seconds="${TERLAN_TYPED_VALIDATOR_TIMEOUT_SECONDS:-1800}"
  if [[ ! "$timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "TERLAN_TYPED_VALIDATOR_TIMEOUT_SECONDS must be a positive integer" >&2
    return 2
  fi
  started="$SECONDS"
  "$@" &
  build_pid="$!"
  while kill -0 "$build_pid" 2>/dev/null; do
    if (( SECONDS - started >= timeout_seconds )); then
      echo "typed validator build timed out after ${timeout_seconds}s: $output" >&2
      kill -TERM "$build_pid" 2>/dev/null || true
      wait "$build_pid" 2>/dev/null || true
      build_pid=0
      return 124
    fi
    sleep 0.1
  done
  if wait "$build_pid"; then
    status=0
  else
    status="$?"
  fi
  build_pid=0
  return "$status"
}
{
  printf 'cache-schema=terlan-typed-validator-v2\n'
  printf 'cache-implementation=%s\n' "$(sha256_file "${BASH_SOURCE[0]}")"
  printf 'output=%s\n' "$output"
  printf 'command='
  printf '%q ' "$@"
  printf '\n'
  emit_input_hashes "${inputs[@]}"
} > "$manifest"
fingerprint="$(sha256_file "$manifest")"

cache_is_valid() {
  local sealed_input sealed_output
  [[ -s "$output" && -f "$stamp" ]] || return 1
  sealed_input="$(sed -n 's/^inputs=//p' "$stamp")"
  sealed_output="$(sed -n 's/^output-sha256=//p' "$stamp")"
  [[ "$sealed_input" == "$fingerprint" && -n "$sealed_output" ]] || return 1
  [[ "$(sha256_file "$output")" == "$sealed_output" ]]
}

if cache_is_valid; then
  echo "reusing typed validator: $output"
  exit 0
fi

mkdir -p "$(dirname "$output")"
lock_started="$SECONDS"
while ! mkdir "$lock" 2>/dev/null; do
  if [[ -f "$lock/pid" ]]; then
    lock_pid="$(cat "$lock/pid" 2>/dev/null || true)"
    if [[ "$lock_pid" =~ ^[0-9]+$ ]] && ! kill -0 "$lock_pid" 2>/dev/null; then
      rm -f "$lock/pid"
      rmdir "$lock" 2>/dev/null || true
      continue
    fi
  fi
  if (( SECONDS - lock_started >= 300 )); then
    echo "timed out waiting for typed validator cache writer: $output" >&2
    exit 1
  fi
  sleep 0.1
done
lock_acquired=1
printf '%s\n' "$$" > "$lock/pid"

# Another writer may have completed while this process waited for the lock.
if cache_is_valid; then
  echo "reusing typed validator after writer handoff: $output"
  exit 0
fi

rm -f "$output" "$stamp"
printf 'pid=%s\ninputs=%s\n' "$$" "$fingerprint" > "$partial"
build_in_progress=1
run_bounded_build "$@"
if [[ ! -s "$output" ]]; then
  echo "typed validator build did not create $output" >&2
  exit 1
fi
output_hash="$(sha256_file "$output")"
stamp_tmp="$(mktemp "$stamp.tmp.XXXXXX")"
printf 'inputs=%s\noutput-sha256=%s\n' "$fingerprint" "$output_hash" > "$stamp_tmp"
mv "$stamp_tmp" "$stamp"
rm -f "$partial"
build_in_progress=0
echo "built and sealed typed validator: $output"
