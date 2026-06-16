#!/bin/bash
set -e

echo "================================================"
echo "  SemanticClipboard — RPM Build Script"
echo "================================================"

# Step 1: Check for cargo-generate-rpm
if ! command -v cargo-generate-rpm &> /dev/null; then
    echo "[1/4] Installing cargo-generate-rpm..."
    cargo install cargo-generate-rpm
else
    echo "[1/4] cargo-generate-rpm already installed. Skipping."
fi

# Step 2: Build release binary
echo "[2/4] Building release binary..."
cargo build --release

# Step 3: Strip binary to reduce size
echo "[3/4] Stripping binary..."
strip -s target/release/SemanticClipboard

# Step 4: Generate RPM
echo "[4/4] Generating RPM package..."
cargo generate-rpm

echo ""
echo "================================================"
echo "  Build complete!"
RPM_FILE=$(find target/generate-rpm -name "*.rpm" | head -1)
echo "  RPM: $RPM_FILE"
echo ""
echo "  To install:"
echo "  sudo rpm -i $RPM_FILE"
echo ""
echo "  Or double-click the .rpm file in your file manager"
echo "  to install via GNOME Software!"
echo "================================================"
