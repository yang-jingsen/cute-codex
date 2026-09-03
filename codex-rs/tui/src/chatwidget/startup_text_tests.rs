use super::*;
use pretty_assertions::assert_eq;

fn selection(tooltips_enabled: bool) -> StartupTextSelection {
    StartupTextSelection {
        plan_type: None,
        fast_mode_enabled: false,
        tooltips_enabled,
    }
}

#[test]
fn first_event_help_is_consumed_once() {
    let mut state = StartupTextState::first_event();

    assert_eq!(
        state.take(selection(/*tooltips_enabled*/ true)),
        SessionInfoText::FirstEventHelp
    );
    assert_eq!(
        state.take(selection(/*tooltips_enabled*/ true)),
        SessionInfoText::None
    );
}

#[test]
fn selected_tooltip_is_consumed_once_without_rerolling() {
    let mut state = StartupTextState::tooltip(Some("session-selected tip".to_string()));

    assert_eq!(
        state.take(selection(/*tooltips_enabled*/ true)),
        SessionInfoText::Tooltip("session-selected tip".to_string())
    );
    assert_eq!(
        state.take(selection(/*tooltips_enabled*/ true)),
        SessionInfoText::None
    );
}

#[test]
fn disabled_tooltips_consume_pending_copy() {
    let mut state = StartupTextState::tooltip(Some("hidden tip".to_string()));

    assert_eq!(
        state.take(selection(/*tooltips_enabled*/ false)),
        SessionInfoText::None
    );
    assert_eq!(
        state.take(selection(/*tooltips_enabled*/ true)),
        SessionInfoText::None
    );
}
