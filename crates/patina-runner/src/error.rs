use std::path::PathBuf;
use thiserror::Error;

/// Fatal runner errors that prevent a generation from being dispatched.
#[derive(Debug, Error)]
pub enum RunnerError {
    /// Creating the base workdir or per-run sandbox failed.
    #[error("failed to create sandbox path `{path}`: {source}")]
    CreateWorkdir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Invalid worker configuration.
    #[error("run configuration must use at least one worker")]
    InvalidWorkerCount,
    /// Preparing a persistent sandbox failed.
    #[error("failed to prepare sandbox path `{path}`: {source}")]
    PrepareSandbox {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A persistent-worker evaluation ran outside the managed thread pool.
    #[error("persistent worker pool could not resolve a worker thread index")]
    MissingWorkerThreadIndex,
    /// Rayon thread-pool construction failed.
    #[error("failed to build Rayon thread pool: {0}")]
    ThreadPoolBuild(rayon::ThreadPoolBuildError),
}
