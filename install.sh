#!/usr/bin/env sh
set -eu

VERSION="${TERLAN_VERSION:-v0.0.8}"
INSTALL_DIR="${TERLAN_INSTALL_DIR:-/usr/local/bin}"
SHARE_DIR="${TERLAN_INSTALL_SHARE_DIR:-$(dirname "$INSTALL_DIR")/share/terlan}"
RELEASE_BASE_URL="${TERLAN_RELEASE_BASE_URL:-https://github.com/terlan-lang/terlan/releases/download}"
DETECTED_OS="${TERLAN_INSTALL_OS:-$(uname -s)}"
DETECTED_ARCH="${TERLAN_INSTALL_ARCH:-$(uname -m)}"

if ! printf '%s\n' "$VERSION" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$'; then
  echo "invalid Terlan release version: $VERSION" >&2
  exit 1
fi

case "$INSTALL_DIR" in
  /*) ;;
  *)
    echo "TERLAN_INSTALL_DIR must be an absolute path: $INSTALL_DIR" >&2
    exit 1
    ;;
esac
case "$SHARE_DIR" in
  /*) ;;
  *)
    echo "TERLAN_INSTALL_SHARE_DIR must be an absolute path: $SHARE_DIR" >&2
    exit 1
    ;;
esac
for destination in "$INSTALL_DIR" "$SHARE_DIR"; do
  case "$destination" in
    *//*|*/|*/./*|*/.|*/../*|*/..)
      echo "installer destinations must not contain redundant or parent path segments: $destination" >&2
      exit 1
      ;;
  esac
done
if [ "$INSTALL_DIR" = "/" ] || [ "$SHARE_DIR" = "/" ] || [ "$INSTALL_DIR" = "$SHARE_DIR" ]; then
  echo "installer destinations must be distinct, non-root directories" >&2
  exit 1
fi
case "$INSTALL_DIR/" in
  "$SHARE_DIR/"*)
    echo "TERLAN_INSTALL_SHARE_DIR must not contain TERLAN_INSTALL_DIR" >&2
    exit 1
    ;;
esac

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
  printf 'share_dir=%s\n' "$SHARE_DIR"
  exit 0
fi

USE_SUDO=0
if ! mkdir -p "$INSTALL_DIR" "$SHARE_DIR" 2>/dev/null; then
  USE_SUDO=1
fi
as_root() {
  if [ "$USE_SUDO" = "1" ]; then
    sudo "$@"
  else
    "$@"
  fi
}
as_root mkdir -p "$INSTALL_DIR" "$SHARE_DIR"
INSTALL_DIR="$(cd "$INSTALL_DIR" && pwd -P)"
SHARE_DIR="$(cd "$SHARE_DIR" && pwd -P)"
if [ "$INSTALL_DIR" = "/" ] || [ "$SHARE_DIR" = "/" ] || [ "$INSTALL_DIR" = "$SHARE_DIR" ]; then
  echo "resolved installer destinations must be distinct, non-root directories" >&2
  exit 1
fi
case "$INSTALL_DIR/" in
  "$SHARE_DIR/"*)
    echo "resolved TERLAN_INSTALL_SHARE_DIR must not contain TERLAN_INSTALL_DIR" >&2
    exit 1
    ;;
esac
TMP_DIR="$(mktemp -d)"
INSTALL_STARTED=0
HAD_COMPILER=0
HAD_VM=0
HAD_NATIVE_WORKER=0
HAD_LSP=0
HAD_SHARE=0
finish() {
  status=$?
  trap - EXIT INT TERM
  if [ "$status" -ne 0 ] && [ "$INSTALL_STARTED" = "1" ]; then
    as_root rm -f "$INSTALL_DIR/terlc" "$INSTALL_DIR/terlan-vm" "$INSTALL_DIR/terlan-native-worker" "$INSTALL_DIR/terlan-lsp"
    as_root rm -rf "$SHARE_DIR"
    if [ "$HAD_COMPILER" = "1" ]; then as_root cp "$TMP_DIR/backup/terlc" "$INSTALL_DIR/terlc"; fi
    if [ "$HAD_VM" = "1" ]; then as_root cp "$TMP_DIR/backup/terlan-vm" "$INSTALL_DIR/terlan-vm"; fi
    if [ "$HAD_NATIVE_WORKER" = "1" ]; then as_root cp "$TMP_DIR/backup/terlan-native-worker" "$INSTALL_DIR/terlan-native-worker"; fi
    if [ "$HAD_LSP" = "1" ]; then as_root cp "$TMP_DIR/backup/terlan-lsp" "$INSTALL_DIR/terlan-lsp"; fi
    if [ "$HAD_SHARE" = "1" ]; then
      as_root mkdir -p "$SHARE_DIR"
      as_root cp -R "$TMP_DIR/backup/share/." "$SHARE_DIR/"
    fi
    echo "Terlan install failed; previous installation restored." >&2
  fi
  rm -rf "$TMP_DIR"
  exit "$status"
}
trap finish EXIT INT TERM

cd "$TMP_DIR"
fetch() {
  case "$1" in
    https://*)
      curl --fail --location --proto '=https' --tlsv1.2 \
        --retry 4 --retry-all-errors --connect-timeout 15 --max-time 600 \
        "$1" -o "$2"
      ;;
    file://*)
      curl --fail --location "$1" -o "$2"
      ;;
    *)
      echo "release URL must use https:// or file://: $1" >&2
      exit 1
      ;;
  esac
}
fetch "$URL" "$ARTIFACT"
fetch "$URL.sha256" "$ARTIFACT.sha256"
expected_checksum="$({
  awk -v expected="$ARTIFACT" '
    NF == 2 && length($1) == 64 && $2 == expected && NR == 1 { digest = $1 }
    END { if (NR == 1 && digest != "") print digest; else exit 1 }
  ' "$ARTIFACT.sha256"
} || true)"
case "$expected_checksum" in
  *[!0-9a-fA-F]*|'')
    echo "invalid SHA-256 file for $ARTIFACT" >&2
    exit 1
    ;;
esac
if [ "${#expected_checksum}" -ne 64 ]; then
  echo "invalid SHA-256 length for $ARTIFACT" >&2
  exit 1
fi
expected_checksum="$(printf '%s' "$expected_checksum" | tr 'A-F' 'a-f')"
if command -v sha256sum >/dev/null 2>&1; then
  actual_checksum="$(sha256sum "$ARTIFACT" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual_checksum="$(shasum -a 256 "$ARTIFACT" | awk '{ print $1 }')"
else
  echo "installer requires sha256sum or shasum to verify $ARTIFACT" >&2
  exit 1
fi
if [ "$actual_checksum" != "$expected_checksum" ]; then
  echo "checksum verification failed for $ARTIFACT" >&2
  exit 1
fi
if tar -tzf "$ARTIFACT" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  echo "release artifact contains an unsafe path: $ARTIFACT" >&2
  exit 1
fi
if tar -tvzf "$ARTIFACT" | awk 'substr($1, 1, 1) != "-" && substr($1, 1, 1) != "d" { found = 1 } END { exit !found }'; then
  echo "release artifact contains a link or special filesystem entry: $ARTIFACT" >&2
  exit 1
fi
tar -xzf "$ARTIFACT"
chmod +x terlc
if [ ! -f terlan-vm ]; then
  echo "release artifact $ARTIFACT did not contain terlan-vm" >&2
  exit 1
fi
chmod +x terlan-vm
if [ ! -f terlan-native-worker ]; then
  echo "release artifact $ARTIFACT did not contain terlan-native-worker" >&2
  exit 1
fi
chmod +x terlan-native-worker
if [ ! -f terlan-lsp ]; then
  echo "release artifact $ARTIFACT did not contain terlan-lsp" >&2
  exit 1
fi
chmod +x terlan-lsp
for required in share/terlan/std share/terlan/editors/vscode share/terlan/tree-sitter-terlan share/terlan/runtime/release-self-test.tvm SHA256SUMS terlan-install-manifest.json terlan-release.json; do
  if [ ! -e "$required" ]; then
    echo "release artifact $ARTIFACT did not contain $required" >&2
    exit 1
  fi
done
internal_paths="$TMP_DIR/internal-checksum-paths"
: > "$internal_paths"
internal_count=0
while IFS= read -r row || [ -n "$row" ]; do
  digest=${row%%  *}
  relative=${row#*  }
  if [ "$digest" = "$row" ] || [ "$relative" = "$row" ]; then
    echo "release artifact contains an invalid SHA256SUMS row" >&2
    exit 1
  fi
  case "$digest" in
    *[!0-9a-fA-F]*|'')
      echo "release artifact contains an invalid SHA256SUMS digest" >&2
      exit 1
      ;;
  esac
  if [ "${#digest}" -ne 64 ]; then
    echo "release artifact contains an invalid SHA256SUMS digest length" >&2
    exit 1
  fi
  case "$relative" in
    ''|/*|../*|*/../*|*/..|..|*\\*)
      echo "SHA256SUMS contains an unsafe path: $relative" >&2
      exit 1
      ;;
  esac
  if [ ! -f "$relative" ] || [ -L "$relative" ]; then
    echo "SHA256SUMS references a missing or unsafe file: $relative" >&2
    exit 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    internal_actual="$(sha256sum "$relative" | awk '{ print $1 }')"
  else
    internal_actual="$(shasum -a 256 "$relative" | awk '{ print $1 }')"
  fi
  digest="$(printf '%s' "$digest" | tr 'A-F' 'a-f')"
  if [ "$internal_actual" != "$digest" ]; then
    echo "internal checksum verification failed for $relative" >&2
    exit 1
  fi
  printf '%s\n' "$relative" >> "$internal_paths"
  internal_count=$((internal_count + 1))
done < SHA256SUMS
if [ "$internal_count" -eq 0 ]; then
  echo "release artifact contains an empty SHA256SUMS manifest" >&2
  exit 1
fi
if [ -n "$(LC_ALL=C sort "$internal_paths" | uniq -d)" ]; then
  echo "release artifact contains duplicate SHA256SUMS paths" >&2
  exit 1
fi

mkdir -p "$TMP_DIR/backup"
if [ -f "$INSTALL_DIR/terlc" ]; then cp "$INSTALL_DIR/terlc" "$TMP_DIR/backup/terlc"; HAD_COMPILER=1; fi
if [ -f "$INSTALL_DIR/terlan-vm" ]; then cp "$INSTALL_DIR/terlan-vm" "$TMP_DIR/backup/terlan-vm"; HAD_VM=1; fi
if [ -f "$INSTALL_DIR/terlan-native-worker" ]; then cp "$INSTALL_DIR/terlan-native-worker" "$TMP_DIR/backup/terlan-native-worker"; HAD_NATIVE_WORKER=1; fi
if [ -f "$INSTALL_DIR/terlan-lsp" ]; then cp "$INSTALL_DIR/terlan-lsp" "$TMP_DIR/backup/terlan-lsp"; HAD_LSP=1; fi
if [ -d "$SHARE_DIR" ]; then cp -R "$SHARE_DIR" "$TMP_DIR/backup/share"; HAD_SHARE=1; fi

as_root mkdir -p "$INSTALL_DIR" "$SHARE_DIR"
INSTALL_STARTED=1
as_root cp terlc "$INSTALL_DIR/terlc"
as_root cp terlan-vm "$INSTALL_DIR/terlan-vm"
as_root cp terlan-native-worker "$INSTALL_DIR/terlan-native-worker"
as_root cp terlan-lsp "$INSTALL_DIR/terlan-lsp"
as_root chmod 0755 "$INSTALL_DIR/terlc" "$INSTALL_DIR/terlan-vm" "$INSTALL_DIR/terlan-native-worker" "$INSTALL_DIR/terlan-lsp"
as_root rm -rf "$SHARE_DIR/std" "$SHARE_DIR/editors" "$SHARE_DIR/tree-sitter-terlan"
as_root cp -R share/terlan/. "$SHARE_DIR/"
as_root cp terlan-release.json SHA256SUMS terlan-install-manifest.json "$SHARE_DIR/"

"$INSTALL_DIR/terlc" --version
"$INSTALL_DIR/terlan-vm" --version
"$INSTALL_DIR/terlan-vm" validate-package "$SHARE_DIR"
"$INSTALL_DIR/terlan-native-worker" --version
"$INSTALL_DIR/terlan-lsp" --help >/dev/null
INSTALL_STARTED=0
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Add $INSTALL_DIR to PATH to run terlc from your shell." ;;
esac
