//! Read-only legacy timeline reconstruction; never converts the source rollout.
use super::super::LocalThreadStore;
use super::realtime::TimelineCursor;
use super::realtime::entry_key;
use super::thread_history_error;
use crate::ListTimelineParams;
use crate::ThreadStoreResult;
use crate::TimelinePage;
use codex_app_server_protocol::ThreadHistoryBuilder;
use codex_app_server_protocol::ThreadTimelineEntry;
use codex_app_server_protocol::TurnStatus;
use codex_protocol::protocol::EventMsg;
use codex_rollout::RolloutItem;
use std::collections::BTreeMap;

pub(super) async fn list(
    store: &LocalThreadStore,
    params: ListTimelineParams,
) -> ThreadStoreResult<TimelinePage> {
    let source = super::super::thread_rollout_resolver::resolve_current(store, params.thread_id)
        .await?
        .ok_or(crate::ThreadStoreError::ThreadNotFound {
            thread_id: params.thread_id,
        })?;
    // This reader rejects undecodable rows before deriving positions or absence.
    let items = super::super::read_thread::load_presentation_history_items(&source.path).await?;
    let mut builder = ThreadHistoryBuilder::new();
    let mut ordinary = BTreeMap::new();
    let mut starts = BTreeMap::new();
    let mut ends = BTreeMap::new();
    let mut displays = BTreeMap::new();
    for (position, item) in items.iter().enumerate() {
        let position = position as u64;
        if let RolloutItem::EventMsg(EventMsg::PresentationAppended(record)) = item {
            record.validate().map_err(thread_history_error)?;
            displays
                .entry(record.presentation.id.clone())
                .or_insert_with(|| ThreadTimelineEntry::Presentation {
                    position,
                    item: record.clone(),
                });
        }
        let changes = builder.handle_rollout_item_with_changes(item);
        for removed in changes.removed_turn_ids {
            starts.remove(&removed);
            ends.remove(&removed);
            ordinary.retain(|(turn, _), _| turn != &removed);
        }
        for changed in changes.changed_items {
            let key = (changed.turn_id.clone(), changed.item.id().to_string());
            let entry = ordinary
                .entry(key)
                .or_insert_with(|| ThreadTimelineEntry::Item {
                    position,
                    turn_id: changed.turn_id,
                    item: Box::new(changed.item.clone()),
                });
            if let ThreadTimelineEntry::Item { item, .. } = entry {
                **item = changed.item;
            }
        }
        for turn in changes.changed_turns {
            starts.entry(turn.turn_id.clone()).or_insert_with(|| {
                ThreadTimelineEntry::TurnStarted {
                    position,
                    turn_id: turn.turn_id.clone(),
                    started_at: turn.started_at,
                }
            });
            if turn.status != TurnStatus::InProgress {
                ends.insert(
                    turn.turn_id.clone(),
                    ThreadTimelineEntry::TurnCompleted {
                        position,
                        turn_id: turn.turn_id,
                        status: turn.status,
                        error: turn.error,
                        started_at: turn.started_at,
                        completed_at: turn.completed_at,
                        duration_ms: turn.duration_ms,
                    },
                );
            }
        }
    }
    builder.finish_checked().map_err(thread_history_error)?;
    let mut entries = ordinary
        .into_values()
        .chain(starts.into_values())
        .chain(ends.into_values())
        .chain(displays.into_values())
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| entry_key(b).cmp(&entry_key(a)));
    if let Some(cursor) = params.cursor {
        let cursor: TimelineCursor = serde_json::from_str(&cursor).map_err(thread_history_error)?;
        if cursor.thread_id != params.thread_id || cursor.kind > 4 {
            return Err(thread_history_error("invalid legacy timeline cursor"));
        }
        entries
            .retain(|entry| entry_key(entry) < (cursor.position, cursor.kind, cursor.id.as_str()));
    }
    let more = entries.len() > params.page_size;
    entries.truncate(params.page_size);
    let next_cursor = if more {
        entries
            .last()
            .map(|entry| {
                let (position, kind, id) = entry_key(entry);
                serde_json::to_string(&TimelineCursor {
                    thread_id: params.thread_id,
                    position,
                    kind,
                    id: id.to_owned(),
                })
                .map_err(thread_history_error)
            })
            .transpose()?
    } else {
        None
    };
    entries.reverse();
    Ok(TimelinePage {
        items: entries,
        next_cursor,
        active_realtime_session_at_page_start: None,
    })
}
