use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::backend_adapters::{
    absolute_workdir, parse_got_energy_only_file, patch_gin_for_single_point, truncate_stderr,
    CompletedProcess, EvalError, GinWriter, GotParser,
};
use crate::composition::ComposedExternalEvaluator;
use crate::{
    CompletedExternalRun, ExternalArtifactClass, ExternalArtifactMaterialization,
    ExternalArtifactRetention, ExternalArtifacts, ExternalError, ExternalEvaluationMode,
    ExternalEvaluationOutcome, ExternalEvaluationRequest, ExternalEvaluationStatus,
    ExternalEvaluator, ExternalExecutionMode, ExternalInputWriter, ExternalLaunchOutcome,
    ExternalLauncher, ExternalOutputParser, ExternalProgram, SubmittedExternalRun,
};

/// Configures a concrete `GULP` external adapter using the newer external ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GulpExternalAdapterConfig {
    pub template_path: PathBuf,
    pub executable: PathBuf,
    pub input_name: String,
    pub output_name: String,
    pub timeout: Option<Duration>,
}

impl GulpExternalAdapterConfig {
    pub fn new(template_path: impl Into<PathBuf>, executable: impl Into<PathBuf>) -> Self {
        Self {
            template_path: template_path.into(),
            executable: executable.into(),
            input_name: "candidate.gin".into(),
            output_name: "candidate.got".into(),
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

    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Concrete external adapter for `GULP`.
///
/// This adapter supports:
/// - local subprocess/script execution for laptop and workstation use
/// - export-only staging
/// - deferred scheduler-submitted launch metadata
#[derive(Debug, Clone)]
pub struct GulpExternalAdapter {
    inner: ComposedExternalEvaluator<GulpInputWriter, GulpLauncher, GulpOutputParser>,
}

impl GulpExternalAdapter {
    pub fn new(cfg: GulpExternalAdapterConfig) -> Result<Self, ExternalError> {
        let writer = GulpInputWriter::new(
            GinWriter::from_template_file(&cfg.template_path)
                .map_err(|error| map_eval_error(ExternalProgram::Gulp, error))?,
            cfg.input_name.clone(),
            cfg.output_name.clone(),
        );
        let launcher = GulpLauncher::new(
            cfg.executable,
            cfg.input_name.clone(),
            cfg.output_name.clone(),
            cfg.timeout,
        );
        let parser = GulpOutputParser::new(cfg.output_name);

        Ok(Self {
            inner: ComposedExternalEvaluator::new(writer, launcher, parser),
        })
    }
}

impl ExternalEvaluator for GulpExternalAdapter {
    fn evaluate(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalEvaluationOutcome, ExternalError> {
        self.inner.evaluate(request)
    }
}

#[derive(Debug, Clone)]
struct GulpInputWriter {
    gin_writer: GinWriter,
    input_name: String,
    output_name: String,
}

impl GulpInputWriter {
    fn new(gin_writer: GinWriter, input_name: String, output_name: String) -> Self {
        Self {
            gin_writer,
            input_name,
            output_name,
        }
    }
}

impl ExternalInputWriter for GulpInputWriter {
    fn write_inputs(
        &self,
        request: &ExternalEvaluationRequest,
    ) -> Result<ExternalArtifacts, ExternalError> {
        if request.program != ExternalProgram::Gulp {
            return Err(ExternalError::UnsupportedProgram {
                program: request.program,
                reason: "GulpExternalAdapter only supports the `gulp` program".into(),
            });
        }

        fs::create_dir_all(&request.workdir)?;
        let input_path = request.workdir.join(&self.input_name);
        let output_path = request.workdir.join(&self.output_name);
        let stderr_path = request.workdir.join("gulp.stderr.log");

        self.gin_writer
            .write_candidate(&request.candidate, &input_path)
            .map_err(|error| map_eval_error(ExternalProgram::Gulp, error))?;
        if request.mode == ExternalEvaluationMode::SinglePoint {
            patch_gin_for_single_point(&input_path)
                .map_err(|error| map_eval_error(ExternalProgram::Gulp, error))?;
        }

        let mut artifacts = ExternalArtifacts::new(&request.workdir);
        artifacts.register_input(
            input_path,
            ExternalArtifactClass::ControlInput,
            ExternalArtifactMaterialization::Rendered,
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

        Ok(artifacts)
    }
}

#[derive(Debug, Clone)]
struct GulpLauncher {
    executable: PathBuf,
    input_name: String,
    output_name: String,
    timeout: Option<Duration>,
}

impl GulpLauncher {
    fn new(
        executable: PathBuf,
        input_name: String,
        output_name: String,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            executable,
            input_name,
            output_name,
            timeout,
        }
    }

    fn uses_stdin_stdout_contract(&self) -> bool {
        self.executable
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("gulp"))
            .unwrap_or(false)
    }

    fn invocation_input_arg(&self) -> String {
        let executable_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if executable_name.eq_ignore_ascii_case("gulp") {
            Path::new(&self.input_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(&self.input_name)
                .to_string()
        } else {
            self.input_name.clone()
        }
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

impl ExternalLauncher for GulpLauncher {
    fn launch(
        &self,
        request: &ExternalEvaluationRequest,
        artifacts: &ExternalArtifacts,
    ) -> Result<ExternalLaunchOutcome, ExternalError> {
        match request.execution {
            ExternalExecutionMode::SchedulerSubmitted => {
                return Ok(ExternalLaunchOutcome::Submitted {
                    run: SubmittedExternalRun {
                        program: request.program,
                        execution: request.execution,
                        artifacts: artifacts.clone(),
                        launch_handle: self.submitted_handle(request),
                        remote_workdir: Some(artifacts.workdir.clone()),
                    },
                });
            }
            ExternalExecutionMode::RemoteBatch => {
                return Err(ExternalError::UnsupportedProgram {
                    program: request.program,
                    reason: "remote batch launch is not implemented for GULP yet".into(),
                });
            }
            ExternalExecutionMode::LocalSubprocess | ExternalExecutionMode::LocalScript => {}
        }

        if request.mode == ExternalEvaluationMode::ExportOnly {
            return Ok(ExternalLaunchOutcome::Completed {
                run: CompletedExternalRun {
                    program: request.program,
                    execution: request.execution,
                    artifacts: artifacts.clone(),
                    exit_code: None,
                    elapsed: Duration::default(),
                },
            });
        }

        let started = Instant::now();
        let workdir = absolute_workdir(&artifacts.workdir)?;
        let input_path = artifacts.input_paths.first().cloned().ok_or_else(|| {
            ExternalError::InvalidRequest {
                reason: "GULP launcher expected an input path".into(),
            }
        })?;
        let stdout_path = artifacts
            .stdout_path
            .clone()
            .unwrap_or_else(|| artifacts.workdir.join(&self.output_name));
        let stderr_path = artifacts
            .stderr_path
            .clone()
            .unwrap_or_else(|| artifacts.workdir.join("gulp.stderr.log"));

        let mut command = Command::new(&self.executable);
        command
            .current_dir(&workdir)
            .env("OMP_NUM_THREADS", "1")
            .stderr(Stdio::piped());

        if self.uses_stdin_stdout_contract() {
            let stdin = File::open(&input_path)?;
            let stdout = File::create(&stdout_path)?;
            command.stdin(Stdio::from(stdin));
            command.stdout(Stdio::from(stdout));
        } else {
            command.arg(self.invocation_input_arg());
            command.stdout(Stdio::null());
        }

        let mut child = command.spawn()?;
        let output = if let Some(timeout) = self.timeout {
            loop {
                if let Some(status) = child.try_wait()? {
                    let stderr = child.wait_with_output()?.stderr;
                    break CompletedProcess { status, stderr };
                }
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ExternalError::Timeout {
                        program: request.program,
                        elapsed: started.elapsed(),
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        } else {
            let output = child.wait_with_output()?;
            CompletedProcess {
                status: output.status,
                stderr: output.stderr,
            }
        };

        fs::write(&stderr_path, &output.stderr)?;

        if !output.status.success() {
            return Err(ExternalError::ProcessFailed {
                program: request.program,
                exit_code: output.status.code(),
                stderr: truncate_stderr(&output.stderr),
            });
        }

        Ok(ExternalLaunchOutcome::Completed {
            run: CompletedExternalRun {
                program: request.program,
                execution: request.execution,
                artifacts: artifacts.clone(),
                exit_code: output.status.code(),
                elapsed: started.elapsed(),
            },
        })
    }
}

#[derive(Debug, Clone)]
struct GulpOutputParser {
    output_name: String,
}

impl GulpOutputParser {
    fn new(output_name: String) -> Self {
        Self { output_name }
    }

    fn output_path<'a>(&self, run: &'a CompletedExternalRun) -> Result<&'a Path, ExternalError> {
        run.artifacts
            .output_paths
            .first()
            .map(PathBuf::as_path)
            .or(run.artifacts.stdout_path.as_deref())
            .ok_or_else(|| ExternalError::ParseFailed {
                program: run.program,
                reason: format!(
                    "expected `{}` output artifact for GULP parser",
                    self.output_name
                ),
            })
    }
}

impl ExternalOutputParser for GulpOutputParser {
    fn parse_outputs(
        &self,
        request: &ExternalEvaluationRequest,
        run: &CompletedExternalRun,
    ) -> Result<ExternalEvaluationOutcome, ExternalError> {
        if request.mode == ExternalEvaluationMode::ExportOnly {
            return Ok(ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::ExportedOnly,
                result: None,
                artifacts: run.artifacts.clone(),
                submitted_run: None,
            });
        }

        let output_path = self.output_path(run)?;
        let mut result = match request.mode {
            ExternalEvaluationMode::Relaxation => GotParser::parse_file(output_path)
                .map_err(|error| map_eval_error(request.program, error))?,
            ExternalEvaluationMode::SinglePoint => {
                parse_got_energy_only_file(output_path, &request.candidate)
                    .map_err(|error| map_eval_error(request.program, error))?
            }
            ExternalEvaluationMode::ExportOnly => unreachable!(),
        };
        result.relaxed_candidate.label = request.candidate.label.clone();
        result.wall_time = run.elapsed;

        let status = if request.mode == ExternalEvaluationMode::SinglePoint || result.converged {
            ExternalEvaluationStatus::Converged
        } else {
            ExternalEvaluationStatus::NotConverged
        };

        Ok(ExternalEvaluationOutcome {
            status,
            result: Some(result),
            artifacts: run.artifacts.clone(),
            submitted_run: None,
        })
    }
}

fn map_eval_error(program: ExternalProgram, error: EvalError) -> ExternalError {
    match error {
        EvalError::ProcessFailed { exit_code, stderr } => ExternalError::ProcessFailed {
            program,
            exit_code,
            stderr,
        },
        EvalError::ParseFailed { reason, .. } => ExternalError::ParseFailed { program, reason },
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
    use super::{GulpExternalAdapter, GulpExternalAdapterConfig};
    use crate::{
        ExternalEvaluationMode, ExternalEvaluationRequest, ExternalEvaluationStatus,
        ExternalEvaluator, ExternalExecutionMode, ExternalProgram, ExternalTemplateSet,
    };
    use patina_types::Candidate;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Duration;
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
            request_id: "req-gulp-1".into(),
            stage: 1,
            program: ExternalProgram::Gulp,
            mode,
            execution,
            retrieve_relaxed_geometry: true,
            candidate: periodic_candidate(),
            workdir: workdir.into(),
            templates: ExternalTemplateSet::default(),
        }
    }

    #[test]
    #[cfg(unix)]
    fn gulp_external_adapter_runs_local_script_and_parses_result() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.gin");
        fs::write(
            &template_path,
            "\
opti conp
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
        )
        .expect("write template");
        let script_path = temp.path().join("fake_gulp.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nprintf 'Final energy = -1.23 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Ce 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n' > candidate.got\n",
        )
        .expect("write script");
        let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

        let adapter = GulpExternalAdapter::new(
            GulpExternalAdapterConfig::new(&template_path, &script_path)
                .with_timeout(Some(Duration::from_secs(5))),
        )
        .expect("adapter");

        let outcome = adapter
            .evaluate(&request(
                &temp.path().join("run-local"),
                ExternalEvaluationMode::Relaxation,
                ExternalExecutionMode::LocalScript,
            ))
            .expect("local script should evaluate");

        assert_eq!(outcome.status, ExternalEvaluationStatus::Converged);
        assert_eq!(outcome.result.as_ref().map(|r| r.energy), Some(-1.23));
        assert!(outcome.submitted_run.is_none());
        assert!(outcome.artifacts.workdir.join("candidate.gin").exists());
    }

    #[test]
    fn gulp_external_adapter_returns_deferred_submitted_outcome() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.gin");
        fs::write(
            &template_path,
            "\
opti conp
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
        )
        .expect("write template");

        let adapter = GulpExternalAdapter::new(GulpExternalAdapterConfig::new(
            &template_path,
            temp.path().join("unused-gulp"),
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
            Some("submitted:gulp:req-gulp-1:1")
        );
        assert!(outcome.artifacts.workdir.join("candidate.gin").exists());
    }

    #[test]
    fn gulp_external_adapter_supports_export_only_staging_without_launch() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.gin");
        fs::write(
            &template_path,
            "\
single
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
        )
        .expect("write template");

        let adapter = GulpExternalAdapter::new(GulpExternalAdapterConfig::new(
            &template_path,
            temp.path().join("unused-gulp"),
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
        assert!(outcome.artifacts.workdir.join("candidate.gin").exists());
    }
}
