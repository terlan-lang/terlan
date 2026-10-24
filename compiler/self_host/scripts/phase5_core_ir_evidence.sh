#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: phase5_core_ir_evidence.sh SELF_HOST_IMAGE INTERFACES_MANIFEST OUTPUT" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE=$1
INTERFACES=$2
OUTPUT=$3
CORPUS="$ROOT/compiler/self_host/corpus/REGRESSION_CORPUS.tsv"
PARITY="$ROOT/target/debug/terlan-self-host-parity"
ADMIT="$ROOT/target/debug/terlan-self-host-abi1"
MARKER="$OUTPUT/complete.marker"

[[ -x "$IMAGE" ]] || { echo "self-host image is not executable: $IMAGE" >&2; exit 2; }
[[ -s "$INTERFACES" ]] || { echo "interface manifest is missing: $INTERFACES" >&2; exit 2; }
[[ -x "$PARITY" ]] || { echo "Rust canonicalizer is missing: $PARITY" >&2; exit 2; }
[[ -x "$ADMIT" ]] || { echo "Rust ABI-1 admission tool is missing: $ADMIT" >&2; exit 2; }

mkdir -p "$OUTPUT/run1" "$OUTPUT/run2" "$OUTPUT/canonical" "$OUTPUT/rejected"
if [[ -e "$MARKER" ]]; then
    mv "$MARKER" "$OUTPUT/stale-complete-marker.$$.txt"
fi
: > "$OUTPUT/evidence.tsv"

equivalent=0
rejected=0
while IFS=$'\t' read -r stage expectation relative; do
    [[ "$stage" == "backend" ]] || continue
    [[ "$stage" != \#* ]] || continue
    source_path="$ROOT/compiler/self_host/corpus/$relative"
    stem=$(basename "$source_path" .terl)

    case "$expectation" in
        equivalent)
            for run in run1 run2; do
                "$IMAGE" backend-unit \
                    "$source_path" \
                    "$INTERFACES" \
                    "$OUTPUT/$run/$stem.core.json" \
                    "$OUTPUT/$run/$stem.diagnostics"
                [[ -s "$OUTPUT/$run/$stem.core.json" ]]
                [[ ! -s "$OUTPUT/$run/$stem.diagnostics" ]]
            done
            cmp -s "$OUTPUT/run1/$stem.core.json" "$OUTPUT/run2/$stem.core.json"
            "$PARITY" canonicalize \
                "$OUTPUT/run1/$stem.core.json" \
                "$OUTPUT/canonical/$stem.core.json"
            cmp -s "$OUTPUT/run1/$stem.core.json" "$OUTPUT/canonical/$stem.core.json"
            "$ADMIT" "$OUTPUT/run1/$stem.core.json" >/dev/null
            digest=$(sha256sum "$OUTPUT/run1/$stem.core.json" | cut -d' ' -f1)
            printf 'equivalent\t%s\t%s\n' "$relative" "$digest" >> "$OUTPUT/evidence.tsv"
            equivalent=$((equivalent + 1))
            ;;
        reject)
            expected="$source_path"
            expected=${expected%.terl}.diagnostic.txt
            [[ -s "$expected" ]] || {
                echo "backend rejection fixture lacks exact diagnostics: $relative" >&2
                exit 1
            }
            "$IMAGE" backend-unit \
                "$source_path" \
                "$INTERFACES" \
                "$OUTPUT/rejected/$stem.core.json" \
                "$OUTPUT/rejected/$stem.diagnostics"
            [[ ! -s "$OUTPUT/rejected/$stem.core.json" ]]
            cmp -s "$expected" "$OUTPUT/rejected/$stem.diagnostics"
            digest=$(sha256sum "$OUTPUT/rejected/$stem.diagnostics" | cut -d' ' -f1)
            printf 'reject\t%s\t%s\n' "$relative" "$digest" >> "$OUTPUT/evidence.tsv"
            rejected=$((rejected + 1))
            ;;
        *)
            echo "unsupported backend corpus expectation: $expectation" >&2
            exit 1
            ;;
    esac
done < "$CORPUS"

[[ $equivalent -gt 0 ]]
[[ $rejected -gt 0 ]]
printf 'schema\tterlan.self-host.phase5-core-ir-evidence/v1\n' > "$OUTPUT/summary.tsv"
printf 'equivalent\t%s\nrejected\t%s\nfailures\t0\n' \
    "$equivalent" "$rejected" >> "$OUTPUT/summary.tsv"
printf 'complete\n' > "$MARKER"
printf 'Phase 5 Core IR evidence passed: equivalent=%s rejected=%s\n' \
    "$equivalent" "$rejected"
