"""Native interruption gate, held request recovery, and migration rejection."""

import hashlib
import json
import sqlite3
import queue
import resource
import signal
import sys
import threading

import external_input_probe_support as h

writer_fault = len(sys.argv) > 2 and sys.argv[2] == "writer-error"
retry_queue = len(sys.argv) > 2 and sys.argv[2] == "retry-queue"
if writer_fault:
    signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
owner = None
gate = threading.Event()
try:
    owner = h.Owner("a")
    thread = owner.call(
        "thread/start",
        {
            "cwd": str(h.RUN),
            "ephemeral": False,
            "historyMode": "paginated",
            "sandbox": "read-only",
            "approvalPolicy": "on-request",
        },
    )["thread"]["id"]
    (h.RUN / "thread-id").write_text(thread)
    assert (
        owner.call("thread/read", {"threadId": thread, "includeTurns": True})["thread"][
            "turns"
        ]
        == []
    )
    owner.close()
    binding = h.RUN / "binding.json"
    binding.write_text(
        json.dumps(
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": thread,
                "runtimeGeneration": 7,
            }
        )
    )
    binding.chmod(0o600)
    owner = h.Owner("b", binding, restore_signals=not writer_fault)
    owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
    active = h.envelope(
        thread, 7, ident="interrupted-request", text="Uncertain request remains held."
    )
    h.MODES.put(("output", gate))
    owner.call("thread/externalInput/submit", active)
    h.REQUESTS.get(timeout=45)
    claimed = h.status(owner, active)
    assert claimed["processing"]["state"] == "claimed"
    if writer_fault:
        path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
        before = path.read_bytes()
        previous = resource.prlimit(owner.child.pid, resource.RLIMIT_FSIZE)
        resource.prlimit(
            owner.child.pid, resource.RLIMIT_FSIZE, (len(before), previous[1])
        )
        denied = owner.call(
            "turn/interrupt",
            {"threadId": thread, "turnId": claimed["receipt"]["turnId"]},
            error=True,
        )
        owner.call(
            "thread/externalInput/status",
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": thread,
                "runtimeGeneration": 7,
                "messages": [
                    {
                        "messageId": active["message"]["id"],
                        "semanticSha256": active["semanticSha256"],
                    }
                ],
            },
            error=True,
        )
        resource.prlimit(owner.child.pid, resource.RLIMIT_FSIZE, previous)
        owner.close(abrupt=True)
        gate.set()
        assert path.read_bytes() == before
        binding.write_text(
            json.dumps(
                {
                    "version": 1,
                    "ownerId": "private-owner",
                    "threadId": thread,
                    "runtimeGeneration": 8,
                }
            )
        )
        owner = h.Owner("c", binding)
        owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
        active["runtimeGeneration"] = 8
        recovered = h.status(owner, active)
        assert recovered["receipt"] == claimed["receipt"]
        assert recovered["processing"]["reason"] == "request_uncertain"
        assert h.REQUESTS.empty()
        (h.RUN / "PASS.json").write_text(
            json.dumps(
                {
                    "thread_id": thread,
                    "interrupt_error_not_ack": denied,
                    "recovered": recovered,
                    "fault": "actual OS EFBIG",
                    "binary_sha256": hashlib.file_digest(
                        h.BINARY.open("rb"), "sha256"
                    ).hexdigest(),
                },
                indent=2,
            )
        )
        raise SystemExit(0)
    pending = h.envelope(
        thread,
        7,
        ident="pending-after-interrupt",
        text="Unclaimed input can resume on a genuine user turn.",
    )
    owner.call("thread/externalInput/submit", pending)
    if retry_queue:
        owner.call(
            "thread/queue/add",
            {
                "threadId": thread,
                "clientUserMessageId": "paused-user",
                "input": [
                    {
                        "type": "text",
                        "text": "Previously queued user input must remain paused.",
                    }
                ],
            },
        )
    owner.call(
        "turn/interrupt", {"threadId": thread, "turnId": claimed["receipt"]["turnId"]}
    )
    owner.event("turn/completed")
    gate.set()
    held = h.status(owner, active)
    assert held["processing"]["state"] == "held", held
    assert held["processing"]["reason"] == "request_uncertain", held
    assert held["receipt"] == claimed["receipt"]
    paused = h.status(owner, pending)
    assert (
        paused["receipt"] is None and paused["processing"]["reason"] == "interrupted"
    ), paused
    if retry_queue:
        retry = {
            key: active[key]
            for key in ["version", "ownerId", "threadId", "runtimeGeneration"]
        }
        retry.update(
            messageId=active["message"]["id"],
            semanticSha256=active["semanticSha256"],
            expectedAttemptId=held["processing"]["attemptId"],
            retryId="retry-only-active",
        )
        owner.call("thread/externalInput/retry", retry)
        h.REQUESTS.get(timeout=45)
        owner.event("turn/completed")
        # Observe beyond one upstream queue poll interval, then prove the queue
        # and gate persisted; absence alone is not the acceptance oracle.
        try:
            unexpected = h.REQUESTS.get(timeout=11)
        except queue.Empty:
            unexpected = None
        assert unexpected is None, unexpected
        after = h.status(owner, pending)
        assert after == paused
        queued = owner.call("thread/queue/list", {"threadId": thread})["data"]
        assert len(queued) == 1, queued
        owner.call("thread/read", {"threadId": thread, "includeTurns": True})
        path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
        facts = [
            r["payload"]["fact"]
            for r in map(json.loads, path.read_text().splitlines())
            if r["type"] == "external_input"
        ]
        assert [f for f in facts if f["phase"] == "dispatch_gate"] == [
            {"phase": "dispatch_gate", "paused": True, "reason": "interrupted"}
        ]
        queue_rows = []
        for db in (h.RUN / "native").glob("*.sqlite"):
            with sqlite3.connect(db.as_uri() + "?mode=ro", uri=True) as connection:
                if connection.execute(
                    "select name from sqlite_master where type='table' and name='queued_items'"
                ).fetchone():
                    queue_rows.extend(
                        connection.execute("select * from queued_items").fetchall()
                    )
        assert len(queue_rows) == 1, queue_rows
        # This controller invocation is explicitly authorized continuation.
        owner.call("thread/queue/start", {"threadId": thread})
        h.REQUESTS.get(timeout=45)
        completed = owner.event("turn/completed")
        assert h.status(owner, pending)["processing"]["state"] == "output_observed"
        assert owner.call("thread/queue/list", {"threadId": thread})["data"] == []
        assert h.REQUESTS.empty()
        evidence = {
            "paused_after_single_retry": after,
            "retained_queue": queued,
            "persistent_queue_rows": queue_rows,
            "gate_before_explicit_start": facts[-1],
            "explicit_queue_completed": completed,
            "binary_sha256": hashlib.file_digest(
                h.BINARY.open("rb"), "sha256"
            ).hexdigest(),
        }
        (h.RUN / "PASS.json").write_text(json.dumps(evidence, indent=2))
        raise SystemExit(0)
    owner.call("thread/fork", {"threadId": thread}, error=True)
    owner.call("thread/rollback", {"threadId": thread, "numTurns": 1}, error=True)
    assert h.REQUESTS.empty()
    owner.close(abrupt=True)
    binding.write_text(
        json.dumps(
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": thread,
                "runtimeGeneration": 8,
            }
        )
    )
    owner = h.Owner("c", binding)
    owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
    active["runtimeGeneration"] = pending["runtimeGeneration"] = 8
    assert h.status(owner, active) == held
    assert h.status(owner, pending)["deliveryState"] == "unknown"
    # Native pending admission is volatile: an external fixture resends A4pre.
    owner.call("thread/externalInput/submit", pending)
    assert h.status(owner, pending) == paused
    assert h.REQUESTS.empty()
    owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [
                {
                    "type": "text",
                    "text": "A genuine private user turn releases only unclaimed interruption pause.",
                }
            ],
        },
    )
    h.REQUESTS.get(timeout=45)
    owner.event("turn/completed")
    assert h.status(owner, active) == held, "human turn released uncertain request"
    assert h.status(owner, pending)["processing"]["state"] == "output_observed"
    assert h.REQUESTS.empty()
    owner.close()
    path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
    facts = [
        record["payload"]["fact"]
        for record in map(json.loads, path.read_text().splitlines())
        if record["type"] == "external_input"
    ]
    assert [fact for fact in facts if fact["phase"] == "dispatch_gate"] == [
        {"phase": "dispatch_gate", "paused": True, "reason": "interrupted"},
        {"phase": "dispatch_gate", "paused": False, "reason": "interrupted"},
    ]
    assert (
        sum(
            fact["phase"] == "claim"
            and fact["key"]["messageId"] == active["message"]["id"]
            for fact in facts
        )
        == 1
    )
    for db in (h.RUN / "native").glob("state_*.sqlite"):
        with sqlite3.connect(db.as_uri() + "?mode=ro", uri=True) as connection:
            assert connection.execute("select id from threads").fetchall() == [
                (thread,)
            ]
    (h.RUN / "PASS.json").write_text(
        json.dumps(
            {
                "thread_id": thread,
                "held": held,
                "pending_external_resend_required": True,
                "migration_denied_before_new_row": True,
                "binary_sha256": hashlib.file_digest(
                    h.BINARY.open("rb"), "sha256"
                ).hexdigest(),
            },
            indent=2,
        )
    )
finally:
    gate.set()
    if owner:
        owner.close()
    h.SERVER.shutdown()
