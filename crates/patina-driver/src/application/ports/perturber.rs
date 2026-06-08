use anyhow::Result;

use patina_perturber::{
    ClusterStructure, DuplicateDecision, FingerprintDistance, FingerprintVector, PerturbationBatch,
    PerturbationConfig,
};

#[derive(Debug, Clone)]
pub struct ClusterPerturbationRequest {
    pub source: ClusterStructure,
    pub config: PerturbationConfig,
    pub count: usize,
}

pub trait ClusterPerturbationPort {
    fn generate_cluster_perturbations(
        &self,
        request: &ClusterPerturbationRequest,
    ) -> Result<PerturbationBatch>;
}

pub trait StructureFingerprintPort {
    fn fingerprint_structure(&self, structure: &ClusterStructure) -> Result<FingerprintVector>;
}

pub trait DuplicateScreeningPort {
    fn fingerprint_distance(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
    ) -> Result<FingerprintDistance>;

    fn classify_duplicate(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
        threshold: f64,
    ) -> Result<DuplicateDecision>;
}

pub trait ClusterPerturbationArtifactSink {
    fn persist_cluster_perturbation_run(
        &self,
        execution: &crate::application::cluster_perturbation::ClusterPerturbationExecution,
    ) -> Result<()>;
}
