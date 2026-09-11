//! Display-only live and replay consumer. Never submits an operation or model item.
use super::*;
use codex_app_server_protocol::ThreadTimelineEntry;
use codex_protocol::presentation::PresentationAppended;

impl ChatWidget {
    pub(super) fn on_presentation(&mut self, record: PresentationAppended) {
        if let Err(error) = record.validate() {
            self.add_error_message(format!("Invalid durable notice: {error}"));
            return;
        }
        let identity = (
            record.origin_thread_id.clone(),
            record.presentation.id.clone(),
        );
        if let Some(previous) = self.transcript.presentations_seen.get(&identity) {
            if previous != &record {
                self.add_error_message(
                    "Conflicting durable notice identity; original notice retained.".into(),
                );
            }
            return;
        }
        match history_cell::PresentationHistoryCell::new(record.clone()) {
            Ok(cell) => {
                self.transcript.presentations_seen.insert(identity, record);
                self.add_to_history(cell);
            }
            Err(error) => self.add_error_message(format!("Invalid durable notice: {error}")),
        }
    }

    pub(super) fn replay_presentation_timeline(
        &mut self,
        turns: Vec<Turn>,
        timeline: Vec<ThreadTimelineEntry>,
        replay_kind: ReplayKind,
    ) {
        let hidden: std::collections::HashSet<String> = turns
            .windows(2)
            .filter(|pair| crate::app_backtrack::is_hidden_nested_review_turn(&pair[0], &pair[1]))
            .map(|pair| pair[1].id.clone())
            .collect();
        for entry in timeline {
            match entry {
                ThreadTimelineEntry::Presentation { item, .. } => self.on_presentation(item),
                ThreadTimelineEntry::Item { turn_id, item, .. } => {
                    if hidden.contains(&turn_id)
                        && matches!(item.as_ref(), ThreadItem::UserMessage { .. })
                    {
                        continue;
                    }
                    self.replay_thread_item(*item, turn_id, replay_kind);
                }
                ThreadTimelineEntry::TurnStarted { turn_id, .. } => {
                    if turns
                        .iter()
                        .any(|t| t.id == turn_id && t.status == TurnStatus::InProgress)
                    {
                        self.turn_lifecycle.last_turn_id = Some(turn_id);
                        self.last_non_retry_error = None;
                        self.on_task_started();
                    }
                }
                ThreadTimelineEntry::TurnCompleted {
                    turn_id,
                    status,
                    error,
                    started_at,
                    completed_at,
                    duration_ms,
                    ..
                } => {
                    self.handle_turn_completed_notification(
                        TurnCompletedNotification {
                            thread_id: self.thread_id.map(|id| id.to_string()).unwrap_or_default(),
                            turn: Turn {
                                id: turn_id.clone(),
                                items_view: codex_app_server_protocol::TurnItemsView::NotLoaded,
                                items: Vec::new(),
                                status: if hidden.contains(&turn_id) {
                                    TurnStatus::Completed
                                } else {
                                    status
                                },
                                error,
                                started_at,
                                completed_at,
                                duration_ms,
                            },
                        },
                        Some(replay_kind),
                    );
                }
                // Existing ordinary turn replay did not project realtime entries.
                ThreadTimelineEntry::Realtime { .. } => {}
            }
        }
    }
}
