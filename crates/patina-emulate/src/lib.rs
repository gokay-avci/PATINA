#![forbid(unsafe_code)]

use anyhow::Result;
use patina_types::{EvaluationRecord, SearchConfig, StructureRecord};
use serde::{Deserialize, Serialize};

mod features;
mod surrogate_runtime;

pub use features::*;
pub use surrogate_runtime::*;

/// Stable campaign identifier for long-lived emulate/orchestration state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CampaignId(pub String);

/// Stable branch identifier within one emulate campaign.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BranchId(pub String);

/// High-level scientific objective of a campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScientificObjective {
    ClusterGlobalMinimum,
    CrystalStructurePrediction,
    SurfaceSupportedClusterSearch,
    MixedWorkflowExploration,
}

/// Seed provenance admitted into an emulate campaign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeedSource {
    CandidateJson { path: String },
    RestartDirectory { path: String },
    PriorBranch { branch_id: BranchId },
    ImportedLibrary { label: String },
}

/// Controller family exposed as an asynchronous branch type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchFamily {
    GeneticAlgorithm,
    ParticleSwarm,
    BasinHopping,
    SimulatedAnnealing,
    EnergyLid,
    ExploitLocalRefinement,
    ExplorationRandomized,
}

/// Top-level campaign branching strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchPlan {
    SingleBranch,
    ParallelFamilyPortfolio,
    AdaptiveLearningPortfolio,
}

/// Generic scheduler hints for a branch lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchSchedulerConfig {
    pub max_concurrency: usize,
    pub checkpoint_interval: usize,
    pub allow_child_spawns: bool,
}

impl Default for BranchSchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 1,
            checkpoint_interval: 1,
            allow_child_spawns: false,
        }
    }
}

/// Learning/active-learning binding mode for one branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnerBinding {
    None,
    ObserveOnly,
    AcquisitionGuided,
}

/// Direction of scientific traffic across the emulate boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDirection {
    Upstream,
    Downstream,
}

/// High-level learning or control objective requested from an emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningObjective {
    FitSurrogate,
    ScoreCandidates,
    ReduceUncertainty,
    ProposeCandidates,
    MultiHeadPrediction,
    MultiFidelityThetaLearning,
}

/// Fidelity class associated with one observation, target, or request lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityClass {
    DescriptorOnly,
    GulpReference,
    JanusMaceLow,
    JanusMaceHigh,
    ReferenceDft,
    MixedEvidence,
}

/// Prediction-head family emitted or consumed by an emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredictionHeadKind {
    Scalar,
    Forces,
    Vector,
    MultiHeadScalar,
}

/// Typed target contract for one emulator head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionTarget {
    pub name: String,
    pub head_kind: PredictionHeadKind,
    pub fidelity: FidelityClass,
    pub unit: Option<String>,
}

/// Descriptor or feature representation contract carried with emulate traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRepresentation {
    pub family: String,
    pub version: String,
    pub feature_names: Vec<String>,
    pub provenance_label: String,
}

/// Campaign bootstrap separated from controller/evaluator-specific setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampaignBootstrap {
    pub campaign_id: CampaignId,
    pub objective: ScientificObjective,
    pub seed_sources: Vec<SeedSource>,
    pub branch_plan: BranchPlan,
    pub resume_checkpoint: Option<String>,
}

/// Branch bootstrap detached from runtime backend concerns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchBootstrap {
    pub branch_id: BranchId,
    pub family: BranchFamily,
    pub parent: Option<BranchId>,
    pub scheduler: BranchSchedulerConfig,
    pub controller: ControllerBootstrap,
    pub evaluator: EvaluatorBootstrap,
    pub learner_binding: LearnerBinding,
}

/// Family-specific controller bootstrap payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum ControllerBootstrap {
    GeneticAlgorithm(GeneticBootstrap),
    ParticleSwarm(ParticleSwarmBootstrap),
    BasinHopping(SamplingBootstrap),
    SimulatedAnnealing(SamplingBootstrap),
    EnergyLid(EnergyLidBootstrap),
}

/// Genetic algorithm bootstrap carried independently of backend selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneticBootstrap {
    pub search: SearchConfig,
    pub resume_generation: Option<usize>,
}

/// Particle-swarm bootstrap for future branch families.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleSwarmBootstrap {
    pub particle_count: usize,
    pub iterations: usize,
}

/// Generic Monte-Carlo-style bootstrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplingBootstrap {
    pub steps: usize,
    pub temperature: f64,
    pub step_size: f64,
}

/// Energy-lid-specific bootstrap payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyLidBootstrap {
    pub lids: usize,
    pub threshold: f64,
    pub increment: f64,
    pub runners_per_level: usize,
}

/// Backend/evaluator bootstrap detached from controller mechanics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatorBootstrap {
    pub backend_family: BackendFamily,
    pub routing_policy_ref: Option<String>,
    pub artifact_policy: ArtifactPolicy,
}

/// Evaluator family visible to emulate orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendFamily {
    GulpLikeReference,
    JanusMace,
    MixedRouting,
}

/// Artifact retention posture for one branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactPolicy {
    Minimal,
    KeepAcceptedOnly,
    KeepAll,
}

/// Outcome class emitted by controller/evaluator observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationOutcome {
    Accepted,
    RejectedDuplicate,
    RejectedGeometry,
    EvaluationFailed,
    ScheduledOnly,
}

/// Provenance attached to one observation emitted by a branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationProvenance {
    pub branch_id: BranchId,
    pub family: BranchFamily,
    pub source_label: String,
    pub step: Option<usize>,
    pub generation: Option<usize>,
    pub fidelity: Option<FidelityClass>,
    pub descriptor_provenance: Option<String>,
}

/// Shared append-only observation record for learning/orchestration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateObservation {
    pub campaign_id: CampaignId,
    pub branch_id: BranchId,
    pub controller_family: BranchFamily,
    pub source_candidate: StructureRecord,
    pub evaluated_candidate: Option<EvaluationRecord>,
    pub outcome: ObservationOutcome,
    pub provenance: ObservationProvenance,
    pub feature_representation: Option<FeatureRepresentation>,
    pub feature_vector_ref: Option<String>,
}

/// Ranked learner guidance fed back into branch scheduling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcquisitionRecord {
    pub branch_id: BranchId,
    pub candidate_id: String,
    pub rank: usize,
    pub direction: TaskDirection,
    pub objective: LearningObjective,
    pub acquisition_score: f64,
    pub novelty_score: f64,
    pub uncertainty_score: f64,
    pub expected_improvement: Option<f64>,
    pub recommended_family: Option<BranchFamily>,
}

/// Port for storing campaign-level state snapshots.
pub trait CampaignStateStorePort {
    fn save_campaign_bootstrap(&self, bootstrap: &CampaignBootstrap);
}

/// Port for storing branch-level state snapshots.
pub trait BranchStateStorePort {
    fn save_branch_bootstrap(&self, bootstrap: &BranchBootstrap);
}

/// Port for append-only observation ingestion.
pub trait ObservationStorePort {
    fn append_observation(&self, observation: &CandidateObservation);
}

/// Port for learner/acquisition policy execution.
pub trait AcquisitionPolicyPort {
    fn score_observations(
        &self,
        campaign: &CampaignBootstrap,
        observations: &[CandidateObservation],
    ) -> Vec<AcquisitionRecord>;
}

/// Port for projecting structures or observations into emulator-ready feature spaces.
pub trait FeatureProjectionPort {
    fn derive_representation(
        &self,
        structures: &[StructureRecord],
    ) -> Result<FeatureRepresentation>;

    fn project_structure(
        &self,
        structure: &StructureRecord,
        representation: &FeatureRepresentation,
    ) -> Result<Vec<f64>>;
}

/// Port for training or updating emulator state from upstream evidence.
pub trait SurrogateTrainingPort {
    type FitSummary;

    fn fit_observations(&self, observations: &[CandidateObservation]) -> Self::FitSummary;
}

/// Port for translating scored or uncertainty-ranked candidates into branch proposals.
pub trait CandidateProposalPort {
    fn propose_from_acquisition(&self, scored: &[AcquisitionRecord]) -> Vec<String>;
}

/// Port for mapping candidates or observations onto fidelity lanes.
pub trait FidelityRoutingPort {
    fn select_fidelity(
        &self,
        branch: &BranchBootstrap,
        observation: Option<&CandidateObservation>,
        scored: &AcquisitionRecord,
    ) -> FidelityClass;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_bootstrap_keeps_controller_and_evaluator_separate() {
        let branch = BranchBootstrap {
            branch_id: BranchId("branch-ga-1".into()),
            family: BranchFamily::GeneticAlgorithm,
            parent: None,
            scheduler: BranchSchedulerConfig::default(),
            controller: ControllerBootstrap::GeneticAlgorithm(GeneticBootstrap {
                search: SearchConfig {
                    temperature: 10.0,
                    step_size: 0.1,
                    population_size: 12,
                    max_steps: 5,
                    seed: Some(7),
                },
                resume_generation: None,
            }),
            evaluator: EvaluatorBootstrap {
                backend_family: BackendFamily::JanusMace,
                routing_policy_ref: Some("janus_default".into()),
                artifact_policy: ArtifactPolicy::KeepAcceptedOnly,
            },
            learner_binding: LearnerBinding::ObserveOnly,
        };

        assert!(matches!(
            branch.controller,
            ControllerBootstrap::GeneticAlgorithm(_)
        ));
        assert_eq!(branch.evaluator.backend_family, BackendFamily::JanusMace);
    }

    #[test]
    fn acquisition_record_carries_direction_and_objective() {
        let record = AcquisitionRecord {
            branch_id: BranchId("branch-ga-1".into()),
            candidate_id: "cand-1".into(),
            rank: 1,
            direction: TaskDirection::Downstream,
            objective: LearningObjective::ReduceUncertainty,
            acquisition_score: 0.4,
            novelty_score: 0.2,
            uncertainty_score: 0.3,
            expected_improvement: Some(0.1),
            recommended_family: Some(BranchFamily::GeneticAlgorithm),
        };

        assert_eq!(record.direction, TaskDirection::Downstream);
        assert_eq!(record.objective, LearningObjective::ReduceUncertainty);
    }
}
