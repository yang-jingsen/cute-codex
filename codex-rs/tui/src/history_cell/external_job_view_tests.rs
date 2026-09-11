use super::*;
use serde_json::json;

#[test]
fn structured_job_view_truth_and_unknown_fallback() {
    let mut view = View {
        schema: "cutex.job-completion.v1".into(),
        data: json!({"jobId":"job_1234567890abcdefabcdefabcd","jobRevision":1,"terminalStatus":"exited"}),
    };
    let (header, body) = render(&view, None).unwrap();
    assert!(header.starts_with("Job exited"));
    assert!(body.is_empty());
    view.data["exitCode"] = json!(0);
    view.data["actionId"] = json!("build-check");
    view.data["execution"] =
        json!({"basis":"runner_release_to_wait_v1","observedRunDurationMillis":1234});
    let (header, body) = render(&view, None).unwrap();
    insta::assert_snapshot!(format!("{header}\n{}", body.join("\n")));
    view.data["terminalStatus"] = json!("cancelled");
    assert!(render(&view, None).unwrap().0.starts_with("Job cancelled"));
    view.data["exitCode"] = json!("0");
    assert!(render(&view, None).is_none());
    view.schema = "unknown.v1".into();
    assert!(render(&view, None).is_none());
}
