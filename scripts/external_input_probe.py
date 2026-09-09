"""Actual native common-ingress scenarios; see the private fixture support module."""

import hashlib
import json
import threading

from external_input_probe_support import (
    BINARY,
    MODES,
    REQUESTS,
    RUN,
    SERVER,
    Owner,
    envelope,
    status,
)


def standard():
    owner = None
    try:
        owner = Owner("a")
        assert owner.capability.get("externalInputVersion") is None
        thread = owner.call(
            "thread/start",
            {
                "cwd": str(RUN),
                "ephemeral": False,
                "historyMode": "paginated",
                "sandbox": "read-only",
                "approvalPolicy": "on-request",
            },
        )["thread"]["id"]
        (RUN / "thread-id").write_text(thread)
        pristine = owner.call("thread/read", {"threadId": thread, "includeTurns": True})
        assert pristine["thread"]["turns"] == []
        owner.close()
        binding = RUN / "binding.json"
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
        owner = Owner("b", binding)
        assert owner.capability["externalInputVersion"] == 1
        value = envelope(thread, 7)
        owner.call(
            "thread/externalInput/submit", value, error=True
        )  # no implicit resume
        owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
        submitted = owner.call("thread/externalInput/submit", value)
        request = REQUESTS.get(timeout=45)
        (RUN / "request.json").write_text(json.dumps(request, indent=2))
        owner.event("turn/completed")
        final = status(owner, value)
        assert final["deliveryState"] == "context_persisted", final
        assert final["processing"]["state"] == "output_observed", final
        model_wire = json.dumps(request)
        for mechanical in [
            "semanticSha256",
            "receiptId",
            "runtimeGeneration",
            "private-owner",
            "thread/externalInput/",
        ]:
            assert mechanical not in model_wire, mechanical
        canonical = [
            item for item in request["input"] if item.get("name") == "external_event"
        ]
        assert len(canonical) == 1, canonical
        assert json.loads(canonical[0]["output"]) == {
            "source": value["message"]["source"],
            "type": value["message"]["type"],
            "text": value["message"]["text"],
        }
        assert owner.call("thread/externalInput/submit", value)["statuses"][0] == final
        conflict = envelope(thread, 7, text="changed")
        assert (
            owner.call("thread/externalInput/submit", conflict)["statuses"][0][
                "deliveryState"
            ]
            == "conflict"
        )
        for key, wrong in [
            ("runtimeGeneration", 8),
            ("ownerId", "wrong"),
            ("version", 2),
        ]:
            bad = dict(value, **{key: wrong})
            owner.call("thread/externalInput/submit", bad, error=True)
        # Idle passive remains volatile and does not reserve a turn. A genuine
        # subsequent user turn consumes it at a normal safe input boundary.
        passive = envelope(thread, 7, ident="passive-1", delivery="passive")
        assert (
            owner.call("thread/externalInput/submit", passive)["statuses"][0][
                "deliveryState"
            ]
            == "pending"
        )
        assert status(owner, passive)["receipt"] is None
        assert REQUESTS.empty()
        owner.call(
            "turn/start",
            {
                "threadId": thread,
                "input": [{"type": "text", "text": "Private fixture user turn."}],
            },
        )
        passive_request = REQUESTS.get(timeout=45)
        owner.event("turn/completed")
        assert status(owner, passive)["deliveryState"] == "context_persisted"
        assert status(owner, passive)["processing"]["state"] == "none"
        assert any(
            item.get("name") == "external_event"
            and json.loads(item["output"])["text"] == passive["message"]["text"]
            for item in passive_request["input"]
        )

        # A pending after_turn admission cannot appear in the already-running
        # request. Release that request explicitly, then observe the next request.
        gate = threading.Event()
        MODES.put(("output", gate))
        owner.call(
            "turn/start",
            {
                "threadId": thread,
                "input": [{"type": "text", "text": "Controlled current turn."}],
            },
        )
        current_request = REQUESTS.get(timeout=45)
        owner.call(
            "thread/queue/add",
            {
                "threadId": thread,
                "clientUserMessageId": "private-queued-user",
                "input": [
                    {"type": "text", "text": "Queued genuine user input has priority."}
                ],
            },
        )
        future = envelope(
            thread, 7, ident="future-1", text="Only a subsequent turn may consume this."
        )
        assert (
            owner.call("thread/externalInput/submit", future)["statuses"][0][
                "deliveryState"
            ]
            == "pending"
        )
        assert status(owner, future)["receipt"] is None
        assert not any(
            "Only a subsequent turn" in json.dumps(item)
            for item in current_request["input"]
        )
        gate.set()
        owner.event("turn/completed")
        future_request = REQUESTS.get(timeout=45)
        assert any(
            "Queued genuine user input has priority." in json.dumps(item)
            for item in future_request["input"]
        )
        owner.event("turn/completed")
        assert status(owner, future)["processing"]["state"] == "output_observed"
        assert any(
            "Only a subsequent turn" in json.dumps(item)
            for item in future_request["input"]
        )

        # An actual native approval turn is controlled by a denial, never approval.
        # Passive data joins its next safe boundary; after_turn remains excluded.
        MODES.put(("approval", None))
        owner.call(
            "turn/start",
            {
                "threadId": thread,
                "input": [{"type": "text", "text": "Private approval fixture turn."}],
            },
        )
        REQUESTS.get(timeout=45)
        approval = owner.event("item/commandExecution/requestApproval")
        tool_passive = envelope(
            thread,
            7,
            ident="tool-passive",
            delivery="passive",
            text="Passive data at the next tool boundary.",
        )
        tool_after = envelope(
            thread, 7, ident="tool-after", text="After this approval turn completes."
        )
        owner.call("thread/externalInput/submit", tool_passive)
        owner.call("thread/externalInput/submit", tool_after)
        assert status(owner, tool_passive)["receipt"] is None
        assert status(owner, tool_after)["receipt"] is None
        owner.send({"id": approval["id"], "result": {"decision": "decline"}})
        tool_followup = REQUESTS.get(timeout=45)
        assert any(
            tool_passive["message"]["text"] in json.dumps(item)
            for item in tool_followup["input"]
        )
        assert not any(
            tool_after["message"]["text"] in json.dumps(item)
            for item in tool_followup["input"]
        )
        owner.event("turn/completed")
        tool_next = REQUESTS.get(timeout=45)
        owner.event("turn/completed")
        assert any(
            tool_after["message"]["text"] in json.dumps(item)
            for item in tool_next["input"]
        )
        assert status(owner, tool_passive)["processing"]["state"] == "none"
        assert status(owner, tool_after)["processing"]["state"] == "output_observed"
        (RUN / "approval-case.json").write_text(
            json.dumps(
                {
                    "request": approval,
                    "decision": "decline",
                    "same_turn_request": tool_followup,
                    "next_turn_request": tool_next,
                },
                indent=2,
            )
        )

        held_cases = []
        for mode, reason in [
            ("empty", "no_output"),
            ("disconnect", "request_uncertain"),
        ]:
            MODES.put((mode, None))
            held_value = envelope(thread, 7, ident=mode + "-1", text="Held " + mode)
            owner.call("thread/externalInput/submit", held_value)
            REQUESTS.get(timeout=45)
            owner.event("turn/completed")
            held = status(owner, held_value)
            assert held["processing"]["state"] == "held", held
            assert held["processing"]["reason"] == reason, held
            assert (
                owner.call("thread/externalInput/submit", held_value)["statuses"][0]
                == held
            )
            assert REQUESTS.empty(), "held obligation retried automatically"
            if mode == "empty":
                owner.call("thread/compact/start", {"threadId": thread})
                compact_request = REQUESTS.get(timeout=45)
                owner.event("turn/completed")
                assert status(owner, held_value) == held, (
                    "compaction released held work"
                )
                (RUN / "compaction-request.json").write_text(
                    json.dumps(compact_request, indent=2)
                )
            retry = {
                key: held_value[key]
                for key in ["version", "ownerId", "threadId", "runtimeGeneration"]
            }
            retry.update(
                messageId=held_value["message"]["id"],
                semanticSha256=held_value["semanticSha256"],
                expectedAttemptId=held["processing"]["attemptId"],
                retryId="retry-" + mode,
            )
            if mode == "empty":
                MODES.put(("approval", None))
                owner.call(
                    "turn/start",
                    {
                        "threadId": thread,
                        "input": [
                            {
                                "type": "text",
                                "text": "Current approval turn must not consume a later retry.",
                            }
                        ],
                    },
                )
                current = REQUESTS.get(timeout=45)
                assert not any(
                    item.get("name") == "external_event"
                    and json.loads(item["output"])["text"]
                    == held_value["message"]["text"]
                    for item in current["input"]
                )
                approval = owner.event("item/commandExecution/requestApproval")
            released = owner.call("thread/externalInput/retry", retry)
            if mode == "empty":
                owner.send({"id": approval["id"], "result": {"decision": "decline"}})
                current_followup = REQUESTS.get(timeout=45)
                assert not any(
                    item.get("name") == "external_event"
                    and json.loads(item["output"])["text"]
                    == held_value["message"]["text"]
                    for item in current_followup["input"]
                )
                owner.event("turn/completed")
            retry_request = REQUESTS.get(timeout=45)
            assert any(
                item.get("name") == "external_event"
                and json.loads(item["output"])["text"] == held_value["message"]["text"]
                for item in retry_request["input"]
            )
            (RUN / ("retry-" + mode + "-request.json")).write_text(
                json.dumps(retry_request, indent=2)
            )
            owner.event("turn/completed")
            completed = status(owner, held_value)
            assert completed["receipt"] == held["receipt"]
            assert completed["processing"]["state"] == "output_observed"
            assert owner.call("thread/externalInput/retry", retry) == released
            owner.call(
                "thread/externalInput/retry",
                dict(retry, expectedAttemptId=None),
                error=True,
            )
            assert REQUESTS.empty(), "retry replay produced another request"
            held_cases.append({"mode": mode, "held": held, "completed": completed})
        (RUN / "held-cases.json").write_text(json.dumps(held_cases, indent=2))
        # Model completion is observable before its tool approval is resolved.
        observed = envelope(
            thread,
            7,
            ident="observed-before-tool-drain",
            text="Observe the completed response independently of tool approval.",
        )
        MODES.put(("approval", None))
        owner.call("thread/externalInput/submit", observed)
        REQUESTS.get(timeout=45)
        owner.event("item/commandExecution/requestApproval")
        hints = 0
        while hints < 3:
            hint = owner.event("thread/externalInput/statusChanged")
            hints += hint["params"]["messageId"] == observed["message"]["id"]
        completed_response = status(owner, observed)
        assert completed_response["processing"]["state"] == "output_observed"
        owner.call(
            "turn/interrupt",
            {"threadId": thread, "turnId": completed_response["receipt"]["turnId"]},
        )
        owner.event("turn/completed")
        assert status(owner, observed) == completed_response
        owner.close(abrupt=True)
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
        owner = Owner("c", binding)
        owner.call("thread/resume", {"threadId": thread, "historyMode": "paginated"})
        value["runtimeGeneration"] = 8
        assert status(owner, value) == final
        assert REQUESTS.empty(), "duplicate/recovery produced another request"
        (RUN / "PASS.json").write_text(
            json.dumps(
                {
                    "binary_sha256": hashlib.file_digest(
                        BINARY.open("rb"), "sha256"
                    ).hexdigest(),
                    "thread_id": thread,
                    "receipt": final["receipt"],
                    "actual_native": True,
                    "responses": "fake isolated loopback",
                },
                indent=2,
            )
        )
    finally:
        if owner:
            owner.close()
        SERVER.shutdown()


if __name__ == "__main__":
    standard()
