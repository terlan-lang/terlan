#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: stage_evidence_batch.sh SOURCES_MANIFEST OUTPUT_DIR" >&2
    exit 2
fi

sources_manifest=$1
output=$2
image=${TERLAN_SELF_HOST_IMAGE:?set TERLAN_SELF_HOST_IMAGE to the retained compiler image}
jobs=${TERLAN_SELF_HOST_BATCH_JOBS:-4}
max_commands=${TERLAN_SELF_HOST_BATCH_MAX_COMMANDS:-4}

[[ "$jobs" =~ ^[1-9][0-9]*$ ]] || {
    echo "TERLAN_SELF_HOST_BATCH_JOBS must be a positive integer" >&2
    exit 2
}
[[ "$max_commands" =~ ^[1-9][0-9]*$ ]] || {
    echo "TERLAN_SELF_HOST_BATCH_MAX_COMMANDS must be a positive integer" >&2
    exit 2
}

mkdir -p "$output/tokens" "$output/syntax" "$output/semantic-common" \
    "$output/semantic-supplemental" "$output/batch"

for ((shard = 0; shard < jobs; shard += 1)); do
    : > "$output/batch/commands-$shard.tsv"
done

command_count=0
schedule() {
    local shard=$((command_count % jobs))
    local commands="$output/batch/commands-$shard.tsv"
    printf '%s' "$1" >> "$commands"
    shift
    printf '\t%s' "$@" >> "$commands"
    printf '\n' >> "$commands"
    command_count=$((command_count + 1))
}

while IFS= read -r source; do
    [[ -n "$source" ]] || continue
    key=${source%.terl}
    key=${key//\//_}
    key=${key//./_}

    tokens="$output/tokens/$key.tokens"
    syntax="$output/syntax/$key.syntax"
    common="$output/semantic-common/$key.semantic-common"
    supplemental="$output/semantic-supplemental/$key.semantic-supplemental"

    if [[ ! -f "$tokens" || ! -f "$syntax" || ! -f "$common" || ! -f "$supplemental" ]]; then
        schedule stage-complete "$source" "$tokens" "$syntax" "$common" "$supplemental"
    fi
done < "$sources_manifest"

printf '%s\n' "$command_count" > "$output/batch/command-count"

run_directory=$(mktemp -d "$output/batch/run.XXXXXX")
pids=()
labels=()
for ((shard = 0; shard < jobs; shard += 1)); do
    commands="$output/batch/commands-$shard.tsv"
    [[ -s "$commands" ]] || continue
    prefix="$run_directory/chunk-$shard-"
    split -l "$max_commands" -d -a 4 "$commands" "$prefix"
    (
        : > "$output/batch/worker-$shard.log"
        for chunk in "$prefix"*; do
            while IFS=$'\t' read -r -a command; do
                [[ ${#command[@]} -gt 0 ]] || continue
                "$image" "${command[@]}" >>"$output/batch/worker-$shard.log" 2>&1
            done < "$chunk"
        done
    ) &
    pids+=("$!")
    labels+=("$shard")
done

failed=0
for index in "${!pids[@]}"; do
    if ! wait "${pids[$index]}"; then
        printf 'self-host batch worker %s failed\n' "${labels[$index]}" >&2
        failed=1
    fi
done
[[ "$failed" -eq 0 ]] || exit 1

TERLAN_SELF_HOST_IMAGE="$image" compiler/self_host/scripts/stage_evidence.sh \
    "$sources_manifest" "$output"

printf 'self-host batch evidence: commands=%s workers=%s max_commands_per_worker=%s\n' \
    "$command_count" "$jobs" "$max_commands"
