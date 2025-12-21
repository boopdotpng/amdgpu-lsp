#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_JSON="${ROOT_DIR}/vscode-extension/package.json"
CARGO_TOML="${ROOT_DIR}/Cargo.toml"

if [ ! -f "${PACKAGE_JSON}" ]; then
  echo "Missing ${PACKAGE_JSON}"
  exit 1
fi

if [ ! -f "${CARGO_TOML}" ]; then
  echo "Missing ${CARGO_TOML}"
  exit 1
fi

LATEST_TAG=$(git tag --list "v*" --sort=version:refname | tail -n 1 || true)
if [ -n "${LATEST_TAG}" ]; then
  BASE_VERSION="${LATEST_TAG#v}"
  INCREMENT_PATCH=1
else
  BASE_VERSION=$(python - <<'PY'
import json
from pathlib import Path

data = json.loads(Path("vscode-extension/package.json").read_text())
print(data.get("version", "0.1.0"))
PY
  )
  INCREMENT_PATCH=0
fi

if [[ ! "${BASE_VERSION}" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "Unsupported version format: ${BASE_VERSION}"
  exit 1
fi

MAJOR="${BASH_REMATCH[1]}"
MINOR="${BASH_REMATCH[2]}"
PATCH="${BASH_REMATCH[3]}"
if [ "${INCREMENT_PATCH}" -eq 1 ]; then
  PATCH=$((PATCH + 1))
fi
NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"

python - <<PY
import json
import re
from pathlib import Path

new_version = "${NEW_VERSION}"

package_path = Path("vscode-extension/package.json")
package_data = json.loads(package_path.read_text())
package_data["version"] = new_version
package_path.write_text(json.dumps(package_data, indent=2) + "\n")

cargo_path = Path("Cargo.toml")
cargo_text = cargo_path.read_text()
updated, count = re.subn(r'(?m)^version = ".*"$', f'version = "{new_version}"', cargo_text, count=1)
if count != 1:
  raise SystemExit("Failed to update Cargo.toml version")
cargo_path.write_text(updated)
PY

echo "${NEW_VERSION}"
