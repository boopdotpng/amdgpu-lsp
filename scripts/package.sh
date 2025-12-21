#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXT_DIR="${ROOT_DIR}/vscode-extension"
DATA_FILE="${ROOT_DIR}/data/isa.json"

usage() {
  cat <<EOF
Usage: $(basename "$0") [--targets <list>]

Examples:
  $(basename "$0") --targets linux-x64
  $(basename "$0") --targets linux-x64,win32-x64,darwin-arm64

Targets:
  linux-x64, linux-arm64, win32-x64, darwin-x64, darwin-arm64
EOF
}

TARGETS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --targets)
      TARGETS="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1"
      usage
      exit 1
      ;;
  esac
done

if [ -z "${TARGETS}" ]; then
  TARGETS="linux-x64"
fi

if command -v bun &> /dev/null; then
  PKG_MANAGER="bun"
  PKG_MANAGER_X="bunx"
else
  PKG_MANAGER="npm"
  PKG_MANAGER_X="npx"
fi

if [ ! -f "${DATA_FILE}" ]; then
  echo "Missing ${DATA_FILE}. Run the ISA generator before packaging."
  exit 1
fi

echo "Building extension..."
cd "${EXT_DIR}"
${PKG_MANAGER} install
${PKG_MANAGER} run build
rm -f *.vsix

IFS=',' read -r -a TARGET_LIST <<< "${TARGETS}"
for TARGET in "${TARGET_LIST[@]}"; do
  case "${TARGET}" in
    linux-x64)
      RUST_TARGET="x86_64-unknown-linux-gnu"
      BIN_NAME="amdgpu-lsp"
      ;;
    linux-arm64)
      RUST_TARGET="aarch64-unknown-linux-gnu"
      BIN_NAME="amdgpu-lsp"
      ;;
    win32-x64)
      RUST_TARGET="x86_64-pc-windows-msvc"
      BIN_NAME="amdgpu-lsp.exe"
      ;;
    darwin-x64)
      RUST_TARGET="x86_64-apple-darwin"
      BIN_NAME="amdgpu-lsp"
      ;;
    darwin-arm64)
      RUST_TARGET="aarch64-apple-darwin"
      BIN_NAME="amdgpu-lsp"
      ;;
    *)
      echo "Unknown target: ${TARGET}"
      exit 1
      ;;
  esac

  echo "Building release server for ${TARGET} (${RUST_TARGET})..."
  cargo build --release --target "${RUST_TARGET}"

  BIN_FILE="${ROOT_DIR}/target/${RUST_TARGET}/release/${BIN_NAME}"
  if [ ! -f "${BIN_FILE}" ]; then
    echo "Missing ${BIN_FILE}. Cargo build did not produce the server binary."
    exit 1
  fi

  echo "Staging bundled assets for ${TARGET}..."
  mkdir -p "${EXT_DIR}/bin" "${EXT_DIR}/data"
  cp "${BIN_FILE}" "${EXT_DIR}/bin/${BIN_NAME}"
  cp "${DATA_FILE}" "${EXT_DIR}/data/isa.json"

  echo "Packaging VSIX for ${TARGET}..."
  ${PKG_MANAGER_X} vsce package --target "${TARGET}"

  VSIX_FILE=$(ls -t *.vsix | head -n 1)
  if [ -z "${VSIX_FILE}" ]; then
    echo "No .vsix file found after packaging for ${TARGET}."
    exit 1
  fi

  echo "Created ${EXT_DIR}/${VSIX_FILE}"
done
