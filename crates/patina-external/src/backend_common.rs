use patina_sci_kernel::codec::xyz::{
    write_candidate_xyz as write_kernel_candidate_xyz, XyzCoordinateMode, XyzEncodeOptions,
};
use patina_types::{Candidate, EvalResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Trait implemented by all evaluation backends.
///
/// Contract:
/// - `evaluate` must treat `workdir` as backend-private scratch space for a single evaluation.
/// - Implementations may create files inside `workdir` but must not assume exclusive access to
///   sibling directories.
/// - A backend failure for one candidate must be reported as `Err(EvalError)` and must not poison
///   other evaluations running concurrently.
pub trait BackendEvaluator: Send + Sync {
    /// Evaluates a candidate inside the provided sandbox directory.
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError>;
}

impl<T> BackendEvaluator for Arc<T>
where
    T: BackendEvaluator + ?Sized,
{
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError> {
        (**self).evaluate(candidate, workdir)
    }
}

/// Deterministic mock backend used for tests.
#[derive(Debug, Default, Clone)]
pub struct MockBackend;

impl BackendEvaluator for MockBackend {
    fn evaluate(&self, candidate: &Candidate, _workdir: &Path) -> Result<EvalResult, EvalError> {
        candidate.validate().map_err(|err| EvalError::ParseFailed {
            line: 0,
            reason: format!("invalid mock candidate: {err:?}"),
        })?;

        let mut hasher = DefaultHasher::new();
        candidate.label.hash(&mut hasher);
        candidate.species.hash(&mut hasher);
        for coord in &candidate.fractional_coords {
            for value in coord {
                value.to_bits().hash(&mut hasher);
            }
        }
        let hash = hasher.finish();
        let energy = -((hash % 10_000) as f64) / 100.0;

        Ok(EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
            relaxed_candidate: candidate.clone(),
            converged: true,
            wall_time: Duration::from_millis(1),
        })
    }
}

#[derive(Debug)]
pub(crate) struct CompletedProcess {
    pub status: std::process::ExitStatus,
    pub stderr: Vec<u8>,
}

pub(crate) fn absolute_workdir(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub(crate) fn write_candidate_xyz(
    candidate: &Candidate,
    output_path: &Path,
) -> Result<(), EvalError> {
    write_kernel_candidate_xyz(
        candidate,
        output_path,
        &XyzEncodeOptions {
            coordinate_mode: XyzCoordinateMode::CartesianFromLattice,
            extxyz_fields: true,
            label: Some(candidate.label.clone()),
            energy: None,
            comment: None,
        },
    )
    .map_err(|err| EvalError::IoError(io::Error::other(format!("{err:?}"))))
}

pub(crate) fn cartesian_coords(candidate: &Candidate) -> Vec<[f64; 3]> {
    match candidate.lattice {
        Some(lattice) => candidate
            .fractional_coords
            .iter()
            .map(|coord| fractional_to_cartesian(*coord, lattice))
            .collect(),
        None => candidate.fractional_coords.clone(),
    }
}

fn fractional_to_cartesian(coord: [f64; 3], lattice: [[f64; 3]; 3]) -> [f64; 3] {
    [
        coord[0] * lattice[0][0] + coord[1] * lattice[1][0] + coord[2] * lattice[2][0],
        coord[0] * lattice[0][1] + coord[1] * lattice[1][1] + coord[2] * lattice[2][1],
        coord[0] * lattice[0][2] + coord[1] * lattice[1][2] + coord[2] * lattice[2][2],
    ]
}

pub(crate) fn cartesian_to_fractional(
    coord: [f64; 3],
    lattice: [[f64; 3]; 3],
) -> Result<[f64; 3], EvalError> {
    let inverse = invert_lattice(lattice).ok_or_else(|| EvalError::ParseFailed {
        line: 0,
        reason: "failed to invert lattice from Janus periodic response".into(),
    })?;
    Ok([
        inverse[0][0] * coord[0] + inverse[0][1] * coord[1] + inverse[0][2] * coord[2],
        inverse[1][0] * coord[0] + inverse[1][1] * coord[1] + inverse[1][2] * coord[2],
        inverse[2][0] * coord[0] + inverse[2][1] * coord[1] + inverse[2][2] * coord[2],
    ])
}

fn invert_lattice(lattice: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = lattice[0][0] * (lattice[1][1] * lattice[2][2] - lattice[1][2] * lattice[2][1])
        - lattice[0][1] * (lattice[1][0] * lattice[2][2] - lattice[1][2] * lattice[2][0])
        + lattice[0][2] * (lattice[1][0] * lattice[2][1] - lattice[1][1] * lattice[2][0]);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [
            (lattice[1][1] * lattice[2][2] - lattice[1][2] * lattice[2][1]) * inv_det,
            (lattice[0][2] * lattice[2][1] - lattice[0][1] * lattice[2][2]) * inv_det,
            (lattice[0][1] * lattice[1][2] - lattice[0][2] * lattice[1][1]) * inv_det,
        ],
        [
            (lattice[1][2] * lattice[2][0] - lattice[1][0] * lattice[2][2]) * inv_det,
            (lattice[0][0] * lattice[2][2] - lattice[0][2] * lattice[2][0]) * inv_det,
            (lattice[0][2] * lattice[1][0] - lattice[0][0] * lattice[1][2]) * inv_det,
        ],
        [
            (lattice[1][0] * lattice[2][1] - lattice[1][1] * lattice[2][0]) * inv_det,
            (lattice[0][1] * lattice[2][0] - lattice[0][0] * lattice[2][1]) * inv_det,
            (lattice[0][0] * lattice[1][1] - lattice[0][1] * lattice[1][0]) * inv_det,
        ],
    ])
}

pub(crate) fn truncate_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.chars().take(500).collect()
}

/// Errors produced by ghost-I/O evaluation.
#[derive(Debug, Error)]
pub enum EvalError {
    /// The external process failed or the expected output file was not produced.
    #[error("klmc3 subprocess failed (exit_code={exit_code:?}): {stderr}")]
    ProcessFailed {
        exit_code: Option<i32>,
        stderr: String,
    },
    /// The `.got` output did not match the expected format.
    #[error("failed to parse .got output at line {line}: {reason}")]
    ParseFailed { line: usize, reason: String },
    /// The backend reported non-convergence.
    #[error("backend did not converge after {n_steps} steps (energy={energy})")]
    NotConverged {
        energy: f64,
        n_steps: usize,
        partial_result: Option<Box<EvalResult>>,
    },
    /// The external process exceeded the configured runtime limit.
    #[error("klmc3 subprocess timed out after {elapsed:?}")]
    Timeout { elapsed: Duration },
    /// Filesystem failure.
    #[error("I/O failure: {0}")]
    IoError(#[from] io::Error),
    /// Invalid or inconsistent `.gin` template.
    #[error("invalid gin template `{path}`: {reason}")]
    TemplateInvalid { path: PathBuf, reason: String },
}
