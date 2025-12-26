#!/bin/bash
set -euo pipefail

usage() {
  cat <<EOF
Usage: $(basename "$0") <version|tag>

Examples:
  $(basename "$0") 0.2.1
  $(basename "$0") v0.2.1
EOF
}

VERSION_INPUT="${1:-}"
if [ -z "${VERSION_INPUT}" ] || [ "${VERSION_INPUT}" = "-h" ] || [ "${VERSION_INPUT}" = "--help" ]; then
  usage
  exit 0
fi

VERSION="${VERSION_INPUT#v}"
if ! [[ "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Invalid version: ${VERSION_INPUT}. Expected vX.Y.Z or X.Y.Z."
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

update_version() {
  python - <<'PY' "${VERSION}" "$1" "$2"
import re
import sys

version = sys.argv[1]
path = sys.argv[2]
kind = sys.argv[3]

with open(path, "r", encoding="utf-8") as f:
  data = f.read()

if kind == "toml":
  pattern = r'^version = ".*"$'
  repl = f'version = "{version}"'
  flags = re.M
elif kind == "json":
  pattern = r'"version":\s*".*?"'
  repl = f'"version": "{version}"'
  flags = 0
else:
  raise SystemExit(f"Unknown kind: {kind}")

updated, count = re.subn(pattern, repl, data, count=1, flags=flags)
if count == 0:
  raise SystemExit(f"Version field not found in {path}")

with open(path, "w", encoding="utf-8") as f:
  f.write(updated)
PY
}

update_version "${ROOT_DIR}/Cargo.toml" "toml"
update_version "${ROOT_DIR}/vscode-extension/package.json" "json"

echo "Set version to ${VERSION}"
