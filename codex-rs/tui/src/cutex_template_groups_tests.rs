use super::*;
use codex_app_server_protocol::ManagedAgentActivityStatus;
use codex_app_server_protocol::ManagedAgentOperation;
use codex_app_server_protocol::TaskWatchdogActivityItem;
use codex_app_server_protocol::TaskWatchdogActivityKind;
use codex_app_server_protocol::TaskWatchdogStage;
use codex_config::types::TuiCutexTemplateSelection;

fn groups(stage: &str) -> Vec<TuiCutexGroupedTemplates> {
    vec![
        TuiCutexGroupedTemplates {
            group: "alpha".into(),
            templates: vec![format!("alpha {stage}")],
        },
        TuiCutexGroupedTemplates {
            group: "beta".into(),
            templates: vec![format!("beta {stage}")],
        },
    ]
}

fn phase(activity_phase: AgentManagementPhase, action_id: &str) -> ManagedAgentActivityItem {
    ManagedAgentActivityItem {
        id: action_id.into(),
        event_id: format!("event-{action_id}-{activity_phase:?}"),
        sequence: 1,
        occurred_at_ms: 1,
        project_id: Some("project-1".into()),
        operation: ManagedAgentOperation::Create,
        status: ManagedAgentActivityStatus::InProgress,
        action_id: Some(action_id.into()),
        phase_event_id: Some(format!("phase-{activity_phase:?}")),
        phase: Some(activity_phase),
        managed_agent_id: "cutex.worker".into(),
        managed_agent_name: Some("Worker".into()),
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
        managed_agent_role: None,
        initial_task_preview: None,
        detail: None,
        runtime_generation: None,
    }
}

fn grouped_management() -> TuiCutexActivitySettings {
    let mut settings = TuiCutexActivitySettings::default();
    settings.create.prepared = Some(TuiAgentManagementPhasePresentation {
        grouped_templates: Some(groups("prepared")),
        selection: Some(TuiCutexTemplateSelection::StableRandom),
        ..Default::default()
    });
    settings.create.ready = Some(TuiAgentManagementPhasePresentation {
        grouped_templates: Some(groups("ready")),
        selection: Some(TuiCutexTemplateSelection::StableRandom),
        ..Default::default()
    });
    settings
}

fn task(class: TaskServiceMessageClass, assignment_id: &str) -> TaskServiceMessagePresentation {
    TaskServiceMessagePresentation {
        class,
        project_id: None,
        task_name: "delivery".into(),
        assignment_id: assignment_id.into(),
        transition: None,
        semantic_payload: "semantic payload".into(),
    }
}

fn watchdog(stage: TaskWatchdogStage) -> TaskWatchdogActivityItem {
    TaskWatchdogActivityItem {
        id: "episode-1".into(),
        event_id: format!("fact-{stage:?}"),
        event_key: stage.event_key().into(),
        sequence: 1,
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
        stage,
        source_sequence: 1,
    }
}

fn selected(decision: Option<TemplateDecision>) -> String {
    let Some(TemplateDecision::Selected(value)) = decision else {
        panic!("expected selected template");
    };
    value
}

#[test]
fn management_initial_selection_stays_in_the_exact_group() {
    let settings = grouped_management();
    let mut cache = CutexTemplateCorrelations::default();
    let initial = selected(cache.management_decision(
        &phase(AgentManagementPhase::Prepared, "action-1"),
        &settings,
    ));
    let continuation = selected(
        cache.management_decision(&phase(AgentManagementPhase::Ready, "action-1"), &settings),
    );
    assert_eq!(
        initial.split_once(' ').unwrap().0,
        continuation.split_once(' ').unwrap().0
    );
    assert_eq!(
        continuation,
        selected(
            cache.management_decision(&phase(AgentManagementPhase::Ready, "action-1"), &settings,)
        )
    );
}

#[test]
fn missing_selected_group_and_conflicting_initial_never_jump() {
    let settings = grouped_management();
    let mut cache = CutexTemplateCorrelations::default();
    let initial = selected(cache.management_decision(
        &phase(AgentManagementPhase::Prepared, "action-1"),
        &settings,
    ));
    let selected_group = initial.split_once(' ').unwrap().0;
    let mut changed = settings.clone();
    changed.create.ready.as_mut().unwrap().grouped_templates =
        Some(vec![TuiCutexGroupedTemplates {
            group: if selected_group == "alpha" {
                "beta"
            } else {
                "alpha"
            }
            .into(),
            templates: vec!["wrong group".into()],
        }]);
    assert_eq!(
        cache.management_decision(&phase(AgentManagementPhase::Ready, "action-1"), &changed),
        Some(TemplateDecision::SafeDefault)
    );
    changed.create.prepared.as_mut().unwrap().grouped_templates = Some(groups("changed"));
    assert_eq!(
        cache.management_decision(&phase(AgentManagementPhase::Prepared, "action-1"), &changed),
        Some(TemplateDecision::SafeDefault)
    );
}

#[test]
fn ungrouped_pool_continues_and_restart_does_not_reconstruct() {
    let mut settings = TuiCutexActivitySettings::default();
    for stage in [&mut settings.create.prepared, &mut settings.create.ready] {
        *stage = Some(TuiAgentManagementPhasePresentation {
            templates: Some(vec!["one {phase}".into(), "two {phase}".into()]),
            selection: Some(TuiCutexTemplateSelection::StableRandom),
            ..Default::default()
        });
    }
    let mut cache = CutexTemplateCorrelations::default();
    assert!(matches!(
        cache.management_decision(
            &phase(AgentManagementPhase::Prepared, "action-1"),
            &settings
        ),
        Some(TemplateDecision::Selected(_))
    ));
    assert!(matches!(
        cache.management_decision(&phase(AgentManagementPhase::Ready, "action-1"), &settings),
        Some(TemplateDecision::Selected(_))
    ));
    assert_eq!(
        CutexTemplateCorrelations::default()
            .management_decision(&phase(AgentManagementPhase::Ready, "action-1"), &settings),
        Some(TemplateDecision::SafeDefault)
    );
}

#[test]
fn task_service_uses_assignment_family_and_terminal_evicts() {
    let mut settings = TuiTaskServiceMessageSettings::default();
    settings.assignment.grouped_templates = Some(groups("assigned"));
    settings.assignment.selection = Some(TuiCutexTemplateSelection::StableRandom);
    settings.progress.grouped_templates = Some(groups("progress"));
    settings.progress.selection = Some(TuiCutexTemplateSelection::StableRandom);
    settings.terminal_closure.grouped_templates = Some(groups("closed"));
    settings.terminal_closure.selection = Some(TuiCutexTemplateSelection::StableRandom);
    let mut cache = CutexTemplateCorrelations::default();
    let initial = selected(cache.task_service_decision(
        &task(TaskServiceMessageClass::Assignment, "assignment-1"),
        &settings,
    ));
    let progress = selected(cache.task_service_decision(
        &task(TaskServiceMessageClass::Progress, "assignment-1"),
        &settings,
    ));
    assert_eq!(
        initial.split_once(' ').unwrap().0,
        progress.split_once(' ').unwrap().0
    );
    cache.management_decision(
        &phase(AgentManagementPhase::Prepared, "assignment-1"),
        &grouped_management(),
    );
    assert_eq!(
        cache.len(),
        2,
        "Task and Management identities are isolated"
    );
    assert!(matches!(
        cache.task_service_decision(
            &task(TaskServiceMessageClass::TerminalClosure, "assignment-1"),
            &settings
        ),
        Some(TemplateDecision::Selected(_))
    ));
    assert_eq!(cache.len(), 1);
}

#[test]
fn identities_are_typed_and_cache_is_lru_bounded() {
    let settings = grouped_management();
    let mut missing_project = phase(AgentManagementPhase::Prepared, "missing-project");
    missing_project.project_id = None;
    let mut cache = CutexTemplateCorrelations::default();
    assert_eq!(cache.management_decision(&missing_project, &settings), None);
    for index in 0..=MAX_CORRELATIONS {
        cache.management_decision(
            &phase(AgentManagementPhase::Prepared, &format!("action-{index}")),
            &settings,
        );
    }
    assert_eq!(cache.len(), MAX_CORRELATIONS);
}

#[test]
fn watchdog_random_group_is_stable_across_both_stages_and_restart_is_safe() {
    let mut settings = TuiCutexActivitySettings::default();
    settings.task_watchdog.first_stale.grouped_templates = Some(groups("first"));
    settings.task_watchdog.first_stale.selection = Some(TuiCutexTemplateSelection::StableRandom);
    settings.task_watchdog.director_escalated.grouped_templates = Some(groups("escalated"));
    settings.task_watchdog.director_escalated.selection =
        Some(TuiCutexTemplateSelection::StableRandom);
    let mut cache = CutexTemplateCorrelations::default();
    let first =
        selected(cache.watchdog_decision(&watchdog(TaskWatchdogStage::FirstStale), &settings));
    let escalated = selected(
        cache.watchdog_decision(&watchdog(TaskWatchdogStage::DirectorEscalated), &settings),
    );
    assert_eq!(
        first.split_once(' ').unwrap().0,
        escalated.split_once(' ').unwrap().0
    );
    assert_eq!(
        CutexTemplateCorrelations::default()
            .watchdog_decision(&watchdog(TaskWatchdogStage::DirectorEscalated), &settings,),
        Some(TemplateDecision::SafeDefault),
        "reattach does not invent the forgotten first-stage group"
    );
}
