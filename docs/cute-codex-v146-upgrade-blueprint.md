# cute-codex 0.146 Upgrade Blueprint

Date: 2026-08-13 AEST

Status: Stages 0-5 remain complete. The post-closeout Stage 6 compaction
continuity repair is committed at `b27d06efd0`, and a new immutable Linux
`cute-codex` candidate is built and hashed. The worktree retains only a known,
excluded workspace-version `Cargo.lock` rewrite. No deployment is authorized
by this document.

## Objective And Acceptance

Upgrade the deployed cute-codex 0.144.1 customization set onto the exact stable
upstream `rust-v0.146.0` base, retain only behavior that 0.146 does not provide
natively, and correct the `after_turn` active-to-idle wake boundary.

The work is ready for integration review only when every deployed v1444 change
is classified, retained behavior and the wake correction are implemented and
tested, the branch is committed and clean, and an immutable Linux candidate plus
rollback evidence has been recorded. Artifact creation is not deployment.

## Source Authority And Branch Strategy

- Upstream tag object: `be449751a978f02e5bbba886999662956c7f38f5`.
- Upstream peeled commit: `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`.
- Development branch: `agent/cute-codex-v1-v146`.
- Development worktree: `source/`.
- Deployed v1444 source archive SHA-256:
  `a6d991ed3d9caf6d02c2eff247e94100320283a632f3dd9a0507d63f17d391af`.
- Deployed binary SHA-256:
  `9acb2d40be0bf4486ac94706bc55f53943508fdda6c28440568355c89536ecbe`.
- Mutable historical worktrees are evidence only and are not source authority.

All product edits and commits belong only to `source/`. Immutable comparison
references live outside `source/` and must never be used as writable development
trees. The shared `../../codex` checkout must not be switched or edited.

## Ownership Boundary

This session owns only the cute-codex 0.146 source upgrade. It does not modify
Cutex, Waveline, backend/frontend code, tethysUNE, cute-alden, live services,
runtime configuration, deployment state, or Windows artifacts. A required
Cutex-side adaptation is recorded as an exact contract handoff, not implemented
here.

## Quality And Validation Rules

- Audit native 0.146 behavior before porting any old implementation.
- Preserve app-server v2 method, request, result, thread, turn, and item concepts.
- Keep model-context additions bounded and represented by the repository's
  contextual fragment abstractions.
- Port in coherent commits; do not cherry-pick the old patch branch wholesale.
- Regenerate protocol artifacts with repository-native generators.
- Run focused tests after each slice and record the exact command and result.
- Use `just` rather than direct `cargo test`; request owner approval before the
  final complete workspace suite as required by upstream `AGENTS.md`.
- Run formatting and scoped fixes after code changes in the order prescribed by
  upstream instructions.
- Keep source implementation, test success, artifact creation, deployment, and
  physical acceptance as distinct states.

## Durable Stages

| Stage | Status | Exit gate |
| --- | --- | --- |
| 0. Baseline and patch inventory | Completed | Exact references verified; every v1444 delta classified; native wake baselines pass; no product source edits. |
| 1. Native 0.146 contract audit | Completed | Concrete native contract diff and finalized retain/replace/retire decisions. |
| 2. Retained extension port | Completed | P01-P08 and the retained protocol/core/app-server/TUI slices are committed; focused gates and scoped Clippy passed. |
| 3. `after_turn` wake correction | Completed | Scheduling, race, batching, passive/soon, interrupt, deduplication, and idle-reservation regressions pass without phantom turns. |
| 4. Regression and immutable Linux candidate | Completed | Formatting/generated checks, focused gates, one full feasible workspace attempt, candidate build/hash, and disposable canonical-item smoke are recorded; host-only failures remain explicit. |
| 5. Integration handoff | Completed | Exact product checkpoint, contract difference, test receipts, candidate/source hashes, rollback, and the bounded Cutex consumer adaptation are recorded below. |
| 6. Compaction continuity repair | Completed | Post-0.146 releases and main were audited; a removable prompt/error/input-preservation patch, focused regressions, and a new immutable Linux candidate are recorded. |

Later stages remain intentionally high-level until the prior stage fixes the
actual 0.146 architecture and API shape.

## Stage 6 Result

Official `rust-v0.146.1`, `rust-v0.147.0`, `rust-v0.148.0-alpha.9`, and
upstream main `b1373b74...` retain the affected compaction semantics. Open
issue #37394 reproduces loss of the active task frontier on `0.147.0`; #25619
records the silent completed/null failure boundary. Exact revisions, issue
links, non-goals, and removal gates are in
`docs/cute-codex-v146-compaction-continuity-patch.md`.

Product checkpoint `b27d06efd09c5bc593cd82bb9aeb62303def0941` strengthens
the local compact prompt/prefix so active work resumes from its exact frontier.
Compaction failure now ensures one affecting wire error, and pre-sampling
failure records accepted input without automatic requeue loops. The broader
#32888 token-accounting redesign is explicitly excluded.

`codex-prompts` passed 36/36 and the compact-filtered `codex-core` gate passed
167/167. The focused `after_turn` failure regression also proved exactly one
error, a failed `TurnComplete`, recorded mail, and no follow-up model request.
Formatting, scoped fixes, and diff checks passed. The new clean-archive,
`cute-codex`-only candidate is 1,435,359,944 bytes with SHA-256
`15d42e0616d0bbc414f2760e49b89093855300fe5f89f25da86944052f718dad`;
its source archive SHA-256 is
`3d6e3c0e6d9f0a75449308a6b286610fcf8419c8c2f4e4816817f07605d575f6`.
No full workspace rerun, deployment, live mutation, Cutex change, or Windows
build occurred.

## Current Stage 0 Plan

Goal: establish reproducible immutable old/new references and classify the
complete deployed customization set before any implementation decision.

| Task | Status | Notes |
| --- | --- | --- |
| Read local and required coordination instructions | Completed | Read current brief, handoff, both AGENTS files, retrospective, wake diagnosis, and contract matrix. |
| Verify upstream 0.146 tag object and peeled commit | Completed | `git ls-remote`, exact tag-only fetch, and local `rev-parse` matched recorded hashes. |
| Verify deployed binary/archive provenance | Completed | Both SHA-256 values matched `BUILD_INFO.txt`; mutable 0.144 worktree excluded. |
| Create isolated 0.146 worktree | Completed | Clean `agent/cute-codex-v1-v146` at `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`. |
| Resolve clean upstream 0.144.1 reference | Completed | Official tag object `db75c193...`, peeled commit `44918ea1...`, fetched and verified. |
| Extract immutable v1444 source reference | Completed | Old/new/manifest-commit trees live under read-only `reference/` outside `source/`. |
| Produce complete file and semantic diff | Completed | Reconciled 142 paths; archive has 26 paths beyond manifest commit, so archive remains authority. |
| Classify every patch unit | Completed | Eighteen semantic/evidence units recorded with no `unknown` classifications; Stage 1 will freeze exact retained API shapes. |
| Commit Stage 0 checkpoint | Completed | Blueprint and inventory snapshots are committed on the isolated source branch; product source remains unchanged. |

Validation:

- `git -C source status --short --branch`
- `git -C source rev-parse HEAD`
- exact SHA-256 checks for the deployed binary and v1444 source archive
- exact official tag object/peeled commit checks for 0.144.1 and 0.146.0
- reproducible diff summary and zero remaining `unknown` classifications
- `just test -p codex-core queued_inter_agent_mail`: 3 passed, 3,114 skipped
- `just test -p codex-core trigger_turn_mailbox_mail`: 1 passed, 3,116 skipped
- post-test `git -C source diff --exit-code -- codex-rs/Cargo.lock`: clean

The release-tag test invocation rewrote only workspace package versions in
`Cargo.lock` from their checked-in `0.0.0` placeholders. That incidental
rewrite was reversed exactly before this checkpoint; it is not part of the
upgrade.

## Stage 1 Result

The exact v2 request, delivery-mode semantics, core queue invariants, canonical
item lifecycle, persistence policy, compatibility fields, implementation
slices, and focused regression matrix are frozen in
`NATIVE-CONTRACT-AUDIT.md`. The committed source snapshot is
`docs/cute-codex-v146-native-contract-audit.md`.

Stage 2 may now edit product source, but only in the coherent slices recorded
there. In particular, it must not restore the old sent/received notification
families or treat a submission receipt as proof of model consumption.

## Current Stage 2 Plan

Goal: implement the frozen contract and remaining compatibility extensions as
small independently testable commits.

| Task | Status | Exit gate |
| --- | --- | --- |
| Protocol/core delivery slice | Completed | Four modes, bounded deduplication, selective drain, canonical item lifecycle/persistence, interrupt sequencing, and focused regressions pass. |
| App-server v2 slice | Completed | Request/response, item projection/history, integration tests, and regenerated schemas pass. |
| Canonical TUI item slice | Completed | Live/replay/transcript/status/resume views render one inbound item; focused TUI tests pass. |
| External Cutex Agent Bus tools | Completed | `cutex_agent_list`/`cutex_agent_send` compile and focused parser/HTTP/receipt tests pass. |
| Launch/network/runtime compatibility | In Progress | P01-P08 are implemented and checkpointed; final workspace/artifact gates remain pending. |

The first slice does not add app-server request wiring or TUI rendering. To
respect the repository change-size guidance, its checkpoint is split by
responsibility: protocol/queue/canonical primitives first, then wake-boundary
scheduling plus integration regressions. Rollback is reverting those two
ordered commits.

Validation:

- `just test -p codex-protocol inter_agent_delivery_modes_serialize_and_resolve_legacy_flags`: 1 passed.
- `just test -p codex-core input_queue --retries 0`: 9 passed.
- `just test -p codex-core inter_agent_message --retries 0`: 3 passed.
- `just test -p codex-core pending_input --retries 0`: 18 passed.
- `just test -p codex-core queued_inter_agent_mail --retries 0`: 3 passed.
- `just test -p codex-core trigger_turn_mailbox_mail --retries 0`: 1 passed.
- `just test -p codex-app-server-protocol thread_history_projection --retries 0`: 5 passed.

## Current App-Server V2 Slice Plan

Goal: expose the frozen `thread/inter_agent_message` acceptance boundary on
the current 0.146 app-server without restoring the retired sent/received
notification families or weakening the canonical item lifecycle.

| Task | Status | Exit gate |
| --- | --- | --- |
| Add the stable v2 request and response types | Completed | Required `deliveryMode`, non-empty `messageId`, AgentPath validation, optional `otherRecipients`, and `ResponseItemId` conversion have protocol coverage. |
| Route the request to native core submission | Completed | Thread-scoped serialization loads one thread, submits one `Op::InterAgentCommunication`, and returns only `submissionId`. |
| Cover the public JSON-RPC lifecycle | Completed | Acceptance receipt precedes consumption lifecycle; invalid inputs do not submit; canonical item/history and duplicate behavior are observed through app-server. |
| Document and regenerate the API | Completed | `app-server/README.md` and repository-generated stable schemas contain the method and types, with no legacy notification methods. |
| Validate and checkpoint the slice | Completed | Pre-format focused tests passed; `just fmt` and scoped `just fix` completed; version-only lockfile noise was reversed exactly. |

Validation:

- `just test -p codex-app-server-protocol thread_inter_agent_message --retries 0`: 4 passed.
- `just test -p codex-app-server thread_inter_agent_message --retries 0`: 3 passed.
- `just write-app-server-schema`: completed successfully.
- `just test -p codex-app-server-protocol --retries 0`: 278 passed.
- full workspace `just test` remains deferred until explicit owner approval

## Current Canonical TUI Slice Plan

Goal: render the one persisted inbound `InterAgentMessage` item consistently
across live app-server completion events, thread replay, full transcripts,
`/agent` activity previews, and resume-picker conversation previews. The slice
must not restore the retired sent/received notification families or add an
outbound message card.

| Task | Status | Exit gate |
| --- | --- | --- |
| Add one bounded shared presentation model | Completed | The canonical fields drive a reusable history card and 240-grapheme compact summary; card content and metadata are capped before rendering. |
| Route canonical completion through live and replay | Completed | `item/completed` and replay use the same history-cell path, while `item/started` remains non-rendering and one item produces one card. |
| Reuse the presentation in reconstructed views | Completed | Full transcript, `/agent` status preview, and resume-picker conversation preview preserve author/content identity without independent formatting drift. |
| Add and review visual regressions | Completed | Five intended `insta` snapshots cover the card plus live/replay equality, transcript reconstruction, status preview, and resume preview; no pending snapshots remain. |
| Validate and checkpoint the slice | Completed | Focused TUI tests pass 6/6; the crate gate result and unrelated blockers are recorded; five snapshots are accepted; final `just fmt` and zero-warning `just fix -p codex-tui` passed with lockfile noise reversed. |

Validation:

- focused `just test -p codex-tui <inter-agent filters> --retries 0`
- `just test -p codex-tui --retries 0`
- `cargo insta pending-snapshots --manifest-path tui/Cargo.toml` followed by targeted review and acceptance
- final `just fmt`, then `just fix -p codex-tui`; tests are not rerun afterward per repository instructions

Pre-format results:

- `just test -p codex-tui inter_agent --retries 0`: 6 passed, 3,251 skipped
- `cargo insta pending-snapshots --manifest-path tui/Cargo.toml`: no pending snapshots after five intended snapshots were reviewed and accepted
- `just test -p codex-tui --retries 0`: 3,225 passed; 27 failed; one timed out; four skipped. Twenty-two failures were pre-existing release-tag snapshots that expected `0.0.0` but rendered `0.146.0`; all generated updates were rejected. The remaining five failures and one timeout were existing IDE socket tests whose temporary directory was rejected as writable by other users.
- `just test -p codex-tui fetch_ide_context_ --retries 0`: diagnostic rerun confirmed the unrelated environment gate with one pass, five `PermissionDenied` failures, and one dependent server-thread timeout; no IDE source was changed
- `just fmt`: passed
- `just fix -p codex-tui`: passed with no warnings after the equivalent explicit-field presenter construction cleanup; tests were not rerun afterward per repository instructions

## Current External Agent Bus Tool Slice Plan

Goal: retain `cutex_agent_list` and `cutex_agent_send` as localhost-only,
optionally authenticated external-bus tools on the current 0.146 registry. The
send result must remain a structured Cutex receipt, while outbound visibility
uses the native tool lifecycle rather than the retired sent notification/card.

| Task | Status | Exit gate |
| --- | --- | --- |
| Audit native registry/output contracts and immutable v1444 evidence | Completed | Current runtime planning, `JsonToolOutput`, tool schemas, HTTP boundary, compatibility parser, and receipt fields are classified. |
| Port the handlers, schemas, and conditional registration | Completed | Tools are exposed only when `CUTEX_AGENT_BUS_URL` is non-empty; function payloads use current runtime interfaces and no legacy protocol event is emitted. |
| Preserve and harden the local HTTP boundary | Completed | Only loopback HTTP is accepted, proxy/redirect escape is disabled, bearer auth and five-second timeout remain, and request shapes match the deployed contract. |
| Add focused schema, HTTP, parser, and receipt regressions | Completed | List/send requests, auth, query/body fields, compatibility aliases, all four delivery modes, deduplication, and structured outputs are covered. |
| Validate and checkpoint the slice | Completed | Focused core tests, final `just fmt`, and zero-warning scoped `just fix` passed; incidental lockfile noise was reversed exactly and the slice is committed as one checkpoint. |

Validation:

- `just test -p codex-core cutex_agent --retries 0`
- `just test -p codex-core tools::spec_plan::tests --retries 0`
- final `just fmt`, then `just fix -p codex-core`; tests are not rerun afterward per repository instructions
- full workspace `just test` remains deferred until explicit owner approval

## Current P01 Plan: Parallel Binary And Branding

Goal: provide the retained v1444 `cute-codex` launcher identity on the native
0.146 CLI/TUI without changing upstream crate names, semver, configuration
loading, resume policy, transport, or runtime ownership.

| Task | Status | Exit gate |
| --- | --- | --- |
| Reconcile old evidence with 0.146 entrypoints | Completed | `01`, `02`, and `06` are mapped to current CLI/TUI files; `ResumeCwdPolicy` is excluded because 0.146 already owns the equivalent `tui.resume_cwd` policy, while config/transport helpers are deferred to P02/P03. |
| Add the parallel `cute-codex` bin target | Completed | `codex` remains available; the alias shares the same entrypoint, suppresses only the duplicate unit-test harness, and builds as `cute-codex 0.146.0`. |
| Port CLI invocation branding and resume hints | Completed | Root/plugin/MCP help, completion, login guidance, remote errors, and exit/resume text use `cute-codex`; no Cutex launch code changed. |
| Port TUI display identity and optional build tag | Completed | Session/status headers and bundled tooltips use one display constant; `CUTE_CODEX_BUILD_TAG` appends a trimmed marker without changing semver/update checks. |
| Add focused CLI/TUI regressions and checkpoint | Completed | CLI crate and focused TUI/resume suites pass, snapshots are reviewed, final formatting/scoped fixes are clean, and the slice is ready for one coherent commit. |

Validation:

- `just test -p codex-utils-cli resume_command --retries 0`
- `just test -p codex-cli --retries 0`
- `just test -p codex-tui status_snapshot --retries 0`
- `just test -p codex-tui session_header --retries 0`
- `just test -p codex-tui session_summary --retries 0`
- `just test -p codex-tui resume --retries 0`
- `just fmt`
- `just fix -p codex-cli -p codex-tui -p codex-utils-cli`

The old `ResumeCwdPolicy` flags and `resolve_user_config_path` change are not
part of this slice: the former is superseded by native 0.146 resume-cwd
configuration and the latter belongs to P02's explicit config-file contract.
The P01 checkpoint does not build, deploy, or replace any artifact.

Pre-format results:

- `just test -p codex-utils-cli resume_command --retries 0`: 7 passed.
- Focused CLI branding/output filters passed, including 12 exit-message cases,
  plugin marketplace namespace help, and MCP list/get output.
- `just test -p codex-cli --retries 0`: 316 passed; the alias target builds but
  does not duplicate the shared `main.rs` unit-test harness.
- `just test -p codex-tui status_snapshot --retries 0`: 27 passed;
  `session_header`: 7 passed; `session_summary`: 4 passed; `resume`: 133 passed.
- `cargo build -p codex-cli --bin cute-codex` passed; the resulting binary
  reported `cute-codex 0.146.0` and branded root help/usage correctly.
- Test-generated workspace-version-only `Cargo.lock` changes were reversed exactly.
- No `.snap.new` files remain.
- Final `just fmt` and
  `just fix -p codex-cli -p codex-tui -p codex-utils-cli` passed; tests were
  not rerun afterward per repository instructions.

## Current P02 Plan: Profile Config And Auth-File Isolation

Goal: preserve the deployed `CODEX_CONFIG_FILE` / `CODEX_AUTH_FILE` launch
contract on native 0.146 without replacing its config-layer stack, profile-v2
model, auth manager, or storage backends. A selected file must be used
consistently for reads, writes, diagnostics, and credential identity while the
default no-environment behavior remains byte-for-byte compatible.

| Task | Status | Exit gate |
| --- | --- | --- |
| Freeze config-path precedence and resolution | Completed | `codex_config::resolve_user_config_path` owns the environment contract: an explicit `LoaderOverrides.user_config_path` wins, a nonblank environment path resolves against `CODEX_HOME` unless absolute, and blank/unset falls back to `CODEX_HOME/config.toml`. Native base-plus-selected user-layer behavior is unchanged. |
| Route user-config consumers to the selected file | Completed | Active user-layer persistence and diagnostics plus home-only MCP/plugin/marketplace/config helpers use the selected file. Project `.codex/config.toml`, managed config, and system config remain unaffected; profile-file overrides disable incompatible implicit daemon reuse. |
| Freeze auth-file and credential-identity semantics | Completed | Public login helpers resolve `CODEX_AUTH_FILE` once per storage instance. File load/save/delete uses that path; default direct keyring, encrypted-secret, and ephemeral identities remain native, while override-backed identities are stable and path-distinct. |
| Add focused unit and subprocess regressions | Completed | Resolver, loader precedence, MCP/login subprocess, all auth backend, selected write/reload, project-config preservation, and daemon-selection regressions pass. |
| Validate and checkpoint P02 | Completed | Product checkpoints are `df92174eef` (config routing) and `941b821e23` (auth/daemon isolation). Final formatting and scoped Clippy passed, and generated lockfile version noise was reversed. |

Validation (all test commands cleared inherited `CODEX_CONFIG_FILE` and
`CODEX_AUTH_FILE`):

- `just test -p codex-config --retries 0`: 223/223 passed.
- `just test -p codex-login --retries 0`: 161/161 passed.
- `just test -p codex-core-plugins --retries 0`: 352/352 passed.
- `just test -p codex-cli --retries 0`: 318/318 passed; the named selected MCP
  config and selected login auth-file subprocess regressions passed 2/2.
- `just test -p codex-core config::edit --retries 0`: 44/44 passed;
  `reload_user_config_layer`: 5/5 passed. The selected user-config writer,
  project MCP config preservation, and app-server selected-path writer passed
  1/1 each.
- `just test -p codex-tui can_reuse_implicit_local_daemon_requires_default_launch_config --retries 0`:
  1/1 passed; `just test -p codex-cli sandbox --retries 0`: 17/17 passed.
- `just fmt` and scoped `just fix -p codex-config -p codex-login -p codex-core
  -p codex-core-plugins -p codex-cli -p codex-tui -p codex-app-server` passed.
  Tests were not rerun afterward per repository instructions.

P02 does not change Cutex launch code, profile names, auth payload schemas,
credential material, live configuration, or any deployed artifact. Roll back
the product slice by reverting `941b821e23` and then `df92174eef`; never reset
or rewrite the shared branch.

## Current P03 Plan: SOCKS And Forced HTTP Transport

Goal: preserve the deployed SOCKS-capable HTTP launch contract and
`CUTE_CODEX_FORCE_HTTP_TRANSPORT` compatibility on the native 0.146 transport
stack. Reuse the shared route-aware HTTP client and current Responses transport
predicate; do not restore old per-client construction or add SOCKS support to
the separate WebSocket dialer.

| Task | Status | Exit gate |
| --- | --- | --- |
| Reconcile immutable patch `05` with 0.146 transport ownership | Completed | The workspace reqwest 0.12 feature reaches shared/raw HTTP clients; backend and LM Studio now use `codex-http-client`, so their old direct dependency edits retire. MCP still owns a direct reqwest 0.13 transport and needs the feature independently. |
| Enable SOCKS for retained reqwest generations | Completed | `socks` is enabled only for workspace reqwest 0.12 and MCP's direct reqwest 0.13. The required dependencies were already present in the resolved graph, so reviewed `Cargo.lock` remains unchanged. |
| Port the forced-HTTP launch switch | Completed | One pure truthy-value parser backs `CUTE_CODEX_FORCE_HTTP_TRANSPORT`; Responses avoids WebSocket without latching fallback, realtime returns an explicit invalid-request error, and TUI/archive do not reuse an implicit daemon that missed the process-local override. |
| Add focused transport regressions | Completed | A local SOCKS5h handshake proves proxy-side DNS; three pure core tests cover parsing, Responses selection, and realtime rejection; final scoped Clippy compiles the TUI daemon-selection bridge. |
| Validate and checkpoint P03 | Completed | Focused transport tests and HTTP/MCP crate gates pass; the core gate's unrelated environment failures are recorded below; final formatting and scoped fix pass. Product checkpoint: `ce8fd23c1a`. |

Validation:

- local SOCKS5h route-aware HTTP integration test: 1/1 passed and captured the
  unresolved destination hostname at the proxy
- focused forced-HTTP parser, Responses predicate, and realtime rejection
  tests: 3/3 passed
- `codex-http-client`: 68/68 passed
- `codex-rmcp-client`: 115/115 passed, with five tests skipped
- full `codex-core` attempt reached 3,135/3,142 passes with nine skips. One
  inherited `CUTEX_AGENT_BUS_*` prompt-caching failure and one image rollback
  timeout passed targeted reruns. Five permission tests repeatedly failed on
  this host because stream-fd creation returned `Operation not permitted`; the
  unchanged mechanism was not retried again.
- two pre-fix focused TUI compile attempts exposed the missing public bridge;
  after the bounded `codex-app-server-client` re-export, final scoped Clippy
  compiled `codex-tui` successfully instead of repeating that mechanism
- `just fmt` and scoped `just fix -p codex-http-client -p codex-core -p
  codex-rmcp-client -p codex-app-server-client -p codex-tui` passed; tests were
  not rerun afterward per repository instructions
- exact `Cargo.lock` review found only workspace-version noise from tooling;
  it was reversed and the committed dependency graph is unchanged

P03 does not change proxy environment values, provider schemas, Cutex launch
code, live network state, or the native WebSocket proxy implementation. It
adds build-time capability plus launch-time selection only. Roll back by
reverting the single P03 product checkpoint `ce8fd23c1a`.

## Current P04 Plan: External Status-Line Catalog

Goal: preserve the deployed launcher profile/runtime items and external JSON
catalog while extending the native 0.146 status-line framework. Native config
storage, theme colors, rate-limit labels, PR hyperlinks, placeholder/live
preview data, async refresh, and invalid-item warnings remain authoritative;
do not restore a parallel status surface.

| Task | Status | Exit gate |
| --- | --- | --- |
| Reconcile immutable patch `03` and deployed v1444 with native ownership | Completed | The archive adds `launch-profile`, `launch-runtime`, and catalog-backed string IDs. Native 0.146 already owns every other built-in, ordered string config, setup persistence, styled segments, live preview, and refresh scheduling. |
| Isolate catalog loading and value resolution | Completed | `CODEX_CUSTOM_STATUS_ITEMS_FILE` preserves static, arbitrary environment, catalog-relative UTF-8 file, five launch metadata, and current-directory sources; value/label/template rendering, named/RGB styles, normalization, empty/duplicate filtering, read/parse failure handling, and non-ASCII RGB safety are covered. Built-in IDs retain precedence. |
| Add the two missing native launch items | Completed | `launch-profile` and `launch-runtime` join the native enum, metadata accent, preview data, and value resolver. Nonblank environment values render as `Profile <value>` and `Runtime <value>`; unset/blank values are omitted. |
| Extend native selection, setup, and rendering for catalog IDs | Completed | Known custom IDs survive config parsing/persistence, appear in `/statusline`, preview and render in configured order with explicit styles, and retain native theme-toggle preview, dynamic rate-limit copy, PR hyperlink, async dependency, alias/deduplication, and invalid-ID behavior. |
| Add focused regressions | Completed | Six catalog-module tests and six TUI integration/setup/alias tests cover all source/render/style classes, launch values, custom live/setup rendering, built-in precedence, theme toggling, canonical aliases, and invalid/empty input without environment leakage. |
| Validate and checkpoint P04 | Completed | Catalog tests passed 6/6; the relevant status-line suite passed 72/72 after excluding two pre-existing project-root snapshot drifts; two intended snapshots passed 2/2 with no pending snapshots; final formatting and scoped Clippy passed. Product checkpoint: `f3b2e42704`. |

Validation:

- `custom_status_items`: 6/6 passed, including explicit temp catalog paths,
  all source/render/style classes, invalid/non-ASCII colors, and serialized
  environment mutation only where the public launch contract needs it
- six focused TUI regressions passed: launch values including blank/unset,
  custom live/setup rendering, built-in catalog precedence, canonical alias
  deduplication, mixed styled preview/theme toggle, and string-ID confirmation
- relevant `codex-tui` status-line/setup/style filters: 72/72 passed. Two
  existing project-root snapshots still resolve a test temp root as `tmp`
  instead of `my-project`; they were excluded and not modified. The two
  P04-intended picker snapshots passed 2/2 after review.
- `cargo insta pending-snapshots --manifest-path tui/Cargo.toml`: no pending
  snapshots; unrelated generated updates were rejected
- `just fmt` and scoped `just fix -p codex-tui` passed with no diagnostics;
  tests were not rerun afterward per repository instructions
- non-snapshot `git diff --check` passed. The one ordinary check exception is
  the fixed-width trailing space required by the changed setup snapshot.
- exact `Cargo.lock` review found only workspace-version noise from tooling;
  it was reversed and the committed dependency graph is unchanged. The full
  TUI crate gate remains deferred to the final candidate gate.

P04 does not add a config schema, catalog watcher, shell/command source,
terminal-title customization, Cutex launch mutation, live file mutation,
deployment, or restart. The catalog is read once when a `ChatWidget` is
constructed, matching the deployed launch-time contract. Roll back by reverting
the single P04 product checkpoint `f3b2e42704`.

## Current P05 Plan: Lifecycle Notification HTTP Compatibility

Goal: preserve the deployed v1444 HTTP notification service on native 0.146
without duplicating native desktop notifications, hooks, app-server lifecycle
ownership, or the retired Cutex sent/received event/card families. The external
POST schema, event names, allowlist, timing rules, and payload metadata remain
the compatibility contract; current 0.146 `ChatWidget` modules and
`ThreadSessionState`/`ThreadItem` sources provide the lifecycle signals.

| Task | Status | Exit gate |
| --- | --- | --- |
| Reconcile the immutable v1444 service with native 0.146 ownership | Completed | Archive `NotifyServiceEvent`/settings, payload fields, environment overrides, idle/approval/startup timing, and all optional lifecycle event sources are classified; native terminal notifications and `notify` hook remain separate. |
| Port the configuration and HTTP payload boundary | Completed | Flattened settings, tolerant environment overrides, JSON/Bearer POST, and bounded shutdown delivery are implemented and covered by config plus HTTP tests. |
| Add generation-guarded idle and startup-idle scheduling | Completed | Idle, composer, approval, startup-idle, editor, submission, thread replacement, and exit paths use generation-guarded app-event timers. |
| Wire current lifecycle sources and preserve event detail semantics | Completed | Allowlisted session/user-message/turn/thread/context/item/hook/rate-limit/approval events are wired; queue deduplication, replay suppression, and native notification separation are preserved. |
| Add focused regressions and checkpoint | Completed | Focused tests, formatting, and scoped Clippy pass in product checkpoint `cd72bb822d90`; no artifact or live state changed. |

Validation:

- `just test -p codex-config tui_flattens_notify_service_settings --retries 0`: 1 passed.
- `just test -p codex-core notify_service_env_overrides_apply_after_config_and_ignore_bad_values --retries 0`: 1 passed.
- `just test -p codex-tui idle_notify --retries 0`: 14 passed; `composer_submission`: 40 passed; `history_replay`: 32 passed.
- `cargo check --manifest-path codex-rs/Cargo.toml -p codex-config -p codex-core -p codex-tui --target-dir codex-rs/target-host`: passed.
- `just fmt`: passed. `just fix -p codex-config -p codex-core -p codex-tui`: passed with no Clippy diagnostics after the value-parameter cleanup. Tests were not rerun after the final fix per repository instructions.
- Product checkpoint: `cd72bb822d90`; the worktree was clean after commit.
- full workspace gate and release build remain deferred to Stage 4

The P05 implementation may add only the notification settings/payload module,
its current TUI integration points, and focused tests/docs. It must not change
Cutex/Waveline/backend/Android code, app-server protocol methods, production
configuration, live services, or deployment artifacts. Roll back by reverting
the P05 product checkpoint and its preceding P05 documentation checkpoint.

## Current P06 Plan: Terminal Frame Synchronization And CuteCharm Sideband

Goal: carry the immutable v1444 terminal-host contract onto the current 0.146
terminal writer without changing default terminal output. Synchronized-update
markers and cursor operations use the same backend writer as frame bytes; the
opt-in `CUTE_CODEX_TERMINAL_PROTOCOL=osc777` sideband publishes a monotonic
composer-state frame for CuteCharm consumers.

| Task | Status | Exit gate |
| --- | --- | --- |
| Audit current 0.146 terminal ownership and v1444 evidence | Completed | Current terminal writer, draw paths, composer layout, and immutable P06 evidence are mapped. |
| Add backend-owned synchronized updates and cursor ordering | Completed | Draw, resize-reflow, and pet-image markers/raw writes use the active backend; cursor position precedes visibility and error paths close the update. |
| Add opt-in OSC777 sideband model/emitter | Completed | Default-off schema/prefix/base64url payload, UTF-16 offsets, monotonic sequence, timestamp, and hidden/approval/running modes are covered. |
| Expose composer/viewport state and integrate render emission | Completed | Current layout supplies region/prompt/caret/IME/wrap state and one sideband frame follows each rendered main frame without changing default output. |
| Validate and checkpoint P06 | Completed | Focused sideband/composer/custom-terminal tests, formatting, zero-diagnostic scoped Clippy, diff hygiene, and product checkpoint `2f278f5407` pass. |

Validation:

- `just test -p codex-tui terminal_sideband --retries 0`: 6 passed; `custom_terminal`: 7 passed; exact composer and mode filters: 1 passed each.
- `cargo check --manifest-path codex-rs/Cargo.toml -p codex-tui --target-dir codex-rs/target-host`: passed before final formatting/Clippy.
- `just fmt`; final `just fix -p codex-tui`: passed with no Clippy diagnostics. Tests were not rerun after the final fix.
- The inherited-runtime full TUI attempt was not acceptance evidence: 3,233 passed, 57 failed, one IDE IPC test timed out, and four were skipped. Representative status and resume failures passed after clearing inherited production overrides; the already-recorded IDE IPC timeout reproduced in isolation.
- Test-created `.snap.new` files were removed and workspace-version-only `Cargo.lock` noise was reversed exactly.
- P07/P08 and final workspace/artifact gates remain deferred

P06 is limited to current TUI terminal/composer modules and tests/docs; no
Cutex/Waveline/backend/Android, production, deployment, or Windows changes.

## Current P07 Plan: Session Picker Provider Scope

Goal: retain the v1444 all-provider resume/fork behavior through the native
0.146 `ProviderFilter` and `thread/list` contract. The default remains the
current provider for local latest/picker lookups; `all`, remote workspaces, and
name-based lookups send an explicit empty `modelProviders` array, which is the
native all-provider value. P02's selected config-file model remains canonical;
legacy inline `[profiles.*]` selection is not restored.

| Task | Status | Exit gate |
| --- | --- | --- |
| Audit immutable P07 and native provider semantics | Completed | `None` is implementation default, empty is all providers, remote picker already intends `ProviderFilter::Any`, and current local lookup hard-codes the active provider. |
| Add typed config and generated schema | Completed | `current|all` defaults to `current`, resolves from selected root `[tui]`, remains representable in `ProfileTui`, and the generated schema fixture matches byte-for-byte. |
| Route native TUI and exec lookup paths | Completed | Local latest/picker obey the setting; name lookup, remote workspace, and configured `all` request explicit all-provider semantics without changing cwd/source filters. |
| Add focused config/lookup regressions | Completed | Core config (3), exec (1), TUI latest (6), and picker (1) focused tests pass; named/remote paths are asserted through their request helpers. |
| Validate and checkpoint P07 | Completed | `cargo check` for core/exec/TUI/sample, `just fmt`, and zero-diagnostic Clippy pass; Cargo.lock workspace-version noise and snapshots are clean; product checkpoint is `320ce2ce43`. |

Validation:

- `just write-config-schema` and the config schema fixture test
- focused core config, exec resume, TUI latest-lookup, and picker-provider tests
- touched-crate `cargo check`, then `just fmt` and scoped Clippy/fix; tests are not rerun after final fix
- P08 and final workspace/artifact gates remain deferred

P07 is limited to config/core facade/sample construction plus exec/TUI lookup
callers and the generated config schema. It does not change app-server request
types, thread-store semantics, Cutex/Waveline, live configuration, deployment,
or Windows artifacts.

## Current P08 Plan: Transitional Runtime Heartbeat

Goal: retain the v1444 foreground/runtime heartbeat contract at the current
TUI session boundary. Heartbeats are opt-in through the launch-provided URL,
carry the real `cute-codex session` id plus volatile launch/host metadata, and
never become durable frontend/backend identity or Windows lifecycle ownership.

| Task | Status | Exit gate |
| --- | --- | --- |
| Audit immutable heartbeat and native TUI boundaries | Completed | The v1444 payload/env contract, 30-second cadence, bearer token, and session/name update hooks are mapped onto current `SessionConfigured` flow; no Windows mutation is required. |
| Add opt-in snapshot/payload delivery | Completed | Empty/unconfigured URL is a no-op; configured URL posts camelCase JSON with launch id, session id, pid, cwd, profile, host id, runtime agent id, source, and optional bearer auth; latest snapshot drives periodic heartbeats. |
| Wire session lifecycle updates | Completed | Session configuration and current-thread name changes update the snapshot without blocking TUI startup or changing native session identity. |
| Add focused contract regressions | Completed | Payload serialization/omission and env trimming pass; a disposable localhost HTTP receipt proves bearer/body delivery without touching production services. |
| Validate and checkpoint P08 | Completed | TUI focused tests pass 3/3, `cargo check -p codex-tui`, `just fmt`, and zero-diagnostic Clippy pass; product checkpoint is `0426a02fc9`. |

Validation:

- focused heartbeat unit/HTTP smoke tests: 3/3
- `cargo check -p codex-tui`; `just fmt`; scoped Clippy/fix (no test rerun after final fix)
- no Windows build, live service restart, deployment, or device acceptance

P08 is limited to the TUI heartbeat module, its session-flow hooks, focused
tests, and documentation. It does not change Cutex management APIs, Agent Bus
routing, backend/frontend services, production configuration, or Windows code.

## Stage 4/5 Closeout

### Source And Focused Gates

- Stable base: `rust-v0.146.0` peeled commit
  `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`; corrected product source
  checkpoint before this documentation closeout:
  `7de700f7c30c1e59364957996e8c4db624e3a159` (tree
  `cb02ceb9ce3a2e46c55657745967f6eb3d1d8ecb`).
- `just fmt` passed; generated config and app-server schema fixtures matched
  their native generators byte-for-byte. The final touched-crate checks and
  scoped `just fix`/Clippy runs completed with no diagnostics; tests were not
  rerun after the final fix, per repository guidance.
- Focused protocol/core/app-server/TUI/exec gates recorded in the stage slices
  pass, including the `after_turn` idle/active/completion, batching, mode,
  deduplication, interrupt, and phantom-turn regressions.
- The post-closeout managed-resume compatibility regression passed 1/1 with
  the exact Cutex argv shape. It verifies the session id, root `--cd`, UDS
  remote, and hidden `--cwd-policy current` mapping to the final native
  `tui.resume_cwd="current"` override. The complete `codex-cli` crate gate
  passed 318/318; `just fmt`, `git diff --check`, and all-target/all-feature
  `codex-cli` Clippy with `-D warnings` passed.

### Full Workspace Gate

One clean-environment final attempt was run with the repository `just test`
recipe (`RUST_MIN_STACK=8388608 NEXTEST_PROFILE=local cargo nextest run
--no-fail-fast`). Receipt: `13,225` tests run, `13,179` passed, `45` failed,
`1` timed out, and `23` skipped. This is not a full-gate pass. The first
concrete blockers were host/environment constraints: IDE IPC temporary socket
directories rejected as writable by other users (with one 60-second timeout),
sandbox stream tests returning `Operation not permitted`, and `/tmp`-shared
test state affecting project/config/realtime-context assertions. A separate
set of release-tag snapshot failures reflects expected `0.146.0` output versus
checked-in `0.0.0` workspace-version fixtures. No generated snapshots were
accepted and all `.snap.new` files were removed; the workspace-version-only
`Cargo.lock` rewrite was restored.

### Candidate Artifact

The corrected release was built from a clean archive of product checkpoint
`7de700f7c3` using `cargo build --release --bin cute-codex`. Neither upstream
`codex` nor `cutex` was built. The corrected candidate is copied at
`../artifacts/cute-codex-v146-linux-candidate-7de700f7/cute-codex`:

- version: `cute-codex 0.146.0`
- size: `1,434,161,728` bytes
- SHA-256: `23908e5caa903b38d0cb79aec0cfb4039aa3c30435177eec9c077dc95371d97d`
- binary source checkpoint: `7de700f7c30c1e59364957996e8c4db624e3a159`
- source tree: `cb02ceb9ce3a2e46c55657745967f6eb3d1d8ecb`
- source archive: `cute-codex-v146-7de700f7-source.tar.gz`
- source archive size: `10,383,624` bytes
- source archive SHA-256:
  `91a1eb7343a4c049fe549e6d572833d8bd4eac2b5acd6f5e78a5fb40784be305`

The earlier `cb30843615` candidate is preserved as evidence but is superseded:
it rejects the deployed Cutex `resume --cwd-policy current` argv. The corrected
artifact's `BUILD_INFO.txt` and `SHA256SUMS` validate both files, and the target
and copied artifact are byte-identical.

The candidate is built, hashed, and not deployed. The exact rollback remains
the staged v1444 binary
`/home/example/Resources/Shortcuts/cute-codex` (SHA-256
`9acb2d40be0bf4486ac94706bc55f53943508fdda6c28440568355c89536ecbe`) and its
immutable source archive
`cute-codex-0.144.1-cheers-inter-agent-notifications-v1444-260714-src.tgz`.

### Disposable Consumer Smoke

The legacy Cutex adapter smoke was run against the candidate in an isolated
Unix-socket process. Registration, native submission, and one-shot ACK worked,
but that old harness then failed while waiting for the retired
`thread/interAgentMessage/received` notification. This is a bounded consumer
contract difference, not a candidate runtime failure. The Cutex owner must
replace that assertion with the canonical `item/started`/`item/completed`
notifications for an `interAgentMessage` item correlated by `messageId`; the
`submissionId` receipt remains acceptance only. This session did not edit
`../../cutex`.

An independent candidate-compatible probe used a temporary mock Responses
server and temporary `CODEX_HOME` with the candidate's stdio app-server. It
passed with this exact receipt:

```text
modelRequests=1, turnStarted=1, turnCompleted=1,
interAgentItemStarted=1, interAgentItemCompleted=1,
legacyNotifications=0, duplicate submissions=2,
event order=receipt, duplicate-receipt, turn/started,
  item/started(interAgentMessage), item/completed(interAgentMessage), turn/completed
```

The message content was present in the single model request. The probe and its
temporary state were removed after the run; no production Agent Bus, service,
session, or configuration was touched.

### Bounded Integration Handoff

Cutex consumers should retain the additive request
`thread/inter_agent_message` and its fields (`messageId`, AgentPath identities,
content, `deliveryMode`, optional `otherRecipients`). They must distinguish
three states: request submission accepted (`submissionId`), model turn started
(`turn/started`), and message consumed (`item/started` plus `item/completed`).
The old `thread/interAgentMessage/sent` and `/received` event family is retired;
no Cutex-side source change is made here. Durable deduplication remains owned by
Cutex, while cute-codex owns mailbox acceptance, native item persistence, and
turn scheduling.

For lifecycle launch compatibility, the deployed Cutex argv may continue to
use `resume --cwd-policy current <SESSION_ID> --remote unix://...`. The hidden
compatibility flag accepts only `current` and maps it to native 0.146
`tui.resume_cwd="current"` with highest CLI override precedence; Cutex does not
need an argv change for this rollout.

This closes the cute-codex source/artifact handoff only. Deployment, restart,
Windows builds, device acceptance, and Cutex consumer adaptation require a
separate owner decision.

## Rollback And Stop Conditions

No live runtime is changed, so Stage 0 rollback is simply to leave the isolated
branch/worktree unused. Do not delete or reset it without explicit owner
authorization.

Stop and report on any base-hash mismatch, unexpected target branch/worktree,
another writer, unclassifiable customization, cross-repository mutation need,
second failure of the same mechanism without new evidence, owner pause, stale
live-state dependency, or context degradation.

## Changelog

- 2026-08-07: Completed required reading and baseline provenance checks. Fetched
  only `rust-v0.146.0`, created the clean isolated worktree, and recorded the
  Stage 0 plan. Validation: remote and local tag hashes, worktree HEAD/status,
  binary SHA-256, and source-archive SHA-256 all matched. Follow-up: verify the
  clean 0.144.1 tag and build the immutable diff inventory.
- 2026-08-07: Verified official 0.144.1, extracted read-only comparison trees,
  reconciled every changed path, and classified all deployed customizations.
  Native 0.146 already contains the core active-to-idle pending-work retry, but
  the exact external delivery regressions remain required. Follow-up: record a
  committed Stage 0 audit checkpoint and begin the v2 contract audit.
- 2026-08-07: Passed the native wake baselines: three
  `queued_inter_agent_mail` tests and the answer-boundary
  `trigger_turn_mailbox_mail` test. Restored the incidental release-version
  lockfile rewrite and closed Stage 0 with documentation-only snapshots on the
  isolated branch. Follow-up: freeze the exact v2 request/item and delivery-mode
  contract before editing product source.
- 2026-08-07: Completed the 0.146 contract audit. Froze the additive
  `thread/inter_agent_message` request, four delivery modes, bounded in-process
  deduplication, selective active-turn drain, canonical `InterAgentMessage`
  item lifecycle/history, interrupt sequencing, and regression matrix. The
  app-server history-projection baseline passed 5/5. Follow-up: implement the
  delivery slice before porting the remaining compatibility extensions.
- 2026-08-07: Started Stage 2 with the protocol/core delivery slice. Follow-up:
  implement only the frozen representation, queue, scheduling, item lifecycle,
  and focused regressions before app-server wiring.
- 2026-08-07: Implemented the delivery primitives and wake correction. The
  idle scheduler reservation now drains `after_turn`, pre-existing passive mail
  joins a regular turn's first request, and identified plaintext delivery emits
  one canonical persisted item. Focused protocol, queue, lifecycle, pending
  input, race, batching, mode, interrupt, and history tests passed. Follow-up:
  begin app-server v2 request wiring.
- 2026-08-07: Started the app-server v2 slice from clean commit `26f39881b4`.
  Re-audited current routing, loaded-thread submission, v2 payload rules, and
  the immutable v1444 request implementation. Implemented the frozen request,
  native submission, canonical lifecycle/history coverage, README contract,
  and stable schemas; focused tests passed. `just fmt` and scoped
  `just fix -p codex-app-server-protocol -p codex-app-server` completed, and
  the version-only lockfile rewrite was reversed exactly. Per repository
  instructions, tests did not rerun after format/fix. Follow-up: begin the
  canonical TUI slice.
- 2026-08-07: Started the canonical TUI slice from clean commit `34bd73127f`.
  Re-audited the shared live/replay handler, independent transcript and
  resume-preview projections, `/agent` status feed, TUI style rules, and the
  immutable v1444 inbound-card evidence. Follow-up: implement one bounded
  shared presenter and prove all reconstructed views use it without reviving
  legacy notifications or the outbound card.
- 2026-08-07: Completed the canonical TUI slice. One bounded presenter now
  drives the inbound history card and compact summaries; live completion,
  replay, full transcript, `/agent`, and resume previews share it. Five visual
  snapshots were reviewed and focused tests passed 6/6. The crate gate passed
  3,225 tests and exposed only the recorded release-version snapshots plus the
  unrelated IDE temporary-directory permission failures. `just fmt` and
  zero-warning `just fix -p codex-tui` passed; version-only lockfile noise was
  reversed. Follow-up: checkpoint this slice and begin the external Agent Bus
  tool port.
- 2026-08-07: Started the external Agent Bus tool slice from clean commit
  `067767629b`. Re-audited the 0.146 runtime planner and structured tool-output
  contract against only the immutable v1444 handler/spec evidence. The port
  will preserve the loopback authenticated HTTP and structured bus receipt,
  use native tool lifecycle visibility, and not restore the retired outbound
  event/card. Follow-up: implement the registered handlers and focused
  schema/HTTP/parser/receipt regressions.
- 2026-08-07: Completed the external Agent Bus tool slice. The two environment-
  gated tools now use the current registry and `JsonToolOutput`, preserve the
  structured Cutex receipt, and keep loopback-only authenticated HTTP while
  disabling proxy and redirect escape. No outbound compatibility event/card
  was restored. Focused tests passed 19/19 and the full tool-planning module
  passed 34/34 with the bus environment configured; final formatting and
  zero-warning scoped fix passed, and lockfile noise was reversed. Follow-up:
  begin the remaining launch/network/runtime compatibility slices.
- 2026-08-07: Started P01 from clean commit `5ac4873134`. Reconciled the old
  binary/branding/build-metadata evidence against the current 0.146 CLI/TUI
  entrypoints. The historical `ResumeCwdPolicy` and config-path helper are
  explicitly outside this slice because 0.146 now owns the corresponding
  resume behavior and P02 owns launch-file overrides. Follow-up: add the
  parallel binary target, brand the retained user-facing surfaces, and test
  the optional build tag.
- 2026-08-07: Completed P01. Added the parallel `cute-codex` launcher while
  retaining `codex`, branded CLI guidance/resume output and native TUI
  identity, and added a trim-safe optional build tag without changing the
  official semver. The CLI crate passed 316/316 tests; focused TUI filters
  passed 171 cases in aggregate; the alias binary built and reported
  `cute-codex 0.146.0`; final formatting and scoped fixes passed. Follow-up:
  checkpoint P01 and begin P02 profile config/auth-file compatibility.
- 2026-08-07: Started P02 from clean commit `eebef76f92`. Reconciled immutable
  v1444 path overrides with native 0.146 config layering and auth backends. The
  port will centralize config selection, preserve explicit-loader precedence
  and native base layering, resolve relative launch paths against `CODEX_HOME`,
  keep default credential identities unchanged, and namespace every supported
  override-backed auth store. Follow-up: implement config-path routing and its
  focused regressions before changing auth storage.
- 2026-08-07: Completed P02 in ordered checkpoints `df92174eef` and
  `941b821e23`. Selected config now controls native layers, writes, diagnostics,
  reload fallback, and relevant plugin/MCP paths; selected auth isolates file,
  direct-keyring, encrypted-secret, and ephemeral storage while preserving all
  default identities. Touched crate gates passed 223/223, 161/161, 352/352,
  and 318/318; focused core/app-server/TUI/CLI regressions and final formatting
  plus scoped Clippy passed. No artifact or live state changed. Follow-up: begin
  P03 transport compatibility from the clean documentation checkpoint.
- 2026-08-07: Started P03 from clean `f7735a6132`. Immutable patch `05` maps
  onto the shared reqwest 0.12 dependency plus MCP's direct reqwest 0.13
  dependency; old backend/LM Studio manifest edits are obsolete. The forced
  HTTP switch will feed the existing Responses predicate and reject realtime,
  prevent incompatible implicit-daemon reuse, and use pure selection tests plus
  one real local SOCKS5h handshake. Follow-up: checkpoint this plan, then
  implement the bounded transport slice.
- 2026-08-07: Completed P03 in product checkpoint `ce8fd23c1a`. Enabled SOCKS
  only for retained reqwest 0.12/0.13 owners, added a real proxy-side-DNS
  SOCKS5h handshake, and routed the process-local forced-HTTP switch through
  Responses, realtime rejection, and TUI/archive daemon selection. Focused
  regressions passed 1/1 and 3/3; HTTP passed 68/68; MCP passed 115/115 with
  five skips; final formatting and scoped Clippy passed. The attempted full
  core gate and its five repeated host permission failures are recorded in the
  P03 validation section. No artifact or live state changed. Follow-up: write
  the just-in-time P04 native status-line plan before implementation.
- 2026-08-07: Started P04 from clean `253f2f0e54`. Immutable patch `03` and the
  deployed v1444 archive show two launch-value built-ins plus a launcher-owned
  JSON catalog; native 0.146 already owns the surrounding status-line contract.
  The port will extend native string-ID selection and styled rendering while
  preserving theme-toggle previews, rate-limit copy, PR hyperlinks, async
  refresh, persistence, and invalid-item warnings. Follow-up: checkpoint this
  plan, then implement the catalog domain before UI wiring.
- 2026-08-07: Completed P04 in product checkpoint `f3b2e42704`. Added the
  launcher-owned catalog domain and integrated it with native 0.146 status-line
  enum, preview, theme styling, persistence, async dependencies, hyperlinks,
  alias handling, and invalid-item warnings; added `launch-profile` and
  `launch-runtime`. Catalog tests passed 6/6, the relevant status-line suite
  passed 72/72 after explicitly excluding two pre-existing temp-root snapshot
  drifts, intended snapshots passed 2/2, and final formatting plus scoped
  Clippy passed. No artifact or live state changed. Follow-up: write the
  just-in-time P05 lifecycle-notification plan before implementation.
- 2026-08-08: Started P05 after comparing immutable v1444 notification sources
  with native 0.146. Native terminal notifications, `notify` hooks, and
  app-server lifecycle events cover presentation and observation only; the
  deployed HTTP schema, Bearer endpoint, event allowlist, composer/approval/
  startup idle timers, and rich lifecycle metadata remain compatibility gaps.
  P05 is classified as replace and will use current modular TUI ownership with
  generation-guarded app-event timers. Follow-up: commit this plan, then port
  configuration/payload primitives before lifecycle wiring.
- 2026-08-08: Completed P06 from plan checkpoint `0c4fc00e7d`. Synchronized
  updates, cursor ordering, and raw protocol writes now share the terminal
  backend; an opt-in OSC777 frame publishes exact composer, viewport, UTF-16,
  caret, IME, wrap, and mode state. Focused tests passed 6/6 sideband and 7/7
  custom-terminal, final formatting and scoped Clippy passed, and generated
  lock/snapshot noise was removed. Product checkpoint: `2f278f5407`.
  Follow-up: write the just-in-time P07 plan.
- 2026-08-08: Completed P07 in product checkpoint `320ce2ce43`. Added the typed
  `session_picker_provider_filter` setting and generated schema, routed native
  resume/latest/picker lookups to explicit current-provider or all-provider
  `modelProviders` values, and repaired the sample facade construction. Core
  config tests passed 3/3, exec 1/1, TUI latest 6/6, and picker 1/1; schema
  regeneration matched, touched-crate check passed, formatting and
  zero-diagnostic Clippy passed, and lock/snapshot noise was removed. No
  artifact, deployment, or live state changed. Follow-up: write the just-in-
  time P08 transitional heartbeat plan.
- 2026-08-08: Wrote the just-in-time P08 heartbeat plan after mapping the
  immutable opt-in URL/Bearer payload and session/name hooks onto current TUI
  ownership. Implementation remains limited to the heartbeat module and
  session-flow tests; Windows lifecycle, production services, and deployment
  stay out of scope.
- 2026-08-08: Completed P08 in product checkpoint `0426a02fc9`. Added the
  opt-in 30-second runtime heartbeat with trimmed launch/host/agent metadata,
  camelCase JSON, optional Bearer auth, and non-blocking session/name hooks.
  Payload, omission, env, and disposable localhost receipt tests passed 3/3;
  TUI check, formatting, and zero-diagnostic Clippy passed. No Windows build,
  artifact, deployment, or live state changed. Follow-up: final workspace and
  immutable Linux artifact gates.
- 2026-08-08: Completed the final Stage 4 candidate gate. The isolated Linux
  `cute-codex` candidate reports `0.146.0`, is 1,434,171,856 bytes, and has
  SHA-256 `18071b20db96c27ce03466ea036ccf98fd229ad48e02d8575b1dfa2b56cc14e3`.
  The exact source archive for product checkpoint `cb30843615` is 10,380,920
  bytes with SHA-256
  `1ed05eb467362caa09646bfe412d0b37138a4efd68a9e4270b98731776a2a423`.
  The one clean-environment workspace attempt recorded 13,179 passes, 45
  host/snapshot failures, one IDE timeout, and 23 skips; it is explicitly not
  called a full-gate pass.
- 2026-08-08: Candidate-compatible disposable app-server smoke passed with
  two submission receipts, one model request, one turn, one canonical
  inter-agent item lifecycle, and zero retired notifications. The older Cutex
  adapter smoke exposed the exact bounded consumer adaptation: consume the
  canonical item lifecycle instead of requiring `thread/interAgentMessage/*`.
  No Cutex source, production service, deployment, restart, Windows build, or
  device state changed. Stages 4 and 5 are closed for integration review.
- 2026-08-09: Restored the deployed Cutex managed-resume argv compatibility at
  product checkpoint `7de700f7c3` without reviving the retired cwd-policy
  implementation. The hidden `current` alias maps to native
  `tui.resume_cwd="current"`; the exact argv regression passed 1/1, the full
  `codex-cli` crate passed 318/318, and formatting, diff check, and scoped
  Clippy passed. A clean-archive Linux build produced only `cute-codex` at
  SHA-256 `23908e5caa903b38d0cb79aec0cfb4039aa3c30435177eec9c077dc95371d97d`;
  its source archive SHA-256 is
  `91a1eb7343a4c049fe549e6d572833d8bd4eac2b5acd6f5e78a5fb40784be305`.
  The prior full-workspace receipt was not rerun and remains explicitly not a
  full-gate pass. No deployment or live state changed.
- 2026-08-13: Completed the removable Stage 6 compaction-continuity repair at
  `b27d06efd0`. Official stable/prerelease/main comparisons and open upstream
  issues are frozen in the removal ledger. Prompt tests passed 36/36 and the
  compact core gate passed 167/167; the exact failure regression proved one
  terminal error plus preserved `after_turn` input. A clean source archive
  produced only `cute-codex`, SHA-256 `15d42e0616d0bbc414f2760e49b89093855300fe5f89f25da86944052f718dad`.
  No deployment or live state changed.
