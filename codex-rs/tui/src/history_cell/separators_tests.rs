use super::*;
use pretty_assertions::assert_eq;

#[test]
fn timestamps_label_plain_and_worked_separators_without_changing_on_redraw() {
    let mut snapshots = Vec::new();
    for elapsed_seconds in [None, Some(12), Some(87)] {
        let cell = FinalMessageSeparator {
            occurred_at: "14:32".into(),
            elapsed_seconds,
            runtime_metrics: None,
        };
        for width in [64, 20, 5, 0] {
            let lines = cell.display_lines(width);
            assert_eq!(lines, cell.display_lines(width));
            for line in &lines {
                assert!(line.width() <= usize::from(width));
            }
            snapshots.push(format!(
                "elapsed={elapsed_seconds:?}, width={width}\n{}",
                lines
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        let raw = cell
            .raw_lines()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            raw,
            if elapsed_seconds == Some(87) {
                "14:32 • Worked for 1m 27s"
            } else {
                "14:32"
            }
        );
    }
    insta::assert_snapshot!(snapshots.join("\n\n"));
}

#[test]
fn new_separator_captures_local_time_at_creation() {
    let before = chrono::Local::now().format("%H:%M").to_string();
    let cell =
        FinalMessageSeparator::new(/*elapsed_seconds*/ None, /*runtime_metrics*/ None);
    let after = chrono::Local::now().format("%H:%M").to_string();
    assert!(cell.occurred_at == before || cell.occurred_at == after);
    assert_eq!(cell.raw_lines()[0].to_string(), cell.occurred_at);
}

#[test]
fn historical_separator_uses_persisted_time_and_never_invents_missing_time() {
    let timestamp = 1_700_000_000;
    let expected = chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap()
        .with_timezone(&chrono::Local)
        .format("%H:%M")
        .to_string();
    let cell = FinalMessageSeparator::with_time(
        /*elapsed_seconds*/ Some(87),
        /*runtime_metrics*/ None,
        SeparatorTime::Historical(Some(timestamp)),
    );
    assert_eq!(
        cell.raw_lines()[0].to_string(),
        format!("{expected} • Worked for 1m 27s")
    );
    for timestamp in [None, Some(i64::MAX)] {
        let cell = FinalMessageSeparator::with_time(
            /*elapsed_seconds*/ None,
            /*runtime_metrics*/ None,
            SeparatorTime::Historical(timestamp),
        );
        assert_eq!(cell.raw_lines()[0].to_string(), "");
        insta::allow_duplicates! {
            insta::assert_snapshot!(cell.display_lines(20)[0].to_string(), @"────────────────────");
        }
    }
}
