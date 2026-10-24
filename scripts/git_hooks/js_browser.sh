#!/usr/bin/env bash
# Every commit executes the complete browser matrix against staged Terlan code.
set -euo pipefail
unset TERLAN_JS_MODULES
export TERLC="$CARGO_TARGET_DIR/release/terlc"
export CI=1
npm --prefix tests/js ci --ignore-scripts
node --test tests/js/*.test.mjs
mkdir -p target/js-browser
(
  cd tests/js
  npx --no-install playwright test --list --reporter=json > ../../target/js-browser/expected.json
  npx --no-install playwright test --forbid-only --retries=0 \
    --project=chromium --project=firefox --project=webkit
)
node tests/js/verify-results.mjs
