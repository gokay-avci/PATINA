use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("run directory `{path}` does not exist")]
    MissingRunDir { path: PathBuf },
    #[error("manifest `{path}` is not a JSON object")]
    InvalidManifest { path: PathBuf },
}
