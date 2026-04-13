#!/bin/sh
# Install numi — deterministic resource code generator for Apple projects.
# Usage: curl -fsSL https://numi.elata.ai/install.sh | sh
set -eu

REPO="oops-rs/numi"
INSTALL_DIR="${NUMI_INSTALL_DIR:-/usr/local/bin}"
GITHUB_API="https://api.github.com"
GITHUB_DL="https://github.com"

main() {
  detect_platform
  fetch_latest_version
  download_and_install
  verify_installation
}

detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Linux)  TARGET_OS="unknown-linux-gnu" ;;
    Darwin) TARGET_OS="apple-darwin" ;;
    *)
      err "unsupported operating system: $OS"
      ;;
  esac

  case "$ARCH" in
    x86_64|amd64)  TARGET_ARCH="x86_64" ;;
    arm64|aarch64) TARGET_ARCH="aarch64" ;;
    *)
      err "unsupported architecture: $ARCH"
      ;;
  esac

  TARGET="${TARGET_ARCH}-${TARGET_OS}"
  log "detected platform: $TARGET"
}

fetch_latest_version() {
  if [ -n "${NUMI_VERSION:-}" ]; then
    VERSION="$NUMI_VERSION"
    log "using requested version: $VERSION"
    return
  fi

  log "fetching latest release..."
  VERSION="$(
    curl -fsSL "$GITHUB_API/repos/$REPO/releases/latest" \
      | grep '"tag_name"' \
      | head -1 \
      | sed 's/.*"tag_name": *"//;s/".*//'
  )"

  if [ -z "$VERSION" ]; then
    err "could not determine latest version — set NUMI_VERSION to install a specific release"
  fi

  log "latest version: $VERSION"
}

download_and_install() {
  ASSET="numi-${VERSION}-${TARGET}.tar.gz"
  CHECKSUM_ASSET="${ASSET}.sha256"
  DOWNLOAD_URL="$GITHUB_DL/$REPO/releases/download/$VERSION/$ASSET"
  CHECKSUM_URL="$GITHUB_DL/$REPO/releases/download/$VERSION/$CHECKSUM_ASSET"

  TMPDIR="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR"' EXIT

  log "downloading $ASSET..."
  curl -fsSL -o "$TMPDIR/$ASSET" "$DOWNLOAD_URL"

  log "downloading checksum..."
  if curl -fsSL -o "$TMPDIR/$CHECKSUM_ASSET" "$CHECKSUM_URL" 2>/dev/null; then
    verify_checksum "$TMPDIR" "$ASSET" "$CHECKSUM_ASSET"
  else
    warn "checksum file not available — skipping verification"
  fi

  log "extracting..."
  tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

  BINARY="$TMPDIR/numi-${VERSION}-${TARGET}/numi"
  if [ ! -f "$BINARY" ]; then
    err "expected binary not found in archive: numi-${VERSION}-${TARGET}/numi"
  fi

  if [ -w "$INSTALL_DIR" ]; then
    mv "$BINARY" "$INSTALL_DIR/numi"
  else
    log "installing to $INSTALL_DIR (requires sudo)..."
    sudo mv "$BINARY" "$INSTALL_DIR/numi"
  fi

  chmod +x "$INSTALL_DIR/numi"
}

verify_checksum() {
  DIR="$1"
  ARCHIVE="$2"
  CHECKSUMFILE="$3"

  EXPECTED="$(awk '{print $1}' "$DIR/$CHECKSUMFILE")"

  if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL="$(sha256sum "$DIR/$ARCHIVE" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    ACTUAL="$(shasum -a 256 "$DIR/$ARCHIVE" | awk '{print $1}')"
  else
    warn "no sha256 tool found — skipping checksum verification"
    return
  fi

  if [ "$EXPECTED" != "$ACTUAL" ]; then
    err "checksum mismatch: expected $EXPECTED, got $ACTUAL"
  fi

  log "checksum verified"
}

verify_installation() {
  if command -v numi >/dev/null 2>&1; then
    log "installed numi $(numi --version 2>/dev/null || echo "$VERSION") to $INSTALL_DIR/numi"
  else
    warn "numi was installed to $INSTALL_DIR/numi but is not on your PATH"
    warn "add $INSTALL_DIR to your PATH to use it"
  fi
}

log()  { printf '\033[1;32m>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m>\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

main
