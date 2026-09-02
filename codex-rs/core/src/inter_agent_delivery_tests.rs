use super::*;
use codex_protocol::AgentPath;
use codex_protocol::ResponseItemId;

fn communication(content: &str) -> InterAgentCommunication {
    let mut communication = InterAgentCommunication::new_with_delivery_mode(
        AgentPath::root().join("sender").unwrap(),
        AgentPath::root().join("recipient").unwrap(),
        vec![AgentPath::root().join("observer").unwrap()],
        content.to_string(),
        InterAgentDeliveryMode::Soon,
    );
    communication.id = Some(ResponseItemId::from_server("mail-1".to_string()));
    communication.external_message_id = Some("external-mail-1".to_string());
    communication
}

#[tokio::test]
async fn a2_a3_append_flush_and_restart_barriers_publish_only_durable_a4() {
    let thread_id = ThreadId::new();
    let tracker = InterAgentDeliveryTracker::from_rollout_items(thread_id, &[]);
    let communication = communication("canonical content");
    let digest = inter_agent_semantic_sha256(&communication);
    let query = InterAgentDeliveryQuery {
        message_id: "external-mail-1".to_string(),
        semantic_sha256: digest.clone(),
    };
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::Unknown
    );

    tracker.mark_pending(&communication).await;
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::Pending
    );
    let receipt = tracker
        .prepare_receipt(&communication, "turn-1", "mail-1")
        .await
        .unwrap();
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::Pending,
        "preparing a receipt before the rollout flush must not publish A4"
    );
    tracker
        .mark_context_persisted(receipt.clone(), /*actionable*/ false)
        .await;
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::ContextPersisted(receipt.clone())
    );

    let marker = RolloutItem::InterAgentCommunicationMetadata {
        trigger_turn: true,
        delivery_receipt: Some(receipt.clone()),
        model_action: None,
    };
    let truncated =
        InterAgentDeliveryTracker::from_rollout_items(thread_id, std::slice::from_ref(&marker));
    assert_eq!(
        truncated.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::Conflict,
        "a receipt marker without its following canonical response item must fail closed"
    );
    let mut response_item = communication.to_model_input_item();
    response_item.set_turn_id_if_missing("turn-1");
    let rebuilt = InterAgentDeliveryTracker::from_rollout_items(
        thread_id,
        &[
            marker.clone(),
            RolloutItem::ResponseItem(response_item.clone().into()),
        ],
    );
    assert_eq!(
        rebuilt.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::ContextPersisted(receipt.clone())
    );
    assert!(
        rebuilt.has_pending_model_actions().await,
        "restart must recover the actionable A4-to-A5 obligation"
    );
    let claimed = rebuilt
        .claim_model_actions_for_input(std::slice::from_ref(&response_item), "a5-turn-1")
        .await;
    assert_eq!(
        claimed,
        vec![InterAgentModelAction {
            message_id: "external-mail-1".to_string(),
            semantic_sha256: digest.clone(),
            turn_id: "a5-turn-1".to_string(),
            completed: false,
        }]
    );
    assert!(!rebuilt.has_pending_model_actions().await);
    rebuilt.restore_model_actions_for_turn("a5-turn-1").await;
    assert!(rebuilt.has_pending_model_actions().await);
    let reclaimed = rebuilt
        .claim_model_actions_for_input(std::slice::from_ref(&response_item), "a5-turn-2")
        .await;
    rebuilt.restore_model_actions(&reclaimed).await;
    let reclaimed = rebuilt
        .claim_model_actions_for_input(std::slice::from_ref(&response_item), "a5-turn-3")
        .await;
    rebuilt.complete_model_actions(&reclaimed).await;
    assert!(!rebuilt.has_pending_model_actions().await);

    let mut same_turn_user_steer = codex_protocol::models::ResponseItem::Message {
        id: Some(ResponseItemId::from_server("msg_a5_output_1".to_string())),
        role: "user".to_string(),
        content: vec![codex_protocol::models::ContentItem::OutputText {
            text: "same-turn steer is not model output".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    };
    same_turn_user_steer.set_turn_id_if_missing("a5-turn-3");
    let mut premature_completion = reclaimed[0].clone();
    premature_completion.completed = true;
    let rebuilt_after_steer = InterAgentDeliveryTracker::from_rollout_items(
        thread_id,
        &[
            marker.clone(),
            RolloutItem::ResponseItem(response_item.clone().into()),
            RolloutItem::InterAgentCommunicationMetadata {
                trigger_turn: false,
                delivery_receipt: None,
                model_action: Some(premature_completion),
            },
            RolloutItem::InterAgentCommunicationMetadata {
                trigger_turn: false,
                delivery_receipt: None,
                model_action: Some(reclaimed[0].clone()),
            },
            RolloutItem::ResponseItem(same_turn_user_steer.into()),
        ],
    );
    assert!(
        rebuilt_after_steer.has_pending_model_actions().await,
        "a premature completion or same-turn user steer must not falsely discharge A5 on restart"
    );

    let mut completion = reclaimed[0].clone();
    completion.completed = true;
    let rebuilt_after_a5 = InterAgentDeliveryTracker::from_rollout_items(
        thread_id,
        &[
            marker,
            RolloutItem::ResponseItem(response_item.into()),
            RolloutItem::InterAgentCommunicationMetadata {
                trigger_turn: false,
                delivery_receipt: None,
                model_action: Some(reclaimed[0].clone()),
            },
            RolloutItem::InterAgentCommunicationMetadata {
                trigger_turn: false,
                delivery_receipt: None,
                model_action: Some(completion),
            },
        ],
    );
    assert!(
        !rebuilt_after_a5.has_pending_model_actions().await,
        "only an explicit durable completion record may discharge A5 after restart"
    );

    let changed = InterAgentDeliveryQuery {
        semantic_sha256: "f".repeat(64),
        ..query
    };
    assert_eq!(
        rebuilt.statuses(&[changed]).await[0].state,
        InterAgentDeliveryState::Conflict
    );
}

#[test]
fn semantic_digest_excludes_presentation_and_binds_the_canonical_tuple() {
    let canonical = communication("canonical content");
    let expected = inter_agent_semantic_sha256(&canonical);
    let mut presentation_changed = canonical.clone();
    presentation_changed.model_visible_content = Some("projected content".to_string());
    assert_eq!(inter_agent_semantic_sha256(&presentation_changed), expected);

    for changed in [
        communication("changed content"),
        {
            let mut changed = canonical.clone();
            changed.delivery_mode = Some(InterAgentDeliveryMode::AfterTurn);
            changed
        },
        {
            let mut changed = canonical;
            changed.external_message_id = Some("external-mail-2".to_string());
            changed
        },
    ] {
        assert_ne!(inter_agent_semantic_sha256(&changed), expected);
    }
}

#[test]
fn semantic_digest_matches_the_cross_runtime_golden_vector() {
    let mut golden = InterAgentCommunication::new_with_delivery_mode(
        AgentPath::root().join("sender").unwrap(),
        AgentPath::root().join("recipient").unwrap(),
        vec![AgentPath::root().join("observer").unwrap()],
        "hello from sender".to_string(),
        InterAgentDeliveryMode::Passive,
    );
    golden.id = Some(ResponseItemId::from_server("mail_a4_restart_1".to_string()));
    golden.external_message_id = Some("mail_a4_restart_1".to_string());

    assert_eq!(
        inter_agent_semantic_sha256(&golden),
        "8b677efc23d0fe2c4b11d23d4fa0c2af7049d9afa37aa5de2b214b9d6c83bdef"
    );
}

#[tokio::test]
async fn retryable_is_not_a4_and_same_id_changed_body_conflicts() {
    let tracker = InterAgentDeliveryTracker::from_rollout_items(ThreadId::new(), &[]);
    let mail = communication("canonical content");
    tracker.mark_pending(&mail).await;
    let receipt = tracker
        .prepare_receipt(&mail, "turn-1", "mail-1")
        .await
        .unwrap();
    tracker.mark_retryable_error(&receipt).await;
    let matching = InterAgentDeliveryQuery {
        message_id: receipt.message_id.clone(),
        semantic_sha256: receipt.semantic_sha256.clone(),
    };
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&matching)).await[0].state,
        InterAgentDeliveryState::RetryableError
    );
    assert_eq!(
        tracker.admission(&mail).await,
        AdmissionDecision::Retry,
        "same-semantics retryable delivery must be redrivable"
    );
    assert_eq!(
        tracker.admission(&communication("changed content")).await,
        AdmissionDecision::Conflict
    );
}

#[tokio::test]
async fn admission_is_atomic_and_late_pending_cannot_regress_a4() {
    let tracker = InterAgentDeliveryTracker::from_rollout_items(ThreadId::new(), &[]);
    let mail = communication("canonical content");
    let query = InterAgentDeliveryQuery {
        message_id: "external-mail-1".to_string(),
        semantic_sha256: inter_agent_semantic_sha256(&mail),
    };

    assert_eq!(tracker.admission(&mail).await, AdmissionDecision::Admit);
    assert_eq!(
        tracker.admission(&mail).await,
        AdmissionDecision::Duplicate,
        "the first admission must reserve the ID before enqueue completes"
    );
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::Unknown,
        "a reservation is not mailbox admission"
    );

    let receipt = tracker
        .prepare_receipt(&mail, "turn-1", "mail-1")
        .await
        .unwrap();
    tracker
        .mark_context_persisted(receipt.clone(), /*actionable*/ false)
        .await;
    tracker.mark_pending(&mail).await;
    assert_eq!(
        tracker.statuses(std::slice::from_ref(&query)).await[0].state,
        InterAgentDeliveryState::ContextPersisted(receipt.clone()),
        "a fast consumer must not be regressed by the later A3 transition"
    );

    let second = communication("second message");
    assert_eq!(
        tracker.admission(&second).await,
        AdmissionDecision::Conflict
    );
    tracker.cancel_admission(&mail).await;
    assert_eq!(
        tracker.statuses(&[query]).await[0].state,
        InterAgentDeliveryState::ContextPersisted(receipt)
    );
}
