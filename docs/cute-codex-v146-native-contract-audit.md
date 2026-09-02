# cute-codex 0.146 Native Contract Audit

Date: 2026-08-07 AEST

Status: Stage 1 complete. This document freezes the 0.146 implementation
contract before product source changes begin.

## Scope And Authorities

This audit compares the immutable deployed v1444 archive with exact upstream
`rust-v0.146.0` at `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`.
It is bounded to the cute-codex source branch
`agent/cute-codex-v1-v146`. Cutex remains the owner of durable routing,
cross-host federation, submission state, and durable deduplication.

The Stage 1 exit gate is a concrete request/item/scheduling contract with no
remaining `unknown` decisions. Implementation, artifact creation, deployment,
and device acceptance remain later and distinct states.

## Stage 1 Checklist

| Task | Status | Result |
| --- | --- | --- |
| Audit native inter-agent submission and persistence | Completed | Reuse `Op::InterAgentCommunication`, typed `ResponseItem::AgentMessage`, rollout persistence, and reconstruction. |
| Audit active-to-idle scheduling | Completed | Reuse the atomic idle reservation and 0.146 completion retry; add external-mode regressions. |
| Audit app-server v2 surface | Completed | Retain one stable `thread/inter_agent_message` method; no native equivalent exists. |
| Freeze four delivery modes | Completed | Exact active/idle semantics and batching policy are recorded below. |
| Freeze message identity and deduplication | Completed | `messageId` is the canonical item id; keep bounded process-local duplicate suppression as defense in depth. |
| Freeze visible item projection | Completed | Emit canonical item lifecycle only when a turn consumes the message; retire old parallel notifications. |
| Map every other retained customization to 0.146 | Completed | Compatibility boundaries are recorded below; no old patch is replayed wholesale. |
| Establish focused baselines | Completed | Core wake tests passed 4/4; app-server history projection passed 5/5. |

## Native 0.146 Findings

- Core already owns `InterAgentCommunication`, `Op::InterAgentCommunication`,
  mailbox storage, rollout reconstruction, model-context recording, and the
  pending-work scheduler.
- `Session::on_task_finished` clears only its matching active turn and then
  retries `maybe_start_turn_for_pending_work`. The scheduler reserves the idle
  slot under the active-turn mutex, so concurrent completion/submission calls
  cannot both start turns.
- `MailboxDeliveryPhase` protects the visible-answer boundary, but the native
  boolean `trigger_turn` cannot express unconditional active-turn exclusion for
  `after_turn` or preserve all four external modes.
- App-server exposes no public request that submits an inter-agent message.
  Raw response-item notifications are opt-in and are not canonical item
  lifecycle/history.
- Native multi-agent tools target in-process Codex agents. They do not replace
  the Cutex Agent Bus, groups, Bridgeboard routing, or delivery receipts.
- The native status line, notification hooks, session picker, configuration
  profiles, terminal backend, and networking stack cover parts of v1444 but do
  not provide its exact launch/runtime compatibility contracts.

## Frozen App-Server V2 Contract

The stable request remains:

```text
thread/inter_agent_message
```

Request fields are required except `otherRecipients`, which defaults to an
empty array:

```text
threadId: string
messageId: non-empty string
author: AgentPath string
recipient: AgentPath string
otherRecipients: AgentPath string[]
content: string
deliveryMode: after_turn | soon | passive | interrupt
```

There is no `triggerTurn` request fallback. `deliveryMode` is authoritative.
Invalid ids or agent paths are rejected as invalid requests before submission.

The response is:

```text
submissionId: string
```

`submissionId` proves that the loaded Codex thread accepted a native
submission. It does not prove that a model turn started or consumed the
message. `messageId` remains the cross-layer correlation id.

## Frozen Delivery Semantics

| Mode | Idle receiver | Active receiver | Trigger-capable |
| --- | --- | --- | --- |
| `after_turn` | Reserve and start one synthetic turn immediately. | Retain outside the active turn, then start one follow-up after normal or interrupt cleanup. Never inject into the active turn. | Yes |
| `soon` | Reserve and start one synthetic turn immediately. | Make eligible at the next current-turn model boundary. If the visible-answer boundary has closed, consume it in one follow-up turn. | Yes |
| `passive` | Remain queued until another turn starts. | Make eligible at the next current-turn model boundary, without independently causing another model sample. | No |
| `interrupt` | Reserve and start one synthetic turn immediately. | Enqueue first, interrupt through native cleanup, then consume in exactly one replacement turn. | Yes |

The queue is FIFO. When an idle synthetic turn starts, it drains all currently
deliverable messages in FIFO order into one model turn. During an active turn,
selective draining may consume `soon` and `passive` while retaining
`after_turn`; once the native visible-answer boundary closes, all remaining
mail waits for the next turn. A later message can race with completion, but the
native active-turn reservation still permits at most one starter.

## Core Representation And Compatibility

- Add `InterAgentDeliveryMode` to `codex-protocol` with snake-case wire values.
- Add optional `delivery_mode` metadata to `InterAgentCommunication`.
  Existing native constructors keep their boolean behavior; external requests
  use a mode-aware constructor. `resolved_delivery_mode()` maps legacy
  boolean-only values to `soon` or `passive`.
- Keep `trigger_turn` internally for native compatibility and derive it from
  an explicit delivery mode. Only `passive` resolves to false.
- Convert external `messageId` to the permissive existing `ResponseItemId` so
  the raw response item, canonical turn item, bus audit, and management
  projection share one id.
- Keep a bounded 4,096-id in-process recent set in the mailbox. A duplicate
  submission returns its normal submission receipt but does not enqueue,
  interrupt, emit an item, or start a turn. Cutex remains the durable dedup
  authority across process restarts.
- Preserve native encrypted internal agent messages. Canonical external item
  materialization applies only to identified plaintext deliveries and does not
  expose encrypted payloads.

## Canonical Item Lifecycle

When a turn actually consumes an identified external message, core emits one
standard `ItemStarted` followed by one `ItemCompleted` for a typed
`InterAgentMessage` turn item:

```text
type: interAgentMessage
id: messageId
author: string
recipient: string
otherRecipients: string[]
content: string
deliveryMode: after_turn | soon | passive | interrupt
```

The item intentionally omits `submissionId` and redundant `triggerTurn`.
App-server projects it through the ordinary `item/started` and
`item/completed` methods. The completed item is persisted for both legacy and
paginated history modes so `thread/read`, item pagination, replay, and live TUI
all observe the same typed object.

Do not restore `thread/interAgentMessage/sent` or
`thread/interAgentMessage/received`. Outbound `cutex_agent_send` remains a
normal tool lifecycle plus a Cutex bus receipt; adding an extra sender card
would duplicate that truth.

## Scheduling And Race Invariants

1. Enqueue and local duplicate acceptance happen before scheduling decisions.
2. `after_turn` filtering is per message; it must not globally block a later
   `soon` message while the current-turn mailbox phase is still open.
3. Normal completion, abort cleanup, and direct idle submission all call the
   same pending-work scheduler.
4. The scheduler rechecks deliverable trigger mail before and after its atomic
   idle reservation; no empty synthetic turn is allowed.
5. `interrupt` uses native cancellation/cleanup, then the same scheduler. A
   second scheduler call is harmless because the reserved/active slot wins.
6. Receipt, item materialization, and turn start/completion remain separately
   observable states.

## Remaining Compatibility Boundaries

| Patch units | Frozen 0.146 implementation boundary |
| --- | --- |
| P01-P02 | Add the parallel `cute-codex` executable/build identity and exact `CODEX_CONFIG_FILE`/`CODEX_AUTH_FILE` launch compatibility without renaming upstream crates. |
| P03 | Add SOCKS support through the shared reqwest feature set and map `CUTE_CODEX_FORCE_HTTP_TRANSPORT` into the current responses transport selection. |
| P04 | Add only missing external catalog/profile/runtime values to the native status-line item framework. |
| P05 | Map compatible HTTP idle/approval/exit notifications onto current lifecycle sources; do not restore duplicate generic events. |
| P06 | Keep one terminal writer for synchronized frames, position before showing the cursor, and add the opt-in CuteCharm sideband at current composer/keymap boundaries. |
| P07 | Add a local-session `all` provider-filter setting that feeds native `ProviderFilter`; preserve native remote behavior. |
| P08 | Retain the transitional runtime heartbeat contract without changing Windows lifecycle ownership. |
| P09-P10 | Port `cutex_agent_list` and `cutex_agent_send` as external-bus tools using the current tool registry and localhost authenticated HTTP boundary. |
| P11 | Use native 0.146 serialization, rollout, memory, and multi-agent scheduling; do not copy old core machinery. |
| P12-P16 | Implement the frozen request, modes, queue, item lifecycle, regressions, and generated schemas described above. |
| P17-P18 | Retire historical patch artifacts and the archive-only broken symlink. |

## Just-In-Time Implementation Slices

1. Protocol and queue slice: delivery enum/metadata, bounded deduplication,
   selective drain, interrupt sequencing, canonical core turn item, and core
   unit/session regressions.
2. App-server slice: v2 request/response wiring, item projection/history,
   request and lifecycle integration tests, then repository-native schema
   generation.
3. TUI slice: render the canonical inbound item consistently for live events,
   replay, transcript, status previews, and resume previews; no old custom
   notification families.
4. External tool slice: port `cutex_agent_list` and `cutex_agent_send` against
   the 0.146 registry with focused HTTP/parser/receipt tests.
5. Launch/network/UI compatibility slices: P01-P08 in small independent
   commits, each with its own focused tests.

No slice may silently expand into Cutex, Waveline, deployment, runtime restart,
or Windows artifact work.

## Required Focused Regression Matrix

- request serialization, required `deliveryMode`, non-empty `messageId`, and
  invalid AgentPath rejection;
- idle `after_turn` starts exactly one turn and emits one canonical item;
- active `after_turn` is absent from the current turn and wakes once after
  normal completion;
- completion/submission race starts at most one follow-up;
- multiple retained messages batch FIFO into one follow-up;
- `passive` never wakes idle and can piggyback on later work;
- `soon` retains native active-boundary behavior;
- `interrupt` aborts once, consumes the message once, and produces no empty or
  repeating phantom turn;
- duplicate `messageId` neither re-materializes nor wakes;
- submission response precedes and remains distinct from item/turn lifecycle;
- legacy and paginated `thread/read`/item projection preserve the typed item;
- live TUI and replay render the same inbound item once.

## Baseline Evidence

- `just test -p codex-core queued_inter_agent_mail`: 3 passed.
- `just test -p codex-core trigger_turn_mailbox_mail`: 1 passed.
- `just test -p codex-app-server-protocol thread_history_projection`: 5 passed.
- The release-stamped test-only `Cargo.lock` rewrite was reversed exactly after
  each baseline; the product tree was clean before this audit checkpoint.

## Rollback And Stop Conditions

Stage 1 is documentation-only. Rollback is to leave the isolated branch unused.
Stop on a contract mismatch, a second real failure of the same automation path,
an ownership conflict, cross-repository mutation need, stale live-state
dependency, or owner pause. Do not infer deployment readiness from source or
test completion.
