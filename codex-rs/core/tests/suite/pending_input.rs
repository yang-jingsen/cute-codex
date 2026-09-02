use core_test_support::test_codex::local_selections;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_core::CodexThread;
use codex_core::StartIfIdleSubmission;
use codex_core::TurnInput;
use codex_core::TurnInputRequest;
use codex_core::TurnInputSubmission;
use codex_core::config::CurrentTimeReminderConfig;
use codex_core::inter_agent_delivery::InterAgentDeliveryQuery;
use codex_core::inter_agent_delivery::InterAgentDeliveryState;
use codex_core::inter_agent_delivery::inter_agent_semantic_sha256;
use codex_core::test_support::inject_inter_agent_persistence_failures;
use codex_core::test_support::reconstructed_inter_agent_has_pending_model_actions;
use codex_extension_items::ExtensionItem;
use codex_extension_items::sleep::SleepItem;
use codex_features::Feature;
use codex_history::RolloutItem;
use codex_history::RolloutLine;
use codex_protocol::AgentPath;
use codex_protocol::ResponseItemId;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::Settings;
use codex_protocol::items::TurnItem;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::InterAgentCommunication;
use codex_protocol::protocol::InterAgentDeliveryMode;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::user_input::UserInput;
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::responses;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_message_item_added;
use core_test_support::responses::ev_output_text_delta;
use core_test_support::responses::ev_reasoning_item;
use core_test_support::responses::ev_reasoning_item_added;
use core_test_support::responses::ev_response_created;
use core_test_support::streaming_sse::StreamingSseChunk;
use core_test_support::streaming_sse::StreamingSseServer;
use core_test_support::streaming_sse::start_streaming_sse_server;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::test_codex::turn_permission_fields;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::from_slice;
use serde_json::json;
use tokio::sync::oneshot;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_user_input_reaches_the_first_model_request() -> anyhow::Result<()> {
    assert_idle_user_input_reaches_the_first_model_request(ModeKind::Default).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_user_input_reaches_the_first_model_request_in_plan_mode() -> anyhow::Result<()> {
    assert_idle_user_input_reaches_the_first_model_request(ModeKind::Plan).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_response_items_include_pending_mailbox_in_first_request() -> anyhow::Result<()> {
    let server = responses::start_mock_server().await;
    let response = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            ev_response_created("idle-response-items"),
            ev_completed("idle-response-items"),
        ]),
    )
    .await;
    let test = test_codex().build_with_auto_env(&server).await?;

    submit_queue_only_agent_mail(test.codex.as_ref(), "pending mailbox input").await;
    let submission = test
        .codex
        .start_turn_if_idle(TurnInputRequest::new(TurnInput::ResponseItem(
            responses::user_message_item("automatic response item"),
        )))
        .await?;
    assert!(matches!(submission, StartIfIdleSubmission::Started { .. }));
    wait_for_turn_complete(test.codex.as_ref()).await;

    let request = response.single_request();
    let request_body = request.body_json();
    responses::assert_root_turn(&request_body, /*expected*/ None)?;
    responses::assert_parent_turn(&request_body, /*expected*/ None)?;
    let user_messages = request.message_input_texts("user");
    assert!(
        user_messages
            .iter()
            .any(|message| message == "automatic response item")
    );
    assert!(
        request
            .inputs_of_type("agent_message")
            .iter()
            .any(|message| {
                message["author"] == "/root/worker"
                    && message["recipient"] == "/root"
                    && message["content"].as_array().is_some_and(|content| {
                        content.iter().any(|item| {
                            item["type"] == "input_text" && item["text"] == "pending mailbox input"
                        })
                    })
            })
    );

    Ok(())
}

async fn assert_idle_user_input_reaches_the_first_model_request(
    mode: ModeKind,
) -> anyhow::Result<()> {
    let server = responses::start_mock_server().await;
    let response = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            ev_response_created("idle-user-input"),
            ev_completed("idle-user-input"),
        ]),
    )
    .await;
    let test = test_codex().build_with_auto_env(&server).await?;

    if mode == ModeKind::Plan {
        core_test_support::submit_thread_settings(
            test.codex.as_ref(),
            ThreadSettingsOverrides {
                collaboration_mode: Some(CollaborationMode {
                    mode,
                    settings: Settings {
                        model: test.session_configured.model.clone(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            },
        )
        .await?;
    }

    let expected_input = vec![UserInput::Text {
        text: "queued user input reaches the first request".to_string(),
        text_elements: Vec::new(),
    }];
    let submission = test
        .codex
        .start_turn_if_idle(TurnInputRequest::new(TurnInput::UserInput {
            content: expected_input.clone(),
            client_id: Some("queued-user-message".to_string()),
        }))
        .await?;
    assert!(matches!(submission, StartIfIdleSubmission::Started { .. }));

    let user_message = core_test_support::wait_for_event_match(test.codex.as_ref(), |event| {
        let EventMsg::ItemCompleted(event) = event else {
            return None;
        };
        let TurnItem::UserMessage(item) = &event.item else {
            return None;
        };
        Some(item.clone())
    })
    .await;
    assert_eq!(
        Some("queued-user-message".to_string()),
        user_message.client_id
    );
    assert_eq!(expected_input, user_message.content);
    wait_for_turn_complete(test.codex.as_ref()).await;

    let request = response.single_request();
    let request_body = request.body_json();
    let turn_id = request_body["client_metadata"]["turn_id"]
        .as_str()
        .expect("idle user turn id");
    responses::assert_root_turn(&request_body, Some(turn_id))?;
    assert!(
        request
            .message_input_texts("user")
            .iter()
            .any(|text| text == "queued user input reaches the first request"),
        "the first Responses request should contain the queued user message"
    );

    Ok(())
}

fn ev_message_item_done(id: &str, text: &str) -> Value {
    serde_json::json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "id": id,
            "content": [{"type": "output_text", "text": text}]
        }
    })
}

fn ev_final_answer_item_done(id: &str, text: &str) -> Value {
    serde_json::json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "id": id,
            "content": [{"type": "output_text", "text": text}],
            "phase": "final_answer"
        }
    })
}

fn sse_event(event: Value) -> String {
    responses::sse(vec![event])
}

fn message_input_texts(body: &Value, role: &str) -> Vec<String> {
    body.get("input")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter(|item| item.get("role").and_then(Value::as_str) == Some(role))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_text"))
        .filter_map(|span| span.get("text").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn agent_message_input_texts(body: &Value) -> Vec<String> {
    body.get("input")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("agent_message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_text"))
        .filter_map(|span| span.get("text").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn function_call_output_text<'a>(body: &'a Value, call_id: &str) -> Option<&'a str> {
    body.get("input")
        .and_then(Value::as_array)?
        .iter()
        .find(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                && item.get("call_id").and_then(Value::as_str) == Some(call_id)
        })?
        .get("output")?
        .as_str()
}

fn assert_interrupted_sleep_output(output: Option<&str>) {
    let Some(output) = output else {
        panic!("sleep output missing");
    };
    let Some(wall_time) = output
        .strip_prefix("Wall time: ")
        .and_then(|output| output.strip_suffix(" seconds\nSleep interrupted by new input."))
    else {
        panic!("sleep output should include wall time");
    };
    assert!(
        wall_time.parse::<f64>().is_ok(),
        "sleep wall time should be a number"
    );
}

fn chunk(event: Value) -> StreamingSseChunk {
    StreamingSseChunk {
        gate: None,
        body: responses::sse(vec![event]),
    }
}

fn gated_chunk(gate: oneshot::Receiver<()>, events: Vec<Value>) -> StreamingSseChunk {
    StreamingSseChunk {
        gate: Some(gate),
        body: responses::sse(events),
    }
}

fn response_completed_chunks(response_id: &str) -> Vec<StreamingSseChunk> {
    vec![
        chunk(ev_response_created(response_id)),
        chunk(ev_completed(response_id)),
    ]
}

fn final_answer_chunks(response_id: &str, message_id: &str, text: &str) -> Vec<StreamingSseChunk> {
    vec![
        chunk(ev_response_created(response_id)),
        chunk(ev_message_item_added(message_id, "")),
        chunk(ev_output_text_delta(text)),
        chunk(ev_final_answer_item_done(message_id, text)),
        chunk(ev_completed(response_id)),
    ]
}

async fn build_codex(server: &StreamingSseServer) -> Arc<CodexThread> {
    test_codex()
        .with_model("gpt-5.4")
        .build_with_streaming_server(server)
        .await
        .expect("build streaming Codex test session")
        .codex
}

async fn wait_for_request_count(server: &StreamingSseServer, expected: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if server.requests().await.len() >= expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {expected} model request(s)"));
}

async fn wait_for_mock_request_count(server: &MockServer, expected: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let count = server
                .received_requests()
                .await
                .expect("wiremock request log")
                .len();
            if count >= expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {expected} mock model request(s)"));
}

async fn rollout_reconstructs_pending_model_action(test: &TestCodex) -> bool {
    test.codex
        .flush_rollout()
        .await
        .expect("flush rollout before reconstruction");
    let rollout = tokio::fs::read_to_string(
        test.codex
            .rollout_path()
            .expect("materialized rollout path"),
    )
    .await
    .expect("read rollout for reconstruction");
    let items = rollout
        .lines()
        .filter_map(|line| serde_json::from_str::<RolloutLine>(line).ok())
        .map(|line| line.item)
        .collect::<Vec<_>>();
    reconstructed_inter_agent_has_pending_model_actions(test.session_configured.thread_id, &items)
        .await
}

#[derive(Clone, Copy)]
enum PreRestartBarrier {
    RequestError,
    Cancellation,
}

struct FailOrBlockThenBlock {
    barrier: PreRestartBarrier,
    calls: AtomicUsize,
}

impl Respond for FailOrBlockThenBlock {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 && matches!(self.barrier, PreRestartBarrier::RequestError) {
            return ResponseTemplate::new(500)
                .insert_header("content-type", "application/json")
                .set_body_json(json!({
                    "error": {
                        "type": "bad_request",
                        "message": "synthetic A5 request failure"
                    }
                }))
                .set_delay(Duration::from_millis(200));
        }
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_raw(
                responses::sse(vec![
                    ev_response_created("blocked-response"),
                    ev_completed("blocked-response"),
                ]),
                "text/event-stream",
            )
            .set_delay(Duration::from_secs(60))
    }
}

async fn submit_user_input(codex: &CodexThread, text: &str) {
    codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        }]))
        .await
        .expect("submit user input");
}

async fn submit_danger_full_access_user_turn(test: &TestCodex, text: &str) {
    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, test.config.cwd.as_path());
    test.codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                environments: Some(local_selections(test.config.cwd.clone())),
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                collaboration_mode: Some(CollaborationMode {
                    mode: ModeKind::Default,
                    settings: Settings {
                        model: test.session_configured.model.clone(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("submit user turn");
}

async fn steer_user_input(codex: &CodexThread, text: &str) {
    let submission = codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        }]))
        .await
        .expect("steer user input");
    assert!(matches!(submission, TurnInputSubmission::Steered { .. }));
}

async fn enqueue_queue_only_agent_mail(codex: &CodexThread, text: &str) {
    codex
        .submit(Op::InterAgentCommunication {
            communication: InterAgentCommunication::new(
                AgentPath::try_from("/root/worker").expect("worker path should parse"),
                AgentPath::root(),
                Vec::new(),
                text.to_string(),
                /*trigger_turn*/ false,
            ),
        })
        .await
        .expect("submit queue-only agent mail");
}

async fn submit_mode_agent_mail(
    codex: &CodexThread,
    message_id: &str,
    text: &str,
    delivery_mode: InterAgentDeliveryMode,
) {
    let communication = mode_agent_mail(message_id, text, delivery_mode);
    codex
        .submit(Op::InterAgentCommunication { communication })
        .await
        .expect("submit mode-aware agent mail");
}

fn mode_agent_mail(
    message_id: &str,
    text: &str,
    delivery_mode: InterAgentDeliveryMode,
) -> InterAgentCommunication {
    let mut communication = InterAgentCommunication::new_with_delivery_mode(
        AgentPath::try_from("/root/worker").expect("worker path should parse"),
        AgentPath::root(),
        Vec::new(),
        text.to_string(),
        delivery_mode,
    );
    communication.id = Some(ResponseItemId::from_server(message_id.to_string()));
    communication.external_message_id = Some(message_id.to_string());
    communication
}

async fn wait_for_a4(codex: &CodexThread, communication: &InterAgentCommunication) {
    let message_id = communication
        .external_message_id
        .clone()
        .expect("test communication external message ID");
    let query = InterAgentDeliveryQuery {
        message_id,
        semantic_sha256: inter_agent_semantic_sha256(communication),
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if matches!(
                codex
                    .inter_agent_delivery_status(std::slice::from_ref(&query))
                    .await[0]
                    .state,
                InterAgentDeliveryState::ContextPersisted(_)
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actionable message should reach A4");
}

fn request_has_agent_message_id(body: &Value, message_id: &str) -> bool {
    body.get("input")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("type").and_then(Value::as_str) == Some("agent_message")
                    && item.get("id").and_then(Value::as_str) == Some(message_id)
            })
        })
}

async fn wait_for_submission_barrier(codex: &CodexThread) {
    codex
        .submit(Op::RealtimeConversationListVoices)
        .await
        .expect("submit list-voices barrier");
    wait_for_event(codex, |event| {
        matches!(event, EventMsg::RealtimeConversationListVoicesResponse(_))
    })
    .await;
}

async fn submit_queue_only_agent_mail(codex: &CodexThread, text: &str) {
    enqueue_queue_only_agent_mail(codex, text).await;
    wait_for_submission_barrier(codex).await;
}

async fn wait_for_reasoning_item_started(codex: &CodexThread) {
    wait_for_event(codex, |event| {
        matches!(
            event,
            EventMsg::ItemStarted(item_started)
                if matches!(&item_started.item, TurnItem::Reasoning(_))
        )
    })
    .await;
}

async fn wait_for_agent_message(codex: &CodexThread, text: &str) {
    let final_message = wait_for_event(
        codex,
        |event| matches!(event, EventMsg::AgentMessage(message) if message.message == text),
    )
    .await;
    assert!(matches!(final_message, EventMsg::AgentMessage(_)));
}

async fn wait_for_turn_complete(codex: &CodexThread) {
    wait_for_event(codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;
}

async fn wait_for_inter_agent_message_completed(
    codex: &CodexThread,
    message_id: &str,
    expected_text: &str,
    expected_mode: InterAgentDeliveryMode,
) {
    let event = wait_for_event(codex, |event| {
        matches!(
            event,
            EventMsg::ItemCompleted(completed)
                if matches!(
                    &completed.item,
                    TurnItem::InterAgentMessage(item) if item.id == message_id
                )
        )
    })
    .await;
    let EventMsg::ItemCompleted(completed) = event else {
        unreachable!("wait predicate accepts only item/completed events");
    };
    let TurnItem::InterAgentMessage(item) = completed.item else {
        unreachable!("wait predicate accepts only inter-agent items");
    };
    assert_eq!(item.content, expected_text);
    assert_eq!(item.delivery_mode, expected_mode);
}

async fn wait_for_sleep_item_started(codex: &CodexThread, call_id: &str, duration_ms: u64) {
    let event = wait_for_event(codex, |event| {
        matches!(
            event,
            EventMsg::ItemStarted(started)
                if matches!(
                    &started.item,
                    TurnItem::Extension(ExtensionItem::Sleep(item)) if item.id == call_id
                )
        )
    })
    .await;
    let EventMsg::ItemStarted(started) = event else {
        unreachable!("wait predicate only accepts item/started events");
    };
    let TurnItem::Extension(ExtensionItem::Sleep(item)) = started.item else {
        unreachable!("wait predicate only accepts sleep items");
    };
    assert_eq!(
        item,
        SleepItem {
            id: call_id.to_string(),
            duration_ms,
        }
    );
}

async fn wait_for_sleep_item_completed(codex: &CodexThread, call_id: &str, duration_ms: u64) {
    let event = wait_for_event(codex, |event| {
        matches!(
            event,
            EventMsg::ItemCompleted(completed)
                if matches!(
                    &completed.item,
                    TurnItem::Extension(ExtensionItem::Sleep(item)) if item.id == call_id
                )
        )
    })
    .await;
    let EventMsg::ItemCompleted(completed) = event else {
        unreachable!("wait predicate only accepts item/completed events");
    };
    let TurnItem::Extension(ExtensionItem::Sleep(item)) = completed.item else {
        unreachable!("wait predicate only accepts sleep items");
    };
    assert_eq!(
        item,
        SleepItem {
            id: call_id.to_string(),
            duration_ms,
        }
    );
}

async fn assert_retryable_actionable_a4_reserves_one_a5(delivery_mode: InterAgentDeliveryMode) {
    let message_id = format!("amsg_retryable_a5_{delivery_mode:?}").to_ascii_lowercase();
    let (server, _completions) = start_streaming_sse_server(vec![
        final_answer_chunks("resp-before-a4", "msg-before-a4", "first request done"),
        final_answer_chunks("resp-after-a4", "msg-after-a4", "action handled"),
        final_answer_chunks("unexpected-resp", "unexpected-msg", "unexpected"),
    ])
    .await;
    let codex = build_codex(&server).await;
    inject_inter_agent_persistence_failures(&codex, 0, u64::MAX, 0);
    let communication = mode_agent_mail(&message_id, "actionable retry payload", delivery_mode);

    codex
        .submit(Op::InterAgentCommunication {
            communication: communication.clone(),
        })
        .await
        .expect("submit actionable mail");
    wait_for_request_count(&server, 1).await;

    let first: Value = from_slice(&server.requests().await[0]).expect("parse first request");
    assert!(
        !request_has_agent_message_id(&first, &message_id),
        "the controlled first snapshot must precede A4"
    );
    let query = InterAgentDeliveryQuery {
        message_id: message_id.clone(),
        semantic_sha256: inter_agent_semantic_sha256(&communication),
    };
    assert_eq!(
        codex
            .inter_agent_delivery_status(std::slice::from_ref(&query))
            .await[0]
            .state,
        InterAgentDeliveryState::RetryableError
    );

    for _ in 0..8 {
        codex
            .submit(Op::InterAgentCommunication {
                communication: communication.clone(),
            })
            .await
            .expect("submit concurrent duplicate");
    }
    wait_for_turn_complete(&codex).await;
    assert_eq!(
        server.requests().await.len(),
        1,
        "the terminal first response must not invent an A5 request before A4"
    );

    inject_inter_agent_persistence_failures(&codex, 0, 0, 0);
    wait_for_a4(&codex, &communication).await;
    wait_for_request_count(&server, 2).await;
    wait_for_turn_complete(&codex).await;
    wait_for_submission_barrier(&codex).await;

    let requests = server.requests().await;
    assert_eq!(
        requests.len(),
        2,
        "one A4 obligation must reserve exactly one successor request"
    );
    let second: Value = from_slice(&requests[1]).expect("parse successor request");
    assert!(
        request_has_agent_message_id(&second, &message_id),
        "successor request missing message ID {message_id}: {second}"
    );
    assert_eq!(
        agent_message_input_texts(&second),
        vec!["actionable retry payload"]
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retryable_actionable_a4_reserves_exactly_one_a5_request() {
    for delivery_mode in [
        InterAgentDeliveryMode::Soon,
        InterAgentDeliveryMode::AfterTurn,
        InterAgentDeliveryMode::Interrupt,
    ] {
        assert_retryable_actionable_a4_reserves_one_a5(delivery_mode).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retryable_passive_a4_does_not_reserve_a5_and_waits_for_regular_work() {
    let (server, _completions) = start_streaming_sse_server(vec![
        final_answer_chunks("resp-1", "msg-1", "first regular work done"),
        final_answer_chunks("resp-2", "msg-2", "second regular work done"),
    ])
    .await;
    let codex = build_codex(&server).await;
    inject_inter_agent_persistence_failures(&codex, 0, u64::MAX, 0);
    let communication = mode_agent_mail(
        "amsg_retryable_passive_a5",
        "passive retry payload",
        InterAgentDeliveryMode::Passive,
    );

    codex
        .submit(Op::InterAgentCommunication {
            communication: communication.clone(),
        })
        .await
        .expect("submit passive mail");
    wait_for_submission_barrier(&codex).await;
    assert!(server.requests().await.is_empty());

    submit_user_input(&codex, "first unrelated work").await;
    wait_for_request_count(&server, 1).await;
    let first: Value = from_slice(&server.requests().await[0]).expect("parse first request");
    assert!(!request_has_agent_message_id(
        &first,
        "amsg_retryable_passive_a5"
    ));
    wait_for_turn_complete(&codex).await;

    inject_inter_agent_persistence_failures(&codex, 0, 0, 0);
    wait_for_a4(&codex, &communication).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        server.requests().await.len(),
        1,
        "Passive A4 must not wake an idle session"
    );

    submit_user_input(&codex, "second unrelated work").await;
    wait_for_request_count(&server, 2).await;
    wait_for_turn_complete(&codex).await;
    let second: Value = from_slice(&server.requests().await[1]).expect("parse second request");
    assert!(request_has_agent_message_id(
        &second,
        "amsg_retryable_passive_a5"
    ));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actionable_a4_survives_an_empty_model_completion_until_nonempty_a5() {
    let (server, _completions) = start_streaming_sse_server(vec![
        response_completed_chunks("empty-resp"),
        final_answer_chunks("nonempty-resp", "nonempty-msg", "action handled"),
        final_answer_chunks("unexpected-resp", "unexpected-msg", "unexpected"),
    ])
    .await;
    let codex = build_codex(&server).await;
    let communication = mode_agent_mail(
        "amsg_empty_then_a5",
        "must receive nonempty outcome",
        InterAgentDeliveryMode::Soon,
    );

    codex
        .submit(Op::InterAgentCommunication {
            communication: communication.clone(),
        })
        .await
        .expect("submit actionable mail");
    wait_for_a4(&codex, &communication).await;
    wait_for_request_count(&server, 2).await;
    wait_for_turn_complete(&codex).await;
    wait_for_submission_barrier(&codex).await;

    let requests = server.requests().await;
    assert_eq!(
        requests.len(),
        2,
        "an empty successful response must restore, not discharge, the A5 obligation"
    );
    for request in requests {
        let body: Value = from_slice(&request).expect("parse A5 request");
        assert!(request_has_agent_message_id(&body, "amsg_empty_then_a5"));
    }

    server.shutdown().await;
}

async fn assert_explicit_a5_completion_survives_restart(
    barrier: PreRestartBarrier,
) -> anyhow::Result<()> {
    let old_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(FailOrBlockThenBlock {
            barrier,
            calls: AtomicUsize::new(0),
        })
        .expect(2)
        .mount(&old_server)
        .await;
    let test = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
        })
        .build_with_auto_env(&old_server)
        .await?;
    let message_id = match barrier {
        PreRestartBarrier::RequestError => "amsg_a5_error_steer_restart",
        PreRestartBarrier::Cancellation => "amsg_a5_cancel_steer_restart",
    };
    let communication = mode_agent_mail(
        message_id,
        "explicit completion required",
        InterAgentDeliveryMode::Soon,
    );
    test.codex
        .submit(Op::InterAgentCommunication {
            communication: communication.clone(),
        })
        .await?;
    wait_for_mock_request_count(&old_server, 1).await;
    steer_user_input(&test.codex, "same-turn steer cannot complete A5").await;
    if matches!(barrier, PreRestartBarrier::Cancellation) {
        test.codex.submit(Op::Interrupt).await?;
    }

    // The ordinary live recovery request is deliberately held open. Its schedule record and the
    // same-turn steer are durable, but neither is an explicit completion proof.
    wait_for_mock_request_count(&old_server, 2).await;
    assert!(
        rollout_reconstructs_pending_model_action(&test).await,
        "restart reconstruction must retain A5 after {message_id} without completion proof"
    );

    let success_server = MockServer::start().await;
    responses::mount_response_once(
        &success_server,
        responses::sse_response(responses::sse(vec![
            ev_response_created("recovered-a5"),
            ev_message_item_added("recovered-a5-message", ""),
            ev_output_text_delta("explicitly completed"),
            ev_final_answer_item_done("recovered-a5-message", "explicitly completed"),
            ev_completed("recovered-a5"),
        ]))
        .set_delay(Duration::from_millis(200)),
    )
    .await;
    let mut restart_builder = test_codex().with_model("gpt-5.4").with_config(|config| {
        config.model_provider.request_max_retries = Some(0);
        config.model_provider.stream_max_retries = Some(0);
    });
    let resumed = restart_builder.restart(&success_server, &test).await?;
    wait_for_turn_complete(&resumed.codex).await;
    wait_for_mock_request_count(&success_server, 1).await;
    let recovery_requests = success_server
        .received_requests()
        .await
        .expect("recovery request log");
    assert_eq!(recovery_requests.len(), 1);
    let recovery: Value = from_slice(&recovery_requests[0].body)?;
    assert!(request_has_agent_message_id(&recovery, message_id));
    assert!(
        !rollout_reconstructs_pending_model_action(&resumed).await,
        "flushed explicit completion must discharge A5"
    );

    let duplicate_restart_server = MockServer::start().await;
    let mut duplicate_restart_builder = test_codex().with_model("gpt-5.4");
    let resumed_again = duplicate_restart_builder
        .restart(&duplicate_restart_server, &resumed)
        .await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        duplicate_restart_server
            .received_requests()
            .await
            .expect("duplicate restart request log")
            .is_empty(),
        "explicit completion must prevent a duplicate recovery request"
    );
    resumed_again.codex.shutdown_and_wait().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_error_and_same_turn_steer_require_explicit_a5_completion_after_restart()
-> anyhow::Result<()> {
    assert_explicit_a5_completion_survives_restart(PreRestartBarrier::RequestError).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_and_same_turn_steer_require_explicit_a5_completion_after_restart()
-> anyhow::Result<()> {
    assert_explicit_a5_completion_survives_restart(PreRestartBarrier::Cancellation).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_mode_idle_after_turn_starts_once_and_deduplicates() {
    let (server, _completions) = start_streaming_sse_server(vec![
        final_answer_chunks("resp-1", "msg-1", "done"),
        final_answer_chunks("unexpected-resp-2", "unexpected-msg-2", "unexpected"),
    ])
    .await;
    let codex = build_codex(&server).await;

    submit_mode_agent_mail(
        &codex,
        "message-idle-after-turn",
        "idle after turn",
        InterAgentDeliveryMode::AfterTurn,
    )
    .await;
    submit_mode_agent_mail(
        &codex,
        "message-idle-after-turn",
        "idle after turn",
        InterAgentDeliveryMode::AfterTurn,
    )
    .await;

    wait_for_inter_agent_message_completed(
        &codex,
        "message-idle-after-turn",
        "idle after turn",
        InterAgentDeliveryMode::AfterTurn,
    )
    .await;
    wait_for_turn_complete(&codex).await;
    wait_for_submission_barrier(&codex).await;

    let requests = server.requests().await;
    assert_eq!(
        requests.len(),
        1,
        "duplicate mail must not start another turn"
    );
    let request: Value = from_slice(&requests[0]).expect("parse request");
    assert_eq!(agent_message_input_texts(&request), vec!["idle after turn"]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_mode_active_after_turn_batches_fifo_in_one_follow_up() {
    let (gate_first_done_tx, gate_first_done_rx) = oneshot::channel();
    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        gated_chunk(
            gate_first_done_rx,
            vec![
                ev_message_item_added("msg-1", ""),
                ev_output_text_delta("first done"),
                ev_final_answer_item_done("msg-1", "first done"),
                ev_completed("resp-1"),
            ],
        ),
    ];
    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        final_answer_chunks("resp-2", "msg-2", "follow-up done"),
        final_answer_chunks("unexpected-resp-3", "unexpected-msg-3", "unexpected"),
    ])
    .await;
    let codex = build_codex(&server).await;

    submit_user_input(&codex, "active prompt").await;
    wait_for_request_count(&server, 1).await;

    submit_mode_agent_mail(
        &codex,
        "message-after-one",
        "after one",
        InterAgentDeliveryMode::AfterTurn,
    )
    .await;
    submit_mode_agent_mail(
        &codex,
        "message-after-two",
        "after two",
        InterAgentDeliveryMode::AfterTurn,
    )
    .await;
    wait_for_submission_barrier(&codex).await;
    assert_eq!(server.requests().await.len(), 1);

    let _ = gate_first_done_tx.send(());
    let mut timeline = Vec::new();
    let mut turn_completions = 0;
    let mut completed_mail = Vec::new();
    while turn_completions < 2 || completed_mail.len() < 2 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), codex.next_event())
            .await
            .unwrap_or_else(|_| panic!("timed out collecting timeline: {timeline:?}"))
            .expect("event channel should remain open")
            .msg;
        match event {
            EventMsg::RawResponseCompleted(event) => {
                timeline.push(format!("response:{}", event.response_id));
            }
            EventMsg::TurnStarted(_) => timeline.push("turn_started".to_string()),
            EventMsg::TurnComplete(_) => {
                turn_completions += 1;
                timeline.push("turn_complete".to_string());
            }
            EventMsg::ItemCompleted(completed) => {
                if let TurnItem::InterAgentMessage(item) = completed.item {
                    assert_eq!(item.delivery_mode, InterAgentDeliveryMode::AfterTurn);
                    timeline.push(format!("mail:{}", item.id));
                    completed_mail.push(item.content);
                }
            }
            _ => {}
        }
    }
    assert_eq!(completed_mail, vec!["after one", "after two"]);

    let requests = server.requests().await;
    let request_agent_messages = requests
        .iter()
        .map(|request| {
            let request: Value = from_slice(request).expect("parse request for diagnostics");
            agent_message_input_texts(&request)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        requests.len(),
        2,
        "retained mail should use one follow-up; agent inputs: {request_agent_messages:?}; timeline: {timeline:?}"
    );
    let first: Value = from_slice(&requests[0]).expect("parse first request");
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    assert!(agent_message_input_texts(&first).is_empty());
    assert_eq!(
        agent_message_input_texts(&second),
        vec!["after one", "after two"]
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_mode_after_turn_completion_race_starts_one_follow_up() {
    let (gate_first_done_tx, gate_first_done_rx) = oneshot::channel();
    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        gated_chunk(
            gate_first_done_rx,
            vec![
                ev_message_item_added("msg-1", ""),
                ev_output_text_delta("first done"),
                ev_final_answer_item_done("msg-1", "first done"),
                ev_completed("resp-1"),
            ],
        ),
    ];
    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        final_answer_chunks("resp-2", "msg-2", "follow-up done"),
        final_answer_chunks("unexpected-resp-3", "unexpected-msg-3", "unexpected"),
    ])
    .await;
    let codex = build_codex(&server).await;

    submit_user_input(&codex, "active prompt").await;
    wait_for_request_count(&server, 1).await;

    let submit_mail = submit_mode_agent_mail(
        &codex,
        "message-after-race",
        "after race",
        InterAgentDeliveryMode::AfterTurn,
    );
    let release_completion = async move {
        let _ = gate_first_done_tx.send(());
    };
    tokio::join!(submit_mail, release_completion);

    wait_for_inter_agent_message_completed(
        &codex,
        "message-after-race",
        "after race",
        InterAgentDeliveryMode::AfterTurn,
    )
    .await;
    wait_for_turn_complete(&codex).await;
    wait_for_submission_barrier(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 2, "race must reserve only one follow-up");
    let first: Value = from_slice(&requests[0]).expect("parse first request");
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    assert!(agent_message_input_texts(&first).is_empty());
    assert_eq!(agent_message_input_texts(&second), vec!["after race"]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_mode_passive_waits_idle_and_piggybacks() {
    let (server, _completions) =
        start_streaming_sse_server(vec![final_answer_chunks("resp-1", "msg-1", "done")]).await;
    let codex = build_codex(&server).await;

    submit_mode_agent_mail(
        &codex,
        "message-passive",
        "passive update",
        InterAgentDeliveryMode::Passive,
    )
    .await;
    wait_for_submission_barrier(&codex).await;
    assert!(
        server.requests().await.is_empty(),
        "passive mail must not wake an idle thread"
    );

    submit_user_input(&codex, "consume passive mail").await;
    wait_for_inter_agent_message_completed(
        &codex,
        "message-passive",
        "passive update",
        InterAgentDeliveryMode::Passive,
    )
    .await;
    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 1);
    let request: Value = from_slice(&requests[0]).expect("parse request");
    assert_eq!(agent_message_input_texts(&request), vec!["passive update"]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_mode_soon_uses_next_active_model_boundary() {
    let (gate_first_done_tx, gate_first_done_rx) = oneshot::channel();
    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_reasoning_item_added("reason-1", &["thinking"])),
        gated_chunk(
            gate_first_done_rx,
            vec![
                ev_reasoning_item("reason-1", &["thinking"], &[]),
                ev_message_item_added("msg-1", ""),
                ev_output_text_delta("first boundary"),
                ev_final_answer_item_done("msg-1", "first boundary"),
                ev_completed("resp-1"),
            ],
        ),
    ];
    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        final_answer_chunks("resp-2", "msg-2", "continued"),
    ])
    .await;
    let codex = build_codex(&server).await;

    submit_user_input(&codex, "active prompt").await;
    wait_for_reasoning_item_started(&codex).await;
    submit_mode_agent_mail(
        &codex,
        "message-soon",
        "soon update",
        InterAgentDeliveryMode::Soon,
    )
    .await;
    wait_for_submission_barrier(&codex).await;

    let _ = gate_first_done_tx.send(());
    wait_for_inter_agent_message_completed(
        &codex,
        "message-soon",
        "soon update",
        InterAgentDeliveryMode::Soon,
    )
    .await;
    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 2, "soon should continue the active turn");
    let first: Value = from_slice(&requests[0]).expect("parse first request");
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    assert!(agent_message_input_texts(&first).is_empty());
    assert_eq!(agent_message_input_texts(&second), vec!["soon update"]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_mode_interrupt_aborts_once_and_runs_one_replacement() {
    let (gate_first_done_tx, gate_first_done_rx) = oneshot::channel();
    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_reasoning_item_added("reason-1", &["thinking"])),
        gated_chunk(
            gate_first_done_rx,
            vec![
                ev_reasoning_item("reason-1", &["thinking"], &[]),
                ev_completed("resp-1"),
            ],
        ),
    ];
    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        final_answer_chunks("resp-2", "msg-2", "replacement done"),
        final_answer_chunks("unexpected-resp-3", "unexpected-msg-3", "unexpected"),
    ])
    .await;
    let codex = build_codex(&server).await;

    submit_user_input(&codex, "active prompt").await;
    wait_for_reasoning_item_started(&codex).await;
    submit_mode_agent_mail(
        &codex,
        "message-interrupt",
        "interrupt update",
        InterAgentDeliveryMode::Interrupt,
    )
    .await;

    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnAborted(_))).await;
    let _ = gate_first_done_tx.send(());
    wait_for_inter_agent_message_completed(
        &codex,
        "message-interrupt",
        "interrupt update",
        InterAgentDeliveryMode::Interrupt,
    )
    .await;
    wait_for_turn_complete(&codex).await;
    wait_for_submission_barrier(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 2, "interrupt should create one replacement");
    let first: Value = from_slice(&requests[0]).expect("parse first request");
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    assert!(agent_message_input_texts(&first).is_empty());
    assert_eq!(agent_message_input_texts(&second), vec!["interrupt update"]);

    server.shutdown().await;
}

struct SleepingRootExtension;

impl codex_extension_api::ThreadLifecycleContributor<codex_core::config::Config>
    for SleepingRootExtension
{
    fn on_thread_start<'a>(
        &'a self,
        input: codex_extension_api::ThreadStartInput<'a, codex_core::config::Config>,
    ) -> codex_extension_api::ExtensionFuture<'a, ()> {
        Box::pin(async move {
            input.thread_store.insert(SleepItem {
                id: "clock-wait-1".to_string(),
                duration_ms: 60_000,
            });
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queue_only_agent_mail_wakes_sleeping_root_and_persists_message() {
    const CHILD_MESSAGE: &str = "worker completed";

    let (server, _completions) =
        start_streaming_sse_server(vec![response_completed_chunks("resp-1")]).await;
    let mut extensions =
        codex_extension_api::ExtensionRegistryBuilder::<codex_core::config::Config>::new();
    extensions.thread_lifecycle_contributor(Arc::new(SleepingRootExtension));
    let codex = test_codex()
        .with_model("gpt-5.4")
        .with_extensions(Arc::new(extensions.build()))
        .build_with_streaming_server(&server)
        .await
        .expect("build Codex test session")
        .codex;

    enqueue_queue_only_agent_mail(&codex, CHILD_MESSAGE).await;
    wait_for_turn_complete(&codex).await;

    assert_eq!(server.requests().await.len(), 1);
    let history = codex
        .load_history(/*include_archived*/ true)
        .await
        .expect("load persisted thread history");
    assert!(history.items.iter().any(|item| {
        matches!(
            item,
            RolloutItem::ResponseItem(envelope)
                if matches!(
                    &envelope.item,
                    codex_protocol::models::ResponseItem::AgentMessage { content, .. }
                        if content.iter().any(|content| matches!(
                            content,
                            codex_protocol::models::AgentMessageInputContent::InputText { text }
                                if text == CHILD_MESSAGE
                        ))
                )
        )
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn steer_interrupts_wait_agent_and_is_sent_in_follow_up_request() {
    const WAIT_CALL_ID: &str = "wait-call";
    const INITIAL_PROMPT: &str = "wait for an agent";
    const STEER_PROMPT: &str = "stop waiting and continue";
    const MULTI_AGENT_V2_NAMESPACE: &str = "collaboration";

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_function_call_with_namespace(
            WAIT_CALL_ID,
            MULTI_AGENT_V2_NAMESPACE,
            "wait_agent",
            r#"{"timeout_ms":10000}"#,
        )),
        chunk(ev_completed("resp-1")),
    ];
    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, response_completed_chunks("resp-2")]).await;
    let codex = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config
                .features
                .enable(Feature::MultiAgentV2)
                .expect("test config should allow feature update");
        })
        .build_with_streaming_server(&server)
        .await
        .expect("build Codex test session")
        .codex;

    submit_user_input(&codex, INITIAL_PROMPT).await;
    wait_for_event(&codex, |event| {
        matches!(event, EventMsg::CollabWaitingBegin(_))
    })
    .await;

    steer_user_input(&codex, STEER_PROMPT).await;
    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 2);
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    let relevant_user_input = message_input_texts(&second, "user")
        .into_iter()
        .filter(|text| text == INITIAL_PROMPT || text == STEER_PROMPT)
        .collect::<Vec<_>>();
    assert_eq!(
        relevant_user_input,
        vec![INITIAL_PROMPT.to_string(), STEER_PROMPT.to_string()]
    );
    let wait_output = function_call_output_text(&second, WAIT_CALL_ID).expect("wait_agent output");
    assert_eq!(
        serde_json::from_str::<Value>(wait_output).expect("parse wait_agent output"),
        json!({
            "message": "Wait interrupted by new input.",
            "timed_out": false,
        })
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn any_new_input_interrupts_sleep() {
    const FIRST_SLEEP_CALL_ID: &str = "sleep-call-1";
    const SECOND_SLEEP_CALL_ID: &str = "sleep-call-2";
    const SLEEP_DURATION_MS: u64 = 3_600_000;
    const INITIAL_PROMPT: &str = "sleep for a while";
    const STEER_PROMPT: &str = "stop sleeping and continue";
    let sleep_arguments = json!({ "duration_ms": SLEEP_DURATION_MS }).to_string();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_function_call_with_namespace(
            FIRST_SLEEP_CALL_ID,
            "clock",
            "sleep",
            &sleep_arguments,
        )),
        chunk(ev_completed("resp-1")),
    ];
    let second_chunks = vec![
        chunk(ev_response_created("resp-2")),
        chunk(ev_function_call_with_namespace(
            SECOND_SLEEP_CALL_ID,
            "clock",
            "sleep",
            &sleep_arguments,
        )),
        chunk(ev_completed("resp-2")),
    ];
    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        second_chunks,
        response_completed_chunks("resp-3"),
    ])
    .await;
    let codex = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config
                .features
                .enable(Feature::CurrentTimeReminder)
                .expect("test config should allow current-time reminders");
            config.current_time_reminder = Some(CurrentTimeReminderConfig {
                sleep_tool: true,
                ..CurrentTimeReminderConfig::default()
            });
        })
        .build_with_streaming_server(&server)
        .await
        .expect("build Codex test session")
        .codex;

    submit_user_input(&codex, INITIAL_PROMPT).await;
    wait_for_sleep_item_started(&codex, FIRST_SLEEP_CALL_ID, SLEEP_DURATION_MS).await;

    steer_user_input(&codex, STEER_PROMPT).await;
    wait_for_sleep_item_completed(&codex, FIRST_SLEEP_CALL_ID, SLEEP_DURATION_MS).await;
    wait_for_sleep_item_started(&codex, SECOND_SLEEP_CALL_ID, SLEEP_DURATION_MS).await;

    submit_queue_only_agent_mail(&codex, "new mailbox input").await;
    wait_for_sleep_item_completed(&codex, SECOND_SLEEP_CALL_ID, SLEEP_DURATION_MS).await;
    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 3);
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    let relevant_user_input = message_input_texts(&second, "user")
        .into_iter()
        .filter(|text| text == INITIAL_PROMPT || text == STEER_PROMPT)
        .collect::<Vec<_>>();
    assert_eq!(
        relevant_user_input,
        vec![INITIAL_PROMPT.to_string(), STEER_PROMPT.to_string()]
    );
    assert_interrupted_sleep_output(function_call_output_text(&second, FIRST_SLEEP_CALL_ID));

    let third: Value = from_slice(&requests[2]).expect("parse third request");
    assert_interrupted_sleep_output(function_call_output_text(&third, SECOND_SLEEP_CALL_ID));

    codex.submit(Op::Shutdown).await.expect("shutdown session");
    wait_for_event(&codex, |event| matches!(event, EventMsg::ShutdownComplete)).await;

    let rollout_path = codex.rollout_path().expect("rollout path");
    let rollout = tokio::fs::read_to_string(rollout_path)
        .await
        .expect("read rollout");
    let persisted_sleep_items = rollout
        .lines()
        .filter_map(|line| serde_json::from_str::<RolloutLine>(line).ok())
        .filter_map(|line| match line.item {
            RolloutItem::EventMsg(EventMsg::ItemCompleted(event)) => match event.item {
                TurnItem::Extension(ExtensionItem::Sleep(item)) => Some(item),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted_sleep_items,
        vec![
            SleepItem {
                id: FIRST_SLEEP_CALL_ID.to_string(),
                duration_ms: SLEEP_DURATION_MS,
            },
            SleepItem {
                id: SECOND_SLEEP_CALL_ID.to_string(),
                duration_ms: SLEEP_DURATION_MS,
            },
        ]
    );

    server.shutdown().await;
}

fn assert_two_responses_input_snapshot(snapshot_name: &str, requests: &[Vec<u8>]) {
    assert_eq!(requests.len(), 2);
    let options = ContextSnapshotOptions::default().strip_capability_instructions();
    let first: Value = from_slice(&requests[0]).expect("parse first request");
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    let first_items = first["input"]
        .as_array()
        .expect("first request input")
        .clone();
    let second_items = second["input"]
        .as_array()
        .expect("second request input")
        .clone();
    let snapshot = context_snapshot::format_labeled_items_snapshot(
        "/responses POST bodies (input only, redacted like other suite snapshots)",
        &[
            ("First request", first_items.as_slice()),
            ("Second request", second_items.as_slice()),
        ],
        &options,
    );
    insta::assert_snapshot!(snapshot_name, snapshot);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "TODO(aibrahim): flaky"]
async fn injected_user_input_triggers_follow_up_request_with_deltas() {
    let (gate_completed_tx, gate_completed_rx) = oneshot::channel();

    let first_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_response_created("resp-1")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_message_item_added("msg-1", "")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_output_text_delta("first ")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_output_text_delta("turn")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_message_item_done("msg-1", "first turn")),
        },
        StreamingSseChunk {
            gate: Some(gate_completed_rx),
            body: sse_event(ev_completed("resp-1")),
        },
    ];

    let second_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_response_created("resp-2")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_completed("resp-2")),
        },
    ];

    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, second_chunks]).await;

    let codex = test_codex()
        .with_model("gpt-5.4")
        .build_with_streaming_server(&server)
        .await
        .unwrap()
        .codex;

    codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "first prompt".into(),
            text_elements: Vec::new(),
        }]))
        .await
        .unwrap();

    wait_for_event(&codex, |event| {
        matches!(event, EventMsg::AgentMessageContentDelta(_))
    })
    .await;

    codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "second prompt".into(),
            text_elements: Vec::new(),
        }]))
        .await
        .unwrap();

    let _ = gate_completed_tx.send(());

    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 2);

    let first_body: Value = serde_json::from_slice(&requests[0]).expect("parse first request");
    let second_body: Value = serde_json::from_slice(&requests[1]).expect("parse second request");

    let first_texts = message_input_texts(&first_body, "user");
    assert!(first_texts.iter().any(|text| text == "first prompt"));
    assert!(!first_texts.iter().any(|text| text == "second prompt"));

    let second_texts = message_input_texts(&second_body, "user");
    assert!(second_texts.iter().any(|text| text == "first prompt"));
    assert!(second_texts.iter().any(|text| text == "second prompt"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_inter_agent_mail_triggers_follow_up_after_reasoning_item() {
    let (gate_reasoning_done_tx, gate_reasoning_done_rx) = oneshot::channel();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_reasoning_item_added("reason-1", &["thinking"])),
        gated_chunk(
            gate_reasoning_done_rx,
            vec![
                ev_reasoning_item("reason-1", &["thinking"], &[]),
                ev_function_call(
                    "call-stale",
                    "shell",
                    r#"{"command":"echo stale tool call"}"#,
                ),
                ev_message_item_added("msg-stale", ""),
                ev_output_text_delta("stale final"),
                ev_message_item_done("msg-stale", "stale final"),
                ev_completed("resp-1"),
            ],
        ),
    ];

    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, response_completed_chunks("resp-2")]).await;

    let codex = build_codex(&server).await;

    submit_user_input(&codex, "first prompt").await;

    wait_for_reasoning_item_started(&codex).await;

    submit_queue_only_agent_mail(&codex, "queued child update").await;

    let _ = gate_reasoning_done_tx.send(());

    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_two_responses_input_snapshot("pending_input_queued_mail_after_reasoning", &requests);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_inter_agent_mail_triggers_follow_up_after_commentary_message_item() {
    let (gate_message_done_tx, gate_message_done_rx) = oneshot::channel();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_message_item_added("msg-1", "")),
        gated_chunk(
            gate_message_done_rx,
            vec![
                ev_output_text_delta("first answer"),
                json!({
                    "type": "response.output_item.done",
                    "item": {
                        "type": "message",
                        "role": "assistant",
                        "id": "msg-1",
                        "content": [{"type": "output_text", "text": "first answer"}],
                        "phase": "commentary",
                    }
                }),
                ev_function_call(
                    "call-stale",
                    "shell",
                    r#"{"command":"echo stale tool call"}"#,
                ),
                ev_message_item_added("msg-stale", ""),
                ev_output_text_delta("stale final"),
                ev_message_item_done("msg-stale", "stale final"),
                ev_completed("resp-1"),
            ],
        ),
    ];

    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, response_completed_chunks("resp-2")]).await;

    let codex = build_codex(&server).await;

    submit_user_input(&codex, "first prompt").await;

    wait_for_event(&codex, |event| {
        matches!(
            event,
            EventMsg::ItemStarted(item_started)
                if matches!(&item_started.item, TurnItem::AgentMessage(_))
        )
    })
    .await;

    submit_queue_only_agent_mail(&codex, "queued child update").await;

    let _ = gate_message_done_tx.send(());

    wait_for_agent_message(&codex, "first answer").await;

    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_two_responses_input_snapshot("pending_input_queued_mail_after_commentary", &requests);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_inter_agent_mail_piggybacks_once_without_restart() {
    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_message_item_added("msg-1", "")),
        chunk(ev_output_text_delta("first answer")),
        chunk(json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "id": "msg-1",
                "content": [{"type": "output_text", "text": "first answer"}],
                "phase": "final_answer",
            }
        })),
        chunk(ev_completed("resp-1")),
    ];

    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        final_answer_chunks("resp-2", "msg-2", "second answer"),
    ])
    .await;
    let codex = build_codex(&server).await;

    submit_queue_only_agent_mail(&codex, "queued child update").await;
    submit_user_input(&codex, "first prompt").await;
    wait_for_turn_complete(&codex).await;
    wait_for_submission_barrier(&codex).await;

    let mut requests = server.requests().await;
    assert_eq!(requests.len(), 1);
    let request: Value = from_slice(&requests[0]).expect("parse request");
    assert_eq!(
        agent_message_input_texts(&request),
        vec!["queued child update"]
    );

    submit_user_input(&codex, "second prompt").await;
    wait_for_turn_complete(&codex).await;

    requests = server.requests().await;
    assert_eq!(requests.len(), 2);
    let request: Value = from_slice(&requests[1]).expect("parse request");
    assert_eq!(
        agent_message_input_texts(&request),
        vec!["queued child update"],
        "consumed passive mail should remain once in history without duplication"
    );
    let user_input = message_input_texts(&request, "user")
        .into_iter()
        .filter(|text| text == "second prompt")
        .collect::<Vec<_>>();
    assert_eq!(user_input, vec!["second prompt"]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injected_response_item_reopens_turn_after_final_answer() {
    const INITIAL_PROMPT: &str = "first prompt";
    const INJECTED_CONTEXT: &str = "late injected context";
    let (gate_completed_tx, gate_completed_rx) = oneshot::channel();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_message_item_added("msg-1", "")),
        chunk(ev_output_text_delta("first answer")),
        chunk(json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "id": "msg-1",
                "content": [{"type": "output_text", "text": "first answer"}],
                "phase": "final_answer",
            }
        })),
        // Keep the response open past an observable event so the answer boundary is established
        // before the late context is injected.
        chunk(ev_reasoning_item_added("reason-after-final", &["done"])),
        gated_chunk(
            gate_completed_rx,
            vec![
                ev_reasoning_item("reason-after-final", &["done"], &[]),
                ev_completed("resp-1"),
            ],
        ),
    ];
    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, response_completed_chunks("resp-2")]).await;
    let codex = build_codex(&server).await;

    submit_user_input(&codex, INITIAL_PROMPT).await;
    wait_for_reasoning_item_started(&codex).await;

    assert!(
        codex
            .inject_if_running(vec![responses::user_message_item(INJECTED_CONTEXT)])
            .await
            .is_ok()
    );
    let _ = gate_completed_tx.send(());

    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 2);
    let second: Value = from_slice(&requests[1]).expect("parse second request");
    let relevant_user_input = message_input_texts(&second, "user")
        .into_iter()
        .filter(|text| text == INITIAL_PROMPT || text == INJECTED_CONTEXT)
        .collect::<Vec<_>>();
    assert_eq!(relevant_user_input, vec![INITIAL_PROMPT, INJECTED_CONTEXT]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_input_does_not_preempt_after_reasoning_item() {
    let (gate_reasoning_done_tx, gate_reasoning_done_rx) = oneshot::channel();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_reasoning_item_added("reason-1", &["thinking"])),
        gated_chunk(
            gate_reasoning_done_rx,
            vec![
                ev_reasoning_item("reason-1", &["thinking"], &[]),
                ev_function_call(
                    "call-preserved",
                    "shell",
                    r#"{"command":"echo preserved tool call"}"#,
                ),
                ev_message_item_added("msg-1", ""),
                ev_output_text_delta("first answer"),
                ev_message_item_done("msg-1", "first answer"),
                ev_completed("resp-1"),
            ],
        ),
    ];

    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, response_completed_chunks("resp-2")]).await;

    let codex = build_codex(&server).await;

    submit_user_input(&codex, "first prompt").await;

    wait_for_reasoning_item_started(&codex).await;

    steer_user_input(&codex, "second prompt").await;

    let _ = gate_reasoning_done_tx.send(());

    wait_for_agent_message(&codex, "first answer").await;

    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_two_responses_input_snapshot(
        "pending_input_user_input_no_preempt_after_reasoning",
        &requests,
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn steered_user_input_waits_for_model_continuation_after_mid_turn_compact() {
    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_function_call("call-1", "test_tool", "{}")),
        chunk(ev_completed_with_tokens(
            "resp-1", /*total_tokens*/ 500,
        )),
    ];

    let compact_chunks = vec![
        chunk(ev_response_created("resp-compact")),
        chunk(ev_message_item_done("msg-compact", "AUTO_COMPACT_SUMMARY")),
        chunk(ev_completed_with_tokens(
            "resp-compact",
            /*total_tokens*/ 50,
        )),
    ];

    let post_compact_continuation_chunks = vec![
        chunk(ev_response_created("resp-post-compact")),
        chunk(ev_message_item_added("msg-post-compact", "")),
        chunk(ev_output_text_delta("resumed old task")),
        chunk(ev_message_item_done("msg-post-compact", "resumed old task")),
        chunk(ev_completed_with_tokens(
            "resp-post-compact",
            /*total_tokens*/ 60,
        )),
    ];

    let steered_follow_up_chunks = vec![
        chunk(ev_response_created("resp-steered")),
        chunk(ev_message_item_done(
            "msg-steered",
            "processed steered prompt",
        )),
        chunk(ev_completed_with_tokens(
            "resp-steered",
            /*total_tokens*/ 70,
        )),
    ];

    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        compact_chunks,
        post_compact_continuation_chunks,
        steered_follow_up_chunks,
    ])
    .await;

    let codex = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config.model_provider.name = "OpenAI (test)".to_string();
            config.model_provider.supports_websockets = false;
            config.model_auto_compact_token_limit = Some(200);
        })
        .build_with_streaming_server(&server)
        .await
        .expect("build streaming Codex test session")
        .codex;

    submit_user_input(&codex, "first prompt").await;
    submit_user_input(&codex, "second prompt").await;

    wait_for_agent_message(&codex, "resumed old task").await;
    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 4);

    let post_compact_body: Value = from_slice(&requests[2]).expect("parse post-compact request");
    let steered_body: Value = from_slice(&requests[3]).expect("parse steered request");

    let post_compact_user_texts = message_input_texts(&post_compact_body, "user");
    assert!(
        !post_compact_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should stay pending until the model resumes after compaction"
    );

    let steered_user_texts = message_input_texts(&steered_body, "user");
    assert!(
        steered_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should be recorded on the request after the post-compact continuation"
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn steered_user_input_follows_compact_when_only_the_steer_needs_follow_up() {
    let (gate_first_completed_tx, gate_first_completed_rx) = oneshot::channel();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_message_item_added("msg-1", "")),
        chunk(ev_output_text_delta("first answer")),
        chunk(ev_message_item_done("msg-1", "first answer")),
        gated_chunk(
            gate_first_completed_rx,
            vec![ev_completed_with_tokens(
                "resp-1", /*total_tokens*/ 500,
            )],
        ),
    ];

    let compact_chunks = vec![
        chunk(ev_response_created("resp-compact")),
        chunk(ev_message_item_done("msg-compact", "AUTO_COMPACT_SUMMARY")),
        chunk(ev_completed_with_tokens(
            "resp-compact",
            /*total_tokens*/ 50,
        )),
    ];

    let steered_follow_up_chunks = vec![
        chunk(ev_response_created("resp-steered")),
        chunk(ev_message_item_done(
            "msg-steered",
            "processed steered prompt",
        )),
        chunk(ev_completed_with_tokens(
            "resp-steered",
            /*total_tokens*/ 70,
        )),
    ];

    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, compact_chunks, steered_follow_up_chunks])
            .await;

    let codex = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config.model_provider.name = "OpenAI (test)".to_string();
            config.model_provider.supports_websockets = false;
            config.model_auto_compact_token_limit = Some(200);
        })
        .build_with_streaming_server(&server)
        .await
        .expect("build streaming Codex test session")
        .codex;

    submit_user_input(&codex, "first prompt").await;
    wait_for_agent_message(&codex, "first answer").await;
    steer_user_input(&codex, "second prompt").await;
    let _ = gate_first_completed_tx.send(());

    wait_for_agent_message(&codex, "processed steered prompt").await;
    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 3);

    let compact_body: Value = from_slice(&requests[1]).expect("parse compact request");
    let steered_body: Value = from_slice(&requests[2]).expect("parse steered request");

    let compact_user_texts = message_input_texts(&compact_body, "user");
    assert!(
        !compact_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should not be included in the compaction request"
    );

    let steered_user_texts = message_input_texts(&steered_body, "user");
    assert!(
        steered_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should follow compaction without an empty resume request when the model was already done"
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn steered_user_input_waits_when_tool_output_triggers_compact_before_next_request() {
    let (gate_first_completed_tx, gate_first_completed_rx) = oneshot::channel();

    let large_output_command = if cfg!(windows) {
        "[Console]::Out.Write([string]::new([char]'0', 4000))"
    } else {
        "printf '%04000d' 0"
    };
    let large_output_args = json!({
        "cmd": large_output_command,
        "login": false,
        "yield_time_ms": 2000,
    })
    .to_string();

    let first_chunks = vec![
        chunk(ev_response_created("resp-1")),
        chunk(ev_function_call(
            "call-1",
            "exec_command",
            &large_output_args,
        )),
        gated_chunk(
            gate_first_completed_rx,
            vec![ev_completed_with_tokens(
                "resp-1", /*total_tokens*/ 100,
            )],
        ),
    ];

    let compact_chunks = vec![
        chunk(ev_response_created("resp-compact")),
        chunk(ev_message_item_done("msg-compact", "TOOL_OUTPUT_SUMMARY")),
        chunk(ev_completed_with_tokens(
            "resp-compact",
            /*total_tokens*/ 50,
        )),
    ];

    let post_compact_continuation_chunks = vec![
        chunk(ev_response_created("resp-post-compact")),
        chunk(ev_message_item_done(
            "msg-post-compact",
            "resumed after compacting tool output",
        )),
        chunk(ev_completed_with_tokens(
            "resp-post-compact",
            /*total_tokens*/ 60,
        )),
    ];

    let steered_follow_up_chunks = vec![
        chunk(ev_response_created("resp-steered")),
        chunk(ev_message_item_done(
            "msg-steered",
            "processed steered prompt",
        )),
        chunk(ev_completed_with_tokens(
            "resp-steered",
            /*total_tokens*/ 70,
        )),
    ];

    let (server, _completions) = start_streaming_sse_server(vec![
        first_chunks,
        compact_chunks,
        post_compact_continuation_chunks,
        steered_follow_up_chunks,
    ])
    .await;

    let test = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config.model_provider.name = "OpenAI (test)".to_string();
            config.model_provider.supports_websockets = false;
            config.model_auto_compact_token_limit = Some(200);
        })
        .build_with_streaming_server(&server)
        .await
        .expect("build streaming Codex test session");
    let codex = test.codex.clone();

    submit_danger_full_access_user_turn(&test, "first prompt").await;
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnStarted(_))).await;
    steer_user_input(&codex, "second prompt").await;
    let _ = gate_first_completed_tx.send(());

    wait_for_turn_complete(&codex).await;

    let requests = server.requests().await;
    assert_eq!(requests.len(), 4);

    let compact_body: Value = from_slice(&requests[1]).expect("parse compact request");
    let post_compact_body: Value = from_slice(&requests[2]).expect("parse post-compact request");
    let steered_body: Value = from_slice(&requests[3]).expect("parse steered request");

    let compact_user_texts = message_input_texts(&compact_body, "user");
    assert!(
        !compact_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should not be included in the compaction request"
    );

    let post_compact_user_texts = message_input_texts(&post_compact_body, "user");
    assert!(
        !post_compact_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should stay pending until after the compacted continuation"
    );

    let steered_user_texts = message_input_texts(&steered_body, "user");
    assert!(
        steered_user_texts
            .iter()
            .any(|text| text == "second prompt"),
        "steered input should be recorded on the request after the post-compact continuation"
    );

    server.shutdown().await;
}
