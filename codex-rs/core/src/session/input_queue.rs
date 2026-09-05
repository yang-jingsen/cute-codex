use crate::state::ActiveTurn;
use crate::state::MailboxDeliveryPhase;
use crate::state::TurnState;
use codex_diagnostics::Gauge;
use codex_diagnostics::GaugeGuard;
use codex_history::ResponseItemEnvelope;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::InterAgentCommunication;
use codex_protocol::protocol::InterAgentDeliveryMode;
use codex_protocol::turn_input::TurnStartOptions;
use codex_protocol::user_input::UserInput;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::watch;

static PENDING_MAILBOX_MESSAGES: Gauge = Gauge::new("core.mailbox.pending");

/// Input consumed by a regular turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TurnInput {
    UserInput {
        content: Vec<UserInput>,
        client_id: Option<String>,
    },
    FunctionCallOutput(ResponseItem),
    // Preserve the existing serialized format while carrying injection API metadata
    // through the in-memory queue.
    ResponseItem(#[serde(with = "turn_input_response_item")] ResponseItemEnvelope),
    InterAgentCommunication(InterAgentCommunication),
}

mod turn_input_response_item {
    use super::ResponseItem;
    use super::ResponseItemEnvelope;
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use serde::Serializer;
    use serde::ser::Error as _;

    pub(super) fn serialize<S>(
        item: &ResponseItemEnvelope,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if item.metadata.is_some() {
            return Err(S::Error::custom(
                "annotated response items cannot cross the turn-input serialization boundary",
            ));
        }
        item.item.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<ResponseItemEnvelope, D::Error>
    where
        D: Deserializer<'de>,
    {
        ResponseItem::deserialize(deserializer).map(ResponseItemEnvelope::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputQueueActivity {
    Mailbox,
    Steer,
}

/// Turn-local pending input storage owned by the input queue flow.
#[derive(Default)]
pub(crate) struct TurnInputQueue {
    items: Vec<TurnInput>,
}

/// Session-scoped pending input storage and active-turn mailbox delivery coordination.
pub(crate) struct InputQueue {
    activity_tx: watch::Sender<InputQueueActivity>,
    mailbox: Mutex<MailboxQueue>,
}

struct PendingMailboxCommunication {
    communication: InterAgentCommunication,
    start_options: TurnStartOptions,
    _diagnostics_guard: GaugeGuard,
}

#[derive(Default)]
struct MailboxQueue {
    pending_mails: VecDeque<PendingMailboxCommunication>,
    recent_message_ids: VecDeque<codex_protocol::ResponseItemId>,
    recent_message_id_set: HashSet<codex_protocol::ResponseItemId>,
}

const RECENT_MAILBOX_MESSAGE_ID_CAPACITY: usize = 4096;

impl InputQueue {
    pub(crate) fn new() -> Self {
        let (activity_tx, _) = watch::channel(InputQueueActivity::Mailbox);
        Self {
            activity_tx,
            mailbox: Mutex::new(MailboxQueue::default()),
        }
    }

    pub(crate) async fn subscribe_activity(
        &self,
        turn_state: Option<&Mutex<TurnState>>,
    ) -> (
        watch::Receiver<InputQueueActivity>,
        Option<InputQueueActivity>,
    ) {
        let activity_rx = self.activity_tx.subscribe();
        let has_pending_steer = if let Some(turn_state) = turn_state {
            turn_state.lock().await.pending_input.has_pending_input()
        } else {
            false
        };
        let pending_activity = if has_pending_steer {
            Some(InputQueueActivity::Steer)
        } else if self.has_pending_mailbox_items().await {
            Some(InputQueueActivity::Mailbox)
        } else {
            None
        };
        (activity_rx, pending_activity)
    }

    pub(crate) async fn enqueue_mailbox_communication(
        &self,
        communication: InterAgentCommunication,
        start_options: TurnStartOptions,
    ) -> bool {
        let mut mailbox = self.mailbox.lock().await;
        if let Some(message_id) = communication
            .id
            .as_ref()
            .filter(|message_id| !message_id.as_str().is_empty())
        {
            if !mailbox.recent_message_id_set.insert(message_id.clone()) {
                return false;
            }
            mailbox.recent_message_ids.push_back(message_id.clone());
            if mailbox.recent_message_ids.len() > RECENT_MAILBOX_MESSAGE_ID_CAPACITY
                && let Some(expired) = mailbox.recent_message_ids.pop_front()
            {
                mailbox.recent_message_id_set.remove(&expired);
            }
        }
        mailbox
            .pending_mails
            .push_back(PendingMailboxCommunication {
                communication,
                start_options,
                _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
            });
        drop(mailbox);
        self.activity_tx.send_replace(InputQueueActivity::Mailbox);
        true
    }

    pub(crate) async fn has_pending_mailbox_items(&self) -> bool {
        !self.mailbox.lock().await.pending_mails.is_empty()
    }

    pub(crate) async fn has_trigger_turn_mailbox_items(&self) -> bool {
        self.mailbox
            .lock()
            .await
            .pending_mails
            .iter()
            .any(|mail| mail.communication.resolved_delivery_mode().trigger_turn())
    }

    pub(crate) async fn drain_mailbox_input_items(
        &self,
        include_after_turn: bool,
    ) -> (Vec<TurnInput>, TurnStartOptions) {
        let mut mailbox = self.mailbox.lock().await;
        let mut drained = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(mail) = mailbox.pending_mails.pop_front() {
            if include_after_turn
                || mail.communication.resolved_delivery_mode() != InterAgentDeliveryMode::AfterTurn
            {
                drained.push(mail);
            } else {
                retained.push_back(mail);
            }
        }
        mailbox.pending_mails = retained;
        drop(mailbox);

        // A later follow-up supersedes the earlier choice, including an omitted choice.
        let mut start_options = drained
            .iter()
            .rev()
            .find(|mail| mail.communication.trigger_turn)
            .map(|mail| mail.start_options.clone())
            .unwrap_or_default();
        start_options.parent_turn_id = drained
            .iter()
            .filter(|mail| mail.communication.trigger_turn)
            .map(|mail| mail.start_options.parent_turn_id.as_deref())
            .reduce(|expected, candidate| expected.filter(|id| candidate == Some(*id)))
            .and_then(|id| id.filter(|id| !id.trim().is_empty()).map(str::to_string));
        start_options.root_turn_id = drained
            .iter()
            .find(|mail| mail.communication.trigger_turn)
            .and_then(|mail| {
                mail.start_options
                    .parent_turn_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .and(mail.start_options.root_turn_id.as_deref())
                    .filter(|id| !id.trim().is_empty())
            })
            .map(str::to_string);
        let items = drained
            .into_iter()
            .map(|mail| TurnInput::InterAgentCommunication(mail.communication))
            .collect();
        (items, start_options)
    }

    pub(crate) async fn turn_state_for_sub_id(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) -> Option<Arc<Mutex<TurnState>>> {
        let active = active_turn.lock().await;
        active.as_ref().and_then(|active_turn| {
            active_turn
                .task
                .as_ref()
                .is_some_and(|task| task.turn_context.sub_id == sub_id)
                .then(|| Arc::clone(&active_turn.turn_state))
        })
    }

    pub(crate) async fn accepts_mailbox_delivery_for_current_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) -> bool {
        let Some(turn_state) = self.turn_state_for_sub_id(active_turn, sub_id).await else {
            return false;
        };
        turn_state
            .lock()
            .await
            .accepts_mailbox_delivery_for_current_turn()
    }

    /// Clear any pending waiters and input buffered for the current turn.
    pub(crate) async fn clear_pending(&self, active_turn: &ActiveTurn) {
        let mut turn_state = active_turn.turn_state.lock().await;
        turn_state.clear_pending_waiters();
        turn_state.pending_input.items.clear();
    }

    pub(crate) async fn defer_mailbox_delivery_to_next_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await;
        let Some(turn_state) = turn_state else {
            return;
        };
        let mut turn_state = turn_state.lock().await;
        // Explicit same-turn work still needs a follow-up. Queue-only child mail does not: keep
        // it pending so task completion records it for the next turn without sampling again.
        if turn_state.pending_input.items.iter().any(|input| {
            !matches!(
                input,
                TurnInput::InterAgentCommunication(communication) if !communication.trigger_turn
            )
        }) {
            return;
        }
        turn_state.set_mailbox_delivery_phase(MailboxDeliveryPhase::NextTurn);
    }

    pub(crate) async fn accept_mailbox_delivery_for_current_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await;
        let Some(turn_state) = turn_state else {
            return;
        };
        self.accept_mailbox_delivery_for_turn_state(turn_state.as_ref())
            .await;
    }

    pub(super) async fn accept_mailbox_delivery_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
    ) {
        turn_state
            .lock()
            .await
            .accept_mailbox_delivery_for_current_turn();
    }

    pub(super) async fn extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
        input: Vec<TurnInput>,
    ) {
        {
            let mut turn_state = turn_state.lock().await;
            turn_state.pending_input.items.extend(input);
            turn_state.accept_mailbox_delivery_for_current_turn();
        }
        self.activity_tx.send_replace(InputQueueActivity::Steer);
    }

    pub(crate) async fn extend_pending_input_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
        input: Vec<TurnInput>,
    ) {
        turn_state.lock().await.pending_input.items.extend(input);
    }

    pub(crate) async fn take_pending_input_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
    ) -> Vec<TurnInput> {
        turn_state.lock().await.pending_input.items.split_off(0)
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state updates must remain atomic"
    )]
    pub(crate) async fn get_pending_input(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
    ) -> (Vec<TurnInput>, TurnStartOptions) {
        let (pending_input, accepts_mailbox_delivery, include_after_turn, active_turn_metadata) = {
            let mut active = active_turn.lock().await;
            match active.as_mut() {
                Some(active_turn) => {
                    let active_turn_metadata = active_turn
                        .task
                        .as_ref()
                        .map(|task| Arc::clone(&task.turn_context.turn_metadata_state));
                    let mut turn_state = active_turn.turn_state.lock().await;
                    let accepts_mailbox_delivery =
                        turn_state.accepts_mailbox_delivery_for_current_turn();
                    let include_after_turn = active_turn.task.is_none();
                    let pending_input = if accepts_mailbox_delivery {
                        turn_state.pending_input.items.split_off(0)
                    } else {
                        Vec::new()
                    };
                    (
                        pending_input,
                        accepts_mailbox_delivery,
                        include_after_turn,
                        active_turn_metadata,
                    )
                }
                None => (Vec::new(), true, true, None),
            }
        };
        if !accepts_mailbox_delivery {
            return (pending_input, TurnStartOptions::default());
        }
        let (mailbox_items, start_options) =
            self.drain_mailbox_input_items(include_after_turn).await;
        if let Some(active_turn_metadata) = active_turn_metadata
            && active_turn_metadata.root_turn_id().is_none()
            && let Some(root_turn_id) = start_options.root_turn_id.as_ref()
        {
            active_turn_metadata.set_root_turn_id(root_turn_id.clone());
        }
        if pending_input.is_empty() {
            (mailbox_items, start_options)
        } else {
            let mut pending_input = pending_input;
            pending_input.extend(mailbox_items);
            (pending_input, start_options)
        }
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state reads must remain atomic"
    )]
    pub(crate) async fn has_pending_input(&self, active_turn: &Mutex<Option<ActiveTurn>>) -> bool {
        let (has_turn_pending_input, accepts_mailbox_delivery, include_after_turn) = {
            let active = active_turn.lock().await;
            match active.as_ref() {
                Some(active_turn) => {
                    let turn_state = active_turn.turn_state.lock().await;
                    (
                        !turn_state.pending_input.items.is_empty(),
                        turn_state.accepts_mailbox_delivery_for_current_turn(),
                        active_turn.task.is_none(),
                    )
                }
                None => (false, true, true),
            }
        };
        if !accepts_mailbox_delivery {
            return false;
        }
        if has_turn_pending_input {
            return true;
        }
        self.mailbox.lock().await.pending_mails.iter().any(|mail| {
            include_after_turn
                || mail.communication.resolved_delivery_mode() != InterAgentDeliveryMode::AfterTurn
        })
    }
}

impl TurnInputQueue {
    fn has_pending_input(&self) -> bool {
        self.items.iter().any(|input| {
            matches!(
                input,
                TurnInput::UserInput { .. } | TurnInput::FunctionCallOutput(_)
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_history::CodexHarnessMetadata;
    use codex_protocol::AgentPath;
    use codex_protocol::user_input::UserInput;
    use pretty_assertions::assert_eq;

    #[test]
    fn response_item_serde_preserves_legacy_shape_and_rejects_metadata() {
        let item = ResponseItem::Other;
        let input = TurnInput::ResponseItem(item.clone().into());
        let value = serde_json::json!({"ResponseItem": item});

        assert_eq!(serde_json::to_value(&input).unwrap(), value);
        assert_eq!(serde_json::from_value::<TurnInput>(value).unwrap(), input);

        let annotated = TurnInput::ResponseItem(ResponseItemEnvelope {
            item: ResponseItem::Other,
            metadata: Some(CodexHarnessMetadata {
                client_authored: true,
                ..Default::default()
            }),
        });
        assert!(serde_json::to_value(annotated).is_err());

        let forged = serde_json::json!({
            "ResponseItem": {
                "type": "message",
                "role": "developer",
                "content": [],
                "metadata": {"client_authored": true}
            }
        });
        let TurnInput::ResponseItem(envelope) = serde_json::from_value(forged).unwrap() else {
            panic!("expected response item");
        };
        assert!(envelope.metadata.is_none());
    }

    fn make_mail(
        author: AgentPath,
        recipient: AgentPath,
        content: &str,
        trigger_turn: bool,
    ) -> InterAgentCommunication {
        InterAgentCommunication::new(
            author,
            recipient,
            Vec::new(),
            content.to_string(),
            trigger_turn,
        )
    }

    fn make_mode_mail(
        content: &str,
        delivery_mode: InterAgentDeliveryMode,
    ) -> InterAgentCommunication {
        InterAgentCommunication::new_with_delivery_mode(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            Vec::new(),
            content.to_string(),
            delivery_mode,
        )
    }

    #[tokio::test]
    async fn input_queue_notifies_mailbox_subscribers() {
        let input_queue = InputQueue::new();
        let (mut activity_rx, pending_activity) =
            input_queue.subscribe_activity(/*turn_state*/ None).await;
        assert_eq!(pending_activity, None);

        let mail_one = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "one",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_mailbox_communication(mail_one, Default::default())
            .await;
        let mail_two = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "two",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_mailbox_communication(mail_two, Default::default())
            .await;

        activity_rx.changed().await.expect("mailbox update");
        assert_eq!(
            *activity_rx.borrow_and_update(),
            InputQueueActivity::Mailbox
        );
    }

    #[tokio::test]
    async fn input_queue_notifies_steer_subscribers() {
        let input_queue = InputQueue::new();
        let turn_state = Mutex::new(TurnState::default());
        let (mut activity_rx, pending_activity) =
            input_queue.subscribe_activity(Some(&turn_state)).await;
        assert_eq!(pending_activity, None);

        input_queue
            .extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
                &turn_state,
                vec![TurnInput::UserInput {
                    content: vec![UserInput::Text {
                        text: "steer".to_string(),
                        text_elements: Vec::new(),
                    }],
                    client_id: None,
                }],
            )
            .await;

        activity_rx.changed().await.expect("steer update");
        assert_eq!(*activity_rx.borrow_and_update(), InputQueueActivity::Steer);
    }

    #[tokio::test]
    async fn input_queue_reports_already_pending_steer() {
        let input_queue = InputQueue::new();
        let turn_state = Mutex::new(TurnState::default());
        let passive_output = serde_json::from_value(serde_json::json!({
            "ResponseItem": {"type": "function_call_output", "name": "notify", "output": "passive"}
        }))
        .unwrap();
        input_queue
            .extend_pending_input_for_turn_state(&turn_state, vec![passive_output])
            .await;
        assert_eq!(
            input_queue.subscribe_activity(Some(&turn_state)).await.1,
            None
        );
        input_queue
            .extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
                &turn_state,
                vec![TurnInput::UserInput {
                    content: vec![UserInput::Text {
                        text: "already pending".to_string(),
                        text_elements: Vec::new(),
                    }],
                    client_id: None,
                }],
            )
            .await;

        let (_activity_rx, pending_activity) =
            input_queue.subscribe_activity(Some(&turn_state)).await;

        assert_eq!(pending_activity, Some(InputQueueActivity::Steer));
    }

    #[tokio::test]
    async fn input_queue_drains_mailbox_in_delivery_order() {
        let input_queue = InputQueue::new();
        let mail_one = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "one",
            /*trigger_turn*/ false,
        );
        let mail_two = make_mail(
            AgentPath::try_from("/root/worker").expect("agent path"),
            AgentPath::root(),
            "two",
            /*trigger_turn*/ true,
        );

        input_queue
            .enqueue_mailbox_communication(mail_one.clone(), Default::default())
            .await;
        input_queue
            .enqueue_mailbox_communication(mail_two.clone(), Default::default())
            .await;

        assert_eq!(
            input_queue
                .drain_mailbox_input_items(/*include_after_turn*/ true)
                .await
                .0,
            vec![
                TurnInput::InterAgentCommunication(mail_one),
                TurnInput::InterAgentCommunication(mail_two)
            ]
        );
        assert!(!input_queue.has_pending_mailbox_items().await);
    }

    #[tokio::test]
    async fn input_queue_uses_unambiguous_trigger_parent_and_first_root() {
        let (parent, peer, root, root2) = (Some("a"), Some("b"), Some("r"), Some("s"));
        for (pending_mails, expected_parent_turn_id, expected_root_turn_id) in [
            (Vec::new(), None, None),
            (vec![(false, Some("q"), root)], None, None),
            (vec![(true, Some(""), root)], None, None),
            (vec![(true, Some("   "), root)], None, None),
            (vec![(true, None, root)], None, None),
            (vec![(true, parent, None)], parent, None),
            (vec![(true, parent, Some(""))], parent, None),
            (vec![(true, parent, root), (true, peer, root)], None, root),
            (vec![(true, parent, root), (true, peer, root2)], None, root),
            (vec![(true, parent, root), (true, None, root)], None, root),
            (
                vec![(true, parent, root), (true, parent, root)],
                parent,
                root,
            ),
            (
                vec![(false, Some("q"), root2), (true, parent, root)],
                parent,
                root,
            ),
        ] {
            let input_queue = InputQueue::new();
            for (trigger_turn, parent_turn_id, root_turn_id) in pending_mails {
                input_queue
                    .enqueue_mailbox_communication(
                        make_mail(AgentPath::root(), AgentPath::root(), "task", trigger_turn),
                        TurnStartOptions {
                            parent_turn_id: parent_turn_id.map(str::to_string),
                            root_turn_id: root_turn_id.map(str::to_string),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            let (_, start_options) = input_queue
                .drain_mailbox_input_items(/*include_after_turn*/ true)
                .await;
            assert_eq!(
                start_options.parent_turn_id.as_deref(),
                expected_parent_turn_id
            );
            assert_eq!(start_options.root_turn_id.as_deref(), expected_root_turn_id);
        }
    }

    #[tokio::test]
    async fn input_queue_uses_latest_followup_choice_and_ignores_queue_only_mail() {
        use codex_protocol::turn_input::CyberAccessProgram;

        for latest in [Some(CyberAccessProgram::Standard), None] {
            let input_queue = InputQueue::new();
            for (trigger_turn, program) in [
                (true, Some(CyberAccessProgram::DaybreakBlue)),
                (true, latest),
                (false, Some(CyberAccessProgram::DaybreakRed)),
            ] {
                input_queue
                    .enqueue_mailbox_communication(
                        make_mail(AgentPath::root(), AgentPath::root(), "task", trigger_turn),
                        TurnStartOptions {
                            cyber_access_program: program,
                            ..Default::default()
                        },
                    )
                    .await;
            }
            let (_, start_options) = input_queue
                .drain_mailbox_input_items(/*include_after_turn*/ true)
                .await;
            assert_eq!(start_options.cyber_access_program, latest);
        }
    }

    #[tokio::test]
    async fn input_queue_accepts_identified_mail_once_and_bounds_recent_ids() {
        let input_queue = InputQueue::new();
        let first_id = codex_protocol::ResponseItemId::with_suffix("amsg", 0);
        let mut first_mail = make_mode_mail("first", InterAgentDeliveryMode::Soon);
        first_mail.id = Some(first_id.clone());

        assert!(
            input_queue
                .enqueue_mailbox_communication(first_mail.clone(), TurnStartOptions::default(),)
                .await
        );
        assert!(
            !input_queue
                .enqueue_mailbox_communication(first_mail.clone(), TurnStartOptions::default(),)
                .await
        );

        for index in 1..=RECENT_MAILBOX_MESSAGE_ID_CAPACITY {
            let mut mail = make_mode_mail("filler", InterAgentDeliveryMode::Passive);
            mail.id = Some(codex_protocol::ResponseItemId::with_suffix("amsg", index));
            assert!(
                input_queue
                    .enqueue_mailbox_communication(mail, TurnStartOptions::default())
                    .await
            );
        }

        {
            let mailbox = input_queue.mailbox.lock().await;
            assert_eq!(
                mailbox.recent_message_ids.len(),
                RECENT_MAILBOX_MESSAGE_ID_CAPACITY
            );
            assert_eq!(
                mailbox.recent_message_id_set.len(),
                RECENT_MAILBOX_MESSAGE_ID_CAPACITY
            );
            assert!(!mailbox.recent_message_id_set.contains(&first_id));
        }
        assert!(
            input_queue
                .enqueue_mailbox_communication(first_mail, TurnStartOptions::default())
                .await
        );

        let unidentified = make_mode_mail("unidentified", InterAgentDeliveryMode::Passive);
        assert!(
            input_queue
                .enqueue_mailbox_communication(unidentified.clone(), TurnStartOptions::default(),)
                .await
        );
        assert!(
            input_queue
                .enqueue_mailbox_communication(unidentified, TurnStartOptions::default(),)
                .await
        );
    }

    #[tokio::test]
    async fn input_queue_active_drain_retains_after_turn_and_preserves_fifo() {
        let input_queue = InputQueue::new();
        let after_one = make_mode_mail("after one", InterAgentDeliveryMode::AfterTurn);
        let soon = make_mode_mail("soon", InterAgentDeliveryMode::Soon);
        let passive = make_mode_mail("passive", InterAgentDeliveryMode::Passive);
        let after_two = make_mode_mail("after two", InterAgentDeliveryMode::AfterTurn);
        let interrupt = make_mode_mail("interrupt", InterAgentDeliveryMode::Interrupt);

        for mail in [
            after_one.clone(),
            soon.clone(),
            passive.clone(),
            after_two.clone(),
            interrupt.clone(),
        ] {
            assert!(
                input_queue
                    .enqueue_mailbox_communication(mail, TurnStartOptions::default())
                    .await
            );
        }

        assert_eq!(
            input_queue
                .drain_mailbox_input_items(/*include_after_turn*/ false)
                .await
                .0,
            vec![
                TurnInput::InterAgentCommunication(soon),
                TurnInput::InterAgentCommunication(passive),
                TurnInput::InterAgentCommunication(interrupt),
            ]
        );
        assert_eq!(
            input_queue
                .drain_mailbox_input_items(/*include_after_turn*/ true)
                .await
                .0,
            vec![
                TurnInput::InterAgentCommunication(after_one),
                TurnInput::InterAgentCommunication(after_two),
            ]
        );
        assert!(!input_queue.has_pending_mailbox_items().await);
    }

    #[tokio::test]
    async fn input_queue_idle_reservation_drains_after_turn_mail() {
        let input_queue = InputQueue::new();
        let after_turn_mail = make_mode_mail("after turn", InterAgentDeliveryMode::AfterTurn);
        assert!(
            input_queue
                .enqueue_mailbox_communication(
                    after_turn_mail.clone(),
                    TurnStartOptions::default(),
                )
                .await
        );
        let reserved_idle_turn = Mutex::new(Some(ActiveTurn::default()));

        assert!(input_queue.has_pending_input(&reserved_idle_turn).await);
        assert_eq!(
            input_queue.get_pending_input(&reserved_idle_turn).await.0,
            vec![TurnInput::InterAgentCommunication(after_turn_mail)]
        );
        assert!(!input_queue.has_pending_mailbox_items().await);
    }

    #[tokio::test]
    async fn input_queue_trigger_modes_wake_except_passive() {
        for (mode, expected_trigger) in [
            (InterAgentDeliveryMode::AfterTurn, true),
            (InterAgentDeliveryMode::Soon, true),
            (InterAgentDeliveryMode::Passive, false),
            (InterAgentDeliveryMode::Interrupt, true),
        ] {
            let input_queue = InputQueue::new();
            assert!(
                input_queue
                    .enqueue_mailbox_communication(
                        make_mode_mail("mail", mode),
                        TurnStartOptions::default(),
                    )
                    .await
            );
            assert_eq!(
                input_queue.has_trigger_turn_mailbox_items().await,
                expected_trigger,
                "unexpected trigger behavior for {mode:?}"
            );
        }
    }

    #[tokio::test]
    async fn input_queue_tracks_pending_trigger_turn_mail() {
        let input_queue = InputQueue::new();

        let queued_mail = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "queued",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_mailbox_communication(queued_mail, Default::default())
            .await;
        assert!(!input_queue.has_trigger_turn_mailbox_items().await);

        let trigger_mail = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "wake",
            /*trigger_turn*/ true,
        );
        input_queue
            .enqueue_mailbox_communication(trigger_mail, Default::default())
            .await;
        assert!(input_queue.has_trigger_turn_mailbox_items().await);
    }
}
