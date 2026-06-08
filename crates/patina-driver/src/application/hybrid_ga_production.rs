use anyhow::{anyhow, Result};
use std::cell::RefCell;

use super::ga_execution::RustJanusGaExecution;
use super::ga_workflow::{GaWorkflowRequest, GaWorkflowService};
use super::ports::{
    GaArtifactSink, GaEvaluationPort, HybridGaStagePort, HybridProductionStagePort,
    ProductionArtifactSink, ProductionEvaluationPort, ProductionIdentityPort,
    ProductionProgressPort,
};
use super::scott_production::{ProductionWorkflowExecution, ProductionWorkflowService};

#[allow(unused_imports)]
pub use super::hybrid_core::{
    build_hybrid_seed_candidate_id, EmulateBackedHybridSeedSelectionAdapter,
    EmulateBackedHybridSeedSelectionConfig, HybridAcceptedChild, HybridEmulateContext,
    HybridGaProductionExecution, HybridGaProductionRequest, HybridGaProductionSummary,
    HybridGaProductionWorkflowService, HybridGaStageRequest, HybridProductionStageRequest,
    HybridSeedCandidateMode, HybridSeedRankingBasis, HybridSeedSelectionCandidate,
    HybridSeedSelectionDecision, HybridSeedSelectionMode, HybridSeedSelectionRequest,
    HybridSelectedSeed, StaticAcquisitionGuidedHybridSeedSelectionPolicy,
    TopRankedHybridSeedSelectionPolicy,
};

#[derive(Debug, Clone)]
pub struct ServiceBackedHybridGaStageConfig {
    pub system: String,
    pub workdir: std::path::PathBuf,
    pub requested_generations: usize,
}

/// Service-backed adapter that exposes an existing GA workflow as the GA stage of a hybrid run.
///
/// The adapter is intentionally one-shot because the GA core setup owns consumable controller
/// start-state and should not be reused implicitly across orchestration calls.
pub struct ServiceBackedHybridGaStageAdapter<'a, E> {
    workflow_service: GaWorkflowService,
    core: RefCell<Option<super::rust_janus_ga::RustJanusGaCoreSetup>>,
    config: ServiceBackedHybridGaStageConfig,
    evaluator: &'a E,
    artifact_sink: Option<&'a dyn GaArtifactSink>,
}

impl<'a, E> ServiceBackedHybridGaStageAdapter<'a, E> {
    pub fn new(
        workflow_service: GaWorkflowService,
        core: super::rust_janus_ga::RustJanusGaCoreSetup,
        config: ServiceBackedHybridGaStageConfig,
        evaluator: &'a E,
        artifact_sink: Option<&'a dyn GaArtifactSink>,
    ) -> Self {
        Self {
            workflow_service,
            core: RefCell::new(Some(core)),
            config,
            evaluator,
            artifact_sink,
        }
    }
}

impl<E> HybridGaStagePort for ServiceBackedHybridGaStageAdapter<'_, E>
where
    E: GaEvaluationPort,
{
    fn execute_ga_stage(&self, request: &HybridGaStageRequest) -> Result<RustJanusGaExecution> {
        if request.system != self.config.system {
            return Err(anyhow!(
                "hybrid GA stage request system `{}` does not match configured GA args `{}`",
                request.system,
                self.config.system
            ));
        }
        if request.workdir != self.config.workdir {
            return Err(anyhow!(
                "hybrid GA stage request workdir `{}` does not match configured GA args `{}`",
                request.workdir.display(),
                self.config.workdir.display()
            ));
        }
        let core = self.core.borrow_mut().take().ok_or_else(|| {
            anyhow!("hybrid GA stage adapter has already consumed its core setup")
        })?;
        self.workflow_service.execute_with_generation_sink(
            GaWorkflowRequest {
                requested_generations: self.config.requested_generations,
            },
            core,
            self.evaluator,
            self.artifact_sink,
        )
    }
}

/// Service-backed adapter that exposes the production workflow service as the production stage
/// of a hybrid workflow without leaking production coordination into the orchestrator.
pub struct ServiceBackedHybridProductionStageAdapter<'a, E, I, P, A> {
    workflow_service: ProductionWorkflowService,
    evaluation_port: &'a E,
    identity_port: &'a I,
    progress_port: &'a P,
    artifact_sink: &'a A,
}

impl<'a, E, I, P, A> ServiceBackedHybridProductionStageAdapter<'a, E, I, P, A> {
    pub fn new(
        workflow_service: ProductionWorkflowService,
        evaluation_port: &'a E,
        identity_port: &'a I,
        progress_port: &'a P,
        artifact_sink: &'a A,
    ) -> Self {
        Self {
            workflow_service,
            evaluation_port,
            identity_port,
            progress_port,
            artifact_sink,
        }
    }
}

impl<E, I, P, A> HybridProductionStagePort
    for ServiceBackedHybridProductionStageAdapter<'_, E, I, P, A>
where
    E: ProductionEvaluationPort,
    I: ProductionIdentityPort,
    P: ProductionProgressPort,
    A: ProductionArtifactSink,
{
    fn execute_production_stage(
        &self,
        request: &HybridProductionStageRequest,
    ) -> Result<ProductionWorkflowExecution> {
        self.workflow_service.execute(
            &request.seeds,
            &request.system,
            &request.workdir,
            &request.production_config,
            request.restart_state_before.clone(),
            request.restart_counter_base,
            request.fallback_routing_policy.clone(),
            self.evaluation_port,
            self.identity_port,
            self.progress_port,
            self.artifact_sink,
        )
    }
}
