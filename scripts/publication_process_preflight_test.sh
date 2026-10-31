#!/usr/bin/env bash
set -euo pipefail

repository=$(git rev-parse --show-toplevel)
mkdir -p "$repository/target"
temporary=$(mktemp -d "$repository/target/publication-process-preflight-test.XXXXXX")
blocker_pid=''
cleanup() {
  if [[ -n "$blocker_pid" ]]; then
    kill "$blocker_pid" 2>/dev/null || true
    wait "$blocker_pid" 2>/dev/null || true
  fi
  rm -rf "$temporary"
}
trap cleanup EXIT

bash "$repository/scripts/publication_process_preflight.sh" "$temporary"
mkdir -p "$temporary/target"
cp "$(command -v sleep)" "$temporary/target/terlc"
"$temporary/target/terlc" 30 &
blocker_pid=$!

for _attempt in $(seq 1 50); do
  [[ -e "/proc/$blocker_pid/exe" ]] && break
  sleep 0.02
done

if bash "$repository/scripts/publication_process_preflight.sh" "$temporary" \
  >"$temporary/preflight.log" 2>&1; then
  echo 'Publication preflight accepted a live repository build process' >&2
  exit 1
fi
grep -Fq 'repository build processes are still running' "$temporary/preflight.log"
grep -Fq "pid=$blocker_pid executable=$temporary/target/terlc" "$temporary/preflight.log"

kill "$blocker_pid"
wait "$blocker_pid" 2>/dev/null || true
blocker_pid=''
bash "$repository/scripts/publication_process_preflight.sh" "$temporary"
echo 'Publication process preflight regression passed.'
