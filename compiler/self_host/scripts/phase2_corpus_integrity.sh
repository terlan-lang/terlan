#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
corpus="$root/compiler/self_host/corpus"
manifest="$corpus/CORPUS_PHASE_2.tsv"

declare -A declared=()
while IFS=$'\t' read -r first second rest; do
    [[ -z "${first:-}" || "$first" == \#* ]] && continue
    if [[ "$first" == *.terl ]]; then
        fixture="$first"
        expectation="$second"
    else
        expectation="$first"
        fixture="$second"
    fi
    case "$expectation" in
        accepted|rejected) ;;
        *) echo "invalid Phase 2 expectation: $expectation" >&2; exit 1 ;;
    esac
    [[ "$fixture" = /* ]] || fixture="$corpus/$fixture"
    [[ -f "$fixture" ]] || { echo "missing Phase 2 fixture: $fixture" >&2; exit 1; }
    [[ -z "${declared[$fixture]:-}" ]] || { echo "duplicate Phase 2 fixture: $fixture" >&2; exit 1; }
    declared[$fixture]="$expectation"
done < "$manifest"

while IFS= read -r fixture; do
    [[ -n "${declared[$fixture]:-}" ]] || {
        echo "fixture omitted from CORPUS_PHASE_2.tsv: $fixture" >&2
        exit 1
    }
done < <(find "$corpus/accepted" "$corpus/rejected" -maxdepth 1 -type f -name '*.terl' | sort)

for required in \
    accepted/Minimal.terl \
    accepted/Surface.terl \
    accepted/Expressions.terl \
    accepted/Patterns.terl \
    rejected/RecoveryAfterInvalidExpression.terl \
    rejected/FuzzMissingCallClose.terl \
    rejected/FuzzMissingCaseArrow.terl \
    rejected/FuzzUnterminatedList.terl \
    rejected/FuzzInvalidType.terl; do
    fixture="$corpus/$required"
    [[ -n "${declared[$fixture]:-}" ]] || {
        echo "mandatory Phase 2 coverage missing: $required" >&2
        exit 1
    }
done

printf 'Phase 2 corpus integrity: %d fixtures\n' "${#declared[@]}"
