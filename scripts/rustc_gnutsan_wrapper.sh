#!/usr/bin/env bash
# Keep Cargo build-script TARGET values ecosystem-compatible while selecting
# Rust's instrumented GNU ThreadSanitizer standard library for rustc.
set -euo pipefail

rustc_path=$1
shift

arguments=()
replace_next=false
for argument in "$@"; do
    if $replace_next; then
        if [[ "$argument" == "x86_64-unknown-linux-gnu" ]]; then
            argument="x86_64-unknown-linux-gnutsan"
        fi
        replace_next=false
    elif [[ "$argument" == "--target" ]]; then
        replace_next=true
    elif [[ "$argument" == "--target=x86_64-unknown-linux-gnu" ]]; then
        argument="--target=x86_64-unknown-linux-gnutsan"
    fi
    arguments+=("$argument")
done

exec "$rustc_path" "${arguments[@]}"
