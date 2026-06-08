use anyhow::{anyhow, Result};
use patina_search::{PopulationKernelState, WorkflowFamily};

use super::ga_execution::RustJanusGaExecution;
use super::ports::{GaArtifactSink, GaEvaluationPort};
use super::rust_janus_ga::{RustJanusGaCoreSetup, RustJanusGaStartState};
use super::scott_ga_runtime::{
    build_rust_janus_generation_artifact, build_rust_janus_origin_metrics, build_worker_requests,
    pair_child_responses, pair_initial_responses, resolve_pending_repopulation,
};
use super::workflow_policy::GaWorkflowPolicy;
use super::workflow_tasks::take_queued_task;

#[derive(Debug, Clone, Copy)]
pub struct GaWorkflowRequest {
    pub requested_generations: usize,
}

/// Application service that owns GA workflow sequencing while delegating
/// evaluation details to an explicit port.
pub struct GaWorkflowService {
    policy: GaWorkflowPolicy,
}

impl GaWorkflowService {
    pub fn new(policy: GaWorkflowPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> GaWorkflowPolicy {
        self.policy
    }

    pub fn execute_with_generation_sink(
        &self,
        request: GaWorkflowRequest,
        core: RustJanusGaCoreSetup,
        evaluator: &impl GaEvaluationPort,
        artifact_sink: Option<&dyn GaArtifactSink>,
    ) -> Result<RustJanusGaExecution> {
        let RustJanusGaCoreSetup {
            mut controller_bootstrap,
            ..
        } = core;
        let mut generation_origins = Vec::with_capacity(request.requested_generations + 1);
        let mut execution = RustJanusGaExecution {
            generation_artifacts: Vec::with_capacity(request.requested_generations + 1),
            generation_responses: Vec::with_capacity(request.requested_generations + 1),
            generation_origin_metrics: Vec::with_capacity(request.requested_generations + 1),
            generation_boundary_populations: Vec::with_capacity(request.requested_generations + 1),
            generation_populations: Vec::with_capacity(request.requested_generations + 1),
            generation_boundary_kernel_states: Vec::with_capacity(
                request.requested_generations + 1,
            ),
            generation_kernel_states: Vec::with_capacity(request.requested_generations + 1),
            controller_trace: Vec::new(),
            final_population: Vec::new(),
        };

        let mut population;
        let start_generation;
        match controller_bootstrap.start_state {
            RustJanusGaStartState::Fresh { initial_candidates } => {
                let initial_requests = build_worker_requests(0, &initial_candidates);
                let initial_started = std::time::Instant::now();
                let mut initial_responses = evaluator.evaluate_generation(&initial_requests)?;
                let mut initial_elapsed_secs = initial_started.elapsed().as_secs_f64();
                let mut initial_response_origins =
                    vec![patina_search::ScottGaOrigin::Seed; initial_requests.len()];
                let initial_eval = pair_initial_responses(&initial_candidates, &initial_responses)?;
                let transition = controller_bootstrap
                    .controller
                    .ingest_initial_population_with_boundary(initial_eval)?;
                let boundary_population = transition.boundary_population;
                population = transition.working_population;
                resolve_pending_repopulation(
                    0,
                    evaluator,
                    &mut controller_bootstrap.controller,
                    &mut population,
                    &mut initial_responses,
                    &mut initial_response_origins,
                    &mut initial_elapsed_secs,
                )?;
                execution
                    .generation_artifacts
                    .push(build_rust_janus_generation_artifact(
                        0,
                        "initialize",
                        initial_elapsed_secs,
                        &initial_responses,
                        &boundary_population,
                        &population,
                        controller_bootstrap.controller.trace().last(),
                    ));
                execution.generation_responses.push(initial_responses);
                generation_origins.push(initial_response_origins);
                execution
                    .generation_boundary_populations
                    .push(boundary_population.clone());
                execution.generation_populations.push(population.clone());
                execution
                    .generation_boundary_kernel_states
                    .push(snapshot_population_kernel_state(0, &boundary_population)?);
                execution
                    .generation_kernel_states
                    .push(snapshot_population_kernel_state(0, &population)?);
                execution
                    .generation_origin_metrics
                    .push(build_rust_janus_origin_metrics(
                        0,
                        "initialize",
                        generation_origins
                            .last()
                            .ok_or_else(|| anyhow!("initial GA origin metrics missing origins"))?,
                        execution.generation_responses.last().ok_or_else(|| {
                            anyhow!("initial GA origin metrics missing worker responses")
                        })?,
                        &boundary_population,
                        &population,
                    ));
                persist_generation_if_requested(artifact_sink, &execution)?;
                start_generation = 1;
            }
            RustJanusGaStartState::Resume {
                completed_generation,
                population: resumed_population,
            } => {
                population = resumed_population;
                start_generation = completed_generation + 1;
            }
        }

        for generation in start_generation..=request.requested_generations {
            let selected = controller_bootstrap
                .controller
                .selection_tournament(&population);
            controller_bootstrap.controller.record_selection_trace(
                generation,
                &population,
                &selected,
            );
            let children = controller_bootstrap.controller.spawn_children(
                generation,
                &population,
                &selected,
            )?;
            let mut response_origins = children
                .iter()
                .map(|(_, origin, _)| *origin)
                .collect::<Vec<_>>();
            let child_candidates: Vec<_> = children
                .iter()
                .map(|(candidate, _, _)| candidate.clone())
                .collect();
            let requests = build_worker_requests(generation, &child_candidates);
            let generation_started = std::time::Instant::now();
            let mut responses = evaluator.evaluate_generation(&requests)?;
            let mut elapsed_secs = generation_started.elapsed().as_secs_f64();
            let evaluated = pair_child_responses(children, &responses)?;
            let transition = controller_bootstrap
                .controller
                .integrate_generation_with_boundary(generation, &population, evaluated)?;
            let boundary_population = transition.boundary_population;
            population = transition.working_population;
            resolve_pending_repopulation(
                generation,
                evaluator,
                &mut controller_bootstrap.controller,
                &mut population,
                &mut responses,
                &mut response_origins,
                &mut elapsed_secs,
            )?;
            execution
                .generation_artifacts
                .push(build_rust_janus_generation_artifact(
                    generation,
                    "evolve",
                    elapsed_secs,
                    &responses,
                    &boundary_population,
                    &population,
                    controller_bootstrap.controller.trace().last(),
                ));
            execution.generation_responses.push(responses);
            generation_origins.push(response_origins);
            execution
                .generation_boundary_populations
                .push(boundary_population.clone());
            execution.generation_populations.push(population.clone());
            execution
                .generation_boundary_kernel_states
                .push(snapshot_population_kernel_state(
                    generation,
                    &boundary_population,
                )?);
            execution
                .generation_kernel_states
                .push(snapshot_population_kernel_state(generation, &population)?);
            execution
                .generation_origin_metrics
                .push(build_rust_janus_origin_metrics(
                    generation,
                    "evolve",
                    generation_origins
                        .last()
                        .ok_or_else(|| anyhow!("GA generation metrics missing origins"))?,
                    execution
                        .generation_responses
                        .last()
                        .ok_or_else(|| anyhow!("GA generation metrics missing worker responses"))?,
                    &boundary_population,
                    &population,
                ));
            persist_generation_if_requested(artifact_sink, &execution)?;
        }

        execution.controller_trace = controller_bootstrap.controller.trace().to_vec();
        execution.final_population = population;

        Ok(execution)
    }
}

fn persist_generation_if_requested(
    artifact_sink: Option<&dyn GaArtifactSink>,
    execution: &RustJanusGaExecution,
) -> Result<()> {
    if let Some(artifact_sink) = artifact_sink {
        let generation_index = execution
            .generation_artifacts
            .len()
            .checked_sub(1)
            .ok_or_else(|| anyhow!("generation persistence requires at least one artifact"))?;
        artifact_sink.persist_generation(generation_index, execution)?;
    }
    Ok(())
}

fn snapshot_population_kernel_state(
    generation: usize,
    population: &[patina_search::ScottGaMember],
) -> Result<PopulationKernelState> {
    let mut state = PopulationKernelState::new(WorkflowFamily::GeneticAlgorithm);
    state.begin_generation(generation);
    let elite_limit = population.len().clamp(1, 3);

    for member in population {
        let lineage = member.lineage.clone();
        state.queue_candidate(member.source_candidate.clone(), lineage);
        let task = take_queued_task(state.next_task(), "GA population snapshot")?;
        state.record_member(task, member.result.clone(), elite_limit);
    }

    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::{snapshot_population_kernel_state, GaWorkflowRequest, GaWorkflowService};
    use crate::application::ga_execution::RustJanusGenerationArtifact;
    use crate::application::janus_operator_policy::{
        select_janus_ga_operator_policy, GaOperatorBackendProfile,
    };
    use crate::application::ports::GaEvaluationPort;
    use crate::application::rust_janus_ga::{prepare_rust_janus_ga_core, RustJanusGaCoreRequest};
    use crate::application::rust_janus_ga_checkpoint::{build_checkpoint, RustGaCheckpointContext};
    use crate::application::workflow_policy::GaWorkflowPolicy;
    use anyhow::Result;
    use patina_search::{ScottGaMember, ScottGaOrigin, ScottGaTopologyIdentity, WorkflowLineage};
    use patina_types::{
        Candidate, EvalResult, SearchConfig, WorkerOutcome, WorkerRequest, WorkerResponse,
    };
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn candidate(label: &str) -> patina_types::Candidate {
        patina_types::Candidate::cluster(label, vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn member(label: &str, energy: f64, origin: ScottGaOrigin) -> patina_search::ScottGaMember {
        patina_search::ScottGaMember {
            source_candidate: candidate(label),
            result: patina_types::EvalResult {
                energy,
                forces: vec![[0.0, 0.0, 0.0]],
                relaxed_candidate: candidate(&format!("{label}_relaxed")),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin,
            occurrences: 1,
            lineage: WorkflowLineage::seed(label.to_string()).with_generation(4),
            topology: patina_search::ScottGaTopologyIdentity::default(),
        }
    }

    #[test]
    fn snapshot_population_kernel_state_tracks_generation_and_elites() {
        let population = vec![
            member("seed", -5.0, ScottGaOrigin::Seed),
            member("mut", -3.0, ScottGaOrigin::Mutate),
            member("cross", -4.0, ScottGaOrigin::Crosso),
        ];

        let state = snapshot_population_kernel_state(4, &population).expect("snapshot state");

        assert_eq!(state.generation, 4);
        assert_eq!(state.population.members.len(), 3);
        assert_eq!(state.members().len(), 3);
        assert_eq!(state.elites().len(), 3);
        assert_eq!(state.elites()[0].result.energy, -5.0);
        assert_eq!(state.members()[0].lineage.origin_label, "seed");
    }

    fn cluster_candidate(label: &str, offset: f64) -> Candidate {
        Candidate::cluster(
            label,
            vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
            vec![
                [offset, 0.0, 0.0],
                [1.2 + offset * 0.10, 0.1, 0.0],
                [0.2, 1.4 + offset * 0.20, 0.2],
                [0.4, 0.3, 1.6 + offset * 0.15],
            ],
        )
    }

    fn resumed_member(label: &str, idx: usize, energy: f64) -> ScottGaMember {
        let source = cluster_candidate(label, idx as f64 + 1.0);
        let mut relaxed = source.clone();
        relaxed.label = format!("{label}_relaxed");
        ScottGaMember {
            source_candidate: source.clone(),
            result: EvalResult {
                energy,
                forces: vec![[0.0, 0.0, 0.0]; source.len()],
                relaxed_candidate: relaxed,
                converged: true,
                wall_time: Duration::from_secs(0),
            },
            origin: ScottGaOrigin::Seed,
            occurrences: 1,
            lineage: WorkflowLineage::seed(label.to_string()).with_generation(1),
            topology: ScottGaTopologyIdentity::default(),
        }
    }

    fn checkpoint_artifact(
        generation: usize,
        population_size: usize,
    ) -> RustJanusGenerationArtifact {
        RustJanusGenerationArtifact {
            generation,
            phase: "evolve".into(),
            request_count: population_size,
            success_count: population_size,
            failure_count: 0,
            failure_kind_counts: BTreeMap::new(),
            converged_count: population_size,
            elapsed_secs: 0.0,
            best_energy: Some(-10.0),
            mean_energy: Some(-9.0),
            worst_energy: Some(-8.0),
            boundary_population_size: population_size,
            boundary_valid_population_size: population_size,
            population_size,
            valid_population_size: population_size,
            duplicate_count: 0,
            duplicate_hashkey_count: 0,
            duplicate_pmoi_count: 0,
            duplicate_energy_tol_count: 0,
            repopulated_count: 0,
        }
    }

    struct DeterministicEvaluator;

    impl GaEvaluationPort for DeterministicEvaluator {
        fn evaluate_generation(&self, requests: &[WorkerRequest]) -> Result<Vec<WorkerResponse>> {
            Ok(requests
                .iter()
                .enumerate()
                .map(|(idx, request)| {
                    let mut relaxed = request.candidate.clone();
                    relaxed.label = format!("{}_relaxed", request.candidate.label);
                    let generation = request.generation.unwrap_or(0);
                    WorkerResponse {
                        request_id: request.request_id.clone(),
                        generation: request.generation,
                        worker_slot: Some(idx),
                        outcome: WorkerOutcome::Success {
                            result: EvalResult {
                                energy: -100.0 - generation as f64 - idx as f64 / 1000.0,
                                forces: vec![[0.0, 0.0, 0.0]; request.candidate.len()],
                                relaxed_candidate: relaxed,
                                converged: true,
                                wall_time: Duration::from_secs(0),
                            },
                        },
                    }
                })
                .collect())
        }
    }

    #[test]
    fn resumed_workflow_continues_with_child_lineage_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workdir = temp.path().join("workdir");
        let run_dir = temp.path().join("run");
        let checkpoint_path = temp.path().join("rust_ga_checkpoint.json");
        let search_cfg = SearchConfig {
            temperature: 625.0,
            step_size: 0.25,
            population_size: 4,
            max_steps: 2,
            seed: Some(31),
        };
        let operator_policy = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::Gulp,
            crate::DriverJanusMode::LocalOpt,
            search_cfg.step_size,
        );
        let population = vec![
            resumed_member("resume_parent_0", 0, -10.0),
            resumed_member("resume_parent_1", 1, -9.0),
            resumed_member("resume_parent_2", 2, -8.0),
            resumed_member("resume_parent_3", 3, -7.0),
        ];
        let checkpoint = build_checkpoint(
            &RustGaCheckpointContext {
                workflow_id: "ga.scott-monolithic",
                workflow_owner: "scott_monolithic_ga",
                backend: "scott_runtime",
                backend_mode: "gulp",
                system: "resume-test",
                requested_generations: 2,
            },
            &search_cfg,
            &operator_policy,
            &[checkpoint_artifact(1, population.len())],
            &population,
        );
        std::fs::write(
            &checkpoint_path,
            serde_json::to_string_pretty(&checkpoint).expect("checkpoint json"),
        )
        .expect("write checkpoint");

        let core = prepare_rust_janus_ga_core(&RustJanusGaCoreRequest {
            base_candidate: None,
            base_candidate_source_path: None,
            workdir,
            run_dir,
            resume_from_checkpoint: Some(checkpoint_path),
            requested_generations: 2,
            population_size: 4,
            seed: Some(31),
            temperature: search_cfg.temperature,
            step_size: search_cfg.step_size,
            operator_policy_backend: Some("gulp".into()),
            janus_mode: crate::DriverJanusMode::LocalOpt,
            atoms_in_template: None,
            use_dreadnaut_keys: false,
            hashkey_radius: "IR".into(),
            hashkey_radius_const: 0.4,
            pmoi_tolerance: 0.02,
            enable_pmoi: false,
            operator_overrides:
                crate::application::rust_janus_ga::RustJanusGaOperatorOverrides::default(),
        })
        .expect("resume core");

        let execution = GaWorkflowService::new(GaWorkflowPolicy::scott_staged_runtime())
            .execute_with_generation_sink(
                GaWorkflowRequest {
                    requested_generations: 2,
                },
                core,
                &DeterministicEvaluator,
                None,
            )
            .expect("resume execution");

        assert_eq!(execution.generation_artifacts.len(), 1);
        assert_eq!(execution.generation_artifacts[0].generation, 2);
        assert_eq!(execution.final_population.len(), search_cfg.population_size);
        assert!(execution
            .final_population
            .iter()
            .all(|member| member.lineage.generation.is_some()));
        assert!(execution.final_population.iter().any(|member| {
            member.lineage.generation == Some(2)
                && !member.lineage.parent_labels.is_empty()
                && !matches!(member.origin, ScottGaOrigin::RePopR | ScottGaOrigin::Seed)
        }));
    }
}
