use anyhow::Context;
use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::write_mock_responses_config_toml_with_chatgpt_base_url;
use codex_app_server_protocol::CutexParticipantPresentation;
use codex_app_server_protocol::InterAgentDeliveryMode;
use codex_app_server_protocol::InterAgentDeliveryStatus;
use codex_app_server_protocol::InterAgentDeliveryStatusQuery;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::ItemStartedNotification;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::TaskServiceMessageClass;
use codex_app_server_protocol::TaskServiceMessagePresentation;
use codex_app_server_protocol::ThreadInterAgentMessageParams;
use codex_app_server_protocol::ThreadInterAgentMessageResponse;
use codex_app_server_protocol::ThreadInterAgentMessageStatusParams;
use codex_app_server_protocol::ThreadInterAgentMessageStatusResponse;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_core::inter_agent_delivery::inter_agent_semantic_sha256;
use codex_protocol::ResponseItemId;
use codex_protocol::models::AgentMessageInputContent;
use codex_protocol::models::ResponseItem;
use core_test_support::responses;
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(10);
const TASK_SERVICE_RECOVERY_ID: &str =
    "amsg_tsc_tsn-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const NORMALIZED_TASK_SERVICE_RECOVERY_ID: &str =
    "amsg_ext_5a4c996908528125c6973aa905caaeb96fae8dec05d50acd44be7e0";

#[derive(Debug, Default)]
struct InterAgentLifecycle {
    started: Vec<ThreadItem>,
    completed: Vec<ThreadItem>,
}

#[tokio::test]
async fn thread_inter_agent_message_receipt_precedes_passive_consumption_and_history() -> Result<()>
{
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Done"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut mcp).await?;

    let message = inter_agent_params(
        &thread_id,
        "mail_passive_1",
        InterAgentDeliveryMode::Passive,
    );
    let request_id = send_inter_agent_message(&mut mcp, message).await?;
    let receipt: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(request_id)).await??;
    assert!(!receipt.submission_id.is_empty());
    assert!(
        response_mock.requests().is_empty(),
        "a passive message must not start a model turn before other work arrives"
    );

    let turn_request_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "consume queued mail".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(turn_request_id)).await??;

    let lifecycle =
        collect_inter_agent_lifecycle_until_turn_completed(&mut mcp, &thread_id, "mail_passive_1")
            .await?;
    let expected = expected_item("mail_passive_1", InterAgentDeliveryMode::Passive);
    assert_eq!(lifecycle.started, vec![expected.clone()]);
    assert_eq!(lifecycle.completed, vec![expected.clone()]);
    assert_eq!(response_mock.requests().len(), 1);

    let read_request_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread_id.clone(),
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(read_request_id)).await??;
    let stored_messages = read
        .thread
        .turns
        .into_iter()
        .flat_map(|turn| turn.items)
        .filter(|item| {
            matches!(
                item,
                ThreadItem::InterAgentMessage { id, .. } if id == "mail_passive_1"
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(stored_messages, vec![expected]);

    Ok(())
}

#[tokio::test]
async fn delivery_status_is_a4_only_after_consumption_and_is_stable_across_resume() -> Result<()> {
    let server = responses::start_mock_server().await;
    responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-a4"),
            responses::ev_assistant_message("msg-a4", "Done"),
            responses::ev_completed("resp-a4"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut primary = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut primary).await?;
    let params = inter_agent_params(
        &thread_id,
        "mail_a4_restart_1",
        InterAgentDeliveryMode::Passive,
    );
    let digest = semantic_digest(&params)?;
    assert_eq!(
        digest, "8b677efc23d0fe2c4b11d23d4fa0c2af7049d9afa37aa5de2b214b9d6c83bdef",
        "the cross-runtime semantic digest fixture must stay stable"
    );

    let unknown = delivery_status(&mut primary, &thread_id, "mail_a4_restart_1", &digest).await?;
    assert!(matches!(
        unknown.statuses.as_slice(),
        [InterAgentDeliveryStatus::Unknown { .. }]
    ));

    let submit_id = send_inter_agent_message(&mut primary, params.clone()).await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(submit_id)).await??;
    let pending = delivery_status(&mut primary, &thread_id, "mail_a4_restart_1", &digest).await?;
    assert!(matches!(
        pending.statuses.as_slice(),
        [InterAgentDeliveryStatus::Unknown { .. } | InterAgentDeliveryStatus::Pending { .. }]
    ));

    let turn_request_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "consume mail for A4".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(turn_request_id)).await??;
    collect_inter_agent_lifecycle_until_turn_completed(
        &mut primary,
        &thread_id,
        "mail_a4_restart_1",
    )
    .await?;
    let persisted = delivery_status(&mut primary, &thread_id, "mail_a4_restart_1", &digest).await?;
    let [InterAgentDeliveryStatus::ContextPersisted { receipt, .. }] =
        persisted.statuses.as_slice()
    else {
        panic!("successful canonical append and flush must publish A4: {persisted:?}");
    };
    assert!(receipt.receipt_id.starts_with("a4r_"));
    assert_eq!(receipt.response_item_id, "mail_a4_restart_1");
    assert!(!receipt.turn_id.is_empty());
    assert!(receipt.rollout_ordinal > 0);

    timeout(DEFAULT_READ_TIMEOUT, primary.shutdown_gracefully()).await??;
    let mut resumed = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let resume_id = resumed
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread_id.clone(),
            ..Default::default()
        })
        .await?;
    let _: ThreadResumeResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(resume_id)).await??;
    assert_eq!(
        delivery_status(&mut resumed, &thread_id, "mail_a4_restart_1", &digest).await?,
        persisted,
        "resume must rebuild and return the identical stable receipt"
    );

    let duplicate_id = send_inter_agent_message(&mut resumed, params.clone()).await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(duplicate_id)).await??;
    let mut changed = params;
    changed.content = "changed body".to_string();
    let changed_digest = semantic_digest(&changed)?;
    let conflict_id = send_inter_agent_message(&mut resumed, changed).await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(conflict_id)).await??;
    let conflict = delivery_status(
        &mut resumed,
        &thread_id,
        "mail_a4_restart_1",
        &changed_digest,
    )
    .await?;
    assert!(matches!(
        conflict.statuses.as_slice(),
        [InterAgentDeliveryStatus::Conflict { .. }]
    ));

    let read_id = resumed
        .send_thread_read_request(ThreadReadParams {
            thread_id,
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(read_id)).await??;
    assert_eq!(
        read.thread
            .turns
            .iter()
            .flat_map(|turn| &turn.items)
            .filter(|item| matches!(item, ThreadItem::InterAgentMessage { id, .. } if id == "mail_a4_restart_1"))
            .count(),
        1,
        "same-ID replay and conflict must not duplicate canonical history"
    );

    Ok(())
}

#[tokio::test]
async fn restart_before_a4_allows_same_id_replay() -> Result<()> {
    let server = responses::start_mock_server().await;
    responses::mount_sse_sequence(
        &server,
        ["setup", "replay"]
            .into_iter()
            .map(|name| {
                responses::sse(vec![
                    responses::ev_response_created(&format!("resp-{name}")),
                    responses::ev_assistant_message(&format!("msg-{name}"), "Done"),
                    responses::ev_completed(&format!("resp-{name}")),
                ])
            })
            .collect(),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut primary = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut primary).await?;
    let setup_turn_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "materialize rollout".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(setup_turn_id)).await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    let params = inter_agent_params(
        &thread_id,
        "mail_pre_a4_restart_1",
        InterAgentDeliveryMode::Passive,
    );
    let digest = semantic_digest(&params)?;
    let submit_id = send_inter_agent_message(&mut primary, params.clone()).await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(submit_id)).await??;
    timeout(DEFAULT_READ_TIMEOUT, primary.shutdown_gracefully()).await??;

    let mut resumed = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let resume_id = resumed
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread_id.clone(),
            ..Default::default()
        })
        .await?;
    let _: ThreadResumeResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(resume_id)).await??;
    let status =
        delivery_status(&mut resumed, &thread_id, "mail_pre_a4_restart_1", &digest).await?;
    assert!(matches!(
        status.statuses.as_slice(),
        [InterAgentDeliveryStatus::Unknown { .. }]
    ));

    let replay_id = send_inter_agent_message(&mut resumed, params).await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(replay_id)).await??;
    let turn_id = resumed
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "consume replayed mail".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(turn_id)).await??;
    collect_inter_agent_lifecycle_until_turn_completed(
        &mut resumed,
        &thread_id,
        "mail_pre_a4_restart_1",
    )
    .await?;
    let persisted =
        delivery_status(&mut resumed, &thread_id, "mail_pre_a4_restart_1", &digest).await?;
    assert!(matches!(
        persisted.statuses.as_slice(),
        [InterAgentDeliveryStatus::ContextPersisted { .. }]
    ));

    Ok(())
}

#[tokio::test]
async fn tagged_cutex_metadata_is_live_only_and_never_changes_history() -> Result<()> {
    let server = responses::start_mock_server().await;
    responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Done"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut mcp).await?;
    let mut params = inter_agent_params(
        &thread_id,
        "mail_cutex_presentation_1",
        InterAgentDeliveryMode::Passive,
    );
    params.author_metadata = Some(CutexParticipantPresentation {
        display_name: Some("Worker".to_string()),
        cutex_session_id: Some("cutex.worker.1".to_string()),
        role: Some("Worker".to_string()),
        ..Default::default()
    });
    params.recipient_metadata = Some(CutexParticipantPresentation {
        display_name: Some("Director".to_string()),
        ..Default::default()
    });
    let request_id = send_inter_agent_message(&mut mcp, params).await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(request_id)).await??;

    let turn_request_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "consume tagged mail".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(turn_request_id)).await??;
    let lifecycle = collect_inter_agent_lifecycle_until_turn_completed(
        &mut mcp,
        &thread_id,
        "mail_cutex_presentation_1",
    )
    .await?;
    for item in lifecycle.started.iter().chain(&lifecycle.completed) {
        let ThreadItem::InterAgentMessage {
            author_metadata,
            recipient_metadata,
            ..
        } = item
        else {
            panic!("expected inter-agent message");
        };
        assert_eq!(
            author_metadata
                .as_ref()
                .and_then(|metadata| metadata.display_name.as_deref()),
            Some("Worker")
        );
        assert_eq!(
            recipient_metadata
                .as_ref()
                .and_then(|metadata| metadata.display_name.as_deref()),
            Some("Director")
        );
    }

    let read_request_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id,
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(read_request_id)).await??;
    let stored = read
        .thread
        .turns
        .iter()
        .flat_map(|turn| &turn.items)
        .find(|item| matches!(item, ThreadItem::InterAgentMessage { id, .. } if id == "mail_cutex_presentation_1"))
        .expect("canonical history item");
    let ThreadItem::InterAgentMessage {
        author_metadata,
        recipient_metadata,
        ..
    } = stored
    else {
        unreachable!()
    };
    assert_eq!(author_metadata, &None);
    assert_eq!(recipient_metadata, &None);

    Ok(())
}

#[tokio::test]
async fn thread_inter_agent_message_duplicate_has_two_receipts_but_one_wake() -> Result<()> {
    let server = responses::start_mock_server().await;
    responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Done"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut mcp).await?;

    let first_request_id = send_inter_agent_message(
        &mut mcp,
        inter_agent_params(
            &thread_id,
            "mail_duplicate_1",
            InterAgentDeliveryMode::AfterTurn,
        ),
    )
    .await?;
    let first_receipt: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(first_request_id)).await??;
    let second_request_id = send_inter_agent_message(
        &mut mcp,
        inter_agent_params(
            &thread_id,
            "mail_duplicate_1",
            InterAgentDeliveryMode::AfterTurn,
        ),
    )
    .await?;
    let second_receipt: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(second_request_id)).await??;
    assert!(!first_receipt.submission_id.is_empty());
    assert!(!second_receipt.submission_id.is_empty());

    let lifecycle = collect_inter_agent_lifecycle_until_turn_completed(
        &mut mcp,
        &thread_id,
        "mail_duplicate_1",
    )
    .await?;
    let expected = expected_item("mail_duplicate_1", InterAgentDeliveryMode::AfterTurn);
    assert_eq!(lifecycle.started, vec![expected.clone()]);
    assert_eq!(lifecycle.completed, vec![expected]);

    sleep(Duration::from_millis(200)).await;
    let response_request_count = server
        .received_requests()
        .await
        .context("wiremock should record response requests")?
        .iter()
        .filter(|request| request.url.path().ends_with("/responses"))
        .count();
    assert_eq!(
        response_request_count, 1,
        "a duplicate messageId must not start a second model turn"
    );

    Ok(())
}

#[tokio::test]
async fn task_service_assignment_projects_model_context_but_preserves_raw_events_and_history()
-> Result<()> {
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Accepted"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut mcp).await?;
    let raw = "Message Type: TASK_SERVICE_ASSIGNMENT\n\
Task name: worker-task\n\
Sender: cutex-task-service\n\
Coordinator: cutex.director\n\
Assignment ID: assignment-01\n\
Task ID: task-01\n\
Task Revision: 4\n\
Contract SHA-256: deadbeef\n\
Send Attempt ID: send-01\n\
External Action ID: action-01\n\
External Message ID: transport-message-01\n\
Summary:\nRedundant transport summary.\n\
Opaque Contract:\n# Exact contract\n\nPerform the assigned work.\n";

    let request_id = send_inter_agent_message(
        &mut mcp,
        ThreadInterAgentMessageParams {
            thread_id: thread_id.clone(),
            message_id: TASK_SERVICE_RECOVERY_ID.to_string(),
            author: "/root/cutex_task_service".to_string(),
            recipient: "/root".to_string(),
            other_recipients: None,
            content: raw.to_string(),
            delivery_mode: InterAgentDeliveryMode::AfterTurn,
            author_metadata: None,
            recipient_metadata: None,
        },
    )
    .await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(request_id)).await??;

    let lifecycle = collect_inter_agent_lifecycle_until_turn_completed(
        &mut mcp,
        &thread_id,
        NORMALIZED_TASK_SERVICE_RECOVERY_ID,
    )
    .await?;
    let expected = ThreadItem::InterAgentMessage {
        id: NORMALIZED_TASK_SERVICE_RECOVERY_ID.to_string(),
        author: "/root/cutex_task_service".to_string(),
        recipient: "/root".to_string(),
        other_recipients: Vec::new(),
        content: raw.to_string(),
        delivery_mode: InterAgentDeliveryMode::AfterTurn,
        author_metadata: None,
        recipient_metadata: None,
        task_service_presentation: Some(TaskServiceMessagePresentation {
            class: TaskServiceMessageClass::Assignment,
            task_name: "worker-task".to_string(),
            assignment_id: "assignment-01".to_string(),
            project_id: None,
            transition: None,
            semantic_payload: "# Exact contract\n\nPerform the assigned work.\n".to_string(),
        }),
    };
    assert_eq!(lifecycle.started, vec![expected.clone()]);
    assert_eq!(lifecycle.completed, vec![expected.clone()]);

    let read_request_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread_id.clone(),
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(read_request_id)).await??;
    assert!(
        read.thread
            .turns
            .iter()
            .flat_map(|turn| &turn.items)
            .any(|item| item == &expected),
        "the full authenticated envelope should remain available to raw-event consumers"
    );

    timeout(DEFAULT_READ_TIMEOUT, mcp.shutdown_gracefully()).await??;
    let mut resumed = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let resume_request_id = resumed
        .send_thread_resume_request(ThreadResumeParams {
            thread_id,
            ..Default::default()
        })
        .await?;
    let resumed_thread: ThreadResumeResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        resumed.read_response(resume_request_id),
    )
    .await??;
    assert!(
        resumed_thread
            .thread
            .turns
            .iter()
            .flat_map(|turn| &turn.items)
            .any(|item| item == &expected),
        "the full authenticated envelope should remain durable across resume"
    );

    let request_body = response_mock.single_request().body_json().to_string();
    assert!(request_body.contains("Assignment ID: assignment-01"));
    assert_eq!(request_body.matches("# Exact contract").count(), 1);
    assert!(request_body.contains("Perform the assigned work."));
    for omitted in [
        "Coordinator:",
        "Task ID:",
        "Task Revision:",
        "Contract SHA-256:",
        "Send Attempt ID:",
        "External Action ID:",
        "External Message ID:",
        "Redundant transport summary",
    ] {
        assert!(
            !request_body.contains(omitted),
            "model request unexpectedly contains {omitted}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn oversized_message_id_is_safe_in_lifecycle_history_model_input_and_resume() -> Result<()> {
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_sequence(
        &server,
        (1..=2)
            .map(|index| {
                responses::sse(vec![
                    responses::ev_response_created(&format!("resp-{index}")),
                    responses::ev_assistant_message(
                        &format!("msg-{index}"),
                        &format!("Done {index}"),
                    ),
                    responses::ev_completed(&format!("resp-{index}")),
                ])
            })
            .collect(),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut primary = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut primary).await?;

    let request_id = send_inter_agent_message(
        &mut primary,
        inter_agent_params(
            &thread_id,
            TASK_SERVICE_RECOVERY_ID,
            InterAgentDeliveryMode::Passive,
        ),
    )
    .await?;
    let _: ThreadInterAgentMessageResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(request_id)).await??;

    let turn_request_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "consume oversized-id mail".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(turn_request_id)).await??;

    let lifecycle = collect_inter_agent_lifecycle_until_turn_completed(
        &mut primary,
        &thread_id,
        NORMALIZED_TASK_SERVICE_RECOVERY_ID,
    )
    .await?;
    let expected = expected_item(
        NORMALIZED_TASK_SERVICE_RECOVERY_ID,
        InterAgentDeliveryMode::Passive,
    );
    assert_eq!(lifecycle.started, vec![expected.clone()]);
    assert_eq!(lifecycle.completed, vec![expected.clone()]);

    let read_request_id = primary
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread_id.clone(),
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse =
        timeout(DEFAULT_READ_TIMEOUT, primary.read_response(read_request_id)).await??;
    assert!(
        read.thread
            .turns
            .iter()
            .flat_map(|turn| &turn.items)
            .any(|item| item == &expected),
        "normalized inter-agent message should be persisted"
    );

    timeout(DEFAULT_READ_TIMEOUT, primary.shutdown_gracefully()).await??;
    let mut resumed = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let resume_request_id = resumed
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread_id.clone(),
            ..Default::default()
        })
        .await?;
    let response: ThreadResumeResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        resumed.read_response(resume_request_id),
    )
    .await??;
    assert!(
        response
            .thread
            .turns
            .iter()
            .flat_map(|turn| &turn.items)
            .any(|item| item == &expected),
        "normalized inter-agent message should survive resume"
    );

    let turn_request_id = resumed
        .send_turn_start_request(TurnStartParams {
            thread_id,
            input: vec![UserInput::Text {
                text: "verify resumed model input".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, resumed.read_response(turn_request_id)).await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        resumed.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let requests = response_mock.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        let agent_message_ids = request
            .input()
            .into_iter()
            .filter(|item| item["type"] == "agent_message")
            .filter_map(|item| item["id"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        assert!(
            agent_message_ids.contains(&NORMALIZED_TASK_SERVICE_RECOVERY_ID.to_string()),
            "normalized ID should reach model request: {agent_message_ids:?}"
        );
        assert!(!agent_message_ids.contains(&TASK_SERVICE_RECOVERY_ID.to_string()));
        assert!(agent_message_ids.iter().all(|id| id.len() <= 64));
    }

    Ok(())
}

#[tokio::test]
async fn unsafe_agent_message_id_from_existing_history_is_not_sent_to_model() -> Result<()> {
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Done"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;

    let resume_request_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: codex_protocol::ThreadId::new().to_string(),
            history: Some(vec![ResponseItem::AgentMessage {
                id: Some(ResponseItemId::from_server(
                    TASK_SERVICE_RECOVERY_ID.to_string(),
                )),
                author: "/root/task-service".to_string(),
                recipient: "/root".to_string(),
                content: vec![AgentMessageInputContent::InputText {
                    text: "polluted existing rollout fixture".to_string(),
                }],
                internal_chat_message_metadata_passthrough: None,
            }]),
            ..Default::default()
        })
        .await?;
    let resumed: ThreadResumeResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(resume_request_id)).await??;

    let turn_request_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: resumed.thread.id,
            input: vec![UserInput::Text {
                text: "continue after polluted history".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(turn_request_id)).await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let request = response_mock.single_request();
    let existing_history_message = request
        .input()
        .into_iter()
        .find(|item| {
            item["type"] == "agent_message"
                && item["content"].as_array().is_some_and(|content| {
                    content.iter().any(|part| {
                        part["text"].as_str() == Some("polluted existing rollout fixture")
                    })
                })
        })
        .context("existing AgentMessage should remain in model input")?;
    assert!(existing_history_message.get("id").is_none());
    assert!(
        !request
            .body_json()
            .to_string()
            .contains(TASK_SERVICE_RECOVERY_ID)
    );

    Ok(())
}

#[tokio::test]
async fn thread_inter_agent_message_rejects_invalid_params_before_submission() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_mock_responses_config_toml_with_chatgpt_base_url(
        codex_home.path(),
        "http://localhost/unused",
        "http://localhost/unused",
    )?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut mcp).await?;

    let cases = [
        (
            json!({
                "threadId": thread_id,
                "messageId": " ",
                "author": "/root/sender",
                "recipient": "/root/recipient",
                "content": "hello",
                "deliveryMode": "soon"
            }),
            "messageId must not be empty",
        ),
        (
            json!({
                "threadId": thread_id,
                "messageId": "mail_invalid_author",
                "author": "sender",
                "recipient": "/root/recipient",
                "content": "hello",
                "deliveryMode": "soon"
            }),
            "absolute agent paths",
        ),
        (
            json!({
                "threadId": thread_id,
                "messageId": "mail_invalid_observer",
                "author": "/root/sender",
                "recipient": "/root/recipient",
                "otherRecipients": ["/root/Invalid"],
                "content": "hello",
                "deliveryMode": "soon"
            }),
            "lowercase letters",
        ),
        (
            json!({
                "threadId": thread_id,
                "messageId": "mail_missing_mode",
                "author": "/root/sender",
                "recipient": "/root/recipient",
                "content": "hello"
            }),
            "deliveryMode",
        ),
    ];

    for (params, expected_message) in cases {
        let request_id = mcp
            .send_raw_request("thread/inter_agent_message", Some(params))
            .await?;
        let error = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
        )
        .await??;
        assert_eq!(error.error.code, -32600);
        assert!(
            error.error.message.contains(expected_message),
            "unexpected error for {expected_message}: {}",
            error.error.message
        );
    }

    Ok(())
}

#[tokio::test]
async fn delivery_status_validates_bounds_digest_and_preserves_request_order() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_mock_responses_config_toml_with_chatgpt_base_url(
        codex_home.path(),
        "http://localhost/unused",
        "http://localhost/unused",
    )?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let thread_id = start_thread(&mut mcp).await?;

    let ordered_id = mcp
        .send_raw_request(
            "thread/inter_agent_message/status",
            Some(json!({
                "threadId": thread_id,
                "messages": [
                    {"messageId": "message_second", "semanticSha256": "b".repeat(64)},
                    {"messageId": "message_first", "semanticSha256": "a".repeat(64)}
                ]
            })),
        )
        .await?;
    let ordered: ThreadInterAgentMessageStatusResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(ordered_id)).await??;
    assert!(matches!(
        ordered.statuses.as_slice(),
        [
            InterAgentDeliveryStatus::Unknown { message_id: second, .. },
            InterAgentDeliveryStatus::Unknown { message_id: first, .. }
        ] if second == "message_second" && first == "message_first"
    ));

    let invalid_cases = [
        json!({"threadId": thread_id, "messages": []}),
        json!({
            "threadId": thread_id,
            "messages": [
                {"messageId": "duplicate", "semanticSha256": "a".repeat(64)},
                {"messageId": "duplicate", "semanticSha256": "a".repeat(64)}
            ]
        }),
        json!({
            "threadId": thread_id,
            "messages": [{"messageId": "bad_digest", "semanticSha256": "A".repeat(64)}]
        }),
        json!({
            "threadId": thread_id,
            "messages": (0..257).map(|index| json!({
                "messageId": format!("message_{index}"),
                "semanticSha256": "a".repeat(64)
            })).collect::<Vec<_>>()
        }),
    ];
    for params in invalid_cases {
        let request_id = mcp
            .send_raw_request("thread/inter_agent_message/status", Some(params))
            .await?;
        let error = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
        )
        .await??;
        assert_eq!(error.error.code, -32600);
    }

    Ok(())
}

async fn start_thread(mcp: &mut TestAppServer) -> Result<String> {
    let request_id = mcp
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let ThreadStartResponse { thread, .. } =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(request_id)).await??;
    Ok(thread.id)
}

async fn send_inter_agent_message(
    mcp: &mut TestAppServer,
    params: ThreadInterAgentMessageParams,
) -> Result<i64> {
    mcp.send_raw_request(
        "thread/inter_agent_message",
        Some(serde_json::to_value(params)?),
    )
    .await
}

async fn delivery_status(
    mcp: &mut TestAppServer,
    thread_id: &str,
    message_id: &str,
    semantic_sha256: &str,
) -> Result<ThreadInterAgentMessageStatusResponse> {
    let request_id = mcp
        .send_raw_request(
            "thread/inter_agent_message/status",
            Some(serde_json::to_value(ThreadInterAgentMessageStatusParams {
                thread_id: thread_id.to_string(),
                messages: vec![InterAgentDeliveryStatusQuery {
                    message_id: message_id.to_string(),
                    semantic_sha256: semantic_sha256.to_string(),
                }],
            })?),
        )
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(request_id)).await?
}

fn semantic_digest(params: &ThreadInterAgentMessageParams) -> Result<String> {
    let mut communication = params.to_core_communication().map_err(anyhow::Error::msg)?;
    communication.external_message_id = Some(params.message_id.clone());
    Ok(inter_agent_semantic_sha256(&communication))
}

fn inter_agent_params(
    thread_id: &str,
    message_id: &str,
    delivery_mode: InterAgentDeliveryMode,
) -> ThreadInterAgentMessageParams {
    ThreadInterAgentMessageParams {
        thread_id: thread_id.to_string(),
        message_id: message_id.to_string(),
        author: "/root/sender".to_string(),
        recipient: "/root/recipient".to_string(),
        other_recipients: Some(vec!["/root/observer".to_string()]),
        content: "hello from sender".to_string(),
        delivery_mode,
        author_metadata: None,
        recipient_metadata: None,
    }
}

fn expected_item(message_id: &str, delivery_mode: InterAgentDeliveryMode) -> ThreadItem {
    ThreadItem::InterAgentMessage {
        id: message_id.to_string(),
        author: "/root/sender".to_string(),
        recipient: "/root/recipient".to_string(),
        other_recipients: vec!["/root/observer".to_string()],
        content: "hello from sender".to_string(),
        delivery_mode,
        author_metadata: None,
        recipient_metadata: None,
        task_service_presentation: None,
    }
}

async fn collect_inter_agent_lifecycle_until_turn_completed(
    mcp: &mut TestAppServer,
    thread_id: &str,
    message_id: &str,
) -> Result<InterAgentLifecycle> {
    let mut lifecycle = InterAgentLifecycle::default();

    loop {
        let message = timeout(DEFAULT_READ_TIMEOUT, mcp.read_next_message()).await??;
        let JSONRPCMessage::Notification(notification) = message else {
            continue;
        };
        assert_ne!(notification.method, "thread/interAgentMessage/sent");
        assert_ne!(notification.method, "thread/interAgentMessage/received");

        match notification.method.as_str() {
            "item/started" => {
                let params = notification
                    .params
                    .context("item/started should include params")?;
                let started: ItemStartedNotification = serde_json::from_value(params)?;
                if matches!(
                    &started.item,
                    ThreadItem::InterAgentMessage { id, .. } if id == message_id
                ) {
                    lifecycle.started.push(started.item);
                }
            }
            "item/completed" => {
                let params = notification
                    .params
                    .context("item/completed should include params")?;
                let completed: ItemCompletedNotification = serde_json::from_value(params)?;
                if matches!(
                    &completed.item,
                    ThreadItem::InterAgentMessage { id, .. } if id == message_id
                ) {
                    lifecycle.completed.push(completed.item);
                }
            }
            "turn/completed" => {
                let params = notification
                    .params
                    .context("turn/completed should include params")?;
                let completed: TurnCompletedNotification = serde_json::from_value(params)?;
                if completed.thread_id == thread_id {
                    return Ok(lifecycle);
                }
            }
            _ => {}
        }
    }
}
