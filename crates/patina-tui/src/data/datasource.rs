use std::path::Path;

use anyhow::Result;

use crate::domain::artifacts::RunSnapshot;

pub trait RunDataSource {
    fn load_run(&self, run_dir: &Path) -> Result<RunSnapshot>;
}
