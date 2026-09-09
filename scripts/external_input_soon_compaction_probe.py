"""Deterministic inline compaction boundary, real native/fake Responses."""

import hashlib
import http.server
import json
import queue
import threading
import sys
import external_input_probe_support as h

initial = len(sys.argv) > 2 and sys.argv[2] == "initial"
seen = queue.Queue()
gate = threading.Event()
requests = []


class Responses(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        requests.append(body)
        n = len(requests)
        (h.RUN / f"request-{n}.json").write_text(json.dumps(body, indent=2))
        seen.put(n)
        assert n <= 4, "unexpected repeated sampling/compaction"
        if n == 2:
            assert gate.wait(40)
        if n == 1 and not initial:
            item = {
                "type": "function_call",
                "call_id": "compaction-tool",
                "name": "exec_command",
                "arguments": json.dumps(
                    {
                        "cmd": "printf private-compaction-fixture",
                        "sandbox_permissions": "require_escalated",
                        "justification": "Decline this private fixture request.",
                    }
                ),
            }
        else:
            item = {
                "type": "message",
                "id": f"msg-{n}",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": "Private summary" if n == 2 else "Soon reply",
                    }
                ],
            }
        events = [{"type": "response.created", "response": {"id": f"resp-{n}"}}]
        if n != 3:
            events.append({"type": "response.output_item.done", "item": item})
        events.append(
            {
                "type": "response.completed",
                "response": {
                    "id": f"resp-{n}",
                    "usage": {
                        "input_tokens": 200000 if n == 1 else 100,
                        "output_tokens": 0,
                        "total_tokens": 200000 if n == 1 else 100,
                    },
                },
            }
        )
        data = "".join("data: " + json.dumps(e) + "\n\n" for e in events).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        self.wfile.write(data)


h.SERVER.RequestHandlerClass = Responses
config = h.RUN / "native/config.toml"
config.write_text("model_auto_compact_token_limit=100000\n" + config.read_text())
owner = None
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
    owner.call("thread/read", {"threadId": thread, "includeTurns": True})
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
    turn = owner.call(
        "turn/start",
        {
            "threadId": thread,
            "input": [{"type": "text", "text": "Controlled inline compaction"}],
        },
    )["turn"]["id"]
    assert seen.get(timeout=45) == 1
    if initial:
        owner.event("turn/completed")
        turn = owner.call(
            "turn/start",
            {
                "threadId": thread,
                "input": [{"type": "text", "text": "Fresh direct input before Soon"}],
            },
        )["turn"]["id"]
    else:
        approval = owner.event("item/commandExecution/requestApproval")
        owner.send({"id": approval["id"], "result": {"decision": "decline"}})
    assert seen.get(timeout=45) == 2
    value = h.envelope(
        thread,
        7,
        ident="inline-soon",
        delivery="soon",
        text="Only after deferred continuation",
    )
    owner.call("thread/externalInput/submit", value)
    assert h.status(owner, value)["receipt"] is None
    gate.set()
    assert seen.get(timeout=45) == 3
    assert seen.get(timeout=45) == 4
    owner.event("turn/completed")
    assert all(
        not any(i.get("name") == "external_event" for i in b["input"])
        for b in requests[:3]
    )
    assert any(i.get("name") == "external_event" for i in requests[3]["input"])
    if initial:
        assert "Fresh direct input before Soon" in json.dumps(requests[2]["input"])
    final = h.status(owner, value)
    assert (
        final["receipt"]["turnId"] == turn
        and final["processing"]["state"] == "output_observed"
    )
    owner.close()
    owner = None
    records = [
        json.loads(line)
        for p in (h.RUN / "native/sessions").rglob("*.jsonl")
        for line in p.read_text().splitlines()
    ]
    assert any(r["type"] == "compacted" for r in records), "not a real compaction"
    (h.RUN / "PASS.json").write_text(
        json.dumps(
            {
                "thread": thread,
                "turn": turn,
                "status": final,
                "requests": 4,
                "native_compacted_record": True,
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
