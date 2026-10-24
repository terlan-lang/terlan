#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
    printf 'usage: %s <terlan-self-host> <output.{o|tvm}> <source.terl> [source.terl ...]\n' "$0" >&2
    exit 64
fi

self_host=$1
output_file=$2
shift 2
workspace=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
if [[ "$output_file" == *.tvm ]]; then
    native_consumer=${TERLAN_SELF_HOST_IMAGE_CONSUMER:-"$workspace/target/debug/terlan-self-host-image"}
else
    native_consumer=${TERLAN_SELF_HOST_NATIVE_APP:-"$workspace/target/debug/terlan-self-host-native-app"}
fi
if [[ -n "${TERLAN_SELF_HOST_STAGE_WORK:-}" ]]; then
    temporary_directory=$TERLAN_SELF_HOST_STAGE_WORK
    mkdir -p "$temporary_directory"
    cleanup_temporary_directory=false
else
    temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/terlan-self-host-native-app.XXXXXX")
    cleanup_temporary_directory=true
fi
cleanup() {
    if [[ "$cleanup_temporary_directory" == true ]]; then
        rm -rf "$temporary_directory"
    else
        printf 'native application stage work retained at %s\n' "$temporary_directory" >&2
    fi
}
trap cleanup EXIT
interface_jobs=${TERLAN_SELF_HOST_INTERFACE_JOBS:-4}
backend_jobs=${TERLAN_SELF_HOST_BACKEND_JOBS:-4}

for jobs in "$interface_jobs" "$backend_jobs"; do
    if [[ ! "$jobs" =~ ^[1-9][0-9]*$ ]]; then
        printf 'native application worker counts must be positive integers\n' >&2
        exit 64
    fi
done

interfaces_manifest="$temporary_directory/interfaces.manifest"
interface_commands="$temporary_directory/interface-commands"
backend_commands="$temporary_directory/backend-commands"
default_actor_profiles="$temporary_directory/actor-profiles.json"
default_actor_dynamic_evidence="$temporary_directory/actor-dynamic-evidence.json"
printf '%s\n' \
    '{"schema":"terlan.self-host.actor-ir/v2","profiles":[],"continuations":[]}' \
    > "$default_actor_profiles"
printf '%s\n' \
    '{"schema":"terlan.self-host.actor-dynamic-evidence/v1","functions":[]}' \
    > "$default_actor_dynamic_evidence"
actor_profiles=${TERLAN_SELF_HOST_ACTOR_PROFILES:-$default_actor_profiles}
actor_dynamic_evidence=${TERLAN_SELF_HOST_ACTOR_DYNAMIC_EVIDENCE:-$default_actor_dynamic_evidence}

sources=("$@")
interface_files=()
: > "$interface_commands"
index=0
for source_file in "${sources[@]}"; do
    identity=$(printf '%06d' "$index")
    interface_file="$temporary_directory/$identity.interface"
    interface_files+=("$interface_file")
    if [[ ! -f "$interface_file" ]]; then
        printf '%s\0%s\0' "$source_file" "$interface_file" >> "$interface_commands"
    fi
    index=$((index + 1))
done
xargs -0 -r -n 2 -P "$interface_jobs" \
    "$self_host" interface-unit < "$interface_commands"
for ((index = 0; index < ${#sources[@]}; index += 1)); do
    if [[ ! -f "${interface_files[$index]}" ]]; then
        printf 'interface publication failed for %s\n' "${sources[$index]}" >&2
        exit 1
    fi
done
printf '%s\n' "${interface_files[@]}" > "$interfaces_manifest"

typed_inputs=()
core_files=()
variant_files=()
actor_files=()
diagnostics_files=()
completion_markers=()
: > "$backend_commands"
index=0
for source_file in "${sources[@]}"; do
    identity=$(printf '%06d' "$index")
    core_file="$temporary_directory/$identity.core.json"
    variant_file="$temporary_directory/$identity.variants.json"
    actor_file="$temporary_directory/$identity.actors.json"
    diagnostics_file="$temporary_directory/$identity.diagnostics"
    completion_marker="$temporary_directory/$identity.complete"
    printf '%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0' \
        "$source_file" \
        "$interfaces_manifest" \
        "$actor_profiles" \
        "$actor_dynamic_evidence" \
        "$core_file" \
        "$variant_file" \
        "$actor_file" \
        "$diagnostics_file" \
        "$completion_marker" >> "$backend_commands"
    core_files+=("$core_file")
    variant_files+=("$variant_file")
    actor_files+=("$actor_file")
    diagnostics_files+=("$diagnostics_file")
    completion_markers+=("$completion_marker")
    typed_inputs+=("$core_file" "$variant_file" "$actor_file")
    index=$((index + 1))
done
xargs -0 -r -n 9 -P "$backend_jobs" \
    "$self_host" actor-backend-unit < "$backend_commands"

for ((index = 0; index < ${#sources[@]}; index += 1)); do
    if [[ ! -s "${core_files[$index]}" \
        || ! -s "${variant_files[$index]}" \
        || ! -s "${actor_files[$index]}" \
        || -s "${diagnostics_files[$index]}" \
        || ! -f "${completion_markers[$index]}" ]] \
        || ! grep -qx 'complete' "${completion_markers[$index]}"; then
        printf 'actor backend publication failed for %s\n' "${sources[$index]}" >&2
        if [[ -s "${diagnostics_files[$index]}" ]]; then
            cat "${diagnostics_files[$index]}" >&2
        fi
        exit 1
    fi
done

"$native_consumer" "$output_file" "${typed_inputs[@]}"
