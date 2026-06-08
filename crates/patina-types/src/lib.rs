#![forbid(unsafe_code)]

/*!
What this crate implements: the pure data model shared by the rest of the workspace.
Design basis: prompt Sections 2 and 3 require `patina-types` to remain I/O-free and FFI-free
so it can compile quickly, be tested independently, and be reused across backends.
Assumption: a `Population` stores the evaluated structure together with the candidate that
generated it, even if the relaxed geometry inside `EvalResult` differs from the original.
*/
mod evaluation;
mod search;
mod structure;
mod worker;
mod workflow;
mod workflow_catalog;
mod workflow_spec;

pub use evaluation::{EvalResult, EvaluationRecord};
pub use search::{Population, SearchConfig};
pub use structure::{Candidate, CandidateError, StructureDimensionality, StructureRecord};
pub use worker::{WorkerFailure, WorkerFailureKind, WorkerOutcome, WorkerRequest, WorkerResponse};
pub use workflow::{
    BestSetDecisionKind, BestSetDecisionRecord, BhAcceptanceRuleRecord, BhMethodRecord,
    BhMoveClassRecord, BhMoveRegimeRecord, BhScientificConfig, BhWalkerScientificState,
    BhWalkerState, EnergyLidWindowState, GaGenerationState, GaMemberLineageRecord, GaMemberState,
    GaMemberTopologyRecord, HoldingPointRecord, HybridChildOrigin, HybridCrossoverAttemptRecord,
    HybridCrossoverConfig, HybridCrossoverScientificMode, MoveClassActivationRecord,
    PopulationRepopulationRecord, PopulationRepopulationSource, RelaxationBackendStatusRecord,
    RelaxationConvergenceRecord, RelaxationStageRecord, RestartSeedProvenance,
    RestartSeedSourceKind, RunnerBranchRecord, SamplingCheckpointSchedule, SamplingRejectionRecord,
    SamplingWalkerState, SamplingWorkflowFamily, SimulatedAnnealingStructureState,
    SolidSolutionDuplicateRecord, SolidSolutionDuplicateSource, SolidSolutionRunState,
    SolidSolutionStepDecision, SolidSolutionStepState, TopologyComparisonActionRecord,
    TopologyComparisonReasonRecord, TopologyComparisonRecord, WalkerRestartArtifactKind,
    WalkerRestartEquivalenceRecord, WalkerRestartState, WalkerStartSource,
};
pub use workflow_catalog::{
    workflow_by_id, workflow_by_route, workflow_registry, WorkflowContext, WorkflowDefinition,
    WorkflowFileContract, WorkflowFileKind,
};
pub use workflow_spec::{
    BasinHoppingSamplingSpec, BasinHoppingSpec, CandidateSeedSpec, ClusterPerturbationControlSpec,
    DuplicatePolicySpec, EnergyLidSamplingSpec, EnergyLidSpec, FrameworkGcmcControlSpec,
    FrameworkGcmcSpec, GaSeedSpec, GenerateSurfaceSpec, JanusDuplicatePolicySpec,
    JanusGaOperatorPolicySpec, JanusPersistentGaSpec, JanusRuntimeSpec, PeriodicStructureInputSpec,
    PersistentJanusBackendSpec, PerturbClusterSpec, RunLocationSpec, RuntimeRoutingSpec,
    SamplingBackendSpec, ScottGaControllerSpec, ScottStagedGaSpec, SimulatedAnnealingSamplingSpec,
    SimulatedAnnealingSpec, SourceRunSelectionSpec, StagedBackendTemplateSpec, SurfaceCutSpec,
    SurfaceRunLocationSpec, WorkflowRunSpec,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample_candidate(label: &str, offset: f64) -> Candidate {
        Candidate::fully_periodic(
            label,
            vec!["Ce".into(), "O".into()],
            vec![[0.0 + offset, 0.0, 0.0], [0.5, 0.5 + offset, 0.5]],
            [[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]],
        )
    }

    fn sample_result(label: &str, energy: f64) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; 2],
            relaxed_candidate: sample_candidate(label, 0.0),
            converged: true,
            wall_time: Duration::from_secs(1),
        }
    }

    #[test]
    fn candidate_validation_catches_shape_errors() {
        let candidate = Candidate {
            species: vec!["Ce".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "broken".into(),
        };

        assert_eq!(
            candidate.validate(),
            Err(CandidateError::MismatchedSiteCounts {
                species: 1,
                coords: 2,
            })
        );
    }

    #[test]
    fn candidate_validation_rejects_periodic_axes_without_lattice() {
        let candidate = Candidate {
            species: vec!["Ce".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [true, false, false],
            label: "invalid-wire".into(),
        };

        assert_eq!(
            candidate.validate(),
            Err(CandidateError::PeriodicAxesWithoutLattice {
                periodic_axes: [true, false, false],
            })
        );
    }

    #[test]
    fn candidate_reports_declared_dimensionality_and_search_support() {
        let zero_d = Candidate::cluster("cluster", vec!["Ce".into()], vec![[0.0, 0.0, 0.0]]);
        let one_d = Candidate::periodic(
            "wire",
            vec!["Ce".into()],
            vec![[0.0, 0.0, 0.0]],
            [[5.0, 0.0, 0.0], [0.0, 20.0, 0.0], [0.0, 0.0, 20.0]],
            [true, false, false],
        );
        let two_d = Candidate::periodic(
            "slab",
            vec!["Ce".into()],
            vec![[0.0, 0.0, 0.0]],
            [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 20.0]],
            [true, true, false],
        );
        let three_d = sample_candidate("bulk", 0.0);

        assert_eq!(
            zero_d.declared_dimensionality(),
            StructureDimensionality::ZeroD
        );
        assert_eq!(
            one_d.declared_dimensionality(),
            StructureDimensionality::OneD
        );
        assert_eq!(
            two_d.declared_dimensionality(),
            StructureDimensionality::TwoD
        );
        assert_eq!(
            three_d.declared_dimensionality(),
            StructureDimensionality::ThreeD
        );

        assert!(zero_d.supports_native_scott_search());
        assert!(!one_d.supports_native_scott_search());
        assert!(!two_d.supports_native_scott_search());
        assert!(three_d.supports_native_scott_search());
    }

    #[test]
    fn structure_record_reports_declared_dimensionality() {
        let record = StructureRecord {
            label: "surface".into(),
            species: vec!["Ce".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0]],
            lattice: Some([[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 20.0]]),
            periodic_axes: [true, true, false],
        };

        assert_eq!(
            record.declared_dimensionality(),
            StructureDimensionality::TwoD
        );
    }

    #[test]
    fn candidate_roundtrip_serializes_with_serde() {
        let candidate = sample_candidate("ceria", 0.1);
        let json = serde_json::to_string(&candidate).expect("serialize candidate");
        let decoded: Candidate = serde_json::from_str(&json).expect("deserialize candidate");
        assert_eq!(decoded, candidate);
    }

    #[test]
    fn eval_result_roundtrip_serializes_with_serde() {
        let result = sample_result("ceria", -123.45);
        let json = serde_json::to_string(&result).expect("serialize eval result");
        let decoded: EvalResult = serde_json::from_str(&json).expect("deserialize eval result");
        assert_eq!(decoded, result);
    }

    #[test]
    fn structure_record_roundtrip_matches_candidate() {
        let candidate = sample_candidate("ceria", 0.2);
        let record = StructureRecord::from(&candidate);
        let restored = Candidate::from(&record);
        assert_eq!(restored, candidate);
    }

    #[test]
    fn evaluation_record_rebuilds_minimal_eval_result() {
        let result = sample_result("ceria", -55.0);
        let record = EvaluationRecord::from(&result);
        let restored = record.to_eval_result();
        assert_eq!(restored.energy, result.energy);
        assert_eq!(restored.converged, result.converged);
        assert_eq!(restored.relaxed_candidate, result.relaxed_candidate);
        assert_eq!(restored.forces.len(), result.relaxed_candidate.len());
    }

    #[test]
    fn evaluation_record_serializes_non_finite_energy_as_reloadable_value() {
        let mut result = sample_result("ceria", -55.0);
        result.energy = f64::INFINITY;
        result.converged = false;

        let record = EvaluationRecord::from(&result);
        let json = serde_json::to_string(&record).expect("serialize evaluation record");
        let decoded: EvaluationRecord =
            serde_json::from_str(&json).expect("deserialize evaluation record");

        assert_eq!(decoded.energy, f64::MAX);
        assert!(!decoded.converged);
    }

    #[test]
    fn evaluation_record_deserializes_legacy_null_energy() {
        let json = r#"{
          "label":"legacy",
          "energy":null,
          "converged":false,
          "structure":{
            "label":"legacy",
            "species":["Ce"],
            "fractional_coords":[[0.0,0.0,0.0]],
            "lattice":[[5.4,0.0,0.0],[0.0,5.4,0.0],[0.0,0.0,5.4]],
            "periodic_axes":[true,true,true]
          },
          "backend_run_dir":null,
          "primary_output_path":null
        }"#;

        let decoded: EvaluationRecord =
            serde_json::from_str(json).expect("deserialize legacy evaluation record");
        assert_eq!(decoded.energy, f64::MAX);
        assert!(!decoded.converged);
    }

    #[test]
    fn population_stays_sorted_by_energy() {
        let mut population = Population::default();
        population.insert(sample_result("high", -1.0), sample_candidate("high", 0.0));
        population.insert(sample_result("low", -5.0), sample_candidate("low", 0.1));
        population.insert(sample_result("mid", -3.0), sample_candidate("mid", 0.2));

        let labels: Vec<_> = population
            .members
            .iter()
            .map(|(_, candidate)| candidate.label.as_str())
            .collect();
        assert_eq!(labels, vec!["low", "mid", "high"]);
        assert_eq!(population.best().expect("best member").0.energy, -5.0);
    }

    #[test]
    fn worker_protocol_roundtrip_serializes_with_serde() {
        let response = WorkerResponse {
            request_id: "req-0001".into(),
            generation: Some(3),
            worker_slot: Some(1),
            outcome: WorkerOutcome::Failure {
                error: WorkerFailure {
                    kind: WorkerFailureKind::NotConverged,
                    message: "backend did not converge".into(),
                },
            },
        };

        let json = serde_json::to_string(&response).expect("serialize worker response");
        let decoded: WorkerResponse =
            serde_json::from_str(&json).expect("deserialize worker response");
        assert_eq!(decoded, response);
    }

    #[test]
    fn workflow_registry_resolves_by_id_and_route() {
        let by_id = workflow_by_id("ga.scott-monolithic").expect("workflow by id");
        let by_route = workflow_by_route("run-ga scott-monolithic").expect("workflow by route");

        assert_eq!(by_id, by_route);
        assert_eq!(by_id.expected_owner, Some("scott_monolithic_ga"));
    }

    #[test]
    fn staged_ga_workflow_records_adapter_templates() {
        let workflow = workflow_by_id("ga.scott-monolithic").expect("monolithic ga workflow");

        assert!(workflow
            .file_contracts
            .iter()
            .any(|entry| entry.path_pattern == "inputs/run.job"));
        assert!(workflow
            .file_contracts
            .iter()
            .any(|entry| entry.path_pattern == "inputs/Master.gin"));
    }
}
