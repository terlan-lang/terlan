#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    printf 'usage: %s <terlan-self-host> <source.terl> <output.o>\n' "$0" >&2
    exit 64
fi

self_host=$1
source_file=$2
output_file=$3
workspace=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
native_consumer=${TERLAN_SELF_HOST_NATIVE:-"$workspace/target/debug/terlan-self-host-native"}
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/terlan-self-host-native.XXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT

core_file="$temporary_directory/module.core.json"
variant_file="$temporary_directory/module.variants.json"
actor_file="$temporary_directory/module.actors.json"
diagnostics_file="$temporary_directory/module.diagnostics"
completion_marker="$temporary_directory/module.complete"
interfaces_manifest="$temporary_directory/interfaces.manifest"
actor_profiles="$temporary_directory/actor-profiles.json"
actor_dynamic_evidence="$temporary_directory/actor-dynamic-evidence.json"

: > "$interfaces_manifest"
printf '%s\n' \
    '{"schema":"terlan.self-host.actor-ir/v2","profiles":[],"continuations":[]}' \
    > "$actor_profiles"
printf '%s\n' \
    '{"schema":"terlan.self-host.actor-dynamic-evidence/v1","functions":[]}' \
    > "$actor_dynamic_evidence"

"$self_host" actor-backend-unit \
    "$source_file" \
    "$interfaces_manifest" \
    "$actor_profiles" \
    "$actor_dynamic_evidence" \
    "$core_file" \
    "$variant_file" \
    "$actor_file" \
    "$diagnostics_file" \
    "$completion_marker"

test -s "$core_file"
test -s "$variant_file"
test -s "$actor_file"
test ! -s "$diagnostics_file"
grep -qx 'complete' "$completion_marker"
"$native_consumer" "$core_file" "$variant_file" "$actor_file" "$output_file"
