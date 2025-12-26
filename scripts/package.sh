#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXT_DIR="${ROOT_DIR}/vscode-extension"
DATA_FILE="${ROOT_DIR}/data/isa.json"
MINIFIED_DATA_FILE="${EXT_DIR}/data/isa.json"

fetch_isa() {
  echo "Fetching ISA XMLs..."
  out_dir="${ROOT_DIR}/amd_gpu_xmls"
  tmp_dir="$(mktemp -d)"
  zip_path="${tmp_dir}/isa.zip"

  cleanup_fetch() {
    rm -rf "${tmp_dir}"
  }
  trap cleanup_fetch EXIT

  mkdir -p "${out_dir}"
  curl -L "https://gpuopen.com/download/machine-readable-isa/latest/" -o "${zip_path}"
  unzip -o "${zip_path}" -d "${out_dir}"

  echo "Downloaded AMDGPU ISA files to ${out_dir}"
}

ensure_isa_data() {
  if [ -f "${DATA_FILE}" ]; then
    return
  fi

  if [ -d "${ROOT_DIR}/amd_gpu_xmls" ] && [ -n "$(ls -A "${ROOT_DIR}/amd_gpu_xmls")" ]; then
    echo "ISA XMLs already present, skipping fetch"
  else
    fetch_isa
  fi

  echo "Parsing ISA and generating ${DATA_FILE}..."
  cargo run --bin parse_isa
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [--targets <list>] [--mode <debug|release>] [--no-minify] [--include-meta] [--minify-json]

Examples:
  $(basename "$0") --targets linux-x64
  $(basename "$0") --targets linux-x64,win32-x64,darwin-arm64 --mode release
  $(basename "$0") --targets linux-x64 --include-meta --no-minify --minify-json

Targets:
  linux-x64, linux-arm64, win32-x64, darwin-x64, darwin-arm64
EOF
}

TARGETS=""
MODE="release"
NO_MINIFY=false
INCLUDE_META=false
FORCE_MINIFY_JSON=false
while [ $# -gt 0 ]; do
  case "$1" in
    --targets)
      TARGETS="${2:-}"
      shift 2
      ;;
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --no-minify)
      NO_MINIFY=true
      shift
      ;;
    --include-meta)
      INCLUDE_META=true
      shift
      ;;
    --minify-json)
      FORCE_MINIFY_JSON=true
      shift
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

if [ "${MODE}" != "debug" ] && [ "${MODE}" != "release" ]; then
  echo "Unknown mode: ${MODE}. Use debug or release."
  exit 1
fi

MINIFY_JSON=false
if [ "${MODE}" = "release" ] || [ "${FORCE_MINIFY_JSON}" = true ]; then
  MINIFY_JSON=true
fi

if command -v bun &> /dev/null; then
  PKG_MANAGER="bun"
  PKG_MANAGER_X="bunx"
else
  PKG_MANAGER="npm"
  PKG_MANAGER_X="npx"
fi

ensure_isa_data

echo "Building extension..."
cd "${EXT_DIR}"
if [ "${PKG_MANAGER}" = "bun" ]; then
  ${PKG_MANAGER} install --frozen-lockfile
else
  ${PKG_MANAGER} install
fi
ESBUILD_FLAGS=""
if [ "${INCLUDE_META}" = true ]; then
  ESBUILD_FLAGS="--metafile=dist/meta.json"
fi
if [ "${MODE}" = "debug" ]; then
  ESBUILD_FLAGS="${ESBUILD_FLAGS}" ${PKG_MANAGER} run build
else
  if [ "${NO_MINIFY}" = true ]; then
    ESBUILD_FLAGS="${ESBUILD_FLAGS}" ${PKG_MANAGER} run build:release:nomini
  else
    ESBUILD_FLAGS="${ESBUILD_FLAGS}" ${PKG_MANAGER} run build:release
  fi
  rm -f dist/extension.js.map
fi
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
  cargo build --locked --release --target "${RUST_TARGET}"

  BIN_FILE="${ROOT_DIR}/target/${RUST_TARGET}/release/${BIN_NAME}"
  if [ ! -f "${BIN_FILE}" ]; then
    echo "Missing ${BIN_FILE}. Cargo build did not produce the server binary."
    exit 1
  fi

  echo "Staging bundled assets for ${TARGET}..."
  mkdir -p "${EXT_DIR}/bin" "${EXT_DIR}/data"
  cp "${BIN_FILE}" "${EXT_DIR}/bin/${BIN_NAME}"
  if [ "${MINIFY_JSON}" = true ]; then
    echo "Compacting ISA JSON for ${TARGET}..."
    python - <<'PY' "${DATA_FILE}" "${MINIFIED_DATA_FILE}"
import json
import sys

src = sys.argv[1]
dst = sys.argv[2]

with open(src, "r", encoding="utf-8") as f:
  data = json.load(f)

with open(dst, "w", encoding="utf-8") as f:
  json.dump(data, f, separators=(",", ":"))
PY
  else
    echo "Copying ISA JSON for ${TARGET}..."
    cp "${DATA_FILE}" "${MINIFIED_DATA_FILE}"
  fi

  echo "Packaging VSIX for ${TARGET}..."
  ${PKG_MANAGER_X} vsce package --target "${TARGET}"

  VSIX_FILE=$(ls -t *.vsix | head -n 1)
  if [ -z "${VSIX_FILE}" ]; then
    echo "No .vsix file found after packaging for ${TARGET}."
    exit 1
  fi

  echo "Created ${EXT_DIR}/${VSIX_FILE}"
done

echo "Cleaning packaged artifacts..."
rm -rf "${EXT_DIR}/bin" "${EXT_DIR}/data"
if [ "${INCLUDE_META}" = false ]; then
  rm -rf "${EXT_DIR}/dist"
fi
