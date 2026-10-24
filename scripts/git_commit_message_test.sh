#!/usr/bin/env bash
# Exercise the policy through real commits, with no compiler or browser builds.
set -euo pipefail
hook="$(cd "$(dirname "$0")/../.githooks" && pwd)/commit-msg"
mkdir -p target
temporary=$(mktemp -d "$(pwd)/target/commit-message-test.XXXXXX")
trap 'result=$?; if (( result != 0 )); then cat "$temporary"/*.log >&2; fi; rm -rf "$temporary"' EXIT
while IFS= read -r variable; do unset "$variable"; done < <(git rev-parse --local-env-vars)
git init --quiet --initial-branch=main "$temporary/repo"
cd "$temporary/repo"
git config user.name 'Commit message test'
git config user.email 'hooks@example.invalid'
mkdir hooks
cp "$hook" hooks/commit-msg
chmod +x hooks/commit-msg
git config core.hooksPath "$PWD/hooks"

accept_message() {
  printf '%s\n' "$1" > "$temporary/message"
  git -c commit.gpgSign=false commit --quiet --allow-empty --file "$temporary/message" > "$temporary/accept.log" 2>&1
  test "$(git log -1 --format=%B)" = "$1"
}

reject_message() {
  local before
  before=$(git rev-parse HEAD)
  printf '%s\n' "$1" > "$temporary/message"
  if git -c commit.gpgSign=false commit --quiet --allow-empty --file "$temporary/message" > "$temporary/reject.log" 2>&1; then
    echo 'Commit hook accepted a long or multiline message' >&2
    exit 1
  fi
  test "$(git rev-parse HEAD)" = "$before"
  grep -Fq 'one nonempty line of at most 50 characters' "$temporary/reject.log"
}

accept_message 'Build one executable'
accept_message '#123 Fix application startup'
accept_message "$(printf '%050d' 0)"
reject_message "$(printf '%051d' 0)"
reject_message 'Test JavaScript bindings in browsers and fix recursive type-checking timeouts'
reject_message $'Short subject\n\nA body is too much detail for a commit message.'
reject_message $'Short subject\n\n# This is literal message text with -F.'
reject_message ''

# Editor comments do not become part of the committed message.
cat > "$temporary/editor" <<'EDITOR'
#!/usr/bin/env bash
printf 'Keep messages short\n\n# Git status comments\n' > "$1"
EDITOR
chmod +x "$temporary/editor"
GIT_EDITOR="$temporary/editor" git -c commit.gpgSign=false commit --quiet --allow-empty > "$temporary/editor.log" 2>&1
test "$(git log -1 --format=%B)" = 'Keep messages short'

# Verbose editor diffs and a custom comment character are Git metadata.
cat > "$temporary/editor" <<'EDITOR'
#!/usr/bin/env bash
{ printf 'Keep verbose commits short\n'; cat "$1"; } > "$1.new"
mv "$1.new" "$1"
EDITOR
printf 'changed\n' > tracked.txt
git add tracked.txt
GIT_EDITOR="$temporary/editor" git -c commit.gpgSign=false -c core.commentChar=';' commit --quiet --verbose > "$temporary/verbose.log" 2>&1
test "$(git log -1 --format=%B)" = 'Keep verbose commits short'
echo 'Commit message regression tests passed (10 scenarios).'
