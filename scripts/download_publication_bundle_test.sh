#!/usr/bin/env bash
set -euo pipefail

repository=$(git rev-parse --show-toplevel)
mkdir -p "$repository/target"
fixture=$(mktemp -d "$repository/target/publication-bundle-test.XXXXXX")
trap 'rm -rf -- "$fixture"' EXIT
mkdir -p "$fixture/scripts" "$fixture/bin" "$fixture/source/dist"
cp "$repository/scripts/download_publication_bundle.sh" "$fixture/scripts/"
printf archive > "$fixture/source/dist/terlc-linux-x86_64.tar.gz"
printf candidate > "$fixture/source/dist/release-candidate.json"
archive_size=$(wc -c < "$fixture/source/dist/terlc-linux-x86_64.tar.gz" | tr -d ' ')
candidate_size=$(wc -c < "$fixture/source/dist/release-candidate.json" | tr -d ' ')
archive_hash=$(sha256sum "$fixture/source/dist/terlc-linux-x86_64.tar.gz" | awk '{print $1}')
candidate_hash=$(sha256sum "$fixture/source/dist/release-candidate.json" | awk '{print $1}')
cat > "$fixture/source/publication-plan.json" <<JSON
{"schema":"terlan.publication-plan.v1","version":"0.0.10","source_revision":"aaaaaaaaaaaa","candidate_seal":"sha256:$candidate_hash","artifacts":[{"path":"dist/terlc-linux-x86_64.tar.gz","size_bytes":$archive_size,"sha256":"sha256:$archive_hash"},{"path":"dist/release-candidate.json","size_bytes":$candidate_size,"sha256":"sha256:$candidate_hash"}]}
JSON
(
  cd "$fixture/source"
  find . -type f ! -name bundle.sha256 -print0 | LC_ALL=C sort -z | xargs -0 sha256sum > bundle.sha256
)
cat > "$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$1 $2" in
  'repo view') echo fixture/repository ;;
  'api repos/fixture/repository/commits/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/status')
    echo '{"state":"success","target_url":"https://github.com/fixture/repository/actions/runs/21"}'
    ;;
  'api repos/fixture/repository/actions/runs/21')
    echo '{"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","conclusion":"success","path":".github/workflows/release.yml"}'
    ;;
  'run download')
    destination=''
    while (( $# > 0 )); do
      if [[ "$1" == --dir ]]; then destination=$2; break; fi
      shift
    done
    cp -a "$PUBLICATION_BUNDLE_SOURCE/." "$destination/"
    ;;
  'attestation verify') printf 'attest\n' >> "$PUBLICATION_BUNDLE_CALLS" ;;
  *) echo "unexpected gh invocation: $*" >&2; exit 91 ;;
esac
SH
chmod 700 "$fixture/bin/gh"
(
  cd "$fixture"
  PATH="$fixture/bin:$PATH" \
    PUBLICATION_BUNDLE_SOURCE="$fixture/source" \
    PUBLICATION_BUNDLE_CALLS="$fixture/calls" \
    bash scripts/download_publication_bundle.sh aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
)
test "$(wc -l < "$fixture/calls" | tr -d ' ')" = 3
cmp "$fixture/source/publication-plan.json" "$fixture/target/publication-bundle/publication-plan.json"
cmp "$fixture/source/dist/terlc-linux-x86_64.tar.gz" "$fixture/dist/terlc-linux-x86_64.tar.gz"

printf corrupt >> "$fixture/source/dist/release-candidate.json"
if (
  cd "$fixture"
  PATH="$fixture/bin:$PATH" \
    PUBLICATION_BUNDLE_SOURCE="$fixture/source" \
    PUBLICATION_BUNDLE_CALLS="$fixture/calls" \
    bash scripts/download_publication_bundle.sh aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
) >/dev/null 2>&1; then
  echo 'corrupt publication bundle was accepted' >&2
  exit 1
fi

echo 'Publication bundle download regressions passed.'
