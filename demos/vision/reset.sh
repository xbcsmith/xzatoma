#!/bin/sh
# XZatoma Vision Demo - Reset
#
# Removes all generated state under tmp/. Static input files, configuration,
# and scripts are never modified by this script.
#
# Usage:
#   ./reset.sh
#   sh ./reset.sh
#
# After reset, run ./setup.sh before running the demo again.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Resetting Vision demo..."

# Remove generated runtime state
rm -f tmp/xzatoma.db

# Remove generated image created by setup.sh
rm -f tmp/sample.png

# Remove all output artifacts
rm -f tmp/output/*

echo "Reset complete."
echo "Run ./setup.sh before running the demo again."
