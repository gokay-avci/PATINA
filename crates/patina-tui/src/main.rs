#![forbid(unsafe_code)]

mod app;
mod cli;
mod data;
mod domain;
mod error;
mod ui;

use anyhow::Result;
use clap::Parser;
use data::datasource::RunDataSource;
use data::filesystem::FilesystemRunDataSource;
use patina_llm::config::LlmStackConfig;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let data_source = FilesystemRunDataSource;
    let workspace_root = std::env::current_dir()?;
    let snapshot = match cli.run_dir.as_ref() {
        Some(run_dir) => data_source.load_run(run_dir)?,
        None => crate::domain::artifacts::RunSnapshot::workspace(workspace_root.clone()),
    };
    let llm_stack = LlmStackConfig::load_toml(&cli.llm_config)?;
    let mut app =
        app::state::App::new(cli.run_dir, snapshot, cli.follow, llm_stack, cli.llm_config);
    ui::run(&mut app, &data_source)
}
