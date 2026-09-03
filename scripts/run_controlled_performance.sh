#!/usr/bin/env bash
# Run one performance evidence owner under an exclusive host lease.
set -euo pipefail

make_flags_request_dry_run() {
  local flags="$1"
  local flag
  for flag in $flags; do
    case "$flag" in
      --dry-run|--just-print|--recon)
        return 0
        ;;
      --*) ;;
      *n*) return 0 ;;
    esac
  done
  return 1
}

if [[ "${1:-}" == "--self-test" ]]; then
  sample='1 0 0 0 0 0 0 0 0 0 5 3 90 2 0 0 0'
  read -r runnable _rest <<<"$sample"
  [[ "$runnable" == "1" ]]
  ! make_flags_request_dry_run ' --no-print-directory'
  make_flags_request_dry_run 'n --no-print-directory'
  make_flags_request_dry_run 'krn --jobserver-auth=3,4'
  make_flags_request_dry_run ' --dry-run'
  echo "controlled performance lease self-test passed"
  exit 0
fi

if [[ "${1:-}" != "--" || "$#" -lt 2 ]]; then
  echo "usage: scripts/run_controlled_performance.sh -- <command> [arguments...]" >&2
  exit 2
fi
shift

# GNU Make executes recursive-make recipe lines under `-n`. Preserve the plan
# without probing or locking the host; the nested Make inherits dry-run mode.
if make_flags_request_dry_run "${MAKEFLAGS:-}"; then
  exec "$@"
fi

command -v flock >/dev/null 2>&1 || {
  echo "error[performance.lease]: flock is required" >&2
  exit 127
}
command -v vmstat >/dev/null 2>&1 || {
  echo "error[performance.quiescence]: vmstat is required" >&2
  exit 127
}

runtime_root="${XDG_RUNTIME_DIR:-/tmp}"
lease_path="$runtime_root/terlan-controlled-performance.lock"
exec 9>"$lease_path"
if ! flock --nonblock 9; then
  echo "error[performance.lease]: another Terlan performance owner holds $lease_path" >&2
  exit 75
fi

effective_cpus="$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc)"
sample="$(vmstat 1 3 | awk 'NF >= 17 && $1 ~ /^[0-9]+$/ { row=$0 } END { print row }')"
if [[ -z "$sample" ]]; then
  echo "error[performance.quiescence]: vmstat did not produce a CPU sample" >&2
  exit 1
fi
read -r runnable _blocked _swapd _free _buff _cache _si _so _bi _bo _in _cs user system idle wait _steal <<<"$sample"
busy=$((user + system + wait))
max_runnable=$((effective_cpus > 2 ? effective_cpus / 2 : 1))
if (( busy > 20 || runnable > max_runnable )); then
  echo "error[performance.quiescence]: host is busy (r=$runnable, busy=${busy}%, cpus=$effective_cpus)" >&2
  echo "retry after competing builds or tests finish; performance evidence was not started" >&2
  exit 75
fi

echo "[performance.lease] acquired $lease_path (r=$runnable, busy=${busy}%, cpus=$effective_cpus)"
exec "$@"
