# cute-codex 0.147 Upgrade Record

Date: 2026-08-13 AEST

Status: implementation, validation, formatting, and scoped Clippy are complete;
the merge checkpoint and immutable Linux candidate are pending. Artifact
creation is not deployment.

## Provenance And Boundary

- exact upstream: `rust-v0.147.0` peeled to
  `be6e8eac029b183056b7e4402879f15d2c85f61b`;
- branch/worktree: `agent/cute-codex-v1-v147`, `source-v147/`;
- retained 0.146 parent:
  `365f698cf62f4ab29172d4337ea2cab0c88161bd`;
- exact final commit/tree and artifact hashes: pending the clean checkpoint;
- out of scope: Cutex, Waveline, Windows, deployment, services, sessions,
  credentials, and runtime configuration.

## Migration Result

All prior P01-P20 patches were reclassified against native 0.147. Retained or
adapted behavior includes:

- `cute-codex` binary identity and branding;
- isolated config/auth paths, SOCKS support, and forced HTTP compatibility;
- external status catalog, lifecycle notification service, terminal sideband,
  all-provider resume picker, and Cutex heartbeat;
- localhost-only Cutex bus tools, app-server submission method, canonical
  persisted item, delivery modes, deduplication, and native TUI projections;
- normal-completion `after_turn` wake and compaction-continuity repairs;
- hidden managed-resume `--cwd-policy current` compatibility.

Native 0.147 parent-turn provenance, durable submission queue, paginated
history, remote-compaction delegated-message preservation, and current
app-server/core/TUI contracts remain authoritative. Obsolete named lookup and
retired notification families were not revived.

## Validation Receipt

Focused/touched suites passed:

- protocol/app-server inter-agent: 4 + 3;
- core input queue/inter-agent/pending input: 10 + 3 + 21;
- compaction: prompts 36, core filter 177, exact silent-failure regression 1;
- Cutex tools/spec planning: 19 + 48;
- delivery mode/thread history/resume CLI: 1 + 6 + 7;
- config/login/plugins/http/rmcp: 237 + 172 + 385 + 74 + 211, with 5 rmcp
  cases skipped;
- app-server protocol: 289 accounted, with one ignored generator test;
- CLI: 344; exec: 134; forced HTTP: 2;
- generated app-server stable/experimental exports and config schema checks.

Final source hygiene passed: `just fmt`, one scoped
`cargo clippy --fix --tests --all-targets --all-features` invocation covering
all changed crates, and `git diff --check` on every path except one fixed-width
TUI snapshot line whose two trailing spaces are intentional 72-column terminal
padding. The excluded-path check passed, and the snapshot had already passed
its focused regression. Per repository instructions, tests were not rerun after
Clippy. Cargo's 135 workspace-package version rewrites were restored to the
upstream `0.0.0`; the lockfile retains only the three real dependency edges for
`gethostname` and `reqwest`.

Feasible full gates:

- app-server: 1,113 passed, 9 failed, 1 skipped. Six failures are the host
  stream-fd restriction; three require the unavailable code-mode host.
- TUI: 3,434 passed, 24 failed, 1 timeout, 4 skipped on the first full run.
  Every nonpass was reproduced and accounted for: intended 0.147 branding
  snapshots, shared `/tmp/.git` project-name pollution, secure-temp umask, and
  a PTY harness waiting for the upstream product name. All affected focused
  reruns passed after test/snapshot adaptations.
- core: 3,233 passed, 92 failed, 7 timed out, 8 skipped in one highly concurrent
  3,332-test run. The dominant failure group requires the unavailable
  `codex-code-mode-host`. Low-load reruns passed the retained-tool expectation,
  all five approval matrices, CLI PAT 401, remote compact parity, cold root
  resume, six remote-environment cases, and six pushed-process-event cases.
  Five permission-output assertions remain host-blocked by `systemd-cat`; the
  command/file effects succeeded. Responses-lite/WebSocket metadata failures
  are the documented Direct fallback when the code-mode host is absent.

The exact missing fixture attempt was:

```text
cargo build -p codex-code-mode-host --bin codex-code-mode-host
```

It failed because the `rusty_v8 150.4.0` prebuilt Linux URL returned HTTP 404.
The same mechanism was not retried. The release candidate builds only
`--bin cute-codex` and does not require this test fixture.

## Build Hygiene

All validation used the existing shared target outside `source-v147/`. An
accidental partial 2.2 GiB target was interrupted and moved, not deleted, to:

```text
/mnt/disk02/development-histories/cute-codex/cute-codex-v1-v147-partial-target-20260813
```

Tests used a mode-0700 temporary root because the host umask otherwise creates
directories rejected by IDE-socket security checks. The immutable build will
be made from an exact committed source archive and will select only
`--bin cute-codex`.

## Removal And Rollback

The detailed official evidence, local deltas, regressions, and removal gates
are in `docs/cute-codex-v147-local-repair-ledger.md`. The prior 0.146 candidate
SHA-256 is
`15d42e0616d0bbc414f2760e49b89093855300fe5f89f25da86944052f718dad`;
the deployed 0.144.1 rollback SHA-256 is
`9acb2d40be0bf4486ac94706bc55f53943508fdda6c28440568355c89536ecbe`.
