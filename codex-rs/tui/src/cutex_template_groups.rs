//! Bounded, live-process-only correlation for grouped Cutex presentation templates.

use std::collections::HashMap;

use codex_app_server_protocol::AgentManagementPhase;
use codex_app_server_protocol::ManagedAgentActivityItem;
use codex_app_server_protocol::ManagedAgentOperation;
use codex_app_server_protocol::TaskServiceMessageClass;
use codex_app_server_protocol::TaskServiceMessagePresentation;
use codex_app_server_protocol::TaskWatchdogActivityItem;
use codex_app_server_protocol::TaskWatchdogStage;
use codex_config::types::TuiAgentManagementPhasePresentation;
use codex_config::types::TuiCutexActivitySettings;
use codex_config::types::TuiCutexGroupedTemplates;
use codex_config::types::TuiTaskServiceMessagePresentation;
use codex_config::types::TuiTaskServiceMessageSettings;
use codex_config::types::TuiTaskWatchdogStagePresentation;
use sha2::Digest;
use sha2::Sha256;

const MAX_CORRELATIONS: usize = 256;

/// A renderer instruction. `SafeDefault` is deliberately distinct from no live decision: replay
/// may retain the legacy per-stage selector, while a broken live correlation must never jump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TemplateDecision {
    Selected(String),
    SafeDefault,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CorrelationKey {
    Management {
        project_id: String,
        action_id: String,
        operation: ManagedAgentOperation,
    },
    TaskService {
        assignment_id: String,
        family: TaskPresentationFamily,
    },
    Watchdog {
        episode_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TaskPresentationFamily {
    TaskServiceSystem,
}

#[derive(Clone, Debug)]
enum SelectedPool {
    Ungrouped,
    Grouped(String),
}

#[derive(Clone, Debug)]
struct CorrelationEntry {
    pool: SelectedPool,
    initial_fingerprint: [u8; 32],
    last_used: u64,
}

#[derive(Debug, Default)]
pub(crate) struct CutexTemplateCorrelations {
    entries: HashMap<CorrelationKey, CorrelationEntry>,
    clock: u64,
}

#[derive(Clone, Copy)]
struct StageTemplates<'a> {
    template: Option<&'a str>,
    templates: Option<&'a [String]>,
    groups: Option<&'a [TuiCutexGroupedTemplates]>,
    selection_enabled: bool,
}

impl CutexTemplateCorrelations {
    pub(crate) fn management_decision(
        &mut self,
        activity: &ManagedAgentActivityItem,
        presentation: &TuiCutexActivitySettings,
    ) -> Option<TemplateDecision> {
        let phase = activity.phase?;
        let project_id = nonempty(activity.project_id.as_deref())?;
        let action_id = nonempty(activity.action_id.as_deref())?;
        let key = CorrelationKey::Management {
            project_id: project_id.to_string(),
            action_id: action_id.to_string(),
            operation: activity.operation,
        };
        let settings = crate::cutex_management_phases::phase_settings(activity, presentation);
        let stage = StageTemplates::management(settings);
        Some(self.resolve(
            key,
            matches!(phase, AgentManagementPhase::Prepared),
            matches!(
                phase,
                AgentManagementPhase::Complete
                    | AgentManagementPhase::NoWrite
                    | AgentManagementPhase::OwnerActionRequired
                    | AgentManagementPhase::Failure
            ),
            crate::cutex_management_phases::phase_name(phase),
            activity.phase_event_id.as_deref().unwrap_or_default(),
            stage,
        ))
    }

    pub(crate) fn task_service_decision(
        &mut self,
        presentation: &TaskServiceMessagePresentation,
        settings: &TuiTaskServiceMessageSettings,
    ) -> Option<TemplateDecision> {
        let assignment_id = nonempty(Some(&presentation.assignment_id))?;
        let key = CorrelationKey::TaskService {
            assignment_id: assignment_id.to_string(),
            family: TaskPresentationFamily::TaskServiceSystem,
        };
        let stage = StageTemplates::task_service(crate::task_service_messages::class_settings(
            presentation.class,
            settings,
        ));
        Some(self.resolve(
            key,
            matches!(presentation.class, TaskServiceMessageClass::Assignment),
            matches!(presentation.class, TaskServiceMessageClass::TerminalClosure),
            crate::task_service_messages::class_name(presentation.class),
            presentation.transition.as_deref().unwrap_or_default(),
            stage,
        ))
    }

    pub(crate) fn watchdog_decision(
        &mut self,
        activity: &TaskWatchdogActivityItem,
        settings: &TuiCutexActivitySettings,
    ) -> Option<TemplateDecision> {
        let episode_id = nonempty(Some(&activity.id))?;
        let key = CorrelationKey::Watchdog {
            episode_id: episode_id.to_string(),
        };
        let stage_settings = match activity.stage {
            TaskWatchdogStage::FirstStale => &settings.task_watchdog.first_stale,
            TaskWatchdogStage::DirectorEscalated => &settings.task_watchdog.director_escalated,
        };
        Some(self.resolve(
            key,
            matches!(activity.stage, TaskWatchdogStage::FirstStale),
            false,
            activity.stage.event_key(),
            &activity.event_id,
            StageTemplates::watchdog(stage_settings),
        ))
    }

    fn resolve(
        &mut self,
        key: CorrelationKey,
        initial: bool,
        terminal: bool,
        stage_name: &str,
        semantic_event: &str,
        stage: StageTemplates<'_>,
    ) -> TemplateDecision {
        self.clock = self.clock.wrapping_add(1);
        let fingerprint = stage.fingerprint();
        let decision = if initial {
            match self.entries.get_mut(&key) {
                Some(entry) if Some(entry.initial_fingerprint) == fingerprint => {
                    entry.last_used = self.clock;
                    select_from_pool(&key, stage_name, semantic_event, stage, &entry.pool)
                }
                Some(_) => TemplateDecision::SafeDefault,
                None => match fingerprint {
                    Some(initial_fingerprint) => {
                        let pool = select_initial_pool(&key, stage);
                        match pool {
                            Some(pool) => {
                                let decision = select_from_pool(
                                    &key,
                                    stage_name,
                                    semantic_event,
                                    stage,
                                    &pool,
                                );
                                self.insert(
                                    key.clone(),
                                    CorrelationEntry {
                                        pool,
                                        initial_fingerprint,
                                        last_used: self.clock,
                                    },
                                );
                                decision
                            }
                            None => TemplateDecision::SafeDefault,
                        }
                    }
                    None => TemplateDecision::SafeDefault,
                },
            }
        } else {
            match self.entries.get_mut(&key) {
                Some(entry) => {
                    entry.last_used = self.clock;
                    select_from_pool(&key, stage_name, semantic_event, stage, &entry.pool)
                }
                None => TemplateDecision::SafeDefault,
            }
        };
        if terminal {
            self.entries.remove(&key);
        }
        decision
    }

    fn insert(&mut self, key: CorrelationKey, entry: CorrelationEntry) {
        if self.entries.len() >= MAX_CORRELATIONS
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, entry);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

impl<'a> StageTemplates<'a> {
    fn management(settings: Option<&'a TuiAgentManagementPhasePresentation>) -> Self {
        Self {
            template: settings.and_then(|settings| settings.template.as_deref()),
            templates: settings.and_then(|settings| settings.templates.as_deref()),
            groups: settings.and_then(|settings| settings.grouped_templates.as_deref()),
            selection_enabled: settings.and_then(|settings| settings.selection).is_some(),
        }
    }

    fn task_service(settings: &'a TuiTaskServiceMessagePresentation) -> Self {
        Self {
            template: settings.template.as_deref(),
            templates: settings.templates.as_deref(),
            groups: settings.grouped_templates.as_deref(),
            selection_enabled: settings.selection.is_some(),
        }
    }

    fn watchdog(settings: &'a TuiTaskWatchdogStagePresentation) -> Self {
        Self {
            template: settings.template.as_deref(),
            templates: settings.templates.as_deref(),
            groups: settings.grouped_templates.as_deref(),
            selection_enabled: settings.selection.is_some(),
        }
    }

    fn fingerprint(self) -> Option<[u8; 32]> {
        let mut hash = Sha256::new();
        match (self.template, self.templates, self.groups) {
            (Some(template), None, None) => {
                hash.update(b"singular");
                frame(&mut hash, template);
            }
            (None, Some(templates), None)
                if self.selection_enabled && valid_candidates(templates) =>
            {
                hash.update(b"ungrouped");
                frame_candidates(&mut hash, templates);
            }
            (None, None, Some(groups)) if self.selection_enabled && valid_groups(groups) => {
                hash.update(b"grouped");
                for group in groups {
                    frame(&mut hash, &group.group);
                    frame_candidates(&mut hash, &group.templates);
                }
            }
            _ => return None,
        }
        Some(hash.finalize().into())
    }
}

fn select_initial_pool(key: &CorrelationKey, stage: StageTemplates<'_>) -> Option<SelectedPool> {
    if stage.template.is_some() || stage.templates.is_some() {
        return Some(SelectedPool::Ungrouped);
    }
    let groups = stage.groups?;
    let index = stable_index(key, "initial-group", groups.len(), |hash| {
        for group in groups {
            frame(hash, &group.group);
            frame_candidates(hash, &group.templates);
        }
    });
    groups
        .get(index)
        .map(|group| SelectedPool::Grouped(group.group.clone()))
}

fn select_from_pool(
    key: &CorrelationKey,
    stage_name: &str,
    semantic_event: &str,
    stage: StageTemplates<'_>,
    pool: &SelectedPool,
) -> TemplateDecision {
    let candidates: &[String] = match pool {
        SelectedPool::Ungrouped => {
            if let Some(template) = stage.template {
                return TemplateDecision::Selected(template.to_string());
            }
            let Some(candidates) = stage.templates.filter(|values| valid_candidates(values)) else {
                return TemplateDecision::SafeDefault;
            };
            candidates
        }
        SelectedPool::Grouped(group_id) => {
            let Some(group) = stage
                .groups
                .and_then(|groups| groups.iter().find(|group| group.group == *group_id))
            else {
                return TemplateDecision::SafeDefault;
            };
            if !valid_candidates(&group.templates) {
                return TemplateDecision::SafeDefault;
            }
            &group.templates
        }
    };
    let index = stable_index(key, stage_name, candidates.len(), |hash| {
        frame(hash, semantic_event);
        frame_candidates(hash, candidates)
    });
    candidates
        .get(index)
        .cloned()
        .map(TemplateDecision::Selected)
        .unwrap_or(TemplateDecision::SafeDefault)
}

fn stable_index(
    key: &CorrelationKey,
    stage: &str,
    len: usize,
    add_candidates: impl FnOnce(&mut Sha256),
) -> usize {
    let mut hash = Sha256::new();
    match key {
        CorrelationKey::Management {
            project_id,
            action_id,
            operation,
        } => {
            frame(&mut hash, "management");
            frame(&mut hash, project_id);
            frame(&mut hash, action_id);
            frame(
                &mut hash,
                crate::cutex_management_phases::operation_name(*operation),
            );
        }
        CorrelationKey::TaskService {
            assignment_id,
            family: TaskPresentationFamily::TaskServiceSystem,
        } => {
            frame(&mut hash, "task_service_system");
            frame(&mut hash, assignment_id);
        }
        CorrelationKey::Watchdog { episode_id } => {
            frame(&mut hash, "task_watchdog");
            frame(&mut hash, episode_id);
        }
    }
    frame(&mut hash, stage);
    add_candidates(&mut hash);
    let digest = hash.finalize();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(prefix) as usize % len
}

fn valid_candidates(candidates: &[String]) -> bool {
    !candidates.is_empty() && candidates.len() <= 8
}

fn valid_groups(groups: &[TuiCutexGroupedTemplates]) -> bool {
    !groups.is_empty()
        && groups.len() <= 8
        && groups
            .iter()
            .all(|group| !group.group.is_empty() && valid_candidates(&group.templates))
}

fn frame_candidates(hash: &mut Sha256, candidates: &[String]) {
    for candidate in candidates {
        frame(hash, candidate);
    }
}

fn frame(hash: &mut Sha256, value: &str) {
    hash.update(value.len().to_be_bytes());
    hash.update(value.as_bytes());
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "cutex_template_groups_tests.rs"]
mod tests;
