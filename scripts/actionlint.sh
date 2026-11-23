#!/usr/bin/env bash
set -euo pipefail

version=1.7.12
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) platform=linux_amd64; expected=8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8 ;;
  Linux:aarch64) platform=linux_arm64; expected=325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6 ;;
  Darwin:x86_64) platform=darwin_amd64; expected=5b44c3bc2255115c9b69e30efc0fecdf498fdb63c5d58e17084fd5f16324c644 ;;
  Darwin:arm64) platform=darwin_arm64; expected=aba9ced2dee8d27fecca3dc7feb1a7f9a52caefa1eb46f3271ea66b6e0e6953f ;;
  *) echo "actionlint does not provide a pinned archive for $(uname -s)/$(uname -m)" >&2; exit 1 ;;
esac

archive="actionlint_${version}_${platform}.tar.gz"
cache_dir="${TERLAN_ACTIONLINT_CACHE_DIR:-target/tools/actionlint}"
archive_path="$cache_dir/$archive"
mkdir -p "$cache_dir"

if [[ ! -f "$archive_path" ]]; then
  partial=$(mktemp "$cache_dir/.actionlint.XXXXXX")
  trap 'rm -f "$partial"' EXIT
  curl --fail --location --silent --show-error \
    "https://github.com/rhysd/actionlint/releases/download/v${version}/${archive}" \
    --output "$partial"
  mv "$partial" "$archive_path"
  trap - EXIT
fi

if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$archive_path" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$archive_path" | awk '{print $1}')
fi
[[ "$actual" == "$expected" ]] || {
  echo "actionlint archive checksum mismatch: $archive_path" >&2
  exit 1
}

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/terlan-actionlint.XXXXXX")
trap 'rm -rf "$run_dir"' EXIT
tar -xzf "$archive_path" -C "$run_dir" actionlint
"$run_dir/actionlint" "$@"
