use super::*;

fn task_service_author() -> AgentPath {
    AgentPath::root()
        .join("cutex_task_service")
        .expect("task service path")
}

#[test]
fn assignment_keeps_contract_once_and_omits_transport_metadata() {
    let raw = "Message Type: TASK_SERVICE_ASSIGNMENT\n\
Task name: worker-task\n\
Sender: cutex-task-service\n\
Coordinator: cutex.director\n\
Assignment ID: assignment-01\n\
Project ID: cutex-stack-main\n\
Task ID: task-01\n\
Task Revision: 7\n\
Contract SHA-256: deadbeef\n\
Send Attempt ID: send-01\n\
External Action ID: action-01\n\
External Message ID: message-01\n\
Summary:\nA redundant summary.\n\
Opaque Contract:\n# Exact contract\n\nDo this exactly.\n";

    let projected = project_task_service_model_content(&task_service_author(), raw)
        .expect("authenticated assignment projection");

    assert_eq!(
        projected,
        "Message Type: TASK_SERVICE_ASSIGNMENT\n\
Task name: worker-task\n\
Sender: cutex-task-service\n\
Assignment ID: assignment-01\n\
Opaque Contract:\n# Exact contract\n\nDo this exactly.\n"
    );
    assert_eq!(projected.matches("# Exact contract").count(), 1);
    let presentation =
        task_service_message_presentation(&task_service_author(), raw).expect("typed presentation");
    assert_eq!(presentation.class, TaskServiceMessageClass::Assignment);
    assert_eq!(presentation.assignment_id, "assignment-01");
    assert_eq!(presentation.project_id.as_deref(), Some("cutex-stack-main"));
    assert_eq!(
        presentation
            .semantic_payload
            .matches("# Exact contract")
            .count(),
        1
    );
    for omitted in [
        "Coordinator:",
        "Project ID:",
        "Task ID:",
        "Task Revision:",
        "Contract SHA-256:",
        "Send Attempt ID:",
        "External Action ID:",
        "External Message ID:",
        "redundant summary",
    ] {
        assert!(!projected.contains(omitted), "unexpected {omitted}");
        assert!(raw.contains(omitted), "raw envelope lost {omitted}");
    }
}

#[test]
fn assignment_payload_variant_keeps_exact_payload() {
    let raw = "Message Type: TASK_SERVICE_ASSIGNMENT\nTask name: worker-task\nSender: cutex-task-service\nAssignment ID: assignment-01\nExternal Message ID: message-01\nPayload:\nline one\nline two\n";
    let projected = project_task_service_model_content(&task_service_author(), raw)
        .expect("payload assignment projection");

    assert!(projected.ends_with("Payload:\nline one\nline two\n"));
    assert!(!projected.contains("message-01"));
}

#[test]
fn completion_transitions_are_concise_and_model_visible() {
    for transition in ["ReviewReady", "TerminalClosure", "UrgentActionRequired"] {
        let raw = format!(
            "Message Type: TASK_SERVICE_COMPLETION\nTask name: director-task\nSender: cutex-task-service\nNotification ID: notification-01\nAssignment ID: assignment-01\nProject ID: cutex-stack-main\nTask ID: task-01\nTask Revision: 2\nAttempt Number: 1\nTransition: {transition}\nTarget Seat: cutex-director\nExternal Action ID: action-01\nExternal Message ID: message-01\nPayload:\nTask Service transition {transition}; inspect the result.\n"
        );
        let projected = project_task_service_model_content(&task_service_author(), &raw)
            .expect("completion projection");

        assert!(projected.contains(&format!("Transition: {transition}")));
        assert!(projected.contains("Assignment ID: assignment-01"));
        assert!(projected.contains("inspect the result"));
        if transition != "UrgentActionRequired" {
            assert_eq!(
                task_service_message_presentation(&task_service_author(), &raw)
                    .expect("known completion presentation")
                    .project_id
                    .as_deref(),
                Some("cutex-stack-main")
            );
        }
        for omitted in [
            "Notification ID:",
            "Project ID:",
            "Task ID:",
            "Task Revision:",
            "Attempt Number:",
            "Target Seat:",
            "External Action ID:",
            "External Message ID:",
        ] {
            assert!(!projected.contains(omitted), "unexpected {omitted}");
            assert!(raw.contains(omitted), "raw envelope lost {omitted}");
        }
    }
}

#[test]
fn meaningful_completion_classes_are_typed_without_changing_projection() {
    for (transition, class) in [
        ("Progress", TaskServiceMessageClass::Progress),
        ("Blocked", TaskServiceMessageClass::Blocked),
        ("Resumed", TaskServiceMessageClass::Resumed),
        ("ReviewReady", TaskServiceMessageClass::ReviewReady),
        ("Retry", TaskServiceMessageClass::Retry),
        ("Completed", TaskServiceMessageClass::TerminalClosure),
    ] {
        let raw = format!(
            "Message Type: TASK_SERVICE_COMPLETION\nTask name: task\nSender: cutex-task-service\nAssignment ID: assignment-01\nTransition: {transition}\nPayload:\nsemantic result\n"
        );
        let before = project_task_service_model_content(&task_service_author(), &raw);
        let presentation = task_service_message_presentation(&task_service_author(), &raw)
            .expect("known semantic class");
        let after = project_task_service_model_content(&task_service_author(), &raw);
        assert_eq!(presentation.class, class);
        assert_eq!(presentation.project_id, None);
        assert_eq!(
            before, after,
            "presentation must not mutate model projection"
        );
    }
    let urgent = "Message Type: TASK_SERVICE_COMPLETION\nTask name: task\nSender: cutex-task-service\nAssignment ID: assignment-01\nTransition: UrgentActionRequired\nPayload:\ninspect\n";
    assert!(project_task_service_model_content(&task_service_author(), urgent).is_some());
    assert!(task_service_message_presentation(&task_service_author(), urgent).is_none());
}

#[test]
fn projection_is_limited_to_authenticated_well_formed_task_service_messages() {
    let raw = "Message Type: TASK_SERVICE_ASSIGNMENT\nTask name: worker-task\nSender: cutex-task-service\nAssignment ID: assignment-01\nOpaque Contract:\ncontract\n";
    let ordinary_author = AgentPath::root().join("director").expect("director path");
    assert_eq!(
        project_task_service_model_content(&ordinary_author, raw),
        None
    );
    assert_eq!(
        project_task_service_model_content(
            &task_service_author(),
            &raw.replace("Sender: cutex-task-service", "Sender: attacker")
        ),
        None
    );
    assert_eq!(
        project_task_service_model_content(
            &task_service_author(),
            &raw.replace("Assignment ID: assignment-01\n", "")
        ),
        None
    );
}
