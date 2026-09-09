"""F5 real compaction and retry restoration-order regression."""

import json, hashlib, threading
import external_input_probe_support as h

owner = None
gate = threading.Event()


def req(name):
    r = h.REQUESTS.get(timeout=45)
    (h.RUN / (name + ".json")).write_text(json.dumps(r, indent=2))
    return r


def order(r):
    return [
        json.loads(x["output"])["text"]
        for x in r["input"]
        if x.get("name") == "external_event"
    ]


try:
    owner = h.Owner("a")
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
    owner.close()
    binding = h.RUN / "b.json"
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
    owner = h.Owner("b", binding)
    owner.call("thread/resume", {"threadId": t, "historyMode": "paginated"})
    msgs = []
    statuses = []
    for ident, text in [
        ("z-first", "FIRST ordinal one"),
        ("a-second", "SECOND ordinal two"),
    ]:
        e = h.envelope(t, 7, ident=ident, text=text)
        msgs.append(e)
        h.MODES.put(("empty", None))
        owner.call("thread/externalInput/submit", e)
        req(ident)
        owner.event("turn/completed")
        st = h.status(owner, e)
        assert st["processing"]["reason"] == "no_output"
        statuses.append(st)
    h.MODES.put(("output", gate))
    owner.call("thread/compact/start", {"threadId": t})
    comp = req("compact-request")
    for e, st in zip(msgs, statuses):
        r = {k: e[k] for k in ["version", "ownerId", "threadId", "runtimeGeneration"]}
        r.update(
            messageId=e["message"]["id"],
            semanticSha256=e["semanticSha256"],
            expectedAttemptId=st["processing"]["attemptId"],
            retryId="retry-" + e["message"]["id"],
        )
        owner.call("thread/externalInput/retry", r)
    gate.set()
    owner.event("turn/completed")
    after = req("after-compact-request")
    owner.event("turn/completed")
    final = [h.status(owner, e) for e in msgs]
    owner.call("thread/read", {"threadId": t, "includeTurns": True})
    path = next((h.RUN / "native/sessions").rglob("*.jsonl"))
    records = list(map(json.loads, path.read_text().splitlines()))
    result = {
        "binary_sha256": hashlib.file_digest(h.BINARY.open("rb"), "sha256").hexdigest(),
        "receipts_before": [s["receipt"] for s in statuses],
        "order_in_compaction_input": order(comp),
        "order_after_compaction": order(after),
        "final_statuses": final,
        "compacted_records": [r for r in records if r["type"] == "compacted"],
        "F5_fixed": order(after) == ["FIRST ordinal one", "SECOND ordinal two"],
    }
    assert result["F5_fixed"], result
    (h.RUN / "RESULT.json").write_text(json.dumps(result, indent=2))
finally:
    gate.set()
    if owner:
        owner.close()
    h.SERVER.shutdown()
