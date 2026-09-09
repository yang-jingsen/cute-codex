"""Independent read-only JSONL/SQLite oracle for the private native probe."""

import hashlib
import json
from pathlib import Path
import sqlite3
import struct
import sys

run = Path(sys.argv[1]).resolve()
thread = (run / "thread-id").read_text()
paths = list((run / "native/sessions").rglob("*.jsonl"))
assert len(paths) == 1, paths
records = [json.loads(line) for line in paths[0].read_text().splitlines()]
assert sum(item["type"] == "session_meta" for item in records) == 1
assert (
    next(item for item in records if item["type"] == "session_meta")["payload"]["id"]
    == thread
)
turn = None
receipts = {}
phases = []
for index, record in enumerate(records):
    payload = record["payload"]
    if record["type"] == "event_msg":
        if payload["type"] == "task_started":
            turn = payload["turn_id"]
        elif payload["type"] in ["task_complete", "turn_aborted"]:
            turn = None
    if record["type"] != "external_input":
        continue
    assert payload["version"] == 1 and payload["threadId"] == thread
    fact = payload["fact"]
    phases.append(fact["phase"])
    if fact["phase"] != "commit":
        continue
    envelope, receipt = fact["commit"]["envelope"], fact["commit"]["receipt"]
    message = envelope["message"]
    assert receipt["messageId"] not in receipts
    assert receipt["ordinal"] == len(receipts) + 1
    assert receipt["turnId"] == turn, (receipt["turnId"], turn)
    fields = [
        envelope["ownerId"],
        thread,
        message["id"],
        envelope["semanticSha256"],
        receipt["responseItemId"],
        turn,
    ]
    framed = b"".join(
        struct.pack("!Q", len(field.encode())) + field.encode() for field in fields
    )
    expected = hashlib.sha256(
        b"codex:external-input-receipt:v1\0"
        + framed
        + struct.pack("!Q", receipt["ordinal"])
    ).hexdigest()
    assert receipt["receiptId"] == "eir1_" + expected
    item = records[index + 1]
    assert (
        item["type"] == "response_item"
        and item["payload"]["type"] == "function_call_output"
    )
    assert item["payload"]["id"] == message["id"]
    assert item["payload"]["name"] == "external_event"
    assert item["payload"]["namespace"] == "external"
    assert item["payload"].get("call_id") is None
    assert json.loads(item["payload"]["output"]) == {
        "source": message["source"],
        "type": message["type"],
        "text": message["text"],
    }
    receipts[receipt["messageId"]] = receipt
rows = []
for path in (run / "native").glob("state_*.sqlite"):
    with sqlite3.connect(path.as_uri() + "?mode=ro", uri=True) as connection:
        rows += connection.execute(
            "select id, rollout_path from threads where id = ?", (thread,)
        ).fetchall()
assert rows == [(thread, str(paths[0]))], rows
result = {
    "thread_id": thread,
    "session_meta_count": 1,
    "metadata_rows": rows,
    "receipts": receipts,
    "phases": phases,
    "read_only": True,
}
print(json.dumps(result, indent=2))
