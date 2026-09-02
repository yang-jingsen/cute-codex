use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;

pub(crate) const PROTOCOL_ENV_VAR: &str = "CUTE_CODEX_TERMINAL_PROTOCOL";
const PROTOCOL_ENV_VALUE: &str = "osc777";
const SCHEMA: &str = "cutecharm-cutex.terminal.v1";
const SOURCE: &str = "cute-codex";
const OSC_PREFIX: &str = "\x1b]777;cutecharm-cutex;";
const OSC_SUFFIX: &str = "\x07";

#[derive(Debug, Default)]
pub(crate) struct TerminalSidebandEmitter {
    enabled: bool,
    next_seq: u64,
}

impl TerminalSidebandEmitter {
    pub(crate) fn from_env() -> Self {
        let enabled = std::env::var(PROTOCOL_ENV_VAR)
            .ok()
            .is_some_and(|value| value.eq_ignore_ascii_case(PROTOCOL_ENV_VALUE));
        Self {
            enabled,
            next_seq: 1,
        }
    }

    #[cfg(test)]
    pub(crate) fn enabled_for_tests() -> Self {
        Self {
            enabled: true,
            next_seq: 1,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn encode(&mut self, state: TerminalSidebandState) -> serde_json::Result<String> {
        let frame = TerminalSidebandFrame {
            schema: SCHEMA,
            kind: "composer_state",
            seq: self.next_seq,
            timestamp_ms: current_timestamp_ms(),
            source: SOURCE,
            cols: state.cols,
            rows: state.rows,
            input_ready: state.input_ready,
            mode: state.mode,
            composer: state.composer,
            footer: state.footer,
        };
        self.next_seq = self.next_seq.saturating_add(1);
        encode_frame(&frame)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TerminalSidebandState {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) input_ready: bool,
    pub(crate) mode: &'static str,
    pub(crate) composer: ComposerSidebandState,
    pub(crate) footer: Option<FooterSidebandState>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct TerminalSidebandFrame {
    schema: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    seq: u64,
    timestamp_ms: u64,
    source: &'static str,
    cols: u16,
    rows: u16,
    input_ready: bool,
    mode: &'static str,
    composer: ComposerSidebandState,
    #[serde(skip_serializing_if = "Option::is_none")]
    footer: Option<FooterSidebandState>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ComposerSidebandState {
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) text: String,
    pub(crate) cursor_index: usize,
    pub(crate) selection: SelectionSidebandState,
    pub(crate) multiline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) region: Option<RegionSidebandState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt: Option<PromptSidebandState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) caret: Option<PointSidebandState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ime_anchor: Option<PointSidebandState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wrap: Option<WrapSidebandState>,
}

impl ComposerSidebandState {
    pub(crate) fn hidden() -> Self {
        Self {
            visible: false,
            focused: false,
            text: String::new(),
            cursor_index: 0,
            selection: SelectionSidebandState { start: 0, end: 0 },
            multiline: false,
            region: None,
            prompt: None,
            caret: None,
            ime_anchor: None,
            wrap: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SelectionSidebandState {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct RegionSidebandState {
    pub(crate) top: u16,
    pub(crate) bottom: u16,
    pub(crate) left: u16,
    pub(crate) right: u16,
}

impl RegionSidebandState {
    pub(crate) fn from_rect(rect: ratatui::layout::Rect) -> Option<Self> {
        if rect.is_empty() {
            return None;
        }
        Some(Self {
            top: rect.y,
            bottom: rect.bottom().saturating_sub(1),
            left: rect.x,
            right: rect.right().saturating_sub(1),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PointSidebandState {
    pub(crate) row: u16,
    pub(crate) column: u16,
    pub(crate) visible: bool,
}

impl PointSidebandState {
    pub(crate) fn new(row: u16, column: u16) -> Self {
        Self {
            row,
            column,
            visible: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PromptSidebandState {
    pub(crate) row: u16,
    pub(crate) column: u16,
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct WrapSidebandState {
    pub(crate) width: u16,
    pub(crate) first_line_column: u16,
    pub(crate) continuation_column: u16,
    pub(crate) visible_start_row: u16,
    pub(crate) rows: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct FooterSidebandState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) region: Option<RegionSidebandState>,
    pub(crate) text: String,
}

pub(crate) fn utf16_index_for_byte_index(text: &str, byte_index: usize) -> usize {
    let mut clamped = byte_index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    text[..clamped].encode_utf16().count()
}

pub(crate) fn osc_sequence(encoded_payload: &str) -> String {
    format!("{OSC_PREFIX}{encoded_payload}{OSC_SUFFIX}")
}

fn encode_frame(frame: &TerminalSidebandFrame) -> serde_json::Result<String> {
    let json = serde_json::to_vec(frame)?;
    Ok(osc_sequence(&URL_SAFE_NO_PAD.encode(json)))
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::Value;

    #[test]
    fn default_emitter_is_disabled() {
        assert!(!TerminalSidebandEmitter::default().is_enabled());
    }

    #[test]
    fn utf16_index_counts_code_units_before_cursor() {
        assert_eq!(utf16_index_for_byte_index("a😀b", 0), 0);
        assert_eq!(utf16_index_for_byte_index("a😀b", 1), 1);
        assert_eq!(utf16_index_for_byte_index("a😀b", "a😀".len()), 3);
        assert_eq!(utf16_index_for_byte_index("a😀b", "a😀b".len()), 4);
    }

    #[test]
    fn utf16_index_clamps_to_char_boundary() {
        let text = "éx";
        assert_eq!(utf16_index_for_byte_index(text, 1), 0);
        assert_eq!(utf16_index_for_byte_index(text, 2), 1);
    }

    #[test]
    fn emitter_encodes_osc777_base64url_json() {
        let mut emitter = TerminalSidebandEmitter::enabled_for_tests();
        let encoded = emitter
            .encode(TerminalSidebandState {
                cols: 100,
                rows: 30,
                input_ready: true,
                mode: "editing",
                composer: ComposerSidebandState {
                    visible: true,
                    focused: true,
                    text: "ab\nc".to_string(),
                    cursor_index: 3,
                    selection: SelectionSidebandState { start: 3, end: 3 },
                    multiline: true,
                    region: Some(RegionSidebandState {
                        top: 22,
                        bottom: 24,
                        left: 2,
                        right: 99,
                    }),
                    prompt: Some(PromptSidebandState {
                        row: 22,
                        column: 2,
                        text: "> ".to_string(),
                    }),
                    caret: Some(PointSidebandState::new(23, 4)),
                    ime_anchor: Some(PointSidebandState::new(23, 4)),
                    wrap: Some(WrapSidebandState {
                        width: 96,
                        first_line_column: 4,
                        continuation_column: 2,
                        visible_start_row: 0,
                        rows: 2,
                    }),
                },
                footer: None,
            })
            .expect("encode sideband");

        assert!(encoded.starts_with("\x1b]777;cutecharm-cutex;"));
        assert!(encoded.ends_with('\x07'));
        let payload = encoded
            .trim_start_matches("\x1b]777;cutecharm-cutex;")
            .trim_end_matches('\x07');
        let json = URL_SAFE_NO_PAD.decode(payload).expect("base64url payload");
        let value: Value = serde_json::from_slice(&json).expect("json payload");

        assert_eq!(value["schema"], "cutecharm-cutex.terminal.v1");
        assert_eq!(value["type"], "composer_state");
        assert_eq!(value["seq"], 1);
        assert_eq!(value["source"], "cute-codex");
        assert_eq!(value["cols"], 100);
        assert_eq!(value["rows"], 30);
        assert_eq!(value["mode"], "editing");
        assert_eq!(value["composer"]["text"], "ab\nc");
        assert_eq!(value["composer"]["cursor_index"], 3);
        assert_eq!(value["composer"]["caret"]["row"], 23);
        assert_eq!(value["composer"]["ime_anchor"]["column"], 4);

        let encoded = emitter
            .encode(TerminalSidebandState {
                cols: 100,
                rows: 30,
                input_ready: false,
                mode: "hidden",
                composer: ComposerSidebandState::hidden(),
                footer: None,
            })
            .expect("encode second sideband frame");
        let payload = encoded
            .trim_start_matches("\x1b]777;cutecharm-cutex;")
            .trim_end_matches('\x07');
        let json = URL_SAFE_NO_PAD.decode(payload).expect("base64url payload");
        let value: Value = serde_json::from_slice(&json).expect("json payload");
        assert_eq!(value["seq"], 2);
        assert_eq!(value["mode"], "hidden");
        assert_eq!(value["composer"]["visible"], false);
    }
}
