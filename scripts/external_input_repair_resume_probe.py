"""F5 bind/resume from a genuine native compaction checkpoint prefix copy."""

import json, hashlib, sys
from pathlib import Path
import external_input_probe_support as h

owner = None
try:
    original = next((Path(sys.argv[2]) / "native/sessions").rglob("*.jsonl"))
    lines = []
    for line in original.read_bytes().splitlines(keepends=True):
        lines.append(line)
        if json.loads(line)["type"] == "compacted":
            break
    partial = len(sys.argv) > 3 and sys.argv[3] == "partial"
    if partial:
        # Construct only a new fixture checkpoint with the later item retained.
        # No original history or receipt/digest is changed.
        retained = next(
            json.loads(line)["payload"]
            for line in lines
            if json.loads(line)["type"] == "response_item"
            and json.loads(line)["payload"].get("id") == "a-second"
        )
        checkpoint = json.loads(lines[-1])
        checkpoint["payload"]["replacement_history"].append(retained)
        lines[-1] = (json.dumps(checkpoint) + "\n").encode()
    t = json.loads(lines[0])["payload"]["id"]
    dest = h.RUN / "native/sessions" / original.name
    dest.parent.mkdir(exist_ok=True)
    dest.write_bytes(b"".join(lines))
    binding = h.RUN / "b.json"
    binding.write_text(
        json.dumps(
            {
                "version": 1,
                "ownerId": "private-owner",
                "threadId": t,
                "runtimeGeneration": 8,
            }
        )
    )
    binding.chmod(0o600)
    owner = h.Owner("a", binding)
    owner.call("thread/resume", {"threadId": t, "historyMode": "paginated"})
    owner.call(
        "turn/start",
        {
            "threadId": t,
            "input": [{"type": "text", "text": "Private resume order observation."}],
        },
    )
    req = h.REQUESTS.get(timeout=45)
    (h.RUN / "request.json").write_text(json.dumps(req, indent=2))
    owner.event("turn/completed")
    order = [
        json.loads(i["output"])["text"]
        for i in req["input"]
        if i.get("name") == "external_event"
    ]
    expected = (
        ["SECOND ordinal two", "FIRST ordinal one"]
        if partial
        else ["FIRST ordinal one", "SECOND ordinal two"]
    )
    assert order == expected, order
    (h.RUN / "RESULT.json").write_text(
        json.dumps(
            {
                "order": order,
                "fixed": order == expected,
                "partial_retention_fixture": partial,
                "fixture": (
                    "new checkpoint fixture retaining the later canonical item"
                    if partial
                    else "exact native checkpoint prefix copy"
                ),
                "prefix_sha256": hashlib.sha256(b"".join(lines)).hexdigest(),
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
