#!/usr/bin/env bash
# Compare staged source with the canonical formatting of its exact Git base.
set -euo pipefail
base=$1
path=$2
terlc=$3
case "$path" in
  *.terl|*.terli|*.terls) ;;
  *) exit 1 ;;
esac
test -f "$path"
mkdir -p target
temporary=$(mktemp -d "$PWD/target/git-hooks-format.XXXXXX")
original="$temporary/source.${path##*.}"
trap 'rm -- "$original"; rmdir -- "$temporary"' EXIT
# Missing/new paths and invalid originals retain the ordinary test path.
git show "$base:$path" > "$original" 2>/dev/null || exit 1
"$terlc" fmt --write "$original" >/dev/null 2>&1 || exit 1
cmp -s -- "$original" "$path"
