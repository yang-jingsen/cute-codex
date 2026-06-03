#!/usr/bin/env bash
set -euo pipefail

# Regenerate the authoritative patch file from the upstream base plus the
# current branch/worktree changes.
# Usage:
#   git checkout -b cute-side6-v0136 rust-v0.136.0
#   # apply/port local changes
#   bash patches/generate.sh
#
# This overwrites the rollup patch. It intentionally diffs against the upstream
# release tag, not HEAD, so the patch remains a complete patcher artifact after
# the branch is committed. The patch is scoped to `codex-rs/`.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_DIR"

BASE_REF="${1:-${CUTE_CODEX_PATCH_BASE:-rust-v0.136.0}}"

INTENT_TO_ADD_FILES=(
  codex-rs/tui/src/custom_status_items.rs
  codex-rs/tui/src/notify_service.rs
  codex-rs/tui/src/chatwidget/notify_service_events.rs
  codex-rs/tui/src/terminal_sideband.rs
  codex-rs/tui/src/cutex_agent_receiver.rs
  codex-rs/core/src/tools/handlers/cutex_agent_bus.rs
  codex-rs/core/src/tools/handlers/cutex_agent_bus_spec.rs
  codex-rs/app-server-protocol/schema/json/v2/ThreadInterAgentMessageParams.json
  codex-rs/app-server-protocol/schema/json/v2/ThreadInterAgentMessageResponse.json
  codex-rs/app-server-protocol/schema/typescript/v2/ThreadInterAgentMessageParams.ts
  codex-rs/app-server-protocol/schema/typescript/v2/ThreadInterAgentMessageResponse.ts
)

for file in "${INTENT_TO_ADD_FILES[@]}"; do
  if [ -e "$file" ] && ! git ls-files --error-unmatch "$file" >/dev/null 2>&1; then
    git add -N "$file"
  fi
done

PATCH="$SCRIPT_DIR/00-cute-codex-side6-rollup.patch"

echo "Generating patch: $BASE_REF..worktree"
echo ""

git diff --binary "$BASE_REF" -- codex-rs > "$PATCH"
echo "  $(basename "$PATCH")"

echo ""
echo "Done. Patches regenerated in $SCRIPT_DIR/"
