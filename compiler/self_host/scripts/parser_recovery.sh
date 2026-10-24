#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fixture="$root/compiler/self_host/corpus/rejected/RecoveryAfterInvalidExpression.terl"
candidate="${TERLAN_SELF_HOST_IMAGE:-$root/target/quality/self-host-bootstrap-current-39/bin/terlan-self-host}"
compiler="${TERLAN_COMPILER:-$root/target/debug/terlc}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$compiler" syntax-contract --tokens "$fixture" --out "$work/rust.json"
"$candidate" tokens "$fixture" "$work/candidate.json"
cmp "$work/rust.json" "$work/candidate.json"
last_token_end="$(awk -F '\t' 'seen && NF >= 3 { end = $3 } /^--$/ { seen = 1 } END { print end }' "$work/rust.json")"
source_end="$(awk '{ total += length($0) + 1; if ($0 ~ /[^[:space:]]/) end = total - 1 } END { print end }' "$fixture")"
[[ -n "$last_token_end" && "$last_token_end" -eq "$source_end" ]] || {
    echo "token stream stopped at byte ${last_token_end:-unknown}; source ends at byte $source_end" >&2
    exit 1
}

echo 'Phase 2 recoverable token-stream parity: pass'
