#!/bin/bash
set -e

MIGRATIONS_DIR="migrations"

# Parse arguments or prompt
if [ $# -eq 0 ]; then
    read -p "Enter migration name: " migration_name
elif [ $# -ge 1 ]; then
    migration_name="$1"
else
    echo "Usage: make migration NAME=<migration-name>"
    exit 1
fi

# Generate timestamp
datetime=$(date '+%Y%m%d%H%M')

# Create filename
filename="${datetime}_${migration_name}.sql"

# Create migrations directory if it doesn't exist
mkdir -p "$MIGRATIONS_DIR"

# Create migration file
filepath="${MIGRATIONS_DIR}/${filename}"
touch "$filepath"

echo "Created migration file: $filepath"
echo ""
echo "Edit the file to add your migration SQL."
