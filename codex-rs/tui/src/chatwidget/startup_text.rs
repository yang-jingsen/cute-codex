//! One-shot ownership for the semantic text attached to a session header.

use codex_protocol::account::PlanType;

use crate::history_cell::SessionInfoText;
use crate::tooltips;

#[derive(Clone, Copy, Debug)]
pub(super) struct StartupTextSelection {
    pub(super) plan_type: Option<PlanType>,
    pub(super) fast_mode_enabled: bool,
    pub(super) tooltips_enabled: bool,
}

#[derive(Debug)]
pub(super) enum StartupTextState {
    FirstEventHelpPending,
    TooltipPending(Option<String>),
    Consumed,
}

impl StartupTextState {
    pub(super) fn first_event() -> Self {
        Self::FirstEventHelpPending
    }

    pub(super) fn tooltip(tooltip_override: Option<String>) -> Self {
        Self::TooltipPending(tooltip_override)
    }

    /// Resolve startup copy exactly once for this chat session.
    ///
    /// Keeping the pending/consumed state on `ChatWidget` prevents a later session-header
    /// reconstruction from rerolling randomized copy. Once selected, the history cell owns the
    /// resulting string and terminal resize only asks that cell to present it at a new width.
    pub(super) fn take(&mut self, selection: StartupTextSelection) -> SessionInfoText {
        match std::mem::replace(self, Self::Consumed) {
            Self::FirstEventHelpPending => SessionInfoText::FirstEventHelp,
            Self::TooltipPending(_) if !selection.tooltips_enabled => SessionInfoText::None,
            Self::TooltipPending(tooltip_override) => tooltip_override
                .or_else(|| tooltips::get_tooltip(selection.plan_type, selection.fast_mode_enabled))
                .map(SessionInfoText::Tooltip)
                .unwrap_or(SessionInfoText::None),
            Self::Consumed => SessionInfoText::None,
        }
    }
}

#[cfg(test)]
#[path = "startup_text_tests.rs"]
mod tests;
