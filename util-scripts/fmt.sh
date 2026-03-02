#!/bin/bash
set -e

CRATES=(filament-core filament-cli filament-daemon filament-tui)

# Parse arguments
if [ $# -eq 0 ]; then
    echo "Usage: $0 <crate-name|all> [--check]"
    echo ""
    echo "Available crates:"
    for crate in "${CRATES[@]}"; do
        echo "  - $crate"
    done
    echo "  - all (format all crates)"
    echo ""
    echo "Options:"
    echo "  --check    Check formatting without modifying files (for CI)"
    exit 1
fi

CRATE="$1"
CHECK=""

for arg in "$@"; do
    if [ "$arg" == "--check" ]; then
        CHECK="--check"
    fi
done

# Determine which crates to format
if [ "$CRATE" == "all" ]; then
    crates_to_fmt=("${CRATES[@]}")
else
    crates_to_fmt=("$CRATE")
fi

failed=()

for crate in "${crates_to_fmt[@]}"; do
    if [ ! -d "crates/$crate" ]; then
        echo "Error: Crate '$crate' not found in crates/"
        exit 1
    fi

    echo "========================================="
    if [ -n "$CHECK" ]; then
        echo "Checking format: $crate"
    else
        echo "Formatting: $crate"
    fi
    echo "========================================="

    if cargo fmt -p "$crate" $CHECK; then
        echo "$crate: OK"
    else
        echo "$crate: FAILED"
        failed+=("$crate")
    fi

    echo ""
done

echo "========================================="
if [ ${#failed[@]} -eq 0 ]; then
    if [ -n "$CHECK" ]; then
        echo "All format checks passed."
    else
        echo "All crates formatted."
    fi
else
    echo "Failed crates: ${failed[*]}"
    exit 1
fi
