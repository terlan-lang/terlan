#!/usr/bin/env bash
# Real formatter comparisons must preserve behavioral edits and string contents.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
checker="$root/scripts/git_hooks/formatting_only.sh"
terlc=${TERLC:-$root/target/debug/terlc}
case "$terlc" in /*) ;; *) terlc="$PWD/$terlc" ;; esac
mkdir -p "$root/target"
temporary=$(mktemp -d "$root/target/git-hooks-format-test.XXXXXX")
trap 'result=$?; if (( result != 0 )); then for log in "$temporary"/*.log; do if [[ -f "$log" ]]; then cat "$log" >&2; fi; done; fi; rm -r -- "$temporary"' EXIT
while IFS= read -r variable; do unset "$variable"; done < <(git rev-parse --local-env-vars)
cd "$temporary"
git init --quiet --initial-branch=main
git config user.name 'Hook formatting test'
git config user.email 'hooks@example.invalid'
git config core.hooksPath /dev/null
cat > Example.terl <<'SOURCE'
module hooks.Example.
pub value(): String ->
    String.concat([
        "a b",
        "c"
    ]).
SOURCE
printf 'invalid original\n' > Invalid.terl
git add .
git commit --quiet -m Initial
base=$(git rev-parse HEAD)
"$terlc" fmt --write Example.terl
bash "$checker" "$base" Example.terl "$terlc"

# A return-value change, including whitespace inside a literal, is semantic.
cp Example.terl canonical.terl
sed 's/a b/ab/' canonical.terl > Example.terl
if bash "$checker" "$base" Example.terl "$terlc"; then
  echo 'Hook classified a changed string value as formatting only' >&2; exit 1
fi
cp canonical.terl New.terl
if bash "$checker" "$base" New.terl "$terlc"; then
  echo 'Hook classified a new file as formatting only' >&2; exit 1
fi
if bash "$checker" "$base" Missing.terl "$terlc"; then
  echo 'Hook classified a missing file as formatting only' >&2; exit 1
fi
cp canonical.terl Invalid.terl
if bash "$checker" "$base" Invalid.terl "$terlc"; then
  echo 'Hook classified an invalid original as formatting only' >&2; exit 1
fi

# The clone's HEAD can differ from the candidate base; use the supplied base.
git add Example.terl
git commit --quiet -m Changed
cp canonical.terl Example.terl
bash "$checker" "$base" Example.terl "$terlc"
if bash "$checker" HEAD Example.terl "$terlc"; then
  echo 'Hook compared against a different Git base' >&2; exit 1
fi

# Exercise the real source gate with entries outside a package's source roots.
mkdir -p package/src/hooks package/scripts scripts/git_hooks
cp "$checker" scripts/git_hooks/formatting_only.sh
cat > package/terlan.toml <<'MANIFEST'
[package]
name = "hooks"
version = "0.0.1"
[build]
source_roots = ["src"]
artifact = "library"
MANIFEST
cat > package/src/hooks/Value.terl <<'SOURCE'
module hooks.Value.
pub text(): String -> "a b".
SOURCE
cat > package/src/hooks/ValueTest.terl <<'SOURCE'
module hooks.ValueTest.
import hooks.Value.
@test
pub source_value_matches(): Bool ->
    Value.text() ==
        "a b".
SOURCE
cat > package/scripts/Entry.terls <<'SOURCE'
import hooks.Value.
assert(
    Value.text() == "a b"
);
Unit.
SOURCE
git add package scripts
git commit --quiet -m Package
package_base=$(git rev-parse HEAD)
"$terlc" fmt --write package
printf '%s\n' package/scripts/Entry.terls package/src/hooks/ValueTest.terl > changed.txt
bash "$root/scripts/git_hooks/terlan.sh" changed.txt "$package_base" "$terlc" false > source-pass.log 2>&1
grep -F 'canonical formatting only: package/scripts/Entry.terls' source-pass.log
grep -F 'canonical formatting only: package/src/hooks/ValueTest.terl' source-pass.log

# A semantic test edit must still execute and reject the failing assertion.
sed 's/"a b"/"different"/' package/src/hooks/ValueTest.terl > changed-test.terl
cp changed-test.terl package/src/hooks/ValueTest.terl
printf '%s\n' package/src/hooks/ValueTest.terl > changed.txt
if bash "$root/scripts/git_hooks/terlan.sh" changed.txt "$package_base" "$terlc" false > source-fail.log 2>&1; then
  echo 'Hook did not execute the semantically changed failing test' >&2; exit 1
fi
grep -F 'test source_value_matches ... FAILED' source-fail.log
echo 'Git hook canonical formatting and package-entry tests passed (10 scenarios).'
