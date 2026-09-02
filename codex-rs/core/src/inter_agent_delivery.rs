use codex_history::InterAgentDeliveryReceipt;
use codex_history::InterAgentModelAction;
use codex_history::RolloutItem;
use codex_protocol::ThreadId;
use codex_protocol::protocol::InterAgentCommunication;
use codex_protocol::protocol::InterAgentDeliveryMode;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;

const SEMANTIC_DIGEST_DOMAIN: &[u8] = b"cutex:inter-agent-message-semantic:v1\0";
const RECEIPT_ID_DOMAIN: &[u8] = b"cutex-inter-agent-a4-receipt-v1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterAgentDeliveryQuery {
    pub message_id: String,
    pub semantic_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterAgentDeliveryStatus {
    pub message_id: String,
    pub semantic_sha256: String,
    pub state: InterAgentDeliveryState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterAgentDeliveryState {
    Unknown,
    Pending,
    ContextPersisted(InterAgentDeliveryReceipt),
    Conflict,
    RetryableError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionDecision {
    Admit,
    Retry,
    Duplicate,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistedReceiptPresence {
    Absent,
    Exact,
    Conflict,
}

#[derive(Debug, Clone)]
enum DeliveryEntry {
    Reserved {
        semantic_sha256: String,
    },
    Pending {
        semantic_sha256: String,
    },
    ContextPersisted {
        receipt: InterAgentDeliveryReceipt,
        model_action: Option<ModelActionState>,
    },
    RetryableError {
        semantic_sha256: String,
    },
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelActionState {
    Pending,
    Claimed { turn_id: String },
}

pub(crate) struct InterAgentDeliveryTracker {
    thread_id: ThreadId,
    entries: Mutex<HashMap<String, DeliveryEntry>>,
    next_rollout_ordinal: AtomicU64,
    #[cfg(debug_assertions)]
    fail_live_thread: AtomicU64,
    #[cfg(debug_assertions)]
    fail_append: AtomicU64,
    #[cfg(debug_assertions)]
    fail_flush: AtomicU64,
}

impl InterAgentDeliveryTracker {
    pub(crate) fn from_rollout_items(thread_id: ThreadId, items: &[RolloutItem]) -> Self {
        let mut entries = HashMap::new();
        let mut next_rollout_ordinal = 1;
        for (index, item) in items.iter().enumerate() {
            let RolloutItem::InterAgentCommunicationMetadata {
                trigger_turn,
                delivery_receipt: Some(receipt),
                ..
            } = item
            else {
                continue;
            };
            next_rollout_ordinal =
                next_rollout_ordinal.max(receipt.rollout_ordinal.saturating_add(1));
            let response_item_matches = matches!(
                items.get(index + 1),
                Some(RolloutItem::ResponseItem(item))
                    if item.id().is_some_and(|id| id.as_str() == receipt.response_item_id)
                        && item.turn_id() == Some(receipt.turn_id.as_str())
            );
            if receipt.thread_id != thread_id.to_string()
                || receipt.message_id.trim().is_empty()
                || !valid_semantic_sha256(&receipt.semantic_sha256)
                || receipt.response_item_id.is_empty()
                || receipt.turn_id.is_empty()
                || receipt.rollout_ordinal == 0
                || receipt.receipt_id != receipt_id(receipt)
                || !response_item_matches
            {
                entries.insert(receipt.message_id.clone(), DeliveryEntry::Conflict);
                continue;
            }
            match entries.get(&receipt.message_id) {
                None => {
                    entries.insert(
                        receipt.message_id.clone(),
                        DeliveryEntry::ContextPersisted {
                            receipt: receipt.clone(),
                            // An A4 receipt proves context durability, not A5. Recover the
                            // actionable obligation after restart unless a live request claims it.
                            model_action: (*trigger_turn
                                && !persisted_model_action_completed(items, index, receipt))
                            .then_some(ModelActionState::Pending),
                        },
                    );
                }
                Some(DeliveryEntry::ContextPersisted {
                    receipt: existing, ..
                }) if existing == receipt => {}
                Some(_) => {
                    entries.insert(receipt.message_id.clone(), DeliveryEntry::Conflict);
                }
            }
        }
        Self {
            thread_id,
            entries: Mutex::new(entries),
            next_rollout_ordinal: AtomicU64::new(next_rollout_ordinal),
            #[cfg(debug_assertions)]
            fail_live_thread: AtomicU64::new(0),
            #[cfg(debug_assertions)]
            fail_append: AtomicU64::new(0),
            #[cfg(debug_assertions)]
            fail_flush: AtomicU64::new(0),
        }
    }

    pub(crate) async fn admission(
        &self,
        communication: &InterAgentCommunication,
    ) -> AdmissionDecision {
        let Some(message_id) = canonical_message_id(communication) else {
            return AdmissionDecision::Admit;
        };
        let semantic_sha256 = inter_agent_semantic_sha256(communication);
        let mut entries = self.entries.lock().await;
        match entries.get(message_id) {
            None => {
                entries.insert(
                    message_id.to_string(),
                    DeliveryEntry::Reserved { semantic_sha256 },
                );
                AdmissionDecision::Admit
            }
            Some(DeliveryEntry::Reserved {
                semantic_sha256: existing,
            })
            | Some(DeliveryEntry::Pending {
                semantic_sha256: existing,
            }) if existing == &semantic_sha256 => AdmissionDecision::Duplicate,
            Some(DeliveryEntry::RetryableError {
                semantic_sha256: existing,
            }) if existing == &semantic_sha256 => AdmissionDecision::Retry,
            Some(DeliveryEntry::ContextPersisted { receipt, .. })
                if receipt.semantic_sha256 == semantic_sha256 =>
            {
                AdmissionDecision::Duplicate
            }
            Some(_) => AdmissionDecision::Conflict,
        }
    }

    pub(crate) async fn cancel_admission(&self, communication: &InterAgentCommunication) {
        let Some(message_id) = canonical_message_id(communication) else {
            return;
        };
        let semantic_sha256 = inter_agent_semantic_sha256(communication);
        let mut entries = self.entries.lock().await;
        if matches!(
            entries.get(message_id),
            Some(DeliveryEntry::Reserved {
                semantic_sha256: existing,
            }) if existing == &semantic_sha256
        ) {
            entries.remove(message_id);
        }
    }

    pub(crate) async fn mark_pending(&self, communication: &InterAgentCommunication) {
        let Some(message_id) = canonical_message_id(communication) else {
            return;
        };
        let semantic_sha256 = inter_agent_semantic_sha256(communication);
        let mut entries = self.entries.lock().await;
        match entries.get(message_id) {
            Some(DeliveryEntry::Reserved {
                semantic_sha256: existing,
            }) if existing == &semantic_sha256 => {
                entries.insert(
                    message_id.to_string(),
                    DeliveryEntry::Pending { semantic_sha256 },
                );
            }
            None => {
                entries.insert(
                    message_id.to_string(),
                    DeliveryEntry::Pending { semantic_sha256 },
                );
            }
            Some(_) => {}
        }
    }

    pub(crate) async fn prepare_receipt(
        &self,
        communication: &InterAgentCommunication,
        turn_id: &str,
        response_item_id: &str,
    ) -> Option<InterAgentDeliveryReceipt> {
        let message_id = canonical_message_id(communication)?.to_string();
        let semantic_sha256 = inter_agent_semantic_sha256(communication);
        let mut entries = self.entries.lock().await;
        match entries.get(&message_id) {
            Some(DeliveryEntry::ContextPersisted { .. })
            | Some(DeliveryEntry::RetryableError { .. })
            | Some(DeliveryEntry::Conflict) => return None,
            Some(DeliveryEntry::Reserved {
                semantic_sha256: existing,
            })
            | Some(DeliveryEntry::Pending {
                semantic_sha256: existing,
            }) if existing != &semantic_sha256 => return None,
            Some(DeliveryEntry::Reserved { .. }) | Some(DeliveryEntry::Pending { .. }) => {}
            None => {
                entries.insert(
                    message_id.clone(),
                    DeliveryEntry::Pending {
                        semantic_sha256: semantic_sha256.clone(),
                    },
                );
            }
        }
        drop(entries);
        let rollout_ordinal = self.next_rollout_ordinal.fetch_add(1, Ordering::Relaxed);
        let mut receipt = InterAgentDeliveryReceipt {
            receipt_id: String::new(),
            thread_id: self.thread_id.to_string(),
            message_id,
            semantic_sha256,
            response_item_id: response_item_id.to_string(),
            turn_id: turn_id.to_string(),
            rollout_ordinal,
        };
        receipt.receipt_id = receipt_id(&receipt);
        Some(receipt)
    }

    pub(crate) async fn mark_context_persisted(
        &self,
        receipt: InterAgentDeliveryReceipt,
        actionable: bool,
    ) {
        self.entries.lock().await.insert(
            receipt.message_id.clone(),
            DeliveryEntry::ContextPersisted {
                receipt,
                model_action: actionable.then_some(ModelActionState::Pending),
            },
        );
    }

    pub(crate) async fn has_pending_model_actions(&self) -> bool {
        self.entries.lock().await.values().any(|entry| {
            matches!(
                entry,
                DeliveryEntry::ContextPersisted {
                    model_action: Some(ModelActionState::Pending),
                    ..
                }
            )
        })
    }

    /// Claims only A4 actions whose canonical response item is present in this request snapshot.
    /// The caller must either complete or restore every returned message ID.
    pub(crate) async fn claim_model_actions_for_input(
        &self,
        input: &[codex_protocol::models::ResponseItem],
        turn_id: &str,
    ) -> Vec<InterAgentModelAction> {
        let mut entries = self.entries.lock().await;
        let response_item_ids = input
            .iter()
            .filter_map(codex_protocol::models::ResponseItem::id)
            .map(codex_protocol::ResponseItemId::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut claimed = Vec::new();
        for (message_id, entry) in entries.iter_mut() {
            let DeliveryEntry::ContextPersisted {
                receipt,
                model_action: Some(model_action),
                ..
            } = entry
            else {
                continue;
            };
            if *model_action == ModelActionState::Pending
                && response_item_ids.contains(receipt.response_item_id.as_str())
            {
                *model_action = ModelActionState::Claimed {
                    turn_id: turn_id.to_string(),
                };
                claimed.push(InterAgentModelAction {
                    message_id: message_id.clone(),
                    semantic_sha256: receipt.semantic_sha256.clone(),
                    turn_id: turn_id.to_string(),
                    completed: false,
                });
            }
        }
        claimed
    }

    pub(crate) async fn complete_model_actions(&self, actions: &[InterAgentModelAction]) {
        let mut entries = self.entries.lock().await;
        for action in actions {
            if let Some(DeliveryEntry::ContextPersisted {
                receipt,
                model_action,
            }) = entries.get_mut(&action.message_id)
                && receipt.semantic_sha256 == action.semantic_sha256
                && matches!(
                    model_action,
                    Some(ModelActionState::Claimed { turn_id }) if turn_id == &action.turn_id
                )
            {
                *model_action = None;
            }
        }
    }

    pub(crate) async fn restore_model_actions(&self, actions: &[InterAgentModelAction]) {
        let mut entries = self.entries.lock().await;
        for action in actions {
            if let Some(DeliveryEntry::ContextPersisted {
                receipt,
                model_action,
            }) = entries.get_mut(&action.message_id)
                && receipt.semantic_sha256 == action.semantic_sha256
                && matches!(
                    model_action,
                    Some(ModelActionState::Claimed { turn_id }) if turn_id == &action.turn_id
                )
            {
                *model_action = Some(ModelActionState::Pending);
            }
        }
    }

    pub(crate) async fn restore_model_actions_for_turn(&self, turn_id: &str) {
        let mut entries = self.entries.lock().await;
        for entry in entries.values_mut() {
            if let DeliveryEntry::ContextPersisted { model_action, .. } = entry
                && matches!(
                    model_action,
                    Some(ModelActionState::Claimed { turn_id: claimed }) if claimed == turn_id
                )
            {
                *model_action = Some(ModelActionState::Pending);
            }
        }
    }

    pub(crate) async fn mark_retryable_error(&self, receipt: &InterAgentDeliveryReceipt) {
        self.entries.lock().await.insert(
            receipt.message_id.clone(),
            DeliveryEntry::RetryableError {
                semantic_sha256: receipt.semantic_sha256.clone(),
            },
        );
    }

    pub(crate) async fn mark_conflict(&self, message_id: &str) {
        self.entries
            .lock()
            .await
            .insert(message_id.to_string(), DeliveryEntry::Conflict);
    }

    #[cfg(debug_assertions)]
    pub(crate) fn inject_persistence_failures(&self, live_thread: u64, append: u64, flush: u64) {
        self.fail_live_thread.store(live_thread, Ordering::SeqCst);
        self.fail_append.store(append, Ordering::SeqCst);
        self.fail_flush.store(flush, Ordering::SeqCst);
    }

    pub(crate) fn should_fail_live_thread(&self) -> bool {
        #[cfg(debug_assertions)]
        {
            consume_failure(&self.fail_live_thread)
        }
        #[cfg(not(debug_assertions))]
        false
    }

    pub(crate) fn should_fail_append(&self) -> bool {
        #[cfg(debug_assertions)]
        {
            consume_failure(&self.fail_append)
        }
        #[cfg(not(debug_assertions))]
        false
    }

    pub(crate) fn should_fail_flush(&self) -> bool {
        #[cfg(debug_assertions)]
        {
            consume_failure(&self.fail_flush)
        }
        #[cfg(not(debug_assertions))]
        false
    }

    pub(crate) async fn statuses(
        &self,
        queries: &[InterAgentDeliveryQuery],
    ) -> Vec<InterAgentDeliveryStatus> {
        let entries = self.entries.lock().await;
        queries
            .iter()
            .map(|query| {
                let state = match entries.get(&query.message_id) {
                    None => InterAgentDeliveryState::Unknown,
                    Some(DeliveryEntry::Reserved { semantic_sha256 })
                        if semantic_sha256 == &query.semantic_sha256 =>
                    {
                        InterAgentDeliveryState::Unknown
                    }
                    Some(DeliveryEntry::Pending { semantic_sha256 })
                        if semantic_sha256 == &query.semantic_sha256 =>
                    {
                        InterAgentDeliveryState::Pending
                    }
                    Some(DeliveryEntry::ContextPersisted { receipt, .. })
                        if receipt.semantic_sha256 == query.semantic_sha256 =>
                    {
                        InterAgentDeliveryState::ContextPersisted(receipt.clone())
                    }
                    Some(DeliveryEntry::RetryableError { semantic_sha256 })
                        if semantic_sha256 == &query.semantic_sha256 =>
                    {
                        InterAgentDeliveryState::RetryableError
                    }
                    Some(_) => InterAgentDeliveryState::Conflict,
                };
                InterAgentDeliveryStatus {
                    message_id: query.message_id.clone(),
                    semantic_sha256: query.semantic_sha256.clone(),
                    state,
                }
            })
            .collect()
    }
}

fn persisted_model_action_completed(
    items: &[RolloutItem],
    receipt_index: usize,
    receipt: &InterAgentDeliveryReceipt,
) -> bool {
    items
        .iter()
        .enumerate()
        .skip(receipt_index.saturating_add(2))
        .filter_map(|(index, item)| match item {
            RolloutItem::InterAgentCommunicationMetadata {
                model_action: Some(action),
                ..
            } if action.message_id == receipt.message_id
                && action.semantic_sha256 == receipt.semantic_sha256
                && !action.completed =>
            {
                Some((index, action))
            }
            _ => None,
        })
        .any(|(schedule_index, action)| {
            items
                .iter()
                .skip(schedule_index.saturating_add(1))
                .any(|item| {
                    matches!(
                        item,
                        RolloutItem::InterAgentCommunicationMetadata {
                            model_action: Some(completion),
                            ..
                        } if completion.completed
                            && completion.message_id == action.message_id
                            && completion.semantic_sha256 == action.semantic_sha256
                            && completion.turn_id == action.turn_id
                    )
                })
        })
}

pub(crate) fn persisted_receipt_presence(
    items: &[RolloutItem],
    expected: &InterAgentDeliveryReceipt,
) -> PersistedReceiptPresence {
    let mut found_exact = false;
    for (index, item) in items.iter().enumerate() {
        let RolloutItem::InterAgentCommunicationMetadata {
            delivery_receipt: Some(receipt),
            ..
        } = item
        else {
            continue;
        };
        if receipt.message_id != expected.message_id {
            continue;
        }
        let response_item_matches = matches!(
            items.get(index + 1),
            Some(RolloutItem::ResponseItem(item))
                if item.id().is_some_and(|id| id.as_str() == receipt.response_item_id)
                    && item.turn_id() == Some(receipt.turn_id.as_str())
        );
        if receipt == expected && response_item_matches {
            if found_exact {
                return PersistedReceiptPresence::Conflict;
            }
            found_exact = true;
        } else {
            return PersistedReceiptPresence::Conflict;
        }
    }
    if found_exact {
        PersistedReceiptPresence::Exact
    } else {
        PersistedReceiptPresence::Absent
    }
}

pub fn inter_agent_semantic_sha256(communication: &InterAgentCommunication) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_DIGEST_DOMAIN);
    hash_field(
        &mut hasher,
        canonical_message_id(communication)
            .unwrap_or_default()
            .as_bytes(),
    );
    hash_field(&mut hasher, communication.author.to_string().as_bytes());
    hash_field(&mut hasher, communication.recipient.to_string().as_bytes());
    hash_u64(&mut hasher, communication.other_recipients.len() as u64);
    for recipient in &communication.other_recipients {
        hash_field(&mut hasher, recipient.to_string().as_bytes());
    }
    hash_field(
        &mut hasher,
        delivery_mode_name(communication.resolved_delivery_mode()).as_bytes(),
    );
    hash_field(&mut hasher, communication.content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn canonical_message_id(communication: &InterAgentCommunication) -> Option<&str> {
    communication
        .external_message_id
        .as_deref()
        .or_else(|| {
            communication
                .id
                .as_ref()
                .map(codex_protocol::ResponseItemId::as_str)
        })
        .filter(|id| !id.is_empty())
}

fn delivery_mode_name(mode: InterAgentDeliveryMode) -> &'static str {
    match mode {
        InterAgentDeliveryMode::AfterTurn => "after_turn",
        InterAgentDeliveryMode::Soon => "soon",
        InterAgentDeliveryMode::Passive => "passive",
        InterAgentDeliveryMode::Interrupt => "interrupt",
    }
}

fn receipt_id(receipt: &InterAgentDeliveryReceipt) -> String {
    let mut hasher = Sha256::new();
    hasher.update(RECEIPT_ID_DOMAIN);
    for field in [
        receipt.thread_id.as_bytes(),
        receipt.message_id.as_bytes(),
        receipt.semantic_sha256.as_bytes(),
        receipt.response_item_id.as_bytes(),
        receipt.turn_id.as_bytes(),
    ] {
        hash_field(&mut hasher, field);
    }
    hash_u64(&mut hasher, receipt.rollout_ordinal);
    format!("a4r_{:x}", hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value);
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

fn valid_semantic_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(debug_assertions)]
fn consume_failure(counter: &AtomicU64) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
            remaining.checked_sub(1)
        })
        .is_ok()
}

#[cfg(test)]
#[path = "inter_agent_delivery_tests.rs"]
mod tests;
