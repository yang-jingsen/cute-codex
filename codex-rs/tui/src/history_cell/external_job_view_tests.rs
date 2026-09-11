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

#[test]
fn structured_job_view_reference_and_action_byte_boundaries() {
    let baseline = View {
        schema: "cutex.job-completion.v1".into(),
        data: json!({"jobId":"job-id","jobRevision":1,"terminalStatus":"exited"}),
    };
    let expected = render(&baseline, None);
    for text in [
        "x".repeat(256),
        "x".repeat(257),
        "x".repeat(2048),
        "é".repeat(1024),
    ] {
        let mut view = baseline.clone();
        view.data["outputReference"] = json!(text);
        assert_eq!(render(&view, None), expected);
    }
    for text in [
        "x".repeat(2049),
        format!("{}x", "é".repeat(1024)),
        String::new(),
    ] {
        let mut view = baseline.clone();
        view.data["outputReference"] = json!(text);
        assert!(render(&view, None).is_none());
    }
    for (text, accepted) in [
        ("x".repeat(256), true),
        ("é".repeat(128), true),
        ("x".repeat(257), false),
        (format!("{}x", "é".repeat(128)), false),
    ] {
        let mut view = baseline.clone();
        view.data["actionId"] = json!(text);
        view.data["outputReference"] = json!("x".repeat(2048));
        assert_eq!(render(&view, None).is_some(), accepted);
    }
}
