use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use codex_tools::ToolExecutor;
use codex_tools::ToolSpec;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::*;

const QUERY: &str = r#"{"operation":"query_managed","action_id":"query-1"}"#;

fn integration(timeout: Duration) -> Integration {
    Integration {
        executable: PathBuf::from("unused"),
        runtime_agent_id: "runtime-1".to_string(),
        timeout,
    }
}

fn input(value: Value) -> ToolInput {
    parse_input(&value.to_string()).expect("valid tool input")
}

fn query_input() -> ToolInput {
    parse_input(QUERY).expect("valid query")
}

fn resolved(runtime_agent_id: Option<&str>) -> Result<Integration, &'static str> {
    Integration::resolve(
        OsString::from("cutex"),
        runtime_agent_id.map(str::to_string),
        Duration::from_secs(1),
    )
}

#[test]
fn native_tool_identity_and_schema_expose_only_a_non_authoritative_project_selector() {
    let handler = AgentManagementHandler {
        integration: Err("missing_ambient_identity"),
    };
    assert_eq!(handler.tool_name().to_string(), AGENT_MANAGEMENT_TOOL_NAME);
    let ToolSpec::Function(tool) = handler.spec() else {
        panic!("agent management must be a function tool");
    };
    let schema = serde_json::to_value(tool).expect("serializable tool");
    assert_eq!(schema["name"], AGENT_MANAGEMENT_TOOL_NAME);
    assert_eq!(
        schema["parameters"]["required"],
        json!(["operation", "action_id"])
    );
    let properties = schema["parameters"]["properties"]
        .as_object()
        .expect("object properties");
    assert!(properties.contains_key("project_id"));
    let encoded = schema["parameters"].to_string();
    for operation in [
        "create",
        "query_managed",
        "online",
        "offline",
        "restart",
        "close",
        "replace",
        "director_rotate",
    ] {
        assert!(encoded.contains(operation));
    }
    for forbidden in [
        "caller_cutex_session",
        "caller_runtime_agent_id",
        "runtime_agent_id",
        "session_id",
        "seat_id",
        "authorized_director_session",
    ] {
        assert!(!properties.contains_key(forbidden));
    }
}

#[tokio::test]
async fn only_authenticated_runtime_identity_gates_the_native_integration() {
    assert_eq!(resolved(None).unwrap_err(), "missing_ambient_identity");
    assert_eq!(
        resolved(Some("not valid!")).unwrap_err(),
        "missing_ambient_identity"
    );
    assert_eq!(
        resolved(Some("runtime-1"))
            .expect("valid runtime")
            .runtime_agent_id,
        "runtime-1"
    );

    let handler = AgentManagementHandler {
        integration: Err("missing_ambient_identity"),
    };
    let receipt: Value = serde_json::from_str(&handler.invoke_arguments(QUERY).await).unwrap();
    assert_eq!(receipt["outcome"]["status"], "no_write");
    assert_eq!(receipt["outcome"]["code"], "missing_ambient_identity");
}

#[test]
fn every_v1_operation_serializes_stable_request_bytes_with_an_optional_selector() {
    let spec = json!({
        "name": "worker", "cwd": "/private/worker", "profile": "aemeath",
        "runtime_backend": "cute_alden", "model": "gpt-5.6-sol", "reasoning": "high",
        "permissions": "danger-full-access", "approval_policy": "never",
        "sandbox_mode": "danger-full-access", "groups": ["project:cutex-stack-main"]
    });
    let cases = [
        json!({"operation":"create","action_id":"a1","spec":spec,"start_mode":"bootstrap_only"}),
        json!({"operation":"query_managed","action_id":"a2","project_id":"cutex-stack-main"}),
        json!({"operation":"online","action_id":"a3","cutex_session_id":"cutex.session"}),
        json!({"operation":"offline","action_id":"a4","project_id":"cutex-stack-main","cutex_session_id":"cutex.session"}),
        json!({"operation":"restart","action_id":"a5","cutex_session_id":"cutex.session"}),
        json!({"operation":"close","action_id":"a6","project_id":"cutex-stack-main","cutex_session_id":"cutex.session"}),
        json!({"operation":"replace","action_id":"a7","predecessor_cutex_session_id":"cutex.old","policy":"close_after_ready","successor":spec,"start_mode":"custom_message","frozen_message":"start"}),
        json!({"operation":"director_rotate","action_id":"a8","project_id":"cutex-stack-main","expected_predecessor_cutex_session":"cutex.director","expected_authority_epoch":2,"mode":"retain_predecessor_bootstrap_only","successor":spec}),
    ];
    for value in cases {
        let selector = value.get("project_id").cloned();
        let input = input(value);
        let first = request_bytes(&input).expect("serializable");
        let second = request_bytes(&input).expect("stable replay");
        assert_eq!(first, second);
        let request: Value = serde_json::from_slice(&first.0).expect("request JSON");
        assert_eq!(request["schema"], CONTRACT);
        assert_eq!(request.get("project_id"), selector.as_ref());
        assert_eq!(request["operation"], input.operation.name());
        assert!(request.get("caller_cutex_session").is_none());
        assert!(request.get("caller_runtime_agent_id").is_none());
        assert_eq!(first.1, format!("{:x}", Sha256::digest(&first.0)));
    }
}

#[test]
fn implicit_and_explicit_project_receipts_are_sanitized_strictly() {
    let implicit = query_input();
    let (_, implicit_digest) = request_bytes(&implicit).unwrap();
    let implicit_receipt = json!({
        "schema": RECEIPT_SCHEMA,
        "action_id": "query-1",
        "request_sha256": implicit_digest,
        "operation": "query_managed",
        "project_id": "cutex-stack-main",
        "completed_at": "2026-08-27T00:00:00Z",
        "result": {
            "kind": "query_managed",
            "authority": {"project_id": "cutex-stack-main"},
            "agents": [{"project_id": "cutex-stack-main"}]
        }
    });
    let response = json!({
        "schema": CONTRACT,
        "action_id": "query-1",
        "outcome": {"status": "complete", "receipt": implicit_receipt}
    });
    assert_eq!(
        sanitize_response(&implicit, &implicit_digest, response)["outcome"]["status"],
        "complete"
    );

    let explicit = input(json!({
        "operation": "query_managed",
        "action_id": "query-1",
        "project_id": "cutex-stack-main"
    }));
    let (_, explicit_digest) = request_bytes(&explicit).unwrap();
    let failure = json!({
        "schema": FAILURE_SCHEMA,
        "event_id": "event-1",
        "action_id": "query-1",
        "project_id": "cutex-stack-main",
        "operation": "query_managed",
        "code": "external_failure",
        "detail": "owner action",
        "routing_status": "unrouted",
        "created_at": "2026-08-27T00:00:00Z"
    });
    let cases = [
        (
            json!({"schema":CONTRACT,"action_id":"query-1","outcome":{"status":"no_write","code":"project_selection_required","detail":"select one authorized project"}}),
            "no_write",
        ),
        (
            json!({"schema":CONTRACT,"action_id":"query-1","outcome":{"status":"owner_action_required","failure":failure}}),
            "owner_action_required",
        ),
    ];
    for (response, status) in cases {
        assert_eq!(
            sanitize_response(&explicit, &explicit_digest, response)["outcome"]["status"],
            status
        );
    }
}

#[test]
fn owner_action_failure_accepts_only_documented_optional_session_targets() {
    let input = input(json!({
        "operation": "query_managed",
        "action_id": "query-1",
        "project_id": "cutex-stack-main"
    }));
    let (_, digest) = request_bytes(&input).unwrap();
    let failure = json!({
        "schema": FAILURE_SCHEMA,
        "event_id": "event-1",
        "action_id": "query-1",
        "project_id": "cutex-stack-main",
        "operation": "query_managed",
        "code": "owner_authority_required",
        "detail": "the project owner must rotate the Director",
        "routing_status": "routable",
        "created_at": "2026-08-30T00:00:00Z",
        "route_to_director_session": "cutex.director.current",
        "target_cutex_session_id": "cutex.director.target"
    });
    let response = json!({
        "schema": CONTRACT,
        "action_id": "query-1",
        "outcome": {"status": "owner_action_required", "failure": failure}
    });
    assert_eq!(
        sanitize_response(&input, &digest, response.clone()),
        response
    );

    let mut unknown = failure.clone();
    unknown["unexpected"] = json!(true);
    let mut mistyped = failure.clone();
    mistyped["target_cutex_session_id"] = json!(42);
    let mut malformed = failure.clone();
    malformed["target_cutex_session_id"] = json!("not valid!");
    let mut oversized = failure;
    oversized["target_cutex_session_id"] = json!("x".repeat(257));
    for invalid in [unknown, mistyped, malformed, oversized] {
        let response = json!({
            "schema": CONTRACT,
            "action_id": "query-1",
            "outcome": {"status": "owner_action_required", "failure": invalid}
        });
        assert_eq!(
            sanitize_response(&input, &digest, response)["outcome"]["code"],
            "invalid_provider_response"
        );
    }
}

#[test]
fn inconsistent_or_forged_provider_project_payloads_fail_closed() {
    let implicit = query_input();
    let (_, digest) = request_bytes(&implicit).unwrap();
    let receipt = |request_digest: &str, project: &str, nested_project: &str| {
        json!({
            "schema": RECEIPT_SCHEMA,
            "action_id": "query-1",
            "request_sha256": request_digest,
            "operation": "query_managed",
            "project_id": project,
            "completed_at": "2026-08-27T00:00:00Z",
            "result": {"kind":"query_managed","authority":{"project_id":nested_project}}
        })
    };
    let mut receipt_with_unknown_field = receipt(&digest, "cutex-stack-main", "cutex-stack-main");
    receipt_with_unknown_field["unexpected"] = json!(true);
    let mut receipt_with_wrong_result = receipt(&digest, "cutex-stack-main", "cutex-stack-main");
    receipt_with_wrong_result["result"]["kind"] = json!("lifecycle");
    for bad_receipt in [
        receipt(&digest, "cutex-stack-main", "other-project"),
        receipt(&digest, "not valid!", "not valid!"),
        receipt_with_unknown_field,
        receipt_with_wrong_result,
    ] {
        let response = json!({
            "schema": CONTRACT,
            "action_id": "query-1",
            "outcome": {"status":"complete", "receipt":bad_receipt}
        });
        assert_eq!(
            sanitize_response(&implicit, &digest, response)["outcome"]["code"],
            "invalid_provider_response"
        );
    }

    let explicit = input(json!({
        "operation":"query_managed",
        "action_id":"query-1",
        "project_id":"authorized-project"
    }));
    let (_, explicit_digest) = request_bytes(&explicit).unwrap();
    let response = json!({
        "schema": CONTRACT,
        "action_id": "query-1",
        "outcome": {"status":"complete", "receipt":receipt(&explicit_digest, "other-project", "other-project")}
    });
    assert_eq!(
        sanitize_response(&explicit, &explicit_digest, response)["outcome"]["code"],
        "invalid_provider_response"
    );
}

#[tokio::test]
async fn selector_is_validated_and_model_authority_claims_are_rejected_before_execution() {
    let handler = AgentManagementHandler {
        integration: Ok(integration(Duration::from_secs(1))),
    };
    let mut invalid_selector: Value = serde_json::from_str(QUERY).unwrap();
    invalid_selector["project_id"] = json!("not valid!");
    let receipt: Value = serde_json::from_str(
        &handler
            .invoke_arguments(&invalid_selector.to_string())
            .await,
    )
    .unwrap();
    assert_eq!(receipt["outcome"]["code"], "invalid_arguments");

    for forbidden in [
        "caller_cutex_session",
        "caller_runtime_agent_id",
        "runtime_agent_id",
        "session_id",
        "seat_id",
        "authorized_director_session",
        "authority_epoch",
        "project_authority",
    ] {
        let mut value: Value = serde_json::from_str(QUERY).unwrap();
        value[forbidden] = json!("forged");
        let receipt: Value =
            serde_json::from_str(&handler.invoke_arguments(&value.to_string()).await).unwrap();
        assert_eq!(receipt["outcome"]["status"], "no_write");
        assert_eq!(receipt["outcome"]["code"], "invalid_arguments");
    }
}

#[cfg(unix)]
fn executable_with_runtime(
    script: &str,
    timeout: Duration,
    runtime_agent_id: &str,
) -> (tempfile::TempDir, AgentManagementHandler) {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temp directory");
    let path = directory.path().join("cutex-provider");
    std::fs::write(&path, format!("#!/bin/sh\nset -eu\n{script}\n")).expect("write provider");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .expect("executable provider");
    let handler = AgentManagementHandler {
        integration: Ok(Integration {
            executable: path,
            runtime_agent_id: runtime_agent_id.to_string(),
            timeout,
        }),
    };
    (directory, handler)
}

#[cfg(unix)]
fn executable(script: &str, timeout: Duration) -> (tempfile::TempDir, AgentManagementHandler) {
    executable_with_runtime(script, timeout, "runtime-1")
}

#[cfg(unix)]
#[tokio::test]
async fn argv_provider_receives_private_selector_free_request_and_no_group_authority() {
    let script = r#"
test "$1" = agent
test "$2" = manage
test "$3" = query-managed
test "$4" = --request-file
test "$CUTEX_AGENT_ID" = runtime-1
test -z "${CUTEX_AGENT_GROUPS+x}"
test -f "$5"
test "$(stat -c '%a' "$5")" = 600
! grep -q '"project_id"' "$5"
! grep -q 'caller_' "$5"
printf '%s' '{"schema":"cutex/agent-management/v1","action_id":"query-1","outcome":{"status":"no_write","code":"not_authorized_director","detail":"test"}}'
"#;
    let (_directory, handler) = executable(script, Duration::from_secs(1));
    let receipt: Value =
        serde_json::from_str(&handler.invoke_arguments(QUERY).await).expect("typed receipt");
    assert_eq!(receipt["outcome"]["status"], "no_write");
    assert_eq!(receipt["outcome"]["code"], "not_authorized_director");
}

#[cfg(unix)]
#[tokio::test]
async fn fake_cutex_cli_covers_zero_one_multiple_projects_and_exact_replay() {
    let script = r#"
request="$5"
digest=$(sha256sum "$request" | cut -d ' ' -f 1)
no_write() {
  printf '{"schema":"cutex/agent-management/v1","action_id":"query-1","outcome":{"status":"no_write","code":"%s","detail":"typed provider denial"}}' "$1"
}
complete() {
  printf '{"schema":"cutex/agent-management/v1","action_id":"query-1","outcome":{"status":"complete","receipt":{"schema":"cutex/agent-management-receipt/v1","action_id":"query-1","request_sha256":"%s","operation":"query_managed","project_id":"%s","completed_at":"2026-08-27T00:00:00Z","result":{"kind":"query_managed","authority":{"project_id":"%s"},"agents":[]}}}}' "$digest" "$1" "$1"
}
case "$CUTEX_AGENT_ID" in
  runtime-zero) no_write not_authorized_director ;;
  runtime-stale) no_write stale_runtime_identity ;;
  runtime-one) complete cutex-one ;;
  runtime-many)
    if grep -q '"project_id":"cutex-two"' "$request"; then
      complete cutex-two
    elif grep -q '"project_id"' "$request"; then
      no_write project_not_authorized
    else
      no_write project_selection_required
    fi
    ;;
  *) exit 9 ;;
esac
"#;
    for (runtime, selector, expected_status, expected_code) in [
        (
            "runtime-zero",
            None,
            "no_write",
            Some("not_authorized_director"),
        ),
        (
            "runtime-stale",
            None,
            "no_write",
            Some("stale_runtime_identity"),
        ),
        ("runtime-one", None, "complete", None),
        (
            "runtime-many",
            None,
            "no_write",
            Some("project_selection_required"),
        ),
        (
            "runtime-many",
            Some("not-authorized"),
            "no_write",
            Some("project_not_authorized"),
        ),
        ("runtime-many", Some("cutex-two"), "complete", None),
    ] {
        let (_directory, handler) =
            executable_with_runtime(script, Duration::from_secs(1), runtime);
        let mut query: Value = serde_json::from_str(QUERY).unwrap();
        if let Some(selector) = selector {
            query["project_id"] = json!(selector);
        }
        let first: Value =
            serde_json::from_str(&handler.invoke_arguments(&query.to_string()).await).unwrap();
        let replay: Value =
            serde_json::from_str(&handler.invoke_arguments(&query.to_string()).await).unwrap();
        assert_eq!(first, replay, "exact action replay must be stable");
        assert_eq!(first["outcome"]["status"], expected_status);
        if let Some(expected_code) = expected_code {
            assert_eq!(first["outcome"]["code"], expected_code);
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn provider_nonzero_invalid_output_timeout_and_strict_outer_schema_fail_closed() {
    let cases = [
        ("exit 7", Duration::from_secs(1), "provider_rejected"),
        (
            "printf 'not-json'",
            Duration::from_secs(1),
            "invalid_provider_response",
        ),
        ("sleep 1", Duration::from_millis(10), "provider_timeout"),
        (
            "printf '%s' '{\"schema\":\"cutex/agent-management/v1\",\"action_id\":\"query-1\",\"unexpected\":true,\"outcome\":{\"status\":\"no_write\",\"code\":\"not_authorized_director\",\"detail\":\"test\"}}'",
            Duration::from_secs(1),
            "invalid_provider_response",
        ),
    ];
    for (script, timeout, code) in cases {
        let (_directory, handler) = executable(script, timeout);
        let receipt: Value = serde_json::from_str(&handler.invoke_arguments(QUERY).await).unwrap();
        assert_eq!(receipt["outcome"]["status"], "no_write");
        assert_eq!(receipt["outcome"]["code"], code);
    }
}
