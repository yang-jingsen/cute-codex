use super::*;
use codex_protocol::external_input::Delivery;
use codex_protocol::external_input::Message;
use codex_protocol::external_input::Source;
use codex_protocol::external_input::SourceKind;

fn envelope(text: String) -> Envelope {
    let mut envelope = Envelope {
        version: 1,
        owner_id: "owner".into(),
        thread_id: "thread".into(),
        runtime_generation: 7,
        message: Message {
            id: "message".into(),
            source: Source {
                kind: SourceKind::Service,
                id: "source".into(),
            },
            event_type: "opaque.v1".into(),
            delivery: Delivery::AfterTurn,
            text,
        },
        semantic_sha256: String::new(),
    };
    envelope.semantic_sha256 = envelope.digest();
    envelope
}

#[test]
fn canonical_item_bound_includes_escaping_and_envelope_overhead() {
    for unit in ["x", "世界", "\n", "\"", "\\", "\u{0000}"] {
        let mut last_good = None;
        for n in 1..=MAX_CANONICAL_ITEM_BYTES {
            let envelope = envelope(unit.repeat(n));
            let expected = envelope.response_item().unwrap();
            let bytes = serde_json::to_vec(&expected).unwrap().len();
            let result = ExternalInputContext::new(&envelope, true);
            if bytes > MAX_CANONICAL_ITEM_BYTES {
                assert!(result.is_err());
                assert!(last_good.is_some());
                break;
            }
            let item = result.unwrap().into_response_item();
            assert_eq!(item, expected);
            assert!(matches!(
                item,
                ResponseItem::FunctionCallOutput { call_id: None, .. }
            ));
            last_good = Some(bytes);
        }
    }
}

#[test]
fn unknown_sizing_rejects_even_small_valid_input() {
    assert!(matches!(
        ExternalInputContext::new(&envelope("hello".into()), false),
        Err(Error::Invalid("unknown model item sizing"))
    ));
}
