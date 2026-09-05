//! Generates the canonical config schema fixture for development and releases.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Generate the JSON Schema for `config.toml` and write it to `config.schema.json`.
#[derive(Parser)]
#[command(name = "codex-write-config-schema")]
struct Args {
    #[arg(short, long, value_name = "PATH")]
    out: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    cutex_out: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let default_output = args.out.is_none();
    let out_path = args.out.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/config.schema.json")
    });
    codex_config::schema::write_config_schema(&out_path)?;
    if let Some(cutex_out) = args.cutex_out.or_else(|| {
        default_output
            .then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tui/cutex-tui.schema.json"))
    }) {
        codex_config::schema::write_cutex_tui_schema(&cutex_out)?;
    }
    Ok(())
}
