#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${1:-$HOME/.local/bin}"
BINARY_NAME="filament"

echo "Building filament (release)..."
cargo build --release --package filament-cli

SRC="$(dirname "$0")/../target/release/$BINARY_NAME"
if [ ! -f "$SRC" ]; then
    echo "error: binary not found at $SRC"
    exit 1
fi

mkdir -p "$INSTALL_DIR"
cp "$SRC" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo "Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"

# Check if install dir is on PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "Warning: $INSTALL_DIR is not on your PATH."
    echo "Add it with:  export PATH=\"$INSTALL_DIR:\$PATH\""
fi
