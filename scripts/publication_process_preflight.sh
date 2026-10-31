#!/usr/bin/env bash
# Refuse destructive publication cleanup while this checkout owns live tools.
set -euo pipefail

root=${1:-$(git rev-parse --show-toplevel)}
root=$(realpath -e "$root")
blockers=()

for process in /proc/[0-9]*; do
  pid=${process##*/}
  [[ "$pid" != "$$" ]] || continue
  executable=$(readlink -f "$process/exe" 2>/dev/null || true)
  [[ -n "$executable" ]] || continue

  blocked=false
  case "$executable" in
    "$root"/target/*) blocked=true ;;
  esac

  if [[ "$blocked" == false ]]; then
    case "${executable##*/}" in
      cargo|rustc|sccache)
        working_directory=$(readlink -f "$process/cwd" 2>/dev/null || true)
        case "$working_directory" in
          "$root"|"$root"/*) blocked=true ;;
        esac
        ;;
    esac
  fi

  if [[ "$blocked" == true ]]; then
    working_directory=$(readlink -f "$process/cwd" 2>/dev/null || true)
    blockers+=("pid=$pid executable=$executable cwd=${working_directory:-unavailable}")
  fi
done

if (( ${#blockers[@]} > 0 )); then
  echo 'publish-preflight failed: repository build processes are still running' >&2
  printf '  - %s\n' "${blockers[@]}" >&2
  echo 'wait for these processes to finish, then rerun make publish' >&2
  exit 1
fi
