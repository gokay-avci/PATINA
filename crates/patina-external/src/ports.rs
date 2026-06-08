use crate::{
    CompletedExternalRun, ExternalArtifacts, ExternalError, ExternalEvaluationOutcome,
    ExternalEvaluationRequest, ExternalLaunchOutcome,
};

/// Synchronous port for rendering external-code inputs.
pub trait ExternalInputWriter: Send + Sync {
    fn write_inputs(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalArtifacts, ExternalError>;
}

/// Synchronous port for launching a prepared external-code job.
pub trait ExternalLauncher: Send + Sync {
    fn launch(
        &self,
        request: &ExternalEvaluationRequest,
        artifacts: &ExternalArtifacts,
    ) -> Result<ExternalLaunchOutcome, ExternalError>;
}

/// Synchronous port for parsing external-code outputs back into Rust evaluation data.
pub trait ExternalOutputParser: Send + Sync {
    fn parse_outputs(
        &self,
        request: &ExternalEvaluationRequest,
        run: &CompletedExternalRun,
    ) -> Result<ExternalEvaluationOutcome, ExternalError>;
}

/// Synchronous adapter boundary consumed by application orchestration.
pub trait ExternalEvaluator: Send + Sync {
    fn evaluate(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalEvaluationOutcome, ExternalError>;
}
