use anyhow::Result;
use std::path::PathBuf;

use patina_evaluator::{ScottBackendMode, ScottEvalOutcome, ScottProcedurePlan};
use patina_runtime::ScottBackendRoutingPolicy;
use patina_types::Candidate;

use crate::application::scott_production::{ProductionDecision, ProductionRestartState};

#[derive(Debug, Clone)]
pub struct ProductionEvaluationRequest {
    pub index: usize,
    pub seed_counter: usize,
    pub candidate: Candidate,
    pub candidate_workdir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProductionEvaluationOutput {
    pub default_backend: ScottBackendMode,
    pub routing_policy: ScottBackendRoutingPolicy,
    pub procedure_plan: ScottProcedurePlan,
    pub outcome: ScottEvalOutcome,
}

#[derive(Debug, Clone)]
pub struct ProductionAcceptedArtifactRecord {
    pub index: usize,
    pub system: String,
    pub candidate: Candidate,
    pub candidate_workdir: PathBuf,
    pub default_backend: ScottBackendMode,
    pub routing_policy: ScottBackendRoutingPolicy,
    pub procedure_plan: ScottProcedurePlan,
    pub outcome: ScottEvalOutcome,
    pub decision: ProductionDecision,
    pub final_hashkey: Option<String>,
    pub best_set_rank: Option<usize>,
}

pub trait ProductionEvaluationPort {
    fn evaluate_candidate(
        &self,
        request: &ProductionEvaluationRequest,
    ) -> Result<ProductionEvaluationOutput>;
}

pub trait ProductionIdentityPort {
    fn build_input_hashkey(&self, request: &ProductionEvaluationRequest) -> Result<Option<String>>;

    fn build_final_hashkey(
        &self,
        request: &ProductionEvaluationRequest,
        evaluated: &ProductionEvaluationOutput,
        fallback_hashkey: Option<&str>,
    ) -> Result<Option<String>>;
}

pub trait ProductionProgressPort {
    fn mark_seed_consumed(
        &self,
        source_name: Option<&str>,
        restart_state: Option<&ProductionRestartState>,
    ) -> Result<()>;
}

pub trait ProductionArtifactSink {
    fn persist_accepted_candidate(&self, artifact: &ProductionAcceptedArtifactRecord)
        -> Result<()>;
}
