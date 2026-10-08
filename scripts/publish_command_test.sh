#!/usr/bin/env bash
# Execute the production Make entry point in private Git fixtures; never push.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
mkdir -p "$root/target"
temporary=$(mktemp -d "$root/target/publish-command-test.XXXXXX")
trap 'status=$?; if (( status != 0 )); then cat "$temporary"/*.log >&2; fi; rm -rf "$temporary"' EXIT
while IFS= read -r variable; do unset "$variable"; done < <(git rev-parse --local-env-vars)
for mode in ready failed dirty staged untracked drift head-drift version dry-run locked; do
  fixture="$temporary/$mode"
  git init --quiet --initial-branch=main "$fixture"
  (
    cd "$fixture"
    git config user.name 'Publication test'
    git config user.email 'publication@example.invalid'
    git config core.hooksPath /dev/null
    git config commit.gpgsign false
    printf '/target/\n' > .gitignore
    printf '[workspace.package]\nversion = "0.0.9"\n' > Cargo.toml
    printf 'SHELL := /bin/bash\nRELEASE_VERSION := 0.0.9\nTERLAN_RELEASE_CONTROL := bash submit.sh\n' > Makefile
    awk '/^publish-source-preflight:/ { active=1 } active && /^# Network\/tag/ { exit } active { print }' "$root/Makefile" >> Makefile
    awk '/^\.PHONY: publish publish-local / { active=1 } active && /^publish-local-check:/ { exit } active { print }' "$root/Makefile" >> Makefile
    cat >> Makefile <<'MAKE'
.PHONY: publish-local-check terlan-release-control-bootstrap
publish-local-check:
	@mkdir -p target
	@echo validate >> target/operations
	@test "$$PUBLISH_TEST_MODE" != failed
	@if test "$$PUBLISH_TEST_MODE" = drift; then echo changed >> Cargo.toml; fi
	@if test "$$PUBLISH_TEST_MODE" = head-drift; then git commit --quiet --allow-empty -m Changed; fi
terlan-release-control-bootstrap:
	@echo bootstrap >> target/operations
MAKE
    cat > submit.sh <<'SH'
set -eu
test "$1" = submit
test "$3" = "$(git rev-parse HEAD)"
printf 'submit\n' >> target/operations
SH
    git add .
    git commit --quiet -m Fixture
    version=0.0.9
    case "$mode" in
      dirty) echo changed >> Cargo.toml ;;
      staged) echo changed >> Cargo.toml; git add Cargo.toml ;;
      untracked) touch keep.txt ;;
      version) version=0.0.8 ;;
      locked) mkdir -p target/quality; exec 8>target/quality/release-control.lock; flock 8 ;;
    esac
    arguments=(--no-print-directory publish "VERSION=$version")
    if [[ "$mode" == dry-run ]]; then arguments=(-n "${arguments[@]}"); fi
    status=0
    PUBLISH_TEST_MODE="$mode" make "${arguments[@]}" > "$temporary/$mode.log" 2>&1 || status=$?
    operations=$(cat target/operations 2>/dev/null || true)
    case "$mode" in
      ready) test "$status" = 0; test "$operations" = $'validate\nbootstrap\nsubmit' ;;
      dry-run) test "$status" = 0; test -z "$operations"; test ! -e target/quality/release-control.lock ;;
      failed) test "$status" != 0; test "$operations" = validate ;;
      drift|head-drift) test "$status" != 0; test "$operations" = $'validate\nbootstrap' ;;
      *) test "$status" != 0; test -z "$operations" ;;
    esac
    if [[ "$mode" == untracked ]]; then test -f keep.txt; fi
  )
done
echo 'Publication command regressions passed (10 scenarios).'

# Exercise the hosted tag guard itself, including retries from a newer main.
# Checkout must use the named tag: the implicit event SHA flattens annotations.
grep -Fxq '          ref: refs/tags/${{ inputs.tag || github.ref_name }}' "$root/.github/workflows/publish.yml"
awk '/^      - name: Require an annotated version tag$/ { guard=1; next }
  guard && /^        run: \|$/ { body=1; next }
  body && /^          / { sub(/^          /, ""); print; next }
  body { exit }' "$root/.github/workflows/publish.yml" > "$temporary/tag-guard.sh"
test -s "$temporary/tag-guard.sh"
mkdir "$temporary/tag-bin"
cat > "$temporary/tag-bin/gh" <<'SH'
#!/bin/sh
test "$*" = 'auth setup-git --hostname github.com'
SH
chmod 700 "$temporary/tag-bin/gh"
for mode in annotated retry lightweight wrong-head wrong-event outside-main invalid-version; do
  fixture="$temporary/tag-$mode"
  git init --quiet --initial-branch=main "$fixture"
  (
    cd "$fixture"
    git config user.name 'Publication test'
    git config user.email 'publication@example.invalid'
    git config core.hooksPath /dev/null
    git config commit.gpgsign false
    git config tag.gpgsign false
    git commit --quiet --allow-empty -m Initial
    initial=$(git rev-parse HEAD)
    git commit --quiet --allow-empty -m Candidate
    revision=$(git rev-parse HEAD)
    git update-ref refs/remotes/origin/main "$revision"
    export RELEASE_TAG=v0.0.9 GITHUB_EVENT_NAME=push GITHUB_SHA="$revision" GITHUB_ENV="$fixture/environment"
    if [[ "$mode" == lightweight ]]; then git tag "$RELEASE_TAG";
    else git tag --annotate "$RELEASE_TAG" --message Release; fi
    case "$mode" in
      retry)
        git commit --quiet --allow-empty -m Workflow
        git update-ref refs/remotes/origin/main HEAD
        export GITHUB_EVENT_NAME=workflow_dispatch GITHUB_SHA="$(git rev-parse HEAD)"
        git checkout --quiet --detach "refs/tags/$RELEASE_TAG"
        ;;
      wrong-head) git checkout --quiet --detach "$initial" ;;
      wrong-event) export GITHUB_SHA="$initial" ;;
      outside-main) git update-ref refs/remotes/origin/main "$initial" ;;
      invalid-version) export RELEASE_TAG=v0.0.09 ;;
    esac
    status=0
    PATH="$temporary/tag-bin:$PATH" bash -euo pipefail "$temporary/tag-guard.sh" > "$temporary/tag-$mode.log" 2>&1 || status=$?
    case "$mode" in
      annotated|retry) test "$status" = 0; test "$(cat "$GITHUB_ENV")" = VERSION=0.0.9 ;;
      *) test "$status" != 0; test ! -e "$GITHUB_ENV" ;;
    esac
  )
done
echo 'Hosted tag guard regressions passed (7 scenarios).'

# With the built controller, also verify exact atomic ref updates using real Git.
if (( $# == 2 )); then
  vm=$1
  image=$2
  git init --bare --quiet --initial-branch=main "$temporary/remote.git"
  git init --quiet --initial-branch=main "$temporary/submission"
  (
    cd "$temporary/submission"
    git config user.name 'Publication test'
    git config user.email 'publication@example.invalid'
    git config core.hooksPath /dev/null
    git config commit.gpgsign false
    git config tag.gpgsign false
    printf '[workspace.package]\nversion = "0.0.9"\n' > Cargo.toml
    printf '/target/\n' > .gitignore
    git add .
    git commit --quiet -m Initial
    git remote add origin "$temporary/remote.git"
    git push --quiet origin main
    git commit --quiet --allow-empty -m Candidate
    revision=$(git rev-parse HEAD)
    mkdir "$temporary/bin"
    cat > "$temporary/bin/gh" <<'SH'
#!/bin/sh
case "$1 $2" in
  'auth status') exit 0 ;;
  'api repos/{owner}/{repo}/releases?per_page=100')
    echo '[{"tag_name":"v0.0.9","draft":false,"html_url":"https://github.com/fixture/repository/releases/tag/v0.0.9"}]' ;;
  *) exit 91 ;;
esac
SH
    chmod 700 "$temporary/bin/gh"
    PATH="$temporary/bin:$PATH" "$vm" run "$image" --script-eval -- submit 0.0.9 "$revision"
    test "$(git --git-dir="$temporary/remote.git" cat-file -t refs/tags/v0.0.9)" = tag
    test "$(git --git-dir="$temporary/remote.git" rev-parse 'refs/tags/v0.0.9^{commit}')" = "$revision"
    test "$(git --git-dir="$temporary/remote.git" rev-parse refs/heads/main)" = "$revision"
    tag_object=$(git rev-parse refs/tags/v0.0.9)
    PATH="$temporary/bin:$PATH" "$vm" run "$image" --script-eval -- submit 0.0.9 "$revision"
    test "$(git --git-dir="$temporary/remote.git" rev-parse refs/tags/v0.0.9)" = "$tag_object"
  )
  echo 'Real Git atomic submission and retry passed.'
fi
