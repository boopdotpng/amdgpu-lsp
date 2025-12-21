#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo_step() {
    echo -e "${GREEN}==>${NC} $1"
}

echo_error() {
    echo -e "${RED}Error:${NC} $1"
}

echo_warning() {
    echo -e "${YELLOW}Warning:${NC} $1"
}

# Check if bun is available
if command -v bun &> /dev/null; then
    PKG_MANAGER="bun"
    PKG_MANAGER_X="bunx"
else
    PKG_MANAGER="npm"
    PKG_MANAGER_X="npx"
    echo_warning "bun not found, using npm instead"
fi

# Step 1: Fetch ISA XMLs if needed
if [ -d "amd_gpu_xmls" ] && [ -n "$(ls -A amd_gpu_xmls)" ]; then
    echo_step "ISA XMLs already present, skipping fetch"
else
    echo_step "Fetching ISA XMLs..."
    ./fetch.sh
fi

# Step 2: Parse ISA and generate isa.json
echo_step "Parsing ISA and generating data/isa.json..."
cargo run --bin parse_isa

# Step 3: Build LSP server
echo_step "Building LSP server..."
cargo build --release

# Step 4: Install extension dependencies
echo_step "Installing extension dependencies with $PKG_MANAGER..."
cd vscode-extension
$PKG_MANAGER install

# Step 5: Build extension
echo_step "Building extension..."
$PKG_MANAGER run build

# Step 6: Package extension
echo_step "Packaging extension..."
$PKG_MANAGER_X vsce package

# Get the generated VSIX filename (should be rdna-lsp-0.1.0.vsix based on package.json)
VSIX_FILE=$(ls -t *.vsix | head -n 1)
if [ -z "$VSIX_FILE" ]; then
    echo_error "No .vsix file found after packaging!"
    exit 1
fi
echo_step "Generated: $VSIX_FILE"

# Step 7: Uninstall existing extension
echo_step "Uninstalling existing rdna-lsp extension..."
if code --uninstall-extension rdna-lsp.rdna-lsp 2>/dev/null; then
    echo "Successfully uninstalled previous version"
else
    echo_warning "Extension was not installed or failed to uninstall (this is OK if first install)"
fi

# Step 8: Install new extension
echo_step "Installing new extension..."
code --install-extension "$VSIX_FILE"

cd ..

echo_step "${GREEN}Build complete!${NC}"
echo ""
echo "The LSP server binary is at: target/release/rdna-lsp"
echo "The extension has been installed in VS Code"
echo ""
echo "You may need to reload VS Code for changes to take effect."

echo "If you're running this extension locally, you will need to manually set the rdna-lsp binary path and the path to isa.json in VS Code settings."
