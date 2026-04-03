#!/bin/sh
# Chat Demo - Reset
#
# Removes all generated state under tmp/ and returns the demo to its initial
# state. Static input files and configuration are never modified by this script.
#
# Usage:
#   sh ./reset.sh
#   ./reset.sh          (after chmod +x reset.sh)
#
# Safe to run multiple times. Does not require setup.sh to have been run first.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Resetting Chat demo..."

# Remove the SQLite history database if it exists
if [ -f tmp/xzatoma.db ]; then
    rm -f tmp/xzatoma.db
    echo "  Removed tmp/xzatoma.db"
fi

# Remove all files in tmp/output/ but keep the directory and .gitkeep
if [ -d tmp/output ]; then
    find tmp/output -type f ! -name '.gitkeep' -delete
    echo "  Cleared tmp/output/"
fi

# Remove any other generated files in tmp/ (logs, temp files, etc.)
# but preserve .gitignore, the output/ directory, and its contents
find tmp -maxdepth 1 -type f ! -name '.gitignore' -delete

echo "Reset complete."
echo "Run ./setup.sh to prepare the demo before running again."
