#!/bin/sh
# XZatoma llama.cpp Provider Demo - Reset
#
# Removes all generated state under tmp/ and returns the demo to its initial
# state. Static input files, plan files, and configuration are never removed.
#
# Usage:
#   sh ./reset.sh
#   ./reset.sh          (after chmod +x reset.sh)
#
# Safe to run multiple times. Running reset.sh before setup.sh is harmless.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Resetting llama.cpp provider demo..."

# Remove generated output artifacts
if [ -d tmp/output ]; then
    find tmp/output -type f ! -name '.gitkeep' -delete
    echo "  Cleared tmp/output/"
fi

# Remove the demo storage database
if [ -f tmp/xzatoma.db ]; then
    rm -f tmp/xzatoma.db
    echo "  Removed tmp/xzatoma.db"
fi

# Remove any other generated files under tmp/ except .gitignore, output/, and .gitkeep
find tmp -maxdepth 1 -type f ! -name '.gitignore' -delete

echo "Reset complete."
echo "Run ./setup.sh to prepare the demo before running it again."
