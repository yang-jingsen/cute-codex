# cute-codex v1444 Patch Inventory

Date: 2026-08-07 AEST

Status: Stage 1 classification complete. Every deployed delta is reconciled,
no `unknown` remains, and implementation status is recorded through P08. Exact
replacement API shapes are frozen in `NATIVE-CONTRACT-AUDIT.md`.

## Authorities

- Deployed source:
  `../../releases/cute-codex-0.144.1-cheers-inter-agent-notifications-v1444-260714-src.tgz`
- Deployed source SHA-256:
  `a6d991ed3d9caf6d02c2eff247e94100320283a632f3dd9a0507d63f17d391af`
- Manifest-recorded source commit:
  `5a48beaa8a43def7a5348b8a01b1e3c79ab675db`
- Clean old upstream tag object:
  `db75c19352d29ef29c17dbcf73a7244f1b1a8d10`.
- Clean old upstream peeled commit:
  `44918ea10c0f99151c6710411b4322c2f5c96bea`.
- New upstream reference: `rust-v0.146.0` at
  `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`.

The dirty `../../codex-0144-cheers` worktree is explicitly excluded. Contract
documents provide expected categories but do not replace a complete immutable
source diff.

## Classification Rules

- `native-0.146`: upstream 0.146 provides equivalent required behavior.
- `retain`: the deployed behavior is still required and lacks a native
  equivalent; port it using 0.146 architecture.
- `replace`: the requirement remains but the old implementation or API shape
  must be superseded.
- `retire`: the deployed customization is redundant or no longer required.
- `unknown`: evidence is incomplete. No implementation begins while any entry
  remains unknown.

Generated schemas and snapshots are linked to the semantic patch that produced
them rather than silently discarded as noise.

## Diff Reconciliation

The complete no-index comparison between the official 0.144.1 tree and the
immutable v1444 archive contains 142 changed paths:

- 115 `codex-rs/` paths represented by the archive's rollup patch;
- 11 additional semantic/generated `codex-rs/` paths from the final v1444
  archive delta;
- one archive-packaging-only broken symlink target at
  `codex-rs/vendor/bubblewrap/LICENSE`;
- 15 `patches/` scripts, notes, and patch artifacts.

This reconciles every changed path. Seventeen of the 127 `codex-rs/` paths are
generated app-server schemas and belong to their protocol source unit.

The archive is not byte-equivalent to manifest commit `5a48beaa`. Relative to
that immutable commit it contains 26 changed paths (1,085 additions and 195
deletions), including nine generated schema paths and the final delivery-mode,
notification, queue, persistence, and TUI changes. Therefore the archive, not
the commit or dirty historical worktree, remains the deployed source authority.

## Complete Patch Units

The numbered historical patches are feature evidence; the rollup plus the final
archive delta is the file-completeness authority. `replace` means preserve the
required behavior using native 0.146 architecture rather than replaying old
hunks.

| ID | Old files / behavior | 0.146 evidence | Classification | Validation / notes |
| --- | --- | --- | --- | --- |
| P01 | `01`, `02`, and `06`: `cute-codex` bin target, CLI/TUI branding, resume command text, optional build tag. | 0.146 exposes only the `codex` bin and upstream display/version strings. | retain | Keep the official semver and add the parallel project identity/build tag without renaming upstream crates. |
| P02 | `04` plus CLI entry wiring: `CODEX_CONFIG_FILE` and `CODEX_AUTH_FILE` per-profile isolation. | 0.146 has native named config profiles and configurable loader overrides internally, but no equivalent auth-file override or compatible environment contract. | retain | Completed at `df92174eef` and `941b821e23`: selected config routing plus isolated file/direct-keyring/encrypted-secret/ephemeral auth identity; Cutex launch code is unchanged. |
| P03 | `05`: SOCKS-enabled reqwest clients and `CUTE_CODEX_FORCE_HTTP_TRANSPORT`. | 0.146 can disable Responses WebSockets through feature/provider configuration, but has no compatibility env switch and its shared reqwest features do not include SOCKS. | replace | Completed at `ce8fd23c1a`: retained reqwest owners enable SOCKS; forced HTTP feeds native Responses selection, explicit realtime rejection, and implicit-daemon routing; focused transport/HTTP/MCP gates and final scoped Clippy passed. |
| P04 | `03`: arbitrary catalog-driven custom status items from env, files, and launch metadata. | 0.146 now has a rich native configurable status-line framework, but not the external catalog or launch-profile/runtime values. | replace | Completed at `f3b2e42704`: catalog sources/render/styles, launch-profile/runtime built-ins, native picker/preview/persistence, alias/deduplication, and invalid-item handling are integrated without a parallel status surface; catalog/status-line tests and final scoped Clippy passed. |
| P05 | `07`: delayed idle/approval/exit HTTP notifications and optional lifecycle event payloads. | 0.146 has native turn-complete notify commands, desktop notifications, hooks, and app-server lifecycle events, but not the deployed HTTP schema, composer-idle rules, or full event allowlist. | replace | Preserve compatibility-only behavior using current lifecycle sources; retire duplicated legacy event plumbing. |
| P06 | `08`: same-writer synchronized frames, cursor-position-before-show, and opt-in CuteCharm OSC 777 composer/keymap sideband. | 0.146 uses synchronized updates but still opens them on a separate stdout handle and still shows the cursor before moving it; no CuteCharm sideband exists. | replace | Port narrowly around the current terminal backend and composer/keymap architecture. |
| P07 | `09`: configurable all-provider session picker for Cutex profiles/custom providers. | 0.146 natively owns `ProviderFilter` and omits provider filtering for remote workspaces, but local selection still always matches the default provider and has no `all` setting. | retain | Add only the missing config switch and feed the native filter path. |
| P08 | Final 0.144 cutover: transitional runtime heartbeat with launch/session/profile/host/agent metadata. | No 0.146 equivalent. The frozen contract explicitly retains it until authenticated Windows runtime ownership replaces `host_foreground`. | retain | No Windows build or lifecycle mutation is part of this source port. |
| P09 | Built-in `cutex_agent_list` external Agent Bus tool. | Native 0.146 `list_agents` covers in-process subagents, not Cutex registration, groups, Bridgeboard hosts, or runtime endpoints. | retain | Adapt to the 0.146 tool registry; keep the localhost-only authenticated HTTP boundary. |
| P10 | Built-in `cutex_agent_send` external Agent Bus tool and structured bus receipt. | Native `send_message` targets in-process subagents and cannot replace Cutex cross-host routing/delivery receipts. | retain | Preserve `message_id`, delivery mode, queue, trigger, and dedup receipt fields. |
| P11 | Core `InterAgentCommunication`, typed agent-message conversion, rollout persistence/reconstruction, truncation, and native multi-agent scheduling base. | 0.146 provides a substantially richer native implementation across protocol/core/rollout/state/memory with tests. | native-0.146 | Do not copy old serialization or rollout patches. Add only metadata that the external Cutex contract demonstrably lacks. |
| P12 | `thread/inter_agent_message` request returning `submissionId`. | 0.146 core accepts `Op::InterAgentCommunication`, but app-server exposes no public request that submits it. | retain | Implement as a v2 extension mapped onto native core submission and current app-server error/ID patterns. |
| P13 | External `messageId`, four delivery modes, bounded duplicate acceptance, and `after_turn` exclusion from an active turn. | Native communication carries an optional response-item id and a trigger boolean, but no four-mode metadata, external idempotency contract, or unconditional active-turn deferral for `after_turn`. | replace | Keep delivery metadata at the narrow protocol/core boundary and reuse native queue phase/scheduler primitives. Cutex remains the durable dedup owner. |
| P14 | Missing normal-completion wake retry diagnosed in 0.144.1. | 0.146 `on_task_finished` atomically clears the matching active turn and then invokes `maybe_start_turn_for_pending_work`; interrupt paths also retry. | native-0.146 | Add exact external-arrival, race, batching, passive/soon, and phantom-turn regression coverage before accepting the native fix. |
| P15 | Custom sent/received protocol events, app-server notifications, and TUI message cards. | 0.146 provides native tool lifecycle and native agent-message persistence, but raw inter-agent response items are not materialized live as canonical TUI items and lack delivery metadata. | replace | Retire both parallel notification families and the duplicate outbound card. Materialize consumed inbound delivery as one canonical `InterAgentMessage` v2 item with the external message id and delivery metadata. |
| P16 | Generated app-server JSON/TypeScript schemas and pending-input snapshots. | 0.146 has native generators and substantially changed fixtures. | replace | Regenerate from retained semantic types; never copy generated v1444 files. |
| P17 | `patches/` rollup, numbered historical patches, scripts, and feature notes. | These are replay/evidence artifacts tied to 0.144.1, not runtime behavior. | retire | Preserve only this inventory and exact immutable archive reference; do not port patcher bytes. |
| P18 | Archive-only `vendor/bubblewrap/LICENSE` symlink target changed from `COPYING` to `codex-0144-cheers/COPYING`. | Official 0.144.1, manifest commit, and 0.146 all use the correct relative `COPYING` target. | retire | Packaging transform artifact; never reproduce it. |

## Evidence Log

- 2026-08-07: Deployed binary and source-archive SHA-256 matched
  `BUILD_INFO.txt` exactly.
- 2026-08-07: Official `rust-v0.146.0` tag object and peeled commit matched the
  handoff, and the isolated branch was created cleanly.
- 2026-08-07: Official `rust-v0.144.1` resolved to tag object `db75c193...`
  and peeled commit `44918ea1...`; both old/new reference trees were extracted
  outside `source/` and made read-only.
- 2026-08-07: Reconciled all 142 changed paths and proved the v1444 archive has
  a 26-path delta beyond manifest commit `5a48beaa`; classifications above use
  the archive as authority.
- 2026-08-07: Native 0.146 wake baselines passed with
  `just test -p codex-core queued_inter_agent_mail` (3/3) and
  `just test -p codex-core trigger_turn_mailbox_mail` (1/1). The test-only
  lockfile version rewrite was reversed, leaving the product tree unchanged.
- 2026-08-07: Stage 1 froze all replacement boundaries. The critical P12-P15
  contract uses the retained request method, native core submission/scheduler,
  selective mode-aware queueing, and a canonical item rather than old custom
  notifications.
- 2026-08-07: P02 completed at `df92174eef` and `941b821e23`. Default config
  and auth behavior remains native 0.146; nonblank launch-file overrides are
  relative to `CODEX_HOME`, consistently routed, and isolated across supported
  credential stores. Touched crate and focused regression gates passed.
- 2026-08-08: Final candidate evidence is recorded in the upgrade blueprint.
  The single retained Linux artifact is `cute-codex 0.146.0` at SHA-256
  `18071b20db96c27ce03466ea036ccf98fd229ad48e02d8575b1dfa2b56cc14e3`; the
  source archive hash is
  `1ed05eb467362caa09646bfe412d0b37138a4efd68a9e4270b98731776a2a423`.
  Candidate-compatible app-server smoke passed; the legacy Cutex harness's
  retired notification expectation is documented as a consumer adaptation.
