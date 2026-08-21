#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
orchestrator="$repo_root/target/debug/terlan-test-orchestrator"

test -x "$orchestrator" || {
  echo "missing prebuilt canonical Rust-suite orchestrator: $orchestrator" >&2
  exit 1
}

TERLAN_RUST_SUITE_REPORT="$repo_root/target/quality/rust-test-suite-report.json" \
  "$orchestrator"
bash "$repo_root/scripts/check_registry_cli_integration_v1.sh"

echo "Registry adversarial package corpus passed"
