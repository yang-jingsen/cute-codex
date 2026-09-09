use codex_history::RolloutItem;
use codex_protocol::external_input::Error;
use codex_protocol::external_input::HistoryEntry;
use codex_protocol::external_input::ProcessingPhase;
use codex_protocol::external_input::ProcessingRecord;
use codex_protocol::external_input::Recovery;
use codex_protocol::external_input::recover;
use codex_protocol::external_input_record::Fact;
use codex_protocol::external_input_record::Record;
use codex_protocol::protocol::EventMsg;

pub(crate) struct Restored {
    pub recovery: Recovery,
    pub paused: bool,
    pub permits: std::collections::BTreeSet<String>,
}

fn processing(record: &Record) -> Option<ProcessingRecord> {
    let (key, processing) = match &record.fact {
        Fact::Claim { key, attempt_id } => (
            key,
            ProcessingPhase::Claim {
                attempt_id: *attempt_id,
            },
        ),
        Fact::Output { key, attempt_id } => (
            key,
            ProcessingPhase::Output {
                attempt_id: *attempt_id,
            },
        ),
        Fact::Hold {
            key,
            attempt_id,
            reason,
        } => (
            key,
            ProcessingPhase::Hold {
                attempt_id: *attempt_id,
                reason: reason.clone(),
            },
        ),
        Fact::Retry {
            key,
            expected_attempt_id,
            retry_id,
        } => (
            key,
            ProcessingPhase::Retry {
                expected_attempt_id: *expected_attempt_id,
                retry_id: retry_id.clone(),
            },
        ),
        Fact::Commit { .. } | Fact::DispatchGate { .. } => return None,
    };
    Some(ProcessingRecord {
        version: record.version,
        owner_id: record.owner_id.clone(),
        thread_id: record.thread_id.clone(),
        message_id: key.message_id.clone(),
        semantic_sha256: key.semantic_sha256.clone(),
        processing,
    })
}

/// Caller must supply the complete, strictly parsed native history. In particular,
/// the ordinary tolerant rollout reader's discarded parse errors are not absence.
pub(crate) fn restore(owner: &str, thread: &str, items: &[RolloutItem]) -> Result<Restored, Error> {
    let processing: Vec<_> = items
        .iter()
        .map(|item| match item {
            RolloutItem::ExternalInput(record) => processing(record),
            _ => None,
        })
        .collect();
    let mut entries = Vec::with_capacity(items.len());
    let mut turn_id = "";
    let mut paused = false;
    let mut permits = std::collections::BTreeSet::new();
    let mut retries = std::collections::BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        entries.push(match item {
            RolloutItem::ExternalInput(record) => {
                if record.version != 1 {
                    return Err(Error::Version);
                }
                if record.owner_id != owner || record.thread_id != thread {
                    return Err(Error::Conflict);
                }
                match &record.fact {
                    Fact::Commit { commit } => HistoryEntry::Commit(commit),
                    Fact::DispatchGate { paused: next, .. } => {
                        paused = *next;
                        if paused {
                            permits.clear();
                        }
                        HistoryEntry::Other
                    }
                    fact => {
                        match fact {
                            Fact::Retry { key, retry_id, .. }
                                if retries.insert(retry_id.clone()) =>
                            {
                                permits.insert(key.message_id.clone());
                            }
                            Fact::Claim { key, .. } => {
                                permits.remove(&key.message_id);
                            }
                            _ => {}
                        }
                        HistoryEntry::Processing(processing[index].as_ref().ok_or(Error::Corrupt)?)
                    }
                }
            }
            RolloutItem::ResponseItem(envelope) => HistoryEntry::Item {
                item: &envelope.item,
                turn_id,
            },
            RolloutItem::EventMsg(EventMsg::TurnStarted(event)) => {
                turn_id = &event.turn_id;
                HistoryEntry::Other
            }
            RolloutItem::EventMsg(EventMsg::TurnComplete(_) | EventMsg::TurnAborted(_)) => {
                turn_id = "";
                HistoryEntry::Other
            }
            _ => HistoryEntry::Other,
        });
    }
    Ok(Restored {
        recovery: recover(owner, thread, &entries)?,
        paused,
        permits,
    })
}

/// Validate a complete source snapshot before any fork/rollback filtering.
pub(crate) fn ensure_migration_allowed(items: &[RolloutItem]) -> Result<(), Error> {
    let Some(record) = items.iter().find_map(|item| {
        if let RolloutItem::ExternalInput(record) = item {
            Some(record)
        } else {
            None
        }
    }) else {
        return Ok(());
    };
    let restored = restore(&record.owner_id, &record.thread_id, items)?;
    if restored.recovery.messages.values().any(|found| {
        !matches!(
            found.processing,
            codex_protocol::external_input::Processing::None
                | codex_protocol::external_input::Processing::OutputObserved(_)
        )
    }) {
        return Err(Error::Invalid("unfinished external input migration"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
