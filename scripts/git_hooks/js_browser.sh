#!/usr/bin/env bash
# Publication executes the complete browser matrix against validated Terlan code.
set -euo pipefail
unset TERLAN_JS_MODULES
export TERLC="$CARGO_TARGET_DIR/release/terlc"
export CI=1

hash_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{ print $1 }'
  else
    echo "JavaScript test caching requires sha256sum or shasum" >&2
    return 1
  fi
}

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

mkdir -p target/js-browser/cache
cache_key="$({
  printf '%s\n' 'terlan-js-browser-result-v1'
  git ls-files -z tests/js std/js scripts/git_hooks/js_browser.sh \
    | LC_ALL=C sort -z \
    | while IFS= read -r -d '' path; do hash_file "$path"; done
  hash_file "$TERLC"
  node --version
  npm --version
} | hash_stream)"
cache_marker="target/js-browser/cache/${cache_key}.passed"
if [[ -f "$cache_marker" && "${TERLAN_TEST_FORCE_RERUN:-0}" != "1" ]]; then
  echo "JavaScript and Playwright tests unchanged; reusing ${cache_key}"
  exit 0
fi

npm --prefix tests/js ci --ignore-scripts
node --test tests/js/*.test.mjs
(
  cd tests/js
  npx --no-install playwright test --list --reporter=json > ../../target/js-browser/expected.json
  npx --no-install playwright test --forbid-only --retries=0 \
    --project=chromium --project=firefox --project=webkit
)
node tests/js/verify-results.mjs
temporary_marker="${cache_marker}.$$.tmp"
printf '%s\n' 'terlan-js-browser-result-v1' > "$temporary_marker"
mv "$temporary_marker" "$cache_marker"
