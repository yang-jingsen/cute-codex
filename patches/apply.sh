#!/usr/bin/env bash
set -euo pipefail

# Apply the cute-codex patch on top of a clean upstream checkout.
# Usage:
#   git checkout <upstream-tag>      # e.g. rust-v0.136.0
#   bash patches/apply.sh
#
# The 0.136 SIDE6 patcher uses one rollup patch because several local features
# intentionally touch the same TUI/config files.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_DIR"

PATCH="$SCRIPT_DIR/00-cute-codex-side6-rollup.patch"

if [ ! -f "$PATCH" ]; then
  echo "Missing patch: $PATCH"
  exit 1
fi

if git apply --check "$PATCH" 2>/dev/null; then
  git apply "$PATCH"
  echo "  OK: $(basename "$PATCH")"
else
  echo "FAIL: $(basename "$PATCH")"
  echo "Try manually with: git apply --3way patches/$(basename "$PATCH")"
  exit 1
fi

echo ""
echo "Applied: 1 / 1"
