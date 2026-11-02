#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    printf 'usage: %s <terlan-self-host> <output-directory>\n' "$0" >&2
    exit 64
fi

self_host=$1
output=$2
workspace=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
source_file="$workspace/compiler/self_host/corpus/backend/ActorContinuation.terl"

if [[ ! -x "$self_host" ]]; then
    printf 'self-host image is not executable: %s\n' "$self_host" >&2
    exit 64
fi

mkdir -p "$output/first" "$output/second"
: > "$output/interfaces.manifest"
: > "$output/dynamic.manifest"

identity=$(printf 'terlan-tvm-reduction-continuation-v1\0self_host_probe.ActorContinuation\0suspend\0\001\0\0\0\0\0\0\0' \
    | sha256sum | cut -d ' ' -f 1)
cat > "$output/profiles.json" <<EOF
{"schema":"terlan.self-host.actor-ir/v2","profiles":[{"module_name":"self_host_probe.ActorContinuation","callable_name":"suspend","arity":1,"runtime_owned":true,"entries":[{"has_export_identity":false,"export_identity":"","continuation_identity":"sha256:$identity","capture_count":0}]}],"continuations":[]}
EOF
printf '%s\n' "$output/profiles.json" > "$output/profiles.manifest"

compile() {
    local run=$1
    "$self_host" actor-backend-unit \
        "$source_file" \
        "$output/interfaces.manifest" \
        "$output/profiles.manifest" \
        "$output/dynamic.manifest" \
        "$output/$run/core.json" \
        "$output/$run/variant.json" \
        "$output/$run/actor.json" \
        "$output/$run/diagnostics.txt" \
        "$output/$run/complete.marker"
    test -s "$output/$run/core.json"
    test -s "$output/$run/variant.json"
    test -s "$output/$run/actor.json"
    test ! -s "$output/$run/diagnostics.txt"
    grep -qx 'complete' "$output/$run/complete.marker"
}

compile first
compile second

cmp "$output/first/core.json" "$output/second/core.json"
cmp "$output/first/variant.json" "$output/second/variant.json"
cmp "$output/first/actor.json" "$output/second/actor.json"

jq -e '
    any(.profiles[];
        .module_name == "self_host_probe.ActorContinuation"
        and .callable_name == "suspend"
        and .arity == 1
        and .runtime_owned == true)
    and any(.profiles[];
        .module_name == "self_host_probe.ActorContinuation"
        and .callable_name == "main"
        and .arity == 0
        and .runtime_owned == false)
    and any(.continuations[];
        .source_module == "self_host_probe.ActorContinuation"
        and .source_function == "main"
        and .source_arity == 0
        and .outward_entry == true)
    and any(.suspension_sites[];
        .source_function == "main"
        and .callee_module == "self_host_probe.ActorContinuation"
        and .callee_function == "suspend"
        and .callee_arity == 1)
' "$output/first/actor.json" > /dev/null

printf '%s\n' \
    $'actor-profile-catalog\tpass' \
    $'actor-continuation-projection\tpass' \
    $'actor-suspension-site\tpass' \
    $'actor-byte-determinism\tpass' \
    > "$output/evidence.tsv"
printf 'complete\n' > "$output/complete.marker"
printf 'Phase 5 actor continuation evidence passed\n'
