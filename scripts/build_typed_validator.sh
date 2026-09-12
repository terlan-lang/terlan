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
    printf 'compiler-toolchain=%s\n' "${TERLAN_TYPED_VALIDATOR_TOOLCHAIN_IDENTITY:-}"
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
  local fixture source output counter builder common first_common second_common prior_output prior_stamp
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
    'if [[ "${IGNORE_TERM:-0}" == "1" ]]; then trap "" TERM; while :; do :; done; fi' \
    'if [[ -n "${SLOW_BUILD_SECONDS:-}" ]]; then sleep "$SLOW_BUILD_SECONDS"; elif [[ "${SLOW_BUILD:-0}" == "1" ]]; then sleep 0.2; fi' \
    'if [[ "${FAIL_BUILD:-0}" == "1" ]]; then printf "partial\n" > "$3"; exit 9; fi' \
    'if [[ "${MUTATE_INPUT:-0}" == "1" ]]; then printf "changed during build\n" >> "$2"; fi' \
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
  TERLAN_TYPED_VALIDATOR_TOOLCHAIN_IDENTITY=fixture-toolchain-one "$0" fingerprint "$common" "$source"
  first_common="$(cat "$common")"
  TERLAN_TYPED_VALIDATOR_TOOLCHAIN_IDENTITY=fixture-toolchain-two "$0" fingerprint "$common" "$source"
  if [[ "$(cat "$common")" == "$first_common" ]]; then
    echo "typed validator common fingerprint ignored compiler toolchain identity" >&2
    return 1
  fi
  "$0" fingerprint "$common" "$source"
  first_common="$(cat "$common")"

  if [[ "$(uname -s)" == Linux ]]; then
    local test_lease_fd lease_wait_status
    exec {test_lease_fd}>>"$output.writer-lease"
    flock --exclusive "$test_lease_fd"
    lease_wait_status=0
    timeout 1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output" \
      || lease_wait_status="$?"
    exec {test_lease_fd}>&-
    if [[ "$lease_wait_status" != 124 || "$(cat "$counter")" != 0 || -e "$output.partial" ]]; then
      echo "typed validator cache self-test admitted a producer while its kernel lease was held" >&2
      return 1
    fi
    echo "typed validator live kernel lease excludes another writer"
  fi

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
  prior_output="$(sha256_file "$output")"
  prior_stamp="$(sha256_file "$output.inputs.sha256")"
  printf 'module fixture.Failed.\n' > "$source"
  if FAIL_BUILD=1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
    echo "typed validator cache self-test accepted a failed build" >&2
    return 1
  fi
  if [[ "$(sha256_file "$output")" != "$prior_output" || "$(sha256_file "$output.inputs.sha256")" != "$prior_stamp" || -e "$output.partial" || -e "$output.work" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test lost prior evidence after a failed build" >&2
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
  prior_output="$(sha256_file "$output")"
  prior_stamp="$(sha256_file "$output.inputs.sha256")"
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
  if [[ "$(sha256_file "$output")" != "$prior_output" || "$(sha256_file "$output.inputs.sha256")" != "$prior_stamp" || -e "$output.partial" || -e "$output.work" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test lost prior evidence after interruption" >&2
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
  if [[ "$(sha256_file "$output")" != "$prior_output" || "$(sha256_file "$output.inputs.sha256")" != "$prior_stamp" || -e "$output.partial" || -e "$output.work" || -e "$output.lock" ]]; then
    echo "typed validator cache self-test lost prior evidence after timeout" >&2
    return 1
  fi
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  prior_output="$(sha256_file "$output")"
  prior_stamp="$(sha256_file "$output.inputs.sha256")"
  printf 'module fixture.ConcurrentMutation.\n' > "$source"
  if MUTATE_INPUT=1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
    echo "typed validator cache self-test sealed inputs changed during compilation" >&2
    return 1
  fi
  if [[ "$(sha256_file "$output")" != "$prior_output" || "$(sha256_file "$output.inputs.sha256")" != "$prior_stamp" || -e "$output.work" ]]; then
    echo "typed validator cache self-test lost prior evidence after input mutation" >&2
    return 1
  fi
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  prior_output="$(sha256_file "$output")"
  prior_stamp="$(sha256_file "$output.inputs.sha256")"
  printf 'module fixture.Uncooperative.\n' > "$source"
  if TERLAN_TYPED_VALIDATOR_TIMEOUT_SECONDS=1 IGNORE_TERM=1 \
    "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
    echo "typed validator cache self-test accepted a TERM-ignoring builder" >&2
    return 1
  fi
  if [[ "$(sha256_file "$output")" != "$prior_output" || "$(sha256_file "$output.inputs.sha256")" != "$prior_stamp" || -e "$output.work" ]]; then
    echo "typed validator cache self-test lost prior evidence after forced termination" >&2
    return 1
  fi
  "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
  # Fault injection lives entirely in this disposable PATH shim. The production
  # cache has no testing branch, signal hook, or skip-validation environment flag.
  local real_mv phase before_calls destination
  real_mv="$(command -v mv)"
  mkdir "$fixture/bin"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    '"$REAL_MV" "$@"' \
    'destination="${!#}"' \
    'if [[ -f "$INJECT_MARKER" && "$destination" == "$(cat "$INJECT_MARKER")" ]]; then' \
    '  rm "$INJECT_MARKER"' \
    '  kill -KILL "$PPID"' \
    'fi' > "$fixture/bin/mv"
  chmod +x "$fixture/bin/mv"
  for phase in image seal corrupt-image corrupt-seal foreign-journal; do
    prior_output="$(sha256_file "$output")"
    prior_stamp="$(sha256_file "$output.inputs.sha256")"
    printf 'module fixture.Publication_%s.\n' "$phase" > "$source"
    before_calls="$(cat "$counter")"
    destination="$output"
    if [[ "$phase" == seal ]]; then destination="$output.inputs.sha256"; fi
    printf '%s\n' "$destination" > "$fixture/inject"
    if PATH="$fixture/bin:$PATH" REAL_MV="$real_mv" INJECT_MARKER="$fixture/inject" \
      "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
      echo "typed validator self-test failed to interrupt $phase publication" >&2
      return 1
    fi
    if [[ ! -f "$output.work/journal" || ! -f "$output.partial" ]]; then
      echo "typed validator self-test lost interrupted publication state" >&2
      return 1
    fi
    if [[ "$phase" == corrupt-seal || "$phase" == foreign-journal ]]; then
      cp "$output.work/seal" "$fixture/saved-seal"
      cp "$output.work/journal" "$fixture/saved-journal"
      if [[ "$phase" == corrupt-seal ]]; then
        printf 'damaged seal\n' >> "$output.work/seal"
      else
        printf 'owner=another-image.tvm\n' > "$output.work/journal"
      fi
      if "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
        echo "typed validator self-test admitted invalid publication metadata" >&2
        return 1
      fi
      if [[ "$(cat "$counter")" != "$((before_calls + 1))" || ! -f "$output.work/previous.image" || ! -f "$output.work/journal" ]]; then
        echo "typed validator self-test replayed work or deleted recovery evidence after invalid metadata" >&2
        return 1
      fi
      cp "$fixture/saved-seal" "$output.work/seal"
      cp "$fixture/saved-journal" "$output.work/journal"
    fi
    if [[ "$phase" == corrupt-image ]]; then
      printf 'damaged published image\n' > "$output"
      if FAIL_BUILD=1 "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"; then
        echo "typed validator self-test accepted a failed rebuild after rollback" >&2
        return 1
      fi
      if [[ "$(sha256_file "$output")" != "$prior_output" || "$(sha256_file "$output.inputs.sha256")" != "$prior_stamp" ]]; then
        echo "typed validator self-test failed to restore the previous sealed pair" >&2
        return 1
      fi
      before_calls=$((before_calls + 2))
    fi
    "$0" "$output" "$source" -- "$builder" "$counter" "$source" "$output"
    if [[ "$(cat "$counter")" != "$((before_calls + 1))" || -e "$output.partial" || -e "$output.work" ]]; then
      echo "typed validator self-test replayed a completed build or left publication residue" >&2
      return 1
    fi
    echo "typed validator publication recovery passed: $phase"
  done
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
if [[ -z "$output" || "$output" != *.tvm ]]; then
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
workspace="$output.work"
manifest="$(mktemp)"
lock_acquired=0
build_in_progress=0
build_pid=0

# Recovery runs under the image's writer lock. The journal is published only
# after both prior files have been copied and their hashes recorded.
recover_publication() {
  local expected previous_image previous_seal actual pending_input pending_seal entry
  if [[ -L "$workspace" || ( -e "$workspace" && ! -d "$workspace" ) ]]; then
    echo "typed validator staging path is not an owned directory: $workspace" >&2
    return 1
  fi
  if [[ -f "$workspace/journal" ]]; then
    [[ "$(sed -n 's/^owner=//p' "$workspace/journal")" == "$output" ]] || return 1
    pending_seal="$(sed -n 's/^pending-seal=//p' "$workspace/journal")"
    [[ "$pending_seal" =~ ^[0-9a-f]{64}$ ]] || return 1
    [[ "$(sha256_file "$workspace/seal")" == "$pending_seal" ]] || return 1
    pending_input="$(sed -n 's/^inputs=//p' "$workspace/seal")"
    [[ "$pending_input" =~ ^[0-9a-f]{64}$ ]] || return 1
    [[ "$(sed -n 's/^inputs=//p' "$partial")" == "$pending_input" ]] || return 1
    expected="$(sed -n 's/^output-sha256=//p' "$workspace/seal")" || return 1
    [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || return 1
    actual="$(sha256_file "$output" 2>/dev/null || true)"
    if [[ "$actual" == "$expected" ]]; then
      # The expensive build finished and its image is intact. Finish sealing
      # that generation; current inputs are checked separately before reuse.
      cp "$workspace/seal" "$workspace/seal.publish" || return 1
      mv "$workspace/seal.publish" "$stamp" || return 1
    else
      previous_image="$(sed -n 's/^image=//p' "$workspace/journal")"
      previous_seal="$(sed -n 's/^seal=//p' "$workspace/journal")"
      for entry in image seal; do
        if [[ "$entry" == image ]]; then expected="$previous_image"; else expected="$previous_seal"; fi
        if [[ "$expected" != absent ]]; then
          [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || return 1
          [[ "$(sha256_file "$workspace/previous.$entry")" == "$expected" ]] || return 1
        fi
      done
      # Validate both backups before replacing either file. Copies leave the
      # journal retryable if recovery itself is interrupted or runs out of disk.
      if [[ "$previous_image" == absent ]]; then
        rm -f "$output" || return 1
      else
        cp -p "$workspace/previous.image" "$workspace/restore.image" || return 1
        mv "$workspace/restore.image" "$output" || return 1
      fi
      if [[ "$previous_seal" == absent ]]; then
        rm -f "$stamp" || return 1
      else
        cp -p "$workspace/previous.seal" "$workspace/restore.seal" || return 1
        mv "$workspace/restore.seal" "$stamp" || return 1
      fi
    fi
  fi
  # Once publication/recovery has completed, losing cleanup state is harmless.
  # The remaining image/seal pair is always rehashed before any cache hit.
  rm -f "$workspace/journal" || return 1
  if [[ -d "$workspace" ]]; then rm -rf -- "$workspace" || return 1; fi
  rm -f "$partial"
}

prepare_publication() {
  local previous_image=absent previous_seal=absent pending_seal
  if [[ -e "$output" ]]; then
    cp -p "$output" "$workspace/previous.image"
    previous_image="$(sha256_file "$workspace/previous.image")"
  fi
  if [[ -e "$stamp" ]]; then
    cp -p "$stamp" "$workspace/previous.seal"
    previous_seal="$(sha256_file "$workspace/previous.seal")"
  fi
  pending_seal="$(sha256_file "$workspace/seal")"
  printf 'owner=%s\nimage=%s\nseal=%s\npending-seal=%s\n' "$output" "$previous_image" "$previous_seal" "$pending_seal" > "$workspace/journal.pending"
  mv "$workspace/journal.pending" "$workspace/journal"
}

terminate_build() {
  local deadline
  if [[ "$build_pid" -gt 0 ]]; then
    kill -TERM "$build_pid" 2>/dev/null || true
    # Let the owner finish bounded activity-log writes and reap its producer.
    # Killing it after two seconds could interrupt the five-second log lock wait.
    deadline=$((SECONDS + 15))
    while kill -0 "$build_pid" 2>/dev/null; do
      if (( SECONDS >= deadline )); then
        kill -KILL "$build_pid" 2>/dev/null || true
        break
      fi
      sleep 0.05
    done
    wait "$build_pid" 2>/dev/null || true
    build_pid=0
  fi
}

cleanup() {
  local status="$?"
  trap - EXIT INT TERM HUP
  if [[ "$build_pid" -gt 0 ]] && kill -0 "$build_pid" 2>/dev/null; then
    terminate_build
  fi
  rm -f "$manifest"
  if [[ "$build_in_progress" -eq 1 ]]; then
    if ! recover_publication; then
      echo "typed validator publication recovery is incomplete; journal retained: $workspace" >&2
      status=1
    fi
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

process_owner_path() {
  local root owner
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || return
  owner="$root/target/validation-tools/terlan-test-orchestrator"
  # Snapshot installation adds the executable suffix on Windows.
  if [[ ! -f "$owner" && -f "$owner.exe" ]]; then owner="$owner.exe"; fi
  printf '%s\n' "$owner"
}

run_bounded_build() {
  local timeout_seconds status owner
  timeout_seconds="${TERLAN_TYPED_VALIDATOR_TIMEOUT_SECONDS:-1800}"
  if [[ ! "$timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "TERLAN_TYPED_VALIDATOR_TIMEOUT_SECONDS must be a positive integer" >&2
    return 2
  fi
  owner="$(process_owner_path)"
  if [[ ! -x "$owner" ]]; then
    echo "typed validator build requires the prebuilt process owner: $owner; run make terlan-compiler-bootstrap" >&2
    return 127
  fi
  "$owner" --run-owned --timeout-seconds "$timeout_seconds" -- "$@" </dev/null &
  build_pid="$!"
  if wait "$build_pid"; then
    status=0
  else
    status="$?"
  fi
  build_pid=0
  return "$status"
}
build_command=("$@")
refresh_fingerprint() {
  local owner owner_hash
  owner="$(process_owner_path)" || return
  if [[ ! -x "$owner" ]]; then
    echo "typed validator cache requires the prebuilt process owner: $owner; run make terlan-compiler-bootstrap" >&2
    return 127
  fi
  owner_hash="$(sha256_file "$owner")" || return
{
  printf 'cache-schema=terlan-typed-validator-v4\n'
  printf 'cache-implementation=%s\n' "$(sha256_file "${BASH_SOURCE[0]}")"
  printf 'process-owner=%s\n' "$owner_hash"
  printf 'output=%s\n' "$output"
  printf 'command='
  printf '%q ' "${build_command[@]}"
  printf '\n'
  emit_input_hashes "${inputs[@]}"
} > "$manifest"
fingerprint="$(sha256_file "$manifest")"
}
refresh_fingerprint

cache_is_valid() {
  local sealed_input sealed_output
  [[ ! -e "$partial" && ! -e "$workspace" && -s "$output" && -f "$stamp" ]] || return 1
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
if [[ "$(uname -s)" == Linux ]]; then
  # A PID is not an ownership token across containers (or PID reuse). Keep one
  # stable inode and inherit its kernel lease into the producer: a killed shell
  # cannot release another live descendant's ownership. Never unlink the lease.
  lease="$output.writer-lease"
  if [[ -L "$lease" || ( -e "$lease" && ! -f "$lease" ) ]]; then
    echo "typed validator writer lease is not a regular file: $lease" >&2
    exit 1
  fi
  exec {writer_lease_fd}>>"$lease"
  flock --exclusive --timeout 300 "$writer_lease_fd"
  if [[ -e "$lock" || -L "$lock" ]]; then
    echo "legacy typed validator writer lock requires verified recovery: $lock" >&2
    exit 1
  fi
else
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
fi

# Another writer may have completed while this process waited for the lock.
recover_publication
refresh_fingerprint
if cache_is_valid; then
  echo "reusing typed validator after writer handoff: $output"
  exit 0
fi

# Only the declared image is published. Compiler side products remain private;
# they are not inputs to validator execution or reusable cache authority.
mkdir "$workspace"
printf 'pid=%s\ninputs=%s\n' "$$" "$fingerprint" > "$partial"
build_in_progress=1
staged_output=""
staged_command=("${build_command[@]}")
for (( index=0; index<${#staged_command[@]}; index++ )); do
  if [[ "${staged_command[index]}" == --out-dir ]]; then
    (( index += 1 ))
    destination="${staged_command[index]:-}"
    if [[ -z "$destination" || "$output" != "${destination%/}/"* ]]; then
      echo 'typed validator output must belong to its explicit --out-dir' >&2
      exit 2
    fi
    staged_command[index]="$workspace/build"
    staged_output="$workspace/build/${output#"${destination%/}/"}"
  elif [[ "${staged_command[index]}" == "$output" ]]; then
    staged_command[index]="$workspace/image.tvm"
    staged_output="$workspace/image.tvm"
  fi
done
if [[ -z "$staged_output" ]]; then
  echo 'typed validator build command must declare its output destination' >&2
  exit 2
fi
build_fingerprint="$fingerprint"
run_bounded_build "${staged_command[@]}"
if [[ ! -s "$staged_output" ]]; then
  echo "typed validator build did not create $staged_output" >&2
  exit 1
fi
refresh_fingerprint
if [[ "$fingerprint" != "$build_fingerprint" ]]; then
  echo 'typed validator inputs changed during compilation; prior checkpoint preserved' >&2
  exit 1
fi
output_hash="$(sha256_file "$staged_output")"
stamp_tmp="$workspace/seal"
printf 'inputs=%s\noutput-sha256=%s\n' "$fingerprint" "$output_hash" > "$stamp_tmp"
prepare_publication
mv "$staged_output" "$output"
recover_publication
build_in_progress=0
echo "built and sealed typed validator: $output"
