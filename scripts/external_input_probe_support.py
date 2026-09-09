"""Real native Unix RPC and fake Responses probe, inside a private network namespace.

Run under bwrap --unshare-user --unshare-pid --unshare-net with only this
owned task root writable. No credentials, external service, or production store.
"""

import base64
import errno
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import signal
import socket
import struct
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(
    os.environ.get(
        "CODEX_PRIVATE_PROBE_APP_SERVER", str(ROOT / "target/debug/codex-app-server")
    )
)
RUN = ROOT / sys.argv[1]
RUN.mkdir(mode=0o700)
assert socket.if_nameindex() == [(1, "lo")]
with socket.socket() as oracle:
    try:
        oracle.connect(("192.0.2.1", 443))
        raise AssertionError("external network available")
    except OSError as error:
        assert error.errno == errno.ENETUNREACH
(RUN / "network.json").write_text(
    json.dumps(
        {"interfaces": socket.if_nameindex(), "external_errno": errno.ENETUNREACH}
    )
)
REQUESTS = queue.Queue()
MODES = queue.Queue()


class Responses(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        assert self.path == "/v1/responses", self.path
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        REQUESTS.put(body)
        try:
            mode, gate = MODES.get_nowait()
        except queue.Empty:
            mode, gate = "output", None
        if gate:
            assert gate.wait(40), "controlled response release timeout"
        if mode == "disconnect":
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()
            return
        ident = "resp-private"
        events = [
            {"type": "response.created", "response": {"id": ident}},
            {
                "type": "response.output_item.done",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "id": "msg-private",
                    "content": [
                        {"type": "output_text", "text": "Private fixture response."}
                    ],
                },
            },
            {
                "type": "response.completed",
                "response": {
                    "id": ident,
                    "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
                },
            },
        ]
        if mode == "approval":
            events[1] = {
                "type": "response.output_item.done",
                "item": {
                    "type": "function_call",
                    "call_id": "approval-fixture",
                    "name": "exec_command",
                    "arguments": json.dumps(
                        {
                            "cmd": "printf private-approval-fixture",
                            "sandbox_permissions": "require_escalated",
                            "justification": "Private fixture: controller will deny this command.",
                        }
                    ),
                },
            }
        if mode == "empty":
            events.pop(1)
        data = "".join(
            "data: " + json.dumps(event) + "\n\n" for event in events
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            # The interruption fixture deliberately cancels an in-flight request.
            pass


SERVER = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Responses)
threading.Thread(target=SERVER.serve_forever, daemon=True).start()
for directory in ["home", "native", "tmp"]:
    (RUN / directory).mkdir(mode=0o700)
(RUN / "native/config.toml").write_text(f"""model = "gpt-5.4"
model_provider = "private-fixture"
[model_providers.private-fixture]
name = "Private fake Responses"
base_url = "http://127.0.0.1:{SERVER.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
[analytics]
enabled = false
""")
ENV = {
    "PATH": "/usr/bin:/bin",
    "HOME": str(RUN / "home"),
    "CODEX_HOME": str(RUN / "native"),
    "TMPDIR": str(RUN / "tmp"),
    "RUST_LOG": "codex_app_server_transport=info",
    "LANG": "C.UTF-8",
}


class Owner:
    def __init__(self, name, binding=None, env=None, restore_signals=True):
        self.events = []
        self.messages = queue.Queue()
        self.serial = 0
        self.lock = threading.Lock()
        self.path = RUN / name
        self.log = open(RUN / (name + ".log"), "wb")
        args = [
            str(BINARY),
            "--listen",
            "unix://" + str(self.path),
            "--disable-plugin-startup-tasks-for-tests",
        ]
        if binding:
            args += ["--external-input-binding-file", str(binding)]
        ready = threading.Event()
        self.child = subprocess.Popen(
            args,
            env=env or ENV,
            cwd=RUN,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            close_fds=True,
            restore_signals=restore_signals,
            start_new_session=True,
        )

        def logs():
            for line in self.child.stderr:
                self.log.write(line)
                self.log.flush()
                if b"app-server control socket listening" in line:
                    ready.set()
            ready.set()

        threading.Thread(target=logs, daemon=True).start()
        try:
            assert ready.wait(30), "native readiness timeout"
            assert self.child.poll() is None, "native failed before listening"
            self.sock = socket.socket(socket.AF_UNIX)
            self.sock.settimeout(30)
            self.sock.connect(str(self.path))
            key = base64.b64encode(os.urandom(16)).decode()
            self.sock.sendall(
                f"GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode()
            )
            response = b""
            while not response.endswith(b"\r\n\r\n"):
                response += self.sock.recv(1)
            assert response.startswith(b"HTTP/1.1 101 "), response
            self.sock.settimeout(None)
            threading.Thread(target=self.read, daemon=True).start()
            self.capability = self.call(
                "initialize",
                {
                    "clientInfo": {"name": "private-native-probe", "version": "1"},
                    "capabilities": {"experimentalApi": True},
                },
            )
            self.send({"method": "initialized"})
        except BaseException:
            self.close(abrupt=True)
            raise

    def frame(self, payload, opcode=1):
        mask = os.urandom(4)
        size = len(payload)
        header = (
            bytes([0x80 | opcode, 0x80 | size])
            if size < 126
            else bytes([0x80 | opcode, 0x80 | 126]) + struct.pack("!H", size)
            if size < 65536
            else bytes([0x80 | opcode, 0x80 | 127]) + struct.pack("!Q", size)
        )
        with self.lock:
            self.sock.sendall(
                header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            )

    def send(self, value):
        self.frame(json.dumps(value).encode())

    def read(self):
        def exact(size):
            data = b""
            while len(data) < size:
                part = self.sock.recv(size - len(data))
                if not part:
                    raise EOFError()
                data += part
            return data

        try:
            while True:
                first, second = exact(2)
                assert first & 0x80 and not second & 0x80, (
                    "fragmented/masked server frame"
                )
                size = second & 127
                if size == 126:
                    size = struct.unpack("!H", exact(2))[0]
                elif size == 127:
                    size = struct.unpack("!Q", exact(8))[0]
                assert size < 16 * 1024 * 1024
                data = exact(size)
                if first & 15 == 9:
                    self.frame(data, 10)
                elif first & 15 == 8:
                    break
                else:
                    assert first & 15 == 1
                    message = json.loads(data)
                    with (RUN / (self.path.name + ".messages.jsonl")).open(
                        "a"
                    ) as record:
                        record.write(json.dumps(message) + "\n")
                    self.messages.put(message)
        except (EOFError, OSError):
            pass
        finally:
            self.messages.put({"eof": True})

    def call(self, method, params, error=False):
        self.serial += 1
        self.send({"id": self.serial, "method": method, "params": params})
        deadline = time.monotonic() + 45
        while True:
            item = self.messages.get(timeout=max(0.001, deadline - time.monotonic()))
            assert "eof" not in item, ("native EOF", method)
            if item.get("id") == self.serial:
                (RUN / "rpc.jsonl").open("a").write(
                    json.dumps({"method": method, "reply": item}) + "\n"
                )
                if error:
                    assert "error" in item, item
                    return item
                assert "error" not in item, item
                return item["result"]
            self.events.append(item)

    def event(self, method):
        deadline = time.monotonic() + 45
        while True:
            for index, item in enumerate(self.events):
                if item.get("method") == method:
                    return self.events.pop(index)
            item = self.messages.get(timeout=max(0.001, deadline - time.monotonic()))
            assert "eof" not in item
            self.events.append(item)

    def close(self, abrupt=False):
        if self.child.poll() is None:
            os.killpg(self.child.pid, signal.SIGKILL if abrupt else signal.SIGTERM)
        code = self.child.wait(timeout=30)
        (RUN / (self.path.name + ".exit.json")).write_text(
            json.dumps({"exit_code": code})
        )
        if hasattr(self, "sock"):
            self.sock.close()


def envelope(
    thread,
    generation,
    text="Common external data: 世界",
    delivery="after_turn",
    ident="event-1",
):
    message = {
        "id": ident,
        "source": {"kind": "service", "id": "fixture"},
        "type": "opaque.example",
        "delivery": delivery,
        "text": text,
    }
    fields = [
        "private-owner",
        thread,
        ident,
        "service",
        "fixture",
        "opaque.example",
        delivery,
        text,
    ]
    digest = hashlib.sha256(
        b"codex:external-input:v1\0"
        + b"".join(
            struct.pack("!Q", len(field.encode())) + field.encode() for field in fields
        )
    ).hexdigest()
    return {
        "version": 1,
        "ownerId": "private-owner",
        "threadId": thread,
        "runtimeGeneration": generation,
        "message": message,
        "semanticSha256": digest,
    }


def status(owner, value):
    params = {
        key: value[key]
        for key in ["version", "ownerId", "threadId", "runtimeGeneration"]
    }
    params["messages"] = [
        {"messageId": value["message"]["id"], "semanticSha256": value["semanticSha256"]}
    ]
    return owner.call("thread/externalInput/status", params)["statuses"][0]
