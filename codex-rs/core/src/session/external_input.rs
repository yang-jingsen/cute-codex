use super::session::Session;
use super::turn_context::TurnContext;
use crate::context::ExternalInputContext;
use crate::external_input::Pending;
use crate::external_input::Runtime;
use crate::external_input::recovery::restore;
use anyhow::Result;
use anyhow::ensure;
use codex_history::RolloutItem;
use codex_protocol::external_input::Commit;
use codex_protocol::external_input::Delivery;
use codex_protocol::external_input::Envelope;
use codex_protocol::external_input::HoldReason;
use codex_protocol::external_input::Processing;
use codex_protocol::external_input::Receipt;
use codex_protocol::external_input::Recovered;
use codex_protocol::external_input_record::Fact;
use codex_protocol::external_input_record::Record;
use codex_protocol::external_input_status::DeliveryState;
use codex_protocol::external_input_status::Status;
use codex_protocol::protocol::TruncationPolicy;
use codex_rollout::RolloutRecorder;
use std::collections::BTreeMap;
use std::sync::Arc;

impl Session {
    /// Called by the trusted launch owner before exposing the loaded thread.
    /// This never creates or resumes a thread or infers an owner from input text.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "publication must wait for complete recovery while admission remains excluded"
    )]
    pub(crate) async fn bind_external_input(&self, owner: &str) -> Result<()> {
        let mut state = self.external_input.lock().await;
        if let Some(runtime) = state.as_ref() {
            ensure!(
                runtime.owner == owner && !runtime.poisoned,
                "external input unavailable"
            );
            return Ok(());
        }
        let live = self
            .services
            .live_thread
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("external input requires persistent history"))?;
        let path = live
            .local_rollout_path()
            .await?
            .ok_or_else(|| anyhow::anyhow!("external input requires local history"))?;
        let (items, thread_id, parse_errors) = RolloutRecorder::load_rollout_items(&path).await?;
        ensure!(
            parse_errors == 0 && thread_id == Some(self.thread_id),
            "unreadable external input history"
        );
        let restored = restore(owner, &self.thread_id.to_string(), &items)?;
        let retries = items
            .iter()
            .filter_map(|item| match item {
                RolloutItem::ExternalInput(record) => match &record.fact {
                    Fact::Retry { retry_id, .. } => Some((retry_id.clone(), record.clone())),
                    _ => None,
                },
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        // Reconstruct complete canonical input before making historical receipts
        // queryable. Do not treat a compaction summary mentioning it as the item.
        let mut history = self.state.lock().await;
        for found in restored.recovery.messages.values() {
            if !matches!(found.processing, Processing::Pending(_)) {
                continue;
            }
            let item = ExternalInputContext::new(
                &found.commit.envelope,
                model_has_byte_bound(
                    history
                        .session_configuration
                        .step_settings
                        .collaboration_mode
                        .model(),
                ),
            )?
            .into_response_item();
            if !history.history.raw_items().any(|old| old == &item) {
                history.record_items(std::iter::once(&item), TruncationPolicy::Bytes(10_000));
            }
        }
        drop(history);
        *state = Some(Runtime {
            dispatch_revision: 1,
            attempted_revision: 0,
            retry_exclusions: BTreeMap::new(),
            changed: tokio::sync::broadcast::channel(100).0,
            owner: owner.to_owned(),
            recovery: restored.recovery,
            pending: Vec::new(),
            paused: restored.paused,
            poisoned: false,
            retries,
            permits: restored.permits,
        });
        Ok(())
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "status must observe one recovered state and its effective mode"
    )]
    pub(crate) async fn external_input_status(
        &self,
        owner: &str,
        id: &str,
        digest: &str,
    ) -> Result<Status> {
        let state = self.external_input.lock().await;
        let runtime = state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("external input disabled"))?;
        ensure!(
            runtime.owner == owner && !runtime.poisoned,
            "external input recovery required"
        );
        let mut status = snapshot(runtime, id, digest);
        if status.processing.state
            == codex_protocol::external_input_status::ProcessingState::Pending
            && self
                .state
                .lock()
                .await
                .session_configuration
                .step_settings
                .collaboration_mode
                .mode
                == codex_protocol::config_types::ModeKind::Plan
        {
            status.processing =
                (&Processing::Held(status.processing.attempt_id, HoldReason::PlanMode)).into();
        }
        Ok(status)
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "duplicate validation, model sizing and admission must remain atomic"
    )]
    pub(crate) async fn admit_external_input(&self, envelope: Envelope) -> Result<(Status, bool)> {
        envelope.validate()?;
        ensure!(
            envelope.thread_id == self.thread_id.to_string(),
            "external input thread mismatch"
        );
        let mut state = self.external_input.lock().await;
        let runtime = state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("external input disabled"))?;
        ensure!(
            runtime.owner == envelope.owner_id && !runtime.poisoned,
            "external input recovery required"
        );
        let found = snapshot(runtime, &envelope.message.id, &envelope.semantic_sha256);
        if found.delivery_state != DeliveryState::Unknown {
            return Ok((found, false));
        }
        let model = self
            .state
            .lock()
            .await
            .session_configuration
            .step_settings
            .collaboration_mode
            .model()
            .to_owned();
        ExternalInputContext::new(&envelope, model_has_byte_bound(&model))?;
        let obligations = runtime
            .recovery
            .messages
            .values()
            .filter(|found| {
                !matches!(
                    found.processing,
                    Processing::None | Processing::OutputObserved(_)
                )
            })
            .count();
        ensure!(
            runtime.pending.len() + obligations < 100,
            "external input overloaded"
        );
        let id = envelope.message.id.clone();
        let digest = envelope.semantic_sha256.clone();
        runtime.dispatch_revision = runtime
            .dispatch_revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("external input dispatch revision exhausted"))?;
        // Linearize admission with the active/reserved-turn snapshot. The
        // runtime lock keeps concurrent admissions in this same order.
        let active = self.active_turn.lock().await;
        let excluded_turn = active.as_ref().map(|turn| Arc::clone(&turn.turn_state));
        runtime.pending.push(Pending {
            envelope,
            excluded_turn,
        });
        drop(active);
        Ok((snapshot(runtime, &id, &digest), true))
    }

    /// Only a regular turn's safe input boundary calls this method. The durable
    /// pair precedes publication; ordinary best-effort record helpers are not used.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "pair flush and context publication must serialize against admission, status and interruption"
    )]
    pub(crate) async fn consume_external_input(&self, turn: &TurnContext) -> Result<()> {
        let current_turn = self
            .active_turn
            .lock()
            .await
            .as_ref()
            .map(|turn| Arc::clone(&turn.turn_state));
        let mut state = self.external_input.lock().await;
        let Some(runtime) = state.as_mut() else {
            return Ok(());
        };
        ensure!(
            !runtime.poisoned,
            "external input recovery required before sampling"
        );
        let live = self
            .services
            .live_thread
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("external input writer unavailable"))?;
        // A compaction summary is not the canonical input. Restore only released
        // pending obligations; held uncertain/no-output items require explicit retry.
        for (id, found) in &runtime.recovery.messages {
            if !matches!(found.processing, Processing::Pending(_))
                || (runtime.paused && !runtime.permits.contains(id))
                || runtime
                    .retry_exclusions
                    .get(id)
                    .zip(current_turn.as_ref())
                    .is_some_and(|(excluded, current)| Arc::ptr_eq(excluded, current))
            {
                continue;
            }
            let item = ExternalInputContext::new(
                &found.commit.envelope,
                model_has_byte_bound(&turn.model_info().slug),
            )?
            .into_response_item();
            let mut history = self.state.lock().await;
            if !history.history.raw_items().any(|old| old == &item) {
                history.record_items(std::iter::once(&item), TruncationPolicy::Bytes(10_000));
            }
        }
        let mut index = 0;
        while index < runtime.pending.len() {
            let pending = &runtime.pending[index];
            if pending.envelope.message.delivery == Delivery::AfterTurn
                && (runtime.paused
                    || pending
                        .excluded_turn
                        .as_ref()
                        .zip(current_turn.as_ref())
                        .is_some_and(|(excluded, current)| Arc::ptr_eq(excluded, current)))
            {
                index += 1;
                continue;
            }
            let envelope = pending.envelope.clone();
            let item = ExternalInputContext::new(
                &envelope,
                model_has_byte_bound(&turn.model_info().slug),
            )?
            .into_response_item();
            let next_ordinal = runtime
                .recovery
                .next_ordinal
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("external input receipt ordinal exhausted"))?;
            let receipt = Receipt::new(
                &envelope,
                turn.sub_id.clone(),
                runtime.recovery.next_ordinal,
            )?;
            let commit = Commit { envelope, receipt };
            let record = Record {
                version: 1,
                owner_id: runtime.owner.clone(),
                thread_id: self.thread_id.to_string(),
                fact: Fact::Commit {
                    commit: Box::new(commit.clone()),
                },
            };
            runtime.poisoned = true;
            #[cfg(all(debug_assertions, unix))]
            crate::external_input::probe::barrier(live, "before_pair", &commit.envelope.message.id)
                .await?;
            live.append_items(&[
                RolloutItem::ExternalInput(record),
                RolloutItem::ResponseItem(item.clone().into()),
            ])
            .await?;
            live.flush().await?;
            live.persist_for_read().await?;
            #[cfg(all(debug_assertions, unix))]
            crate::external_input::probe::barrier(live, "after_flush", &commit.envelope.message.id)
                .await?;
            self.state
                .lock()
                .await
                .record_items(std::iter::once(&item), TruncationPolicy::Bytes(10_000));
            runtime.recovery.next_ordinal = next_ordinal;
            let processing = if commit.envelope.message.delivery == Delivery::AfterTurn {
                Processing::Pending(None)
            } else {
                Processing::None
            };
            runtime.recovery.messages.insert(
                commit.envelope.message.id.clone(),
                Recovered { commit, processing },
            );
            let _ = runtime
                .changed
                .send(runtime.pending[index].envelope.message.id.clone());
            runtime.pending.remove(index);
            runtime.poisoned = false;
            self.send_raw_response_items(turn, std::slice::from_ref(&item))
                .await;
        }
        Ok(())
    }
}

fn snapshot(runtime: &Runtime, id: &str, digest: &str) -> Status {
    let mut status = Status {
        message_id: id.into(),
        semantic_sha256: digest.into(),
        delivery_state: DeliveryState::Unknown,
        receipt: None,
        processing: (&Processing::None).into(),
    };
    if let Some(found) = runtime.recovery.messages.get(id) {
        if found.commit.envelope.semantic_sha256 != digest {
            status.delivery_state = DeliveryState::Conflict;
            return status;
        }
        status.delivery_state = DeliveryState::ContextPersisted;
        status.receipt = Some(found.commit.receipt.clone());
        status.processing = if runtime.paused
            && !runtime.permits.contains(id)
            && let Processing::Pending(attempt) = found.processing
        {
            (&Processing::Held(attempt, HoldReason::Interrupted)).into()
        } else {
            (&found.processing).into()
        };
    } else if let Some(found) = runtime
        .pending
        .iter()
        .find(|found| found.envelope.message.id == id)
    {
        status.delivery_state = if found.envelope.semantic_sha256 == digest {
            DeliveryState::Pending
        } else {
            DeliveryState::Conflict
        };
        if found.envelope.message.delivery == Delivery::AfterTurn {
            status.processing = if runtime.paused {
                (&Processing::Held(None, HoldReason::Interrupted)).into()
            } else {
                (&Processing::Pending(None)).into()
            };
        }
    }
    status
}

// These model families use byte-fallback text tokenization. An unknown family
// is not assigned the upstream four-bytes/token estimate as a hard bound.
fn model_has_byte_bound(model: &str) -> bool {
    matches!(
        model,
        "gpt-6-astra"
            | "gpt-5.6-sol"
            | "gpt-5.6-terra"
            | "gpt-5.6-luna"
            | "gpt-5.5"
            | "gpt-5.4"
            | "gpt-5.4-mini"
            | "gpt-5.2"
    )
}
