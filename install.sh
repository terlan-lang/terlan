#!/usr/bin/env sh
set -eu

VERSION="${TERLAN_VERSION:-v0.0.2}"
INSTALL_DIR="${TERLAN_INSTALL_DIR:-/usr/local/bin}"
ARTIFACT="terlc-linux-x86_64.tar.gz"
URL="https://github.com/terlan-lang/terlan/releases/download/${VERSION}/${ARTIFACT}"

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

cd "$TMP_DIR"
curl -L "$URL" -o terlc.tar.gz
tar -xzf terlc.tar.gz
chmod +x terlc

if [ -w "$INSTALL_DIR" ]; then
  mv terlc "$INSTALL_DIR/terlc"
else
  sudo mv terlc "$INSTALL_DIR/terlc"
fi

terlc version
