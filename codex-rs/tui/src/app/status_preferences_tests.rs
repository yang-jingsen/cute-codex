use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn status_preferences_share_order_without_changing_runtime_config() {
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("config.toml");
    let shared = root.path().join("status-line.toml");
    std::fs::write(&local, "model='unchanged'\n").unwrap();
    let ids = vec!["current-dir".into(), "cutex_welcome".into()];
    save(
        &local,
        Some(shared.clone().into_os_string()),
        &ids,
        /*use_theme_colors*/ false,
    )
    .await
    .unwrap();
    let value: toml::Value = toml::from_str(&std::fs::read_to_string(&shared).unwrap()).unwrap();
    assert_eq!(
        value["tui"]["status_line"]
            .clone()
            .try_into::<Vec<String>>()
            .unwrap(),
        ids
    );
    assert_eq!(
        value["tui"]["status_line_use_colors"].as_bool(),
        Some(false)
    );
    assert_eq!(
        std::fs::read_to_string(&local).unwrap(),
        "model='unchanged'\n"
    );
    save(
        &local,
        Some(shared.clone().into_os_string()),
        &[],
        /*use_theme_colors*/ true,
    )
    .await
    .unwrap();
    let value: toml::Value = toml::from_str(&std::fs::read_to_string(&shared).unwrap()).unwrap();
    assert_eq!(value["tui"]["status_line"].as_array().unwrap().len(), 0);
    save(
        &local, /*shared_path*/ None, &ids, /*use_theme_colors*/ true,
    )
    .await
    .unwrap();
    let value: toml::Value = toml::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
    assert_eq!(value["model"].as_str(), Some("unchanged"));
    assert_eq!(
        value["tui"]["status_line"]
            .clone()
            .try_into::<Vec<String>>()
            .unwrap(),
        ids
    );
}
