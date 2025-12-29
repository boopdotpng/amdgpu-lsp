#!/bin/bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: create_release.sh <version|tag>

Examples:
  create_release.sh 0.2.4
  create_release.sh v0.2.4
USAGE
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
cd "${ROOT_DIR}"

if ! git rev-parse --show-toplevel >/dev/null 2>&1; then
  echo "Not inside a git repository."
  exit 1
fi

if git rev-parse "v${VERSION}" >/dev/null 2>&1; then
  echo "Tag v${VERSION} already exists."
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is not clean. Commit or stash changes before releasing."
  exit 1
fi

"${ROOT_DIR}/scripts/set-version.sh" "${VERSION}"

git add Cargo.toml vscode-extension/package.json
if git diff --cached --quiet; then
  echo "No version changes to commit."
  exit 1
fi

git commit -m "Release v${VERSION}"
git tag "v${VERSION}"
git push --follow-tags

echo "Release v${VERSION} created and pushed."
