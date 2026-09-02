use super::*;
use pretty_assertions::assert_eq;

#[test]
fn deserialize_skill_config_with_name_selector() {
    let cfg: SkillConfig = toml::from_str(
        r#"
            name = "github:yeet"
            enabled = false
        "#,
    )
    .expect("should deserialize skill config with name selector");

    assert_eq!(cfg.name.as_deref(), Some("github:yeet"));
    assert_eq!(cfg.path, None);
    assert!(!cfg.enabled);
}

#[test]
fn deserialize_skill_config_with_path_selector() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let skill_path = tempdir.path().join("skills").join("demo").join("SKILL.md");
    let cfg: SkillConfig = toml::from_str(&format!(
        r#"
            path = {path:?}
            enabled = false
        "#,
        path = skill_path.display().to_string(),
    ))
    .expect("should deserialize skill config with path selector");

    assert_eq!(
        cfg,
        SkillConfig {
            path: Some(
                AbsolutePathBuf::from_absolute_path(&skill_path)
                    .expect("skill path should be absolute"),
            ),
            name: None,
            enabled: false,
        }
    );
}

#[test]
fn memories_config_clamps_count_limits_to_nonzero_values() {
    let config = MemoriesConfig::from(MemoriesToml {
        max_raw_memories_for_consolidation: Some(0),
        max_rollouts_per_startup: Some(0),
        ..Default::default()
    });

    assert_eq!(
        config,
        MemoriesConfig {
            max_raw_memories_for_consolidation: 1,
            max_rollouts_per_startup: 1,
            ..MemoriesConfig::default()
        }
    );
}

#[test]
fn memories_config_clamps_rate_limit_remaining_threshold() {
    let config = MemoriesConfig::from(MemoriesToml {
        min_rate_limit_remaining_percent: Some(101),
        ..Default::default()
    });
    assert_eq!(
        config,
        MemoriesConfig {
            min_rate_limit_remaining_percent: 100,
            ..MemoriesConfig::default()
        }
    );

    let config = MemoriesConfig::from(MemoriesToml {
        min_rate_limit_remaining_percent: Some(-1),
        ..Default::default()
    });
    assert_eq!(
        config,
        MemoriesConfig {
            min_rate_limit_remaining_percent: 0,
            ..MemoriesConfig::default()
        }
    );
}

#[test]
fn cutex_presentation_config_accepts_typed_styles_and_keeps_defaults() {
    let settings: TuiCutexActivitySettings = toml::from_str(
        r##"
            [inbound_message]
            label = "{author} → {recipient}\n{role}"
            show_metadata = false
            show_ids = false
            content_indent = 6

            [inbound_message.style]
            foreground = "magenta"
            bold = false
            dim = true
            italic = true

            [task_assignment.review_ready]
            template = "{assignee} ready"

            [task_assignment.review_ready.style]
            foreground = "#98FF98"
            bold = true
        "##,
    )
    .expect("typed Cutex presentation should deserialize");

    assert!(settings.visible);
    assert_eq!(settings.inbound_message.content_indent, 6);
    assert!(!settings.inbound_message.show_metadata);
    assert_eq!(
        settings.inbound_message.style.foreground,
        Some(TuiCutexForegroundColor::Magenta)
    );
    assert_eq!(
        settings.task_assignment.review_ready.template.as_deref(),
        Some("{assignee} ready")
    );
    assert_eq!(
        settings.task_assignment.review_ready.style.foreground,
        Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98))
    );

    let partial: TuiCutexActivitySettings = toml::from_str(
        r#"
            [task_service_message]
            visible = true
        "#,
    )
    .expect("partial Task Service presentation should keep safe per-class defaults");
    assert_eq!(
        partial.task_service_message.terminal_closure.show,
        Some(false)
    );
}

#[test]
fn cutex_presentation_config_rejects_unknown_colors_and_fields() {
    let unknown_color = toml::from_str::<TuiCutexActivitySettings>(
        r#"
            [inbound_message.style]
            foreground = "execute_shell"
        "#,
    );
    assert!(unknown_color.is_err());

    let unknown_field = toml::from_str::<TuiCutexActivitySettings>(
        r#"
            [inbound_message]
            arbitrary_code = "run"
        "#,
    );
    assert!(unknown_field.is_err());
}

#[test]
fn agent_management_phase_config_accepts_frozen_examples_and_typed_overrides() {
    let settings: TuiCutexActivitySettings = toml::from_str(
        r#"
            phase_display = "append"

            [director_rotate]
            phase_display = "coalesce"

            [director_rotate.predecessor_closing]
            template = "{agent_name}：自刎归天！"
            foreground = "red"
            bold = true
            indent = 2
            wrap_width = 72

            [director_rotate.successor_ready]
            template = "恭喜 {agent_name} 已经可以撑地了"
            foreground = "green"
            italic = true

            [replace.ready]
            show = false
        "#,
    )
    .expect("typed Agent Management phase presentation should deserialize");

    assert_eq!(
        settings.phase_display,
        TuiAgentManagementPhaseDisplay::Append
    );
    assert_eq!(
        settings.director_rotate.phase_display,
        Some(TuiAgentManagementPhaseDisplay::Coalesce)
    );
    assert_eq!(
        settings
            .director_rotate
            .predecessor_closing
            .as_ref()
            .and_then(|phase| phase.template.as_deref()),
        Some("{agent_name}：自刎归天！")
    );
    assert_eq!(
        settings
            .director_rotate
            .successor_ready
            .as_ref()
            .and_then(|phase| phase.foreground),
        Some(TuiCutexForegroundColor::Green)
    );
    assert_eq!(
        settings.replace.ready.as_ref().and_then(|phase| phase.show),
        Some(false)
    );
}

#[test]
fn agent_management_phase_config_rejects_unknown_fields_colors_and_modes() {
    for invalid in [
        r#"
            [director_rotate.predecessor_closing]
            foreground = "execute_shell"
        "#,
        r#"
            [director_rotate.predecessor_closing]
            arbitrary_field = true
        "#,
        r#"
            phase_display = "rewrite_history"
        "#,
        r#"
            [director_rotate.unknown_phase]
            show = true
        "#,
    ] {
        assert!(toml::from_str::<TuiCutexActivitySettings>(invalid).is_err());
    }
}
