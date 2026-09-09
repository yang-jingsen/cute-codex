"""Real native Soon boundaries with deterministic fake-Responses/approval gates."""

import hashlib
import json
import threading
import external_input_probe_support as h

owner = None
gate = threading.Event()
requests = []


def request():
    value = h.REQUESTS.get(timeout=45)
    requests.append(value)
    (h.RUN / f"request-{len(requests)}.json").write_text(json.dumps(value, indent=2))
    return value


def contains(body, message):
    return any(
        item.get("name") == "external_event"
        and json.loads(item["output"])["text"] == message["message"]["text"]
        for item in body["input"]
    )


def submit(ident, delivery="soon"):
    value = h.envelope(
        thread, 7, ident=ident, delivery=delivery, text="Canonical " + ident
    )
    owner.call("thread/externalInput/submit", value)
    return value


try:
    owner = h.Owner("a")
    assert owner.capability.get("externalInputDeliveries") is None
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
    owner = h.Owner("b", binding)
    assert owner.capability["externalInputDeliveries"] == [
        "after_turn",
        "passive",
        "soon",
    ]
    owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
    idle = submit("idle")
    assert contains(request(), idle)
    owner.event("turn/completed")
    idle_status = h.status(owner, idle)
    assert idle_status["processing"]["state"] == "output_observed"
    assert owner.call("thread/externalInput/submit", idle)["statuses"][0] == idle_status
    assert h.REQUESTS.empty()

    # No model-request follow-up/tool is requested: Soon alone must keep this
    # regular turn alive after a controlled empty response.
    h.MODES.put(("empty", gate))
    turn = owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [{"type": "text", "text": "Controlled first request"}],
        },
    )["turn"]["id"]
    first = request()
    soon = submit("same-turn-continuation")
    after = submit("excluded-after", "after_turn")
    passive = submit("boundary-passive", "passive")
    assert all(h.status(owner, v)["receipt"] is None for v in [soon, after, passive])
    gate.set()
    follow = request()
    assert (
        contains(follow, soon)
        and contains(follow, passive)
        and not contains(follow, after)
    )
    owner.event("turn/completed")
    assert h.status(owner, soon)["receipt"]["turnId"] == turn
    assert h.status(owner, passive)["receipt"]["turnId"] == turn
    later = request()
    owner.event("turn/completed")
    assert (
        contains(later, after) and h.status(owner, after)["receipt"]["turnId"] != turn
    )

    # Actual native approval blocks tool completion. Incoming modes do not
    # authorize/cancel it; controller explicitly declines only after observation.
    h.MODES.put(("approval", None))
    turn = owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [{"type": "text", "text": "Controlled approval boundary"}],
        },
    )["turn"]["id"]
    request()
    approval = owner.event("item/commandExecution/requestApproval")
    soon_tool = submit("tool-soon")
    after_tool = submit("tool-after", "after_turn")
    passive_tool = submit("tool-passive", "passive")
    assert all(
        h.status(owner, v)["receipt"] is None
        for v in [soon_tool, after_tool, passive_tool]
    )
    assert h.REQUESTS.empty()
    owner.send({"id": approval["id"], "result": {"decision": "decline"}})
    follow = request()
    assert (
        contains(follow, soon_tool)
        and contains(follow, passive_tool)
        and not contains(follow, after_tool)
    )
    owner.event("turn/completed")
    assert h.status(owner, soon_tool)["receipt"]["turnId"] == turn
    request()
    owner.event("turn/completed")
    assert h.status(owner, after_tool)["receipt"]["turnId"] != turn

    # Empty successful processing holds Soon; duplicate/status/restart do not retry.
    h.MODES.put(("empty", None))
    held = submit("held-empty")
    request()
    owner.event("turn/completed")
    held_status = h.status(owner, held)
    assert held_status["processing"]["reason"] == "no_output", held_status
    owner.close(abrupt=True)
    data = json.loads(binding.read_text())
    data["runtimeGeneration"] = 8
    binding.write_text(json.dumps(data))
    owner = h.Owner("c", binding)
    owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
    held["runtimeGeneration"] = 8
    assert h.status(owner, held) == held_status
    assert owner.call("thread/externalInput/submit", held)["statuses"][0] == held_status
    assert h.REQUESTS.empty()
    retry = {
        key: held[key]
        for key in ["version", "ownerId", "threadId", "runtimeGeneration"]
    }
    retry.update(
        messageId=held["message"]["id"],
        semanticSha256=held["semanticSha256"],
        expectedAttemptId=held_status["processing"]["attemptId"],
        retryId="one-permit",
    )
    gate.clear()
    h.MODES.put(("empty", gate))
    busy = owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [{"type": "text", "text": "Retry cannot join current turn"}],
        },
    )["turn"]["id"]
    request()
    released = owner.call("thread/externalInput/retry", retry)
    assert owner.call("thread/externalInput/retry", retry) == released
    assert h.status(owner, held)["processing"]["state"] == "pending"
    gate.set()
    owner.event("turn/completed")
    retried = request()
    owner.event("turn/completed")
    assert contains(retried, held)
    assert h.status(owner, held)["receipt"] == held_status["receipt"]
    assert h.status(owner, held)["processing"]["state"] == "output_observed"
    for body in requests:
        wire = json.dumps(body)
        for mechanical in [
            "semanticSha256",
            "receiptId",
            "runtimeGeneration",
            "thread/externalInput/",
        ]:
            assert mechanical not in wire
    # A standalone compaction is not a regular-turn consumption boundary.
    gate.clear()
    h.MODES.put(("output", gate))
    owner.call("thread/compact/start", {"threadId": thread})
    compact_request = request()
    compact_soon = h.envelope(
        thread,
        8,
        ident="after-compaction",
        delivery="soon",
        text="Canonical after compaction",
    )
    owner.call("thread/externalInput/submit", compact_soon)
    assert h.status(owner, compact_soon)["receipt"] is None
    gate.set()
    owner.event("turn/completed")
    after_compact = request()
    owner.event("turn/completed")
    assert not contains(compact_request, compact_soon) and contains(
        after_compact, compact_soon
    )
    owner.close()
    owner = None
    records = [
        json.loads(line)
        for path in (h.RUN / "native/sessions").rglob("*.jsonl")
        for line in path.read_text().splitlines()
    ]
    current = None
    retry_turns = []
    for record in records:
        payload = record["payload"]
        if record["type"] == "event_msg" and payload["type"] == "task_started":
            current = payload["turn_id"]
        if (
            record["type"] == "external_input"
            and payload["fact"]["phase"] == "claim"
            and payload["fact"]["key"]["messageId"] == "held-empty"
        ):
            retry_turns.append(current)
    assert len(retry_turns) == 2 and retry_turns[-1] != busy, retry_turns
    (h.RUN / "PASS.json").write_text(
        json.dumps(
            {
                "thread": thread,
                "requests": len(requests),
                "idle": idle_status,
                "same_turn": turn,
                "held_receipt": held_status["receipt"],
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
