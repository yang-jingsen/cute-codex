# cute-codex patches

Feature patches applied on top of upstream [openai/codex](https://github.com/openai/codex).

## Current Patcher

For the 0.135 SIDE5 upgrade, the authoritative patch is:

- `00-cute-codex-side5-rollup.patch`

This is a single rollup patch generated from `git diff --binary HEAD -- codex-rs`.
It is deliberate: notification, statusline, terminal sideband, and session
picker support touch several of the same TUI/config files, and a rollup avoids
duplicated hunks when replaying onto a clean upstream tag.

Use:

```bash
git checkout -b cute-side5-v0135 rust-v0.135.0
bash patches/apply.sh
```

## Patches

The older numbered patches are retained as feature notes/history. For 0.135
SIDE5, `apply.sh` uses the rollup patch above.

| # | File | Description |
|---|------|-------------|
| 01 | `01-binary-rename-and-branding.patch` | Rename binary to `cute-codex`, TUI header/tooltips branding, `version.rs` with optional build tag |
| 02 | `02-cli-entry-point.patch` | CLI entry point changes: clap metadata, help text, `CODEX_CONFIG_FILE` import, resume command strings |
| 03 | `03-custom-status-items.patch` | Custom status-line items loaded from `CODEX_CUSTOM_STATUS_ITEMS_FILE`, full TUI integration |
| 04 | `04-config-auth-path-override.patch` | `CODEX_CONFIG_FILE` and `CODEX_AUTH_FILE` env var overrides for per-profile config/auth isolation |
| 05 | `05-proxy-transport.patch` | `CUTE_CODEX_FORCE_HTTP_TRANSPORT` env var, SOCKS proxy support via reqwest feature |
| 06 | `06-build-metadata.patch` | Build metadata, `cute-codex` bin target, `serial_test` dev-dep, local build tag support |
| 07 | `07-idle-notify-service.patch` | HTTP notification service: delayed idle/approval/session-exit notifications plus opt-in lifecycle events such as session/user-message/turn/hook events. See `07-idle-notify-service.md` |
| 08 | `08-terminal-frame-sync.patch` | TUI terminal protocol: stable synchronized-update framing, cursor move/show order, and opt-in `OSC 777;cutecharm-cutex` composer sideband for CuteCharm |
| 09 | `09-session-picker-provider-filter.patch` | Resume/fork session picker provider filter config, including all-provider lookup across cutex profiles/custom providers |

## Upgrading to a new upstream version

```bash
cd codex
git fetch upstream --tags
git checkout -b cute-main-XXXX rust-vX.X.X
bash patches/apply.sh
# Fix any conflicts, then:
# Update version in 06-build-metadata.patch
cd codex-rs
CUTE_CODEX_TARGET="$(readlink -f "$HOME/Resources/Shortcuts/cute-codex")"
cp -a "$CUTE_CODEX_TARGET" "${CUTE_CODEX_TARGET}.bak-$(date +%Y%m%d-%H%M%S)"
cargo build -p codex-cli --bin cute-codex --release --target-dir target-host
BUILD_OUTPUT="$PWD/target-host/release/cute-codex"
if [ "$CUTE_CODEX_TARGET" != "$BUILD_OUTPUT" ]; then
  cp -a "$BUILD_OUTPUT" "$CUTE_CODEX_TARGET"
fi
"$HOME/Resources/Shortcuts/cute-codex" --version
cd ..
bash patches/generate.sh   # regenerate the rollup patch from the result
```

## Regenerating patches

After modifying the patched worktree:

```bash
bash patches/generate.sh
```

Then run `bash patches/apply.sh` in a fresh worktree at the upstream tag to
verify the rollup still replays cleanly.
