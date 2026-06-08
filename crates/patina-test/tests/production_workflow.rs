use anyhow::Result;
use indexmap::IndexMap;
use patina_driver::application::ports::{
    ProductionAcceptedArtifactRecord, ProductionArtifactSink, ProductionEvaluationOutput,
    ProductionEvaluationPort, ProductionEvaluationRequest, ProductionIdentityPort,
    ProductionProgressPort,
};
use patina_driver::application::scott_production::{
    classify_production_outcome, extract_master_species, ProductionBestEntry,
    ProductionDecisionKind, ProductionRestartState, ProductionRunConfig, ProductionSeedInput,
    ProductionWorkflowService, ProductionWorkflowTracker,
};
use patina_driver::application::topology_identity::TopologyIdentityMatch;
use patina_evaluator::{
    FinalStageFailurePolicy, MasterTemplateLayout, NormalizedConvergence, ScottBackendMode,
    ScottEvalOutcome, ScottEvaluationState, ScottEvaluatorPlan, ScottEvaluatorSettings,
    ScottLatticeMode, ScottProcedureIntent, ScottProcedurePlan, ScottStageStatus,
    ScottValidityCheckKind, StageBackendStatus, StageEngine, StageIndex, StagePlan, StageSelection,
};
use patina_runtime::ScottBackendRoutingPolicy;
use patina_types::Candidate;
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;

fn candidate(label: &str, distance: f64) -> Candidate {
    Candidate {
        species: vec!["Mg".into(), "Mg".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [distance, 0.0, 0.0]],
        lattice: None,
        periodic_axes: [false, false, false],
        label: label.into(),
    }
}

fn accepted_outcome(label: &str, distance: f64, energy: f64) -> ScottEvalOutcome {
    let relaxed_candidate = candidate(label, distance);
    let result = patina_types::EvalResult {
        energy,
        forces: vec![[0.0, 0.0, 0.0]; 2],
        relaxed_candidate: relaxed_candidate.clone(),
        converged: true,
        wall_time: Duration::from_secs(1),
    };
    let mut state = ScottEvaluationState::new(
        "test-request".into(),
        relaxed_candidate.clone(),
        StageIndex(1),
    );
    state.record_stage_evaluation(
        ScottStageStatus {
            stage: StageIndex(1),
            attempt: 1,
            backend_status: StageBackendStatus::Converged,
            convergence: NormalizedConvergence::Accepted,
            energy: Some(energy),
            gnorm: Some(0.0),
            relaxed_label: Some(relaxed_candidate.label.clone()),
            primary_output_path: Some("/tmp/test.got".into()),
        },
        Some(result.clone()),
    );
    state.set_accepted_result(StageIndex(1), result.clone());
    let provenance = state.provenance();
    ScottEvalOutcome {
        final_result: Some(result),
        final_stage: Some(StageIndex(1)),
        relax_failed: false,
        provenance,
        state,
    }
}

fn dummy_routing_policy() -> ScottBackendRoutingPolicy {
    ScottBackendRoutingPolicy {
        default_backend: ScottBackendMode::Gulp,
        stage_overrides: IndexMap::new(),
    }
}

fn dummy_procedure_plan() -> ScottProcedurePlan {
    ScottProcedurePlan {
        evaluator: ScottEvaluatorPlan {
            master_template: camino::Utf8PathBuf::from("/tmp/Master.gin"),
            atoms_in: None,
            work_root: camino::Utf8PathBuf::from("/tmp"),
            settings: ScottEvaluatorSettings {
                backend_mode: ScottBackendMode::Gulp,
                procedure_intent: ScottProcedureIntent::ProductionRun,
                lattice_mode: ScottLatticeMode::Cluster,
                max_relaxation_attempts: 1,
                gnorm_tolerance: 1.0e-4,
                final_stage_failure_policy: FinalStageFailurePolicy::KeepPreviousAccepted,
                retrieve_relaxed_geometry: true,
                keep_stage_artifacts: true,
            },
        },
        stages: StageSelection {
            stages: vec![StagePlan {
                stage: StageIndex(1),
                engine: StageEngine::Gulp,
                refine_if_energy_below: None,
                energy_min_threshold: None,
                energy_max_threshold: None,
                keep_only_if_final_stage: false,
            }],
        },
    }
}

struct FakeEvaluationPort {
    input_hashkeys: Vec<Option<String>>,
    final_hashkeys: Vec<Option<String>>,
    outcomes: Vec<ScottEvalOutcome>,
    evaluated_indices: RefCell<Vec<usize>>,
}

impl ProductionEvaluationPort for FakeEvaluationPort {
    fn evaluate_candidate(
        &self,
        request: &ProductionEvaluationRequest,
    ) -> Result<ProductionEvaluationOutput> {
        self.evaluated_indices.borrow_mut().push(request.index);
        Ok(ProductionEvaluationOutput {
            default_backend: ScottBackendMode::Gulp,
            routing_policy: dummy_routing_policy(),
            procedure_plan: dummy_procedure_plan(),
            outcome: self.outcomes[request.index].clone(),
        })
    }
}

impl ProductionIdentityPort for FakeEvaluationPort {
    fn build_input_hashkey(&self, request: &ProductionEvaluationRequest) -> Result<Option<String>> {
        Ok(self.input_hashkeys[request.index].clone())
    }

    fn build_final_hashkey(
        &self,
        request: &ProductionEvaluationRequest,
        _evaluated: &ProductionEvaluationOutput,
        fallback_hashkey: Option<&str>,
    ) -> Result<Option<String>> {
        Ok(self.final_hashkeys[request.index]
            .clone()
            .or_else(|| fallback_hashkey.map(ToOwned::to_owned)))
    }
}

#[derive(Default)]
struct FakeProgressPort {
    consumed: RefCell<Vec<(Option<String>, Option<usize>)>>,
}

impl ProductionProgressPort for FakeProgressPort {
    fn mark_seed_consumed(
        &self,
        source_name: Option<&str>,
        restart_state: Option<&ProductionRestartState>,
    ) -> Result<()> {
        self.consumed.borrow_mut().push((
            source_name.map(ToOwned::to_owned),
            restart_state.and_then(|state| state.counter),
        ));
        Ok(())
    }
}

#[derive(Default)]
struct FakeArtifactSink {
    accepted_indices: RefCell<Vec<usize>>,
}

impl ProductionArtifactSink for FakeArtifactSink {
    fn persist_accepted_candidate(
        &self,
        artifact: &ProductionAcceptedArtifactRecord,
    ) -> Result<()> {
        self.accepted_indices.borrow_mut().push(artifact.index);
        Ok(())
    }
}

#[test]
fn production_precheck_only_skips_best_archive_matches() {
    let mut tracker = ProductionWorkflowTracker::new(
        1,
        "MgO",
        PathBuf::from("/tmp/production"),
        ProductionRunConfig {
            max_best_clusters: 2,
            use_top_analysis: true,
            ..ProductionRunConfig::default()
        },
        None,
    );
    tracker.consider_best_entry(ProductionBestEntry {
        candidate_label: "accepted".into(),
        final_stage: Some(1),
        energy: -10.0,
        hashkey: Some("hk-archive".into()),
        relaxed_candidate: candidate("accepted", 2.0),
    });

    assert!(matches!(
        tracker.compare_topology_hashkey(Some("hk-archive")),
        Some(TopologyIdentityMatch::BestArchive(record))
            if record.rank == 1 && record.candidate_label == "accepted"
    ));
    assert!(tracker.compare_topology_hashkey(Some("hk-black")).is_none());
    assert!(tracker.compare_topology_hashkey(None).is_none());
}

#[test]
fn production_workflow_service_routes_accept_skip_and_reject() {
    let service = ProductionWorkflowService;
    let seeds = vec![
        ProductionSeedInput::restart_artifact("seed_a.xyz".into(), candidate("accepted", 2.0)),
        ProductionSeedInput::restart_artifact("seed_b.xyz".into(), candidate("skipped", 2.1)),
        ProductionSeedInput::restart_artifact("seed_c.xyz".into(), candidate("rejected", 1.0)),
    ];
    let evaluation_port = FakeEvaluationPort {
        input_hashkeys: vec![Some("hk-1".into()), Some("hk-1".into()), None],
        final_hashkeys: vec![Some("hk-1".into()), Some("hk-1".into()), None],
        outcomes: vec![
            accepted_outcome("accepted", 2.0, -10.0),
            accepted_outcome("skipped", 2.1, -9.0),
            accepted_outcome("rejected", 1.0, -1.0),
        ],
        evaluated_indices: RefCell::new(Vec::new()),
    };
    let progress_port = FakeProgressPort::default();
    let artifact_sink = FakeArtifactSink::default();
    let workdir = tempdir().expect("tempdir");

    let execution = service
        .execute(
            &seeds,
            "MgO",
            workdir.path(),
            &ProductionRunConfig {
                max_best_clusters: 2,
                best_energy_cutoff: -5.0,
                use_top_analysis: true,
                collapse_threshold: 1.5,
                ..ProductionRunConfig::default()
            },
            Some(ProductionRestartState {
                counter: Some(0),
                random_start: Some(false),
            }),
            0,
            dummy_routing_policy(),
            &evaluation_port,
            &evaluation_port,
            &progress_port,
            &artifact_sink,
        )
        .expect("execute production workflow");

    assert_eq!(execution.summary.success_count, 1);
    assert_eq!(execution.summary.failure_count, 1);
    assert_eq!(execution.summary.topology_skip_count, 1);
    assert_eq!(execution.summary.best_set_size, 1);
    assert_eq!(execution.summary.candidates.len(), 3);
    assert_eq!(
        execution.summary.candidates[1]
            .seed_provenance
            .as_ref()
            .map(|record| record.source_name.as_deref()),
        Some(Some("seed_b.xyz"))
    );
    assert_eq!(execution.summary.candidates[0].relaxation_stages.len(), 1);
    assert_eq!(
        execution.summary.candidates[0]
            .topology_comparison
            .as_ref()
            .and_then(|record| record.final_hashkey.as_deref()),
        Some("hk-1")
    );
    assert_eq!(
        execution.summary.candidates[0]
            .best_set_decision
            .as_ref()
            .map(|record| record.kind),
        Some(patina_types::BestSetDecisionKind::Inserted)
    );
    assert!(execution.summary.candidates[1].relaxation_stages.is_empty());
    assert_eq!(
        execution.summary.candidates[1]
            .topology_comparison
            .as_ref()
            .and_then(|record| record.action),
        Some(patina_types::TopologyComparisonActionRecord::MatchedBestArchiveInputHashkey)
    );
    assert_eq!(
        execution.summary.candidates[1]
            .best_set_decision
            .as_ref()
            .map(|record| record.kind),
        Some(patina_types::BestSetDecisionKind::MatchedExisting)
    );
    assert!(execution.summary.candidates[2].best_set_decision.is_none());
    assert_eq!(*evaluation_port.evaluated_indices.borrow(), vec![0, 2]);
    assert_eq!(*artifact_sink.accepted_indices.borrow(), vec![0]);
}

#[test]
fn production_classification_records_hashkey_evidence_for_accepted_candidates() {
    let cfg = ProductionRunConfig {
        use_top_analysis: true,
        ..ProductionRunConfig::default()
    };
    let mut outcome = accepted_outcome("good", 2.0, -12.0);
    let (decision, energy) = classify_production_outcome(
        &cfg,
        &mut outcome,
        Some("input-hk".into()),
        Some("final-hk".into()),
    );

    assert_eq!(decision.kind, ProductionDecisionKind::Accepted);
    assert_eq!(energy, Some(-12.0));
    assert_eq!(outcome.state.science_evidence.validity_checks.len(), 1);
    assert_eq!(
        outcome.state.science_evidence.validity_checks[0].kind,
        ScottValidityCheckKind::HashkeyAvailable
    );
    assert!(outcome.state.science_evidence.validity_checks[0].passed);
}

#[test]
fn production_extracts_master_species_from_template_atom_block() {
    let layout = MasterTemplateLayout::parse(
        r#"
        opti
        cartesian
        Mg core 0.0 0.0 0.0
        O core 1.0 1.0 1.0
        extra
        species
        Mg core 2.0
        O core -2.0
        "#,
    )
    .expect("layout");

    assert_eq!(extract_master_species(&layout), vec!["Mg", "O"]);
}
