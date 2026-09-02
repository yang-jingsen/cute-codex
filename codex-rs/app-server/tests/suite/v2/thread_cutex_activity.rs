use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::AgentManagementPhase;
use codex_app_server_protocol::CutexUiActivity;
use codex_app_server_protocol::CutexUiActivityCheckpoint;
use codex_app_server_protocol::CutexUiActivityDelivery;
use codex_app_server_protocol::CutexUiActivityDeliveryClass;
use codex_app_server_protocol::CutexUiActivityDeliverySchema;
use codex_app_server_protocol::CutexUiActivityIngestionDisposition;
use codex_app_server_protocol::CutexUiActivityNotification;
use codex_app_server_protocol::ManagedAgentActivityItem;
use codex_app_server_protocol::ManagedAgentActivityStatus;
use codex_app_server_protocol::ManagedAgentOperation;
use codex_app_server_protocol::TaskWatchdogActivityItem;
use codex_app_server_protocol::TaskWatchdogActivityKind;
use codex_app_server_protocol::TaskWatchdogStage;
use codex_app_server_protocol::ThreadCutexActivityParams;
use codex_app_server_protocol::ThreadCutexActivityResponse;
use codex_app_server_protocol::ThreadHistoryMode;
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
use codex_features::Feature;
use core_test_support::responses;
use tempfile::TempDir;
use tokio::time::timeout;

const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn cutex_activity_is_deduplicated_ordered_and_projected_once() -> Result<()> {
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
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::Sqlite)
        .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(READ_TIMEOUT)
        .await?;

    let start_id = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            model: Some("mock-model".to_string()),
            history_mode: Some(ThreadHistoryMode::Paginated),
            ..Default::default()
        })
        .await?;
    let ThreadStartResponse { thread, .. } =
        timeout(READ_TIMEOUT, app.read_response(start_id)).await??;
    let turn_request_id = app
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "create the Director timeline".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let TurnStartResponse { turn } =
        timeout(READ_TIMEOUT, app.read_response(turn_request_id)).await??;
    loop {
        let completed: TurnCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("turn/completed")).await??;
        if completed.turn.id == turn.id {
            break;
        }
    }

    let accepted = send_activity(
        &mut app,
        params(
            &thread.id,
            "event-2",
            2,
            ManagedAgentActivityStatus::Completed,
        ),
    )
    .await?;
    assert_eq!(
        accepted.disposition,
        CutexUiActivityIngestionDisposition::Accepted
    );
    let lifecycle = loop {
        let lifecycle: CutexUiActivityNotification = timeout(
            READ_TIMEOUT,
            app.read_notification("cutexActivity/presented"),
        )
        .await??;
        if matches!(
            lifecycle.item,
            ThreadItem::ManagedAgentActivity { ref activity } if activity.id == "action-1"
        ) {
            break lifecycle;
        }
    };
    assert_eq!(lifecycle.delivery, delivery(2));
    assert!(matches!(
        lifecycle.item,
        ThreadItem::ManagedAgentActivity { ref activity }
            if activity.event_id == "event-2" && activity.sequence == 2
    ));

    let duplicate = send_activity(
        &mut app,
        params(
            &thread.id,
            "event-2",
            2,
            ManagedAgentActivityStatus::Completed,
        ),
    )
    .await?;
    assert_eq!(
        duplicate.disposition,
        CutexUiActivityIngestionDisposition::Duplicate
    );
    let stale = send_activity(
        &mut app,
        params(
            &thread.id,
            "event-1",
            1,
            ManagedAgentActivityStatus::Completed,
        ),
    )
    .await?;
    assert_eq!(
        stale.disposition,
        CutexUiActivityIngestionDisposition::Stale
    );

    let read_id = app
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id.clone(),
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse = timeout(READ_TIMEOUT, app.read_response(read_id)).await??;
    let has_activity = read
        .thread
        .turns
        .into_iter()
        .flat_map(|turn| turn.items)
        .any(|item| matches!(item, ThreadItem::ManagedAgentActivity { .. }));
    assert!(!has_activity);

    Ok(())
}

#[tokio::test]
async fn phase_delivery_is_ui_only_and_recovered_replay_survives_restart() -> Result<()> {
    let server = responses::start_mock_server().await;
    responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-lane"),
            responses::ev_assistant_message("msg-lane", "Ready"),
            responses::ev_completed("resp-lane"),
        ]),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::Sqlite)
        .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(READ_TIMEOUT)
        .await?;

    let start_id = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            model: Some("mock-model".to_string()),
            history_mode: Some(ThreadHistoryMode::Paginated),
            ..Default::default()
        })
        .await?;
    let ThreadStartResponse { thread, .. } =
        timeout(READ_TIMEOUT, app.read_response(start_id)).await??;
    let turn_request_id = app
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "create a native turn that must not own lane activity".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let TurnStartResponse { turn } =
        timeout(READ_TIMEOUT, app.read_response(turn_request_id)).await??;
    loop {
        let turn_completed: TurnCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("turn/completed")).await??;
        if turn_completed.turn.id == turn.id {
            break;
        }
    }

    let started_response = send_activity(
        &mut app,
        phase_params(
            &thread.id,
            "event-started",
            1,
            AgentManagementPhase::Prepared,
            ManagedAgentActivityStatus::InProgress,
        ),
    )
    .await?;
    assert_eq!(
        started_response.disposition,
        CutexUiActivityIngestionDisposition::Accepted
    );
    let started = loop {
        let notification: CutexUiActivityNotification = timeout(
            READ_TIMEOUT,
            app.read_notification("cutexActivity/presented"),
        )
        .await??;
        if matches!(
            notification.item,
            ThreadItem::ManagedAgentActivity { ref activity } if activity.id == "action-1"
        ) {
            break notification;
        }
    };
    assert_eq!(started.delivery, delivery(1));
    assert!(matches!(
        started.item,
        ThreadItem::ManagedAgentActivity { ref activity }
            if activity.event_id == "event-started"
                && activity.phase == Some(AgentManagementPhase::Prepared)
                && activity.status == ManagedAgentActivityStatus::InProgress
    ));

    let completed_response = send_activity(
        &mut app,
        phase_params(
            &thread.id,
            "event-completed",
            2,
            AgentManagementPhase::Complete,
            ManagedAgentActivityStatus::Completed,
        ),
    )
    .await?;
    assert_eq!(
        completed_response.disposition,
        CutexUiActivityIngestionDisposition::Accepted
    );
    let completed = loop {
        let notification: CutexUiActivityNotification = timeout(
            READ_TIMEOUT,
            app.read_notification("cutexActivity/presented"),
        )
        .await??;
        if matches!(
            notification.item,
            ThreadItem::ManagedAgentActivity { ref activity } if activity.id == "action-1"
        ) {
            break notification;
        }
    };
    assert_eq!(completed.delivery, delivery(2));
    assert!(matches!(
        completed.item,
        ThreadItem::ManagedAgentActivity { ref activity }
            if activity.event_id == "event-completed"
                && activity.phase == Some(AgentManagementPhase::Complete)
                && activity.status == ManagedAgentActivityStatus::Completed
    ));

    let duplicate = send_activity(
        &mut app,
        phase_params(
            &thread.id,
            "event-completed",
            2,
            AgentManagementPhase::Complete,
            ManagedAgentActivityStatus::Completed,
        ),
    )
    .await?;
    assert_eq!(
        duplicate.disposition,
        CutexUiActivityIngestionDisposition::Duplicate
    );
    let stale = send_activity(
        &mut app,
        phase_params(
            &thread.id,
            "event-stale",
            1,
            AgentManagementPhase::Prepared,
            ManagedAgentActivityStatus::InProgress,
        ),
    )
    .await?;
    assert_eq!(
        stale.disposition,
        CutexUiActivityIngestionDisposition::Stale
    );

    let read_id = app
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id.clone(),
            include_turns: true,
        })
        .await?;
    let read: ThreadReadResponse = timeout(READ_TIMEOUT, app.read_response(read_id)).await??;
    assert_eq!(read.thread.turns.len(), 1);
    assert_eq!(read.thread.turns[0].id, turn.id);
    assert!(
        read.thread.turns[0]
            .items
            .iter()
            .all(|item| !matches!(item, ThreadItem::ManagedAgentActivity { .. }))
    );

    drop(app);
    let mut replay_app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(READ_TIMEOUT)
        .await?;
    let load_id = replay_app
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id.clone(),
            ..Default::default()
        })
        .await?;
    let _: ThreadResumeResponse =
        timeout(READ_TIMEOUT, replay_app.read_response(load_id)).await??;
    let mut recovered = phase_params(
        &thread.id,
        "event-completed",
        2,
        AgentManagementPhase::Complete,
        ManagedAgentActivityStatus::Completed,
    );
    recovered.delivery.recovered = true;
    let replay_response = send_activity(&mut replay_app, recovered.clone()).await?;
    assert_eq!(
        replay_response.disposition,
        CutexUiActivityIngestionDisposition::Accepted
    );
    let replayed: CutexUiActivityNotification = timeout(
        READ_TIMEOUT,
        replay_app.read_notification("cutexActivity/presented"),
    )
    .await??;
    assert_eq!(replayed.delivery, recovered.delivery);
    let replay_read_id = replay_app
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id,
            include_turns: true,
        })
        .await?;
    let replay: ThreadReadResponse =
        timeout(READ_TIMEOUT, replay_app.read_response(replay_read_id)).await??;
    assert_eq!(replay.thread.turns.len(), 1);
    assert!(
        replay.thread.turns[0]
            .items
            .iter()
            .all(|item| !matches!(item, ThreadItem::ManagedAgentActivity { .. }))
    );

    Ok(())
}

#[tokio::test]
async fn watchdog_stages_update_the_reserved_lane_without_starting_or_persisting_a_turn()
-> Result<()> {
    let server = responses::start_mock_server().await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::Sqlite)
        .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(READ_TIMEOUT)
        .await?;
    let start_id = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            model: Some("mock-model".to_string()),
            history_mode: Some(ThreadHistoryMode::Paginated),
            ..Default::default()
        })
        .await?;
    let ThreadStartResponse { thread, .. } =
        timeout(READ_TIMEOUT, app.read_response(start_id)).await??;

    for (sequence, stage) in [
        (10, TaskWatchdogStage::FirstStale),
        (11, TaskWatchdogStage::DirectorEscalated),
    ] {
        let response =
            send_activity(&mut app, watchdog_params(&thread.id, sequence, stage)).await?;
        assert_eq!(
            response.disposition,
            CutexUiActivityIngestionDisposition::Accepted
        );
        let started = loop {
            let notification: CutexUiActivityNotification = timeout(
                READ_TIMEOUT,
                app.read_notification("cutexActivity/presented"),
            )
            .await??;
            if matches!(
                notification.item,
                ThreadItem::TaskWatchdogActivity { ref activity }
                    if activity.id == "episode-1" && activity.sequence == sequence
            ) {
                break notification;
            }
        };
        assert_eq!(started.delivery, delivery(sequence));
    }

    let duplicate = send_activity(
        &mut app,
        watchdog_params(&thread.id, 11, TaskWatchdogStage::DirectorEscalated),
    )
    .await?;
    assert_eq!(
        duplicate.disposition,
        CutexUiActivityIngestionDisposition::Duplicate
    );
    let stale = send_activity(
        &mut app,
        watchdog_params(&thread.id, 9, TaskWatchdogStage::FirstStale),
    )
    .await?;
    assert_eq!(
        stale.disposition,
        CutexUiActivityIngestionDisposition::Stale
    );

    let read_id = app
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id,
            include_turns: false,
        })
        .await?;
    let read: ThreadReadResponse = timeout(READ_TIMEOUT, app.read_response(read_id)).await??;
    assert!(
        read.thread.turns.is_empty(),
        "watchdog presentation must not create a turn"
    );
    Ok(())
}

async fn send_activity(
    app: &mut TestAppServer,
    params: ThreadCutexActivityParams,
) -> Result<ThreadCutexActivityResponse> {
    let request_id = app
        .send_raw_request("thread/cutexActivity", Some(serde_json::to_value(params)?))
        .await?;
    timeout(READ_TIMEOUT, app.read_response(request_id)).await?
}

fn params(
    thread_id: &str,
    event_id: &str,
    sequence: u64,
    status: ManagedAgentActivityStatus,
) -> ThreadCutexActivityParams {
    ThreadCutexActivityParams {
        thread_id: thread_id.to_string(),
        delivery: delivery(sequence),
        activity: CutexUiActivity::ManagedAgentActivity(Box::new(ManagedAgentActivityItem {
            id: "action-1".to_string(),
            event_id: event_id.to_string(),
            sequence,
            occurred_at_ms: 1_725_000_123_456,
            project_id: Some("project-1".to_string()),
            operation: ManagedAgentOperation::Create,
            status,
            action_id: None,
            phase_event_id: None,
            phase: None,
            managed_agent_id: "cutex.worker-1".to_string(),
            managed_agent_name: Some("Worker".to_string()),
            managed_agent_metadata: None,
            predecessor_agent_id: None,
            predecessor_agent_name: None,
            predecessor_metadata: None,
            successor_agent_id: None,
            successor_agent_name: None,
            successor_metadata: None,
            replace_policy: None,
            rotation_mode: None,
            authority_epoch: None,
            managed_agent_role: Some("worker".to_string()),
            initial_task_preview: Some("implement a focused change".to_string()),
            detail: None,
            runtime_generation: Some(4),
        })),
    }
}

fn phase_params(
    thread_id: &str,
    event_id: &str,
    sequence: u64,
    phase: AgentManagementPhase,
    status: ManagedAgentActivityStatus,
) -> ThreadCutexActivityParams {
    let mut params = params(thread_id, event_id, sequence, status);
    let CutexUiActivity::ManagedAgentActivity(activity) = &mut params.activity else {
        unreachable!("phase helper always starts from managed activity")
    };
    activity.action_id = Some(activity.id.clone());
    activity.phase_event_id = Some(format!("agent-management:{}:phase:{sequence}", activity.id));
    activity.phase = Some(phase);
    params
}

fn watchdog_params(
    thread_id: &str,
    sequence: u64,
    stage: TaskWatchdogStage,
) -> ThreadCutexActivityParams {
    ThreadCutexActivityParams {
        thread_id: thread_id.to_string(),
        delivery: delivery(sequence),
        activity: CutexUiActivity::TaskWatchdogActivity(TaskWatchdogActivityItem {
            id: "episode-1".into(),
            event_id: format!("fact-{sequence}"),
            event_key: stage.event_key().into(),
            sequence,
            occurred_at_ms: 1_787_879_400_000
                + i64::try_from(sequence).expect("test sequence fits in i64"),
            project_id: Some("project-1".into()),
            task_id: "task-1".into(),
            task_revision: 3,
            assignment_id: "assignment-1".into(),
            attempt_number: 2,
            director_agent_id: "cutex.director".into(),
            assignee_agent_id: "cutex.worker".into(),
            assignee_metadata: None,
            activity_watermark: "2026-08-28T01:00:00Z".into(),
            activity_kind: TaskWatchdogActivityKind::LastToolCall,
            idle_duration_secs: if stage == TaskWatchdogStage::FirstStale {
                600
            } else {
                1_200
            },
            stage,
            source_sequence: 41,
        }),
    }
}

fn delivery(sequence: u64) -> CutexUiActivityDelivery {
    let checkpoint = CutexUiActivityCheckpoint {
        stream_id: "management-v2".into(),
        cursor: format!("cursor-{sequence}"),
        sequence,
    };
    CutexUiActivityDelivery {
        schema: CutexUiActivityDeliverySchema::V1,
        class: CutexUiActivityDeliveryClass::Live,
        recovered: false,
        batch_id: format!("live:management-v2:{sequence}:{sequence}"),
        batch_index: 0,
        batch_size: 1,
        source_checkpoint: checkpoint.clone(),
        batch_checkpoint: checkpoint,
    }
}
