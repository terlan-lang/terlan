#!/usr/bin/env bash
set -euo pipefail

bad=0
scan_roots=()

# Adds a path to the stale-reference scan when it exists in the current
# checkout.
#
# Input:
# - $1: repository-relative file or directory path.
#
# Output:
# - Appends the path to scan_roots when present.
#
# Transformation:
# - Converts the fixed release/scratch scan list into a checkout-sensitive list
#   so the same checker can run in scratch and published repositories.
add_scan_root() {
  local path="$1"

  if [[ -e "$path" ]]; then
    scan_roots+=("$path")
  fi
}

while IFS= read -r path; do
  printf 'source extension check failed: stale Terlan source extension remains: %s\n' "$path" >&2
  bad=1
done < <(
  find . \
    \( -path './target' -o -path './terlan' -o -path './otp' -o -path './.git' \) -prune \
    -o \( -name '*.tl' -o -name '*.tli' -o -name '*.tn' -o -name '*.tni' -o -name '*.ter' \) -print |
    sort
)

add_scan_root crates
add_scan_root std
add_scan_root tests
add_scan_root scratch
add_scan_root scripts
add_scan_root Makefile

if [[ "${#scan_roots[@]}" -gt 0 ]] && grep -RInE '\.tl\b|\.tli\b|\.tn\b|\.tni\b|\.ter\b|Some\("(tl|tn|tli|tni|ter)"\)|== "(tl|tn|tli|tni|ter)"|extension == "(tl|tn|tli|tni|ter)"' \
  --exclude-dir=target \
  --exclude-dir=terlan \
  --exclude-dir=otp \
  --exclude-dir=.git \
  --exclude=check_terlan_source_extensions.sh \
  "${scan_roots[@]}" >/tmp/terlan-source-extension-check.matches; then
  cat /tmp/terlan-source-extension-check.matches >&2
  printf 'source extension check failed: stale .tl/.tli/.tn/.tni/.ter source-extension reference remains.\n' >&2
  bad=1
fi

rm -f /tmp/terlan-source-extension-check.matches

if [[ "$bad" -ne 0 ]]; then
  exit 1
fi

printf 'Terlan source extension contract is canonical: .terl source, .terli interface.\n'
