use sha2::Digest;
use sha2::Sha256;

const MAX_RESPONSE_ITEM_ID_BYTES: usize = 64;
const NORMALIZED_MESSAGE_ID_PREFIX: &str = "amsg_ext_";
const NORMALIZATION_DOMAIN: &[u8] = b"codex-external-response-item-id-v1\0";

/// Returns an ID that is safe to persist and send as a Responses API item ID.
///
/// Non-empty external message IDs that already fit the provider constraint remain unchanged.
/// Oversized IDs are replaced with a domain-separated digest so retries stay stable and distinct
/// source IDs do not collapse onto the same queue entry. Blank IDs retain the existing rejection
/// contract.
pub(super) fn normalize_external_message_id(message_id: &str) -> Result<String, String> {
    if message_id.trim().is_empty() {
        return Err("messageId must not be empty".to_string());
    }
    if message_id.len() <= MAX_RESPONSE_ITEM_ID_BYTES {
        return Ok(message_id.to_string());
    }

    let mut hasher = Sha256::new();
    hasher.update(NORMALIZATION_DOMAIN);
    hasher.update(message_id.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let digest_len = MAX_RESPONSE_ITEM_ID_BYTES - NORMALIZED_MESSAGE_ID_PREFIX.len();
    Ok(format!(
        "{NORMALIZED_MESSAGE_ID_PREFIX}{}",
        &digest[..digest_len]
    ))
}

#[cfg(test)]
#[path = "external_message_id_tests.rs"]
mod tests;
