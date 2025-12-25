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
    echo -e "  ${YELLOW}--extension-mode${NC}  Extension build mode: debug or release (default: release)"
    echo -e "  ${YELLOW}--no-minify${NC}  Disable minification for the extension bundle (release mode only)"
    echo -e "  ${YELLOW}--include-meta${NC}  Keep esbuild metafile at vscode-extension/dist/meta.json"
    echo -e "  ${YELLOW}--minify-json${NC}  Minify isa.json even in debug builds"
    echo -e "  ${YELLOW}-h, --help${NC}     Show this help menu"
}

FETCH_LATEST=false
SHOW_HELP=false
EXTENSION_MODE="release"
NO_MINIFY=false
INCLUDE_META=false
MINIFY_JSON=false

while [ $# -gt 0 ]; do
    case "$1" in
        --fetch-latest)
            FETCH_LATEST=true
            ;;
        --extension-mode)
            EXTENSION_MODE="${2:-}"
            shift
            ;;
        --no-minify)
            NO_MINIFY=true
            ;;
        --include-meta)
            INCLUDE_META=true
            ;;
        --minify-json)
            MINIFY_JSON=true
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

if [ "${EXTENSION_MODE}" != "debug" ] && [ "${EXTENSION_MODE}" != "release" ]; then
    echo_error "Unknown extension mode: ${EXTENSION_MODE} (use debug or release)"
    exit 1
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

detect_target() {
    local os arch os_id arch_id
    os="$(uname -s)"
    arch="$(uname -m)"

    case "${os}" in
        Linux)
            os_id="linux"
            ;;
        Darwin)
            os_id="darwin"
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            os_id="win32"
            ;;
        *)
            echo_error "Unsupported OS: ${os}"
            exit 1
            ;;
    esac

    case "${arch}" in
        x86_64|amd64)
            arch_id="x64"
            ;;
        aarch64|arm64)
            arch_id="arm64"
            ;;
        *)
            echo_error "Unsupported architecture: ${arch}"
            exit 1
            ;;
    esac

    if [ "${os_id}" = "win32" ] && [ "${arch_id}" != "x64" ]; then
        echo_error "Unsupported Windows architecture: ${arch}"
        exit 1
    fi

    echo "${os_id}-${arch_id}"
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

# Step 3: Build and package extension for the local platform
LOCAL_TARGET="$(detect_target)"
echo_step "Packaging extension for ${LOCAL_TARGET}..."
PACKAGE_ARGS=(--targets "${LOCAL_TARGET}" --mode "${EXTENSION_MODE}")
if [ "${NO_MINIFY}" = true ]; then
    PACKAGE_ARGS+=(--no-minify)
fi
if [ "${INCLUDE_META}" = true ]; then
    PACKAGE_ARGS+=(--include-meta)
fi
if [ "${MINIFY_JSON}" = true ]; then
    PACKAGE_ARGS+=(--minify-json)
fi
scripts/package.sh "${PACKAGE_ARGS[@]}"

# Get the generated VSIX filename (should be amdgpu-lsp-0.1.0.vsix based on package.json)
VSIX_FILE=$(ls -t vscode-extension/*.vsix | head -n 1)
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

echo_step "${GREEN}Build complete!${NC}"
echo ""
echo "The LSP server binary is at: target/release/amdgpu-lsp"
echo "The extension has been installed in VS Code"
echo ""
echo "You may need to reload VS Code for changes to take effect."

echo "If you're running this extension locally, you will need to manually set the amdgpu-lsp binary path and the path to isa.json in VS Code settings."
