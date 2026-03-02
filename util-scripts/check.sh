#!/bin/bash
set -e

CRATES=(filament-core filament-cli filament-daemon filament-tui)

# Parse arguments
if [ $# -eq 0 ]; then
    echo "Usage: $0 <crate-name|all> [--clippy]"
    echo ""
    echo "Available crates:"
    for crate in "${CRATES[@]}"; do
        echo "  - $crate"
    done
    echo "  - all (check all crates)"
    echo ""
    echo "Options:"
    echo "  --clippy    Also run clippy lints (with -D warnings)"
    exit 1
fi

CRATE="$1"
RUN_CLIPPY=false

for arg in "$@"; do
    if [ "$arg" == "--clippy" ]; then
        RUN_CLIPPY=true
    fi
done

# Determine which crates to check
if [ "$CRATE" == "all" ]; then
    crates_to_check=("${CRATES[@]}")
else
    crates_to_check=("$CRATE")
fi

failed=()

for crate in "${crates_to_check[@]}"; do
    if [ ! -d "crates/$crate" ]; then
        echo "Error: Crate '$crate' not found in crates/"
        exit 1
    fi

    echo "========================================="
    echo "Checking: $crate"
    echo "========================================="

    if ! cargo check -p "$crate"; then
        echo "$crate: CHECK FAILED"
        failed+=("$crate")
        continue
    fi

    if [ "$RUN_CLIPPY" == true ]; then
        echo "Running clippy for $crate..."
        if ! cargo clippy -p "$crate" -- -D warnings; then
            echo "$crate: CLIPPY FAILED"
            failed+=("$crate")
            continue
        fi
    fi

    echo "$crate: OK"
    echo ""
done

echo "========================================="
if [ ${#failed[@]} -eq 0 ]; then
    echo "All checks passed."
else
    echo "Failed crates: ${failed[*]}"
    exit 1
fi
