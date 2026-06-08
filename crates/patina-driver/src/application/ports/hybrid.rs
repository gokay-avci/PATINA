use anyhow::Result;

use crate::application::ga_execution::RustJanusGaExecution;
use crate::application::hybrid_core::{
    HybridGaProductionExecution, HybridGaStageRequest, HybridProductionStageRequest,
    HybridSeedSelectionDecision, HybridSeedSelectionRequest,
};

pub trait HybridGaStagePort {
    fn execute_ga_stage(&self, request: &HybridGaStageRequest) -> Result<RustJanusGaExecution>;
}

pub trait HybridProductionStagePort {
    fn execute_production_stage(
        &self,
        request: &HybridProductionStageRequest,
    ) -> Result<crate::application::scott_production::ProductionWorkflowExecution>;
}

pub trait HybridSeedSelectionPort {
    fn select_seeds(
        &self,
        request: &HybridSeedSelectionRequest,
    ) -> Result<Vec<HybridSeedSelectionDecision>>;
}

pub trait HybridGaProductionArtifactSink {
    fn persist_hybrid_ga_production_run(
        &self,
        execution: &HybridGaProductionExecution,
    ) -> Result<()>;
}
