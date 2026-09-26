#!/usr/bin/env bash
set -euo pipefail

terlc=${TERLC:-target/debug/terlc}
exec "$terlc" --incremental run compiler/self_host_syntax_evidence -- "$@"
