use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::backend_adapters::write_candidate_xyz;
use crate::{
    Cp2kTerminalOutputContract, ExternalArtifactClass, ExternalArtifactMaterialization,
    ExternalArtifactRetention, ExternalArtifacts, ExternalError, ExternalEvaluationMode,
    ExternalEvaluationOutcome, ExternalEvaluationRequest, ExternalEvaluationStatus,
    ExternalEvaluator, ExternalExecutionMode, ExternalProgram, SubmittedExternalRun,
};

/// Configures the first typed CP2K adapter surface.
///
/// This first lane is intentionally conservative:
/// - it stages a stable CP2K workdir shape
/// - it supports export-only workflows
/// - it supports deferred scheduler submission metadata
/// - it does not yet claim terminal scientific parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cp2kExternalAdapterConfig {
    pub template_path: PathBuf,
    pub executable: PathBuf,
    pub input_name: String,
    pub output_name: String,
    pub structure_name: String,
    pub cell_include_name: String,
    pub timeout: Option<Duration>,
}

impl Cp2kExternalAdapterConfig {
    pub fn new(template_path: impl Into<PathBuf>, executable: impl Into<PathBuf>) -> Self {
        Self {
            template_path: template_path.into(),
            executable: executable.into(),
            input_name: "cp2k.inp".into(),
            output_name: "cp2k.out".into(),
            structure_name: "candidate.extxyz".into(),
            cell_include_name: "candidate.cell.inc".into(),
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

    pub fn with_structure_artifacts(
        mut self,
        structure_name: impl Into<String>,
        cell_include_name: impl Into<String>,
    ) -> Self {
        self.structure_name = structure_name.into();
        self.cell_include_name = cell_include_name.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
}

/// First CP2K adapter for staging and deferred launch.
///
/// The design intentionally does not fabricate a parsed `EvalResult` until a stable CP2K output
/// contract is defined.
#[derive(Debug, Clone)]
pub struct Cp2kExternalAdapter {
    cfg: Cp2kExternalAdapterConfig,
}

impl Cp2kExternalAdapter {
    pub fn new(cfg: Cp2kExternalAdapterConfig) -> Result<Self, ExternalError> {
        if !cfg.template_path.exists() {
            return Err(ExternalError::TemplateInvalid {
                program: ExternalProgram::Cp2k,
                path: cfg.template_path.clone(),
                reason: "CP2K template file does not exist".into(),
            });
        }
        Ok(Self { cfg })
    }

    pub fn terminal_output_contract(
        &self,
        workdir: &Path,
    ) -> Result<Cp2kTerminalOutputContract, ExternalError> {
        Cp2kTerminalOutputContract::from_input_file(
            &workdir.join(&self.cfg.input_name),
            self.cfg.output_name.clone(),
            "cp2k.stderr.log",
        )
        .map_err(ExternalError::Io)
    }

    fn stage_inputs(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalArtifacts, ExternalError> {
        if request.program != ExternalProgram::Cp2k {
            return Err(ExternalError::UnsupportedProgram {
                program: request.program,
                reason: "Cp2kExternalAdapter only supports the `cp2k` program".into(),
            });
        }

        fs::create_dir_all(&request.workdir)?;
        let input_path = request.workdir.join(&self.cfg.input_name);
        let output_path = request.workdir.join(&self.cfg.output_name);
        let stderr_path = request.workdir.join("cp2k.stderr.log");
        let structure_path = request.workdir.join(&self.cfg.structure_name);
        let cell_include_path = request.workdir.join(&self.cfg.cell_include_name);

        fs::copy(&self.cfg.template_path, &input_path)?;
        write_candidate_xyz(&request.candidate, &structure_path).map_err(map_eval_error)?;
        if let Some(lattice) = request.candidate.lattice {
            fs::write(
                &cell_include_path,
                render_cp2k_cell_include(lattice, request.candidate.periodic_axes),
            )?;
        }

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
        if request.candidate.lattice.is_some() {
            artifacts.register_input(
                cell_include_path,
                ExternalArtifactClass::ControlInput,
                ExternalArtifactMaterialization::Generated,
                ExternalArtifactRetention::RegenerableScratch,
            );
        }

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

impl ExternalEvaluator for Cp2kExternalAdapter {
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
                reason: "remote batch launch is not implemented for CP2K yet".into(),
            }),
            ExternalExecutionMode::LocalSubprocess | ExternalExecutionMode::LocalScript => {
                let _ = &self.cfg.executable;
                let _ = self.cfg.timeout;
                Err(ExternalError::UnsupportedProgram {
                    program: request.program,
                    reason: "terminal CP2K execution is blocked until CP2K output parsing and result mapping are specified".into(),
                })
            }
        }
    }
}

fn render_cp2k_cell_include(lattice: [[f64; 3]; 3], periodic_axes: [bool; 3]) -> String {
    format!(
        "&CELL\n  A {:.10} {:.10} {:.10}\n  B {:.10} {:.10} {:.10}\n  C {:.10} {:.10} {:.10}\n  PERIODIC {}\n&END CELL\n",
        lattice[0][0],
        lattice[0][1],
        lattice[0][2],
        lattice[1][0],
        lattice[1][1],
        lattice[1][2],
        lattice[2][0],
        lattice[2][1],
        lattice[2][2],
        cp2k_periodic_label(periodic_axes),
    )
}

fn cp2k_periodic_label(periodic_axes: [bool; 3]) -> &'static str {
    match periodic_axes {
        [false, false, false] => "NONE",
        [true, false, false] => "X",
        [false, true, false] => "Y",
        [false, false, true] => "Z",
        [true, true, false] => "XY",
        [true, false, true] => "XZ",
        [false, true, true] => "YZ",
        [true, true, true] => "XYZ",
    }
}

fn map_eval_error(error: crate::EvalError) -> ExternalError {
    match error {
        crate::EvalError::ProcessFailed { exit_code, stderr } => ExternalError::ProcessFailed {
            program: ExternalProgram::Cp2k,
            exit_code,
            stderr,
        },
        crate::EvalError::ParseFailed { reason, .. } => ExternalError::ParseFailed {
            program: ExternalProgram::Cp2k,
            reason,
        },
        crate::EvalError::NotConverged {
            energy,
            n_steps,
            partial_result,
        } => ExternalError::NotConverged {
            program: ExternalProgram::Cp2k,
            energy,
            n_steps,
            partial_result,
        },
        crate::EvalError::Timeout { elapsed } => ExternalError::Timeout {
            program: ExternalProgram::Cp2k,
            elapsed,
        },
        crate::EvalError::IoError(error) => ExternalError::Io(error),
        crate::EvalError::TemplateInvalid { path, reason } => ExternalError::TemplateInvalid {
            program: ExternalProgram::Cp2k,
            path,
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{Cp2kExternalAdapter, Cp2kExternalAdapterConfig};
    use crate::{
        Cp2kArtifactKind, ExternalEvaluationMode, ExternalEvaluationRequest,
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
            request_id: "req-cp2k-1".into(),
            stage: 1,
            program: ExternalProgram::Cp2k,
            mode,
            execution,
            retrieve_relaxed_geometry: true,
            candidate: periodic_candidate(),
            workdir: workdir.into(),
            templates: ExternalTemplateSet::default(),
        }
    }

    #[test]
    fn cp2k_external_adapter_stages_export_only_inputs() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.inp");
        fs::write(&template_path, "&GLOBAL\n  PROJECT test\n&END GLOBAL\n")
            .expect("write template");

        let adapter = Cp2kExternalAdapter::new(Cp2kExternalAdapterConfig::new(
            &template_path,
            temp.path().join("cp2k.psmp"),
        ))
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
        assert!(outcome.artifacts.workdir.join("cp2k.inp").exists());
        assert!(outcome.artifacts.workdir.join("candidate.extxyz").exists());
        assert!(outcome
            .artifacts
            .workdir
            .join("candidate.cell.inc")
            .exists());
        let contract = adapter
            .terminal_output_contract(&outcome.artifacts.workdir)
            .expect("contract");
        assert_eq!(contract.project_name.as_deref(), Some("test"));
    }

    #[test]
    fn cp2k_external_adapter_returns_deferred_submitted_outcome() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.inp");
        fs::write(&template_path, "&GLOBAL\n  PROJECT test\n&END GLOBAL\n")
            .expect("write template");

        let adapter = Cp2kExternalAdapter::new(Cp2kExternalAdapterConfig::new(
            &template_path,
            temp.path().join("cp2k.psmp"),
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
            Some("submitted:cp2k:req-cp2k-1:1")
        );
        assert!(outcome.artifacts.workdir.join("cp2k.inp").exists());
        let contract = adapter
            .terminal_output_contract(&outcome.artifacts.workdir)
            .expect("contract");
        assert_eq!(
            contract.primary_output_path(&outcome.artifacts.workdir),
            outcome.artifacts.workdir.join("cp2k.out")
        );
    }

    #[test]
    fn cp2k_terminal_contract_derives_project_specific_candidates_from_staged_input() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.inp");
        fs::write(
            &template_path,
            "&GLOBAL\n  PROJECT H2O\n  RUN_TYPE GEO_OPT\n&END GLOBAL\n",
        )
        .expect("write template");

        let adapter = Cp2kExternalAdapter::new(Cp2kExternalAdapterConfig::new(
            &template_path,
            temp.path().join("cp2k.psmp"),
        ))
        .expect("adapter");

        let outcome = adapter
            .evaluate(&request(
                &temp.path().join("run-contract"),
                ExternalEvaluationMode::ExportOnly,
                ExternalExecutionMode::LocalSubprocess,
            ))
            .expect("staged");

        let contract = adapter
            .terminal_output_contract(&outcome.artifacts.workdir)
            .expect("contract");
        let candidates = contract.artifact_candidates(&outcome.artifacts.workdir);

        assert_eq!(contract.project_name.as_deref(), Some("H2O"));
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[1].kind, Cp2kArtifactKind::RelaxedTrajectoryXyz);
        assert!(candidates[1].path.ends_with("H2O-pos-1.xyz"));
        assert_eq!(candidates[2].kind, Cp2kArtifactKind::RestartInput);
        assert!(candidates[2].path.ends_with("H2O-1.restart"));
    }

    #[test]
    fn cp2k_external_adapter_rejects_terminal_local_execution_until_parse_contract_exists() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.inp");
        fs::write(&template_path, "&GLOBAL\n  PROJECT test\n&END GLOBAL\n")
            .expect("write template");

        let adapter = Cp2kExternalAdapter::new(Cp2kExternalAdapterConfig::new(
            &template_path,
            temp.path().join("cp2k.psmp"),
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
                if program == ExternalProgram::Cp2k
                    && reason.contains("output parsing")
        ));
    }
}
