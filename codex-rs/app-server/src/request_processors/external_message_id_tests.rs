use pretty_assertions::assert_eq;

use super::MAX_RESPONSE_ITEM_ID_BYTES;
use super::normalize_external_message_id;

const TASK_SERVICE_RECOVERY_ID: &str =
    "amsg_tsc_tsn-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const NORMALIZED_TASK_SERVICE_RECOVERY_ID: &str =
    "amsg_ext_5a4c996908528125c6973aa905caaeb96fae8dec05d50acd44be7e0";

#[test]
fn task_service_recovery_id_normalizes_stably_within_provider_limit() {
    assert_eq!(TASK_SERVICE_RECOVERY_ID.len(), 77);

    let first = normalize_external_message_id(TASK_SERVICE_RECOVERY_ID).expect("normalize ID");
    let replay = normalize_external_message_id(TASK_SERVICE_RECOVERY_ID).expect("normalize replay");

    assert_eq!(first, NORMALIZED_TASK_SERVICE_RECOVERY_ID);
    assert_eq!(replay, NORMALIZED_TASK_SERVICE_RECOVERY_ID);
    assert_eq!(first.len(), MAX_RESPONSE_ITEM_ID_BYTES);
}

#[test]
fn distinct_oversized_ids_do_not_collapse() {
    let first = format!("amsg_external_{}", "a".repeat(80));
    let second = format!("amsg_external_{}", "b".repeat(80));

    let first = normalize_external_message_id(&first).expect("normalize first ID");
    let second = normalize_external_message_id(&second).expect("normalize second ID");

    assert_ne!(first, second);
    assert!(first.len() <= MAX_RESPONSE_ITEM_ID_BYTES);
    assert!(second.len() <= MAX_RESPONSE_ITEM_ID_BYTES);
}

#[test]
fn valid_ids_pass_through_byte_for_byte() {
    let boundary_id = "x".repeat(MAX_RESPONSE_ITEM_ID_BYTES);
    for message_id in ["mail.valid:source/id?1", boundary_id.as_str()] {
        assert_eq!(
            normalize_external_message_id(message_id).expect("preserve valid ID"),
            message_id
        );
    }
}

#[test]
fn empty_source_id_remains_rejected() {
    assert_eq!(
        normalize_external_message_id(" \t\n"),
        Err("messageId must not be empty".to_string())
    );
}
