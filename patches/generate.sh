#!/usr/bin/env bash
set -euo pipefail

# Regenerate the authoritative patch file from the current worktree vs HEAD.
# Usage:
#   git checkout -b cute-side6-v0136 rust-v0.136.0
#   # apply/port local changes
#   bash patches/generate.sh
#
# This overwrites the rollup patch. It intentionally uses `git diff HEAD`
# instead of `BASE..HEAD` so conflict-resolution edits that have not been
# committed yet are included. The patch is scoped to `codex-rs/`.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_DIR"

INTENT_TO_ADD_FILES=(
  codex-rs/tui/src/custom_status_items.rs
  codex-rs/tui/src/notify_service.rs
  codex-rs/tui/src/chatwidget/notify_service_events.rs
  codex-rs/tui/src/terminal_sideband.rs
)

for file in "${INTENT_TO_ADD_FILES[@]}"; do
  if [ -e "$file" ] && ! git ls-files --error-unmatch "$file" >/dev/null 2>&1; then
    git add -N "$file"
  fi
done

PATCH="$SCRIPT_DIR/00-cute-codex-side6-rollup.patch"

echo "Generating patch: HEAD..worktree"
echo ""

git diff --binary HEAD -- codex-rs > "$PATCH"
echo "  $(basename "$PATCH")"

echo ""
echo "Done. Patches regenerated in $SCRIPT_DIR/"
