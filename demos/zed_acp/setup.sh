#!/usr/bin/env bash
set -euo pipefail

DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Create tmp directory structure
mkdir -p "${DEMO_DIR}/tmp/output"

# Create .gitignore for tmp/
cat > "${DEMO_DIR}/tmp/.gitignore" << 'EOF'
output/
EOF

echo "Zed ACP demo ready. Run 'bash run.sh' or configure Zed to use xzatoma agent."
