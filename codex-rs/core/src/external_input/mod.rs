//! Private, common ExternalInput runtime state. Admission is volatile; only
//! committed context and processing facts belong to native history.
mod api;
mod eligibility;
pub(crate) use eligibility::InputBoundary;
pub(crate) use eligibility::SoonBoundary;
#[cfg(all(debug_assertions, unix))]
pub(crate) mod probe;
pub(crate) mod recovery;

use codex_protocol::external_input::Envelope;
use codex_protocol::external_input::Recovery;
use codex_protocol::external_input_record::Record;
use std::collections::BTreeMap;

pub(crate) struct Pending {
    pub envelope: Envelope,
    /// Soon arriving after task installation must wait for an input drain.
    pub running_turn: Option<std::sync::Arc<tokio::sync::Mutex<crate::state::TurnState>>>,
    /// Identity of the active/reserved TurnState at after_turn admission.
    pub excluded_turn: Option<std::sync::Arc<tokio::sync::Mutex<crate::state::TurnState>>>,
}

pub(crate) struct Runtime {
    pub policy: crate::context::CanonicalBytePolicy,
    pub policy_blocked: std::collections::BTreeSet<String>,
    /// One automatic reservation per new admission/retry, including failed turns.
    pub dispatch_revision: u64,
    pub attempted_revision: u64,
    pub retry_exclusions:
        BTreeMap<String, std::sync::Arc<tokio::sync::Mutex<crate::state::TurnState>>>,
    pub changed: tokio::sync::broadcast::Sender<String>,
    pub owner: String,
    pub recovery: Recovery,
    pub pending: Vec<Pending>,
    pub paused: bool,
    /// Set before fallible writes. Cancellation and uncertain completion leave
    /// it set, preventing subsequent sampling until strict process recovery.
    pub poisoned: bool,
    pub retries: BTreeMap<String, Record>,
    pub permits: std::collections::BTreeSet<String>,
}

impl Runtime {
    pub(crate) fn has_dispatch_work(&self) -> bool {
        !self.poisoned
            && self.policy_blocked.is_empty()
            && ((!self.paused
                && self
                    .pending
                    .iter()
                    .any(|pending| pending.envelope.message.delivery.is_active()))
                || self.recovery.messages.iter().any(|(id, found)| {
                    matches!(
                        found.processing,
                        codex_protocol::external_input::Processing::Pending(_)
                    ) && (!self.paused || self.permits.contains(id))
                }))
    }
}
