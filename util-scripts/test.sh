#!/bin/bash
set -e

CRATES=(filament-core filament-cli filament-daemon filament-tui)

# Parse arguments
if [ $# -eq 0 ]; then
    echo "Usage: $0 <crate-name|all>"
    echo ""
    echo "Available crates:"
    for crate in "${CRATES[@]}"; do
        echo "  - $crate"
    done
    echo "  - all (run tests for all crates)"
    exit 1
fi

# Determine which crates to test
if [ "$1" == "all" ]; then
    crates_to_test=("${CRATES[@]}")
else
    crates_to_test=("$1")
fi

failed=()

for crate in "${crates_to_test[@]}"; do
    if [ ! -d "crates/$crate" ]; then
        echo "Error: Crate '$crate' not found in crates/"
        exit 1
    fi

    echo "========================================="
    echo "Running tests for: $crate"
    echo "========================================="

    if cargo test -p "$crate"; then
        echo "$crate: PASSED"
    else
        echo "$crate: FAILED"
        failed+=("$crate")
    fi

    echo ""
done

echo "========================================="
if [ ${#failed[@]} -eq 0 ]; then
    echo "All tests passed."
else
    echo "Failed crates: ${failed[*]}"
    exit 1
fi
