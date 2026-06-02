use std::path::Path;

use anyhow::Result;
use codex_config::types::McpServerTransportConfig;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::load_global_mcp_servers;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::Value as JsonValue;
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;

struct EnvVarGuard {
    key: &'static str,
    old: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn remove(key: &'static str) -> Self {
        let old = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self { key, old }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        unsafe {
            match self.old.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    cmd.env_remove("CODEX_CONFIG_FILE");
    cmd.env_remove("CODEX_AUTH_FILE");
    cmd.env_remove("CODEX_CUSTOM_STATUS_ITEMS_FILE");
    Ok(cmd)
}

#[test]
#[serial(mcp_list_env)]
fn list_shows_empty_state() -> Result<()> {
    let _config = EnvVarGuard::remove("CODEX_CONFIG_FILE");
    let _auth = EnvVarGuard::remove("CODEX_AUTH_FILE");
    let _status = EnvVarGuard::remove("CODEX_CUSTOM_STATUS_ITEMS_FILE");
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd.args(["mcp", "list"]).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("No MCP servers configured yet."));

    Ok(())
}

#[tokio::test]
#[serial(mcp_list_env)]
async fn list_and_get_render_expected_output() -> Result<()> {
    let _config = EnvVarGuard::remove("CODEX_CONFIG_FILE");
    let _auth = EnvVarGuard::remove("CODEX_AUTH_FILE");
    let _status = EnvVarGuard::remove("CODEX_CUSTOM_STATUS_ITEMS_FILE");
    let codex_home = TempDir::new()?;

    let mut add = codex_command(codex_home.path())?;
    add.args([
        "mcp",
        "add",
        "docs",
        "--env",
        "TOKEN=secret",
        "--",
        "docs-server",
        "--port",
        "4000",
    ])
    .assert()
    .success();

    let mut servers = load_global_mcp_servers(codex_home.path()).await?;
    let docs_entry = servers
        .get_mut("docs")
        .expect("docs server should exist after add");
    match &mut docs_entry.transport {
        McpServerTransportConfig::Stdio { env_vars, .. } => {
            *env_vars = vec!["APP_TOKEN".into(), "WORKSPACE_ID".into()];
        }
        other => panic!("unexpected transport: {other:?}"),
    }
    ConfigEditsBuilder::new(codex_home.path())
        .replace_mcp_servers(&servers)
        .apply_blocking()?;

    let mut list_cmd = codex_command(codex_home.path())?;
    let list_output = list_cmd.args(["mcp", "list"]).output()?;
    assert!(list_output.status.success());
    let stdout = String::from_utf8(list_output.stdout)?;
    assert!(stdout.contains("Name"));
    assert!(stdout.contains("docs"));
    assert!(stdout.contains("docs-server"));
    assert!(stdout.contains("TOKEN=*****"));
    assert!(stdout.contains("APP_TOKEN=*****"));
    assert!(stdout.contains("WORKSPACE_ID=*****"));
    assert!(stdout.contains("Status"));
    assert!(stdout.contains("Auth"));
    assert!(stdout.contains("enabled"));
    assert!(stdout.contains("Unsupported"));

    let mut list_json_cmd = codex_command(codex_home.path())?;
    let json_output = list_json_cmd.args(["mcp", "list", "--json"]).output()?;
    assert!(json_output.status.success());
    let stdout = String::from_utf8(json_output.stdout)?;
    let parsed: JsonValue = serde_json::from_str(&stdout)?;
    assert_eq!(
        parsed,
        json!([
          {
            "name": "docs",
            "enabled": true,
            "disabled_reason": null,
            "transport": {
              "type": "stdio",
              "command": "docs-server",
              "args": [
                "--port",
                "4000"
              ],
              "env": {
                "TOKEN": "secret"
              },
              "env_vars": [
                "APP_TOKEN",
                "WORKSPACE_ID"
              ],
              "cwd": null
            },
            "startup_timeout_sec": null,
            "tool_timeout_sec": null,
            "auth_status": "unsupported"
          }
        ]
        )
    );

    let mut get_cmd = codex_command(codex_home.path())?;
    let get_output = get_cmd.args(["mcp", "get", "docs"]).output()?;
    assert!(get_output.status.success());
    let stdout = String::from_utf8(get_output.stdout)?;
    assert!(stdout.contains("docs"));
    assert!(stdout.contains("transport: stdio"));
    assert!(stdout.contains("command: docs-server"));
    assert!(stdout.contains("args: --port 4000"));
    assert!(stdout.contains("env: TOKEN=*****"));
    assert!(stdout.contains("APP_TOKEN=*****"));
    assert!(stdout.contains("WORKSPACE_ID=*****"));
    assert!(stdout.contains("enabled: true"));
    assert!(stdout.contains("remove: cute-codex mcp remove docs"));

    let mut get_json_cmd = codex_command(codex_home.path())?;
    get_json_cmd
        .args(["mcp", "get", "docs", "--json"])
        .assert()
        .success()
        .stdout(contains("\"name\": \"docs\"").and(contains("\"enabled\": true")));

    Ok(())
}

#[tokio::test]
#[serial(mcp_list_env)]
async fn get_disabled_server_shows_single_line() -> Result<()> {
    let _config = EnvVarGuard::remove("CODEX_CONFIG_FILE");
    let _auth = EnvVarGuard::remove("CODEX_AUTH_FILE");
    let _status = EnvVarGuard::remove("CODEX_CUSTOM_STATUS_ITEMS_FILE");
    let codex_home = TempDir::new()?;

    let mut add = codex_command(codex_home.path())?;
    add.args(["mcp", "add", "docs", "--", "docs-server"])
        .assert()
        .success();

    let mut servers = load_global_mcp_servers(codex_home.path()).await?;
    let docs = servers
        .get_mut("docs")
        .expect("docs server should exist after add");
    docs.enabled = false;
    ConfigEditsBuilder::new(codex_home.path())
        .replace_mcp_servers(&servers)
        .apply_blocking()?;

    let mut get_cmd = codex_command(codex_home.path())?;
    let get_output = get_cmd.args(["mcp", "get", "docs"]).output()?;
    assert!(get_output.status.success());
    let stdout = String::from_utf8(get_output.stdout)?;
    assert_eq!(stdout.trim_end(), "docs (disabled)");

    Ok(())
}
