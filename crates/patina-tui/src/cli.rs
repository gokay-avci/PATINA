use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "patina-tui")]
#[command(about = "Launch and inspect PATINA workflows from the terminal")]
pub struct Cli {
    #[arg(value_name = "RUN_DIR")]
    pub run_dir: Option<PathBuf>,

    #[arg(long, help = "Enable polling-based follow mode on startup")]
    pub follow: bool,

    #[arg(
        long,
        default_value = "crates/patina-llm/install/vllm/patina-llm.vllm.toml",
        help = "Path to the PATINA LLM provider config"
    )]
    pub llm_config: PathBuf,
}
