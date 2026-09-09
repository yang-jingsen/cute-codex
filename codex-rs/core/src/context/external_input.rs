//! Canonical external data, deliberately without a chat role.
//!
//! S6b2 Director decision grants a task-scoped exception to AGENTS rule 6:
//! the existing message fragment trait cannot represent a standalone output.
//! Keep this typed sibling local to ExternalInput rather than changing that trait.
use codex_protocol::external_input::Envelope;
use codex_protocol::external_input::Error;
use codex_protocol::models::ResponseItem;

const MAX_CANONICAL_ITEM_BYTES: usize = 10_000;

/// A transport-valid envelope is not sufficient for model admission. This type
/// bounds the entire serialized item, including escaped JSON and identifiers.
/// As with the existing exec-command rejection bound, at most one token per byte
/// is conservative for byte-fallback tokenizers. Callers must establish that
/// tokenizer bound for the selected model; unknown sizing is rejected.
/// P0 review: an individual accepted item can exceed 1,000 tokens.
pub(crate) struct ExternalInputContext {
    item: ResponseItem,
}

impl ExternalInputContext {
    pub(crate) fn new(envelope: &Envelope, byte_fallback_bound: bool) -> Result<Self, Error> {
        if !byte_fallback_bound {
            return Err(Error::Invalid("unknown model item sizing"));
        }
        let item = envelope.response_item()?;
        let serialized =
            serde_json::to_vec(&item).map_err(|_| Error::Invalid("model item serialization"))?;
        if serialized.len() > MAX_CANONICAL_ITEM_BYTES {
            return Err(Error::Invalid("canonical model item token bound"));
        }
        Ok(Self { item })
    }

    pub(crate) fn into_response_item(self) -> ResponseItem {
        self.item
    }
}

#[cfg(test)]
#[path = "external_input_tests.rs"]
mod tests;
