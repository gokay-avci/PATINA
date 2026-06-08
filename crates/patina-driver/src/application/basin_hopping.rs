use anyhow::{anyhow, Result};
use patina_search::{
    BasinHopping, BhEnergyComparison, BhMethod, BhMoveClassPolicy, BhStepControlState, BhStepTrace,
    MonteCarloAcceptance, MonteCarloKernelState, SamplingSchedule, SearchAlgorithm, WorkflowFamily,
    WorkflowLineage,
};
use patina_types::{
    BhAcceptanceRuleRecord, BhMethodRecord, BhMoveRegimeRecord, BhScientificConfig,
    BhWalkerScientificState, BhWalkerState, Candidate, MoveClassActivationRecord, Population,
    SearchConfig,
};
use serde::Serialize;

use super::bh_continuity;
use super::ports::{
    BasinHoppingArtifactSink, BasinHoppingEvaluationLayoutPort, BasinHoppingIdentityPort,
    BasinHoppingIdentityRequest, BasinHoppingMatchedRelaxationComparison, SamplingEvaluationIntent,
    SamplingEvaluationPort, SamplingEvaluationRequest, SamplingWorkflowIntent,
};
use super::workflow_tasks::queue_single_sampling_task;

#[derive(Debug, Clone)]
pub struct BasinHoppingWorkflowRequest {
    pub base_candidate: Candidate,
    pub scientific_config: BhScientificConfig,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
}

#[derive(Debug, Clone)]
pub struct BasinHoppingSummaryContext {
    pub system: String,
    pub backend_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BasinHoppingRunSummary {
    pub system: String,
    pub backend: String,
    pub bh_steps: usize,
    pub walker_count: usize,
    pub temperature: f64,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub scientific_config: BhScientificConfig,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
    pub best_energy: Option<f64>,
    pub best_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BasinHoppingWalkerExecution {
    pub walker_state: BhWalkerState,
    pub kernel_state: MonteCarloKernelState,
}

#[derive(Debug, Clone)]
pub struct BasinHoppingWorkflowExecution {
    pub summary: BasinHoppingRunSummary,
    pub trace: Vec<BhStepTrace>,
    pub walkers: Vec<BasinHoppingWalkerExecution>,
}

pub struct BasinHoppingWorkflowService;

pub fn default_bh_scientific_config(
    max_steps: usize,
    walkers: usize,
    temperature: f64,
    base_step_size: f64,
) -> BhScientificConfig {
    let move_policy = BhMoveClassPolicy::default();
    BhScientificConfig {
        max_steps,
        walkers: walkers.max(1),
        base_step_size,
        acceptance_rule: BhAcceptanceRuleRecord::Metropolis { temperature },
        method: BhMethodRecord::Relax,
        dynamic_threshold: move_policy.dynamic_step_threshold,
        moveclass_threshold: move_policy.moveclass_threshold,
        max_dynamic_step_multiplier: move_policy.max_dynamic_step_multiplier,
        prob_switch_cations: move_policy.prob_swap_cations,
        prob_switch_atoms: move_policy.prob_swap_atoms,
        prob_mutate_cluster: move_policy.prob_mutate_cluster,
        prob_twist_cluster: move_policy.prob_twist_cluster,
        prob_translate_cluster: move_policy.prob_translate_surface,
        prob_rotate_cluster: move_policy.prob_rotate_surface,
    }
}

impl BasinHoppingWorkflowService {
    pub fn execute(
        &self,
        request: &BasinHoppingWorkflowRequest,
        summary_context: &BasinHoppingSummaryContext,
        evaluation_port: &dyn SamplingEvaluationPort,
        layout_port: &dyn BasinHoppingEvaluationLayoutPort,
        identity_port: &dyn BasinHoppingIdentityPort,
        artifact_sink: &dyn BasinHoppingArtifactSink,
    ) -> Result<BasinHoppingWorkflowExecution> {
        let execution = execute_basin_hopping_workflow(
            request,
            summary_context,
            evaluation_port,
            layout_port,
            identity_port,
        )?;
        artifact_sink.persist_basin_hopping_run(&execution)?;
        Ok(execution)
    }
}

fn execute_basin_hopping_workflow(
    request: &BasinHoppingWorkflowRequest,
    summary_context: &BasinHoppingSummaryContext,
    evaluation_port: &dyn SamplingEvaluationPort,
    layout_port: &dyn BasinHoppingEvaluationLayoutPort,
    identity_port: &dyn BasinHoppingIdentityPort,
) -> Result<BasinHoppingWorkflowExecution> {
    let mut controller = build_basin_hopping_controller(request);
    let cfg = search_config_from_bh_scientific_config(&request.scientific_config, request.seed);
    let initial_candidates = controller.initialize(&cfg);
    let mut population = Population {
        members: Vec::with_capacity(initial_candidates.len()),
        generation: 0,
    };
    let mut kernel_states = Vec::with_capacity(initial_candidates.len());
    for (walker_id, initial_candidate) in initial_candidates.into_iter().enumerate() {
        let mut candidate = initial_candidate;
        if request.enforce_container {
            apply_cluster_container(&mut candidate, request.boundary);
        }
        let result = evaluation_port.evaluate(&SamplingEvaluationRequest {
            candidate: candidate.clone(),
            eval_dir: layout_port.initial_eval_dir(walker_id),
            workflow: SamplingWorkflowIntent::BasinHopping,
            intent: bh_initial_evaluation_intent(&request.scientific_config.method),
            step_index: None,
            lid_index: None,
            runner_index: None,
        })?;
        population.insert(result.clone(), candidate.clone());
        let mut kernel_state = MonteCarloKernelState::new(
            WorkflowFamily::BasinHopping,
            SamplingSchedule::FixedTemperature {
                temperature: summary_temperature(&request.scientific_config),
            },
        );
        kernel_state.seed_current(
            candidate,
            result,
            WorkflowLineage::seed(request.base_candidate.label.clone()),
        );
        kernel_states.push(kernel_state);
    }

    for step_index in 0..request.scientific_config.max_steps {
        let candidates = controller.next_candidates(&population)?;
        if candidates.is_empty() {
            break;
        }
        let trace_start = controller.trace().len();
        let mut queued_tasks = Vec::with_capacity(candidates.len());
        let mut evaluated = Vec::with_capacity(candidates.len());
        let mut results = Vec::with_capacity(candidates.len());
        let mut comparisons = Vec::with_capacity(candidates.len());
        for (walker_id, mut candidate) in candidates.into_iter().enumerate() {
            let current_state = population.members.get(walker_id).cloned();
            if request.enforce_container {
                apply_cluster_container(&mut candidate, request.boundary);
            }
            let lineage =
                WorkflowLineage::seed(request.base_candidate.label.clone()).with_step(step_index);
            let task = queue_single_sampling_task(
                &mut kernel_states[walker_id],
                candidate.clone(),
                lineage,
                "basin-hopping walker step",
            )?;
            let result = evaluation_port.evaluate(&SamplingEvaluationRequest {
                candidate: candidate.clone(),
                eval_dir: layout_port.step_eval_dir(walker_id, step_index),
                workflow: SamplingWorkflowIntent::BasinHopping,
                intent: bh_step_evaluation_intent(&request.scientific_config.method),
                step_index: Some(step_index),
                lid_index: None,
                runner_index: None,
            })?;
            queued_tasks.push(task);
            results.push(result.clone());
            comparisons.push(build_bh_energy_comparison(
                identity_port,
                walker_id,
                step_index,
                current_state.as_ref(),
                &candidate,
                &result,
            )?);
            evaluated.push((candidate, result));
        }

        controller.update_with_energy_comparisons(&mut population, &evaluated, &comparisons)?;
        let trace_slice = &controller.trace()[trace_start..];
        for (((task, result), trace_row), kernel_state) in queued_tasks
            .into_iter()
            .zip(results)
            .zip(trace_slice.iter())
            .zip(kernel_states.iter_mut())
        {
            if trace_row.accepted {
                kernel_state.record_accept(task, result);
            } else {
                kernel_state.record_rejection(&task, trace_row.reason.clone());
            }
        }
    }

    let best = population.best().cloned();
    let walkers = population
        .members
        .iter()
        .enumerate()
        .zip(kernel_states)
        .map(
            |((walker_id, (result, _candidate)), kernel_state)| BasinHoppingWalkerExecution {
                walker_state: bh_continuity::build_bh_walker_state(
                    bh_continuity::BhWalkerStateInput {
                        walker_id: &walker_id.to_string(),
                        step: request.scientific_config.max_steps,
                        restart: bh_continuity::fresh_bh_restart_state(
                            request.base_candidate.label.clone(),
                        ),
                        current: result,
                        current_output_path: None,
                        best: best
                            .as_ref()
                            .map(|(best_result, _)| best_result)
                            .unwrap_or(result),
                        best_output_path: None,
                        move_class_activation: last_move_class_activation(
                            controller.trace(),
                            walker_id,
                        ),
                        restart_equivalence: None,
                        scientific_state: Some(build_bh_walker_scientific_state(
                            &controller,
                            controller.trace(),
                            walker_id,
                        )),
                    },
                ),
                kernel_state,
            },
        )
        .collect::<Vec<_>>();

    if walkers.is_empty() {
        return Err(anyhow!("basin-hopping population became empty"));
    }

    let accepted_steps = walkers
        .iter()
        .map(|walker| walker.kernel_state.accepted_steps)
        .sum();
    let rejected_steps = walkers
        .iter()
        .map(|walker| walker.kernel_state.rejected_steps)
        .sum();

    Ok(BasinHoppingWorkflowExecution {
        summary: BasinHoppingRunSummary {
            system: summary_context.system.clone(),
            backend: summary_context.backend_label.clone(),
            bh_steps: request.scientific_config.max_steps,
            walker_count: walkers.len(),
            temperature: summary_temperature(&request.scientific_config),
            step_size: request.scientific_config.base_step_size,
            enforce_container: request.enforce_container,
            boundary: request.boundary,
            scientific_config: request.scientific_config.clone(),
            accepted_steps,
            rejected_steps,
            best_energy: best.as_ref().map(|(result, _)| result.energy),
            best_label: best
                .as_ref()
                .map(|(result, _)| result.relaxed_candidate.label.clone()),
        },
        trace: controller.trace().to_vec(),
        walkers,
    })
}

fn last_move_class_activation(
    trace: &[BhStepTrace],
    walker_id: usize,
) -> Option<MoveClassActivationRecord> {
    trace
        .iter()
        .rev()
        .find(|row| row.walker_id == walker_id)
        .map(|row| MoveClassActivationRecord {
            step: row.step,
            move_class: bh_continuity::bh_move_class_record(row.move_class),
            step_size: row.step_size,
            accepted: row.accepted,
            energy: row.energy,
        })
}

fn build_basin_hopping_controller(request: &BasinHoppingWorkflowRequest) -> BasinHopping {
    BasinHopping::with_acceptance(
        request.base_candidate.clone(),
        request.seed,
        map_bh_acceptance_rule_record(&request.scientific_config.acceptance_rule),
    )
    .with_method(map_bh_method_record(&request.scientific_config.method))
    .with_move_policy(build_bh_move_policy(&request.scientific_config))
}

fn search_config_from_bh_scientific_config(config: &BhScientificConfig, seed: u64) -> SearchConfig {
    SearchConfig {
        temperature: summary_temperature(config),
        step_size: config.base_step_size,
        population_size: config.walkers.max(1),
        max_steps: config.max_steps.max(1),
        seed: Some(seed),
    }
}

fn build_bh_walker_scientific_state(
    controller: &BasinHopping,
    trace: &[BhStepTrace],
    walker_id: usize,
) -> BhWalkerScientificState {
    BhWalkerScientificState {
        controller_step: controller.current_step(),
        active_acceptance_rule: map_bh_acceptance_rule(controller.active_acceptance()),
        move_regime: build_bh_move_regime_record(controller.move_regime()),
        last_matched_relaxation_level: trace
            .iter()
            .rev()
            .find(|row| row.walker_id == walker_id)
            .and_then(|row| row.matched_relaxation_level),
    }
}

fn build_bh_energy_comparison(
    identity_port: &dyn BasinHoppingIdentityPort,
    walker_id: usize,
    step_index: usize,
    current_state: Option<&(patina_types::EvalResult, patina_types::Candidate)>,
    candidate: &Candidate,
    result: &patina_types::EvalResult,
) -> Result<BhEnergyComparison> {
    let current_energy = current_state.map(|(current_result, _)| current_result.energy);
    let Some((current_result, current_candidate)) = current_state else {
        return Ok(BhEnergyComparison::from_final_energies(
            current_energy,
            result.energy,
        ));
    };
    let matched = identity_port.compare_matched_relaxation(&BasinHoppingIdentityRequest {
        walker_id,
        step_index,
        current_candidate: current_candidate.clone(),
        current_result: current_result.clone(),
        candidate: candidate.clone(),
        result: result.clone(),
    })?;
    Ok(map_bh_energy_comparison(
        matched,
        current_energy,
        result.energy,
    ))
}

fn map_bh_energy_comparison(
    matched: Option<BasinHoppingMatchedRelaxationComparison>,
    current_energy: Option<f64>,
    candidate_energy: f64,
) -> BhEnergyComparison {
    match matched {
        Some(matched) => BhEnergyComparison {
            current_energy: Some(matched.current_energy),
            candidate_energy: matched.candidate_energy,
            matched_relaxation_level: Some(matched.matched_relaxation_level),
        },
        None => BhEnergyComparison::from_final_energies(current_energy, candidate_energy),
    }
}

fn bh_initial_evaluation_intent(method: &BhMethodRecord) -> SamplingEvaluationIntent {
    if matches!(method, BhMethodRecord::Fixed) {
        SamplingEvaluationIntent::InitialState
    } else {
        SamplingEvaluationIntent::Relaxation
    }
}

fn bh_step_evaluation_intent(method: &BhMethodRecord) -> SamplingEvaluationIntent {
    if matches!(method, BhMethodRecord::Fixed) {
        SamplingEvaluationIntent::SamplingStep
    } else {
        SamplingEvaluationIntent::Relaxation
    }
}

fn build_bh_move_regime_record(move_regime: BhStepControlState) -> BhMoveRegimeRecord {
    BhMoveRegimeRecord {
        consecutive_rejections: move_regime.rejection_counter,
        random_moveclass_enabled: move_regime.random_moveclass,
        active_step_size: move_regime.current_step_size,
    }
}

fn build_bh_move_policy(config: &BhScientificConfig) -> BhMoveClassPolicy {
    BhMoveClassPolicy {
        dynamic_step_threshold: config.dynamic_threshold,
        moveclass_threshold: config.moveclass_threshold,
        max_dynamic_step_multiplier: config.max_dynamic_step_multiplier,
        prob_swap_cations: config.prob_switch_cations,
        enable_after_rejections: config.dynamic_threshold,
        prob_translate_surface: config.prob_translate_cluster,
        prob_rotate_surface: config.prob_rotate_cluster,
        prob_swap_atoms: config.prob_switch_atoms,
        prob_mutate_cluster: config.prob_mutate_cluster,
        prob_twist_cluster: config.prob_twist_cluster,
    }
}

fn map_bh_acceptance_rule_record(acceptance: &BhAcceptanceRuleRecord) -> MonteCarloAcceptance {
    match acceptance {
        BhAcceptanceRuleRecord::Metropolis { temperature } => MonteCarloAcceptance::Metropolis {
            temperature: *temperature,
        },
        BhAcceptanceRuleRecord::Quench => MonteCarloAcceptance::Quench,
        BhAcceptanceRuleRecord::EnergyThreshold { threshold } => {
            MonteCarloAcceptance::EnergyThreshold {
                threshold: *threshold,
            }
        }
    }
}

fn map_bh_acceptance_rule(acceptance: MonteCarloAcceptance) -> BhAcceptanceRuleRecord {
    match acceptance {
        MonteCarloAcceptance::Metropolis { temperature } => {
            BhAcceptanceRuleRecord::Metropolis { temperature }
        }
        MonteCarloAcceptance::Quench => BhAcceptanceRuleRecord::Quench,
        MonteCarloAcceptance::EnergyThreshold { threshold } => {
            BhAcceptanceRuleRecord::EnergyThreshold { threshold }
        }
    }
}

fn map_bh_method_record(method: &BhMethodRecord) -> BhMethod {
    match method {
        BhMethodRecord::Relax => BhMethod::Relax,
        BhMethodRecord::Fixed => BhMethod::Fixed,
        BhMethodRecord::Oscillate {
            high_temperature_steps,
            low_temperature_steps,
        } => BhMethod::Oscillate {
            high_temperature_steps: *high_temperature_steps,
            low_temperature_steps: *low_temperature_steps,
        },
    }
}

fn summary_temperature(config: &BhScientificConfig) -> f64 {
    match config.acceptance_rule {
        BhAcceptanceRuleRecord::Metropolis { temperature } => temperature,
        BhAcceptanceRuleRecord::Quench => 0.0,
        BhAcceptanceRuleRecord::EnergyThreshold { threshold } => threshold,
    }
}

fn apply_cluster_container(candidate: &mut Candidate, boundary: Option<f64>) {
    if !candidate.is_zero_d() {
        return;
    }
    let Some(limit) = boundary.filter(|value| value.is_finite() && *value > 0.0) else {
        return;
    };
    for coord in &mut candidate.fractional_coords {
        coord[0] = coord[0].clamp(-limit, limit);
        coord[1] = coord[1].clamp(-limit, limit);
        coord[2] = coord[2].clamp(-limit, limit);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_bh_scientific_config, BasinHoppingWorkflowRequest, BasinHoppingWorkflowService,
    };
    use crate::application::ports::{
        BasinHoppingArtifactSink, BasinHoppingEvaluationLayoutPort, BasinHoppingIdentityPort,
        BasinHoppingIdentityRequest, BasinHoppingMatchedRelaxationComparison,
        SamplingEvaluationIntent, SamplingEvaluationPort, SamplingEvaluationRequest,
    };
    use patina_types::{BhAcceptanceRuleRecord, BhMethodRecord, Candidate, EvalResult};
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::time::Duration;

    #[derive(Debug, Clone, Copy)]
    struct IdentitySamplingPort;

    impl SamplingEvaluationPort for IdentitySamplingPort {
        fn evaluate(&self, request: &SamplingEvaluationRequest) -> anyhow::Result<EvalResult> {
            let energy = request
                .candidate
                .fractional_coords
                .iter()
                .flat_map(|coord| coord.iter())
                .copied()
                .sum::<f64>();
            Ok(EvalResult {
                energy,
                forces: vec![[0.0, 0.0, 0.0]; request.candidate.len()],
                relaxed_candidate: request.candidate.clone(),
                converged: true,
                wall_time: Duration::from_secs(0),
            })
        }
    }

    struct NullSink;

    struct LayoutStub;

    #[derive(Debug, Clone, Copy)]
    struct NoopIdentityPort;

    #[derive(Debug, Clone, Copy)]
    struct MatchedRelaxationIdentityPort;

    #[derive(Default)]
    struct RecordingSamplingPort {
        intents: RefCell<Vec<SamplingEvaluationIntent>>,
    }

    impl BasinHoppingArtifactSink for NullSink {
        fn persist_basin_hopping_run(
            &self,
            _execution: &super::BasinHoppingWorkflowExecution,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl BasinHoppingEvaluationLayoutPort for LayoutStub {
        fn initial_eval_dir(&self, walker_id: usize) -> PathBuf {
            PathBuf::from(format!("walker_{walker_id:04}/initial"))
        }

        fn step_eval_dir(&self, walker_id: usize, step_index: usize) -> PathBuf {
            PathBuf::from(format!("walker_{walker_id:04}/step_{step_index:04}"))
        }
    }

    impl SamplingEvaluationPort for RecordingSamplingPort {
        fn evaluate(&self, request: &SamplingEvaluationRequest) -> anyhow::Result<EvalResult> {
            self.intents.borrow_mut().push(request.intent);
            IdentitySamplingPort.evaluate(request)
        }
    }

    impl BasinHoppingIdentityPort for NoopIdentityPort {
        fn compare_matched_relaxation(
            &self,
            _request: &BasinHoppingIdentityRequest,
        ) -> anyhow::Result<Option<BasinHoppingMatchedRelaxationComparison>> {
            Ok(None)
        }
    }

    impl BasinHoppingIdentityPort for MatchedRelaxationIdentityPort {
        fn compare_matched_relaxation(
            &self,
            request: &BasinHoppingIdentityRequest,
        ) -> anyhow::Result<Option<BasinHoppingMatchedRelaxationComparison>> {
            Ok(Some(BasinHoppingMatchedRelaxationComparison {
                matched_relaxation_level: 2,
                current_energy: request.current_result.energy,
                candidate_energy: request.current_result.energy - 1.0,
            }))
        }
    }

    fn cluster_candidate(label: &str) -> Candidate {
        Candidate::cluster(label, vec!["Mg".into()], vec![[0.1, 0.2, 0.3]])
    }

    #[test]
    fn basin_hopping_service_tracks_walkers_and_trace() {
        let execution = BasinHoppingWorkflowService
            .execute(
                &BasinHoppingWorkflowRequest {
                    base_candidate: cluster_candidate("seed"),
                    scientific_config: default_bh_scientific_config(3, 2, 10.0, 0.1),
                    enforce_container: false,
                    boundary: None,
                    seed: 7,
                },
                &super::BasinHoppingSummaryContext {
                    system: "MgO".into(),
                    backend_label: "gulp".into(),
                },
                &IdentitySamplingPort,
                &LayoutStub,
                &NoopIdentityPort,
                &NullSink,
            )
            .expect("execute basin hopping");

        assert_eq!(execution.summary.walker_count, 2);
        assert_eq!(execution.walkers.len(), 2);
        assert!(!execution.trace.is_empty());
        assert!(execution.summary.accepted_steps + execution.summary.rejected_steps > 0);
        assert_eq!(execution.summary.scientific_config.max_steps, 3);
        assert_eq!(execution.summary.scientific_config.walkers, 2);
        assert_eq!(execution.summary.scientific_config.base_step_size, 0.1);
        assert_eq!(
            execution.summary.scientific_config.method,
            BhMethodRecord::Relax
        );
        assert_eq!(
            execution.summary.scientific_config.acceptance_rule,
            BhAcceptanceRuleRecord::Metropolis { temperature: 10.0 }
        );
        let scientific_state = execution.walkers[0]
            .walker_state
            .scientific_state
            .as_ref()
            .expect("walker scientific state");
        assert!(execution.walkers[0]
            .walker_state
            .restart_equivalence
            .is_none());
        assert_eq!(scientific_state.controller_step, 3);
        assert_eq!(
            scientific_state.active_acceptance_rule,
            BhAcceptanceRuleRecord::Metropolis { temperature: 10.0 }
        );
        assert!(scientific_state.move_regime.active_step_size.is_finite());
        assert!(scientific_state.move_regime.active_step_size > 0.0);
        assert_eq!(scientific_state.last_matched_relaxation_level, None);
    }

    #[test]
    fn basin_hopping_method_controls_evaluation_intent() {
        let relax_port = RecordingSamplingPort::default();
        BasinHoppingWorkflowService
            .execute(
                &BasinHoppingWorkflowRequest {
                    base_candidate: cluster_candidate("seed"),
                    scientific_config: default_bh_scientific_config(2, 1, 10.0, 0.1),
                    enforce_container: false,
                    boundary: None,
                    seed: 7,
                },
                &super::BasinHoppingSummaryContext {
                    system: "MgO".into(),
                    backend_label: "gulp".into(),
                },
                &relax_port,
                &LayoutStub,
                &NoopIdentityPort,
                &NullSink,
            )
            .expect("execute relaxed basin hopping");
        assert_eq!(
            *relax_port.intents.borrow(),
            vec![
                SamplingEvaluationIntent::Relaxation,
                SamplingEvaluationIntent::Relaxation,
                SamplingEvaluationIntent::Relaxation,
            ]
        );

        let fixed_port = RecordingSamplingPort::default();
        BasinHoppingWorkflowService
            .execute(
                &BasinHoppingWorkflowRequest {
                    base_candidate: cluster_candidate("seed"),
                    scientific_config: patina_types::BhScientificConfig {
                        method: BhMethodRecord::Fixed,
                        ..default_bh_scientific_config(2, 1, 10.0, 0.1)
                    },
                    enforce_container: false,
                    boundary: None,
                    seed: 7,
                },
                &super::BasinHoppingSummaryContext {
                    system: "MgO".into(),
                    backend_label: "gulp".into(),
                },
                &fixed_port,
                &LayoutStub,
                &NoopIdentityPort,
                &NullSink,
            )
            .expect("execute fixed basin hopping");
        assert_eq!(
            *fixed_port.intents.borrow(),
            vec![
                SamplingEvaluationIntent::InitialState,
                SamplingEvaluationIntent::SamplingStep,
                SamplingEvaluationIntent::SamplingStep,
            ]
        );
    }

    #[test]
    fn basin_hopping_uses_matched_relaxation_comparison_when_identity_port_supplies_it() {
        let execution = BasinHoppingWorkflowService
            .execute(
                &BasinHoppingWorkflowRequest {
                    base_candidate: cluster_candidate("seed"),
                    scientific_config: default_bh_scientific_config(1, 1, 10.0, 0.1),
                    enforce_container: false,
                    boundary: None,
                    seed: 7,
                },
                &super::BasinHoppingSummaryContext {
                    system: "MgO".into(),
                    backend_label: "gulp".into(),
                },
                &IdentitySamplingPort,
                &LayoutStub,
                &MatchedRelaxationIdentityPort,
                &NullSink,
            )
            .expect("execute basin hopping with matched relaxation evidence");

        let scientific_state = execution.walkers[0]
            .walker_state
            .scientific_state
            .as_ref()
            .expect("scientific state");
        assert_eq!(scientific_state.last_matched_relaxation_level, Some(2));
        assert_eq!(execution.trace.len(), 1);
        assert_eq!(execution.trace[0].matched_relaxation_level, Some(2));
        assert_eq!(execution.summary.accepted_steps, 1);
    }
}
