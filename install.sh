#!/usr/bin/env sh
set -eu

VERSION="${TERLAN_VERSION:-v0.0.5}"
INSTALL_DIR="${TERLAN_INSTALL_DIR:-/usr/local/bin}"
RELEASE_BASE_URL="${TERLAN_RELEASE_BASE_URL:-https://github.com/terlan-lang/terlan/releases/download}"
DETECTED_OS="${TERLAN_INSTALL_OS:-$(uname -s)}"
DETECTED_ARCH="${TERLAN_INSTALL_ARCH:-$(uname -m)}"

case "$DETECTED_OS" in
  Linux)
    TERLAN_OS="linux"
    ;;
  Darwin)
    TERLAN_OS="macos"
    ;;
  *)
    echo "unsupported operating system for install.sh: $DETECTED_OS" >&2
    echo "Windows users should use install.ps1." >&2
    exit 1
    ;;
esac

case "$DETECTED_ARCH" in
  x86_64|amd64|AMD64)
    TERLAN_ARCH="x86_64"
    ;;
  aarch64|arm64|ARM64)
    TERLAN_ARCH="aarch64"
    ;;
  *)
    echo "unsupported architecture for install.sh: $DETECTED_ARCH" >&2
    exit 1
    ;;
esac

ARTIFACT="terlc-${TERLAN_OS}-${TERLAN_ARCH}.tar.gz"
URL="${RELEASE_BASE_URL}/${VERSION}/${ARTIFACT}"

if [ "${TERLAN_INSTALL_DRY_RUN:-0}" = "1" ]; then
  printf 'version=%s\n' "$VERSION"
  printf 'os=%s\n' "$TERLAN_OS"
  printf 'arch=%s\n' "$TERLAN_ARCH"
  printf 'artifact=%s\n' "$ARTIFACT"
  printf 'url=%s\n' "$URL"
  printf 'install_dir=%s\n' "$INSTALL_DIR"
  exit 0
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

cd "$TMP_DIR"
curl -fL "$URL" -o terlc.tar.gz
tar -xzf terlc.tar.gz
chmod +x terlc
if [ ! -f terlan-vm ]; then
  echo "release artifact $ARTIFACT did not contain terlan-vm" >&2
  exit 1
fi
chmod +x terlan-vm
mkdir -p "$INSTALL_DIR"

if [ -w "$INSTALL_DIR" ]; then
  mv terlc "$INSTALL_DIR/terlc"
  mv terlan-vm "$INSTALL_DIR/terlan-vm"
else
  sudo mv terlc "$INSTALL_DIR/terlc"
  sudo mv terlan-vm "$INSTALL_DIR/terlan-vm"
fi

"$INSTALL_DIR/terlc" --version
"$INSTALL_DIR/terlan-vm" --version
