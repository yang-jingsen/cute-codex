use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use serde::Serialize;

use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::key_hint::KeyBindingListExt;

pub(crate) const PROTOCOL_ENV_VAR: &str = "CUTE_CODEX_TERMINAL_PROTOCOL";
const PROTOCOL_ENV_VALUE: &str = "osc777";
const SCHEMA: &str = "cutecharm-cutex.terminal.v1";
const SOURCE: &str = "cute-codex";
const OSC_PREFIX: &str = "\x1b]777;cutecharm-cutex;";
const OSC_SUFFIX: &str = "\x07";
pub(crate) const COMPOSER_NEWLINE_INPUT_TOKEN: &str = "__CUTE_CODEX_COMPOSER_NEWLINE__";
const COMPOSER_NEWLINE_INPUT_TOKEN_BASE64: &str = "X19DVVRFX0NPREVYX0NPTVBPU0VSX05FV0xJTkVfXw";

#[derive(Debug, Default)]
pub(crate) struct TerminalSidebandEmitter {
    enabled: bool,
    next_seq: u64,
    last_keymap: Option<KeymapSidebandState>,
}

impl TerminalSidebandEmitter {
    pub(crate) fn from_env() -> Self {
        let enabled = protocol_enabled_from_env();
        Self {
            enabled,
            next_seq: 1,
            last_keymap: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn enabled_for_tests() -> Self {
        Self {
            enabled: true,
            next_seq: 1,
            last_keymap: None,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn encode(&mut self, state: TerminalSidebandState) -> serde_json::Result<String> {
        let mut output = String::new();
        if let Some(keymap) = state.keymap
            && self.last_keymap.as_ref() != Some(&keymap)
        {
            let frame = KeymapSidebandFrame {
                schema: SCHEMA,
                kind: "keymap_state",
                seq: self.next_seq(),
                timestamp_ms: current_timestamp_ms(),
                source: SOURCE,
                mode: keymap.mode,
                bindings: keymap.bindings.clone(),
            };
            output.push_str(&encode_frame(&frame)?);
            self.last_keymap = Some(keymap);
        }

        let frame = ComposerSidebandFrame {
            schema: SCHEMA,
            kind: "composer_state",
            seq: self.next_seq(),
            timestamp_ms: current_timestamp_ms(),
            source: SOURCE,
            cols: state.cols,
            rows: state.rows,
            input_ready: state.input_ready,
            mode: state.mode,
            composer: state.composer,
            footer: state.footer,
        };
        output.push_str(&encode_frame(&frame)?);
        Ok(output)
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        seq
    }
}

pub(crate) fn protocol_enabled_from_env() -> bool {
    std::env::var(PROTOCOL_ENV_VAR)
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case(PROTOCOL_ENV_VALUE))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TerminalSidebandState {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) input_ready: bool,
    pub(crate) mode: &'static str,
    pub(crate) keymap: Option<KeymapSidebandState>,
    pub(crate) composer: ComposerSidebandState,
    pub(crate) footer: Option<FooterSidebandState>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct ComposerSidebandFrame {
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
struct KeymapSidebandFrame {
    schema: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    seq: u64,
    timestamp_ms: u64,
    source: &'static str,
    mode: &'static str,
    bindings: Vec<KeymapBindingSidebandState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct KeymapSidebandState {
    pub(crate) mode: &'static str,
    pub(crate) bindings: Vec<KeymapBindingSidebandState>,
}

impl KeymapSidebandState {
    pub(crate) fn empty(mode: &'static str) -> Self {
        Self {
            mode,
            bindings: Vec::new(),
        }
    }

    pub(crate) fn for_composer_bindings(
        mode: &'static str,
        submit_keys: &[KeyBinding],
        queue_keys: &[KeyBinding],
        insert_newline_keys: &[KeyBinding],
    ) -> Self {
        let mut state = Self::empty(mode);
        if !matches!(mode, "editing" | "assistant_running") {
            return state;
        }

        for binding in insert_newline_keys {
            let (code, modifiers) = binding.parts();
            let event = KeyEvent::new(code, modifiers);
            if submit_keys.is_pressed(event) || queue_keys.is_pressed(event) {
                continue;
            }
            let Some(key) = sideband_key_label(*binding) else {
                continue;
            };
            if state.bindings.iter().any(|existing| existing.key == key) {
                continue;
            }
            state.bindings.push(KeymapBindingSidebandState {
                key,
                action: "composer.newline",
                input: KeymapInputSidebandState {
                    kind: "bytes",
                    data_base64: COMPOSER_NEWLINE_INPUT_TOKEN_BASE64,
                },
            });
        }

        state
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct KeymapBindingSidebandState {
    pub(crate) key: String,
    pub(crate) action: &'static str,
    pub(crate) input: KeymapInputSidebandState,
}

fn sideband_key_label(binding: KeyBinding) -> Option<String> {
    let (key, mut modifiers) = key_hint::normalize_key_parts(binding.parts().0, binding.parts().1);
    let supported_modifiers = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT;
    if !modifiers.difference(supported_modifiers).is_empty() {
        return None;
    }

    let key = match key {
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "page-up".to_string(),
        KeyCode::PageDown => "page-down".to_string(),
        KeyCode::F(number) if (1..=12).contains(&number) => format!("f{number}"),
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(ch) if ch == '-' => "minus".to_string(),
        KeyCode::Char(ch) if ch.is_ascii() && !ch.is_ascii_control() => {
            let mut ch = ch;
            if ch.is_ascii_uppercase() {
                modifiers.insert(KeyModifiers::SHIFT);
                ch = ch.to_ascii_lowercase();
            }
            ch.to_string()
        }
        _ => return None,
    };

    let mut parts = Vec::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl".to_string());
    }
    if modifiers.contains(KeyModifiers::ALT) {
        parts.push("alt".to_string());
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("shift".to_string());
    }
    parts.push(key);
    Some(parts.join("+"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct KeymapInputSidebandState {
    pub(crate) kind: &'static str,
    pub(crate) data_base64: &'static str,
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

fn encode_frame(frame: &impl Serialize) -> serde_json::Result<String> {
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
    use crossterm::event::KeyCode;
    use serde_json::Value;

    use crate::keymap::RuntimeKeymap;

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
        let runtime_keymap = RuntimeKeymap::defaults();
        let encoded = emitter
            .encode(TerminalSidebandState {
                cols: 100,
                rows: 30,
                input_ready: true,
                mode: "editing",
                keymap: Some(KeymapSidebandState::for_composer_bindings(
                    "editing",
                    &runtime_keymap.composer.submit,
                    &runtime_keymap.composer.queue,
                    &runtime_keymap.editor.insert_newline,
                )),
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

        let frames = decode_frames(&encoded);
        assert_eq!(frames.len(), 2);
        let keymap = &frames[0];
        assert_eq!(keymap["schema"], "cutecharm-cutex.terminal.v1");
        assert_eq!(keymap["type"], "keymap_state");
        assert_eq!(keymap["seq"], 1);
        assert_eq!(keymap["source"], "cute-codex");
        assert_eq!(keymap["mode"], "editing");
        let keys = keymap["bindings"]
            .as_array()
            .expect("bindings array")
            .iter()
            .map(|binding| binding["key"].as_str().expect("key string"))
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["ctrl+j", "ctrl+m", "shift+enter", "alt+enter"]);
        for binding in keymap["bindings"].as_array().expect("bindings array") {
            assert_eq!(binding["action"], "composer.newline");
            assert_eq!(binding["input"]["kind"], "bytes");
            assert_eq!(
                binding["input"]["data_base64"],
                "X19DVVRFX0NPREVYX0NPTVBPU0VSX05FV0xJTkVfXw"
            );
        }

        let value = &frames[1];
        assert_eq!(value["schema"], "cutecharm-cutex.terminal.v1");
        assert_eq!(value["type"], "composer_state");
        assert_eq!(value["seq"], 2);
        assert_eq!(value["source"], "cute-codex");
        assert_eq!(value["cols"], 100);
        assert_eq!(value["rows"], 30);
        assert_eq!(value["mode"], "editing");
        assert_eq!(value["composer"]["text"], "ab\nc");
        assert_eq!(value["composer"]["cursor_index"], 3);
        assert_eq!(value["composer"]["caret"]["row"], 23);
        assert_eq!(value["composer"]["ime_anchor"]["column"], 4);
    }

    #[test]
    fn terminal_sideband_keymap_omits_ctrl_j_when_not_editor_newline() {
        let mut runtime_keymap = RuntimeKeymap::defaults();
        runtime_keymap.editor.insert_newline.clear();

        let state = KeymapSidebandState::for_composer_bindings(
            "editing",
            &runtime_keymap.composer.submit,
            &runtime_keymap.composer.queue,
            &runtime_keymap.editor.insert_newline,
        );

        assert_eq!(state, KeymapSidebandState::empty("editing"));
    }

    #[test]
    fn terminal_sideband_keymap_omits_only_bindings_intercepted_by_submit() {
        let mut runtime_keymap = RuntimeKeymap::defaults();
        runtime_keymap
            .composer
            .submit
            .push(key_hint::ctrl(KeyCode::Char('j')));

        let state = KeymapSidebandState::for_composer_bindings(
            "editing",
            &runtime_keymap.composer.submit,
            &runtime_keymap.composer.queue,
            &runtime_keymap.editor.insert_newline,
        );

        assert_eq!(
            state
                .bindings
                .iter()
                .map(|binding| binding.key.as_str())
                .collect::<Vec<_>>(),
            vec!["ctrl+m", "shift+enter", "alt+enter"]
        );
    }

    #[test]
    fn terminal_sideband_keymap_empty_when_all_newline_bindings_are_intercepted() {
        let runtime_keymap = RuntimeKeymap::defaults();
        let insert_newline = vec![key_hint::ctrl(KeyCode::Char('j'))];
        let submit = vec![key_hint::ctrl(KeyCode::Char('j'))];

        let state = KeymapSidebandState::for_composer_bindings(
            "editing",
            &submit,
            &runtime_keymap.composer.queue,
            &insert_newline,
        );

        assert_eq!(state, KeymapSidebandState::empty("editing"));
    }

    #[test]
    fn terminal_sideband_keymap_uses_custom_insert_newline_bindings() {
        let runtime_keymap = RuntimeKeymap::defaults();
        let insert_newline = vec![
            key_hint::shift(KeyCode::Enter),
            key_hint::alt(KeyCode::Enter),
            key_hint::ctrl(KeyCode::Char('x')),
        ];

        let state = KeymapSidebandState::for_composer_bindings(
            "editing",
            &runtime_keymap.composer.submit,
            &runtime_keymap.composer.queue,
            &insert_newline,
        );

        assert_eq!(
            state
                .bindings
                .iter()
                .map(|binding| binding.key.as_str())
                .collect::<Vec<_>>(),
            vec!["shift+enter", "alt+enter", "ctrl+x"]
        );
    }

    #[test]
    fn terminal_sideband_keymap_emits_empty_state_for_non_composer_modes() {
        let runtime_keymap = RuntimeKeymap::defaults();

        let state = KeymapSidebandState::for_composer_bindings(
            "approval_prompt",
            &runtime_keymap.composer.submit,
            &runtime_keymap.composer.queue,
            &runtime_keymap.editor.insert_newline,
        );

        assert_eq!(state, KeymapSidebandState::empty("approval_prompt"));
    }

    #[test]
    fn terminal_sideband_keymap_state_is_sent_only_when_changed() {
        let runtime_keymap = RuntimeKeymap::defaults();
        let mut emitter = TerminalSidebandEmitter::enabled_for_tests();
        let first = emitter
            .encode(minimal_state(Some(
                KeymapSidebandState::for_composer_bindings(
                    "editing",
                    &runtime_keymap.composer.submit,
                    &runtime_keymap.composer.queue,
                    &runtime_keymap.editor.insert_newline,
                ),
            )))
            .expect("first encode");
        let second = emitter
            .encode(minimal_state(Some(
                KeymapSidebandState::for_composer_bindings(
                    "editing",
                    &runtime_keymap.composer.submit,
                    &runtime_keymap.composer.queue,
                    &runtime_keymap.editor.insert_newline,
                ),
            )))
            .expect("second encode");

        assert_eq!(decode_frames(&first).len(), 2);
        assert_eq!(decode_frames(&second).len(), 1);
        assert_eq!(decode_frames(&second)[0]["type"], "composer_state");
    }

    fn minimal_state(keymap: Option<KeymapSidebandState>) -> TerminalSidebandState {
        TerminalSidebandState {
            cols: 80,
            rows: 24,
            input_ready: true,
            mode: "editing",
            keymap,
            composer: ComposerSidebandState::hidden(),
            footer: None,
        }
    }

    fn decode_frames(encoded: &str) -> Vec<Value> {
        const PREFIX: &str = "\x1b]777;cutecharm-cutex;";
        encoded
            .split(PREFIX)
            .filter(|part| !part.is_empty())
            .map(|part| {
                let payload = part.strip_suffix('\x07').expect("osc suffix");
                let json = URL_SAFE_NO_PAD.decode(payload).expect("base64url payload");
                serde_json::from_slice(&json).expect("json payload")
            })
            .collect()
    }
}
