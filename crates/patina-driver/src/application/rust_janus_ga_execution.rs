use super::ga_execution::RustJanusGaExecution;
use super::ga_workflow::{GaWorkflowRequest, GaWorkflowService};
use super::rust_janus_ga::RustJanusGaSetup;
use super::rust_janus_ga_artifacts::{
    RustJanusIncrementalArtifactMetadata, RustJanusIncrementalArtifactSink,
};
use super::scott_ga_runtime::PersistentPoolGaEvaluator;
use super::workflow_policy::GaWorkflowPolicy;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct RustJanusGaExecutionRequest {
    pub requested_generations: usize,
}

pub fn execute_rust_janus_ga_search(
    setup: RustJanusGaSetup,
    request: RustJanusGaExecutionRequest,
    artifact_metadata: RustJanusIncrementalArtifactMetadata<'_>,
) -> Result<RustJanusGaExecution> {
    let evaluator = PersistentPoolGaEvaluator::new(&setup.pool);
    let service = GaWorkflowService::new(GaWorkflowPolicy::rust_janus());
    let artifact_sink = RustJanusIncrementalArtifactSink {
        metadata: artifact_metadata,
    };
    let execution = service.execute_with_generation_sink(
        GaWorkflowRequest {
            requested_generations: request.requested_generations,
        },
        setup.core,
        &evaluator,
        Some(&artifact_sink),
    )?;
    Ok(execution)
}
