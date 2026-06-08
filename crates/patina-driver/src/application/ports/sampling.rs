use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use patina_types::{Candidate, EvalResult};

use crate::application::basin_hopping::BasinHoppingWorkflowExecution;
use crate::application::energy_lid::{
    EnergyLidStart, EnergyLidWorkflowExecution, SimulatedAnnealingWorkflowExecution,
};
use crate::application::scan_surface::ScanSurfaceWorkflowExecution;
use crate::application::solid_solutions::SolidSolutionsWorkflowExecution;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingWorkflowIntent {
    BasinHopping,
    SolidSolutions,
    ScanSurface,
    SurfaceReconstruction,
    SimulatedAnnealing,
    EnergyLid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingEvaluationIntent {
    InitialState,
    SamplingStep,
    QuenchStep,
    Relaxation,
}

impl SamplingEvaluationIntent {
    pub fn uses_relaxation_backend(self) -> bool {
        matches!(self, Self::Relaxation)
    }
}

#[derive(Debug, Clone)]
pub struct SamplingEvaluationRequest {
    pub candidate: Candidate,
    pub eval_dir: PathBuf,
    pub workflow: SamplingWorkflowIntent,
    pub intent: SamplingEvaluationIntent,
    pub step_index: Option<usize>,
    pub lid_index: Option<usize>,
    pub runner_index: Option<usize>,
}

impl SamplingEvaluationRequest {
    pub fn validate_shape(&self) -> Result<()> {
        match (self.workflow, self.intent) {
            (SamplingWorkflowIntent::BasinHopping, SamplingEvaluationIntent::InitialState)
            | (SamplingWorkflowIntent::BasinHopping, SamplingEvaluationIntent::SamplingStep)
            | (SamplingWorkflowIntent::SolidSolutions, SamplingEvaluationIntent::InitialState)
            | (SamplingWorkflowIntent::SolidSolutions, SamplingEvaluationIntent::SamplingStep)
            | (SamplingWorkflowIntent::ScanSurface, SamplingEvaluationIntent::InitialState)
            | (SamplingWorkflowIntent::ScanSurface, SamplingEvaluationIntent::SamplingStep)
            | (
                SamplingWorkflowIntent::SurfaceReconstruction,
                SamplingEvaluationIntent::InitialState,
            )
            | (
                SamplingWorkflowIntent::SurfaceReconstruction,
                SamplingEvaluationIntent::SamplingStep,
            ) => {
                if self.lid_index.is_some() || self.runner_index.is_some() {
                    anyhow::bail!(
                        "sampling request for {:?}/{:?} must not carry lid or runner indices",
                        self.workflow,
                        self.intent
                    );
                }
            }
            (
                SamplingWorkflowIntent::SimulatedAnnealing,
                SamplingEvaluationIntent::InitialState,
            ) => {
                if self.step_index.is_some()
                    || self.lid_index.is_some()
                    || self.runner_index.is_some()
                {
                    anyhow::bail!(
                        "simulated annealing initial-state request must not carry step, lid, or runner indices"
                    );
                }
            }
            (
                SamplingWorkflowIntent::SimulatedAnnealing,
                SamplingEvaluationIntent::SamplingStep,
            ) => {
                if self.step_index.is_none()
                    || self.lid_index.is_some()
                    || self.runner_index.is_some()
                {
                    anyhow::bail!(
                        "simulated annealing sampling-step request must carry only step_index"
                    );
                }
            }
            (SamplingWorkflowIntent::SimulatedAnnealing, SamplingEvaluationIntent::QuenchStep) => {
                if self.step_index.is_none()
                    || self.lid_index.is_some()
                    || self.runner_index.is_none()
                {
                    anyhow::bail!(
                        "simulated annealing quench-step request must carry step_index and runner_index only"
                    );
                }
            }
            (SamplingWorkflowIntent::SimulatedAnnealing, SamplingEvaluationIntent::Relaxation) => {
                if self.lid_index.is_some() {
                    anyhow::bail!(
                        "simulated annealing relaxation request must not carry a lid index"
                    );
                }
            }
            (SamplingWorkflowIntent::EnergyLid, SamplingEvaluationIntent::InitialState) => {
                if self.step_index.is_some()
                    || self.lid_index.is_some()
                    || self.runner_index.is_some()
                {
                    anyhow::bail!(
                        "energy-lid initial-state request must not carry step, lid, or runner indices"
                    );
                }
            }
            (SamplingWorkflowIntent::EnergyLid, SamplingEvaluationIntent::SamplingStep) => {
                if self.step_index.is_none()
                    || self.lid_index.is_none()
                    || self.runner_index.is_some()
                {
                    anyhow::bail!(
                        "energy-lid sampling-step request must carry step_index and lid_index only"
                    );
                }
            }
            (SamplingWorkflowIntent::EnergyLid, SamplingEvaluationIntent::QuenchStep) => {
                if self.step_index.is_none()
                    || self.lid_index.is_none()
                    || self.runner_index.is_none()
                {
                    anyhow::bail!(
                        "energy-lid quench-step request must carry step_index, lid_index, and runner_index"
                    );
                }
            }
            (SamplingWorkflowIntent::EnergyLid, SamplingEvaluationIntent::Relaxation) => {
                if self.lid_index.is_none() || self.runner_index.is_none() {
                    anyhow::bail!(
                        "energy-lid relaxation request must carry lid_index and runner_index"
                    );
                }
            }
            (SamplingWorkflowIntent::BasinHopping, SamplingEvaluationIntent::QuenchStep)
            | (SamplingWorkflowIntent::BasinHopping, SamplingEvaluationIntent::Relaxation)
            | (SamplingWorkflowIntent::SolidSolutions, SamplingEvaluationIntent::QuenchStep)
            | (SamplingWorkflowIntent::SolidSolutions, SamplingEvaluationIntent::Relaxation)
            | (SamplingWorkflowIntent::ScanSurface, SamplingEvaluationIntent::QuenchStep)
            | (SamplingWorkflowIntent::ScanSurface, SamplingEvaluationIntent::Relaxation)
            | (
                SamplingWorkflowIntent::SurfaceReconstruction,
                SamplingEvaluationIntent::QuenchStep,
            )
            | (
                SamplingWorkflowIntent::SurfaceReconstruction,
                SamplingEvaluationIntent::Relaxation,
            ) => {}
        }
        Ok(())
    }
}

pub trait SamplingStartPort {
    fn load_starts(&self, source_run_dir: &Path, top_n: usize) -> Result<Vec<EnergyLidStart>>;
}

pub trait SamplingEvaluationPort {
    fn evaluate(&self, request: &SamplingEvaluationRequest) -> Result<EvalResult>;
}

pub trait BasinHoppingEvaluationLayoutPort {
    fn initial_eval_dir(&self, walker_id: usize) -> PathBuf;

    fn step_eval_dir(&self, walker_id: usize, step_index: usize) -> PathBuf;
}

#[derive(Debug, Clone)]
// This request is a port contract: some adapters compare only energies while
// others inspect the full current/candidate pair and step metadata.
#[allow(dead_code)]
pub struct BasinHoppingIdentityRequest {
    pub walker_id: usize,
    pub step_index: usize,
    pub current_candidate: Candidate,
    pub current_result: EvalResult,
    pub candidate: Candidate,
    pub result: EvalResult,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasinHoppingMatchedRelaxationComparison {
    pub matched_relaxation_level: usize,
    pub current_energy: f64,
    pub candidate_energy: f64,
}

pub trait BasinHoppingIdentityPort {
    fn compare_matched_relaxation(
        &self,
        request: &BasinHoppingIdentityRequest,
    ) -> Result<Option<BasinHoppingMatchedRelaxationComparison>>;
}

pub trait SamplingArtifactSink {
    fn persist_energy_lid_run(&self, execution: &EnergyLidWorkflowExecution) -> Result<()>;

    fn persist_simulated_annealing_run(
        &self,
        execution: &SimulatedAnnealingWorkflowExecution,
    ) -> Result<()>;
}

pub trait BasinHoppingArtifactSink {
    fn persist_basin_hopping_run(&self, execution: &BasinHoppingWorkflowExecution) -> Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct SolidSolutionsLibraryRequest;

pub trait SolidSolutionsLibraryPort {
    fn load_imported_hashkeys(&self, request: &SolidSolutionsLibraryRequest)
        -> Result<Vec<String>>;
}

pub trait SolidSolutionsIdentityPort {
    fn build_hashkey(&self, candidate: &Candidate) -> Result<Option<String>>;
}

pub trait SolidSolutionsGeometryPort {
    fn validate_geometry(&self, candidate: &Candidate) -> Result<()>;
}

pub trait SolidSolutionsArtifactSink {
    fn persist_solid_solutions_run(
        &self,
        execution: &SolidSolutionsWorkflowExecution,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidSolutionsMoveMode {
    MixSolution,
    RandomizeSolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidSolutionsAcceptanceMode {
    Metropolis,
    DownhillOnly,
    RecordAll,
}

#[derive(Debug, Clone)]
pub struct SolidSolutionsMoveRequest {
    pub move_mode: SolidSolutionsMoveMode,
    pub step_index: usize,
    pub current_candidate: Candidate,
    pub initial_candidate: Candidate,
    pub max_exchanges: usize,
}

pub trait SolidSolutionsMovePort {
    fn propose_candidate(&self, request: &SolidSolutionsMoveRequest) -> Result<Candidate>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanSurfaceMoveMode {
    RandomizedLocation,
    TranslateCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanSurfaceAcceptanceMode {
    Metropolis,
    DownhillOnly,
}

#[derive(Debug, Clone)]
pub struct ScanSurfaceMoveRequest {
    pub move_mode: ScanSurfaceMoveMode,
    pub scan_index: usize,
    pub step_index: usize,
    pub current_candidate: Candidate,
    pub seed_candidate: Candidate,
    pub step_size: f64,
}

pub trait ScanSurfaceMovePort {
    fn propose_candidate(&self, request: &ScanSurfaceMoveRequest) -> Result<Candidate>;
}

pub trait ScanSurfaceArtifactSink {
    fn persist_scan_surface_run(&self, execution: &ScanSurfaceWorkflowExecution) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::{SamplingEvaluationIntent, SamplingEvaluationRequest, SamplingWorkflowIntent};
    use patina_types::Candidate;
    use std::path::PathBuf;

    fn candidate() -> Candidate {
        Candidate::cluster("sample", vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    #[test]
    fn energy_lid_quench_request_requires_lid_and_runner_metadata() {
        let request = SamplingEvaluationRequest {
            candidate: candidate(),
            eval_dir: PathBuf::from("/tmp/energy_lid"),
            workflow: SamplingWorkflowIntent::EnergyLid,
            intent: SamplingEvaluationIntent::QuenchStep,
            step_index: Some(3),
            lid_index: Some(2),
            runner_index: Some(1),
        };

        request.validate_shape().expect("valid energy-lid quench");
    }

    #[test]
    fn simulated_annealing_sampling_step_rejects_runner_metadata() {
        let request = SamplingEvaluationRequest {
            candidate: candidate(),
            eval_dir: PathBuf::from("/tmp/anneal"),
            workflow: SamplingWorkflowIntent::SimulatedAnnealing,
            intent: SamplingEvaluationIntent::SamplingStep,
            step_index: Some(4),
            lid_index: None,
            runner_index: Some(0),
        };

        assert!(request.validate_shape().is_err());
    }
}
