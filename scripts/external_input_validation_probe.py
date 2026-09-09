"""Actual native validation, bounded admission, Plan mode, and model-size checks."""

import copy
import hashlib
import json

import external_input_probe_support as h

owner = None
try:
    owner = h.Owner("a")
    assert owner.capability.get("externalInputVersion") is None
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
    owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
    base = h.envelope(thread, 7, delivery="passive")
    denied = []
    for role in ["human", "system", "developer", "assistant"]:
        bad = copy.deepcopy(base)
        bad["message"]["source"]["kind"] = role
        denied.append(owner.call("thread/externalInput/submit", bad, error=True))
    for field in ["role", "rawResponseItem", "permissions", "threadSettings"]:
        bad = copy.deepcopy(base)
        bad[field] = "system"
        denied.append(owner.call("thread/externalInput/submit", bad, error=True))
    for delivery in ["soon", "interrupt", "sleep"]:
        bad = copy.deepcopy(base)
        bad["message"]["delivery"] = delivery
        denied.append(owner.call("thread/externalInput/submit", bad, error=True))
    for text in ["x" * 65537, "x" * 10000, '\\"\n世界' * 1500]:
        bad = h.envelope(thread, 7, text=text, delivery="passive")
        denied.append(owner.call("thread/externalInput/submit", bad, error=True))
    bad = h.envelope("00000000-0000-0000-0000-000000000001", 7, delivery="passive")
    denied.append(owner.call("thread/externalInput/submit", bad, error=True))
    assert h.status(owner, base)["deliveryState"] == "unknown"
    assert h.REQUESTS.empty()
    owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [
                {"type": "text", "text": "Enter Plan mode for a private fixture."}
            ],
            "collaborationMode": {
                "mode": "plan",
                "settings": {
                    "model": "gpt-5.4",
                    "reasoning_effort": None,
                    "developer_instructions": None,
                },
            },
        },
    )
    h.REQUESTS.get(timeout=45)
    owner.event("turn/completed")
    planned = h.envelope(
        thread,
        7,
        ident="planned",
        text="Plan mode automatic dispatch must remain held.",
    )
    owner.call("thread/externalInput/submit", planned)
    assert h.status(owner, planned)["processing"]["reason"] == "plan_mode"
    assert h.REQUESTS.empty()
    inputs = [planned]
    for index in range(99):
        value = h.envelope(
            thread,
            7,
            ident=f"passive-{index}",
            text="x" * 9000 if index == 0 else f"bounded passive {index}",
            delivery="passive",
        )
        owner.call("thread/externalInput/submit", value)
        inputs.append(value)
    overflow = h.envelope(thread, 7, ident="overflow", delivery="passive")
    denied.append(owner.call("thread/externalInput/submit", overflow, error=True))
    assert h.status(owner, overflow)["deliveryState"] == "unknown"
    assert (
        owner.call("thread/externalInput/submit", inputs[-1])["statuses"][0][
            "deliveryState"
        ]
        == "pending"
    )
    conflict = h.envelope(
        thread,
        7,
        ident=inputs[-1]["message"]["id"],
        text="conflict",
        delivery="passive",
    )
    assert (
        owner.call("thread/externalInput/submit", conflict)["statuses"][0][
            "deliveryState"
        ]
        == "conflict"
    )
    assert h.status(owner, conflict)["deliveryState"] == "conflict"
    assert h.REQUESTS.empty()
    keys = [
        {"messageId": value["message"]["id"], "semanticSha256": value["semanticSha256"]}
        for value in inputs
    ]
    queried = owner.call(
        "thread/externalInput/status",
        {
            "version": 1,
            "ownerId": "private-owner",
            "threadId": thread,
            "runtimeGeneration": 7,
            "messages": keys,
        },
    )["statuses"]
    assert [status["messageId"] for status in queried] == [
        key["messageId"] for key in keys
    ]
    assert all(status["receipt"] is None for status in queried)
    owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [
                {
                    "type": "text",
                    "text": "A genuine user turn may consume the bounded pending data.",
                }
            ],
        },
    )
    request = h.REQUESTS.get(timeout=45)
    owner.event("turn/completed")
    canonical = [
        item for item in request["input"] if item.get("name") == "external_event"
    ]
    assert len(canonical) == 100, len(canonical)
    assert any(json.loads(item["output"])["text"] == "x" * 9000 for item in canonical)
    assert h.status(owner, planned)["processing"]["state"] == "output_observed"
    assert all(
        h.status(owner, value)["deliveryState"] == "context_persisted"
        for value in inputs
    )
    owner.close()
    owner = h.Owner("c", binding)
    owner.call(
        "thread/resume",
        {
            "threadId": thread,
            "historyMode": "paginated",
            "model": "unknown-private-tokenizer",
        },
    )
    denied.append(owner.call("thread/externalInput/submit", overflow, error=True))
    assert h.status(owner, overflow)["deliveryState"] == "unknown"
    assert h.REQUESTS.empty()
    (h.RUN / "denials.json").write_text(json.dumps(denied, indent=2))
    (h.RUN / "large-request.json").write_text(json.dumps(request, indent=2))
    (h.RUN / "PASS.json").write_text(
        json.dumps(
            {
                "thread_id": thread,
                "denials": len(denied),
                "pending_bound": 100,
                "canonical_items_in_real_request": 100,
                "long_item_text_bytes": 9000,
                "unknown_sizing_rejected": True,
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
    h.SERVER.shutdown()
