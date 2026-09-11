//! Strict, display-only recovery input, separate from model-context loading.
use super::LocalThreadStore;
use crate::LoadThreadHistoryParams;
use crate::StoredThreadHistory;
use crate::ThreadStoreError;
use crate::ThreadStoreResult;
use codex_protocol::protocol::ThreadHistoryMode;

pub(super) async fn load(
    store: &LocalThreadStore,
    params: LoadThreadHistoryParams,
) -> ThreadStoreResult<StoredThreadHistory> {
    let metadata = super::read_thread::read_thread(
        store,
        crate::ReadThreadParams {
            thread_id: params.thread_id,
            include_archived: params.include_archived,
            include_history: false,
        },
    )
    .await?;
    let mut items = Vec::new();
    if metadata.history_mode == ThreadHistoryMode::Legacy {
        let path = metadata
            .rollout_path
            .ok_or_else(|| error("missing presentation source"))?;
        items = super::read_thread::load_presentation_history_items(&path).await?;
    } else {
        let lineage = store.resolve_rollout_lineage(params.thread_id).await?;
        for segment in lineage.segments() {
            let mut reader = codex_rollout::open_rollout_line_reader(&segment.rollout_path)
                .await
                .map_err(error)?;
            let mut expected = None;
            while let Some(raw) = reader.next_line().await.map_err(error)? {
                if raw.trim().is_empty() {
                    continue;
                }
                let line =
                    codex_rollout::decode_rollout_line(serde_json::from_str(&raw).map_err(error)?)
                        .map_err(error)?;
                let ordinal = line
                    .ordinal
                    .ok_or_else(|| error("missing source ordinal"))?;
                if segment.end_ordinal().is_some_and(|end| ordinal >= end) {
                    break;
                }
                if ordinal < segment.start_ordinal() {
                    continue;
                }
                if expected.is_some_and(|next| next != ordinal) {
                    return Err(error("noncontiguous presentation source"));
                }
                expected = Some(
                    ordinal
                        .checked_add(1)
                        .ok_or_else(|| error("source ordinal overflow"))?,
                );
                items.push(line.item);
            }
        }
    }
    let mut identities = std::collections::BTreeMap::new();
    for item in &items {
        if let codex_rollout::RolloutItem::EventMsg(
            codex_protocol::protocol::EventMsg::PresentationAppended(record),
        ) = item
        {
            record.validate().map_err(error)?;
            if let Some(old) = identities.insert(&record.presentation.id, record)
                && old != record
            {
                return Err(error("conflicting presentation identity"));
            }
        }
    }
    Ok(StoredThreadHistory {
        thread_id: params.thread_id,
        items,
    })
}
fn error(value: impl std::fmt::Display) -> ThreadStoreError {
    ThreadStoreError::Internal {
        message: format!("presentation history: {value}"),
    }
}
