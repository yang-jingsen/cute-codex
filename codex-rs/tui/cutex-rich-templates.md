# Rich Cutex presentation templates

Cutex presentation config is loaded from `~/.cutex/configs/tui.toml` (or the
explicit `CUTEX_TUI_CONFIG_PATH` override). Agent Management phase
presentations, Task Service message classes, and Task Watchdog stages accept an
optional `rich_template`.

`rich_template` is an ordered list of 1–16 spans. A span has required `text`
and optional `foreground`, `bold`, `dim`, and `italic`. Foreground accepts the
existing named colors or a strict `#RRGGBB` value. Per-span fields inherit the
presentation's base style and override only fields that are present.

The aggregate is limited to 1,024 configured Unicode scalar values, 2,048
rendered scalar values, and 8 rendered lines. Newline is the only permitted
control character. Rich templates cannot be combined with `template`,
`templates`, `grouped_templates`, or `selection`.

This example renders the Director escalation as a two-line ordinary Cutex
history cell. The timestamp is the watchdog producer's authoritative decision
time (`occurredAtMs`) formatted as UTC RFC3339 with milliseconds.

```toml
[task_watchdog.director_escalated]
rich_template = [
  { text = "task_service: " },
  { text = "我等了很久，我不会再等了。", foreground = "#98FF98" },
  { text = "\nidle-agent: {assignee} idle-time: {idle} timestamp: {timestamp}", foreground = "#D3D3D3", bold = false },
]
```

Truecolor is passed to the terminal through Ratatui as RGB. A terminal that
does not support truecolor may quantize the displayed color; no ANSI escapes
are embedded in configured or rendered content.

The generated schema for this shared file is `codex-rs/tui/cutex-tui.schema.json`.
