# Patch 07: idle notify service

Quick reference for the `cute-codex` HTTP idle notification service.

## Current Behavior

`cute-codex` POSTs JSON to the configured notify endpoint when selected TUI lifecycle events occur. The endpoint is disabled unless `notify_service_url` is set.

Statuses:

- `task_completed`: agent finished a turn and no queued follow-up or active goal immediately starts another turn.
- `thinking_too_long`: user touched the composer after task completion, then left the draft unchanged for the long composer timeout.
- `waiting_approval`: command or patch approval is waiting.
- `session_exit`: TUI exits and total tokens meet the configured minimum.
- `connection_error`: reserved status in the payload enum; not normally emitted by the current wiring.
- `session_started`: opt-in event emitted after a normal session is configured.
- `user_message_sent`: opt-in event emitted when a user message is submitted or queued.
- `turn_started`: opt-in event emitted when a new turn starts.
- `turn_completed`: opt-in event emitted immediately when a turn completes. This is separate from delayed `task_completed` idle notification.
- `turn_interrupted`: opt-in event emitted when a turn is interrupted.
- `turn_failed`: opt-in event emitted when a turn fails or the backend reports an error.
- `approval_requested`: opt-in event emitted immediately when an exec, patch, or elicitation approval request appears.
- `hook_started`: opt-in event emitted when a configured hook starts.
- `hook_completed`: opt-in event emitted when a configured hook completes.

Idle timing rules:

- Short idle timeout defaults to `60` seconds.
- Composer/draft idle timeout defaults to `600` seconds.
- Approval notify timeout defaults to `30` seconds.
- If the composer has not changed since task completion, `task_completed` uses the short timeout.
- If the composer changes after task completion, the short `task_completed` notify is suppressed and long `thinking_too_long` is used instead.
- Typing and then deleting back to an empty composer still counts as composer activity, so it does not fall back to short idle.
- Approval waits use the approval timeout even if the composer changes.
- Approval waits are canceled if the user approves or denies before the timeout.
- Command or patch execution begin events also cancel approval waits as a defensive fallback.
- Idle notifications are driven by an independent async `AppEvent` timer, with redraw ticks retained only as an opportunistic fallback.
- Session exit invalidates any pending idle timer.
- `session_exit` sends immediately on exit when token usage passes the minimum, using a bounded awaited send of 2 seconds.
- Optional non-idle lifecycle events send immediately and use `idle_seconds = 0`.
- `user_message_sent` clears any pending idle state, because the user has returned and submitted input.

## Configuration

Config lives under `[tui]` because the notify settings are flattened into the TUI config.

```toml
[tui]
notify_service_url = "http://127.0.0.1:18765/api/agent-notify/push"
notify_service_token = ""
notify_service_idle_timeout_secs = 60
notify_service_composer_idle_timeout_secs = 600
notify_service_approval_timeout_secs = 30
notify_service_agent_name = "codex"
notify_service_min_tokens = 100
notify_service_events = [
  "task_completed",
  "thinking_too_long",
  "waiting_approval",
  "session_exit",
]
notify_service_user_message_content = "none"
notify_service_user_message_preview_chars = 200
```

Environment overrides:

- `CODEX_NOTIFY_SERVICE_URL`
- `CODEX_NOTIFY_SERVICE_TOKEN`
- `CODEX_NOTIFY_IDLE_TIMEOUT`
- `CODEX_NOTIFY_COMPOSER_IDLE_TIMEOUT`
- `CODEX_NOTIFY_APPROVAL_TIMEOUT`
- `CODEX_NOTIFY_AGENT_NAME`
- `CODEX_NOTIFY_MIN_TOKENS`
- `CODEX_NOTIFY_EVENTS`
- `CODEX_NOTIFY_USER_MESSAGE_CONTENT`
- `CODEX_NOTIFY_USER_MESSAGE_PREVIEW_CHARS`

Priority is defaults, then config file, then environment overrides.

`notify_service_events` is an allowlist. The default keeps the original idle/exit behavior. Set `CODEX_NOTIFY_EVENTS` to a comma-separated list to opt into more event types, for example:

```bash
CODEX_NOTIFY_EVENTS=task_completed,thinking_too_long,waiting_approval,session_exit,session_started,user_message_sent,turn_started,turn_completed
```

`notify_service_user_message_content` controls whether `user_message_sent` includes user text:

- `none`: default. Includes metadata only.
- `preview`: includes `event_details.user_message.text_preview`, capped by `notify_service_user_message_preview_chars`, plus `text_truncated`.
- `full`: includes `event_details.user_message.text`.

## POST Format

Example payload:

```json
{
  "status": "task_completed",
  "project_name": "cutex",
  "agent_name": "codex",
  "session_name": "optional thread name",
  "cwd": "/path/to/project",
  "host_name": "hostname",
  "thread_id": "optional-session-uuid",
  "duration_seconds": 7,
  "session_duration_seconds": 1234,
  "turn_duration_seconds": 7,
  "total_tokens": 12345,
  "input_tokens": 10000,
  "output_tokens": 2000,
  "cached_input_tokens": 300,
  "reasoning_output_tokens": 45,
  "idle_seconds": 60,
  "completed_at": "2026-05-07T10:26:00Z",
  "event_details": {
    "turn": {
      "follow_up_started": false,
      "active_goal_continuing": false
    }
  }
}
```

Field notes:

- `duration_seconds`: compatibility field. It is the current turn duration when available; otherwise it falls back to session duration.
- `turn_duration_seconds`: current turn duration. Omitted when there is no current turn duration, such as `session_exit`.
- `session_duration_seconds`: total time since the Codex TUI session was configured.
- `idle_seconds`: seconds of idle time before the notify fired. `session_exit` uses `0`.
- `session_name`: optional and omitted when there is no thread name.
- `thread_id`: optional and omitted until a session id is known.
- `event_details`: optional structured details for the event. Receiver should ignore unknown nested fields.
- `completed_at`: UTC RFC3339 timestamp.

HTTP behavior:

- Request method is `POST`.
- Body is JSON.
- If `notify_service_token` is non-empty, `Authorization: Bearer <token>` is sent.
- Ordinary idle notifications are sent from a background task.
- `session_exit` is awaited during shutdown for up to 2 seconds.
- Receiver should ignore unknown fields for forward compatibility.

Known `event_details` shapes:

- `session_started`: `session.model`, `session.model_provider_id`, `session.resumed`, `session.forked_from_id`.
- `user_message_sent`: `user_message.has_text`, `text_chars`, `local_image_count`, `remote_image_count`, `mention_count`; optional `text_preview`/`text_truncated` or `text`.
- `turn_started`: `turn.turn_id`.
- `turn_completed`: `turn.follow_up_started`, `turn.active_goal_continuing`.
- `approval_requested`: `approval.type`, plus type-specific ids/command fields when available.
- `hook_started`: `hook.event_name`, `handler_name`, `execution_id`, `scope`.
- `hook_completed`: `hook.event_name`, `handler_name`, `execution_id`, `status`, `duration_ms`.

## Implementation Map

- `codex-rs/config/src/types.rs`: `NotifyServiceSettings`, defaults, config schema.
- `codex-rs/core/src/config/mod.rs`: config merge and env overrides.
- `codex-rs/tui/src/notify_service.rs`: payload type, status enum, HTTP POST helper.
- `codex-rs/tui/src/chatwidget.rs`: idle state machine, async timer scheduling, composer activity tracking, turn duration capture.
- `codex-rs/tui/src/app_event.rs` and `codex-rs/tui/src/app/event_dispatch.rs`: idle timer event routing.
- `codex-rs/tui/src/bottom_pane/mod.rs`: composer activity generation and last activity timestamp.
- `codex-rs/tui/src/app.rs`: `session_exit` notify.
- `codex-rs/tui/src/chatwidget/tests/idle_notify.rs`: focused behavior tests.

## Validation

Targeted test:

```bash
cd /path/to/cute-codex
cargo test --manifest-path codex-rs/Cargo.toml -p codex-tui idle_notify --target-dir codex-rs/target-host
```

Patch apply check from upstream base:

```bash
cd /path/to/cute-codex
BASE="$(git merge-base HEAD upstream/main 2>/dev/null || echo upstream/main)"
TMP="/tmp/codex-patch-apply-check-$$"
git worktree add --detach "$TMP" "$BASE"
rm -rf "$TMP/patches"
cp -a patches "$TMP/patches"
(cd "$TMP" && bash patches/apply.sh)
git worktree remove --force "$TMP"
```

Regenerate only patch 07 without mixing earlier feature patches into shared files:

```bash
cd /path/to/cute-codex
BASE="$(git merge-base HEAD upstream/main 2>/dev/null || echo upstream/main)"
TMP="$(mktemp -d /tmp/codex-patch07-XXXXXX)"
rmdir "$TMP"
git worktree add --detach "$TMP" "$BASE"
rm -rf "$TMP/patches"
cp -a patches "$TMP/patches"
(
  cd "$TMP"
  for p in patches/0[1-6]-*.patch; do git apply "$p"; done
  git add -A codex-rs
)
# Copy the current patch-07-owned files from this repo into "$TMP", then:
(
  cd "$TMP"
  git add -N codex-rs/tui/src/notify_service.rs codex-rs/tui/src/chatwidget/tests/idle_notify.rs
  git diff -- <patch-07-owned-files> > /path/to/cute-codex/patches/07-idle-notify-service.patch
)
git worktree remove --force "$TMP"
```

Whitespace/conflict marker check:

```bash
cd /path/to/cute-codex
git diff --check
```

## Deployment Notes

The host `sxcut` shortcut normally points directly at the release binary:

```bash
sxcut list | rg cute-codex
readlink -f "$HOME/Resources/Shortcuts/cute-codex"
```

Backup before release builds, because cargo overwrites the target in place:

```bash
CUTE_CODEX_TARGET="$(readlink -f "$HOME/Resources/Shortcuts/cute-codex")"
cp -a "$CUTE_CODEX_TARGET" "${CUTE_CODEX_TARGET}.bak-$(date +%Y%m%d-%H%M%S)"

cd /path/to/cute-codex/codex-rs
CUTE_CODEX_BUILD_TAG=NOTIFY1 cargo build -p codex-cli --bin cute-codex --release --target-dir target-host

"$HOME/Resources/Shortcuts/cute-codex" --version
strings target-host/release/cute-codex | rg "NOTIFY1"
```

`cute-codex --version` still reports the official semver, currently `0.128.0`. The local build tag is visible in the TUI header and embedded in the binary.
