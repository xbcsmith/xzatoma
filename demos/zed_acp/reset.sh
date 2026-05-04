#!/usr/bin/env bash
set -euo pipefail

DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

rm -rf "${DEMO_DIR}/tmp/output"
mkdir -p "${DEMO_DIR}/tmp/output"

echo "Zed ACP demo state reset."
