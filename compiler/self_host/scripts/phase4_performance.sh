#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BUDGET="$ROOT/compiler/self_host/phase/PERFORMANCE_BUDGET.toml"
IMAGE="${TERLAN_SELF_HOST_IMAGE:-$ROOT/target/quality/self-host-phase4-current-image-55/bin/terlan-self-host}"
REFERENCE="$ROOT/target/quality/self-host-phase4-current-stage0-54/evidence"
MANIFEST="$ROOT/target/quality/self-host-phase4-current-stage1-55/package/sources.manifest"
RESULT_ROOT="${TERLAN_SELF_HOST_PERFORMANCE_ROOT:-$ROOT/target/quality/self-host-phase4-performance}"
WORK="$(mktemp -d "$RESULT_ROOT.work.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

budget() {
    local section="$1"
    local key="$2"
    awk -v section="[$section]" -v key="$key" '
        $0 == section { active = 1; next }
        /^\[/ { active = 0 }
        active && $1 == key {
            value = $3
            gsub(/"/, "", value)
            print value
            exit
        }
    ' "$BUDGET"
}

within() {
    awk -v actual="$1" -v maximum="$2" 'BEGIN { exit !(actual <= maximum) }'
}

timed() {
    local metrics="$1"
    shift
    /usr/bin/time -o "$metrics" -f '%e\t%M' "$@"
}

mkdir -p "$RESULT_ROOT"
test -x "$IMAGE"
test -f "$MANIFEST"

cold_metrics="$WORK/cold.metrics"
cold_output="$WORK/ProjectedParser.syntax"
timed "$cold_metrics" "$IMAGE" stage-layer syntax \
    "$ROOT/compiler/self_host/src/self_host/ProjectedParser.terl" "$cold_output"
cold_reference="$(find "$REFERENCE" -type f \
    -name compiler_self_host_src_self_host_ProjectedParser.syntax -print -quit)"
test -n "$cold_reference"
cmp -s "$cold_output" "$cold_reference"
IFS=$'\t' read -r cold_seconds cold_rss < "$cold_metrics"
within "$cold_seconds" "$(budget cold_single wall_seconds_max)"
within "$cold_rss" "$(budget cold_single peak_rss_kib_max)"

mini="$WORK/mini"
mkdir -p "$mini"
rg '/(NativeSyntax|SelfHostBackendEnvelope|ProjectedBackendAbi|ProjectedBackendAbiSyntax)[.]terl' \
    "$MANIFEST" > "$mini/sources.manifest"
test "$(wc -l < "$mini/sources.manifest")" -eq "$(budget parallel_mini modules)"

batch_metrics="$WORK/batch.metrics"
timed "$batch_metrics" env \
    TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    TERLAN_SELF_HOST_BATCH_JOBS="$(budget parallel_mini workers)" \
    TERLAN_SELF_HOST_BATCH_MAX_COMMANDS="$(budget parallel_mini commands_per_worker)" \
    "$ROOT/compiler/self_host/scripts/stage_evidence_batch.sh" \
    "$mini/sources.manifest" "$mini/evidence"
IFS=$'\t' read -r batch_seconds batch_rss < "$batch_metrics"
within "$batch_seconds" "$(budget parallel_mini wall_seconds_max)"
within "$batch_rss" "$(budget parallel_mini peak_rss_kib_max)"
batch_commands="$(cat "$mini/evidence/batch/command-count")"
test "$batch_commands" -eq "$(budget parallel_mini commands)"

artifact_count=0
while IFS= read -r generated; do
    relative="${generated#"$mini/evidence/"}"
    cmp -s "$generated" "$REFERENCE/$relative"
    artifact_count=$((artifact_count + 1))
done < <(find "$mini/evidence" -type f \
    \( -name '*.tokens' -o -name '*.syntax' -o -name '*.semantic-common' -o -name '*.semantic-supplemental' \) \
    | sort)
test "$artifact_count" -eq "$(budget parallel_mini artifacts)"

warm_metrics="$WORK/warm.metrics"
timed "$warm_metrics" env \
    TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    TERLAN_SELF_HOST_BATCH_JOBS="$(budget parallel_mini workers)" \
    TERLAN_SELF_HOST_BATCH_MAX_COMMANDS="$(budget parallel_mini commands_per_worker)" \
    "$ROOT/compiler/self_host/scripts/stage_evidence_batch.sh" \
    "$mini/sources.manifest" "$mini/evidence"
IFS=$'\t' read -r warm_seconds warm_rss < "$warm_metrics"
scheduled="$(cat "$mini/evidence/batch/command-count")"
test "$scheduled" -le "$(budget warm_unchanged scheduled_commands_max)"
within "$warm_seconds" "$(budget warm_unchanged wall_seconds_max)"

{
    printf 'metric\twall_seconds\tpeak_rss_kib\tcommands\tartifacts\n'
    printf 'cold_single\t%s\t%s\t1\t1\n' "$cold_seconds" "$cold_rss"
    printf 'parallel_mini\t%s\t%s\t%s\t%s\n' \
        "$batch_seconds" "$batch_rss" "$batch_commands" "$artifact_count"
    printf 'warm_unchanged\t%s\t%s\t%s\t%s\n' "$warm_seconds" "$warm_rss" "$scheduled" "$artifact_count"
} > "$RESULT_ROOT/quick.tsv"

printf 'Phase 4 quick performance: pass (cold=%ss, batch=%ss, warm=%ss, artifacts=%s)\n' \
    "$cold_seconds" "$batch_seconds" "$warm_seconds" "$artifact_count"
