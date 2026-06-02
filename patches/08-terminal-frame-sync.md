# 08 Terminal Frame Sync And CuteCharm Sideband

Patch: `08-terminal-frame-sync.patch`

## Purpose

This patch fixes the terminal-host integration problems described in the CuteCharm handoff:

- synchronized-update markers must be emitted through the same backend writer that draws frames;
- cursor position must be set before the visible cursor is shown;
- CuteCharm needs explicit composer/caret state instead of inferring it from ANSI output.

## Opt-In

The sideband protocol is off by default.

Enable it by launching cute-codex with:

```text
CUTE_CODEX_TERMINAL_PROTOCOL=osc777
```

This is intentionally an explicit opt-in so older terminals or older CuteCharm builds do not show raw OSC bytes.

## Transport

```text
ESC ] 777 ; cutecharm-cutex ; <base64url-json> BEL
```

The JSON is encoded with base64url without padding.

## Payload

Current message type:

```json
{
  "schema": "cutecharm-cutex.terminal.v1",
  "type": "composer_state",
  "seq": 1,
  "timestamp_ms": 1770000000000,
  "source": "cute-codex",
  "cols": 100,
  "rows": 30,
  "input_ready": true,
  "mode": "editing",
  "composer": {
    "visible": true,
    "focused": true,
    "text": "hello\nworld",
    "cursor_index": 8,
    "selection": { "start": 8, "end": 8 },
    "multiline": true,
    "region": { "top": 22, "bottom": 24, "left": 2, "right": 99 },
    "prompt": { "row": 22, "column": 0, "text": "› " },
    "caret": { "row": 23, "column": 6, "visible": true },
    "ime_anchor": { "row": 23, "column": 6, "visible": true },
    "wrap": {
      "width": 96,
      "first_line_column": 4,
      "continuation_column": 4,
      "visible_start_row": 0,
      "rows": 2
    }
  }
}
```

Important details:

- `cursor_index`, `selection.start`, and `selection.end` are UTF-16 code unit offsets for browser textarea compatibility.
- `caret` and `ime_anchor` are zero-based terminal cells relative to the visible xterm viewport.
- `cols` and `rows` are the current terminal size.
- `seq` is monotonically increasing for stale-frame rejection.

Modes:

- `editing`: normal interactive composer.
- `assistant_running`: agent is running; composer may still be visible, but `input_ready` is false.
- `approval_prompt`: active view requires user action.
- `hidden`: active non-composer view with no direct composer ownership.

When no composer should be owned by CuteCharm, the payload still includes `composer.visible:false` so the host can clear stale anchors.

## Validation

Targeted tests:

```bash
cd codex-rs
cargo test -p codex-tui terminal_sideband --lib
cargo test -p codex-tui custom_terminal --lib
```
