#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BUDGET="$ROOT/compiler/self_host/phase/PERFORMANCE_BUDGET.toml"
IMAGE="${TERLAN_SELF_HOST_IMAGE:-$ROOT/target/quality/self-host-phase4-current-image-55/bin/terlan-self-host}"
MANIFEST="$ROOT/target/quality/self-host-phase4-current-stage1-55/package/sources.manifest"
RESULT_ROOT="${TERLAN_SELF_HOST_PERFORMANCE_ROOT:-$ROOT/target/quality/self-host-phase4-performance}"
WORK="$(mktemp -d "$RESULT_ROOT.incremental.XXXXXX")"
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

timed() {
    local metrics="$1"
    shift
    /usr/bin/time -o "$metrics" -f '%e\t%M' "$@"
}

mkdir -p "$RESULT_ROOT"
printf '%s\n' \
    'compiler/self_host/src/self_host/ProjectedStageEvidence.terl' \
    > "$WORK/changed.manifest"
test "$(wc -l < "$WORK/changed.manifest")" -eq \
    "$(budget affected_incremental changed_modules)"

planner_metrics="$WORK/planner.metrics"
timed "$planner_metrics" "$IMAGE" affected-closure \
    "$MANIFEST" "$WORK/changed.manifest" "$WORK/affected.manifest"
IFS=$'\t' read -r planner_seconds planner_rss < "$planner_metrics"
within "$planner_seconds" "$(budget affected_incremental planner_wall_seconds_max)"
within "$planner_rss" "$(budget affected_incremental planner_peak_rss_kib_max)"

cat > "$WORK/expected.manifest" <<'EOF'
compiler/self_host/src/self_host/Main.terl
compiler/self_host/src/self_host/ProjectedStageEvidence.terl
compiler/self_host/src/self_host/SelfHostSeedStage.terl
EOF
cmp -s "$WORK/affected.manifest" "$WORK/expected.manifest"
test "$(wc -l < "$WORK/affected.manifest")" -eq \
    "$(budget affected_incremental affected_modules)"

mkdir -p "$WORK/reference/tokens" "$WORK/reference/syntax" \
    "$WORK/reference/semantic-common" "$WORK/reference/semantic-supplemental"
while IFS= read -r source; do
    key=${source%.terl}
    key=${key//\//_}
    key=${key//./_}
    "$IMAGE" stage-complete "$source" \
        "$WORK/reference/tokens/$key.tokens" \
        "$WORK/reference/syntax/$key.syntax" \
        "$WORK/reference/semantic-common/$key.semantic-common" \
        "$WORK/reference/semantic-supplemental/$key.semantic-supplemental"
done < "$WORK/affected.manifest"

compile_metrics="$WORK/compile.metrics"
timed "$compile_metrics" env \
    TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    TERLAN_SELF_HOST_BATCH_JOBS="$(budget affected_incremental workers)" \
    TERLAN_SELF_HOST_BATCH_MAX_COMMANDS="$(budget affected_incremental commands_per_worker)" \
    "$ROOT/compiler/self_host/scripts/stage_evidence_batch.sh" \
    "$WORK/affected.manifest" "$WORK/evidence"
IFS=$'\t' read -r compile_seconds compile_rss < "$compile_metrics"
within "$compile_seconds" "$(budget affected_incremental compile_wall_seconds_max)"
within "$compile_rss" "$(budget affected_incremental compile_peak_rss_kib_max)"
commands="$(cat "$WORK/evidence/batch/command-count")"
test "$commands" -eq "$(budget affected_incremental commands)"

artifacts=0
while IFS= read -r generated; do
    relative="${generated#"$WORK/evidence/"}"
    cmp -s "$generated" "$WORK/reference/$relative"
    artifacts=$((artifacts + 1))
done < <(find "$WORK/evidence" -type f \
    \( -name '*.tokens' -o -name '*.syntax' -o -name '*.semantic-common' -o -name '*.semantic-supplemental' \) \
    | sort)
test "$artifacts" -eq "$(budget affected_incremental artifacts)"

warm_metrics="$WORK/warm.metrics"
timed "$warm_metrics" env \
    TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    TERLAN_SELF_HOST_BATCH_JOBS="$(budget affected_incremental workers)" \
    TERLAN_SELF_HOST_BATCH_MAX_COMMANDS="$(budget affected_incremental commands_per_worker)" \
    "$ROOT/compiler/self_host/scripts/stage_evidence_batch.sh" \
    "$WORK/affected.manifest" "$WORK/evidence"
IFS=$'\t' read -r warm_seconds warm_rss < "$warm_metrics"
test "$(cat "$WORK/evidence/batch/command-count")" -eq 0
within "$warm_seconds" "$(budget affected_incremental warm_wall_seconds_max)"

{
    printf 'metric\twall_seconds\tpeak_rss_kib\tcommands\tartifacts\n'
    printf 'affected_planner\t%s\t%s\t0\t0\n' "$planner_seconds" "$planner_rss"
    printf 'affected_compile\t%s\t%s\t%s\t%s\n' \
        "$compile_seconds" "$compile_rss" "$commands" "$artifacts"
    printf 'affected_warm\t%s\t%s\t0\t%s\n' "$warm_seconds" "$warm_rss" "$artifacts"
} > "$RESULT_ROOT/incremental.tsv"

printf 'Phase 4 incremental performance: pass (planner=%ss, compile=%ss, warm=%ss, affected=%s, artifacts=%s)\n' \
    "$planner_seconds" "$compile_seconds" "$warm_seconds" \
    "$(budget affected_incremental affected_modules)" "$artifacts"
