use crate::{
    BackendEvaluator, EvalError, ExternalArtifacts, ExternalError, ExternalEvaluationMode,
    ExternalEvaluationOutcome, ExternalEvaluationRequest, ExternalEvaluationStatus,
    ExternalEvaluator, ExternalInputWriter, ExternalLaunchOutcome, ExternalLauncher,
    ExternalOutputParser, ExternalProgram,
};

/// Small composition helper for adapters that can be expressed as writer -> launcher -> parser.
#[derive(Debug, Clone)]
pub struct ComposedExternalEvaluator<W, L, P> {
    writer: W,
    launcher: L,
    parser: P,
}

impl<W, L, P> ComposedExternalEvaluator<W, L, P> {
    pub fn new(writer: W, launcher: L, parser: P) -> Self {
        Self {
            writer,
            launcher,
            parser,
        }
    }
}

impl<W, L, P> ExternalEvaluator for ComposedExternalEvaluator<W, L, P>
where
    W: ExternalInputWriter,
    L: ExternalLauncher,
    P: ExternalOutputParser,
{
    fn evaluate(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalEvaluationOutcome, ExternalError> {
        request.validate()?;
        let artifacts = self.writer.write_inputs(request)?;
        match self.launcher.launch(request, &artifacts)? {
            ExternalLaunchOutcome::Completed { run } => self.parser.parse_outputs(request, &run),
            ExternalLaunchOutcome::Submitted { run } => Ok(ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::Submitted,
                result: None,
                artifacts: run.artifacts.clone(),
                submitted_run: Some(run),
            }),
        }
    }
}

/// Compatibility adapter for existing backend evaluators during the `patina-external` migration.
///
/// This keeps the old `Candidate + workdir -> EvalResult` adapters usable behind the newer
/// external request/outcome port without moving workflow decisions into this crate.
#[derive(Debug, Clone)]
pub struct BackendEvaluatorExternalAdapter<B> {
    program: ExternalProgram,
    backend: B,
}

impl<B> BackendEvaluatorExternalAdapter<B> {
    pub fn new(program: ExternalProgram, backend: B) -> Self {
        Self { program, backend }
    }

    pub fn program(&self) -> ExternalProgram {
        self.program
    }

    pub fn inner(&self) -> &B {
        &self.backend
    }

    pub fn into_inner(self) -> B {
        self.backend
    }
}

impl<B> ExternalEvaluator for BackendEvaluatorExternalAdapter<B>
where
    B: BackendEvaluator,
{
    fn evaluate(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalEvaluationOutcome, ExternalError> {
        request.validate()?;

        if request.program != self.program {
            return Err(ExternalError::UnsupportedProgram {
                program: request.program,
                reason: format!("adapter is configured for `{}`", self.program.as_str()),
            });
        }

        if request.mode == ExternalEvaluationMode::ExportOnly {
            return Err(ExternalError::UnsupportedProgram {
                program: request.program,
                reason: "legacy backend adapters perform evaluations, not export-only staging"
                    .into(),
            });
        }

        let result = self
            .backend
            .evaluate(&request.candidate, &request.workdir)
            .map_err(|error| map_backend_eval_error(request.program, error))?;
        let status = if result.converged {
            ExternalEvaluationStatus::Converged
        } else {
            ExternalEvaluationStatus::NotConverged
        };

        Ok(ExternalEvaluationOutcome {
            status,
            result: Some(result),
            artifacts: ExternalArtifacts::new(&request.workdir),
            submitted_run: None,
        })
    }
}

fn map_backend_eval_error(program: ExternalProgram, error: EvalError) -> ExternalError {
    match error {
        EvalError::ProcessFailed { exit_code, stderr } => ExternalError::ProcessFailed {
            program,
            exit_code,
            stderr,
        },
        EvalError::ParseFailed { line, reason } => ExternalError::ParseFailed {
            program,
            reason: format!("line {line}: {reason}"),
        },
        EvalError::NotConverged {
            energy,
            n_steps,
            partial_result,
        } => ExternalError::NotConverged {
            program,
            energy,
            n_steps,
            partial_result,
        },
        EvalError::Timeout { elapsed } => ExternalError::Timeout { program, elapsed },
        EvalError::IoError(error) => ExternalError::Io(error),
        EvalError::TemplateInvalid { path, reason } => ExternalError::TemplateInvalid {
            program,
            path,
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::BackendEvaluatorExternalAdapter;
    use crate::{
        BackendEvaluator, CompletedExternalRun, ComposedExternalEvaluator, EvalError,
        ExternalArtifactClass, ExternalArtifactMaterialization, ExternalArtifactRetention,
        ExternalArtifacts, ExternalError, ExternalEvaluationMode, ExternalEvaluationOutcome,
        ExternalEvaluationRequest, ExternalEvaluationStatus, ExternalEvaluator,
        ExternalExecutionMode, ExternalInputWriter, ExternalLaunchOutcome, ExternalLauncher,
        ExternalOutputParser, ExternalProgram, ExternalTemplateSet, MockBackend,
        SubmittedExternalRun,
    };
    use patina_types::{Candidate, EvalResult};
    use std::path::Path;
    use std::time::Duration;

    fn periodic_candidate() -> Candidate {
        Candidate::fully_periodic(
            "mgo",
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]],
        )
    }

    struct FailingBackend(EvalError);

    impl BackendEvaluator for FailingBackend {
        fn evaluate(
            &self,
            _candidate: &Candidate,
            _workdir: &Path,
        ) -> Result<EvalResult, EvalError> {
            match &self.0 {
                EvalError::ProcessFailed { exit_code, stderr } => Err(EvalError::ProcessFailed {
                    exit_code: *exit_code,
                    stderr: stderr.clone(),
                }),
                EvalError::ParseFailed { line, reason } => Err(EvalError::ParseFailed {
                    line: *line,
                    reason: reason.clone(),
                }),
                EvalError::NotConverged {
                    energy,
                    n_steps,
                    partial_result,
                } => Err(EvalError::NotConverged {
                    energy: *energy,
                    n_steps: *n_steps,
                    partial_result: partial_result.clone(),
                }),
                EvalError::Timeout { elapsed } => Err(EvalError::Timeout { elapsed: *elapsed }),
                EvalError::IoError(error) => Err(EvalError::IoError(std::io::Error::new(
                    error.kind(),
                    error.to_string(),
                ))),
                EvalError::TemplateInvalid { path, reason } => Err(EvalError::TemplateInvalid {
                    path: path.clone(),
                    reason: reason.clone(),
                }),
            }
        }
    }

    fn request(program: ExternalProgram, candidate: Candidate) -> ExternalEvaluationRequest {
        ExternalEvaluationRequest {
            request_id: "req-1".into(),
            stage: 1,
            program,
            mode: ExternalEvaluationMode::Relaxation,
            execution: ExternalExecutionMode::LocalSubprocess,
            retrieve_relaxed_geometry: true,
            candidate,
            workdir: "/tmp/patina-external-test".into(),
            templates: ExternalTemplateSet::default(),
        }
    }

    struct StaticWriter;

    impl ExternalInputWriter for StaticWriter {
        fn write_inputs(
            &self,
            request: &ExternalEvaluationRequest,
        ) -> Result<ExternalArtifacts, ExternalError> {
            let mut artifacts = ExternalArtifacts::new(&request.workdir);
            artifacts.register_input(
                request.workdir.join("input.in"),
                ExternalArtifactClass::ControlInput,
                ExternalArtifactMaterialization::Rendered,
                ExternalArtifactRetention::RegenerableScratch,
            );
            artifacts.set_stdout_path(request.workdir.join("stdout.log"));
            artifacts.set_stderr_path_with_retention(
                request.workdir.join("stderr.log"),
                ExternalArtifactRetention::DurableOnFailure,
            );
            Ok(artifacts)
        }
    }

    enum LauncherMode {
        Completed,
        Submitted,
    }

    struct StaticLauncher {
        mode: LauncherMode,
    }

    impl ExternalLauncher for StaticLauncher {
        fn launch(
            &self,
            request: &ExternalEvaluationRequest,
            artifacts: &ExternalArtifacts,
        ) -> Result<ExternalLaunchOutcome, ExternalError> {
            match self.mode {
                LauncherMode::Completed => Ok(ExternalLaunchOutcome::Completed {
                    run: CompletedExternalRun {
                        program: request.program,
                        execution: request.execution,
                        artifacts: artifacts.clone(),
                        exit_code: Some(0),
                        elapsed: Duration::from_secs(2),
                    },
                }),
                LauncherMode::Submitted => Ok(ExternalLaunchOutcome::Submitted {
                    run: SubmittedExternalRun {
                        program: request.program,
                        execution: request.execution,
                        artifacts: artifacts.clone(),
                        launch_handle: "slurm:12345".into(),
                        remote_workdir: Some(artifacts.workdir.clone()),
                    },
                }),
            }
        }
    }

    struct StaticParser<'a> {
        parsed: &'a AtomicBool,
    }

    impl ExternalOutputParser for StaticParser<'_> {
        fn parse_outputs(
            &self,
            request: &ExternalEvaluationRequest,
            run: &CompletedExternalRun,
        ) -> Result<ExternalEvaluationOutcome, ExternalError> {
            self.parsed.store(true, Ordering::Relaxed);
            Ok(ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::Converged,
                result: Some(EvalResult {
                    relaxed_candidate: request.candidate.clone(),
                    energy: -1.0,
                    forces: vec![[0.0, 0.0, 0.0]; request.candidate.len()],
                    converged: true,
                    wall_time: Duration::from_secs(1),
                }),
                artifacts: run.artifacts.clone(),
                submitted_run: None,
            })
        }
    }

    #[test]
    fn backend_external_adapter_wraps_existing_backend_evaluator() {
        let adapter = BackendEvaluatorExternalAdapter::new(ExternalProgram::Gulp, MockBackend);
        let req = request(ExternalProgram::Gulp, periodic_candidate());

        let outcome = adapter
            .evaluate(&req)
            .expect("mock backend should evaluate through external port");
        let result = outcome.result.expect("evaluation result");

        assert_eq!(outcome.status, ExternalEvaluationStatus::Converged);
        assert_eq!(result.relaxed_candidate.label, "mgo");
        assert_eq!(outcome.artifacts.workdir, req.workdir);
        assert!(outcome.submitted_run.is_none());
    }

    #[test]
    fn backend_external_adapter_rejects_wrong_program() {
        let adapter = BackendEvaluatorExternalAdapter::new(ExternalProgram::Gulp, MockBackend);
        let req = request(ExternalProgram::JanusMace, periodic_candidate());

        assert!(matches!(
            adapter.evaluate(&req),
            Err(ExternalError::UnsupportedProgram { program, .. })
                if program == ExternalProgram::JanusMace
        ));
    }

    #[test]
    fn backend_external_adapter_maps_backend_errors() {
        let adapter = BackendEvaluatorExternalAdapter::new(
            ExternalProgram::Gulp,
            FailingBackend(EvalError::ParseFailed {
                line: 9,
                reason: "missing energy".into(),
            }),
        );
        let req = request(ExternalProgram::Gulp, periodic_candidate());

        assert!(matches!(
            adapter.evaluate(&req),
            Err(ExternalError::ParseFailed { program, reason })
                if program == ExternalProgram::Gulp && reason.contains("line 9")
        ));
    }

    #[test]
    fn composed_external_evaluator_returns_submitted_outcome_for_deferred_launch() {
        let parsed = AtomicBool::new(false);
        let evaluator = ComposedExternalEvaluator::new(
            StaticWriter,
            StaticLauncher {
                mode: LauncherMode::Submitted,
            },
            StaticParser { parsed: &parsed },
        );

        let mut req = request(ExternalProgram::Aims, periodic_candidate());
        req.execution = ExternalExecutionMode::SchedulerSubmitted;

        let outcome = evaluator
            .evaluate(&req)
            .expect("submitted launch should return a deferred outcome");

        assert_eq!(outcome.status, ExternalEvaluationStatus::Submitted);
        assert!(outcome.result.is_none());
        assert_eq!(
            outcome
                .submitted_run
                .as_ref()
                .map(|run| run.launch_handle.as_str()),
            Some("slurm:12345")
        );
        assert!(!parsed.load(Ordering::Relaxed));
    }

    #[test]
    fn composed_external_evaluator_parses_completed_launches() {
        let parsed = AtomicBool::new(false);
        let evaluator = ComposedExternalEvaluator::new(
            StaticWriter,
            StaticLauncher {
                mode: LauncherMode::Completed,
            },
            StaticParser { parsed: &parsed },
        );

        let outcome = evaluator
            .evaluate(&request(ExternalProgram::Gulp, periodic_candidate()))
            .expect("completed launch should parse");

        assert_eq!(outcome.status, ExternalEvaluationStatus::Converged);
        assert!(outcome.result.is_some());
        assert!(outcome.submitted_run.is_none());
        assert!(parsed.load(Ordering::Relaxed));
    }
}
