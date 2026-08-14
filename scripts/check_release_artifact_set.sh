#!/usr/bin/env bash
set -euo pipefail

artifact_root="${1:-}"
mode="${2:-}"

expected=(
  terlc-linux-aarch64.tar.gz
  terlc-linux-aarch64.tar.gz.sha256
  terlc-linux-x86_64.tar.gz
  terlc-linux-x86_64.tar.gz.sha256
  terlc-macos-aarch64.tar.gz
  terlc-macos-aarch64.tar.gz.sha256
  terlc-macos-x86_64.tar.gz
  terlc-macos-x86_64.tar.gz.sha256
  terlc-windows-aarch64.zip
  terlc-windows-aarch64.zip.sha256
  terlc-windows-x86_64.zip
  terlc-windows-x86_64.zip.sha256
)

if [[ "$mode" == "--with-local-payload" ]]; then
  expected+=(
    SHA256SUMS
    terlan-install-manifest.json
    terlan-lsp
    terlan-native-worker
    terlan-release.json
    terlan-vm
    terlc
  )
elif [[ -n "$mode" ]]; then
  echo "usage: scripts/check_release_artifact_set.sh <artifact-directory> [--with-local-payload]|self-test" >&2
  exit 2
fi

if [[ "$artifact_root" == "self-test" ]]; then
  fixture="$(mktemp -d)"
  trap 'rm -rf "$fixture"' EXIT
  for archive in "${expected[@]}"; do
    [[ "$archive" == *.sha256 ]] && continue
    printf 'fixture:%s\n' "$archive" > "$fixture/$archive"
    if command -v sha256sum >/dev/null 2>&1; then
      digest="$(sha256sum "$fixture/$archive" | awk '{ print $1 }')"
    else
      digest="$(shasum -a 256 "$fixture/$archive" | awk '{ print $1 }')"
    fi
    printf '%s  %s\n' "$digest" "$archive" > "$fixture/$archive.sha256"
  done
  "$0" "$fixture" >/dev/null
  printf 'unexpected\n' > "$fixture/unexpected.txt"
  if "$0" "$fixture" >/dev/null 2>&1; then
    echo "release artifact set self-test accepted an unexpected file" >&2
    exit 1
  fi
  rm -f "$fixture/unexpected.txt"
  for payload in SHA256SUMS terlan-install-manifest.json terlan-lsp terlan-native-worker terlan-release.json terlan-vm terlc; do
    printf 'fixture:%s\n' "$payload" > "$fixture/$payload"
  done
  "$0" "$fixture" --with-local-payload >/dev/null
  rm -f "$fixture/SHA256SUMS" "$fixture/terlan-install-manifest.json" \
    "$fixture/terlan-lsp" "$fixture/terlan-native-worker" \
    "$fixture/terlan-release.json" "$fixture/terlan-vm" "$fixture/terlc"
  printf 'corrupt\n' >> "$fixture/terlc-linux-x86_64.tar.gz"
  if "$0" "$fixture" >/dev/null 2>&1; then
    echo "release artifact set self-test accepted a corrupted archive" >&2
    exit 1
  fi
  echo "release artifact set self-test passed"
  exit 0
fi

if [[ -z "$artifact_root" || ! -d "$artifact_root" ]]; then
  echo "usage: scripts/check_release_artifact_set.sh <artifact-directory> [--with-local-payload]|self-test" >&2
  exit 2
fi

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
printf '%s\n' "${expected[@]}" | LC_ALL=C sort > "$work_dir/expected"
find "$artifact_root" -mindepth 1 -maxdepth 1 -exec basename {} \; \
  | LC_ALL=C sort > "$work_dir/actual"

if ! cmp -s "$work_dir/expected" "$work_dir/actual"; then
  echo "release artifact set is incomplete or contains unexpected archives" >&2
  diff -u "$work_dir/expected" "$work_dir/actual" >&2 || true
  exit 1
fi

if find "$artifact_root" -mindepth 1 -maxdepth 1 ! -type f | grep -q .; then
  echo "release artifact set must contain regular files only" >&2
  exit 1
fi

for artifact in "${expected[@]}"; do
  [[ -s "$artifact_root/$artifact" ]] || {
    echo "release artifact is empty: $artifact" >&2
    exit 1
  }
done

for archive in "${expected[@]}"; do
  [[ "$archive" == *.sha256 ]] && continue
  [[ "$archive" == terlc-*.tar.gz || "$archive" == terlc-*.zip ]] || continue
  row="$(cat "$artifact_root/$archive.sha256")"
  if [[ ! "$row" =~ ^([0-9a-f]{64})\ \ ([^/]+)$ ]] \
    || [[ "${BASH_REMATCH[2]}" != "$archive" ]]; then
    echo "release checksum sidecar is malformed: $archive.sha256" >&2
    exit 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$artifact_root/$archive" | awk '{ print $1 }')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$artifact_root/$archive" | awk '{ print $1 }')"
  else
    echo "release artifact validation requires sha256sum or shasum" >&2
    exit 127
  fi
  if [[ "$actual" != "${BASH_REMATCH[1]}" ]]; then
    echo "release checksum does not match archive: $archive" >&2
    exit 1
  fi
done

echo "release artifact set passed"
