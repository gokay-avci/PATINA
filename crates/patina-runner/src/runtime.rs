use patina_external::EvalError;
use patina_types::{WorkerFailure, WorkerFailureKind};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

use crate::RunnerError;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn ensure_workdir(path: &Path) -> Result<(), RunnerError> {
    fs::create_dir_all(path).map_err(|source| RunnerError::CreateWorkdir {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn create_dir(path: &Path) -> Result<(), RunnerError> {
    fs::create_dir(path).map_err(|source| RunnerError::CreateWorkdir {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn clear_directory_contents(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            fs::remove_dir_all(entry_path)?;
        } else {
            fs::remove_file(entry_path)?;
        }
    }
    Ok(())
}

pub(crate) fn create_persistent_sandbox(path: &Path) -> Result<(), RunnerError> {
    clear_directory_contents(path).map_err(|source| RunnerError::PrepareSandbox {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn unique_run_name() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("run_{millis}_{seq}")
}

pub(crate) fn map_eval_error(error: &EvalError) -> WorkerFailure {
    match error {
        EvalError::ProcessFailed { stderr, .. } => WorkerFailure {
            kind: WorkerFailureKind::ProcessFailed,
            message: stderr.clone(),
        },
        EvalError::ParseFailed { line, reason } => WorkerFailure {
            kind: WorkerFailureKind::ParseFailed,
            message: format!("line {line}: {reason}"),
        },
        EvalError::NotConverged {
            energy, n_steps, ..
        } => WorkerFailure {
            kind: WorkerFailureKind::NotConverged,
            message: format!("backend did not converge after {n_steps} steps (energy={energy})"),
        },
        EvalError::Timeout { elapsed } => WorkerFailure {
            kind: WorkerFailureKind::Timeout,
            message: format!("backend timed out after {elapsed:?}"),
        },
        EvalError::IoError(source) => WorkerFailure {
            kind: WorkerFailureKind::Io,
            message: source.to_string(),
        },
        EvalError::TemplateInvalid { path, reason } => WorkerFailure {
            kind: WorkerFailureKind::TemplateInvalid,
            message: format!("{}: {reason}", path.display()),
        },
    }
}

pub(crate) fn remove_dir_all_with_warn(path: &Path) {
    if let Err(err) = fs::remove_dir_all(path) {
        warn!(path = %path.display(), error = %err, "failed to remove sandbox");
    }
}

pub(crate) fn remove_empty_dir_with_warn(path: &Path) {
    if let Err(err) = fs::remove_dir(path) {
        warn!(path = %path.display(), error = %err, "failed to remove run directory");
    }
}

pub(crate) fn remove_session_dir_with_warn(path: &PathBuf) {
    if let Err(err) = fs::remove_dir_all(path) {
        warn!(
            path = %path.display(),
            error = %err,
            "failed to remove persistent worker session directory"
        );
    }
}
