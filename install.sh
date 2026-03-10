#!/bin/sh
# Install filament (fl) — https://github.com/JYC11/filament
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/JYC11/filament/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/JYC11/filament/main/install.sh | sh -s -- --to /usr/local/bin
#   curl -fsSL https://raw.githubusercontent.com/JYC11/filament/main/install.sh | sh -s -- --version v1.0.0

set -eu

REPO="JYC11/filament"
BINARY="fl"
INSTALL_DIR="$HOME/.local/bin"
VERSION=""

usage() {
  echo "Usage: install.sh [--to DIR] [--version TAG]"
  echo ""
  echo "Options:"
  echo "  --to DIR       Install directory (default: ~/.local/bin)"
  echo "  --version TAG  Specific version tag (default: latest)"
  exit 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    --to)      INSTALL_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --help|-h) usage ;;
    *)         echo "Unknown option: $1"; usage ;;
  esac
done

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)  os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *)      echo "Error: unsupported OS: $os"; exit 1 ;;
  esac

  case "$arch" in
    x86_64|amd64)  arch_part="x86_64" ;;
    aarch64|arm64) arch_part="aarch64" ;;
    *)             echo "Error: unsupported architecture: $arch"; exit 1 ;;
  esac

  echo "${arch_part}-${os_part}"
}

get_latest_version() {
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
  elif command -v wget > /dev/null 2>&1; then
    wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
  else
    echo "Error: curl or wget required" >&2
    exit 1
  fi
}

download() {
  url="$1"
  output="$2"
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL -o "$output" "$url"
  elif command -v wget > /dev/null 2>&1; then
    wget -qO "$output" "$url"
  fi
}

main() {
  target="$(detect_target)"

  if [ -z "$VERSION" ]; then
    echo "Fetching latest version..."
    VERSION="$(get_latest_version)"
  fi

  if [ -z "$VERSION" ]; then
    echo "Error: could not determine latest version. Use --version to specify."
    exit 1
  fi

  archive="fl-${VERSION}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"

  echo "Installing fl ${VERSION} (${target})"
  echo "  from: ${url}"
  echo "  to:   ${INSTALL_DIR}/${BINARY}"

  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  download "$url" "${tmpdir}/${archive}"
  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

  mkdir -p "$INSTALL_DIR"
  mv "${tmpdir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  chmod +x "${INSTALL_DIR}/${BINARY}"

  echo ""
  echo "Installed fl ${VERSION} to ${INSTALL_DIR}/${BINARY}"

  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "Add to your PATH:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  fi
}

main
