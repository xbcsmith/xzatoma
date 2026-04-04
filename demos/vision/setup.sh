#!/bin/sh
# XZatoma Demo: Vision - Setup
#
# Creates the tmp/ directory structure required by this demo and generates
# a sample PNG image for use with the vision model.
#
# Usage:
#   sh ./setup.sh
#   # or after chmod +x:
#   ./setup.sh
#
# All generated state is written under tmp/. No files outside this demo
# directory are created or modified.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Setting up Vision demo..."

# Create required directory structure
mkdir -p tmp/output

# Generate a sample 64x64 PNG image using Python 3.
# The image is a solid blue (RGB 70, 130, 180) rectangle stored at
# tmp/sample.png. It is used to demonstrate image analysis tasks when
# full image attachment support is available in XZatoma.
python3 - <<'PYEOF'
import struct
import zlib
import os

def make_chunk(chunk_type, data):
    length = struct.pack('>I', len(data))
    crc_val = zlib.crc32(chunk_type + data) & 0xFFFFFFFF
    return length + chunk_type + data + struct.pack('>I', crc_val)

def create_solid_png(width, height, r, g, b):
    signature = b'\x89PNG\r\n\x1a\n'

    ihdr_data = struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
    ihdr = make_chunk(b'IHDR', ihdr_data)

    raw_rows = b''
    for _ in range(height):
        raw_rows += b'\x00' + bytes([r, g, b]) * width

    idat = make_chunk(b'IDAT', zlib.compress(raw_rows, 9))
    iend = make_chunk(b'IEND', b'')

    return signature + ihdr + idat + iend

os.makedirs('tmp', exist_ok=True)
png_data = create_solid_png(64, 64, 70, 130, 180)
with open('tmp/sample.png', 'wb') as f:
    f.write(png_data)
print("Created tmp/sample.png (64x64 solid blue PNG, RGB 70,130,180)")
PYEOF

echo ""
echo "Setup complete."
echo ""
echo "Files created:"
echo "  tmp/output/    (demo output directory)"
echo "  tmp/sample.png (64x64 blue PNG for vision testing)"
echo ""
echo "Run ./run.sh to start the vision demo."
