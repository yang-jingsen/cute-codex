use super::*;
use codex_protocol::config_types::ModeKind;
use codex_protocol::external_input::*;
use codex_protocol::external_input_record::GateReason;
use codex_protocol::external_input_record::MessageKey;
use codex_protocol::protocol::TurnStartedEvent;
use pretty_assertions::assert_eq;
use uuid::Uuid;

fn pair() -> (Commit, Vec<RolloutItem>) {
    let mut envelope = Envelope {
        view: None,
        version: 1,
        owner_id: "owner".into(),
        thread_id: "thread".into(),
        runtime_generation: 7,
        message: Message {
            id: "message".into(),
            source: Source {
                kind: SourceKind::Service,
                id: "fixture".into(),
            },
            event_type: "opaque".into(),
            delivery: Delivery::AfterTurn,
            text: "canonical data".into(),
        },
        semantic_sha256: String::new(),
    };
    envelope.semantic_sha256 = envelope.digest();
    let commit = Commit {
        receipt: Receipt::new(&envelope, "turn".into(), 1).unwrap(),
        envelope,
    };
    let items = vec![
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn".into(),
            trace_id: None,
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        })),
        fact(Fact::Commit {
            commit: Box::new(commit.clone()),
        }),
        RolloutItem::ResponseItem(commit.envelope.response_item().unwrap().into()),
    ];
    (commit, items)
}
fn fact(fact: Fact) -> RolloutItem {
    RolloutItem::ExternalInput(Record {
        version: 1,
        owner_id: "owner".into(),
        thread_id: "thread".into(),
        fact,
    })
}
fn key(commit: &Commit) -> MessageKey {
    MessageKey {
        message_id: commit.envelope.message.id.clone(),
        semantic_sha256: commit.envelope.semantic_sha256.clone(),
    }
}

#[test]
fn native_pair_requires_adjacency_real_turn_and_supported_record_version() {
    let (commit, items) = pair();
    let restored = restore("owner", "thread", &items).unwrap();
    assert_eq!(restored.recovery.messages["message"].commit, commit);
    assert_eq!(restored.recovery.next_ordinal, 2);
    assert!(restore("owner", "thread", &items[..2]).is_err());
    assert!(restore("owner", "thread", &items[1..]).is_err());
    let mut separated = items.clone();
    separated.insert(
        2,
        fact(Fact::DispatchGate {
            paused: true,
            reason: GateReason::Interrupted,
        }),
    );
    assert!(restore("owner", "thread", &separated).is_err());
    let mut unsupported = items;
    let RolloutItem::ExternalInput(record) = &mut unsupported[1] else {
        unreachable!()
    };
    record.version = 2;
    assert!(matches!(
        restore("owner", "thread", &unsupported),
        Err(Error::Version)
    ));
}

#[test]
fn interrupted_claim_is_held_and_retry_replay_does_not_restore_consumed_permit() {
    let (commit, mut items) = pair();
    let attempt = Uuid::from_u128(1);
    items.push(fact(Fact::Claim {
        key: key(&commit),
        attempt_id: attempt,
    }));
    items.push(fact(Fact::DispatchGate {
        paused: true,
        reason: GateReason::Interrupted,
    }));
    let restored = restore("owner", "thread", &items).unwrap();
    assert!(restored.paused);
    assert_eq!(
        restored.recovery.messages["message"].processing,
        Processing::Held(Some(attempt), HoldReason::RequestUncertain)
    );
    let retry = fact(Fact::Retry {
        key: key(&commit),
        expected_attempt_id: Some(attempt),
        retry_id: "retry".into(),
    });
    items.push(retry.clone());
    assert_eq!(
        restore("owner", "thread", &items).unwrap().permits,
        std::collections::BTreeSet::from(["message".into()])
    );
    items.push(fact(Fact::Claim {
        key: key(&commit),
        attempt_id: Uuid::from_u128(2),
    }));
    items.push(retry);
    let restored = restore("owner", "thread", &items).unwrap();
    assert!(restored.permits.is_empty());
    assert_eq!(
        restored.recovery.messages["message"].commit.receipt,
        commit.receipt
    );
}

#[test]
fn migration_checks_complete_history_before_filtering() {
    let (commit, mut items) = pair();
    assert!(ensure_migration_allowed(&items).is_err());
    let attempt = Uuid::from_u128(1);
    items.push(fact(Fact::Claim {
        key: key(&commit),
        attempt_id: attempt,
    }));
    assert!(ensure_migration_allowed(&items).is_err());
    items.push(fact(Fact::Output {
        key: key(&commit),
        attempt_id: attempt,
    }));
    assert_eq!(ensure_migration_allowed(&items), Ok(()));
    assert_eq!(ensure_migration_allowed(&[]), Ok(()));
}

#[test]
fn mechanical_wire_is_strict_and_not_a_response_item() {
    let (_, items) = pair();
    let serialized = serde_json::to_value(&items[1]).unwrap();
    assert_eq!(serialized["type"], "external_input");
    assert_eq!(
        serde_json::from_value::<codex_protocol::models::ResponseItem>(serialized.clone()).unwrap(),
        codex_protocol::models::ResponseItem::Other
    );
    let mut unknown = serialized;
    unknown["payload"]["fact"]["role"] = "system".into();
    assert!(serde_json::from_value::<RolloutItem>(unknown).is_err());
}
