use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::backend_adapters::write_candidate_xyz;
use crate::{
    CrystalRunIntent, CrystalTerminalOutputContract, ExternalArtifactClass,
    ExternalArtifactMaterialization, ExternalArtifactRetention, ExternalArtifacts, ExternalError,
    ExternalEvaluationMode, ExternalEvaluationOutcome, ExternalEvaluationRequest,
    ExternalEvaluationStatus, ExternalEvaluator, ExternalExecutionMode, ExternalProgram,
    SubmittedExternalRun,
};

/// Configures the first typed CRYSTAL adapter surface.
///
/// This adapter stages CRYSTAL input artifacts and deferred-launch metadata, but intentionally
/// does not execute or parse terminal scientific results until representative CRYSTAL output
/// fixtures are available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrystalExternalAdapterConfig {
    pub template_path: PathBuf,
    pub executable: PathBuf,
    pub input_name: String,
    pub output_name: String,
    pub stderr_name: String,
    pub structure_name: String,
    pub properties_output_name: Option<String>,
    pub run_intent: CrystalRunIntent,
    pub timeout: Option<Duration>,
}

impl CrystalExternalAdapterConfig {
    pub fn new(template_path: impl Into<PathBuf>, executable: impl Into<PathBuf>) -> Self {
        Self {
            template_path: template_path.into(),
            executable: executable.into(),
            input_name: "crystal.d12".into(),
            output_name: "crystal.out".into(),
            stderr_name: "crystal.stderr.log".into(),
            structure_name: "candidate.extxyz".into(),
            properties_output_name: None,
            run_intent: CrystalRunIntent::Unknown,
            timeout: None,
        }
    }

    pub fn with_io_names(
        mut self,
        input_name: impl Into<String>,
        output_name: impl Into<String>,
    ) -> Self {
        self.input_name = input_name.into();
        self.output_name = output_name.into();
        self
    }

    pub fn with_stderr_name(mut self, stderr_name: impl Into<String>) -> Self {
        self.stderr_name = stderr_name.into();
        self
    }

    pub fn with_structure_name(mut self, structure_name: impl Into<String>) -> Self {
        self.structure_name = structure_name.into();
        self
    }

    pub fn with_properties_output_name(mut self, output_name: impl Into<String>) -> Self {
        self.properties_output_name = Some(output_name.into());
        self
    }

    pub fn with_run_intent(mut self, run_intent: CrystalRunIntent) -> Self {
        self.run_intent = run_intent;
        self
    }

    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
}

/// First CRYSTAL adapter for staging and deferred launch.
#[derive(Debug, Clone)]
pub struct CrystalExternalAdapter {
    cfg: CrystalExternalAdapterConfig,
}

impl CrystalExternalAdapter {
    pub fn new(cfg: CrystalExternalAdapterConfig) -> Result<Self, ExternalError> {
        if !cfg.template_path.exists() {
            return Err(ExternalError::TemplateInvalid {
                program: ExternalProgram::Crystal,
                path: cfg.template_path.clone(),
                reason: "CRYSTAL template file does not exist".into(),
            });
        }
        Ok(Self { cfg })
    }

    pub fn terminal_output_contract(&self) -> CrystalTerminalOutputContract {
        let mut contract =
            CrystalTerminalOutputContract::new(&self.cfg.output_name, &self.cfg.stderr_name)
                .with_run_intent(self.cfg.run_intent);
        if let Some(output_name) = &self.cfg.properties_output_name {
            contract = contract.with_properties_output_name(output_name);
        }
        contract
    }

    fn stage_inputs(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalArtifacts, ExternalError> {
        if request.program != ExternalProgram::Crystal {
            return Err(ExternalError::UnsupportedProgram {
                program: request.program,
                reason: "CrystalExternalAdapter only supports the `crystal` program".into(),
            });
        }

        fs::create_dir_all(&request.workdir)?;
        let input_path = request.workdir.join(&self.cfg.input_name);
        let output_path = request.workdir.join(&self.cfg.output_name);
        let stderr_path = request.workdir.join(&self.cfg.stderr_name);
        let structure_path = request.workdir.join(&self.cfg.structure_name);

        fs::copy(&self.cfg.template_path, &input_path)?;
        write_candidate_xyz(&request.candidate, &structure_path).map_err(map_eval_error)?;

        let mut artifacts = ExternalArtifacts::new(&request.workdir);
        artifacts.register_input(
            input_path,
            ExternalArtifactClass::TemplateInput,
            ExternalArtifactMaterialization::Copied,
            ExternalArtifactRetention::RegenerableScratch,
        );
        artifacts.register_input(
            structure_path,
            ExternalArtifactClass::StructureInput,
            ExternalArtifactMaterialization::Generated,
            ExternalArtifactRetention::RegenerableScratch,
        );
        artifacts.register_output(
            output_path.clone(),
            ExternalArtifactClass::PrimaryOutput,
            ExternalArtifactRetention::DurableOnTerminalState,
        );
        artifacts.set_stdout_path(output_path);
        artifacts.set_stderr_path_with_retention(
            stderr_path,
            ExternalArtifactRetention::DurableOnFailure,
        );

        if let Some(output_name) = &self.cfg.properties_output_name {
            artifacts.register_output(
                request.workdir.join(output_name),
                ExternalArtifactClass::AuxiliaryOutput,
                ExternalArtifactRetention::DurableOnTerminalState,
            );
        }

        Ok(artifacts)
    }

    fn submitted_handle(&self, request: &ExternalEvaluationRequest) -> String {
        format!(
            "submitted:{}:{}:{}",
            request.program.as_str(),
            request.request_id,
            request.stage
        )
    }
}

impl ExternalEvaluator for CrystalExternalAdapter {
    fn evaluate(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalEvaluationOutcome, ExternalError> {
        request.validate()?;
        let artifacts = self.stage_inputs(request)?;

        if request.mode == ExternalEvaluationMode::ExportOnly {
            return Ok(ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::ExportedOnly,
                result: None,
                artifacts,
                submitted_run: None,
            });
        }

        match request.execution {
            ExternalExecutionMode::SchedulerSubmitted => Ok(ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::Submitted,
                result: None,
                submitted_run: Some(SubmittedExternalRun {
                    program: request.program,
                    execution: request.execution,
                    artifacts: artifacts.clone(),
                    launch_handle: self.submitted_handle(request),
                    remote_workdir: Some(artifacts.workdir.clone()),
                }),
                artifacts,
            }),
            ExternalExecutionMode::RemoteBatch => Err(ExternalError::UnsupportedProgram {
                program: request.program,
                reason: "remote batch launch is not implemented for CRYSTAL yet".into(),
            }),
            ExternalExecutionMode::LocalSubprocess | ExternalExecutionMode::LocalScript => {
                let _ = &self.cfg.executable;
                let _ = self.cfg.timeout;
                Err(ExternalError::UnsupportedProgram {
                    program: request.program,
                    reason: "terminal CRYSTAL execution is blocked until output parsing and result mapping are specified from representative fixtures".into(),
                })
            }
        }
    }
}

fn map_eval_error(error: crate::EvalError) -> ExternalError {
    match error {
        crate::EvalError::ProcessFailed { exit_code, stderr } => ExternalError::ProcessFailed {
            program: ExternalProgram::Crystal,
            exit_code,
            stderr,
        },
        crate::EvalError::ParseFailed { reason, .. } => ExternalError::ParseFailed {
            program: ExternalProgram::Crystal,
            reason,
        },
        crate::EvalError::NotConverged {
            energy,
            n_steps,
            partial_result,
        } => ExternalError::NotConverged {
            program: ExternalProgram::Crystal,
            energy,
            n_steps,
            partial_result,
        },
        crate::EvalError::Timeout { elapsed } => ExternalError::Timeout {
            program: ExternalProgram::Crystal,
            elapsed,
        },
        crate::EvalError::IoError(error) => ExternalError::Io(error),
        crate::EvalError::TemplateInvalid { path, reason } => ExternalError::TemplateInvalid {
            program: ExternalProgram::Crystal,
            path,
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{CrystalExternalAdapter, CrystalExternalAdapterConfig};
    use crate::{
        CrystalArtifactKind, CrystalRunIntent, ExternalEvaluationMode, ExternalEvaluationRequest,
        ExternalEvaluationStatus, ExternalEvaluator, ExternalExecutionMode, ExternalProgram,
        ExternalTemplateSet,
    };
    use patina_types::Candidate;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn periodic_candidate() -> Candidate {
        Candidate::fully_periodic(
            "ceria",
            vec!["Ce".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]],
        )
    }

    fn request(
        workdir: &Path,
        mode: ExternalEvaluationMode,
        execution: ExternalExecutionMode,
    ) -> ExternalEvaluationRequest {
        ExternalEvaluationRequest {
            request_id: "req-crystal-1".into(),
            stage: 2,
            program: ExternalProgram::Crystal,
            mode,
            execution,
            retrieve_relaxed_geometry: true,
            candidate: periodic_candidate(),
            workdir: workdir.into(),
            templates: ExternalTemplateSet::default(),
        }
    }

    #[test]
    fn crystal_external_adapter_stages_export_only_inputs() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.d12");
        fs::write(&template_path, "Generated CRYSTAL template\nEND\n").expect("write template");

        let adapter = CrystalExternalAdapter::new(
            CrystalExternalAdapterConfig::new(&template_path, temp.path().join("Pcrystal"))
                .with_run_intent(CrystalRunIntent::GeometryOptimization)
                .with_properties_output_name("properties.out"),
        )
        .expect("adapter");

        let outcome = adapter
            .evaluate(&request(
                &temp.path().join("run-export"),
                ExternalEvaluationMode::ExportOnly,
                ExternalExecutionMode::LocalSubprocess,
            ))
            .expect("export-only should stage without launching");

        assert_eq!(outcome.status, ExternalEvaluationStatus::ExportedOnly);
        assert!(outcome.result.is_none());
        assert!(outcome.submitted_run.is_none());
        assert!(outcome.artifacts.workdir.join("crystal.d12").exists());
        assert!(outcome.artifacts.workdir.join("candidate.extxyz").exists());

        let contract = adapter.terminal_output_contract();
        let candidates = contract.artifact_candidates(&outcome.artifacts.workdir);
        assert_eq!(contract.run_intent, CrystalRunIntent::GeometryOptimization);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].kind, CrystalArtifactKind::PrimaryOutput);
        assert_eq!(candidates[1].kind, CrystalArtifactKind::ErrorOutput);
        assert_eq!(candidates[2].kind, CrystalArtifactKind::PropertiesOutput);
    }

    #[test]
    fn crystal_external_adapter_returns_deferred_submitted_outcome() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.d12");
        fs::write(&template_path, "Generated CRYSTAL template\nEND\n").expect("write template");

        let adapter = CrystalExternalAdapter::new(CrystalExternalAdapterConfig::new(
            &template_path,
            temp.path().join("Pcrystal"),
        ))
        .expect("adapter");

        let outcome = adapter
            .evaluate(&request(
                &temp.path().join("run-submitted"),
                ExternalEvaluationMode::Relaxation,
                ExternalExecutionMode::SchedulerSubmitted,
            ))
            .expect("scheduler-submitted launch should defer");

        assert_eq!(outcome.status, ExternalEvaluationStatus::Submitted);
        assert!(outcome.result.is_none());
        assert_eq!(
            outcome
                .submitted_run
                .as_ref()
                .map(|run| run.launch_handle.as_str()),
            Some("submitted:crystal:req-crystal-1:2")
        );
        assert!(outcome.artifacts.workdir.join("crystal.d12").exists());
    }

    #[test]
    fn crystal_external_adapter_rejects_terminal_local_execution_until_mapping_exists() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.d12");
        fs::write(&template_path, "Generated CRYSTAL template\nEND\n").expect("write template");

        let adapter = CrystalExternalAdapter::new(CrystalExternalAdapterConfig::new(
            &template_path,
            temp.path().join("Pcrystal"),
        ))
        .expect("adapter");

        let error = adapter
            .evaluate(&request(
                &temp.path().join("run-local"),
                ExternalEvaluationMode::Relaxation,
                ExternalExecutionMode::LocalSubprocess,
            ))
            .expect_err("terminal local execution should stay disabled");

        assert!(matches!(
            error,
            crate::ExternalError::UnsupportedProgram { program, reason }
                if program == ExternalProgram::Crystal
                    && reason.contains("result mapping")
        ));
    }
}
