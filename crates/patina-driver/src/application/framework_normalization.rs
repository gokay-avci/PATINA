use anyhow::Result;
use patina_raspa::{PeriodicFramework, SymmetryTolerance};
use patina_types::Candidate;
use serde::Serialize;

use super::ports::{
    FrameworkNormalizationArtifactSink, FrameworkNormalizationPort, FrameworkNormalizationRequest,
    FrameworkNormalizationTarget,
};

#[derive(Debug, Clone, Serialize)]
pub struct FrameworkNormalizationExecution {
    pub source_candidate: Candidate,
    pub framework: PeriodicFramework,
    pub tolerance: SymmetryTolerance,
    pub target: FrameworkNormalizationTarget,
    pub normalized_framework: PeriodicFramework,
}

pub fn run_framework_normalization_workflow(
    source_candidate: Candidate,
    tolerance: SymmetryTolerance,
    target: FrameworkNormalizationTarget,
    evaluator: &impl FrameworkNormalizationPort,
    sink: &impl FrameworkNormalizationArtifactSink,
) -> Result<FrameworkNormalizationExecution> {
    let framework = PeriodicFramework::try_from_candidate(&source_candidate)?;
    let normalized_framework = evaluator.normalize_framework(&FrameworkNormalizationRequest {
        framework: framework.clone(),
        tolerance,
        target,
    })?;
    let execution = FrameworkNormalizationExecution {
        source_candidate,
        framework,
        tolerance,
        target,
        normalized_framework,
    };
    sink.persist_framework_normalization_run(&execution)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::{run_framework_normalization_workflow, FrameworkNormalizationExecution};
    use crate::application::ports::{
        FrameworkNormalizationArtifactSink, FrameworkNormalizationPort,
        FrameworkNormalizationRequest, FrameworkNormalizationTarget,
    };
    use anyhow::Result;
    use patina_raspa::{PeriodicFramework, SymmetryTolerance};
    use patina_types::Candidate;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<String>>,
    }

    impl FrameworkNormalizationArtifactSink for RecordingSink {
        fn persist_framework_normalization_run(
            &self,
            execution: &FrameworkNormalizationExecution,
        ) -> Result<()> {
            self.writes
                .lock()
                .expect("lock")
                .push(execution.normalized_framework.label.clone());
            Ok(())
        }
    }

    struct StubNormalizer;

    impl FrameworkNormalizationPort for StubNormalizer {
        fn normalize_framework(
            &self,
            request: &FrameworkNormalizationRequest,
        ) -> Result<PeriodicFramework> {
            let mut framework = request.framework.clone();
            framework.label = match request.target {
                FrameworkNormalizationTarget::Standardized => "standardized".into(),
                FrameworkNormalizationTarget::PrimitiveStandardized => {
                    "primitive_standardized".into()
                }
            };
            Ok(framework)
        }
    }

    fn sample_candidate() -> Candidate {
        Candidate::fully_periodic(
            "framework",
            vec!["Mg".into()],
            vec![[0.0, 0.0, 0.0]],
            [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]],
        )
    }

    #[test]
    fn workflow_normalizes_and_persists_standardized_target() {
        let sink = RecordingSink::default();
        let execution = run_framework_normalization_workflow(
            sample_candidate(),
            SymmetryTolerance::default(),
            FrameworkNormalizationTarget::Standardized,
            &StubNormalizer,
            &sink,
        )
        .expect("workflow");

        assert_eq!(execution.normalized_framework.label, "standardized");
        assert_eq!(
            sink.writes.lock().expect("lock").as_slice(),
            ["standardized"]
        );
    }
}
