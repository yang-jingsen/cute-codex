"""Real native persistence faults, using the debug-only private barrier socket.

Modes: crash-after-flush, append-error, partial-write. All stores and child PIDs
are owned by this fixture. OS RLIMIT_FSIZE faults use inherited SIGXFSZ=SIG_IGN
so native write/flush receives EFBIG; no synthetic native server or DB rows.
"""

import hashlib
import json
import os
import resource
import signal
import socket
import sys
import threading
import queue

import external_input_probe_support as h

mode = sys.argv[2]
assert mode in ["crash-after-flush", "append-error", "partial-write"]
owner = None
barriers = queue.Queue()
control = socket.socket(socket.AF_UNIX)
control.bind(str(h.RUN / "barrier"))
control.listen()


def accept():
    while True:
        try:
            connection, _ = control.accept()
        except OSError:
            return
        data = b""
        while not data.endswith(b"\n"):
            data += connection.recv(1)
        barriers.put((json.loads(data), connection))


threading.Thread(target=accept, daemon=True).start()
signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
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
    env = dict(h.ENV, CODEX_PRIVATE_EXTERNAL_INPUT_PROBE_SOCKET=str(h.RUN / "barrier"))
    owner = h.Owner("b", binding, env, restore_signals=False)
    owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
    value = h.envelope(thread, 7)
    # Submit acknowledges only volatile admission. The A4 status request below
    # remains unanswered while the pair flush/publication boundary is held.
    submitted = owner.call("thread/externalInput/submit", value)
    assert submitted["statuses"][0]["receipt"] is None
    event, connection = barriers.get(timeout=30)
    assert event == {"phase": "before_pair", "messageId": "event-1"}
    path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
    before = path.read_bytes()
    (h.RUN / "before-pair.bytes").write_bytes(before)
    if mode != "crash-after-flush":
        original_limit = resource.prlimit(owner.child.pid, resource.RLIMIT_FSIZE)
        resource.prlimit(
            owner.child.pid,
            resource.RLIMIT_FSIZE,
            (len(before) + (128 if mode == "partial-write" else 0), original_limit[1]),
        )
    connection.sendall(b"\x01")
    connection.close()
    receipt = None
    if mode == "crash-after-flush":
        event, connection = barriers.get(timeout=30)
        assert event == {"phase": "after_flush", "messageId": "event-1"}
        records = [json.loads(line) for line in path.read_text().splitlines()]
        commits = [
            record["payload"]["fact"]["commit"]
            for record in records
            if record["type"] == "external_input"
            and record["payload"]["fact"]["phase"] == "commit"
        ]
        assert len(commits) == 1
        receipt = commits[0]["receipt"]
        assert not any(
            record["type"] == "external_input"
            and record["payload"]["fact"]["phase"] == "claim"
            for record in records
        )
        assert h.REQUESTS.empty()
        # Leave a real A4 status RPC outstanding, then terminate the creator.
        owner.send(
            {
                "id": 9999,
                "method": "thread/externalInput/status",
                "params": {
                    "version": 1,
                    "ownerId": "private-owner",
                    "threadId": thread,
                    "runtimeGeneration": 7,
                    "messages": [
                        {
                            "messageId": "event-1",
                            "semanticSha256": value["semanticSha256"],
                        }
                    ],
                },
            }
        )
        owner.close(abrupt=True)
        connection.close()
    else:
        owner.event("turn/completed")
        status_error = owner.call(
            "thread/externalInput/status",
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": thread,
                "runtimeGeneration": 7,
                "messages": [
                    {"messageId": "event-1", "semanticSha256": value["semanticSha256"]}
                ],
            },
            error=True,
        )
        assert h.REQUESTS.empty(), "sampled uncertain item after writer fault"
        (h.RUN / "status-error.json").write_text(json.dumps(status_error))
        resource.prlimit(owner.child.pid, resource.RLIMIT_FSIZE, original_limit)
        owner.close(abrupt=True)
    failed_bytes = path.read_bytes()
    (h.RUN / "after-boundary.bytes").write_bytes(failed_bytes)
    if mode == "append-error":
        assert failed_bytes == before, "append failure changed native history"
    if mode == "partial-write":
        assert len(failed_bytes) == len(before) + 128
        assert b'"type":"external_input"' in failed_bytes[len(before) :], failed_bytes[
            len(before) :
        ]
        try:
            json.loads(failed_bytes.splitlines()[-1])
            raise AssertionError("expected an actual partial JSONL record")
        except json.JSONDecodeError:
            pass
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
    resumed = owner.call(
        "thread/resume",
        {"threadId": thread, "historyMode": "paginated"},
        error=mode == "partial-write",
    )
    value["runtimeGeneration"] = 8
    if mode == "partial-write":
        owner.call(
            "thread/externalInput/status",
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": thread,
                "runtimeGeneration": 8,
                "messages": [
                    {"messageId": "event-1", "semanticSha256": value["semanticSha256"]}
                ],
            },
            error=True,
        )
        assert h.REQUESTS.empty()
    elif mode == "append-error":
        assert h.status(owner, value)["deliveryState"] == "unknown"
        # A4pre belongs to the external sender: resend this same id/digest.
        owner.call("thread/externalInput/submit", value)
        h.REQUESTS.get(timeout=45)
        owner.event("turn/completed")
        receipt = h.status(owner, value)["receipt"]
    else:
        h.REQUESTS.get(timeout=45)
        owner.event("turn/completed")
        assert h.status(owner, value)["receipt"] == receipt
        assert (
            owner.call("thread/externalInput/submit", value)["statuses"][0]["receipt"]
            == receipt
        )
        assert h.REQUESTS.empty()
    (h.RUN / "PASS.json").write_text(
        json.dumps(
            {
                "mode": mode,
                "thread_id": thread,
                "receipt": receipt,
                "actual_creator_sigkill": True,
                "native_binary_sha256": hashlib.file_digest(
                    h.BINARY.open("rb"), "sha256"
                ).hexdigest(),
                "fault": "OS RLIMIT_FSIZE with ignored SIGXFSZ"
                if mode != "crash-after-flush"
                else "debug barrier + actual SIGKILL before A4 reply",
            },
            indent=2,
        )
    )
finally:
    if owner:
        owner.close()
    control.close()
    h.SERVER.shutdown()
