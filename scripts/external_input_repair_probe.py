"""F6/F7 real native regressions; use the private network namespace fixture."""

import json, hashlib, sqlite3, threading, sys
import external_input_probe_support as h

memory_enabled = len(sys.argv) < 3 or sys.argv[2] != "memory-off"
owner = None
gate = threading.Event()


def memory(thread):
    for db in (h.RUN / "native").glob("state_*.sqlite"):
        with sqlite3.connect(db.as_uri() + "?mode=ro", uri=True) as c:
            row = c.execute(
                "select memory_mode from threads where id=?", (thread,)
            ).fetchone()
            if row:
                return row[0]
    raise AssertionError("no thread row")


def request(name):
    r = h.REQUESTS.get(timeout=45)
    (h.RUN / (name + ".json")).write_text(json.dumps(r, indent=2))
    return r


try:
    config = h.RUN / "native/config.toml"
    config.write_text(
        config.read_text()
        + f"\n[memories]\ndisable_on_external_context={str(memory_enabled).lower()}\n"
    )
    owner = h.Owner("creator")
    t = owner.call(
        "thread/start",
        {
            "cwd": str(h.RUN),
            "historyMode": "paginated",
            "sandbox": "read-only",
            "approvalPolicy": "on-request",
        },
    )["thread"]["id"]
    owner.call("thread/read", {"threadId": t, "includeTurns": True})
    before = memory(t)
    owner.close()
    binding = h.RUN / "binding.json"
    binding.write_text(
        json.dumps(
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": t,
                "runtimeGeneration": 7,
            }
        )
    )
    binding.chmod(0o600)
    owner = h.Owner("bound", binding)
    owner.call("thread/resume", {"threadId": t, "historyMode": "paginated"})
    a = h.envelope(t, 7, ident="retry-A", text="Private identical external body.")
    h.MODES.put(("output", gate))
    owner.call("thread/externalInput/submit", a)
    request("A-inflight")
    claimed = h.status(owner, a)
    first_active_memory = memory(t)
    assert first_active_memory == ("polluted" if memory_enabled else "enabled")
    owner.call(
        "turn/interrupt", {"threadId": t, "turnId": claimed["receipt"]["turnId"]}
    )
    owner.event("turn/completed")
    gate.set()
    held = h.status(owner, a)
    assert held["processing"]["reason"] == "request_uncertain"
    b = h.envelope(
        t,
        7,
        ident="fresh-B",
        delivery="passive",
        text="Fresh passive B must not gain A4 from single retry A.",
    )
    owner.call("thread/externalInput/submit", b)
    pre = h.status(owner, b)
    assert pre["receipt"] is None
    c = h.envelope(t, 7, ident="fresh-C", text="Other active item stays paused.")
    owner.call("thread/externalInput/submit", c)
    c_pre = h.status(owner, c)
    retry = {k: a[k] for k in ["version", "ownerId", "threadId", "runtimeGeneration"]}
    retry.update(
        messageId=a["message"]["id"],
        semanticSha256=a["semanticSha256"],
        expectedAttemptId=held["processing"]["attemptId"],
        retryId="only-A",
    )
    owner.call("thread/externalInput/retry", retry)
    req = request("retry-A-request")
    owner.event("turn/completed")
    after = h.status(owner, b)
    mem_external = memory(t)
    owner.call("thread/read", {"threadId": t, "includeTurns": True})
    path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
    facts = [
        x["payload"]["fact"]
        for x in map(json.loads, path.read_text().splitlines())
        if x["type"] == "external_input"
    ]
    b_present = any(
        i.get("name") == "external_event"
        and json.loads(i["output"])["text"] == b["message"]["text"]
        for i in req["input"]
    )
    assert not b_present and after == pre, (b_present, after)
    assert h.status(owner, c) == c_pre
    assert [f for f in facts if f["phase"] == "dispatch_gate"] == [
        {"phase": "dispatch_gate", "paused": True, "reason": "interrupted"}
    ]
    owner.call(
        "turn/start",
        {
            "threadId": t,
            "input": [
                {"type": "text", "text": "Human explicitly continues pending messages."}
            ],
        },
    )
    human_request = request("human-continue-request")
    owner.event("turn/completed")
    assert h.status(owner, b)["receipt"] is not None
    assert h.status(owner, c)["processing"]["state"] == "output_observed"
    external_result = {
        "first_active_memory": first_active_memory,
        "human_consumed_B": True,
        "other_active_stayed_paused": True,
        "before": before,
        "after_external": mem_external,
        "B_before": pre,
        "B_after": after,
        "B_in_retry_request": b_present,
        "gates": [f for f in facts if f["phase"] == "dispatch_gate"],
    }
    owner.close()
    owner = h.Owner("ctl")
    t2 = owner.call(
        "thread/start",
        {
            "cwd": str(h.RUN),
            "historyMode": "paginated",
            "sandbox": "read-only",
            "approvalPolicy": "on-request",
        },
    )["thread"]["id"]
    owner.call("thread/read", {"threadId": t2, "includeTurns": True})
    control_before = memory(t2)
    owner.call(
        "turn/start",
        {
            "threadId": t2,
            "input": [],
            "toolOutput": {
                "name": "external_event",
                "namespace": "external",
                "output": json.dumps(
                    {
                        "source": a["message"]["source"],
                        "type": a["message"]["type"],
                        "text": a["message"]["text"],
                    }
                ),
            },
        },
    )
    request("toolOutput-request")
    owner.event("turn/completed")
    control_after = memory(t2)
    assert control_after == first_active_memory
    t3 = owner.call(
        "thread/start",
        {
            "cwd": str(h.RUN),
            "historyMode": "paginated",
            "sandbox": "read-only",
            "approvalPolicy": "on-request",
        },
    )["thread"]["id"]
    owner.call("thread/read", {"threadId": t3, "includeTurns": True})
    owner.close()
    binding.write_text(
        json.dumps(
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": t3,
                "runtimeGeneration": 7,
            }
        )
    )
    owner = h.Owner("p", binding)
    owner.call("thread/resume", {"threadId": t3, "historyMode": "paginated"})
    passive = h.envelope(
        t3, 7, ident="passive-only", delivery="passive", text=a["message"]["text"]
    )
    owner.call("thread/externalInput/submit", passive)
    assert memory(t3) == "enabled" and h.status(owner, passive)["receipt"] is None
    owner.call(
        "turn/start",
        {
            "threadId": t3,
            "input": [{"type": "text", "text": "Consume only passive external data."}],
        },
    )
    request("passive-first-request")
    owner.event("turn/completed")
    passive_memory = memory(t3)
    assert passive_memory == control_after
    result = {
        "memory_enabled": memory_enabled,
        "passive_first_memory": passive_memory,
        "native_sha256": hashlib.file_digest(h.BINARY.open("rb"), "sha256").hexdigest(),
        "external": external_result,
        "toolOutput": {"before": control_before, "after": control_after},
        "F6_fixed": not b_present and after["receipt"] is None,
        "F7_fixed": mem_external == control_after == passive_memory,
    }
    (h.RUN / "RESULT.json").write_text(json.dumps(result, indent=2))
finally:
    gate.set()
    if owner:
        owner.close()
    h.SERVER.shutdown()
