"""Real private byte-policy changes, retained receipt, and explicit release.

Run with the same isolated native/fake-Responses fixture as the S6b2 probes.
The debug barrier produces real pending A4 history without a processing claim.
"""

import hashlib
import json
import queue
import socket
import threading

import external_input_probe_support as h

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
            part = connection.recv(1)
            if not part:
                return
            data += part
        event = json.loads(data)
        if event["phase"] == "before_pair":
            connection.sendall(b"\x01")
            connection.close()
        else:
            barriers.put((event, connection))


threading.Thread(target=accept, daemon=True).start()
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

    def launch(name, generation, limit=None, barrier=False):
        value = {
            "version": 1,
            "ownerId": "private-owner",
            "threadId": thread,
            "runtimeGeneration": generation,
        }
        if limit is not None:
            value["canonicalByteLimit"] = limit
        binding.write_text(json.dumps(value))
        binding.chmod(0o600)
        env = (
            dict(
                h.ENV, CODEX_PRIVATE_EXTERNAL_INPUT_PROBE_SOCKET=str(h.RUN / "barrier")
            )
            if barrier
            else None
        )
        child = h.Owner(name, binding, env)
        child.call(
            "thread/resume",
            {
                "threadId": thread,
                "historyMode": "paginated",
                "model": "unknown-private-model",
            },
        )
        return child

    owner = launch("b", 7)
    large = h.envelope(thread, 7, text="x" * 12000)
    rejected = owner.call("thread/externalInput/submit", large, error=True)
    assert "receiver canonical byte policy" in json.dumps(rejected)
    override = dict(large, canonicalByteLimit="off")
    owner.call("thread/externalInput/submit", override, error=True)
    assert h.status(owner, large)["receipt"] is None
    owner.call(
        "thread/externalInput/submit",
        h.envelope(thread, 7, ident="transport", text="x" * 65537),
        error=True,
    )
    assert h.REQUESTS.empty()
    owner.close()

    owner = launch("c", 8, 20000, barrier=True)
    large["runtimeGeneration"] = 8
    owner.call(
        "thread/externalInput/submit",
        h.envelope(thread, 8, ident="transport", text="x" * 65537),
        error=True,
    )
    owner.call("thread/externalInput/submit", large)
    event, connection = barriers.get(timeout=45)
    assert event == {"phase": "after_flush", "messageId": large["message"]["id"]}
    path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
    records = [json.loads(line) for line in path.read_text().splitlines()]
    receipt = next(
        r["payload"]["fact"]["commit"]["receipt"]
        for r in records
        if r["type"] == "external_input" and r["payload"]["fact"]["phase"] == "commit"
    )
    assert h.REQUESTS.empty()
    owner.close(abrupt=True)
    connection.close()

    owner = launch("d", 9)
    large["runtimeGeneration"] = 9
    held = h.status(owner, large)
    assert held["receipt"] == receipt
    assert held["processing"] == {
        "state": "held",
        "attemptId": None,
        "reason": "canonical_size_policy",
    }
    assert owner.call("thread/externalInput/submit", large)["statuses"][0] == held
    retry = {
        k: large[k] for k in ["version", "ownerId", "threadId", "runtimeGeneration"]
    }
    retry.update(
        messageId=large["message"]["id"],
        semanticSha256=large["semanticSha256"],
        expectedAttemptId=None,
        retryId="policy-retry",
    )
    owner.call("thread/externalInput/retry", retry, error=True)
    owner.call(
        "turn/start",
        {
            "threadId": thread,
            "model": "another-unknown-model",
            "input": [
                {
                    "type": "text",
                    "text": "Policy still blocks sampling after a model change.",
                }
            ],
        },
    )
    owner.event("turn/completed")
    assert h.REQUESTS.empty()
    owner.call("thread/compact/start", {"threadId": thread})
    owner.event("turn/completed")
    assert h.REQUESTS.empty(), "compaction bypassed receiver policy"
    facts = [
        r["payload"]["fact"]
        for r in map(json.loads, path.read_text().splitlines())
        if r["type"] == "external_input"
    ]
    assert any(
        f["phase"] == "hold" and f["reason"] == "canonical_size_policy" for f in facts
    )
    assert not any(f["phase"] == "claim" for f in facts)
    owner.close()

    owner = launch("e", 10, "off")
    large["runtimeGeneration"] = retry["runtimeGeneration"] = 10
    assert h.status(owner, large)["processing"]["reason"] == "canonical_size_policy"
    assert h.REQUESTS.empty(), "loosening policy auto-released a held obligation"
    owner.call(
        "thread/externalInput/submit",
        h.envelope(thread, 10, ident="transport", text="x" * 65537),
        error=True,
    )
    owner.call(
        "thread/externalInput/retry", dict(retry, canonicalByteLimit="off"), error=True
    )
    assert h.REQUESTS.empty()
    h.MODES.put(("empty", None))
    owner.call("thread/externalInput/retry", retry)
    request = h.REQUESTS.get(timeout=45)
    owner.event("turn/completed")
    canonical = [i for i in request["input"] if i.get("name") == "external_event"]
    assert (
        len(canonical) == 1
        and json.loads(canonical[0]["output"])["text"] == large["message"]["text"]
    )
    after = h.status(owner, large)
    assert after["receipt"] == receipt and after["processing"]["reason"] == "no_output"
    owner.close()

    owner = launch("f", 11, 20000)
    large["runtimeGeneration"] = 11
    assert h.status(owner, large)["processing"] == after["processing"]
    assert h.REQUESTS.empty(), "restart waived no_output hold"
    fresh = h.envelope(thread, 11, ident="raised-fresh", text="y" * 12000)
    owner.call("thread/externalInput/submit", fresh)
    raised_request = h.REQUESTS.get(timeout=45)
    owner.event("turn/completed")
    raised_canonical = [
        i for i in raised_request["input"] if i.get("name") == "external_event"
    ]
    assert any(
        json.loads(i["output"])
        == {
            "source": fresh["message"]["source"],
            "type": fresh["message"]["type"],
            "text": fresh["message"]["text"],
        }
        for i in raised_canonical
    )
    assert h.status(owner, fresh)["processing"]["state"] == "output_observed"
    assert h.status(owner, large)["processing"] == after["processing"]
    (h.RUN / "request-off.json").write_text(json.dumps(request, indent=2))
    (h.RUN / "request-raised.json").write_text(json.dumps(raised_request, indent=2))
    (h.RUN / "PASS.json").write_text(
        json.dumps(
            {
                "thread_id": thread,
                "receipt": receipt,
                "tighter_policy_held": held,
                "no_output_preserved": after,
                "default_rejection": rejected,
                "unknown_models_accepted": True,
                "binary_sha256": hashlib.file_digest(
                    h.BINARY.open("rb"), "sha256"
                ).hexdigest(),
            },
            indent=2,
        )
    )
finally:
    if owner:
        owner.close()
    control.close()
    h.SERVER.shutdown()
