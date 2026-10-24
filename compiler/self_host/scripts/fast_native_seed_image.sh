#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
output=${1:?"usage: fast_native_seed_image.sh <output-directory>"}
if [[ $output != /* ]]; then
    output="$repo_root/$output"
fi

terlc=${TERLAN_SELF_HOST_TERLC:-$repo_root/target/release/terlc}
runtime=${TERLAN_VM:-$repo_root/target/release/terlan-vm}
project=${TERLAN_SELF_HOST_PRODUCTION_PROJECT:-$repo_root/target/quality/self-host-production-project}
cache=${TERLAN_SELF_HOST_BUILD_CACHE:-$repo_root/target/quality/self-host-fast-build-cache}
source_root="$repo_root/compiler/self_host/src/self_host"
generated_root="$project/src/self_host"
selective_staging=${TERLAN_SELF_HOST_SELECTIVE_STAGING:-0}

[[ -x $terlc ]] || {
    printf 'optimized compiler is missing: %s\n' "$terlc" >&2
    printf 'build it once with: cargo build --release -p terlan --bin terlc\n' >&2
    exit 2
}
[[ -x $runtime ]] || {
    printf 'packaged runtime is missing: %s\n' "$runtime" >&2
    printf 'build it once with: cargo build --release -p terlan --bin terlan-vm\n' >&2
    exit 2
}

mkdir -p "$generated_root" "$cache" "$output"

declare -A staged_sources=()

self_host_module_source() {
    local module=$1 suffix candidate
    [[ $module == self_host.* ]] || return 1
    suffix=${module#self_host.}
    while [[ -n $suffix ]]; do
        candidate="$source_root/${suffix//./\/}.terl"
        if [[ -f $candidate ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
        [[ $suffix == *.* ]] || break
        suffix=${suffix%.*}
    done
    return 1
}

import_declarations() {
    awk '
        function trim(value) {
            sub(/^[[:space:]]+/, "", value)
            sub(/[[:space:]]+$/, "", value)
            return value
        }
        {
            line = trim($0)
            if (!collecting && line ~ /^import[[:space:]]/) {
                collecting = 1
                declaration = line
            } else if (collecting) {
                declaration = declaration " " line
            }
            if (collecting && line ~ /\.[[:space:]]*$/) {
                print declaration
                collecting = 0
                declaration = ""
            }
        }
    ' "$1"
}

enqueue_module() {
    local module=$1 source
    if source=$(self_host_module_source "$module"); then
        [[ -n ${staged_sources[$source]+present} ]] || source_queue+=("$source")
    fi
}

enqueue_import() {
    local declaration=$1 body parent items item
    body=${declaration#import }
    body=${body#type }
    body=${body%.}
    body=${body%% as *}
    [[ $body == self_host* ]] || return 0
    if [[ $body == *'.{'* ]]; then
        parent=${body%%.\{*}
        if self_host_module_source "$parent" >/dev/null; then
            enqueue_module "$parent"
        else
            items=${body#*.\{}
            items=${items%\}}
            items=${items// /}
            while IFS= read -r item; do
                [[ -n $item ]] && enqueue_module "$parent.$item"
            done < <(printf '%s\n' "${items//,/$'\n'}")
        fi
    else
        enqueue_module "$body"
    fi
}

stage_source() {
    local source=$1 relative destination declaration
    [[ -n ${staged_sources[$source]+present} ]] && return 0
    staged_sources[$source]=1
    relative=${source#"$source_root/"}
    destination="$generated_root/$relative"
    mkdir -p "$(dirname "$destination")"
    if [[ ! -f $destination ]] || ! cmp -s "$source" "$destination"; then
        cp --reflink=auto --preserve=timestamps "$source" "$destination"
    fi
    while IFS= read -r declaration; do
        enqueue_import "$declaration"
    done < <(import_declarations "$source")
}

# Keep a path-stable production package so unchanged frontend cache keys survive
# across evidence directories. Test modules remain in the authoritative source
# tree and full-source checks, but are not linked into the compiler executable.
if [[ $selective_staging == 1 ]]; then
    entry_source=${TERLAN_SELF_HOST_ENTRY_SOURCE:-}
    [[ -n $entry_source ]] || {
        printf 'selective staging requires TERLAN_SELF_HOST_ENTRY_SOURCE\n' >&2
        exit 2
    }
    if [[ $entry_source != /* ]]; then
        entry_source="$repo_root/$entry_source"
    fi
    source_queue=()
    while IFS= read -r declaration; do
        enqueue_import "$declaration"
    done < <(import_declarations "$entry_source")
    queue_index=0
    while (( queue_index < ${#source_queue[@]} )); do
        stage_source "${source_queue[$queue_index]}"
        ((queue_index += 1))
    done
else
    while IFS= read -r -d '' source; do
        relative=${source#"$source_root/"}
        destination="$generated_root/$relative"
        mkdir -p "$(dirname "$destination")"
        if [[ ! -f $destination ]] || ! cmp -s "$source" "$destination"; then
            cp --reflink=auto --preserve=timestamps "$source" "$destination"
        fi
        staged_sources[$source]=1
    done < <(
        find "$source_root" -type f \( -name '*.terl' -o -name '*.terli' \) \
            ! -name '*Test.terl' ! -name '*Test.terli' -print0
    )
fi

# Remove generated modules that were renamed or removed from the source tree.
while IFS= read -r -d '' generated; do
    relative=${generated#"$generated_root/"}
    source="$source_root/$relative"
    if [[ $relative != Main.terl ]] && { [[ ! -f $source ]] || [[ -z ${staged_sources[$source]+present} ]]; }; then
        unlink "$generated"
    fi
done < <(find "$generated_root" -type f \( -name '*.terl' -o -name '*.terli' \) -print0)

entry_profile=${TERLAN_SELF_HOST_ENTRY_PROFILE:-full}
entry_source=${TERLAN_SELF_HOST_ENTRY_SOURCE:-}
if [[ -n $entry_source ]]; then
    if [[ $entry_source != /* ]]; then
        entry_source="$repo_root/$entry_source"
    fi
    [[ -f $entry_source ]] || {
        printf 'self-host entry source is missing: %s\n' "$entry_source" >&2
        exit 2
    }
    cp --reflink=auto --preserve=timestamps "$entry_source" "$generated_root/Main.terl"
else
    case $entry_profile in
        full) ;;
        phase4)
            cp --reflink=auto --preserve=timestamps \
                "$repo_root/compiler/self_host/entry/Phase4Main.terl" \
                "$generated_root/Main.terl"
            ;;
        *)
            printf 'unknown self-host entry profile: %s\n' "$entry_profile" >&2
            exit 2
            ;;
    esac
fi

cat > "$project/terlan.toml" <<'EOF'
[package]
name = "terlan-self-host"
version = "0.0.1"
namespace = "self_host"

[build]
source_roots = ["src"]
artifact = "terlan-vm"
EOF

compiler_args=(--incremental build --cache-dir "$cache" "$project" --target terlan-vm --out-dir "$output")
environment=("TERLAN_VM=$runtime")
if [[ ${TERLAN_SELF_HOST_LOW_MEMORY:-0} == 1 ]]; then
    environment+=("TERLAN_BUILD_LOW_MEMORY=1")
fi
if [[ ${TERLAN_SELF_HOST_TRACE:-0} == 1 ]]; then
    compiler_args=(--timings "${compiler_args[@]}")
    environment+=("TERLAN_NATIVE_AOT_TIMINGS=1" "TERLAN_NATIVE_AOT_PROFILE_TRACE=1")
fi

env "${environment[@]}" "$terlc" "${compiler_args[@]}"

image="$output/bin/terlan-self-host"
[[ -x $image ]] || {
    printf 'self-host image was not emitted: %s\n' "$image" >&2
    exit 1
}
printf '%s\n' "$image"
