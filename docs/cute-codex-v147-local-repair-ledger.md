# cute-codex 0.147 Local Repair Ledger

Date: 2026-08-13 AEST

Status: source repairs and focused regressions are implemented on the
`agent/cute-codex-v1-v147` merge candidate. This ledger records why each local
delta exists and the evidence required before it can be removed. It does not
authorize deployment.

## Exact Upstream Authority

- stable tag: `rust-v0.147.0`;
- tag object: `3ed6f04f6bf8b7c46299d1cb1ff99c74ce21a51d`;
- peeled commit: `be6e8eac029b183056b7e4402879f15d2c85f61b`;
- retained project parent before this upgrade:
  `365f698cf62f4ab29172d4337ea2cab0c88161bd`.

The exact final merge commit, tree, source archive, and candidate hashes are
added to the immutable artifact manifest after the clean build.

## R01: Compaction Continuity And Failure Visibility

Upstream 0.147 includes
[#36128](https://github.com/openai/codex/pull/36128), which retains bounded,
non-completion agent messages across remote-v2 compaction. That is useful but
narrower than this repair: it does not make a successful compact preserve the
current execution frontier, and it does not make an early compact failure
produce an affecting wire error rather than a completed turn with no agent
message.

The remaining symptom families are documented by:

- open [#37394](https://github.com/openai/codex/issues/37394): successful
  compaction can lose the active task and require another `continue`;
- open [#25619](https://github.com/openai/codex/issues/25619): an early
  `run_turn` return can become silent `turn/completed` with
  `last_agent_message=null`;
- closed [#22335](https://github.com/openai/codex/issues/22335): repeated remote
  compact failures can wedge a resumed long-lived thread;
- duplicate [#25900](https://github.com/openai/codex/issues/25900): successful
  compaction can restore the wrong semantic checkpoint and repeat work.

Local delta:

1. compact prompts require the active task, exact execution frontier, completed
   and rejected work, next unblocked action, and stop condition;
2. a non-abort compact failure has exactly one affecting error, reusing an
   inner error when one already exists;
3. already accepted pre-sampling input is persisted before a compact failure
   completes, without automatically requeueing it into a failure loop.

Focused evidence on 0.147:

- `codex-prompts`: 36 passed;
- compact-filtered `codex-core`: 177 passed;
- the silent pre-turn failure regression passed and observed one error, a
  failed completion, retained accepted mail, and no post-failure model request;
- local/remote context-window failure-shape regressions passed;
- remote-compaction parity passed again under low load after timing out only in
  the saturated full-core run.

Removal gate: revert R01 only after a stable upstream revision supplies all
three semantics and the same regressions pass without local prompt, turn-flow,
or test changes. Record the replacing tag, peeled commit, and upstream PR here.
A release number, issue closure, or delegated-message preservation alone is not
sufficient.

## R02: External `after_turn` Mail Scheduling

Upstream 0.147 adds durable user submissions and improves native collaboration,
but it does not provide the Cutex cross-host app-server submission method,
delivery metadata, external idempotency keys, or the four Cutex delivery modes.
The retained scheduler adaptation therefore keeps external `after_turn` mail
outside an active turn and reserves exactly one follow-up wake after normal
completion.

Focused evidence covers idle and active arrival, normal completion, the
completion/submission race, FIFO batching, deduplication, and distinct
`passive`, `soon`, interrupt, and `after_turn` behavior. The full core run also
passed these tests under load, including sleeping-root wake and commentary-item
follow-up coverage.

Removal gate: remove R02 only when stable upstream accepts the same external
identity and mode contract, persists the canonical item, and passes every local
pending-input regression with no duplicate, lost, empty, or repeating turn.
Submission acceptance, model-turn start, and consumption must remain separately
observable.

## R03: Managed Resume Argv Compatibility

Cutex currently emits:

```text
resume --cwd-policy current <SESSION_ID> --remote unix://...
```

Upstream 0.147 still has the native `tui.resume_cwd="current"` behavior but no
`--cwd-policy` parser compatibility. The hidden flag accepts only `current` and
maps it to the native override at the highest CLI precedence. A complete managed
argv regression covers model, sandbox, approval, config, `--cd`, session id,
and Unix-domain remote endpoint.

Removal gate: delete the flag only after the Cutex launcher no longer emits it
or upstream accepts the same argv. No runtime behavior should be reimplemented
to preserve the spelling.

## R04: Migration-Test Adaptations

The following changes affect tests and snapshots rather than deployed runtime
semantics:

- prompt-caching expectations include retained `cutex_agent_list` and
  `cutex_agent_send` tools;
- version/brand snapshots expect `cute-codex v0.147.0`;
- project-name tests cache an intentional missing root so an unrelated
  `/tmp/.git` cannot change their fallback result;
- the PTY focus harness waits for the actual `cute-codex` banner.

Remove these only with the corresponding feature/branding or upstream harness
change. They must never be used to hide a product assertion failure.

## Upstream Test/Generator Defects (No Product Patch)

1. `just write-app-server-schema` invokes a missing
   `write_schema_fixtures` binary in the 0.147 source. The authoritative Python
   generator was run for both stable and `--experimental` exports; both ignored
   generator checks then passed.
2. `codex-code-mode-host` cannot currently be built from the exact tag because
   `rusty_v8 150.4.0` requests a prebuilt Linux archive whose GitHub release URL
   returns HTTP 404. The unchanged build path was attempted once and was not
   retried. Code-mode-dependent tests are recorded as fixture-blocked.
3. This host cannot create the stream fd used by `systemd-cat`; permission tests
   that otherwise execute successfully receive that diagnostic on stdout. No
   production output normalization was added for this host restriction.

These are validation limitations, not evidence that a cute-codex runtime repair
is needed. Recheck them on the next upstream stable tag before carrying any
workaround forward.

## Rollback

- previous 0.146 candidate SHA-256:
  `15d42e0616d0bbc414f2760e49b89093855300fe5f89f25da86944052f718dad`;
- deployed 0.144.1 binary SHA-256:
  `9acb2d40be0bf4486ac94706bc55f53943508fdda6c28440568355c89536ecbe`.

No binary, live service, configuration, credential, session, Cutex source, or
Windows artifact is changed by this source checkpoint.
