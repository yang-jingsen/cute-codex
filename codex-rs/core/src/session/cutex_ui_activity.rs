use codex_history::RolloutItem;
use codex_protocol::items::CutexUiActivity;
use codex_protocol::items::TurnItem;
use codex_protocol::protocol::CutexUiActivityDisposition;
use codex_protocol::protocol::EventMsg;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

const RECENT_EVENT_IDS_CAPACITY: usize = 4_096;

/// Session-local idempotency and ordering state for UI-only Cutex events.
///
/// The state is rebuilt from persisted item lifecycles on resume. It never records response items
/// and therefore cannot affect model context reconstruction.
#[derive(Default)]
pub(super) struct CutexUiActivityTracker {
    recent_event_ids: VecDeque<String>,
    recent_event_id_set: HashSet<String>,
    latest_sequence_by_item: HashMap<String, u64>,
}

impl CutexUiActivityTracker {
    pub(super) fn from_rollout_items(items: &[RolloutItem]) -> Self {
        let mut tracker = Self {
            recent_event_ids: VecDeque::new(),
            recent_event_id_set: HashSet::new(),
            latest_sequence_by_item: HashMap::new(),
        };
        for item in items {
            let turn_item = match item {
                RolloutItem::EventMsg(EventMsg::ItemStarted(event)) => Some(&event.item),
                RolloutItem::EventMsg(EventMsg::ItemCompleted(event)) => Some(&event.item),
                _ => None,
            };
            if let Some(turn_item) = turn_item {
                tracker.observe_turn_item(turn_item);
            }
        }
        tracker
    }

    pub(super) fn register(&mut self, activity: &CutexUiActivity) -> CutexUiActivityDisposition {
        if self.recent_event_id_set.contains(activity.event_id()) {
            return CutexUiActivityDisposition::Duplicate;
        }

        let item_key = activity_item_key(activity);
        let disposition = if self
            .latest_sequence_by_item
            .get(&item_key)
            .is_some_and(|sequence| *sequence >= activity.sequence())
        {
            CutexUiActivityDisposition::Stale
        } else {
            self.latest_sequence_by_item
                .insert(item_key, activity.sequence());
            CutexUiActivityDisposition::Accepted
        };
        self.remember_event_id(activity.event_id().to_string());
        disposition
    }

    fn observe_turn_item(&mut self, item: &TurnItem) {
        let Some((event_id, item_key, sequence)) = turn_item_identity(item) else {
            return;
        };
        self.latest_sequence_by_item
            .entry(item_key)
            .and_modify(|current| *current = (*current).max(sequence))
            .or_insert(sequence);
        self.remember_event_id(event_id.to_string());
    }

    fn remember_event_id(&mut self, event_id: String) {
        if !self.recent_event_id_set.insert(event_id.clone()) {
            return;
        }
        self.recent_event_ids.push_back(event_id);
        while self.recent_event_ids.len() > RECENT_EVENT_IDS_CAPACITY {
            if let Some(expired) = self.recent_event_ids.pop_front() {
                self.recent_event_id_set.remove(&expired);
            }
        }
    }
}

fn activity_item_key(activity: &CutexUiActivity) -> String {
    let kind = match activity {
        CutexUiActivity::ManagedAgentActivity(_) => "managed",
        CutexUiActivity::OutboundInterAgentMessage(_) => "outbound",
        CutexUiActivity::TaskAssignmentActivity(_) => "assignment",
        CutexUiActivity::TaskWatchdogActivity(_) => "watchdog",
    };
    format!("{kind}:{}", activity.item_id())
}

fn turn_item_identity(item: &TurnItem) -> Option<(&str, String, u64)> {
    match item {
        TurnItem::ManagedAgentActivity(item) => Some((
            &item.event_id,
            format!("managed:{}", item.id),
            item.sequence,
        )),
        TurnItem::OutboundInterAgentMessage(item) => Some((
            &item.event_id,
            format!("outbound:{}", item.id),
            item.sequence,
        )),
        TurnItem::TaskAssignmentActivity(item) => Some((
            &item.event_id,
            format!("assignment:{}", item.id),
            item.sequence,
        )),
        TurnItem::TaskWatchdogActivity(item) => Some((
            &item.event_id,
            format!("watchdog:{}", item.id),
            item.sequence,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::items::AgentManagementPhase;
    use codex_protocol::items::ManagedAgentActivityItem;
    use codex_protocol::items::ManagedAgentActivityStatus;
    use codex_protocol::items::ManagedAgentOperation;
    use codex_protocol::items::TaskWatchdogActivityItem;
    use codex_protocol::items::TaskWatchdogActivityKind;
    use codex_protocol::items::TaskWatchdogStage;

    fn activity(event_id: &str, sequence: u64) -> CutexUiActivity {
        CutexUiActivity::ManagedAgentActivity(Box::new(ManagedAgentActivityItem {
            id: "action-1".to_string(),
            event_id: event_id.to_string(),
            sequence,
            occurred_at_ms: 1,
            project_id: None,
            operation: ManagedAgentOperation::Create,
            status: ManagedAgentActivityStatus::Completed,
            action_id: None,
            phase_event_id: None,
            phase: None,
            managed_agent_id: "cutex.worker".to_string(),
            managed_agent_name: Some("worker".to_string()),
            managed_agent_metadata: None,
            predecessor_agent_id: None,
            predecessor_agent_name: None,
            predecessor_metadata: None,
            successor_agent_id: None,
            successor_agent_name: None,
            successor_metadata: None,
            replace_policy: None,
            rotation_mode: None,
            authority_epoch: None,
            managed_agent_role: Some("worker".to_string()),
            initial_task_preview: None,
            detail: None,
            runtime_generation: None,
        }))
    }

    fn watchdog(episode: &str, event_id: &str, sequence: u64) -> CutexUiActivity {
        CutexUiActivity::TaskWatchdogActivity(TaskWatchdogActivityItem {
            id: episode.into(),
            event_id: event_id.into(),
            event_key: "task_watchdog.first_stale".into(),
            sequence,
            occurred_at_ms: 1,
            project_id: Some("project-1".into()),
            task_id: "task-1".into(),
            task_revision: 1,
            assignment_id: "assignment-1".into(),
            attempt_number: 1,
            director_agent_id: "cutex.director".into(),
            assignee_agent_id: "cutex.worker".into(),
            assignee_metadata: None,
            activity_watermark: "2026-08-28T01:00:00Z".into(),
            activity_kind: TaskWatchdogActivityKind::LastOutput,
            idle_duration_secs: 600,
            stage: TaskWatchdogStage::FirstStale,
            source_sequence: 4,
        })
    }

    #[test]
    fn rejects_duplicate_and_stale_events() {
        let mut tracker = CutexUiActivityTracker::from_rollout_items(&[]);
        assert_eq!(
            tracker.register(&activity("event-2", 2)),
            CutexUiActivityDisposition::Accepted
        );
        assert_eq!(
            tracker.register(&activity("event-2", 2)),
            CutexUiActivityDisposition::Duplicate
        );
        assert_eq!(
            tracker.register(&activity("event-1", 1)),
            CutexUiActivityDisposition::Stale
        );
        assert_eq!(
            tracker.register(&activity("event-3", 3)),
            CutexUiActivityDisposition::Accepted
        );
    }

    #[test]
    fn phase_facts_use_global_envelope_sequence_for_action_local_monotonicity() {
        let mut first = activity("envelope-44", 44);
        let CutexUiActivity::ManagedAgentActivity(first_item) = &mut first else {
            unreachable!()
        };
        first_item.status = ManagedAgentActivityStatus::InProgress;
        first_item.action_id = Some("action-1".into());
        first_item.phase_event_id = Some("agent-management:action-1:phase:3".into());
        first_item.phase = Some(AgentManagementPhase::Configured);

        let mut stale = first.clone();
        let CutexUiActivity::ManagedAgentActivity(stale_item) = &mut stale else {
            unreachable!()
        };
        stale_item.event_id = "envelope-43".into();
        stale_item.sequence = 43;
        stale_item.phase_event_id = Some("agent-management:action-1:phase:2".into());
        stale_item.phase = Some(AgentManagementPhase::Adopted);

        let mut tracker = CutexUiActivityTracker::from_rollout_items(&[]);
        assert_eq!(
            tracker.register(&first),
            CutexUiActivityDisposition::Accepted
        );
        assert_eq!(
            tracker.register(&first),
            CutexUiActivityDisposition::Duplicate
        );
        assert_eq!(tracker.register(&stale), CutexUiActivityDisposition::Stale);
    }

    #[test]
    fn watchdog_replay_coalesces_by_episode_and_new_episode_resets_sequence() {
        let mut tracker = CutexUiActivityTracker::from_rollout_items(&[]);
        assert_eq!(
            tracker.register(&watchdog("episode-1", "fact-2", 2)),
            CutexUiActivityDisposition::Accepted
        );
        assert_eq!(
            tracker.register(&watchdog("episode-1", "fact-2", 2)),
            CutexUiActivityDisposition::Duplicate
        );
        assert_eq!(
            tracker.register(&watchdog("episode-1", "fact-1", 1)),
            CutexUiActivityDisposition::Stale
        );
        assert_eq!(
            tracker.register(&watchdog("episode-2", "fact-3", 1)),
            CutexUiActivityDisposition::Accepted,
            "a recovered-and-restaled task has a fresh producer episode id"
        );
    }
}
