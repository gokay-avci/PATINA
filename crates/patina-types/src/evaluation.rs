use crate::{Candidate, StructureRecord};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Result of evaluating a candidate with a backend such as GULP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalResult {
    /// Final energy reported by the backend.
    pub energy: f64,
    /// Forces on each site, if reported by the backend.
    pub forces: Vec<[f64; 3]>,
    /// Relaxed geometry produced by the backend.
    pub relaxed_candidate: Candidate,
    /// Whether the backend reported a converged optimisation.
    pub converged: bool,
    /// Wall-clock duration spent in the backend.
    pub wall_time: Duration,
}

/// Compact evaluation payload used for checkpoints and resumable orchestration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationRecord {
    /// Human-readable identifier matching the associated structure.
    pub label: String,
    /// Final energy reported by the backend.
    #[serde(
        serialize_with = "serialize_checkpoint_energy",
        deserialize_with = "deserialize_checkpoint_energy"
    )]
    pub energy: f64,
    /// Whether the backend reported a converged optimisation.
    pub converged: bool,
    /// Relaxed structure produced by the backend.
    pub structure: StructureRecord,
    /// Optional backend run directory for provenance.
    pub backend_run_dir: Option<String>,
    /// Optional primary backend output path for provenance.
    pub primary_output_path: Option<String>,
}

impl EvaluationRecord {
    /// Reconstructs a minimal evaluation result for in-memory continuation.
    pub fn to_eval_result(&self) -> EvalResult {
        let relaxed_candidate = Candidate::from(&self.structure);
        EvalResult {
            energy: self.energy,
            forces: vec![[0.0, 0.0, 0.0]; relaxed_candidate.len()],
            relaxed_candidate,
            converged: self.converged,
            wall_time: Duration::from_secs(0),
        }
    }
}

impl From<&EvalResult> for EvaluationRecord {
    fn from(result: &EvalResult) -> Self {
        Self {
            label: result.relaxed_candidate.label.clone(),
            energy: checkpoint_energy_value(result.energy),
            converged: result.converged,
            structure: StructureRecord::from(&result.relaxed_candidate),
            backend_run_dir: None,
            primary_output_path: None,
        }
    }
}

fn checkpoint_energy_value(energy: f64) -> f64 {
    if energy.is_finite() {
        energy
    } else {
        // Checkpoints must remain JSON-reloadable even when runtime failure paths use
        // non-finite sentinel energies to mark worst-case members.
        f64::MAX
    }
}

fn serialize_checkpoint_energy<S>(energy: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f64(checkpoint_energy_value(*energy))
}

fn deserialize_checkpoint_energy<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let decoded = Option::<f64>::deserialize(deserializer)?;
    Ok(checkpoint_energy_value(decoded.unwrap_or(f64::MAX)))
}
