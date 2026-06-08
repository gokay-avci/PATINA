use patina_external::BackendEvaluator;
use std::path::PathBuf;
use std::sync::Arc;

/// Runtime configuration for a population evaluation batch.
#[derive(Clone)]
pub struct RunConfig {
    /// Backend used to evaluate each candidate.
    pub backend: Arc<dyn BackendEvaluator>,
    /// Base directory under which sandbox directories are created.
    pub workdir: PathBuf,
    /// Number of concurrent evaluations.
    pub n_workers: usize,
    /// Whether to retain sandbox directories for debugging.
    pub keep_dirs: bool,
}

impl RunConfig {
    /// Creates a config with machine-local defaults suitable for development.
    pub fn new(backend: Arc<dyn BackendEvaluator>, workdir: impl Into<PathBuf>) -> Self {
        Self {
            backend,
            workdir: workdir.into(),
            n_workers: num_cpus::get().max(1),
            keep_dirs: false,
        }
    }
}
