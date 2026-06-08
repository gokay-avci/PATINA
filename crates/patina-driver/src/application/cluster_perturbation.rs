use anyhow::Result;
use patina_perturber::{
    ClusterStructure, DuplicateDecision, FingerprintDistance, FingerprintVector, PerturbationBatch,
    PerturbationConfig,
};
use patina_types::Candidate;
use serde::Serialize;

use super::ports::{
    ClusterPerturbationArtifactSink, ClusterPerturbationPort, ClusterPerturbationRequest,
    DuplicateScreeningPort, StructureFingerprintPort,
};

#[derive(Debug, Clone, Serialize)]
pub struct ClusterVariantAnalysis {
    pub variant: ClusterStructure,
    pub fingerprint: FingerprintVector,
    pub distance_to_source: FingerprintDistance,
    pub duplicate_vs_source: DuplicateDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterPerturbationExecution {
    pub source_candidate: Candidate,
    pub source_cluster: ClusterStructure,
    pub config: PerturbationConfig,
    pub count: usize,
    pub duplicate_threshold: f64,
    pub source_fingerprint: FingerprintVector,
    pub batch: PerturbationBatch,
    pub variant_analyses: Vec<ClusterVariantAnalysis>,
}

pub struct ClusterPerturbationWorkflowRequest {
    pub source_candidate: Candidate,
    pub config: PerturbationConfig,
    pub count: usize,
    pub duplicate_threshold: f64,
}

pub fn run_cluster_perturbation_workflow(
    request: ClusterPerturbationWorkflowRequest,
    perturbation_port: &impl ClusterPerturbationPort,
    fingerprint_port: &impl StructureFingerprintPort,
    duplicate_port: &impl DuplicateScreeningPort,
    sink: &impl ClusterPerturbationArtifactSink,
) -> Result<ClusterPerturbationExecution> {
    let ClusterPerturbationWorkflowRequest {
        source_candidate,
        config,
        count,
        duplicate_threshold,
    } = request;
    let source_cluster = ClusterStructure::try_from_candidate(&source_candidate)?;
    let batch = perturbation_port.generate_cluster_perturbations(&ClusterPerturbationRequest {
        source: source_cluster.clone(),
        config,
        count,
    })?;
    let source_fingerprint = fingerprint_port.fingerprint_structure(&source_cluster)?;
    let variant_analyses = batch
        .variants
        .iter()
        .map(|variant| {
            Ok(ClusterVariantAnalysis {
                variant: variant.clone(),
                fingerprint: fingerprint_port.fingerprint_structure(variant)?,
                distance_to_source: duplicate_port
                    .fingerprint_distance(&source_cluster, variant)?,
                duplicate_vs_source: duplicate_port.classify_duplicate(
                    &source_cluster,
                    variant,
                    duplicate_threshold,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let execution = ClusterPerturbationExecution {
        source_candidate,
        source_cluster,
        config,
        count,
        duplicate_threshold,
        source_fingerprint,
        batch,
        variant_analyses,
    };
    sink.persist_cluster_perturbation_run(&execution)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::{
        run_cluster_perturbation_workflow, ClusterPerturbationExecution,
        ClusterPerturbationWorkflowRequest,
    };
    use crate::application::ports::{
        ClusterPerturbationArtifactSink, ClusterPerturbationPort, ClusterPerturbationRequest,
        DuplicateScreeningPort, StructureFingerprintPort,
    };
    use anyhow::Result;
    use patina_perturber::{
        ClusterAtom, ClusterStructure, DuplicateDecision, FingerprintDistance, FingerprintVector,
        PerturbationBatch, PerturbationConfig,
    };
    use patina_types::Candidate;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<String>>,
    }

    impl ClusterPerturbationArtifactSink for RecordingSink {
        fn persist_cluster_perturbation_run(
            &self,
            execution: &ClusterPerturbationExecution,
        ) -> Result<()> {
            self.writes
                .lock()
                .expect("lock")
                .push(execution.source_candidate.label.clone());
            Ok(())
        }
    }

    struct StubPerturbationPort;

    impl ClusterPerturbationPort for StubPerturbationPort {
        fn generate_cluster_perturbations(
            &self,
            request: &ClusterPerturbationRequest,
        ) -> Result<PerturbationBatch> {
            let mut variant = request.source.clone();
            variant.label = "cluster__perturbed_0000".into();
            variant.atoms[0].cartesian[0] += 0.25;
            Ok(PerturbationBatch {
                source: request.source.clone(),
                variants: vec![variant],
            })
        }
    }

    struct StubFingerprintPort;

    impl StructureFingerprintPort for StubFingerprintPort {
        fn fingerprint_structure(&self, structure: &ClusterStructure) -> Result<FingerprintVector> {
            Ok(FingerprintVector {
                values: vec![
                    structure.atoms.len() as f64,
                    structure.atoms[0].cartesian[0],
                ],
            })
        }
    }

    struct StubDuplicatePort;

    impl DuplicateScreeningPort for StubDuplicatePort {
        fn fingerprint_distance(
            &self,
            left: &ClusterStructure,
            right: &ClusterStructure,
        ) -> Result<FingerprintDistance> {
            Ok(FingerprintDistance {
                left_label: left.label.clone(),
                right_label: right.label.clone(),
                distance: (right.atoms[0].cartesian[0] - left.atoms[0].cartesian[0]).abs(),
            })
        }

        fn classify_duplicate(
            &self,
            left: &ClusterStructure,
            right: &ClusterStructure,
            threshold: f64,
        ) -> Result<DuplicateDecision> {
            let distance = self.fingerprint_distance(left, right)?.distance;
            Ok(DuplicateDecision {
                left_label: left.label.clone(),
                right_label: right.label.clone(),
                distance,
                threshold,
                duplicate: distance < threshold,
            })
        }
    }

    fn sample_candidate() -> Candidate {
        Candidate::cluster(
            "cluster",
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [1.5, 0.0, 0.0]],
        )
    }

    #[test]
    fn workflow_generates_and_persists_cluster_variants() {
        let sink = RecordingSink::default();
        let execution = run_cluster_perturbation_workflow(
            ClusterPerturbationWorkflowRequest {
                source_candidate: sample_candidate(),
                config: PerturbationConfig {
                    sigma: 0.2,
                    max_displacement: Some(0.3),
                    validate_min_distance: None,
                    seed: Some(7),
                    ..PerturbationConfig::default()
                },
                count: 1,
                duplicate_threshold: 0.1,
            },
            &StubPerturbationPort,
            &StubFingerprintPort,
            &StubDuplicatePort,
            &sink,
        )
        .expect("workflow");

        assert_eq!(execution.batch.variants.len(), 1);
        assert_eq!(execution.variant_analyses.len(), 1);
        assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["cluster"]);
    }

    #[test]
    fn workflow_converts_candidate_into_cluster_structure() {
        let sink = RecordingSink::default();
        let execution = run_cluster_perturbation_workflow(
            ClusterPerturbationWorkflowRequest {
                source_candidate: sample_candidate(),
                config: PerturbationConfig {
                    sigma: 0.2,
                    max_displacement: None,
                    validate_min_distance: None,
                    seed: Some(11),
                    ..PerturbationConfig::default()
                },
                count: 1,
                duplicate_threshold: 0.05,
            },
            &StubPerturbationPort,
            &StubFingerprintPort,
            &StubDuplicatePort,
            &sink,
        )
        .expect("workflow");

        assert_eq!(
            execution.source_cluster,
            ClusterStructure {
                label: "cluster".into(),
                atoms: vec![
                    ClusterAtom {
                        species: "Mg".into(),
                        cartesian: [0.0, 0.0, 0.0],
                    },
                    ClusterAtom {
                        species: "O".into(),
                        cartesian: [1.5, 0.0, 0.0],
                    },
                ],
            }
        );
    }
}
