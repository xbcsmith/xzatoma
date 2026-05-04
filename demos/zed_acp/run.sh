#!/usr/bin/env bash
set -euo pipefail

DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Verify xzatoma is available
if ! command -v xzatoma &> /dev/null; then
    echo "xzatoma not found. Build it with: cargo build --release" >&2
    exit 1
fi

echo "Starting xzatoma in ACP agent mode (stdio)."
echo "Configure Zed to launch: xzatoma agent"
echo ""
echo "To test directly from the terminal (manual JSON-RPC):"
echo "  xzatoma agent"
echo ""
echo "See README.md for Zed configuration instructions."
