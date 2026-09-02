use std::ffi::OsString;
use std::time::Duration;

use codex_tools::ToolExposure;
use codex_tools::ToolSpec;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::TaskServiceDirectorHandler;
use super::create_task_service_director_tool;
use crate::tools::registry::ToolExecutor;

const ACTION_ID: &str = "director-action-1";
const PROJECT_ID: &str = "cutex-stack-main";
const ROUTE_TOKEN: &str = "private-route-token";
const RUNTIME_ID: &str = "director-runtime-1";

fn handler(server: &MockServer) -> TaskServiceDirectorHandler {
    TaskServiceDirectorHandler::new(
        Some(OsString::from(server.uri())),
        Some(OsString::from(ROUTE_TOKEN)),
        Some(OsString::from(RUNTIME_ID)),
        Duration::from_millis(100),
    )
}

fn parse_output(output: &str) -> Value {
    serde_json::from_str(output).expect("model receipt JSON")
}

async fn mount_receipt(server: &MockServer, receipt: Value, expected: u64) {
    Mock::given(method("POST"))
        .and(path("/api/task/v2/director-action"))
        .respond_with(ResponseTemplate::new(200).set_body_json(receipt))
        .expect(expected)
        .mount(server)
        .await;
}

fn create_and_assign_args() -> Value {
    json!({
        "operation": "create_and_assign",
        "action_id": ACTION_ID,
        "project_id": PROJECT_ID,
        "workflow_id": "workflow-1",
        "task_id": "task-1",
        "task_revision": 1,
        "contract_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "opaque_contract": "exact contract\n",
        "completion_policy": "director_acceptance",
        "completion_authority_cutex_session_id": "cutex.director.1",
        "assignment_id": "assignment-1",
        "assignee_cutex_session_id": "cutex.worker.1",
        "summary": "Implement the exact contract",
    })
}

fn create_revision_args() -> Value {
    let mut args = create_and_assign_args();
    let object = args.as_object_mut().expect("arguments object");
    object.insert("operation".to_string(), json!("create_revision"));
    for field in ["assignment_id", "assignee_cutex_session_id", "summary"] {
        object.remove(field);
    }
    args
}

fn assign_args() -> Value {
    let mut args = create_and_assign_args();
    let object = args.as_object_mut().expect("arguments object");
    object.insert("operation".to_string(), json!("assign"));
    for field in [
        "workflow_id",
        "contract_sha256",
        "opaque_contract",
        "completion_policy",
        "completion_authority_cutex_session_id",
    ] {
        object.remove(field);
    }
    args
}

fn provider_receipt(operation: &str) -> Value {
    json!({
        "schema": "cutex/task-service-director-receipt/v1",
        "action_id": ACTION_ID,
        "operation": operation,
        "status": "committed",
        "project_id": PROJECT_ID,
        "task_id": "task-1",
        "task_revision": 1,
        "assignment_id": "assignment-1",
        "attempt_number": 1,
        "closure_reason": null,
        "code": null,
        "detail": "provider-private journal detail",
        "continuation": null,
        "tasks": null,
        "assignments": null
    })
}

#[test]
fn schema_is_separate_and_exposes_only_semantic_director_fields() {
    let first = create_task_service_director_tool();
    let second = create_task_service_director_tool();
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).expect("serialize first tool definition"),
        serde_json::to_vec(&second).expect("serialize second tool definition")
    );
    let ToolSpec::Function(tool) = first else {
        panic!("expected function tool");
    };
    assert_eq!(tool.name, "cutex_task_service_director");
    assert_eq!(
        tool.description,
        "Perform an authenticated semantic Director action through Cutex Task Service. Runtime identity and Coordinator/Completion Authority remain provider-authoritative; conversation text and groups grant nothing. create_and_assign is an idempotent two-step convenience, not an atomic primitive."
    );
    assert!(!tool.strict);
    assert_eq!(tool.defer_loading, None);
    assert_eq!(
        TaskServiceDirectorHandler::new(None, None, None, Duration::from_millis(10)).exposure(),
        ToolExposure::Deferred
    );
    assert_eq!(
        tool.parameters.required,
        Some(vec!["operation".to_string(), "action_id".to_string()])
    );
    let properties = tool.parameters.properties.expect("tool properties");
    assert_eq!(
        properties.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "action_id",
            "assignee_cutex_session_id",
            "assignment_id",
            "completion_authority_cutex_session_id",
            "completion_policy",
            "contract_sha256",
            "decision_reference",
            "opaque_contract",
            "operation",
            "project_id",
            "selector",
            "summary",
            "task_id",
            "task_revision",
            "workflow_id",
        ]
    );
    assert!(properties.contains_key("project_id"));
    for forbidden in [
        "store_revision",
        "attempt_token",
        "runtime_agent_id",
        "journal_sequence",
        "caller_identity",
        "notification_id",
        "external_message_id",
    ] {
        assert!(!properties.contains_key(forbidden));
    }
    let operations = properties["operation"]
        .enum_values
        .clone()
        .expect("operation enum");
    assert_eq!(
        operations,
        vec![
            json!("create_revision"),
            json!("assign"),
            json!("create_and_assign"),
            json!("query"),
            json!("accept_result"),
            json!("request_changes"),
            json!("fail_result"),
            json!("cancel"),
        ]
    );
    assert!(
        tool.output_schema
            .as_ref()
            .and_then(|schema| schema["properties"].get("project_id"))
            .is_some()
    );
}

#[tokio::test]
async fn create_and_assign_posts_exact_authenticated_semantic_request() {
    let server = MockServer::start().await;
    mount_receipt(&server, provider_receipt("create_and_assign"), 1).await;

    let output = handler(&server)
        .invoke_arguments(&create_and_assign_args().to_string())
        .await;
    assert_eq!(
        parse_output(&output),
        json!({
            "schema": "cutex/task-service-director-tool-receipt/v1",
            "status": "committed",
            "operation": "create_and_assign",
            "action_id": ACTION_ID,
            "project_id": PROJECT_ID,
            "task_id": "task-1",
            "task_revision": 1,
            "assignment_id": "assignment-1",
            "attempt_number": 1
        })
    );
    assert!(!output.contains("journal"));
    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url.path(), "/api/task/v2/director-action");
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
    let body: Value = serde_json::from_slice(&request.body).expect("request JSON");
    let expected_body = json!({
        "schema": "cutex/task-service-director-action/v2",
        "action_id": ACTION_ID,
        "operation": "create_and_assign",
        "create_revision": {
            "project_id": PROJECT_ID,
            "workflow_id": "workflow-1",
            "task_id": "task-1",
            "task_revision": 1,
            "contract_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "opaque_contract": "exact contract\n",
            "completion_policy": "director_acceptance",
            "completion_authority_cutex_session_id": "cutex.director.1"
        },
        "assign": {
            "project_id": PROJECT_ID,
            "assignment_id": "assignment-1",
            "task_id": "task-1",
            "task_revision": 1,
            "assignee_cutex_session_id": "cutex.worker.1",
            "summary": "Implement the exact contract"
        }
    });
    assert_eq!(body, expected_body);
    assert_eq!(
        request.body,
        serde_json::to_vec(&expected_body).expect("expected request bytes")
    );
}

#[tokio::test]
async fn create_revision_posts_exact_project_scoped_v2_request() {
    let server = MockServer::start().await;
    let mut receipt = provider_receipt("create_revision");
    receipt["assignment_id"] = Value::Null;
    receipt["attempt_number"] = Value::Null;
    mount_receipt(&server, receipt, 1).await;

    let output = handler(&server)
        .invoke_arguments(&create_revision_args().to_string())
        .await;
    assert_eq!(parse_output(&output)["project_id"], PROJECT_ID);
    let requests = server.received_requests().await.expect("received requests");
    let expected_body = json!({
        "schema": "cutex/task-service-director-action/v2",
        "action_id": ACTION_ID,
        "operation": "create_revision",
        "project_id": PROJECT_ID,
        "workflow_id": "workflow-1",
        "task_id": "task-1",
        "task_revision": 1,
        "contract_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "opaque_contract": "exact contract\n",
        "completion_policy": "director_acceptance",
        "completion_authority_cutex_session_id": "cutex.director.1"
    });
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).expect("request JSON"),
        expected_body
    );
    assert_eq!(
        requests[0].body,
        serde_json::to_vec(&expected_body).expect("expected request bytes")
    );
}

#[tokio::test]
async fn assign_posts_exact_project_scoped_v2_request() {
    let server = MockServer::start().await;
    let mut receipt = provider_receipt("assign");
    receipt["attempt_number"] = Value::Null;
    mount_receipt(&server, receipt, 1).await;

    let output = handler(&server)
        .invoke_arguments(&assign_args().to_string())
        .await;
    assert_eq!(parse_output(&output)["project_id"], PROJECT_ID);
    let requests = server.received_requests().await.expect("received requests");
    let expected_body = json!({
        "schema": "cutex/task-service-director-action/v2",
        "action_id": ACTION_ID,
        "operation": "assign",
        "project_id": PROJECT_ID,
        "assignment_id": "assignment-1",
        "task_id": "task-1",
        "task_revision": 1,
        "assignee_cutex_session_id": "cutex.worker.1",
        "summary": "Implement the exact contract"
    });
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).expect("request JSON"),
        expected_body
    );
    assert_eq!(
        requests[0].body,
        serde_json::to_vec(&expected_body).expect("expected request bytes")
    );
}

#[tokio::test]
async fn query_preserves_only_typed_semantic_state() {
    let server = MockServer::start().await;
    mount_receipt(
        &server,
        json!({
            "schema": "cutex/task-service-director-receipt/v1",
            "action_id": ACTION_ID,
            "operation": "query",
            "status": "current_state",
            "task_id": null,
            "task_revision": null,
            "assignment_id": null,
            "attempt_number": null,
            "closure_reason": null,
            "code": null,
            "detail": "private provider journal",
            "continuation": null,
            "tasks": [{
                "project_id": PROJECT_ID,
                "task_id": "task-1",
                "task_revision": 1,
                "workflow_id": "workflow-1",
                "contract_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "completion_policy": "director_acceptance",
                "completion_authority_cutex_session_id": "cutex.director.1",
                "created_at": "2026-08-28T01:02:03Z"
            }],
            "assignments": [{
                "project_id": PROJECT_ID,
                "assignment_id": "assignment-1",
                "task_id": "task-1",
                "task_revision": 1,
                "assignee_cutex_session_id": "cutex.worker.1",
                "state": "active",
                "active_attempt_number": 1,
                "closure_reason": null,
                "created_at": "2026-08-28T01:03:00Z",
                "attempts": [{
                    "attempt_number": 1,
                    "phase": "review_ready",
                    "started_at": "2026-08-28T01:03:01Z",
                    "updated_at": "2026-08-28T01:04:00Z",
                    "latest_status_summary": "Ready for review",
                    "result_reference": "/stable/result"
                }]
            }]
        }),
        1,
    )
    .await;

    let output = handler(&server)
        .invoke_arguments(
            &json!({
                "operation": "query",
                "action_id": ACTION_ID,
                "selector": {"kind": "assignment", "assignment_id": "assignment-1"}
            })
            .to_string(),
        )
        .await;
    let receipt = parse_output(&output);
    assert_eq!(receipt["status"], "current_state");
    assert_eq!(receipt["tasks"][0]["project_id"], PROJECT_ID);
    assert_eq!(receipt["assignments"][0]["project_id"], PROJECT_ID);
    assert_eq!(
        receipt["assignments"][0]["attempts"][0]["phase"],
        "review_ready"
    );
    assert!(!output.contains("private provider journal"));
    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).expect("request JSON"),
        json!({
            "schema": "cutex/task-service-director-action/v2",
            "action_id": ACTION_ID,
            "operation": "query",
            "selector": {"kind": "assignment", "assignment_id": "assignment-1"}
        })
    );
}

#[tokio::test]
async fn live_r21_query_receipt_preserves_additive_semantic_metadata() {
    const LIVE_ACTION_ID: &str = "r11-raw-query-r21-canary-20260829-01";
    const LIVE_ASSIGNMENT_ID: &str = "r11-r21-request-changes-live-canary-assignment-01";
    let server = MockServer::start().await;
    let live_receipt = json!({
        "action_id": LIVE_ACTION_ID,
        "assignments": [{
            "acknowledged_at": "2026-08-29T13:49:33.846085341Z",
            "active_attempt_number": 1,
            "assignee_cutex_session_id": "cutex.01a049d1-a554-7d61-a3ea-f6e6b09814a6",
            "assignee_display_name": "cutex-task-watchdog-r1",
            "assignment_id": LIVE_ASSIGNMENT_ID,
            "attempts": [{
                "attempt_number": 1,
                "phase": "completed",
                "result_reference": "/home/example/Projects/cutex/agent-home/cutex-task-watchdog-r1/evidence-r21-request-changes-live-canary/followup.txt",
                "result_submitted_at": "2026-08-29T13:52:16.299087617Z",
                "started_at": "2026-08-29T13:49:33.846085341Z",
                "updated_at": "2026-08-29T13:53:00.402917636Z"
            }],
            "closed_at": "2026-08-29T13:53:00.402917636Z",
            "closure_reason": "completed",
            "created_at": "2026-08-29T13:49:21.346200581Z",
            "project_id": PROJECT_ID,
            "state": "closed",
            "task_id": "r11-r21-request-changes-live-canary",
            "task_revision": 1
        }],
        "operation": "query",
        "schema": "cutex/task-service-director-receipt/v1",
        "status": "current_state",
        "tasks": [{
            "completion_authority_cutex_session_id": "cutex.01a041c5-66be-7ae3-9ffa-2bab35203014",
            "completion_policy": "director_acceptance",
            "contract_sha256": "71e25e145d78b8effe9b272021939eccc364e1f158324e07bda8cc29743aa06f",
            "created_at": "2026-08-29T13:49:21.270288165Z",
            "project_id": PROJECT_ID,
            "task_id": "r11-r21-request-changes-live-canary",
            "task_revision": 1,
            "workflow_id": "r11-r21-request-changes-live-canary"
        }]
    });
    mount_receipt(&server, live_receipt.clone(), 1).await;

    let output = parse_output(
        &handler(&server)
            .invoke_arguments(
                &json!({
                    "operation": "query",
                    "action_id": LIVE_ACTION_ID,
                    "selector": {"kind": "assignment", "assignment_id": LIVE_ASSIGNMENT_ID}
                })
                .to_string(),
            )
            .await,
    );
    assert_eq!(
        output,
        json!({
            "schema": "cutex/task-service-director-tool-receipt/v1",
            "status": "current_state",
            "operation": "query",
            "action_id": LIVE_ACTION_ID,
            "tasks": live_receipt["tasks"],
            "assignments": [{
                "acknowledged_at": "2026-08-29T13:49:33.846085341Z",
                "active_attempt_number": 1,
                "assignee_cutex_session_id": "cutex.01a049d1-a554-7d61-a3ea-f6e6b09814a6",
                "assignee_display_name": "cutex-task-watchdog-r1",
                "assignment_id": LIVE_ASSIGNMENT_ID,
                "attempts": [{
                    "attempt_number": 1,
                    "phase": "completed",
                    "started_at": "2026-08-29T13:49:33.846085341Z",
                    "updated_at": "2026-08-29T13:53:00.402917636Z",
                    "latest_status_summary": null,
                    "result_reference": "/home/example/Projects/cutex/agent-home/cutex-task-watchdog-r1/evidence-r21-request-changes-live-canary/followup.txt",
                    "result_submitted_at": "2026-08-29T13:52:16.299087617Z"
                }],
                "closed_at": "2026-08-29T13:53:00.402917636Z",
                "closure_reason": "completed",
                "created_at": "2026-08-29T13:49:21.346200581Z",
                "project_id": PROJECT_ID,
                "state": "closed",
                "task_id": "r11-r21-request-changes-live-canary",
                "task_revision": 1
            }]
        })
    );
}

#[tokio::test]
async fn nested_semantic_additions_do_not_relax_required_identity_or_numbers() {
    let query_args = json!({
        "operation": "query",
        "action_id": ACTION_ID,
        "selector": {"kind": "all"}
    });
    let receipt = json!({
        "schema": "cutex/task-service-director-receipt/v1",
        "action_id": ACTION_ID,
        "operation": "query",
        "status": "current_state",
        "tasks": [{
            "project_id": PROJECT_ID,
            "task_id": "task-1",
            "task_revision": 1,
            "workflow_id": "workflow-1",
            "contract_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "completion_policy": "director_acceptance",
            "completion_authority_cutex_session_id": null,
            "created_at": "2026-08-29T13:49:21Z",
            "future_task_metadata": {"version": 22}
        }],
        "assignments": [{
            "project_id": PROJECT_ID,
            "assignment_id": "assignment-1",
            "task_id": "task-1",
            "task_revision": 1,
            "assignee_cutex_session_id": "cutex.worker.1",
            "state": "active",
            "active_attempt_number": 1,
            "closure_reason": null,
            "created_at": "2026-08-29T13:49:22Z",
            "attempts": [{
                "attempt_number": 1,
                "phase": "running",
                "started_at": "2026-08-29T13:49:23Z",
                "updated_at": "2026-08-29T13:49:24Z",
                "latest_status_summary": null,
                "result_reference": null,
                "future_attempt_metadata": [1, 2, 3]
            }],
            "future_assignment_metadata": true
        }]
    });
    let server = MockServer::start().await;
    mount_receipt(&server, receipt.clone(), 1).await;
    let output = handler(&server)
        .invoke_arguments(&query_args.to_string())
        .await;
    assert_eq!(parse_output(&output)["status"], "current_state");
    assert!(!output.contains("future_"));

    let mut missing_identity = receipt.clone();
    missing_identity["assignments"][0]
        .as_object_mut()
        .expect("assignment object")
        .remove("assignment_id");
    let mut invalid_revision = receipt.clone();
    invalid_revision["tasks"][0]["task_revision"] = json!(0);
    let mut invalid_attempt = receipt;
    invalid_attempt["assignments"][0]["attempts"][0]["attempt_number"] = json!(0);
    for invalid in [missing_identity, invalid_revision, invalid_attempt] {
        let server = MockServer::start().await;
        mount_receipt(&server, invalid, 1).await;
        assert_eq!(
            parse_output(
                &handler(&server)
                    .invoke_arguments(&query_args.to_string())
                    .await
            )["code"],
            "invalid_provider_response"
        );
    }
}

#[tokio::test]
async fn partial_convenience_receipt_has_exact_retry_continuation() {
    let server = MockServer::start().await;
    let mut receipt = provider_receipt("create_and_assign");
    receipt["status"] = json!("response_uncertain");
    receipt["code"] = json!("assign_response_uncertain");
    receipt["continuation"] = json!({
        "phase": "create_revision_committed",
        "retry_action_id": ACTION_ID
    });
    mount_receipt(&server, receipt, 2).await;

    let first = parse_output(
        &handler(&server)
            .invoke_arguments(&create_and_assign_args().to_string())
            .await,
    );
    let second = parse_output(
        &handler(&server)
            .invoke_arguments(&create_and_assign_args().to_string())
            .await,
    );
    assert_eq!(first, second);
    assert_eq!(first["status"], "response_uncertain");
    assert_eq!(first["continuation"]["phase"], "create_revision_committed");
    assert_eq!(first["continuation"]["retry_action_id"], ACTION_ID);
    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(requests[0].body, requests[1].body);
}

#[tokio::test]
async fn terminal_action_is_minimal_and_provider_authoritative() {
    let server = MockServer::start().await;
    let mut receipt = provider_receipt("request_changes");
    receipt["task_id"] = Value::Null;
    receipt["task_revision"] = Value::Null;
    receipt["attempt_number"] = Value::Null;
    receipt["status"] = json!("committed");
    mount_receipt(&server, receipt, 1).await;
    let output = handler(&server)
        .invoke_arguments(
            &json!({
                "operation": "request_changes",
                "action_id": ACTION_ID,
                "assignment_id": "assignment-1",
                "decision_reference": "review/17"
            })
            .to_string(),
        )
        .await;
    assert_eq!(parse_output(&output)["status"], "committed");
    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).expect("request JSON"),
        json!({
            "schema": "cutex/task-service-director-action/v2",
            "action_id": ACTION_ID,
            "operation": "request_changes",
            "assignment_id": "assignment-1",
            "decision_reference": "review/17"
        })
    );
}

#[tokio::test]
async fn project_id_is_required_for_writes_and_rejected_elsewhere() {
    let missing = TaskServiceDirectorHandler::new(None, None, None, Duration::from_millis(10));
    for mut args in [
        create_revision_args(),
        assign_args(),
        create_and_assign_args(),
    ] {
        args.as_object_mut()
            .expect("arguments object")
            .remove("project_id");
        assert_eq!(
            parse_output(&missing.invoke_arguments(&args.to_string()).await)["code"],
            "invalid_semantic_payload"
        );
    }
    for project_id in ["", "project with spaces", &"x".repeat(257)] {
        let mut args = create_and_assign_args();
        args["project_id"] = json!(project_id);
        assert_eq!(
            parse_output(&missing.invoke_arguments(&args.to_string()).await)["code"],
            "invalid_identity"
        );
    }
    for mut args in [
        json!({
            "operation": "query",
            "action_id": ACTION_ID,
            "project_id": PROJECT_ID,
            "selector": {"kind": "all"}
        }),
        json!({
            "operation": "accept_result",
            "action_id": ACTION_ID,
            "project_id": PROJECT_ID,
            "assignment_id": "assignment-1"
        }),
    ] {
        assert_eq!(
            parse_output(&missing.invoke_arguments(&args.to_string()).await)["code"],
            "invalid_semantic_payload"
        );
        args["create_revision"] = json!({"project_id": PROJECT_ID});
        assert_eq!(
            parse_output(&missing.invoke_arguments(&args.to_string()).await)["code"],
            "invalid_arguments"
        );
    }
}

#[tokio::test]
async fn changed_project_exact_retry_preserves_action_id_and_compact_conflict() {
    let server = MockServer::start().await;
    let mut receipt = provider_receipt("create_and_assign");
    receipt["status"] = json!("conflict");
    receipt["code"] = json!("exact_action_conflict");
    mount_receipt(&server, receipt, 1).await;
    let mut changed = create_and_assign_args();
    changed["project_id"] = json!("different-project");

    let output = parse_output(
        &handler(&server)
            .invoke_arguments(&changed.to_string())
            .await,
    );
    assert_eq!(
        output,
        json!({
            "schema": "cutex/task-service-director-tool-receipt/v1",
            "status": "conflict",
            "operation": "create_and_assign",
            "action_id": ACTION_ID,
            "project_id": PROJECT_ID,
            "task_id": "task-1",
            "task_revision": 1,
            "assignment_id": "assignment-1",
            "attempt_number": 1,
            "code": "exact_action_conflict"
        })
    );
    let requests = server.received_requests().await.expect("received requests");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).expect("request JSON")["action_id"],
        ACTION_ID
    );
}

#[tokio::test]
async fn legacy_v1_query_receipt_without_projects_remains_readable() {
    let server = MockServer::start().await;
    mount_receipt(
        &server,
        json!({
            "schema": "cutex/task-service-director-receipt/v1",
            "action_id": ACTION_ID,
            "operation": "query",
            "status": "current_state",
            "task_id": null,
            "task_revision": null,
            "assignment_id": null,
            "attempt_number": null,
            "closure_reason": null,
            "code": null,
            "detail": null,
            "continuation": null,
            "tasks": [{
                "task_id": "legacy-task",
                "task_revision": 1,
                "workflow_id": "legacy-workflow",
                "contract_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "completion_policy": "director_acceptance",
                "completion_authority_cutex_session_id": null,
                "created_at": "2026-08-28T01:02:03Z"
            }],
            "assignments": []
        }),
        1,
    )
    .await;

    let output = parse_output(
        &handler(&server)
            .invoke_arguments(
                &json!({
                    "operation": "query",
                    "action_id": ACTION_ID,
                    "selector": {"kind": "all"}
                })
                .to_string(),
            )
            .await,
    );
    assert_eq!(output["tasks"][0]["task_id"], "legacy-task");
    assert!(output.get("project_id").is_none());
    assert!(output["tasks"][0].get("project_id").is_none());
}

#[tokio::test]
async fn mechanical_fields_and_missing_auth_fail_closed_without_transport() {
    let missing = TaskServiceDirectorHandler::new(None, None, None, Duration::from_millis(10));
    let mut forged = create_and_assign_args();
    forged["attempt_token"] = json!("prompt-claimed-secret");
    assert_eq!(
        parse_output(&missing.invoke_arguments(&forged.to_string()).await)["code"],
        "invalid_arguments"
    );
    assert_eq!(
        parse_output(
            &missing
                .invoke_arguments(&create_and_assign_args().to_string())
                .await
        )["code"],
        "missing_authenticated_integration"
    );
}

#[tokio::test]
async fn malformed_provider_receipt_is_a_secret_free_no_write() {
    let server = MockServer::start().await;
    let mut receipt = provider_receipt("create_and_assign");
    receipt["attempt_token"] = json!("provider-private-token");
    mount_receipt(&server, receipt, 1).await;
    let output = handler(&server)
        .invoke_arguments(&create_and_assign_args().to_string())
        .await;
    assert_eq!(parse_output(&output)["code"], "invalid_provider_response");
    assert!(!output.contains("provider-private-token"));
}

#[tokio::test]
async fn transport_timeout_returns_compact_exact_retry_receipt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/task/v2/director-action"))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&server)
        .await;
    let output = parse_output(
        &handler(&server)
            .invoke_arguments(&create_and_assign_args().to_string())
            .await,
    );
    assert_eq!(output["status"], "response_uncertain");
    assert_eq!(output["code"], "exact_retry_required");
    assert_eq!(output["continuation"]["phase"], "outcome_unknown");
    assert_eq!(output["continuation"]["retry_action_id"], ACTION_ID);
}
