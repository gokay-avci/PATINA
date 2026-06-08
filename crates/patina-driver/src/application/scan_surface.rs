use anyhow::{anyhow, Result};
use patina_search::{
    accept_energy_transition, MonteCarloAcceptance, MonteCarloKernelState, SamplingSchedule,
    WorkflowFamily, WorkflowLineage,
};
use patina_types::{Candidate, EvalResult, EvaluationRecord, SamplingWalkerState, StructureRecord};
use serde::Serialize;
use std::path::PathBuf;

use super::ports::{
    SamplingEvaluationIntent, SamplingEvaluationPort, SamplingEvaluationRequest,
    SamplingWorkflowIntent, ScanSurfaceAcceptanceMode, ScanSurfaceArtifactSink,
    ScanSurfaceMoveMode, ScanSurfaceMovePort, ScanSurfaceMoveRequest,
};
use super::workflow_tasks::queue_single_sampling_task;

#[derive(Debug, Clone)]
pub struct ScanSurfaceWorkflowRequest {
    pub initial_candidate: Candidate,
    pub restart_candidates: Vec<Candidate>,
    pub workdir: PathBuf,
    pub steps_per_scan: usize,
    pub temperature: f64,
    pub base_step_size: f64,
    pub dynamic_threshold: usize,
    pub move_mode: ScanSurfaceMoveMode,
    pub acceptance_mode: ScanSurfaceAcceptanceMode,
    pub evaluate_initial_state: bool,
    pub seed: u64,
}

impl ScanSurfaceWorkflowRequest {
    pub fn validate(&self) -> Result<()> {
        validate_candidate(&self.initial_candidate, "scan-surface initial candidate")?;
        if self.workdir.as_os_str().is_empty() {
            return Err(anyhow!("scan-surface workdir must not be empty"));
        }
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err(anyhow!(
                "scan-surface temperature must be finite and non-negative"
            ));
        }
        if !self.base_step_size.is_finite() || self.base_step_size <= 0.0 {
            return Err(anyhow!(
                "scan-surface base_step_size must be finite and positive"
            ));
        }
        for (index, candidate) in self.restart_candidates.iter().enumerate() {
            validate_candidate(
                candidate,
                &format!("scan-surface restart candidate {index}"),
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScanSurfaceMoveConfig {
    pub center: [f64; 3],
    pub boundary: [f64; 3],
    pub above_surface: bool,
}

impl ScanSurfaceMoveConfig {
    pub fn validate(&self) -> Result<()> {
        for (axis, value) in self.center.iter().enumerate() {
            if !value.is_finite() {
                return Err(anyhow!(
                    "scan-surface center axis {axis} must be finite; got {value}"
                ));
            }
        }
        for (axis, value) in self.boundary.iter().enumerate() {
            if !value.is_finite() || *value <= 0.0 {
                return Err(anyhow!(
                    "scan-surface boundary axis {axis} must be finite and positive; got {value}"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanSurfaceMoveDraws {
    pub rotation: [f64; 2],
    pub displacement: [f64; 3],
}

impl ScanSurfaceMoveDraws {
    pub fn validate(&self) -> Result<()> {
        for (index, draw) in self.rotation.iter().enumerate() {
            validate_unit_draw(*draw, &format!("scan-surface rotation draw {index}"))?;
        }
        for (axis, draw) in self.displacement.iter().enumerate() {
            validate_unit_draw(
                *draw,
                &format!("scan-surface displacement draw axis {axis}"),
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanSurfaceStepDecision {
    Accepted,
    RejectedAcceptance,
    RejectedEvaluation,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSurfaceStepTrace {
    pub scan_index: usize,
    pub step_index: usize,
    pub move_mode: ScanSurfaceMoveMode,
    pub step_size: f64,
    pub source_label: String,
    pub candidate_label: String,
    pub decision: ScanSurfaceStepDecision,
    pub energy: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSurfaceScanExecution {
    pub scan_index: usize,
    pub seed_source: StructureRecord,
    pub initial_evaluation: Option<EvaluationRecord>,
    pub state: SamplingWalkerState,
    pub trace: Vec<ScanSurfaceStepTrace>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSurfaceRunSummary {
    pub scan_count: usize,
    pub steps_per_scan: usize,
    pub move_mode: ScanSurfaceMoveMode,
    pub acceptance_mode: ScanSurfaceAcceptanceMode,
    pub evaluate_initial_state: bool,
    pub accepted_steps: usize,
    pub rejected_acceptance_steps: usize,
    pub rejected_evaluation_steps: usize,
    pub best_energy: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSurfaceWorkflowExecution {
    pub summary: ScanSurfaceRunSummary,
    pub scans: Vec<ScanSurfaceScanExecution>,
}

pub struct ScanSurfaceWorkflowService;

impl ScanSurfaceWorkflowService {
    pub fn execute(
        &self,
        request: &ScanSurfaceWorkflowRequest,
        move_port: &dyn ScanSurfaceMovePort,
        evaluation_port: &dyn SamplingEvaluationPort,
        artifact_sink: &dyn ScanSurfaceArtifactSink,
    ) -> Result<ScanSurfaceWorkflowExecution> {
        let execution = execute_scan_surface_workflow(request, move_port, evaluation_port)?;
        artifact_sink.persist_scan_surface_run(&execution)?;
        Ok(execution)
    }
}

fn execute_scan_surface_workflow(
    request: &ScanSurfaceWorkflowRequest,
    move_port: &dyn ScanSurfaceMovePort,
    evaluation_port: &dyn SamplingEvaluationPort,
) -> Result<ScanSurfaceWorkflowExecution> {
    request.validate()?;

    let seeds = if request.restart_candidates.is_empty() {
        vec![request.initial_candidate.clone()]
    } else {
        request.restart_candidates.clone()
    };

    let mut scans = Vec::with_capacity(seeds.len());
    let mut accepted_steps = 0;
    let mut rejected_acceptance_steps = 0;
    let mut rejected_evaluation_steps = 0;
    let mut best_energy = None;

    for (scan_index, seed_candidate) in seeds.into_iter().enumerate() {
        let mut rng = ScanSurfaceRng::new(request.seed ^ ((scan_index as u64) << 32));
        let mut kernel = MonteCarloKernelState::new(
            WorkflowFamily::ScanSurface,
            SamplingSchedule::FixedTemperature {
                temperature: request.temperature,
            },
        );
        let mut current_candidate = seed_candidate.clone();
        let mut current_evaluation = None;
        let mut initial_evaluation = None;
        let mut trace = Vec::with_capacity(request.steps_per_scan);
        let mut consecutive_failures = 0usize;
        let mut step_size = request.base_step_size;

        if request.evaluate_initial_state {
            let result = evaluation_port.evaluate(&SamplingEvaluationRequest {
                candidate: current_candidate.clone(),
                eval_dir: request
                    .workdir
                    .join(format!("scan_{scan_index:04}"))
                    .join("initial"),
                workflow: SamplingWorkflowIntent::ScanSurface,
                intent: SamplingEvaluationIntent::InitialState,
                step_index: None,
                lid_index: None,
                runner_index: None,
            })?;
            best_energy = Some(update_best_energy(best_energy, result.energy));
            kernel.seed_current(
                current_candidate.clone(),
                result.clone(),
                WorkflowLineage::seed(seed_candidate.label.clone()),
            );
            current_candidate = result.relaxed_candidate.clone();
            current_evaluation = Some(result.clone());
            initial_evaluation = Some(EvaluationRecord::from(&result));
        }

        for step_index in 0..request.steps_per_scan {
            if consecutive_failures > request.dynamic_threshold {
                consecutive_failures = 0;
                step_size += request.base_step_size;
            }

            let proposal = move_port.propose_candidate(&ScanSurfaceMoveRequest {
                move_mode: request.move_mode,
                scan_index,
                step_index,
                current_candidate: current_candidate.clone(),
                seed_candidate: seed_candidate.clone(),
                step_size,
            })?;
            let lineage = WorkflowLineage::seed(seed_candidate.label.clone()).with_step(step_index);
            let task = queue_single_sampling_task(
                &mut kernel,
                proposal.clone(),
                lineage,
                "scan-surface sampling step",
            )?;
            let source_label = task.candidate.label.clone();

            let result = match evaluation_port.evaluate(&SamplingEvaluationRequest {
                candidate: proposal.clone(),
                eval_dir: request
                    .workdir
                    .join(format!("scan_{scan_index:04}"))
                    .join(format!("step_{step_index:04}")),
                workflow: SamplingWorkflowIntent::ScanSurface,
                intent: SamplingEvaluationIntent::SamplingStep,
                step_index: Some(step_index),
                lid_index: None,
                runner_index: None,
            }) {
                Ok(result) => result,
                Err(error) => {
                    rejected_evaluation_steps += 1;
                    consecutive_failures += 1;
                    kernel.record_rejection(&task, format!("evaluation failed: {error}"));
                    trace.push(ScanSurfaceStepTrace {
                        scan_index,
                        step_index,
                        move_mode: request.move_mode,
                        step_size,
                        source_label: current_candidate.label.clone(),
                        candidate_label: proposal.label.clone(),
                        decision: ScanSurfaceStepDecision::RejectedEvaluation,
                        energy: None,
                        reason: Some(error.to_string()),
                    });
                    continue;
                }
            };

            if result.energy < best_energy.unwrap_or(f64::INFINITY) {
                best_energy = Some(result.energy);
                step_size = request.base_step_size;
                consecutive_failures = 0;
            }

            let accept = accept_energy_transition(
                current_evaluation
                    .as_ref()
                    .map(|evaluation: &EvalResult| evaluation.energy),
                result.energy,
                scan_surface_acceptance_rule(request.acceptance_mode, request.temperature),
                rng.next_f64(),
            )
            .accepted;

            if accept {
                accepted_steps += 1;
                kernel.record_accept(task, result.clone());
                current_candidate = result.relaxed_candidate.clone();
                current_evaluation = Some(result.clone());
                trace.push(ScanSurfaceStepTrace {
                    scan_index,
                    step_index,
                    move_mode: request.move_mode,
                    step_size,
                    source_label,
                    candidate_label: result.relaxed_candidate.label.clone(),
                    decision: ScanSurfaceStepDecision::Accepted,
                    energy: Some(result.energy),
                    reason: None,
                });
            } else {
                rejected_acceptance_steps += 1;
                consecutive_failures += 1;
                kernel.record_rejection(&task, "rejected by scan-surface acceptance policy");
                trace.push(ScanSurfaceStepTrace {
                    scan_index,
                    step_index,
                    move_mode: request.move_mode,
                    step_size,
                    source_label: current_candidate.label.clone(),
                    candidate_label: proposal.label.clone(),
                    decision: ScanSurfaceStepDecision::RejectedAcceptance,
                    energy: Some(result.energy),
                    reason: Some("rejected by scan-surface acceptance policy".into()),
                });
            }
        }

        scans.push(ScanSurfaceScanExecution {
            scan_index,
            seed_source: StructureRecord::from(&seed_candidate),
            initial_evaluation,
            state: kernel.snapshot_state(),
            trace,
        });
    }

    Ok(ScanSurfaceWorkflowExecution {
        summary: ScanSurfaceRunSummary {
            scan_count: scans.len(),
            steps_per_scan: request.steps_per_scan,
            move_mode: request.move_mode,
            acceptance_mode: request.acceptance_mode,
            evaluate_initial_state: request.evaluate_initial_state,
            accepted_steps,
            rejected_acceptance_steps,
            rejected_evaluation_steps,
            best_energy,
        },
        scans,
    })
}

fn validate_candidate(candidate: &Candidate, context: &str) -> Result<()> {
    candidate
        .validate()
        .map_err(|error| anyhow!("invalid {context} `{}`: {error:?}", candidate.label))?;
    Ok(())
}

fn update_best_energy(current: Option<f64>, energy: f64) -> f64 {
    current.map(|best| best.min(energy)).unwrap_or(energy)
}

fn scan_surface_acceptance_rule(
    mode: ScanSurfaceAcceptanceMode,
    temperature: f64,
) -> MonteCarloAcceptance {
    match mode {
        ScanSurfaceAcceptanceMode::Metropolis => MonteCarloAcceptance::Metropolis {
            temperature: temperature.max(1.0e-12),
        },
        ScanSurfaceAcceptanceMode::DownhillOnly => MonteCarloAcceptance::Quench,
    }
}

pub fn propose_scan_surface_candidate(
    request: &ScanSurfaceMoveRequest,
    config: ScanSurfaceMoveConfig,
    draws: ScanSurfaceMoveDraws,
) -> Result<Candidate> {
    config.validate()?;
    draws.validate()?;

    let mut candidate = match request.move_mode {
        ScanSurfaceMoveMode::RandomizedLocation => {
            let rotated = rotated_scan_surface_candidate(&request.seed_candidate, draws.rotation)?;
            randomized_location_candidate(&rotated, config, draws.displacement)?
        }
        ScanSurfaceMoveMode::TranslateCluster => translated_surface_candidate(
            &request.current_candidate,
            request.step_size,
            config,
            draws.displacement,
        )?,
    };
    candidate.label = format!(
        "scan_{:04}_step_{:04}_{}",
        request.scan_index,
        request.step_index,
        match request.move_mode {
            ScanSurfaceMoveMode::RandomizedLocation => "randomized",
            ScanSurfaceMoveMode::TranslateCluster => "translated",
        }
    );
    validate_candidate(&candidate, "scan-surface proposal")?;
    Ok(candidate)
}

fn validate_unit_draw(draw: f64, context: &str) -> Result<()> {
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(anyhow!("{context} must be in [0, 1); got {draw}"));
    }
    Ok(())
}

fn rotated_scan_surface_candidate(source: &Candidate, draws: [f64; 2]) -> Result<Candidate> {
    let Some(first) = source.fractional_coords.first() else {
        return Err(anyhow!(
            "scan-surface source candidate must contain at least one atom"
        ));
    };

    let mut mins = *first;
    let mut maxs = *first;
    for coord in &source.fractional_coords {
        for axis in 0..3 {
            mins[axis] = mins[axis].min(coord[axis]);
            maxs[axis] = maxs[axis].max(coord[axis]);
        }
    }
    let center = [
        0.5 * (mins[0] + maxs[0]),
        0.5 * (mins[1] + maxs[1]),
        0.5 * (mins[2] + maxs[2]),
    ];
    let phi = std::f64::consts::TAU * draws[0];
    let eta = std::f64::consts::TAU * draws[1];
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();
    let cos_eta = eta.cos();
    let sin_eta = eta.sin();

    let mut candidate = source.clone();
    for coord in &mut candidate.fractional_coords {
        let x_old = coord[0] - center[0];
        let y_old = coord[1] - center[1];
        let z_old = coord[2] - center[2];
        let xy_new = sin_eta * x_old + cos_eta * y_old;
        coord[0] = cos_eta * x_old - sin_eta * y_old + center[0];
        coord[1] = cos_phi * xy_new - sin_phi * z_old + center[1];
        coord[2] = sin_phi * xy_new + cos_phi * z_old + center[2];
    }
    Ok(candidate)
}

fn translated_surface_candidate(
    source: &Candidate,
    step_size: f64,
    config: ScanSurfaceMoveConfig,
    draws: [f64; 3],
) -> Result<Candidate> {
    if !step_size.is_finite() || step_size <= 0.0 {
        return Err(anyhow!(
            "scan-surface translation step_size must be finite and positive; got {step_size}"
        ));
    }

    let mut shift = [
        2.0 * step_size * (draws[0] - 0.5),
        2.0 * step_size * (draws[1] - 0.5),
        2.0 * step_size * (draws[2] - 0.5),
    ];
    if config.above_surface {
        let z_min = source
            .fractional_coords
            .iter()
            .map(|coord| coord[2])
            .fold(f64::INFINITY, f64::min);
        let z_edge = config.center[2] - config.boundary[2];
        if z_min + shift[2] < z_edge {
            shift[2] = z_edge - z_min;
        }
    }

    let mut candidate = source.clone();
    for coord in &mut candidate.fractional_coords {
        coord[0] += shift[0];
        coord[1] += shift[1];
        coord[2] += shift[2];
    }
    Ok(candidate)
}

fn randomized_location_candidate(
    source: &Candidate,
    config: ScanSurfaceMoveConfig,
    draws: [f64; 3],
) -> Result<Candidate> {
    let Some(first) = source.fractional_coords.first() else {
        return Err(anyhow!(
            "scan-surface source candidate must contain at least one atom"
        ));
    };
    let mut mins = *first;
    let mut maxs = *first;
    for coord in &source.fractional_coords {
        for axis in 0..3 {
            mins[axis] = mins[axis].min(coord[axis]);
            maxs[axis] = maxs[axis].max(coord[axis]);
        }
    }

    let x_ref = config.center[0] - 0.5 * (maxs[0] + mins[0]);
    let y_ref = config.center[1] - 0.5 * (maxs[1] + mins[1]);
    let z_ref = if config.above_surface {
        config.center[2] - config.boundary[2] - mins[2]
    } else {
        config.center[2] - 0.5 * (maxs[2] + mins[2])
    };
    let available = [
        2.0 * config.boundary[0] - (maxs[0] - mins[0]).abs(),
        2.0 * config.boundary[1] - (maxs[1] - mins[1]).abs(),
        2.0 * config.boundary[2] - (maxs[2] - mins[2]).abs(),
    ];
    let shift = [
        x_ref + random_box_displacement(available[0], draws[0], false),
        y_ref + random_box_displacement(available[1], draws[1], false),
        z_ref + random_box_displacement(available[2], draws[2], config.above_surface),
    ];

    let mut candidate = source.clone();
    for coord in &mut candidate.fractional_coords {
        coord[0] += shift[0];
        coord[1] += shift[1];
        coord[2] += shift[2];
    }
    Ok(candidate)
}

fn random_box_displacement(available_span: f64, draw: f64, one_sided: bool) -> f64 {
    if available_span <= 0.0 {
        return 0.0;
    }
    if one_sided {
        available_span * draw
    } else {
        available_span * (draw - 0.5)
    }
}

#[derive(Debug, Clone)]
struct ScanSurfaceRng {
    state: u64,
}

impl ScanSurfaceRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let bits = self.state >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        propose_scan_surface_candidate, ScanSurfaceMoveConfig, ScanSurfaceMoveDraws,
        ScanSurfaceStepDecision, ScanSurfaceWorkflowExecution, ScanSurfaceWorkflowRequest,
        ScanSurfaceWorkflowService,
    };
    use crate::application::ports::{
        SamplingEvaluationIntent, SamplingEvaluationPort, SamplingEvaluationRequest,
        SamplingWorkflowIntent, ScanSurfaceAcceptanceMode, ScanSurfaceArtifactSink,
        ScanSurfaceMoveMode, ScanSurfaceMovePort, ScanSurfaceMoveRequest,
    };
    use anyhow::Result;
    use patina_types::{Candidate, EvalResult};
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};
    use std::path::PathBuf;
    use std::time::Duration;

    fn candidate(label: &str) -> Candidate {
        Candidate {
            species: vec!["Zn".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.25, 0.25, 0.25]],
            lattice: Some([[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 20.0]]),
            periodic_axes: [true, true, false],
            label: label.into(),
        }
    }

    fn result(label: &str, energy: f64) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; 2],
            relaxed_candidate: candidate(label),
            converged: true,
            wall_time: Duration::from_secs(0),
        }
    }

    #[test]
    fn randomized_location_rotates_seed_before_boxing() {
        let mut seed = candidate("seed");
        seed.fractional_coords = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let request = ScanSurfaceMoveRequest {
            move_mode: ScanSurfaceMoveMode::RandomizedLocation,
            scan_index: 0,
            step_index: 0,
            current_candidate: seed.clone(),
            seed_candidate: seed,
            step_size: 1.0,
        };

        let proposal = propose_scan_surface_candidate(
            &request,
            ScanSurfaceMoveConfig {
                center: [0.0, 0.0, 0.0],
                boundary: [10.0, 10.0, 10.0],
                above_surface: false,
            },
            ScanSurfaceMoveDraws {
                rotation: [0.25, 0.0],
                displacement: [0.5, 0.5, 0.5],
            },
        )
        .expect("scan-surface randomized location proposal");

        assert!(proposal.fractional_coords[0][1].abs() < 1.0e-12);
        assert!(proposal.fractional_coords[1][1].abs() < 1.0e-12);
        assert!((proposal.fractional_coords[0][2] + 0.5).abs() < 1.0e-12);
        assert!((proposal.fractional_coords[1][2] - 0.5).abs() < 1.0e-12);
    }

    #[derive(Default)]
    struct MovePortStub {
        proposals: RefCell<VecDeque<Candidate>>,
        requests: RefCell<Vec<ScanSurfaceMoveRequest>>,
    }

    impl ScanSurfaceMovePort for MovePortStub {
        fn propose_candidate(&self, request: &ScanSurfaceMoveRequest) -> Result<Candidate> {
            self.requests.borrow_mut().push(request.clone());
            Ok(self
                .proposals
                .borrow_mut()
                .pop_front()
                .expect("proposal configured"))
        }
    }

    #[derive(Default)]
    struct SamplingPortStub {
        requests: RefCell<Vec<SamplingEvaluationRequest>>,
        energies: RefCell<BTreeMap<String, f64>>,
        failures: RefCell<Vec<String>>,
    }

    impl SamplingEvaluationPort for SamplingPortStub {
        fn evaluate(&self, request: &SamplingEvaluationRequest) -> Result<EvalResult> {
            self.requests.borrow_mut().push(request.clone());
            if self
                .failures
                .borrow()
                .iter()
                .any(|label| label == &request.candidate.label)
            {
                anyhow::bail!("backend failed for {}", request.candidate.label);
            }
            let energy = *self
                .energies
                .borrow()
                .get(&request.candidate.label)
                .expect("configured energy");
            Ok(result(
                &format!("{}__relaxed", request.candidate.label),
                energy,
            ))
        }
    }

    #[derive(Default)]
    struct ArtifactSinkStub {
        executions: RefCell<Vec<ScanSurfaceWorkflowExecution>>,
    }

    impl ScanSurfaceArtifactSink for ArtifactSinkStub {
        fn persist_scan_surface_run(&self, execution: &ScanSurfaceWorkflowExecution) -> Result<()> {
            self.executions.borrow_mut().push(execution.clone());
            Ok(())
        }
    }

    #[test]
    fn workflow_routes_scan_surface_sampling_intents() {
        let request = ScanSurfaceWorkflowRequest {
            initial_candidate: candidate("seed"),
            restart_candidates: Vec::new(),
            workdir: PathBuf::from("/tmp/scan_surface"),
            steps_per_scan: 2,
            temperature: 300.0,
            base_step_size: 0.5,
            dynamic_threshold: 2,
            move_mode: ScanSurfaceMoveMode::TranslateCluster,
            acceptance_mode: ScanSurfaceAcceptanceMode::DownhillOnly,
            evaluate_initial_state: true,
            seed: 7,
        };
        let move_port = MovePortStub {
            proposals: RefCell::new(VecDeque::from(vec![
                candidate("proposal-a"),
                candidate("proposal-b"),
            ])),
            requests: RefCell::new(Vec::new()),
        };
        let sampling_port = SamplingPortStub {
            requests: RefCell::new(Vec::new()),
            energies: RefCell::new(BTreeMap::from([
                ("seed".into(), -1.0),
                ("proposal-a".into(), -2.0),
                ("proposal-b".into(), -1.5),
            ])),
            failures: RefCell::new(Vec::new()),
        };
        let artifact_sink = ArtifactSinkStub::default();

        let execution = ScanSurfaceWorkflowService
            .execute(&request, &move_port, &sampling_port, &artifact_sink)
            .expect("workflow executes");

        let eval_requests = sampling_port.requests.borrow();
        assert_eq!(eval_requests.len(), 3);
        assert_eq!(
            eval_requests[0].workflow,
            SamplingWorkflowIntent::ScanSurface
        );
        assert_eq!(
            eval_requests[0].intent,
            SamplingEvaluationIntent::InitialState
        );
        assert_eq!(
            eval_requests[1].intent,
            SamplingEvaluationIntent::SamplingStep
        );
        assert_eq!(
            eval_requests[2].intent,
            SamplingEvaluationIntent::SamplingStep
        );
        let move_requests = move_port.requests.borrow();
        assert_eq!(move_requests.len(), 2);
        assert_eq!(
            move_requests[0].move_mode,
            ScanSurfaceMoveMode::TranslateCluster
        );
        assert_eq!(move_requests[0].scan_index, 0);
        assert_eq!(move_requests[0].step_index, 0);
        assert_eq!(move_requests[0].current_candidate.label, "seed__relaxed");
        assert_eq!(move_requests[0].seed_candidate.label, "seed");
        assert_eq!(execution.summary.accepted_steps, 1);
        assert_eq!(execution.summary.rejected_acceptance_steps, 1);
        assert_eq!(execution.summary.best_energy, Some(-2.0));
        assert_eq!(
            execution.scans[0]
                .initial_evaluation
                .as_ref()
                .expect("initial")
                .energy,
            -1.0
        );
        assert_eq!(execution.scans[0].state.accepted_steps, 1);
        assert_eq!(execution.scans[0].state.rejected_steps, 1);
        assert_eq!(
            execution.scans[0].trace[0].decision,
            ScanSurfaceStepDecision::Accepted
        );
        assert_eq!(
            execution.scans[0].trace[1].decision,
            ScanSurfaceStepDecision::RejectedAcceptance
        );
        assert_eq!(artifact_sink.executions.borrow().len(), 1);
    }

    #[test]
    fn workflow_increases_dynamic_step_size_after_rejections_and_resets_on_new_best() {
        let request = ScanSurfaceWorkflowRequest {
            initial_candidate: candidate("seed"),
            restart_candidates: Vec::new(),
            workdir: PathBuf::from("/tmp/scan_surface"),
            steps_per_scan: 3,
            temperature: 0.01,
            base_step_size: 1.0,
            dynamic_threshold: 0,
            move_mode: ScanSurfaceMoveMode::TranslateCluster,
            acceptance_mode: ScanSurfaceAcceptanceMode::Metropolis,
            evaluate_initial_state: false,
            seed: 11,
        };
        let move_port = MovePortStub {
            proposals: RefCell::new(VecDeque::from(vec![
                candidate("proposal-a"),
                candidate("proposal-b"),
                candidate("proposal-c"),
            ])),
            requests: RefCell::new(Vec::new()),
        };
        let sampling_port = SamplingPortStub {
            requests: RefCell::new(Vec::new()),
            energies: RefCell::new(BTreeMap::from([
                ("proposal-a".into(), 1.0),
                ("proposal-b".into(), -2.0),
                ("proposal-c".into(), -1.0),
            ])),
            failures: RefCell::new(vec!["proposal-a".into()]),
        };
        let artifact_sink = ArtifactSinkStub::default();

        let execution = ScanSurfaceWorkflowService
            .execute(&request, &move_port, &sampling_port, &artifact_sink)
            .expect("workflow executes");

        let move_requests = move_port.requests.borrow();
        assert_eq!(move_requests[0].step_size, 1.0);
        assert_eq!(move_requests[1].step_size, 2.0);
        assert_eq!(move_requests[2].step_size, 1.0);
        assert_eq!(move_requests[1].current_candidate.label, "seed");
        assert_eq!(
            move_requests[2].current_candidate.label,
            "proposal-b__relaxed"
        );
        assert_eq!(execution.summary.rejected_evaluation_steps, 1);
        assert_eq!(execution.summary.accepted_steps, 1);
        assert_eq!(execution.summary.rejected_acceptance_steps, 1);
    }

    #[test]
    fn workflow_supports_multiple_restart_scans() {
        let request = ScanSurfaceWorkflowRequest {
            initial_candidate: candidate("seed"),
            restart_candidates: vec![candidate("restart-a"), candidate("restart-b")],
            workdir: PathBuf::from("/tmp/scan_surface"),
            steps_per_scan: 1,
            temperature: 300.0,
            base_step_size: 0.5,
            dynamic_threshold: 3,
            move_mode: ScanSurfaceMoveMode::RandomizedLocation,
            acceptance_mode: ScanSurfaceAcceptanceMode::Metropolis,
            evaluate_initial_state: false,
            seed: 19,
        };
        let move_port = MovePortStub {
            proposals: RefCell::new(VecDeque::from(vec![
                candidate("proposal-a"),
                candidate("proposal-b"),
            ])),
            requests: RefCell::new(Vec::new()),
        };
        let sampling_port = SamplingPortStub {
            requests: RefCell::new(Vec::new()),
            energies: RefCell::new(BTreeMap::from([
                ("proposal-a".into(), -1.0),
                ("proposal-b".into(), -2.0),
            ])),
            failures: RefCell::new(Vec::new()),
        };
        let artifact_sink = ArtifactSinkStub::default();

        let execution = ScanSurfaceWorkflowService
            .execute(&request, &move_port, &sampling_port, &artifact_sink)
            .expect("workflow executes");

        assert_eq!(execution.summary.scan_count, 2);
        assert_eq!(execution.scans.len(), 2);
        assert_eq!(execution.scans[0].seed_source.label, "restart-a");
        assert_eq!(execution.scans[1].seed_source.label, "restart-b");
    }
}
