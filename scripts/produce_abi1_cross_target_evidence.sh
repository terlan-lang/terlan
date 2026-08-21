#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
revision=${TERLAN_ABI1_REVISION:?TERLAN_ABI1_REVISION is required}
runner=${TERLAN_ABI1_AARCH64_RUNNER:?TERLAN_ABI1_AARCH64_RUNNER is required}
cargo=${CARGO:-cargo}

case "$revision" in
  *[!A-Za-z0-9._:-]*|'')
    echo "TERLAN_ABI1_REVISION contains unsupported characters" >&2
    exit 1
    ;;
esac

evidence_dir="$root/target/abi1-evidence"
fragment_dir="$evidence_dir/cross-target-fragments"
mkdir -p "$fragment_dir"
rm -f "$fragment_dir/x86_64.json" "$fragment_dir/aarch64.json"

(
  cd "$root"
  TERLAN_ABI1_TARGET_FRAGMENT="$fragment_dir/x86_64.json" \
    TERLAN_ABI1_TARGET_TRIPLE="x86_64-unknown-linux-gnu" \
    TERLAN_ABI1_REVISION="$revision" \
    "$cargo" test --locked --release -p terlan \
      --test abi1_evidence_producers \
      --target x86_64-unknown-linux-gnu \
      abi1_cross_target_probe -- --exact
)

(
  cd "$root"
  TERLAN_ABI1_TARGET_FRAGMENT="$fragment_dir/aarch64.json" \
    TERLAN_ABI1_TARGET_TRIPLE="aarch64-unknown-linux-gnu" \
    TERLAN_ABI1_REVISION="$revision" \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER="$runner" \
    "$cargo" test --locked --release -p terlan \
      --test abi1_evidence_producers \
      --target aarch64-unknown-linux-gnu \
      abi1_cross_target_probe -- --exact
)

test -s "$fragment_dir/x86_64.json"
test -s "$fragment_dir/aarch64.json"

output="$evidence_dir/cross-target-conformance.json"
{
  printf '{\n'
  printf '  "schema": "terlan.abi1.gate-evidence.v1",\n'
  printf '  "gate": "cross-target-conformance",\n'
  printf '  "abi_version": 1,\n'
  printf '  "managed_layout_profile": 1,\n'
  printf '  "status": "passed",\n'
  printf '  "revision": "%s",\n' "$revision"
  printf '  "runs": [\n'
  sed 's/^/    /' "$fragment_dir/x86_64.json"
  printf '    ,\n'
  sed 's/^/    /' "$fragment_dir/aarch64.json"
  printf '  ]\n'
  printf '}\n'
} > "$output"
