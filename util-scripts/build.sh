#!/bin/bash
set -e

CRATES=(filament-core filament-cli filament-daemon filament-tui)

# Parse arguments
if [ $# -eq 0 ]; then
    echo "Usage: $0 <crate-name|all> [--release]"
    echo ""
    echo "Available crates:"
    for crate in "${CRATES[@]}"; do
        echo "  - $crate"
    done
    echo "  - all (build all crates)"
    echo ""
    echo "Options:"
    echo "  --release   Build in release mode"
    exit 1
fi

CRATE="$1"
RELEASE=""

for arg in "$@"; do
    if [ "$arg" == "--release" ]; then
        RELEASE="--release"
    fi
done

# Determine which crates to build
if [ "$CRATE" == "all" ]; then
    crates_to_build=("${CRATES[@]}")
else
    crates_to_build=("$CRATE")
fi

failed=()

for crate in "${crates_to_build[@]}"; do
    if [ ! -d "crates/$crate" ]; then
        echo "Error: Crate '$crate' not found in crates/"
        exit 1
    fi

    echo "========================================="
    echo "Building: $crate"
    echo "========================================="

    if cargo build -p "$crate" $RELEASE; then
        echo "$crate: OK"
    else
        echo "$crate: FAILED"
        failed+=("$crate")
    fi

    echo ""
done

echo "========================================="
if [ ${#failed[@]} -eq 0 ]; then
    echo "All builds passed."
else
    echo "Failed crates: ${failed[*]}"
    exit 1
fi
