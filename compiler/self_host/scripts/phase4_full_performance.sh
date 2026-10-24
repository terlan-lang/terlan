#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BUDGET="$ROOT/compiler/self_host/phase/PERFORMANCE_BUDGET.toml"
IMAGE="${TERLAN_SELF_HOST_IMAGE:-$ROOT/target/quality/self-host-phase4-current-image-55/bin/terlan-self-host}"
REFERENCE="${TERLAN_SELF_HOST_PHASE4_REFERENCE_EVIDENCE:-$ROOT/target/quality/self-host-phase4-current-stage0-54/evidence}"
MANIFEST="${TERLAN_SELF_HOST_PHASE4_SOURCES_MANIFEST:-$ROOT/target/quality/self-host-phase4-current-stage1-55/package/sources.manifest}"
RESULT_ROOT="${TERLAN_SELF_HOST_PERFORMANCE_ROOT:-$ROOT/target/quality/self-host-phase4-performance}"
WORK="$(mktemp -d "$RESULT_ROOT.full.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

budget() {
    local section="$1"
    local key="$2"
    awk -v section="[$section]" -v key="$key" '
        $0 == section { active = 1; next }
        /^\[/ { active = 0 }
        active && $1 == key { print $3; exit }
    ' "$BUDGET"
}

within() {
    awk -v actual="$1" -v maximum="$2" 'BEGIN { exit !(actual <= maximum) }'
}

mkdir -p "$RESULT_ROOT"
metrics="$WORK/full.metrics"
/usr/bin/time -o "$metrics" -f '%e\t%M' env \
    TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    TERLAN_SELF_HOST_BATCH_JOBS="$(budget full_bootstrap workers)" \
    TERLAN_SELF_HOST_BATCH_MAX_COMMANDS="$(budget full_bootstrap commands_per_worker)" \
    "$ROOT/compiler/self_host/scripts/stage_evidence_batch.sh" \
    "$MANIFEST" "$WORK/evidence"
IFS=$'\t' read -r seconds peak_rss < "$metrics"
commands="$(cat "$WORK/evidence/batch/command-count")"
expected_artifacts="$(budget full_bootstrap artifacts)"
test "$commands" -eq "$(budget full_bootstrap commands)"

{
    printf 'metric\twall_seconds\tpeak_rss_kib\tcommands\tartifacts\tworkers\n'
    printf 'full_bootstrap\t%s\t%s\t%s\t%s\t%s\n' \
        "$seconds" "$peak_rss" "$commands" "$expected_artifacts" "$(budget full_bootstrap workers)"
} > "$RESULT_ROOT/full.tsv"

within "$seconds" "$(budget full_bootstrap wall_seconds_max)"
within "$peak_rss" "$(budget full_bootstrap peak_rss_kib_max)"

artifacts=0
while IFS= read -r generated; do
    relative="${generated#"$WORK/evidence/"}"
    cmp -s "$generated" "$REFERENCE/$relative"
    artifacts=$((artifacts + 1))
done < <(find "$WORK/evidence" -type f \
    \( -name '*.tokens' -o -name '*.syntax' -o -name '*.semantic-common' -o -name '*.semantic-supplemental' \) \
    | sort)
test "$artifacts" -eq "$expected_artifacts"

{
    printf 'metric\twall_seconds\tpeak_rss_kib\tcommands\tartifacts\tworkers\n'
    printf 'full_bootstrap\t%s\t%s\t%s\t%s\t%s\n' \
        "$seconds" "$peak_rss" "$commands" "$artifacts" "$(budget full_bootstrap workers)"
} > "$RESULT_ROOT/full.tsv"

printf 'Phase 4 full performance: pass (wall=%ss, peak_rss=%s KiB, artifacts=%s, workers=%s)\n' \
    "$seconds" "$peak_rss" "$artifacts" "$(budget full_bootstrap workers)"
