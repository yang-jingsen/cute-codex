use std::ffi::OsString;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_tools::ToolExecutor;
use codex_tools::ToolSpec;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::TaskServiceHandler;
use crate::tools::handlers::task_service_spec::create_task_service_tool;

const ASSIGNMENT_ID: &str = "assignment-183";
const ACTION_ID: &str = "action-183-start";
const PROJECT_ID: &str = "cute-codex-task-project-r1";
const ROUTE_TOKEN: &str = "private-route-token";
const ATTEMPT_TOKEN_1: &str = "provider-private-attempt-token-1";
const ATTEMPT_TOKEN_2: &str = "provider-private-attempt-token-2";
const RUNTIME_ID: &str = "runtime-occurrence-2";
const RESTARTED_RUNTIME_ID: &str = "runtime-occurrence-3";

fn handler(server: &MockServer) -> TaskServiceHandler {
    handler_with_runtime(server, RUNTIME_ID)
}

fn handler_with_runtime(server: &MockServer, runtime_id: &str) -> TaskServiceHandler {
    TaskServiceHandler::new(
        Some(OsString::from(server.uri())),
        Some(OsString::from(ROUTE_TOKEN)),
        Some(OsString::from(runtime_id)),
        Duration::from_millis(100),
    )
}

fn start_args() -> String {
    action_args("start", ACTION_ID, None)
}

fn action_args(operation: &str, action_id: &str, summary: Option<&str>) -> String {
    let mut value = json!({
        "operation": operation,
        "assignment_id": ASSIGNMENT_ID,
        "action_id": action_id,
    });
    if let Some(summary) = summary {
        value["summary"] = json!(summary);
    }
    value.to_string()
}

fn semantic_action(operation: &str, action_id: &str, summary: Option<&str>) -> Value {
    let mut body = json!({
        "schema": "cutex/task-service-action/v2",
        "action_id": action_id,
        "assignment_id": ASSIGNMENT_ID,
    });
    if let Some(summary) = summary {
        body["summary"] = json!(summary);
    }
    json!({ "operation": operation, "body": body })
}

fn provider_envelope(
    action: Value,
    assignment_revision: u64,
    attempt: Option<(u64, &str, u64)>,
) -> Value {
    let attempt = attempt.map(|(number, token, revision)| {
        json!({
            "attempt_number": number,
            "attempt_token": token,
            "expected_attempt_revision": revision,
        })
    });
    json!({
        "schema": "cutex/task-service-worker-provider/v2",
        "action": action,
        "context": {
            "expected_assignment_revision": assignment_revision,
            "attempt": attempt,
        }
    })
}

fn prepared_response(envelope: Value) -> Value {
    json!({
        "schema": "cutex/task-service-worker-prepare-response/v2",
        "outcome": { "kind": "prepared", "body": envelope }
    })
}

fn committed_prepare(receipt: Value) -> Value {
    json!({
        "schema": "cutex/task-service-worker-prepare-response/v2",
        "outcome": { "kind": "committed", "body": receipt }
    })
}

fn prepare_no_write(code: &str, detail: &str) -> Value {
    json!({
        "schema": "cutex/task-service-worker-prepare-response/v2",
        "outcome": {
            "kind": "no_write",
            "body": { "code": code, "detail": detail }
        }
    })
}

fn provider_receipt(action_id: &str, phase: &str) -> Value {
    json!({
        "schema": "cutex/task-service-receipt/v2",
        "action_id": action_id,
        "request_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "attempt_binding": {
            "attempt_number": 1,
            "attempt_token": ATTEMPT_TOKEN_1
        },
        "committed_at": "2026-08-26T00:00:00Z",
        "journal_sequence": 41,
        "result": {
            "kind": "attempt",
            "body": {
                "assignment_id": ASSIGNMENT_ID,
                "attempt_number": 1,
                "attempt_token": ATTEMPT_TOKEN_1,
                "phase": phase,
                "local_revision": 1,
                "started_at": "2026-08-26T00:00:00Z",
                "updated_at": "2026-08-26T00:00:00Z",
                "status_receipts": [],
                "result_receipts": [],
                "terminal_action_id": null
            }
        }
    })
}

fn provider_v3_attempt_receipt(action_id: &str, phase: &str) -> Value {
    let mut receipt = provider_receipt(action_id, phase);
    receipt["schema"] = json!("cutex/task-service-receipt/v3");
    receipt["result"]["body"]["project_id"] = json!(PROJECT_ID);
    receipt
}

fn provider_v3_assignment_receipt(action_id: &str) -> Value {
    json!({
        "schema": "cutex/task-service-receipt/v3",
        "action_id": action_id,
        "request_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "committed_at": "2026-08-26T00:00:00Z",
        "journal_sequence": 42,
        "result": {
            "kind": "assignment",
            "body": {
                "assignment": {
                    "project_id": PROJECT_ID,
                    "assignment_id": ASSIGNMENT_ID,
                    "state": "closed",
                    "closure": { "reason": "declined" }
                },
                "send_attempt": null
            }
        }
    })
}

fn committed_action_with_receipt(action_id: &str, receipt: Value) -> Value {
    json!({
        "schema": "cutex/task-service-action-response/v2",
        "action_id": action_id,
        "outcome": {
            "kind": "committed",
            "body": receipt
        }
    })
}

fn committed_action(action_id: &str, phase: &str) -> Value {
    committed_action_with_receipt(action_id, provider_receipt(action_id, phase))
}

fn mechanical_conflict(action_id: &str, kind: &str) -> Value {
    json!({
        "schema": "cutex/task-service-action-response/v2",
        "action_id": action_id,
        "outcome": {
            "kind": "no_write",
            "body": {
                "code": "conflict",
                "detail": format!("task service provider error: Conflict(\"{kind}\")")
            }
        }
    })
}

fn parse_output(output: &str) -> Value {
    serde_json::from_str(output).expect("model receipt JSON")
}

fn expected_attempt_receipt(action_id: &str, phase: &str) -> Value {
    json!({
        "schema": "cutex/task-service-tool-receipt/v1",
        "status": "committed",
        "action_id": action_id,
        "assignment_id": ASSIGNMENT_ID,
        "attempt_number": 1,
        "attempt_phase": phase
    })
}

fn sanitize_receipt(arguments: &str, receipt: Value) -> Value {
    let args = serde_json::from_str(arguments).expect("valid task service arguments");
    parse_output(&super::serialize_model_receipt(
        super::sanitize_provider_receipt(&args, receipt),
    ))
}

async fn invoke_immediate_v3(
    arguments: &str,
    attempt: Option<(u64, &str, u64)>,
    receipt: Value,
) -> Value {
    let server = MockServer::start().await;
    let args = serde_json::from_str(arguments).expect("valid task service arguments");
    let action = super::provider_request(&args).expect("valid provider action");
    let action_id = action["body"]["action_id"]
        .as_str()
        .expect("action ID")
        .to_string();
    mount_prepare(
        &server,
        prepared_response(provider_envelope(action, 1, attempt)),
        1,
    )
    .await;
    mount_action(
        &server,
        committed_action_with_receipt(&action_id, receipt),
        1,
    )
    .await;
    parse_output(&handler(&server).invoke_arguments(arguments).await)
}

async fn mount_prepare(server: &MockServer, response: Value, expected: u64) {
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(expected)
        .mount(server)
        .await;
}

async fn mount_action(server: &MockServer, response: Value, expected: u64) {
    Mock::given(method("POST"))
        .and(path("/api/task/v2/actions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(expected)
        .mount(server)
        .await;
}

#[test]
fn schema_exposes_only_stable_worker_fields() {
    let ToolSpec::Function(tool) = create_task_service_tool() else {
        panic!("expected function tool");
    };
    let properties = tool.parameters.properties.expect("tool properties");
    assert_eq!(
        properties.keys().cloned().collect::<Vec<_>>(),
        vec![
            "action_id",
            "assignment_id",
            "evidence_sha256",
            "operation",
            "result_reference",
            "result_sha256",
            "summary",
        ]
    );
    let operations = properties
        .get("operation")
        .and_then(|schema| schema.enum_values.clone())
        .expect("operation enum");
    assert_eq!(
        operations,
        vec![
            json!("start"),
            json!("report_status"),
            json!("block"),
            json!("resume"),
            json!("submit"),
            json!("decline"),
            json!("abort_attempt"),
        ]
    );
}

#[test]
fn production_tool_spec_serialization_identity_is_stable() {
    let serialized =
        serde_json::to_string(&create_task_service_tool()).expect("serialize production ToolSpec");

    assert_eq!(serialized.len(), 1_282);
    assert_eq!(
        format!("{:x}", Sha256::digest(serialized.as_bytes())),
        "b821d43a72ec43ad043e3f897e40189367964931dee69a71d243757f8f6653bd"
    );
}

#[tokio::test]
async fn missing_or_forged_integration_is_a_typed_no_write() {
    let missing = TaskServiceHandler::new(None, None, None, Duration::from_millis(10));
    let output = parse_output(&missing.invoke_arguments(&start_args()).await);
    assert_eq!(output["status"], "no_write");
    assert_eq!(output["code"], "missing_authenticated_integration");

    let forged = json!({
        "operation": "start",
        "assignment_id": ASSIGNMENT_ID,
        "action_id": ACTION_ID,
        "runtime_id": "prompt-claimed-runtime",
        "attempt_token": "prompt-claimed-token",
        "cas_revision": 9,
    });
    let output = parse_output(&missing.invoke_arguments(&forged.to_string()).await);
    assert_eq!(output["code"], "invalid_arguments");

    let non_loopback = TaskServiceHandler::new(
        Some(OsString::from("http://example.com")),
        Some(OsString::from(ROUTE_TOKEN)),
        Some(OsString::from(RUNTIME_ID)),
        Duration::from_millis(10),
    );
    let output = parse_output(&non_loopback.invoke_arguments(&start_args()).await);
    assert_eq!(output["code"], "insecure_integration");
}

#[tokio::test]
async fn authenticated_prepare_returns_an_exact_secret_free_provider_envelope() {
    let server = MockServer::start().await;
    let action = semantic_action("start", ACTION_ID, None);
    mount_prepare(
        &server,
        prepared_response(provider_envelope(action.clone(), 7, None)),
        1,
    )
    .await;
    mount_action(&server, committed_action(ACTION_ID, "running"), 1).await;

    let output = handler(&server).invoke_arguments(&start_args()).await;
    assert_eq!(
        parse_output(&output),
        json!({
            "schema": "cutex/task-service-tool-receipt/v1",
            "status": "committed",
            "action_id": ACTION_ID,
            "assignment_id": ASSIGNMENT_ID,
            "attempt_number": 1,
            "attempt_phase": "running"
        })
    );
    assert!(output.len() < 512);
    for private in [
        ROUTE_TOKEN,
        RUNTIME_ID,
        ATTEMPT_TOKEN_1,
        "journal_sequence",
        "local_revision",
        "expected_assignment_revision",
    ] {
        assert!(!output.contains(private));
    }

    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(
            request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer private-route-token")
        );
        assert_eq!(
            request
                .headers
                .get("x-cutex-agent-id")
                .and_then(|value| value.to_str().ok()),
            Some(RUNTIME_ID)
        );
    }
    let prepare = requests
        .iter()
        .find(|request| request.url.path().ends_with("worker-prepare"))
        .expect("prepare request");
    assert_eq!(
        serde_json::from_slice::<Value>(&prepare.body).expect("prepare JSON"),
        json!({
            "schema": "cutex/task-service-worker-prepare/v2",
            "action": action,
        })
    );
    let execution = requests
        .iter()
        .find(|request| request.url.path().ends_with("actions"))
        .expect("execution request");
    assert_eq!(
        serde_json::from_slice::<Value>(&execution.body).expect("envelope JSON"),
        provider_envelope(semantic_action("start", ACTION_ID, None), 7, None)
    );
}

#[test]
fn v2_provider_receipt_behavior_is_unchanged() {
    assert_eq!(
        sanitize_receipt(&start_args(), provider_receipt(ACTION_ID, "running")),
        expected_attempt_receipt(ACTION_ID, "running")
    );
}

#[tokio::test]
async fn valid_v3_receipts_cover_start_status_submit_and_terminal_actions() {
    let start = invoke_immediate_v3(
        &start_args(),
        None,
        provider_v3_attempt_receipt(ACTION_ID, "running"),
    )
    .await;
    assert_eq!(start, expected_attempt_receipt(ACTION_ID, "running"));

    let status_action_id = "action-183-status-v3";
    let status_args = action_args(
        "report_status",
        status_action_id,
        Some("v3 status remains current"),
    );
    let status = invoke_immediate_v3(
        &status_args,
        Some((1, ATTEMPT_TOKEN_1, 2)),
        provider_v3_attempt_receipt(status_action_id, "running"),
    )
    .await;
    assert_eq!(
        status,
        expected_attempt_receipt(status_action_id, "running")
    );

    let submit_action_id = "action-183-submit-v3";
    let submit_args = json!({
        "operation": "submit",
        "assignment_id": ASSIGNMENT_ID,
        "action_id": submit_action_id,
        "result_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "result_reference": "artifact://v3-result"
    })
    .to_string();
    let submitted = invoke_immediate_v3(
        &submit_args,
        Some((1, ATTEMPT_TOKEN_1, 3)),
        provider_v3_attempt_receipt(submit_action_id, "review_ready"),
    )
    .await;
    assert_eq!(
        submitted,
        expected_attempt_receipt(submit_action_id, "review_ready")
    );

    let terminal_action_id = "action-183-decline-v3";
    let terminal_args = action_args("decline", terminal_action_id, None);
    let terminal = invoke_immediate_v3(
        &terminal_args,
        None,
        provider_v3_assignment_receipt(terminal_action_id),
    )
    .await;
    assert_eq!(
        terminal,
        json!({
            "schema": "cutex/task-service-tool-receipt/v1",
            "status": "committed",
            "action_id": terminal_action_id,
            "assignment_id": ASSIGNMENT_ID,
            "assignment_state": "closed",
            "closure_reason": "declined"
        })
    );
}

#[tokio::test]
async fn valid_v3_prepare_exact_replay_returns_the_current_attempt() {
    let server = MockServer::start().await;
    let action_id = "action-183-status-v3-replay";
    let arguments = action_args(
        "report_status",
        action_id,
        Some("v3 exact replay remains current"),
    );
    mount_prepare(
        &server,
        committed_prepare(provider_v3_attempt_receipt(action_id, "running")),
        1,
    )
    .await;
    mount_action(&server, committed_action(action_id, "running"), 0).await;

    assert_eq!(
        parse_output(&handler(&server).invoke_arguments(&arguments).await),
        expected_attempt_receipt(action_id, "running")
    );
}

#[tokio::test]
async fn closed_assignment_exact_replay_stays_closed_without_attempt_context() {
    let server = MockServer::start().await;
    let action_id = "action-183-closed-replay";
    let arguments = action_args("decline", action_id, None);
    let action = semantic_action("decline", action_id, None);
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                prepared_response(provider_envelope(action, 1, None)),
                committed_prepare(provider_v3_assignment_receipt(action_id)),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;
    mount_action(
        &server,
        committed_action_with_receipt(action_id, provider_v3_assignment_receipt(action_id)),
        1,
    )
    .await;

    let first = parse_output(&handler(&server).invoke_arguments(&arguments).await);
    assert_eq!(first["assignment_state"], "closed");
    assert_eq!(first["closure_reason"], "declined");

    let replay = parse_output(
        &handler_with_runtime(&server, RESTARTED_RUNTIME_ID)
            .invoke_arguments(&arguments)
            .await,
    );
    assert_eq!(replay, first);
    assert!(replay.get("attempt_number").is_none());
    assert!(replay.get("attempt_phase").is_none());

    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().ends_with("actions"))
            .count(),
        1,
        "exact replay of a closed assignment must not execute a second action"
    );
}

#[test]
fn v3_project_scope_is_required_bounded_and_consistent() {
    let invalid = json!({
        "schema": "cutex/task-service-tool-receipt/v1",
        "status": "no_write",
        "action_id": ACTION_ID,
        "assignment_id": ASSIGNMENT_ID,
        "code": "invalid_provider_response"
    });

    let mut missing = provider_v3_attempt_receipt(ACTION_ID, "running");
    missing["result"]["body"]
        .as_object_mut()
        .expect("attempt body")
        .remove("project_id");
    assert_eq!(sanitize_receipt(&start_args(), missing), invalid);

    for malformed in [
        Value::Null,
        json!(""),
        json!("project with spaces"),
        json!("x".repeat(257)),
    ] {
        let mut receipt = provider_v3_attempt_receipt(ACTION_ID, "running");
        receipt["result"]["body"]["project_id"] = malformed;
        assert_eq!(sanitize_receipt(&start_args(), receipt), invalid);
    }

    let mut inconsistent_root = provider_v3_attempt_receipt(ACTION_ID, "running");
    inconsistent_root["project_id"] = json!("different-project");
    assert_eq!(sanitize_receipt(&start_args(), inconsistent_root), invalid);

    let mut inconsistent_nested = provider_v3_attempt_receipt(ACTION_ID, "running");
    inconsistent_nested["result"]["body"]["status_receipts"] = json!([{
        "project_id": "different-project"
    }]);
    assert_eq!(
        sanitize_receipt(&start_args(), inconsistent_nested),
        invalid
    );

    let terminal_action_id = "action-183-decline-v3-invalid";
    let terminal_args = action_args("decline", terminal_action_id, None);
    let terminal_invalid = json!({
        "schema": "cutex/task-service-tool-receipt/v1",
        "status": "no_write",
        "action_id": terminal_action_id,
        "assignment_id": ASSIGNMENT_ID,
        "code": "invalid_provider_response"
    });
    let mut missing_assignment_project = provider_v3_assignment_receipt(terminal_action_id);
    missing_assignment_project["result"]["body"]["assignment"]
        .as_object_mut()
        .expect("assignment body")
        .remove("project_id");
    assert_eq!(
        sanitize_receipt(&terminal_args, missing_assignment_project),
        terminal_invalid
    );

    let mut inconsistent_send_attempt = provider_v3_assignment_receipt(terminal_action_id);
    inconsistent_send_attempt["result"]["body"]["send_attempt"] = json!({
        "project_id": "different-project"
    });
    assert_eq!(
        sanitize_receipt(&terminal_args, inconsistent_send_attempt),
        terminal_invalid
    );
}

struct JsonSequence {
    calls: AtomicUsize,
    responses: Vec<Value>,
}

impl Respond for JsonSequence {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200).set_body_json(self.responses[index].clone())
    }
}

struct TimeoutAlways;

impl Respond for TimeoutAlways {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_json(committed_action(ACTION_ID, "running"))
            .set_delay(Duration::from_millis(250))
    }
}

#[tokio::test]
async fn response_uncertainty_probes_the_durable_committed_receipt() {
    let server = MockServer::start().await;
    let action = semantic_action("start", ACTION_ID, None);
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                prepared_response(provider_envelope(action, 1, None)),
                committed_prepare(provider_receipt(ACTION_ID, "running")),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/actions"))
        .respond_with(TimeoutAlways)
        .expect(2)
        .mount(&server)
        .await;

    let output = handler(&server).invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&output)["status"], "committed");
    let requests = server.received_requests().await.expect("received requests");
    let action_requests = requests
        .iter()
        .filter(|request| request.url.path().ends_with("actions"))
        .collect::<Vec<_>>();
    assert_eq!(action_requests.len(), 2);
    assert_eq!(action_requests[0].body, action_requests[1].body);
}

struct DelayedDurableProbe {
    calls: AtomicUsize,
    first: Value,
    final_response: Value,
}

impl Respond for DelayedDurableProbe {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        match self.calls.fetch_add(1, Ordering::SeqCst) {
            0 => ResponseTemplate::new(200).set_body_json(self.first.clone()),
            1 | 2 => ResponseTemplate::new(200)
                .set_body_json(self.final_response.clone())
                .set_delay(Duration::from_millis(250)),
            _ => ResponseTemplate::new(200).set_body_json(self.final_response.clone()),
        }
    }
}

#[tokio::test]
async fn a_fresh_runtime_recovers_a_start_committed_after_uncertainty() {
    let server = MockServer::start().await;
    let action = semantic_action("start", ACTION_ID, None);
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(DelayedDurableProbe {
            calls: AtomicUsize::new(0),
            first: prepared_response(provider_envelope(action, 1, None)),
            final_response: committed_prepare(provider_receipt(ACTION_ID, "running")),
        })
        .expect(4)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/actions"))
        .respond_with(TimeoutAlways)
        .expect(2)
        .mount(&server)
        .await;

    let first = handler(&server).invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&first)["status"], "response_uncertain");
    let restarted = handler_with_runtime(&server, RESTARTED_RUNTIME_ID);
    let recovered = restarted.invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&recovered)["status"], "committed");
    let requests = server.received_requests().await.expect("received requests");
    assert!(requests.iter().any(|request| {
        request
            .headers
            .get("x-cutex-agent-id")
            .and_then(|value| value.to_str().ok())
            == Some(RESTARTED_RUNTIME_ID)
    }));
}

struct DynamicCommittedPrepare {
    calls: AtomicUsize,
}

impl Respond for DynamicCommittedPrepare {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let request: Value = serde_json::from_slice(&request.body).expect("prepare JSON");
        let action = request["action"].clone();
        let action_id = action["body"]["action_id"].as_str().expect("action ID");
        let response = if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            prepared_response(provider_envelope(action, 1, None))
        } else {
            committed_prepare(provider_receipt(action_id, "running"))
        };
        ResponseTemplate::new(200).set_body_json(response)
    }
}

#[tokio::test]
async fn committed_start_replay_survives_more_than_thirty_two_other_actions() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(DynamicCommittedPrepare {
            calls: AtomicUsize::new(0),
        })
        .expect(35)
        .mount(&server)
        .await;
    mount_action(&server, committed_action(ACTION_ID, "running"), 1).await;

    let handler = handler(&server);
    assert_eq!(
        parse_output(&handler.invoke_arguments(&start_args()).await)["status"],
        "committed"
    );
    for index in 0..33 {
        let action_id = format!("action-183-unrelated-{index}");
        let output = handler
            .invoke_arguments(&action_args("start", &action_id, None))
            .await;
        assert_eq!(parse_output(&output)["status"], "committed");
    }
    let restarted = handler_with_runtime(&server, RESTARTED_RUNTIME_ID);
    let replay = restarted.invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&replay)["status"], "committed");
}

#[tokio::test]
async fn a_fresh_runtime_recovers_a_committed_attempt_action() {
    let server = MockServer::start().await;
    let action_id = "action-183-status-replay";
    let action = semantic_action("report_status", action_id, Some("still working"));
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                prepared_response(provider_envelope(action, 2, Some((1, ATTEMPT_TOKEN_1, 3)))),
                committed_prepare(provider_receipt(action_id, "running")),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;
    mount_action(&server, committed_action(action_id, "running"), 1).await;

    let first = handler(&server)
        .invoke_arguments(&action_args(
            "report_status",
            action_id,
            Some("still working"),
        ))
        .await;
    let restarted = handler_with_runtime(&server, RESTARTED_RUNTIME_ID);
    let replay = restarted
        .invoke_arguments(&action_args(
            "report_status",
            action_id,
            Some("still working"),
        ))
        .await;
    assert_eq!(first, replay);
}

#[tokio::test]
async fn same_attempt_cas_conflict_uses_only_the_refreshed_provider_envelope() {
    let server = MockServer::start().await;
    let action_id = "action-183-status-refresh";
    let action = semantic_action("report_status", action_id, Some("still working"));
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                prepared_response(provider_envelope(
                    action.clone(),
                    2,
                    Some((1, ATTEMPT_TOKEN_1, 3)),
                )),
                prepared_response(provider_envelope(action, 3, Some((1, ATTEMPT_TOKEN_1, 4)))),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/actions"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                mechanical_conflict(action_id, "attempt_revision_conflict"),
                committed_action(action_id, "running"),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;

    let output = handler(&server)
        .invoke_arguments(&action_args(
            "report_status",
            action_id,
            Some("still working"),
        ))
        .await;
    assert_eq!(parse_output(&output)["status"], "committed");
    let requests = server.received_requests().await.expect("received requests");
    let envelopes = requests
        .iter()
        .filter(|request| request.url.path().ends_with("actions"))
        .map(|request| serde_json::from_slice::<Value>(&request.body).expect("envelope JSON"))
        .collect::<Vec<_>>();
    assert_eq!(envelopes[0]["action"], envelopes[1]["action"]);
    assert_eq!(
        envelopes[0]["context"]["attempt"]["attempt_token"],
        envelopes[1]["context"]["attempt"]["attempt_token"]
    );
    assert_ne!(envelopes[0]["context"], envelopes[1]["context"]);
}

#[tokio::test]
async fn a_fresh_runtime_never_retargets_attempt_one_preparation_to_attempt_two() {
    let server = MockServer::start().await;
    let action_id = "action-183-stale-attempt";
    let action = semantic_action("block", action_id, None);
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(DelayedDurableProbe {
            calls: AtomicUsize::new(0),
            first: prepared_response(provider_envelope(action, 2, Some((1, ATTEMPT_TOKEN_1, 3)))),
            final_response: prepare_no_write(
                "conflict",
                "task service provider error: Conflict(\"attempt_handle_conflict\")",
            ),
        })
        .expect(4)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/actions"))
        .respond_with(TimeoutAlways)
        .expect(2)
        .mount(&server)
        .await;

    let first = handler(&server)
        .invoke_arguments(&action_args("block", action_id, None))
        .await;
    assert_eq!(parse_output(&first)["status"], "response_uncertain");
    let restarted = handler_with_runtime(&server, RESTARTED_RUNTIME_ID);
    let refused = restarted
        .invoke_arguments(&action_args("block", action_id, None))
        .await;
    let refused = parse_output(&refused);
    assert_eq!(refused["status"], "conflict");
    assert_eq!(refused["code"], "conflict");
    let refused = refused.to_string();
    assert!(!refused.contains(ATTEMPT_TOKEN_1));
    assert!(!refused.contains(ATTEMPT_TOKEN_2));
}

#[tokio::test]
async fn a_new_runtime_prepares_new_actions_on_the_same_attempt() {
    let server = MockServer::start().await;
    let block = semantic_action("block", "action-183-block", None);
    let resume = semantic_action("resume", "action-183-resume", None);
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                prepared_response(provider_envelope(block, 2, Some((1, ATTEMPT_TOKEN_1, 3)))),
                prepared_response(provider_envelope(resume, 3, Some((1, ATTEMPT_TOKEN_1, 4)))),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/actions"))
        .respond_with(JsonSequence {
            calls: AtomicUsize::new(0),
            responses: vec![
                committed_action("action-183-block", "blocked"),
                committed_action("action-183-resume", "running"),
            ],
        })
        .expect(2)
        .mount(&server)
        .await;

    let first = handler(&server)
        .invoke_arguments(&action_args("block", "action-183-block", None))
        .await;
    let restarted = handler_with_runtime(&server, RESTARTED_RUNTIME_ID);
    let second = restarted
        .invoke_arguments(&action_args("resume", "action-183-resume", None))
        .await;
    assert_eq!(parse_output(&first)["attempt_number"], 1);
    assert_eq!(parse_output(&second)["attempt_number"], 1);
}

#[tokio::test]
async fn unauthorized_malformed_oversized_and_redirected_prepare_fail_closed() {
    let unauthorized = MockServer::start().await;
    mount_prepare(
        &unauthorized,
        prepare_no_write("unauthorized", "secret durable session detail"),
        1,
    )
    .await;
    mount_action(&unauthorized, committed_action(ACTION_ID, "running"), 0).await;
    let output = handler(&unauthorized).invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&output)["code"], "unauthorized");
    assert!(!output.contains("durable session"));

    let malformed = MockServer::start().await;
    let wrong_action = semantic_action("start", "another-action", None);
    mount_prepare(
        &malformed,
        prepared_response(provider_envelope(wrong_action, 1, None)),
        1,
    )
    .await;
    mount_action(&malformed, committed_action(ACTION_ID, "running"), 0).await;
    let output = handler(&malformed).invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&output)["code"], "invalid_provider_response");

    let oversized = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 1024 * 1024 + 1]))
        .expect(1)
        .mount(&oversized)
        .await;
    let output = handler(&oversized).invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&output)["code"], "invalid_provider_response");

    let redirected = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/worker-prepare"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://example.com/forged"),
        )
        .expect(1)
        .mount(&redirected)
        .await;
    let output = handler(&redirected).invoke_arguments(&start_args()).await;
    assert_eq!(parse_output(&output)["code"], "integration_rejected");
}

#[test]
fn handler_keeps_unrelated_tool_identity_stable() {
    let handler = TaskServiceHandler::new(None, None, None, Duration::from_millis(10));
    assert_eq!(handler.tool_name().to_string(), "cutex_task_service");
    let ToolSpec::Function(tool) = handler.spec() else {
        panic!("expected function tool");
    };
    assert_eq!(tool.name, "cutex_task_service");
}
