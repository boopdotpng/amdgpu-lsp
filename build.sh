#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
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

show_help() {
    echo -e "${BLUE}${BOLD}amdgpu-lsp build script${NC}"
    echo -e "${BOLD}Usage:${NC} ./build.sh [options]"
    echo ""
    echo -e "${BOLD}Options:${NC}"
    echo -e "  ${YELLOW}--fetch-latest${NC}  Download latest ISA XMLs and overwrite ${GREEN}amd_gpu_xmls/${NC}"
    echo -e "  ${YELLOW}-h, --help${NC}     Show this help menu"
}

FETCH_LATEST=false
SHOW_HELP=false

while [ $# -gt 0 ]; do
    case "$1" in
        --fetch-latest)
            FETCH_LATEST=true
            ;;
        -h|--help)
            SHOW_HELP=true
            ;;
        *)
            echo_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
    shift
done

if [ "$SHOW_HELP" = true ]; then
    show_help
    exit 0
fi

# Check if bun is available
if command -v bun &> /dev/null; then
    PKG_MANAGER="bun"
    PKG_MANAGER_X="bunx"
else
    PKG_MANAGER="npm"
    PKG_MANAGER_X="npx"
    echo_warning "bun not found, using npm instead"
fi

fetch_isa() {
    echo_step "Fetching ISA XMLs..."
    out_dir="amd_gpu_xmls"
    tmp_dir="$(mktemp -d)"
    zip_path="${tmp_dir}/isa.zip"

    cleanup_fetch() {
        rm -rf "${tmp_dir}"
    }
    trap cleanup_fetch EXIT

    mkdir -p "${out_dir}"
    curl -L "https://gpuopen.com/download/machine-readable-isa/latest/" -o "${zip_path}"
    unzip -o "${zip_path}" -d "${out_dir}"

    echo_step "Downloaded AMDGPU ISA files to ${out_dir}"
}

# Step 1: Fetch ISA XMLs if needed
if [ "$FETCH_LATEST" = true ]; then
    echo_step "Fetching latest ISA XMLs (overwriting existing)..."
    rm -rf "amd_gpu_xmls"
    fetch_isa
elif [ -d "amd_gpu_xmls" ] && [ -n "$(ls -A amd_gpu_xmls)" ]; then
    echo_step "ISA XMLs already present, skipping fetch"
else
    fetch_isa
fi

# Step 2: Parse ISA and generate isa.json
echo_step "Parsing ISA and generating data/isa.json..."
cargo run --bin parse_isa

# Step 3: Build LSP server
echo_step "Building LSP server..."
cargo build --release

# Step 4: Stage bundled server/data into the extension
echo_step "Staging bundled server/data for extension..."
BIN_FILE="target/release/amdgpu-lsp"
DATA_FILE="data/isa.json"
if [ ! -f "${BIN_FILE}" ]; then
    echo_error "Missing ${BIN_FILE}. Cargo build did not produce the server binary."
    exit 1
fi
if [ ! -f "${DATA_FILE}" ]; then
    echo_error "Missing ${DATA_FILE}. ISA generation did not produce isa.json."
    exit 1
fi
mkdir -p vscode-extension/bin vscode-extension/data
cp "${BIN_FILE}" "vscode-extension/bin/amdgpu-lsp"
cp "${DATA_FILE}" "vscode-extension/data/isa.json"

# Step 5: Install extension dependencies
echo_step "Installing extension dependencies with $PKG_MANAGER..."
cd vscode-extension
$PKG_MANAGER install

# Step 6: Build extension
echo_step "Building extension..."
$PKG_MANAGER run build

# Step 7: Package extension
echo_step "Packaging extension..."
$PKG_MANAGER_X vsce package

# Get the generated VSIX filename (should be amdgpu-lsp-0.1.0.vsix based on package.json)
VSIX_FILE=$(ls -t *.vsix | head -n 1)
if [ -z "$VSIX_FILE" ]; then
    echo_error "No .vsix file found after packaging!"
    exit 1
fi
echo_step "Generated: $VSIX_FILE"

# Step 8: Uninstall existing extension
echo_step "Uninstalling existing amdgpu-lsp extension..."
if code --uninstall-extension amdgpu-lsp.amdgpu-lsp 2>/dev/null; then
    echo "Successfully uninstalled previous version"
else
    echo_warning "Extension was not installed or failed to uninstall (this is OK if first install)"
fi

# Step 9: Install new extension
echo_step "Installing new extension..."
code --install-extension "$VSIX_FILE"

cd ..

echo_step "${GREEN}Build complete!${NC}"
echo ""
echo "The LSP server binary is at: target/release/amdgpu-lsp"
echo "The extension has been installed in VS Code"
echo ""
echo "You may need to reload VS Code for changes to take effect."

echo "If you're running this extension locally, you will need to manually set the amdgpu-lsp binary path and the path to isa.json in VS Code settings."
