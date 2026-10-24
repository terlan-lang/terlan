#!/usr/bin/env bash
set -euo pipefail

root=${TERLAN_SELF_HOST_PHASE4_ROOT:-target/quality/self-host-phase4-current-stage1-55}
reference=${TERLAN_SELF_HOST_PHASE4_REFERENCE:-target/quality/self-host-phase4-current-stage0-54}
image=${TERLAN_SELF_HOST_PHASE4_IMAGE:-target/quality/self-host-phase4-current-image-55/bin/terlan-self-host}
evidence="$root/evidence"

checked=0
for layer in tokens syntax semantic-common semantic-supplemental; do
    case "$layer" in
        tokens) pattern='*.tokens' ;;
        syntax) pattern='*.syntax' ;;
        semantic-common) pattern='*.semantic-common' ;;
        semantic-supplemental) pattern='*.semantic-supplemental' ;;
    esac
    while IFS= read -r candidate; do
        expected="$reference/evidence/${candidate#"$evidence/"}"
        cmp -s "$expected" "$candidate" || {
            printf 'Phase 4 evidence mismatch: %s\n' "$candidate" >&2
            exit 1
        }
        checked=$((checked + 1))
    done < <(find "$evidence/$layer" -maxdepth 1 -type f -name "$pattern" | sort)
done

test "$checked" -eq 800
test -s "$evidence/tokens.bundle"
test -s "$evidence/syntax.bundle"
test -s "$evidence/semantics.bundle"
TERLAN_SELF_HOST_IMAGE="$image" compiler/self_host/scripts/bootstrap_fixed_point.sh \
    "$reference/bootstrap-artifact.json" \
    "$root/bootstrap-artifact.json"

printf 'Phase 4 bootstrap correctness: pass (%s byte-identical artifacts)\n' "$checked"
