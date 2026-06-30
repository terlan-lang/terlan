#!/usr/bin/env sh
set -eu

VERSION="${TERLAN_VERSION:-v0.0.5}"
INSTALL_DIR="${TERLAN_INSTALL_DIR:-/usr/local/bin}"
INSTALL_PREFIX="$(dirname "$INSTALL_DIR")"
LIB_DIR="${TERLAN_INSTALL_LIB_DIR:-${INSTALL_PREFIX}/lib/terlan}"
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
  printf 'lib_dir=%s\n' "$LIB_DIR"
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
if [ ! -d experimental/terlan-vm ]; then
  echo "release artifact $ARTIFACT did not contain experimental/terlan-vm" >&2
  exit 1
fi
mkdir -p "$INSTALL_DIR"

if [ -w "$INSTALL_DIR" ]; then
  mv terlc "$INSTALL_DIR/terlc"
else
  sudo mv terlc "$INSTALL_DIR/terlc"
fi

if [ -d "$LIB_DIR" ] && [ -w "$LIB_DIR" ]; then
  rm -rf "$LIB_DIR/experimental/terlan-vm"
  mkdir -p "$LIB_DIR/experimental"
  cp -R experimental/terlan-vm "$LIB_DIR/experimental/terlan-vm"
elif [ ! -e "$LIB_DIR" ] && mkdir -p "$LIB_DIR" 2>/dev/null; then
  mkdir -p "$LIB_DIR/experimental"
  cp -R experimental/terlan-vm "$LIB_DIR/experimental/terlan-vm"
else
  sudo rm -rf "$LIB_DIR/experimental/terlan-vm"
  sudo mkdir -p "$LIB_DIR/experimental"
  sudo cp -R experimental/terlan-vm "$LIB_DIR/experimental/terlan-vm"
fi

"$INSTALL_DIR/terlc" --version
"$INSTALL_DIR/terlc" --experimental otp-runtime version
