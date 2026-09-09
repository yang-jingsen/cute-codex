use super::*;
use pretty_assertions::assert_eq;

fn envelope() -> Envelope {
    let mut envelope = Envelope {
        version: 1,
        owner_id: "owner".into(),
        thread_id: "thread".into(),
        runtime_generation: 7,
        message: Message {
            id: "message".into(),
            source: Source {
                kind: SourceKind::Service,
                id: "service-id".into(),
            },
            event_type: "opaque.v1".into(),
            delivery: Delivery::AfterTurn,
            text: "hello\n世界".into(),
        },
        semantic_sha256: String::new(),
    };
    envelope.semantic_sha256 = envelope.digest();
    envelope
}
fn commit() -> Commit {
    let envelope = envelope();
    let receipt = Receipt::new(&envelope, "turn".into(), 1).unwrap();
    Commit { envelope, receipt }
}
fn record(processing: ProcessingPhase) -> ProcessingRecord {
    let e = envelope();
    ProcessingRecord {
        version: 1,
        owner_id: e.owner_id,
        thread_id: e.thread_id,
        message_id: e.message.id,
        semantic_sha256: e.semantic_sha256,
        processing,
    }
}
#[test]
fn independent_python_digest_vectors_and_no_normalization() {
    // hashlib.sha256(domain + concat(struct.pack('>Q', len(s.encode())) + s.encode())).
    let e = envelope();
    assert_eq!(
        e.digest(),
        "5b066f6337afec950bb316090397702df3c239477ac13768d8a1fd6bf86a400e"
    );
    assert_eq!(
        commit().receipt.receipt_id,
        "eir1_3692b2f248c7c1a6e14112bbf3e6d5c498ee0c737d098c3aed89e42ec7dd93fa"
    );
    let mut changed = e.clone();
    changed.runtime_generation += 1;
    assert_eq!(changed.digest(), e.digest());
    assert_eq!(
        changed.validate_binding("owner", "thread", 7),
        Err(Error::Conflict)
    );
    for text in ["hello\r\n世界", "hello\n世界 ", "hello\n世 界"] {
        changed.message.text = text.into();
        assert_ne!(changed.digest(), e.digest());
    }
    assert_ne!(
        hash_fields(b"d", &["ab", "c"]).finalize(),
        hash_fields(b"d", &["a", "bc"]).finalize()
    );
}
#[test]
fn strict_envelope_and_utf8_boundaries() {
    let e = envelope();
    for role in ["human", "assistant", "system", "developer"] {
        let mut value = serde_json::to_value(&e).unwrap();
        value["message"]["source"]["kind"] = role.into();
        assert!(serde_json::from_value::<Envelope>(value).is_err());
    }
    for path in [
        vec!["role"],
        vec!["message", "permissions"],
        vec!["message", "source", "role"],
    ] {
        let mut value = serde_json::to_value(&e).unwrap();
        let mut cursor = &mut value;
        for key in &path[..path.len() - 1] {
            cursor = &mut cursor[*key];
        }
        cursor[path[path.len() - 1]] = "system".into();
        assert!(serde_json::from_value::<Envelope>(value).is_err());
    }
    let mut changed = e.clone();
    changed.version = 2;
    assert_eq!(changed.validate(), Err(Error::Version));
    changed = e.clone();
    changed.semantic_sha256 = changed.semantic_sha256.to_uppercase();
    assert!(changed.validate().is_err());
    for length in [0, 65536, 65538] {
        changed = e.clone();
        changed.message.text = "é".repeat(length / 2);
        changed.semantic_sha256 = changed.digest();
        assert_eq!(changed.validate().is_ok(), length == 65536);
    }
    for length in [0, 256, 257] {
        changed = e.clone();
        changed.message.source.id = "a".repeat(length);
        changed.semantic_sha256 = changed.digest();
        assert_eq!(changed.validate().is_ok(), length == 256);
    }
    let mut value = serde_json::to_value(&e).unwrap();
    value["message"]["delivery"] = "soon".into();
    assert!(serde_json::from_value::<Envelope>(value).is_err());
}
#[test]
fn canonical_item_excludes_mechanics_and_roundtrips() {
    let e = envelope();
    let item = e.response_item().unwrap();
    let ResponseItem::FunctionCallOutput {
        output, call_id, ..
    } = &item
    else {
        panic!("wrong item");
    };
    assert_eq!(
        output.text_content(),
        Some(
            "{\"source\":{\"kind\":\"service\",\"id\":\"service-id\"},\"type\":\"opaque.v1\",\"text\":\"hello\\n世界\"}"
        )
    );
    assert_eq!(call_id, &None);
    assert_eq!(
        serde_json::from_str::<ResponseItem>(&serde_json::to_string(&item).unwrap()).unwrap(),
        item
    );
    let c = commit();
    assert_eq!(
        serde_json::from_str::<Commit>(&serde_json::to_string(&c).unwrap()).unwrap(),
        c
    );
    assert!(serde_json::from_str::<ResponseItem>(&serde_json::to_string(&c).unwrap()).is_err());
    let p = record(ProcessingPhase::Claim {
        attempt_id: Uuid::new_v4(),
    });
    assert!(serde_json::from_str::<ResponseItem>(&serde_json::to_string(&p).unwrap()).is_err());
    assert_eq!(
        serde_json::from_str::<ProcessingRecord>(&serde_json::to_string(&p).unwrap()).unwrap(),
        p
    );
}
#[test]
fn complete_absent_partial_corrupt_and_conflicting_pairs() {
    let c = commit();
    let item = c.envelope.response_item().unwrap();
    let complete = recover(
        "owner",
        "thread",
        &[
            HistoryEntry::Commit(&c),
            HistoryEntry::Item {
                item: &item,
                turn_id: "turn",
            },
        ],
    )
    .unwrap();
    assert_eq!(complete.next_ordinal, 2);
    let mut restarted = envelope();
    restarted.runtime_generation += 1;
    assert_eq!(complete.lookup(&restarted).unwrap().unwrap().commit, c);
    restarted.message.text.push('!');
    restarted.semantic_sha256 = restarted.digest();
    assert_eq!(complete.lookup(&restarted), Err(Error::Conflict));
    assert_eq!(
        recover("owner", "thread", &[]).unwrap().lookup(&envelope()),
        Ok(None)
    );
    for history in [
        vec![HistoryEntry::Commit(&c)],
        vec![HistoryEntry::Item {
            item: &item,
            turn_id: "turn",
        }],
        vec![
            HistoryEntry::Commit(&c),
            HistoryEntry::Other,
            HistoryEntry::Item {
                item: &item,
                turn_id: "turn",
            },
        ],
    ] {
        assert_eq!(recover("owner", "thread", &history), Err(Error::Corrupt));
    }
    assert_eq!(
        recover(
            "owner",
            "thread",
            &[
                HistoryEntry::Commit(&c),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "different-native-turn"
                }
            ]
        ),
        Err(Error::Corrupt)
    );
    let mut bad = c.clone();
    bad.receipt.response_item_id = "wrong".into();
    assert_eq!(
        recover(
            "owner",
            "thread",
            &[
                HistoryEntry::Commit(&bad),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "turn"
                }
            ]
        ),
        Err(Error::Corrupt)
    );
    bad = c.clone();
    bad.receipt.schema = "v2".into();
    assert_eq!(
        recover(
            "owner",
            "thread",
            &[
                HistoryEntry::Commit(&bad),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "turn"
                }
            ]
        ),
        Err(Error::Version)
    );
    assert_eq!(
        recover(
            "owner",
            "foreign",
            &[
                HistoryEntry::Commit(&c),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "turn"
                }
            ]
        ),
        Err(Error::Conflict)
    );
    assert_eq!(
        recover(
            "owner",
            "thread",
            &[
                HistoryEntry::Commit(&c),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "turn"
                },
                HistoryEntry::Commit(&c),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "turn"
                }
            ]
        ),
        Err(Error::Conflict)
    );
    let mut second = c.clone();
    second.envelope.message.id = "second".into();
    second.envelope.semantic_sha256 = second.envelope.digest();
    second.receipt = Receipt::new(&second.envelope, "turn".into(), 2).unwrap();
    let second_item = second.envelope.response_item().unwrap();
    assert_eq!(
        recover(
            "owner",
            "thread",
            &[
                HistoryEntry::Commit(&c),
                HistoryEntry::Item {
                    item: &item,
                    turn_id: "turn"
                },
                HistoryEntry::Commit(&second),
                HistoryEntry::Item {
                    item: &second_item,
                    turn_id: "turn"
                }
            ]
        )
        .unwrap()
        .next_ordinal,
        3
    );
}
#[test]
fn processing_recovery_holds_uncertainty_and_requires_explicit_retry() {
    let c = commit();
    let item = c.envelope.response_item().unwrap();
    let id = Uuid::new_v4();
    let claim = record(ProcessingPhase::Claim { attempt_id: id });
    let hold = record(ProcessingPhase::Hold {
        attempt_id: Some(id),
        reason: HoldReason::NoOutput,
    });
    let retry = record(ProcessingPhase::Retry {
        expected_attempt_id: Some(id),
        retry_id: "retry".into(),
    });
    let output = record(ProcessingPhase::Output { attempt_id: id });
    let state = |records: &[&ProcessingRecord]| {
        let mut history = vec![
            HistoryEntry::Commit(&c),
            HistoryEntry::Item {
                item: &item,
                turn_id: "turn",
            },
        ];
        history.extend(records.iter().map(|p| HistoryEntry::Processing(p)));
        recover("owner", "thread", &history).map(|r| r.messages["message"].processing.clone())
    };
    assert_eq!(
        state(&[&claim]),
        Ok(Processing::Held(Some(id), HoldReason::RequestUncertain))
    );
    assert_eq!(
        state(&[&claim, &hold]),
        Ok(Processing::Held(Some(id), HoldReason::NoOutput))
    );
    assert_eq!(
        state(&[&claim, &hold, &retry, &retry]),
        Ok(Processing::Pending(Some(id)))
    );
    assert_eq!(
        state(&[&claim, &output]),
        Ok(Processing::OutputObserved(id))
    );
    assert_eq!(state(&[&claim, &retry]), Ok(Processing::Pending(Some(id))));
    assert_eq!(state(&[&output]), Err(Error::Corrupt));
    assert_eq!(state(&[&claim, &claim]), Err(Error::Corrupt));
    assert_eq!(state(&[&claim, &output, &retry]), Err(Error::Corrupt));
    let mut wrong = retry.clone();
    wrong.processing = ProcessingPhase::Retry {
        expected_attempt_id: None,
        retry_id: "retry".into(),
    };
    assert_eq!(
        state(&[&claim, &hold, &retry, &wrong]),
        Err(Error::Conflict)
    );
    let mut bad = claim;
    bad.version = 2;
    assert_eq!(state(&[&bad]), Err(Error::Version));
    let mut passive = c;
    passive.envelope.message.delivery = Delivery::Passive;
    passive.envelope.semantic_sha256 = passive.envelope.digest();
    passive.receipt = Receipt::new(&passive.envelope, "turn".into(), 1).unwrap();
    let passive_item = passive.envelope.response_item().unwrap();
    assert_eq!(
        recover(
            "owner",
            "thread",
            &[
                HistoryEntry::Commit(&passive),
                HistoryEntry::Item {
                    item: &passive_item,
                    turn_id: "turn"
                }
            ]
        )
        .unwrap()
        .messages["message"]
            .processing,
        Processing::None
    );
}
