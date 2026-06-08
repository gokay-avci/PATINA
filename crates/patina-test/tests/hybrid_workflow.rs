use anyhow::Result;
use indexmap::IndexMap;
use patina_driver::application::ga_execution::RustJanusGaExecution;
use patina_driver::application::hybrid_core::{
    build_hybrid_seed_candidate_id, HybridGaProductionExecution, HybridGaProductionRequest,
    HybridGaProductionWorkflowService, HybridSeedCandidateMode, HybridSeedSelectionMode,
    HybridSelectedSeed, StaticAcquisitionGuidedHybridSeedSelectionPolicy,
    TopRankedHybridSeedSelectionPolicy,
};
use patina_driver::application::ports::{
    HybridGaProductionArtifactSink, HybridGaStagePort, HybridProductionStagePort,
};
use patina_driver::application::scott_production::{
    ProductionRestartState, ProductionRunConfig, ProductionWorkflowExecution,
    StagedProductionSummary,
};
use patina_emulate::{AcquisitionRecord, BranchId, LearningObjective, TaskDirection};
use patina_runtime::ScottBackendRoutingPolicy;
use patina_search::{ScottGaMember, ScottGaOrigin, WorkflowLineage};
use patina_types::{
    Candidate, EvalResult, HybridCrossoverConfig, HybridCrossoverScientificMode,
    RestartSeedSourceKind,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

fn shifted_coords(shift: f64) -> Vec<[f64; 3]> {
    vec![
        [0.0 + shift, 0.0, 0.0],
        [1.8 + shift, 0.1, 0.0],
        [0.2 + shift, 1.1, 0.4],
        [1.4 + shift, 1.9, 0.6],
    ]
}

fn cluster_candidate(label: &str, shift: f64) -> Candidate {
    Candidate::cluster(
        label,
        vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        shifted_coords(shift),
    )
}

fn ga_member(
    source_label: &str,
    relaxed_label: &str,
    shift: f64,
    energy: f64,
    converged: bool,
    origin: ScottGaOrigin,
) -> ScottGaMember {
    let source_candidate = cluster_candidate(source_label, shift);
    let mut relaxed_candidate = cluster_candidate(relaxed_label, shift + 0.05);
    relaxed_candidate.label = relaxed_label.into();
    ScottGaMember::new(
        source_candidate.clone(),
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; relaxed_candidate.len()],
            relaxed_candidate,
            converged,
            wall_time: Duration::from_secs(1),
        },
        origin,
        WorkflowLineage::seed(source_label),
    )
}

fn ga_execution(final_population: Vec<ScottGaMember>) -> RustJanusGaExecution {
    RustJanusGaExecution {
        generation_artifacts: Vec::new(),
        generation_responses: Vec::new(),
        generation_origin_metrics: Vec::new(),
        generation_boundary_populations: Vec::new(),
        generation_populations: Vec::new(),
        generation_boundary_kernel_states: Vec::new(),
        generation_kernel_states: Vec::new(),
        controller_trace: Vec::new(),
        final_population,
    }
}

fn dummy_routing_policy() -> ScottBackendRoutingPolicy {
    ScottBackendRoutingPolicy {
        default_backend: patina_evaluator::ScottBackendMode::Gulp,
        stage_overrides: IndexMap::new(),
    }
}

fn hybrid_request(
    selection_mode: HybridSeedSelectionMode,
    candidate_mode: HybridSeedCandidateMode,
    crossover_mode: HybridCrossoverScientificMode,
    max_production_seeds: usize,
) -> HybridGaProductionRequest {
    HybridGaProductionRequest {
        system: "MgO".into(),
        workdir: PathBuf::from("/tmp/patina-hybrid"),
        max_production_seeds,
        seed_selection_mode: selection_mode,
        seed_candidate_mode: candidate_mode,
        crossover_config: HybridCrossoverConfig {
            scientific_mode: crossover_mode,
            attempt_count: 24,
        },
        emulate_context: None,
        require_converged_ga_seeds: true,
        seed: 17,
        production_config: ProductionRunConfig {
            max_best_clusters: 8,
            ..ProductionRunConfig::default()
        },
        restart_state_before: Some(ProductionRestartState {
            counter: Some(3),
            random_start: Some(false),
        }),
        restart_counter_base: 3,
        fallback_routing_policy: dummy_routing_policy(),
    }
}

#[derive(Clone)]
struct RecordingGaStagePort {
    execution: RustJanusGaExecution,
    requests: RefCell<Vec<PathBuf>>,
}

impl RecordingGaStagePort {
    fn new(execution: RustJanusGaExecution) -> Self {
        Self {
            execution,
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl HybridGaStagePort for RecordingGaStagePort {
    fn execute_ga_stage(
        &self,
        request: &patina_driver::application::hybrid_core::HybridGaStageRequest,
    ) -> Result<RustJanusGaExecution> {
        self.requests.borrow_mut().push(request.workdir.clone());
        Ok(self.execution.clone())
    }
}

#[derive(Default)]
struct RecordingProductionStagePort {
    requests: RefCell<Vec<patina_driver::application::hybrid_core::HybridProductionStageRequest>>,
}

impl HybridProductionStagePort for RecordingProductionStagePort {
    fn execute_production_stage(
        &self,
        request: &patina_driver::application::hybrid_core::HybridProductionStageRequest,
    ) -> Result<ProductionWorkflowExecution> {
        self.requests.borrow_mut().push(request.clone());
        Ok(ProductionWorkflowExecution {
            summary: StagedProductionSummary {
                candidate_count: request.seeds.len(),
                success_count: request.seeds.len(),
                failure_count: 0,
                topology_skip_count: 0,
                best_set_size: 0,
                system: request.system.clone(),
                workdir: request.workdir.clone(),
                routing_policy: request.fallback_routing_policy.clone(),
                production_config: request.production_config.clone(),
                restart_state_before: request.restart_state_before.clone(),
                restart_state_after: request.restart_state_before.clone(),
                candidates: Vec::new(),
            },
            best_entries: Vec::new(),
        })
    }
}

#[derive(Default)]
struct RecordingArtifactSink {
    persisted: RefCell<Vec<usize>>,
}

impl HybridGaProductionArtifactSink for RecordingArtifactSink {
    fn persist_hybrid_ga_production_run(
        &self,
        execution: &HybridGaProductionExecution,
    ) -> Result<()> {
        self.persisted
            .borrow_mut()
            .push(execution.summary.selected_seed_count);
        Ok(())
    }
}

fn selected_labels(seeds: &[HybridSelectedSeed]) -> Vec<&str> {
    seeds
        .iter()
        .map(|seed| seed.selected_candidate_label.as_str())
        .collect()
}

#[test]
fn workflow_selects_top_converged_relaxed_candidates_for_production() {
    let ga_port = RecordingGaStagePort::new(ga_execution(vec![
        ga_member(
            "alpha-source",
            "alpha-relaxed",
            0.0,
            -10.0,
            true,
            ScottGaOrigin::Seed,
        ),
        ga_member(
            "beta-source",
            "beta-relaxed",
            2.0,
            -12.0,
            false,
            ScottGaOrigin::Crosso,
        ),
        ga_member(
            "gamma-source",
            "gamma-relaxed",
            4.0,
            -9.0,
            true,
            ScottGaOrigin::Mutate,
        ),
    ]));
    let production_port = RecordingProductionStagePort::default();
    let artifact_sink = RecordingArtifactSink::default();
    let service = HybridGaProductionWorkflowService;

    let execution = service
        .execute(
            &hybrid_request(
                HybridSeedSelectionMode::TopRanked,
                HybridSeedCandidateMode::RelaxedResult,
                HybridCrossoverScientificMode::GaDownstreamSelection,
                2,
            ),
            &ga_port,
            &production_port,
            &TopRankedHybridSeedSelectionPolicy,
            &artifact_sink,
        )
        .expect("execute hybrid workflow");

    assert_eq!(execution.summary.selected_seed_count, 2);
    assert_eq!(
        selected_labels(&execution.selected_seeds),
        ["alpha-relaxed", "gamma-relaxed"]
    );
    assert_eq!(
        execution
            .selected_seeds
            .iter()
            .map(|seed| seed.ga_rank)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        execution
            .selected_seeds
            .iter()
            .map(|seed| seed.source_candidate_label.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha-source", "gamma-source"]
    );

    let ga_requests = ga_port.requests.borrow();
    assert_eq!(
        ga_requests.as_slice(),
        &[PathBuf::from("/tmp/patina-hybrid/ga")]
    );

    let production_requests = production_port.requests.borrow();
    assert_eq!(production_requests.len(), 1);
    let production_request = &production_requests[0];
    assert_eq!(
        production_request.workdir,
        PathBuf::from("/tmp/patina-hybrid/production")
    );
    assert_eq!(production_request.seeds.len(), 2);
    assert_eq!(
        production_request
            .seeds
            .iter()
            .map(|seed| seed.provenance.source_kind)
            .collect::<Vec<_>>(),
        vec![
            RestartSeedSourceKind::HybridGaSelection,
            RestartSeedSourceKind::HybridGaSelection,
        ]
    );
    assert_eq!(
        production_request
            .seeds
            .iter()
            .map(|seed| seed.candidate.label.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha-relaxed", "gamma-relaxed"]
    );
    assert_eq!(artifact_sink.persisted.borrow().as_slice(), &[2]);
}

#[test]
fn workflow_can_select_production_seeds_from_acquisition_guidance() {
    let policy = StaticAcquisitionGuidedHybridSeedSelectionPolicy::new(
        "static-acquisition",
        vec![
            AcquisitionRecord {
                branch_id: BranchId("branch-1".into()),
                candidate_id: build_hybrid_seed_candidate_id(2, "MUTATE", "gamma-source"),
                rank: 1,
                direction: TaskDirection::Downstream,
                objective: LearningObjective::ScoreCandidates,
                acquisition_score: 9.0,
                novelty_score: 0.3,
                uncertainty_score: 0.6,
                expected_improvement: Some(0.4),
                recommended_family: None,
            },
            AcquisitionRecord {
                branch_id: BranchId("branch-1".into()),
                candidate_id: build_hybrid_seed_candidate_id(1, "CROSSO", "beta-source"),
                rank: 2,
                direction: TaskDirection::Downstream,
                objective: LearningObjective::ScoreCandidates,
                acquisition_score: 7.0,
                novelty_score: 0.2,
                uncertainty_score: 0.5,
                expected_improvement: Some(0.3),
                recommended_family: None,
            },
            AcquisitionRecord {
                branch_id: BranchId("branch-1".into()),
                candidate_id: build_hybrid_seed_candidate_id(0, "SEED", "alpha-source"),
                rank: 3,
                direction: TaskDirection::Downstream,
                objective: LearningObjective::ScoreCandidates,
                acquisition_score: 2.0,
                novelty_score: 0.1,
                uncertainty_score: 0.2,
                expected_improvement: Some(0.1),
                recommended_family: None,
            },
        ],
    )
    .expect("policy");
    let ga_port = RecordingGaStagePort::new(ga_execution(vec![
        ga_member(
            "alpha-source",
            "alpha-relaxed",
            0.0,
            -10.0,
            true,
            ScottGaOrigin::Seed,
        ),
        ga_member(
            "beta-source",
            "beta-relaxed",
            2.0,
            -9.0,
            true,
            ScottGaOrigin::Crosso,
        ),
        ga_member(
            "gamma-source",
            "gamma-relaxed",
            4.0,
            -8.0,
            true,
            ScottGaOrigin::Mutate,
        ),
    ]));
    let production_port = RecordingProductionStagePort::default();
    let artifact_sink = RecordingArtifactSink::default();
    let service = HybridGaProductionWorkflowService;

    let execution = service
        .execute(
            &hybrid_request(
                HybridSeedSelectionMode::AcquisitionGuided,
                HybridSeedCandidateMode::SourceCandidate,
                HybridCrossoverScientificMode::GaDownstreamSelection,
                2,
            ),
            &ga_port,
            &production_port,
            &policy,
            &artifact_sink,
        )
        .expect("execute hybrid workflow");

    assert_eq!(
        selected_labels(&execution.selected_seeds),
        ["gamma-source", "beta-source"]
    );
    assert_eq!(
        execution
            .selected_seeds
            .iter()
            .map(|seed| seed.ranking_basis)
            .collect::<Vec<_>>(),
        vec![
            patina_driver::application::hybrid_core::HybridSeedRankingBasis::AcquisitionGuided,
            patina_driver::application::hybrid_core::HybridSeedRankingBasis::AcquisitionGuided,
        ]
    );
    assert_eq!(
        execution.selected_seeds[0]
            .guidance
            .as_ref()
            .expect("guidance")
            .provenance_label,
        "static-acquisition"
    );
    assert_eq!(
        execution.selected_seeds[0]
            .guidance
            .as_ref()
            .expect("guidance")
            .acquisition_score,
        9.0
    );

    let production_requests = production_port.requests.borrow();
    assert_eq!(production_requests.len(), 1);
    let production_request = &production_requests[0];
    assert_eq!(
        production_request
            .seeds
            .iter()
            .map(|seed| seed.candidate.label.as_str())
            .collect::<Vec<_>>(),
        vec!["gamma-source", "beta-source"]
    );
}

#[test]
fn workflow_can_generate_enforced_crossover_children_from_selected_parents() {
    let ga_port = RecordingGaStagePort::new(ga_execution(vec![
        ga_member(
            "left-parent",
            "left-parent",
            0.0,
            -10.0,
            true,
            ScottGaOrigin::Seed,
        ),
        ga_member(
            "right-parent",
            "right-parent",
            0.4,
            -9.0,
            true,
            ScottGaOrigin::Crosso,
        ),
    ]));
    let production_port = RecordingProductionStagePort::default();
    let artifact_sink = RecordingArtifactSink::default();
    let service = HybridGaProductionWorkflowService;

    let execution = service
        .execute(
            &hybrid_request(
                HybridSeedSelectionMode::TopRanked,
                HybridSeedCandidateMode::RelaxedResult,
                HybridCrossoverScientificMode::EnforcedCrossover,
                2,
            ),
            &ga_port,
            &production_port,
            &TopRankedHybridSeedSelectionPolicy,
            &artifact_sink,
        )
        .expect("execute hybrid workflow");

    assert_eq!(execution.summary.selected_seed_count, 2);
    assert_eq!(execution.summary.accepted_child_count, 2);
    assert_eq!(execution.summary.production_input_count, 2);
    assert_eq!(execution.accepted_children.len(), 2);
    assert_eq!(execution.accepted_children[0].parent_ga_ranks, vec![1, 2]);
    assert_eq!(
        execution.accepted_children[0].parent_source_candidate_labels,
        vec!["left-parent".to_string(), "right-parent".to_string()]
    );
    assert!(execution.accepted_children[0]
        .source_name
        .starts_with("hybrid_child_0001_ga_0001_0002_attempt_"));

    let production_requests = production_port.requests.borrow();
    assert_eq!(production_requests.len(), 1);
    let production_request = &production_requests[0];
    assert_eq!(production_request.seeds.len(), 2);
    assert_eq!(
        production_request
            .seeds
            .iter()
            .map(|seed| seed.provenance.source_kind)
            .collect::<Vec<_>>(),
        vec![
            RestartSeedSourceKind::HybridGaCrossover,
            RestartSeedSourceKind::HybridGaCrossover,
        ]
    );
    assert_eq!(
        production_request
            .seeds
            .iter()
            .map(|seed| seed.candidate.label.as_str())
            .collect::<Vec<_>>(),
        execution
            .accepted_children
            .iter()
            .map(|child| child.child_candidate_label.as_str())
            .collect::<Vec<_>>()
    );
}
