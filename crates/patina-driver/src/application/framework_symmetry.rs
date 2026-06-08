use anyhow::Result;
use patina_raspa::{PeriodicFramework, SymmetryAnalysis, SymmetryTolerance};
use patina_types::Candidate;
use serde::Serialize;

use super::ports::{
    FrameworkSymmetryArtifactSink, FrameworkSymmetryPort, FrameworkSymmetryRequest,
};

#[derive(Debug, Clone, Serialize)]
pub struct FrameworkSymmetryExecution {
    pub source_candidate: Candidate,
    pub framework: PeriodicFramework,
    pub tolerance: SymmetryTolerance,
    pub analysis: SymmetryAnalysis,
}

pub fn run_framework_symmetry_workflow(
    source_candidate: Candidate,
    tolerance: SymmetryTolerance,
    evaluator: &impl FrameworkSymmetryPort,
    sink: &impl FrameworkSymmetryArtifactSink,
) -> Result<FrameworkSymmetryExecution> {
    let framework = PeriodicFramework::try_from_candidate(&source_candidate)?;
    let analysis = evaluator.analyze_framework_symmetry(&FrameworkSymmetryRequest {
        framework: framework.clone(),
        tolerance,
    })?;
    let execution = FrameworkSymmetryExecution {
        source_candidate,
        framework,
        tolerance,
        analysis,
    };
    sink.persist_framework_symmetry_run(&execution)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::{run_framework_symmetry_workflow, FrameworkSymmetryExecution};
    use crate::application::ports::{
        FrameworkSymmetryArtifactSink, FrameworkSymmetryPort, FrameworkSymmetryRequest,
    };
    use anyhow::Result;
    use patina_raspa::{PeriodicFramework, SymmetryAnalysis, SymmetryTolerance};
    use patina_types::Candidate;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<String>>,
    }

    impl FrameworkSymmetryArtifactSink for RecordingSink {
        fn persist_framework_symmetry_run(
            &self,
            execution: &FrameworkSymmetryExecution,
        ) -> Result<()> {
            self.writes
                .lock()
                .expect("lock")
                .push(execution.source_candidate.label.clone());
            Ok(())
        }
    }

    struct StubAnalyzer;

    impl FrameworkSymmetryPort for StubAnalyzer {
        fn analyze_framework_symmetry(
            &self,
            request: &FrameworkSymmetryRequest,
        ) -> Result<SymmetryAnalysis> {
            Ok(SymmetryAnalysis {
                hall_number: Some(1),
                international_number: Some(1),
                hm_symbol: Some("P1".into()),
                primitive_lattice: Some(request.framework.lattice),
                reduced_lattice: Some(request.framework.lattice),
                symmetrized_atoms: request.framework.atoms.clone(),
                operations: Vec::new(),
                orbits: vec![0; request.framework.atoms.len()],
                wyckoff_letters: vec!["a".into(); request.framework.atoms.len()],
                standardized_framework: Some(request.framework.clone()),
                primitive_standardized_framework: Some(request.framework.clone()),
            })
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
    fn workflow_runs_analysis_and_persists_artifacts() {
        let sink = RecordingSink::default();
        let execution = run_framework_symmetry_workflow(
            sample_candidate(),
            SymmetryTolerance::default(),
            &StubAnalyzer,
            &sink,
        )
        .expect("workflow");

        assert_eq!(execution.analysis.international_number, Some(1));
        assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["framework"]);
    }

    #[test]
    fn workflow_converts_candidate_into_periodic_framework() {
        let sink = RecordingSink::default();
        let execution = run_framework_symmetry_workflow(
            sample_candidate(),
            SymmetryTolerance::default(),
            &StubAnalyzer,
            &sink,
        )
        .expect("workflow");

        let expected =
            PeriodicFramework::try_from_candidate(&sample_candidate()).expect("periodic");
        assert_eq!(execution.framework, expected);
    }
}
