use super::*;
use crate::history_cell::HistoryCell;
use codex_app_server_protocol::AgentManagementRotationMode;
use codex_app_server_protocol::CutexParticipantPresentation;
use codex_app_server_protocol::ManagedAgentActivityStatus;
use codex_app_server_protocol::ThreadItem;
use codex_config::types::TuiCutexRichSpan;
use codex_config::types::TuiCutexTemplateSelection;

fn phase_activity(phase: AgentManagementPhase) -> ManagedAgentActivityItem {
    let status = match phase {
        AgentManagementPhase::Complete => ManagedAgentActivityStatus::Completed,
        AgentManagementPhase::NoWrite
        | AgentManagementPhase::OwnerActionRequired
        | AgentManagementPhase::Failure => ManagedAgentActivityStatus::Failed,
        _ => ManagedAgentActivityStatus::InProgress,
    };
    ManagedAgentActivityItem {
        id: "rotate-action-1".into(),
        event_id: format!("envelope-{phase:?}"),
        sequence: 44,
        occurred_at_ms: 1_725_000_123_456,
        project_id: Some("project-1".into()),
        operation: ManagedAgentOperation::DirectorRotate,
        status,
        action_id: Some("rotate-action-1".into()),
        phase_event_id: Some("agent-management:rotate-action-1:phase:3".into()),
        phase: Some(phase),
        managed_agent_id: "cutex.successor".into(),
        managed_agent_name: None,
        managed_agent_metadata: Some(CutexParticipantPresentation {
            display_name: Some("New Director".into()),
            cutex_session_id: Some("cutex.successor".into()),
            ..Default::default()
        }),
        predecessor_agent_id: Some("cutex.predecessor".into()),
        predecessor_agent_name: Some("Old Director".into()),
        predecessor_metadata: None,
        successor_agent_id: Some("cutex.successor".into()),
        successor_agent_name: Some("New Director".into()),
        successor_metadata: None,
        replace_policy: None,
        rotation_mode: Some(AgentManagementRotationMode::ClosePredecessorThenCreateWithMessage),
        authority_epoch: Some(7),
        managed_agent_role: Some("Director".into()),
        initial_task_preview: None,
        detail: None,
        runtime_generation: None,
    }
}

fn rendered(
    activity: &ManagedAgentActivityItem,
    presentation: &TuiCutexActivitySettings,
) -> String {
    summary_lines(activity, presentation, /*width*/ 80)
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn stable_random_phase_variants_do_not_reroll_and_distribute_event_identities() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        templates: Some(vec![
            "恭喜 {agent_name} 已经可以撑地了".to_string(),
            "{agent_name} 堂堂登场！".to_string(),
            "新的 Director {agent_name} 已抵达战场".to_string(),
        ]),
        selection: Some(TuiCutexTemplateSelection::StableRandom),
        ..Default::default()
    });
    let activity = phase_activity(AgentManagementPhase::SuccessorReady);
    let first = rendered(&activity, &presentation);
    assert_eq!(first, rendered(&activity, &presentation));
    let choices = (0..96)
        .map(|index| {
            let mut activity = activity.clone();
            activity.event_id = format!("event-{index}");
            activity.phase_event_id = Some(format!("phase-event-{index}"));
            rendered(&activity, &presentation)
        })
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(choices.len(), 3);
}

#[test]
fn grouped_live_decision_composes_with_phase_style() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        grouped_templates: Some(vec![]),
        selection: Some(TuiCutexTemplateSelection::StableRandom),
        foreground: Some(TuiCutexForegroundColor::Green),
        bold: Some(true),
        indent: Some(2),
        ..Default::default()
    });
    let lines = summary_lines_with_decision(
        &phase_activity(AgentManagementPhase::SuccessorReady),
        &presentation,
        80,
        Some(&TemplateDecision::Selected(
            "New Director delivered with alpha".into(),
        )),
    );
    insta::assert_snapshot!(
        lines.into_iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n"),
        @"•   New Director delivered with alpha"
    );
}

#[test]
fn rich_phase_template_preserves_order_and_per_span_inheritance() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        foreground: Some(TuiCutexForegroundColor::Blue),
        bold: Some(true),
        rich_template: Some(vec![
            TuiCutexRichSpan {
                text: "management: ".into(),
                foreground: None,
                bold: None,
                dim: None,
                italic: None,
            },
            TuiCutexRichSpan {
                text: "{agent_name}\n{phase}".into(),
                foreground: Some(TuiCutexForegroundColor::Rgb(0xD3, 0xD3, 0xD3)),
                bold: Some(false),
                dim: Some(true),
                italic: Some(true),
            },
        ]),
        ..Default::default()
    });
    let lines = summary_lines(
        &phase_activity(AgentManagementPhase::SuccessorReady),
        &presentation,
        80,
    );
    assert_eq!(lines[0].to_string(), "• management: New Director");
    assert_eq!(lines[1].to_string(), "  successor_ready");
    assert_eq!(lines[0].spans[1].style.fg, Some(Color::Blue));
    assert!(
        lines[0].spans[1]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(
        lines[0].spans[3].style.fg,
        Some(crate::terminal_palette::rgb_color((0xD3, 0xD3, 0xD3)))
    );
    assert!(
        !lines[0].spans[3]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert!(lines[0].spans[3].style.add_modifier.contains(Modifier::DIM));
    assert!(
        lines[0].spans[3]
            .style
            .add_modifier
            .contains(Modifier::ITALIC)
    );
}

#[test]
fn every_frozen_phase_has_a_safe_default_label() {
    for phase in [
        AgentManagementPhase::Prepared,
        AgentManagementPhase::PrivateCwdReady,
        AgentManagementPhase::NativeBootstrapPending,
        AgentManagementPhase::NativeSessionCaptured,
        AgentManagementPhase::Adopted,
        AgentManagementPhase::Configured,
        AgentManagementPhase::Online,
        AgentManagementPhase::Ready,
        AgentManagementPhase::MessagePending,
        AgentManagementPhase::MessageQueued,
        AgentManagementPhase::PredecessorClosing,
        AgentManagementPhase::PredecessorClosed,
        AgentManagementPhase::AuthorityTransferPending,
        AgentManagementPhase::AuthorityTransferred,
        AgentManagementPhase::SuccessorReady,
        AgentManagementPhase::Complete,
        AgentManagementPhase::NoWrite,
        AgentManagementPhase::OwnerActionRequired,
        AgentManagementPhase::Failure,
    ] {
        let output = rendered(&phase_activity(phase), &TuiCutexActivitySettings::default());
        assert!(!output.trim().is_empty(), "missing default for {phase:?}");
        assert!(output.starts_with("• "));
    }
}

#[test]
fn configured_rotate_examples_render_only_for_their_authoritative_phases() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.predecessor_closing = Some(TuiAgentManagementPhasePresentation {
        template: Some("{agent_name}：自刎归天！".into()),
        ..Default::default()
    });
    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        template: Some("恭喜 {agent_name} 已经可以撑地了".into()),
        ..Default::default()
    });

    let mut closing = phase_activity(AgentManagementPhase::PredecessorClosing);
    closing.managed_agent_id = "cutex.predecessor".into();
    closing.managed_agent_metadata = Some(CutexParticipantPresentation {
        display_name: Some("Old Director".into()),
        ..Default::default()
    });
    let closing_output = rendered(&closing, &presentation);
    let ready_output = rendered(
        &phase_activity(AgentManagementPhase::SuccessorReady),
        &presentation,
    );
    assert!(!closing_output.contains("可以撑地"));
    assert!(!ready_output.contains("自刎归天"));
    insta::assert_snapshot!(format!("{closing_output}\n{ready_output}"), @r###"
    • Old Director：自刎归天！
    • 恭喜 New Director 已经可以撑地了
    "###);
}

#[test]
fn retain_rotation_successor_fact_never_uses_the_closing_template() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.predecessor_closing = Some(TuiAgentManagementPhasePresentation {
        template: Some("{agent_name}：自刎归天！".into()),
        ..Default::default()
    });
    let mut retained = phase_activity(AgentManagementPhase::SuccessorReady);
    retained.rotation_mode = Some(AgentManagementRotationMode::RetainPredecessorWithMessage);
    let output = rendered(&retained, &presentation);
    assert!(output.contains("Successor ready New Director"));
    assert!(!output.contains("自刎归天"));
    assert!(!output.contains("Closing predecessor"));
}

#[test]
fn placeholders_use_authoritative_names_then_durable_ids() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.authority_transferred =
        Some(TuiAgentManagementPhasePresentation {
            template: Some(
                "{predecessor_name} ({predecessor_id}) → {successor_name} ({successor_id}) · epoch {authority_epoch} · {phase}".into(),
            ),
            ..Default::default()
        });
    let mut activity = phase_activity(AgentManagementPhase::AuthorityTransferred);
    activity.predecessor_agent_name = None;
    activity.successor_agent_name = None;
    let output = rendered(&activity, &presentation);
    assert!(output.contains("cutex.predecessor (cutex.predecessor)"));
    assert!(output.contains("cutex.successor (cutex.successor)"));
    assert!(output.contains("epoch 7 · authority_transferred"));
}

#[test]
fn malformed_missing_or_unsafe_templates_fall_back() {
    for template in [
        "{unknown}",
        "{authority_epoch",
        "bad\u{0007}control",
        &"x".repeat(MAX_TEMPLATE_CHARS + 1),
    ] {
        let mut presentation = TuiCutexActivitySettings::default();
        presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
            template: Some(template.into()),
            ..Default::default()
        });
        assert!(
            rendered(
                &phase_activity(AgentManagementPhase::SuccessorReady),
                &presentation,
            )
            .contains("Successor ready New Director")
        );
    }

    let mut missing = phase_activity(AgentManagementPhase::SuccessorReady);
    missing.authority_epoch = None;
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        template: Some("epoch {authority_epoch}".into()),
        ..Default::default()
    });
    assert!(rendered(&missing, &presentation).contains("Successor ready New Director"));
}

#[test]
fn phase_style_visibility_indentation_and_wrapping_are_bounded() {
    let mut presentation = TuiCutexActivitySettings::default();
    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        show: Some(false),
        ..Default::default()
    });
    let activity = phase_activity(AgentManagementPhase::SuccessorReady);
    assert!(!is_shown(&activity, &presentation));
    assert!(summary_lines(&activity, &presentation, /*width*/ 40).is_empty());

    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        show: Some(true),
        template: Some(
            "{successor_name} is ready with a deliberately long bounded status label".into(),
        ),
        foreground: Some(TuiCutexForegroundColor::Magenta),
        bold: Some(true),
        italic: Some(true),
        indent: Some(2),
        wrap_width: Some(18),
        ..Default::default()
    });
    let lines = summary_lines(&activity, &presentation, /*width*/ 80);
    assert!(lines.len() <= MAX_RENDERED_LINES);
    assert!(lines.iter().all(|line| line.width() <= 18));
    let styled = &lines[0].spans[1];
    assert_eq!(styled.style.fg, Some(Color::Magenta));
    assert!(styled.style.add_modifier.contains(Modifier::BOLD));
    assert!(styled.style.add_modifier.contains(Modifier::ITALIC));
    assert!(styled.content.starts_with("  "));

    presentation.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
        template: Some("{successor_name}".into()),
        indent: Some(MAX_INDENT + 1),
        wrap_width: Some(MIN_WRAP_WIDTH - 1),
        ..Default::default()
    });
    let mut long_activity = activity;
    long_activity.successor_metadata = None;
    long_activity.successor_agent_name = Some(format!("{}\nunsafe", "界".repeat(300)));
    let lines = summary_lines(&long_activity, &presentation, u16::MAX);
    assert!(lines.len() <= MAX_RENDERED_LINES);
    assert!(
        lines
            .iter()
            .all(|line| line.width() <= usize::from(MAX_WRAP_WIDTH))
    );
    assert!(lines.iter().all(|line| !line.to_string().contains('\n')));
    assert!(!lines[0].spans[1].content.starts_with(' '));
}

#[test]
fn phase_display_keys_coalesce_or_append_with_per_operation_override() {
    let first = ThreadItem::ManagedAgentActivity {
        activity: phase_activity(AgentManagementPhase::Prepared),
    };
    let mut second_activity = phase_activity(AgentManagementPhase::Ready);
    second_activity.phase_event_id = Some("agent-management:rotate-action-1:phase:4".into());
    second_activity.event_id = "envelope-ready".into();
    second_activity.sequence = 45;
    let second = ThreadItem::ManagedAgentActivity {
        activity: second_activity,
    };

    let mut presentation = TuiCutexActivitySettings::default();
    assert_eq!(
        crate::cutex_activities::activity_key(&first, &presentation),
        crate::cutex_activities::activity_key(&second, &presentation)
    );
    assert!(!crate::cutex_activities::is_terminal(&first, &presentation));

    presentation.phase_display = TuiAgentManagementPhaseDisplay::Append;
    assert_ne!(
        crate::cutex_activities::activity_key(&first, &presentation),
        crate::cutex_activities::activity_key(&second, &presentation)
    );
    assert!(crate::cutex_activities::is_terminal(&first, &presentation));

    presentation.director_rotate.phase_display = Some(TuiAgentManagementPhaseDisplay::Coalesce);
    assert_eq!(
        crate::cutex_activities::activity_key(&first, &presentation),
        crate::cutex_activities::activity_key(&second, &presentation)
    );
}

#[test]
fn coalesced_live_cell_advances_without_mutating_the_canonical_fact() {
    let first = ThreadItem::ManagedAgentActivity {
        activity: phase_activity(AgentManagementPhase::Prepared),
    };
    let unchanged = first.clone();
    let (cell, handle) = crate::cutex_activities::new_activity_cell(
        first,
        None,
        TuiCutexActivitySettings::default(),
    );
    assert!(
        cell.display_lines(/*width*/ 80)[0]
            .to_string()
            .contains("Prepared")
    );

    let mut ready = phase_activity(AgentManagementPhase::Ready);
    ready.event_id = "envelope-ready".into();
    ready.sequence = 45;
    ready.phase_event_id = Some("agent-management:rotate-action-1:phase:4".into());
    handle.update(ThreadItem::ManagedAgentActivity { activity: ready }, None);
    assert!(
        cell.display_lines(/*width*/ 80)[0]
            .to_string()
            .contains("Managed agent ready")
    );
    assert!(matches!(
        unchanged,
        ThreadItem::ManagedAgentActivity {
            activity: ManagedAgentActivityItem {
                phase: Some(AgentManagementPhase::Prepared),
                ..
            }
        }
    ));
}
