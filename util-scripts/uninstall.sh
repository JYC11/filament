#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${1:-$HOME/.local/bin}"
BINARY_NAME="fl"
TARGET="$INSTALL_DIR/$BINARY_NAME"

if [ ! -f "$TARGET" ]; then
    echo "fl not found at $TARGET — nothing to uninstall."
    exit 0
fi

rm "$TARGET"
echo "Removed $TARGET"
echo ""
echo "Note: existing .fl/ project directories were NOT removed."
echo "To remove a project's data, delete its .fl/ directory manually."
