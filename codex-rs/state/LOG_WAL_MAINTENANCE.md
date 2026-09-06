# Bounded Logs WAL Maintenance

Scope: `codex-rs/state` at base `101b893af3fe9ec09776dcae396c38b1074644f4`;
Cutex, retention, vacuuming, deployment, and process lifecycle are out of scope.

## Root cause

SQLite auto-checkpoints are `PASSIVE`. An old reader snapshot can stop a
checkpoint before its end mark and prevent reset. With overlapping readers,
writers keep appending, so the WAL can grow without bound despite small retained
data.

The 0.153.2 startup path runs once, discards the three-column `PASSIVE` result,
and has neither cross-process coordination nor later retries. Also,
`journal_size_limit` is connection-local and applies after commit/reset; it is
not an active-WAL hard cap.

Upstream `868ac158d` added unconditional startup `TRUNCATE` plus vacuum;
`ab43db44a` replaced it with `PASSIVE` to avoid contention. `379511dce` moved to
SQLite 3.51.3 for the WAL-reset race fix, now compile-time-enforced here. This
repair must not restore unconditional per-process truncation.

References: <https://www.sqlite.org/wal.html#ckpt>,
<https://www.sqlite.org/c3ref/wal_checkpoint_v2.html>,
<https://www.sqlite.org/pragma.html#pragma_journal_size_limit>.

## Plan and log

| Stage | Status | Evidence |
| --- | --- | --- |
| Ground semantics/history | Completed | Current source, upstream history, SQLite docs |
| Write concurrent/long-reader tests first | Completed | Focused test initially failed with the maintenance API absent |
| Add threshold, one coordinator, observed `PASSIVE`, quiet `TRUNCATE`, retry/alert | Completed | Three focused regression tests pass |
| Validate and commit | Completed | All 191 `codex-state` tests pass; fix and format checks pass |

The 16 MiB `journal_size_limit` is installed on every logs connection only as a
post-reset retention limit. At 64 MiB, off-path maintenance uses a shared
nonblocking lock and five-minute attempt marker, observes `PASSIVE`, and only
then tries `TRUNCATE` with a 100 ms busy timeout. Incomplete attempts at 1 GiB
or above warn and retry. Full workspace tests are excluded by the task. The
pre-implementation focused test failed as expected (2026-09-06).
