# cute-codex 0.146 Compaction Continuity Patch

Status: local compatibility patch implemented at product checkpoint
`b27d06efd09c5bc593cd82bb9aeb62303def0941` on 2026-08-13. This document is
the removal ledger; the patch is not an upstream claim and does not authorize
deployment.

## Upstream investigation

| Reference | Exact revision checked | Result |
| --- | --- | --- |
| `rust-v0.146.1` | `79b4f03d35962b005b007a015113b38930711665` | Release notes contain no compaction-continuity fix. The pre-sampling and mid-turn error wrappers still return `Ok(None)`. |
| `rust-v0.147.0` | `be6e8eac029b183056b7e4402879f15d2c85f61b` | No equivalent fix. Open issue [#37394](https://github.com/openai/codex/issues/37394) reproduces a compacted task becoming a fresh bootstrap on this release. |
| `rust-v0.148.0-alpha.9` | `9392c3fa5bcda342b5b96a1a04d67b2f781617c2` | No equivalent fix in the latest prerelease checked. |
| upstream `main` | `b1373b74a27d1d9b65074a873202683355cae772` | The same wrapper and prompt semantics remain. |

Relevant upstream reports remain open:

- [#25619](https://github.com/openai/codex/issues/25619): an early return after
  compaction failure can look like successful `turn/completed` with no agent
  message;
- [#37394](https://github.com/openai/codex/issues/37394): on `0.147.0`, automatic
  compaction is followed by a rules acknowledgement and loss of the active
  task frontier;
- [#27352](https://github.com/openai/codex/issues/27352): near context limits,
  commentary-only output can be accepted as terminal and require manual
  continuation;
- [#32888](https://github.com/openai/codex/issues/32888): token accounting can
  use a stale provider-usage baseline around tool output and compaction.

No merged upstream PR linked to #25619 or #32888 was found. The release tags,
latest prerelease, and main source were compared directly rather than inferring
behavior from issue state.

## Local diagnosis

The local compact prompt asks for a handoff but does not say that compaction is
an internal continuation boundary. Its summary prefix likewise does not say
that the active user request is already accepted or that the next model sample
must resume the exact execution frontier. A successful compact can therefore
be interpreted as a new-session bootstrap, matching #37394.

Before sampling, `run_turn` compacts before recording the incoming turn input.
For non-abort failures it emits lifecycle telemetry and returns `Ok(None)`.
Some compact implementations emit an affecting `EventMsg::Error`, but early
wrapper failures need not, and the wrapper must not duplicate errors already
emitted below it. In this workspace, an idle `after_turn` delivery is drained
into the new task before `run_turn`; the same early return can therefore leave
accepted agent mail unrecorded.

## Local patch boundary

1. Strengthen the compact-generation prompt and summary prefix so the summary
   records active/completed/blocked state, exact execution frontier, and next
   unblocked action, and so the resumed model continues without asking for a
   repeated `continue` or re-acknowledging repository instructions.
2. On a non-abort compaction failure, ensure one affecting wire error exists.
   Reuse an inner error when present; synthesize only when none was recorded.
3. On a pre-sampling failure, record the already accepted input before the turn
   finishes. Do not automatically requeue it, which could create an unbounded
   compact/fail/wake loop.

This patch does not change compaction thresholds, history reducers, remote-v2
payloads, model terminal-response policy, or token-accounting baselines. In
particular, #32888 needs an atomic provider-usage baseline and corresponding
history boundary; taking a numeric maximum alone is not correct when a
successful model response omits usage.

## Implementation receipt

- `just test -p codex-prompts`: 36 passed, 0 failed;
- `just test -p codex-core compact`: 167 passed, 0 failed;
- focused failure-boundary regression: 1 passed; it observed one error, a
  failed `TurnComplete`, recorded `after_turn` mail, and no post-failure model
  request;
- local and remote pre-turn context-window failure regressions: 2 passed;
- `just fmt`, `just fix -p codex-core`, `just fix -p codex-prompts`, and
  `git diff --check`: passed;
- immutable Linux candidate:
  `artifacts/cute-codex-v146-linux-candidate-b27d06ef/cute-codex`, SHA-256
  `15d42e0616d0bbc414f2760e49b89093855300fe5f89f25da86944052f718dad`;
- source archive SHA-256:
  `3d6e3c0e6d9f0a75449308a6b286610fcf8419c8c2f4e4816817f07605d575f6`.

The full workspace gate was not repeated for this bounded patch. Its earlier
host/snapshot-constrained receipt remains explicitly incomplete. No live
binary, service, config, session, Cutex source, or Windows artifact changed.

## Removal gate

Remove the local patch only after an official stable release supplies
equivalent behavior and all of these checks pass with the local changes
reverted:

- the compact prompt/continuation contract preserves the already active task
  and exact next action without requiring another user message;
- pre-sampling and mid-turn compact failures produce exactly one affecting
  error before `TurnComplete`, so app-server status is failed rather than a
  silent completed/null result;
- accepted ordinary input and `after_turn` mail remain in conversation history
  after pre-sampling compact failure;
- the focused regressions named in the upgrade blueprint pass against the
  upstream implementation.

Record the replacing upstream tag, peeled commit, PR/issue, and test receipt in
this file before deleting the patch. A newer version number alone is not a
removal signal.
