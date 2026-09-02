use codex_protocol::AgentPath;
use codex_protocol::items::TaskServiceMessageClass;
use codex_protocol::items::TaskServiceMessagePresentation;

const TASK_SERVICE_AUTHOR: &str = "/root/cutex_task_service";
const TASK_SERVICE_SENDER: &str = "cutex-task-service";

pub(crate) fn project_task_service_model_content(
    author: &AgentPath,
    content: &str,
) -> Option<String> {
    if author.as_str() != TASK_SERVICE_AUTHOR {
        return None;
    }

    match TaskServiceModelProjection::parse(content)? {
        TaskServiceModelProjection::Assignment {
            task_name,
            assignment_id,
            section_name,
            section_body,
            ..
        } => Some(format!(
            "Message Type: TASK_SERVICE_ASSIGNMENT\nTask name: {task_name}\nSender: {TASK_SERVICE_SENDER}\nAssignment ID: {assignment_id}\n{section_name}:\n{section_body}"
        )),
        TaskServiceModelProjection::Completion {
            task_name,
            assignment_id,
            transition,
            payload,
            ..
        } => Some(format!(
            "Message Type: TASK_SERVICE_COMPLETION\nTask name: {task_name}\nSender: {TASK_SERVICE_SENDER}\nAssignment ID: {assignment_id}\nTransition: {transition}\nPayload:\n{payload}"
        )),
    }
}

pub(crate) fn task_service_message_presentation(
    author: &AgentPath,
    content: &str,
) -> Option<TaskServiceMessagePresentation> {
    if author.as_str() != TASK_SERVICE_AUTHOR {
        return None;
    }
    let projection = TaskServiceModelProjection::parse(content)?;
    let (class, task_name, assignment_id, project_id, transition, semantic_payload) =
        match projection {
            TaskServiceModelProjection::Assignment {
                task_name,
                assignment_id,
                project_id,
                section_body,
                ..
            } => (
                TaskServiceMessageClass::Assignment,
                task_name,
                assignment_id,
                project_id,
                None,
                section_body,
            ),
            TaskServiceModelProjection::Completion {
                task_name,
                assignment_id,
                project_id,
                transition,
                payload,
            } => (
                completion_class(transition)?,
                task_name,
                assignment_id,
                project_id,
                Some(transition),
                payload,
            ),
        };
    if !bounded_semantic(task_name, 256)
        || !bounded_semantic(assignment_id, 256)
        || project_id.is_some_and(|value| !bounded_semantic(value, 256))
        || transition.is_some_and(|value| !bounded_semantic(value, 128))
    {
        return None;
    }
    Some(TaskServiceMessagePresentation {
        class,
        task_name: task_name.to_string(),
        assignment_id: assignment_id.to_string(),
        project_id: project_id.map(str::to_string),
        transition: transition.map(str::to_string),
        semantic_payload: semantic_payload
            .chars()
            .take(2_048)
            .map(|value| {
                if value.is_control() && value != '\n' {
                    '�'
                } else {
                    value
                }
            })
            .collect(),
    })
}

fn completion_class(transition: &str) -> Option<TaskServiceMessageClass> {
    Some(match transition {
        "Progress" | "ProgressUpdate" | "AttemptProgressed" => TaskServiceMessageClass::Progress,
        "Blocked" | "AttemptBlocked" => TaskServiceMessageClass::Blocked,
        "Resumed" | "AttemptResumed" => TaskServiceMessageClass::Resumed,
        "ReviewReady" => TaskServiceMessageClass::ReviewReady,
        "Retry" | "Retrying" | "RetryScheduled" | "ChangesRequested" => {
            TaskServiceMessageClass::Retry
        }
        "TerminalClosure" | "Completed" | "Accepted" | "Failed" | "Cancelled" | "Closed"
        | "Declined" | "Aborted" => TaskServiceMessageClass::TerminalClosure,
        _ => return None,
    })
}

fn bounded_semantic(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

enum TaskServiceModelProjection<'a> {
    Assignment {
        task_name: &'a str,
        assignment_id: &'a str,
        project_id: Option<&'a str>,
        section_name: &'static str,
        section_body: &'a str,
    },
    Completion {
        task_name: &'a str,
        assignment_id: &'a str,
        project_id: Option<&'a str>,
        transition: &'a str,
        payload: &'a str,
    },
}

impl<'a> TaskServiceModelProjection<'a> {
    fn parse(content: &'a str) -> Option<Self> {
        let message_type = header_value(content, "Message Type")?;
        let task_name = header_value(content, "Task name")?;
        let sender = header_value(content, "Sender")?;
        let assignment_id = header_value(content, "Assignment ID")?;
        let project_id = header_value(content, "Project ID");
        if sender != TASK_SERVICE_SENDER {
            return None;
        }

        match message_type {
            "TASK_SERVICE_ASSIGNMENT" => {
                let (section_name, section_body) = section(content, "Opaque Contract")
                    .map(|body| ("Opaque Contract", body))
                    .or_else(|| section(content, "Payload").map(|body| ("Payload", body)))?;
                Some(Self::Assignment {
                    task_name,
                    assignment_id,
                    project_id,
                    section_name,
                    section_body,
                })
            }
            "TASK_SERVICE_COMPLETION" => Some(Self::Completion {
                task_name,
                assignment_id,
                project_id,
                transition: header_value(content, "Transition")?,
                payload: section(content, "Payload")?,
            }),
            _ => None,
        }
    }
}

fn header_value<'a>(content: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}: ");
    content
        .lines()
        .take_while(|line| !matches!(*line, "Summary:" | "Opaque Contract:" | "Payload:"))
        .find_map(|line| line.strip_prefix(&prefix))
        .filter(|value| !value.is_empty())
}

fn section<'a>(content: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("\n{name}:\n");
    content
        .split_once(&marker)
        .map(|(_, body)| body)
        .filter(|body| !body.is_empty())
}

#[cfg(test)]
#[path = "task_service_model_projection_tests.rs"]
mod tests;
