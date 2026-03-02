#!/bin/bash
set -e

BINARIES=(filament-cli filament-daemon filament-tui)

# Parse arguments
if [ $# -eq 0 ]; then
    echo "Usage: $0 <binary-name> [-- args...]"
    echo ""
    echo "Available binaries:"
    for bin in "${BINARIES[@]}"; do
        echo "  - $bin"
    done
    echo ""
    echo "Examples:"
    echo "  $0 filament-cli -- init"
    echo "  $0 filament-daemon -- serve"
    echo "  $0 filament-tui"
    exit 1
fi

BINARY="$1"
shift

if [ ! -d "crates/$BINARY" ]; then
    echo "Error: Binary '$BINARY' not found in crates/"
    exit 1
fi

if [ ! -f "crates/$BINARY/Cargo.toml" ]; then
    echo "Error: crates/$BINARY has no Cargo.toml"
    exit 1
fi

echo "========================================="
echo "Running: $BINARY"
echo "========================================="

cargo run -p "$BINARY" "$@"
