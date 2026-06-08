use patina_types::{Candidate, EvalResult};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use crate::backend_adapters::{
    truncate_stderr, BackendEvaluator, CompletedProcess, EvalError, GinWriter,
};

/// Line-oriented parser for GULP `.got` files.
///
/// A state machine is preferred over regexes here because GULP output is irregular, warnings may
/// be interleaved with data blocks, and extracting structured coordinate tables requires entering
/// and exiting blocks explicitly. The parser accepts:
/// - `Final energy = ...`
/// - `Optimisation achieved` or `Optimization achieved`
/// - a `Final fractional coordinates of atoms` block
///
/// The parser fails closed: unknown or missing energy/geometry fields return an error instead of
/// fabricating `energy = 0.0`, which would catastrophically bias BH/GA acceptance logic.
#[derive(Debug, Default, Clone, Copy)]
pub struct GotParser;

#[derive(Debug, Clone)]
struct ParsedGulpCoordRow {
    species: String,
    site_kind: Option<String>,
    coord: [f64; 3],
}

impl GotParser {
    /// Parses an `.got` file from disk.
    pub fn parse_file(path: impl AsRef<Path>) -> Result<EvalResult, EvalError> {
        let content = fs::read_to_string(path).map_err(EvalError::IoError)?;
        Self::parse_str(&content)
    }

    /// Parses `.got` content from a string.
    pub fn parse_str(content: &str) -> Result<EvalResult, EvalError> {
        let mut energy = None;
        let mut converged = false;
        let mut rows = Vec::new();
        let mut in_fractional_block = false;
        let mut in_cartesian_block = false;
        let mut explicit_failure = None;

        for (index, raw_line) in content.lines().enumerate() {
            let line_no = index + 1;
            let line = raw_line.trim();

            if let Some(reason) = line.strip_prefix("ERROR :") {
                explicit_failure = Some((
                    line_no,
                    format!("GULP reported input/runtime error: `{}`", reason.trim()),
                ));
            }

            if line.contains("Too many failed attempts to optimise")
                || line.contains("Too many failed attempts to optimize")
            {
                explicit_failure = Some((
                    line_no,
                    format!("GULP reported optimization failure: `{line}`"),
                ));
            }

            if line.contains("Optimisation achieved") || line.contains("Optimization achieved") {
                converged = true;
            }

            if line.starts_with("Final energy =") {
                energy = Some(parse_gulp_energy_value(line, line_no)?);
                continue;
            }

            if line.contains("Final fractional coordinates of atoms") {
                in_fractional_block = true;
                in_cartesian_block = false;
                continue;
            }

            if line.contains("Final cartesian coordinates of atoms") {
                in_cartesian_block = true;
                in_fractional_block = false;
                continue;
            }

            if in_fractional_block {
                if line.is_empty() || line.starts_with("---") {
                    continue;
                }
                if line.starts_with("No.") || line.starts_with("Label") {
                    continue;
                }

                if line.starts_with("End final fractional coordinates")
                    || line.starts_with("Final cartesian coordinates")
                    || line.starts_with("Final internal derivatives")
                    || line.starts_with("Final Cartesian derivatives")
                    || line.starts_with("Final derivatives")
                    || line.starts_with("Components of energy")
                    || line.starts_with("Final energy =")
                    || line.starts_with("Job Finished")
                {
                    in_fractional_block = false;
                    continue;
                }

                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    return Err(EvalError::ParseFailed {
                        line: line_no,
                        reason: format!("expected coordinate row with >= 4 columns, got `{line}`"),
                    });
                }

                rows.push(parse_gulp_coordinate_row(&parts, line, line_no, 3)?);
                continue;
            }

            if in_cartesian_block {
                if line.is_empty() || line.starts_with("---") {
                    continue;
                }
                if line.starts_with("No.") || line.starts_with("Label") {
                    continue;
                }

                if line.starts_with("Final Cartesian derivatives")
                    || line.starts_with("Final fractional coordinates")
                    || line.starts_with("Job Finished")
                {
                    in_cartesian_block = false;
                    continue;
                }

                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() < 6 {
                    return Err(EvalError::ParseFailed {
                        line: line_no,
                        reason: format!(
                            "expected cartesian coordinate row with >= 6 columns, got `{line}`"
                        ),
                    });
                }

                rows.push(parse_gulp_coordinate_row(&parts, line, line_no, 4)?);
            }
        }

        if let Some((line, reason)) = explicit_failure {
            return Err(EvalError::ParseFailed { line, reason });
        }

        let energy = energy.ok_or_else(|| EvalError::ParseFailed {
            line: 0,
            reason: "missing `Final energy =` line".into(),
        })?;

        if rows.is_empty() {
            return Err(EvalError::ParseFailed {
                line: 0,
                reason: "missing final fractional coordinate block".into(),
            });
        }

        let retained_rows = collapse_shell_rows(rows);
        let species = retained_rows
            .iter()
            .map(|row| row.species.clone())
            .collect::<Vec<_>>();
        let coords = retained_rows
            .into_iter()
            .map(|row| row.coord)
            .collect::<Vec<_>>();

        Ok(EvalResult {
            energy,
            forces: Vec::new(),
            relaxed_candidate: Candidate {
                species,
                fractional_coords: coords,
                lattice: None,
                periodic_axes: [false, false, false],
                label: "parsed-from-got".into(),
            },
            converged,
            wall_time: Duration::default(),
        })
    }
}

fn parse_gulp_coordinate_row(
    parts: &[&str],
    line: &str,
    line_no: usize,
    trailing_coord_offset: usize,
) -> Result<ParsedGulpCoordRow, EvalError> {
    let n = parts.len();
    let parsed = [
        parts[n - trailing_coord_offset]
            .parse::<f64>()
            .map_err(|err| EvalError::ParseFailed {
                line: line_no,
                reason: format!("invalid x coordinate in `{line}`: {err}"),
            })?,
        parts[n - trailing_coord_offset + 1]
            .parse::<f64>()
            .map_err(|err| EvalError::ParseFailed {
                line: line_no,
                reason: format!("invalid y coordinate in `{line}`: {err}"),
            })?,
        parts[n - trailing_coord_offset + 2]
            .parse::<f64>()
            .map_err(|err| EvalError::ParseFailed {
                line: line_no,
                reason: format!("invalid z coordinate in `{line}`: {err}"),
            })?,
    ];

    let site_kind = parts
        .get(2)
        .filter(|value| value.parse::<f64>().is_err())
        .map(|value| (*value).to_string());

    Ok(ParsedGulpCoordRow {
        species: parts[1].to_string(),
        site_kind,
        coord: parsed,
    })
}

fn collapse_shell_rows(rows: Vec<ParsedGulpCoordRow>) -> Vec<ParsedGulpCoordRow> {
    if !rows
        .iter()
        .any(|row| row.site_kind.as_deref().is_some_and(is_shell_site_kind))
    {
        return rows;
    }

    let retained = rows
        .into_iter()
        .filter(|row| !row.site_kind.as_deref().is_some_and(is_shell_site_kind))
        .collect::<Vec<_>>();

    if retained.is_empty() {
        return Vec::new();
    }

    retained
}

fn is_shell_site_kind(site_kind: &str) -> bool {
    matches!(
        site_kind.trim().to_ascii_lowercase().as_str(),
        "s" | "shel" | "shell"
    )
}

/// Subprocess-backed evaluator that runs the external `klmc3` binary.
#[derive(Debug, Clone)]
pub struct GulpBackend {
    gin_writer: GinWriter,
    klmc3_bin: PathBuf,
    input_name: String,
    output_name: String,
    scott_sidecars: Option<GulpScottSidecars>,
    timeout: Option<Duration>,
    mode: GulpRunMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GulpRunMode {
    Relaxed,
    SinglePointEnergy,
}

#[derive(Debug, Clone)]
struct GulpScottSidecars {
    master_gin_target_name: String,
    master_gin_template: PathBuf,
    atoms_in_target_name: Option<String>,
    atoms_in_template: Option<PathBuf>,
    restart_xyz_name: Option<String>,
}

impl GulpBackend {
    /// Creates a new backend from a template and executable path.
    pub fn new(
        template_path: impl AsRef<Path>,
        klmc3_bin: impl AsRef<Path>,
        timeout: Option<Duration>,
    ) -> Result<Self, EvalError> {
        Ok(Self {
            gin_writer: GinWriter::from_template_file(template_path)?,
            klmc3_bin: klmc3_bin.as_ref().to_path_buf(),
            input_name: "candidate.gin".into(),
            output_name: "candidate.got".into(),
            scott_sidecars: None,
            timeout,
            mode: GulpRunMode::Relaxed,
        })
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

    pub fn with_scott_sidecars(
        mut self,
        master_gin_template: impl AsRef<Path>,
        atoms_in_template: Option<impl AsRef<Path>>,
    ) -> Self {
        self.scott_sidecars = Some(GulpScottSidecars {
            master_gin_target_name: "Master.gin".into(),
            master_gin_template: master_gin_template.as_ref().to_path_buf(),
            atoms_in_target_name: atoms_in_template.as_ref().map(|_| "atoms.in".into()),
            atoms_in_template: atoms_in_template.map(|path| path.as_ref().to_path_buf()),
            restart_xyz_name: Some("seed.xyz".into()),
        });
        self
    }

    pub fn with_single_point_energy(mut self) -> Self {
        self.mode = GulpRunMode::SinglePointEnergy;
        self
    }

    fn invocation_input_arg(&self) -> String {
        let executable_name = self
            .klmc3_bin
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

    fn uses_stdin_stdout_contract(&self) -> bool {
        self.klmc3_bin
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("gulp"))
            .unwrap_or(false)
    }

    fn output_candidates(&self, workdir: &Path) -> Vec<PathBuf> {
        let mut candidates = vec![workdir.join(&self.output_name)];
        let input_path = workdir.join(&self.input_name);
        if let Some(stem) = input_path.file_stem().and_then(|stem| stem.to_str()) {
            candidates.push(workdir.join(format!("{stem}.gout")));
        }
        candidates.push(input_path.with_extension("gout"));
        candidates.sort();
        candidates.dedup();
        candidates
    }
}

impl BackendEvaluator for GulpBackend {
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError> {
        let started = Instant::now();
        let gin_path = workdir.join(&self.input_name);

        if let Some(sidecars) = &self.scott_sidecars {
            fs::copy(
                &sidecars.master_gin_template,
                workdir.join(&sidecars.master_gin_target_name),
            )
            .map_err(EvalError::IoError)?;
            if let (Some(source), Some(target_name)) =
                (&sidecars.atoms_in_template, &sidecars.atoms_in_target_name)
            {
                fs::copy(source, workdir.join(target_name)).map_err(EvalError::IoError)?;
            }
            if let Some(restart_xyz_name) = &sidecars.restart_xyz_name {
                self.gin_writer
                    .write_restart_xyz(candidate, workdir.join(restart_xyz_name))?;
            }
        }

        self.gin_writer.write_candidate(candidate, &gin_path)?;
        if self.mode == GulpRunMode::SinglePointEnergy {
            patch_gin_for_single_point(&gin_path)?;
        }

        let mut command = Command::new(&self.klmc3_bin);
        command
            .current_dir(workdir)
            .env("OMP_NUM_THREADS", "1")
            .stderr(Stdio::piped());

        if self.uses_stdin_stdout_contract() {
            let stdout_path = workdir.join(&self.output_name);
            let stdin = File::open(&gin_path).map_err(EvalError::IoError)?;
            let stdout = File::create(&stdout_path).map_err(EvalError::IoError)?;
            command.stdin(Stdio::from(stdin));
            command.stdout(Stdio::from(stdout));
        } else {
            let invocation_arg = self.invocation_input_arg();
            command.arg(&invocation_arg);
            command.stdout(Stdio::null());
        }

        let mut child = command.spawn().map_err(EvalError::IoError)?;

        let output = if let Some(timeout) = self.timeout {
            loop {
                if let Some(status) = child.try_wait().map_err(EvalError::IoError)? {
                    let stderr = child.wait_with_output().map_err(EvalError::IoError)?.stderr;
                    break CompletedProcess { status, stderr };
                }

                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(EvalError::Timeout {
                        elapsed: started.elapsed(),
                    });
                }

                std::thread::sleep(Duration::from_millis(50));
            }
        } else {
            let output = child.wait_with_output().map_err(EvalError::IoError)?;
            CompletedProcess {
                status: output.status,
                stderr: output.stderr,
            }
        };

        if !output.status.success() {
            return Err(EvalError::ProcessFailed {
                exit_code: output.status.code(),
                stderr: truncate_stderr(&output.stderr),
            });
        }

        let output_path = self
            .output_candidates(workdir)
            .into_iter()
            .find(|path| path.exists())
            .ok_or_else(|| EvalError::ProcessFailed {
                exit_code: output.status.code(),
                stderr: format!(
                    "process exited successfully but none of the expected output files were created: {}",
                    self.output_candidates(workdir)
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;

        if !output_path.exists() {
            return Err(EvalError::ProcessFailed {
                exit_code: output.status.code(),
                stderr: format!(
                    "process exited successfully but {} was not created",
                    self.output_name
                ),
            });
        }

        let mut parsed = match self.mode {
            GulpRunMode::Relaxed => GotParser::parse_file(&output_path)?,
            GulpRunMode::SinglePointEnergy => parse_got_energy_only_file(&output_path, candidate)?,
        };
        parsed.wall_time = started.elapsed();
        parsed.relaxed_candidate.label = candidate.label.clone();
        if parsed.relaxed_candidate.len() != candidate.len() {
            warn!(
                candidate = %candidate.label,
                parsed_atoms = parsed.relaxed_candidate.len(),
                input_atoms = candidate.len(),
                "GULP returned a reduced or mismatched atom count; preserving input candidate geometry"
            );
            parsed.relaxed_candidate = candidate.clone();
        }
        if candidate.lattice.is_some()
            && parsed.relaxed_candidate.lattice.is_none()
            && parsed.relaxed_candidate.periodic_axes == [false, false, false]
        {
            parsed.relaxed_candidate.lattice = candidate.lattice;
            parsed.relaxed_candidate.periodic_axes = candidate.periodic_axes;
        }
        debug!(candidate = %candidate.label, energy = parsed.energy, "completed GULP evaluation");
        Ok(parsed)
    }
}

pub(crate) fn patch_gin_for_single_point(path: &Path) -> Result<(), EvalError> {
    let content = fs::read_to_string(path).map_err(EvalError::IoError)?;
    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    if let Some((index, line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.trim().is_empty())
    {
        let mut tokens = line
            .split_whitespace()
            .filter(|token| {
                !token.eq_ignore_ascii_case("opti")
                    && !token.eq_ignore_ascii_case("conj")
                    && !token.eq_ignore_ascii_case("bfgs")
                    && !token.eq_ignore_ascii_case("dfp")
                    && !token.eq_ignore_ascii_case("rfo")
                    && !token.eq_ignore_ascii_case("lbfgs")
                    && !token.eq_ignore_ascii_case("unit")
                    && !token.eq_ignore_ascii_case("conv")
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("single"))
        {
            tokens.insert(0, "single".to_string());
        }
        lines[index] = tokens.join(" ");
    }
    fs::write(path, lines.join("\n")).map_err(EvalError::IoError)
}

pub(crate) fn parse_got_energy_only_file(
    path: &Path,
    candidate: &Candidate,
) -> Result<EvalResult, EvalError> {
    let content = fs::read_to_string(path).map_err(EvalError::IoError)?;
    parse_got_energy_only_str(&content, candidate)
}

fn parse_got_energy_only_str(
    content: &str,
    candidate: &Candidate,
) -> Result<EvalResult, EvalError> {
    let mut energy = None;
    let mut explicit_failure = None;
    for (index, raw_line) in content.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim();
        if line.contains("Too many failed attempts to optimise")
            || line.contains("Too many failed attempts to optimize")
        {
            explicit_failure = Some((line_no, line.to_string()));
        }
        if line.contains("Total lattice energy") || line.contains("Surface energy (region 1)") {
            let tokens = line.split_whitespace().collect::<Vec<_>>();
            let unit_is_ev = tokens.iter().any(|token| token.eq_ignore_ascii_case("eV"));
            let value = tokens.iter().find_map(|token| token.parse::<f64>().ok());
            if unit_is_ev {
                energy = value;
                continue;
            }
            if energy.is_none() {
                energy = value;
            }
            continue;
        }
        if line.starts_with("Final energy =") {
            energy = Some(parse_gulp_energy_value(line, line_no)?);
        }
    }
    let energy = energy.ok_or_else(|| EvalError::ParseFailed {
        line: 0,
        reason: "missing single-point GULP energy line".into(),
    })?;
    if let Some((line, reason)) = explicit_failure {
        return Err(EvalError::ParseFailed {
            line,
            reason: format!("GULP reported optimization failure: `{reason}`"),
        });
    }
    Ok(EvalResult {
        energy,
        forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
        relaxed_candidate: candidate.clone(),
        converged: false,
        wall_time: Duration::default(),
    })
}

fn parse_gulp_energy_value(line: &str, line_no: usize) -> Result<f64, EvalError> {
    let value = line
        .split('=')
        .nth(1)
        .ok_or_else(|| EvalError::ParseFailed {
            line: line_no,
            reason: "missing `=` in final energy line".into(),
        })?
        .split_whitespace()
        .next()
        .ok_or_else(|| EvalError::ParseFailed {
            line: line_no,
            reason: "missing energy value".into(),
        })?;
    if value.chars().all(|ch| ch == '*') {
        return Err(EvalError::ParseFailed {
            line: line_no,
            reason: "GULP reported overflowed/non-finite final energy (`****************`); the structure or template likely caused numerical divergence".into(),
        });
    }
    value.parse::<f64>().map_err(|err| EvalError::ParseFailed {
        line: line_no,
        reason: format!("invalid energy value `{value}`: {err}"),
    })
}

/// Static files needed to construct a standalone SCOTT sandbox.
#[derive(Debug, Clone)]
pub struct ScottSandboxTemplate {
    /// Path to the checked-in `run.job` template.
    pub run_job_template: PathBuf,
    /// Path to the checked-in `atoms.in`.
    pub atoms_in_template: PathBuf,
    /// Optional path to the checked-in `jobs` file. When omitted, `run.job` is written directly.
    pub jobs_template: Option<PathBuf>,
}

impl ScottSandboxTemplate {
    /// Creates a template bundle from explicit paths.
    pub fn new(
        run_job_template: impl AsRef<Path>,
        atoms_in_template: impl AsRef<Path>,
        jobs_template: Option<impl AsRef<Path>>,
    ) -> Self {
        Self {
            run_job_template: run_job_template.as_ref().to_path_buf(),
            atoms_in_template: atoms_in_template.as_ref().to_path_buf(),
            jobs_template: jobs_template.map(|path| path.as_ref().to_path_buf()),
        }
    }
}

/// Subprocess-backed evaluator that stages a full SCOTT sandbox and launches `klmc_scott`.
///
/// This backend reflects the current local integration result: Rust should parallelise isolated
/// SCOTT subprocesses, not direct Fortran calls and not the legacy MPI taskfarm wrapper.
#[derive(Debug, Clone)]
pub struct ScottBackend {
    gin_writer: GinWriter,
    scott_bin: PathBuf,
    sandbox_template: ScottSandboxTemplate,
    timeout: Option<Duration>,
}

#[derive(Debug)]
struct ScottSandbox {
    gout_paths: Vec<PathBuf>,
    stderr_path: PathBuf,
    log_path: PathBuf,
}

impl ScottBackend {
    /// Creates a backend from the checked-in SCOTT runtime templates and executable path.
    pub fn new(
        master_gin_template: impl AsRef<Path>,
        scott_bin: impl AsRef<Path>,
        sandbox_template: ScottSandboxTemplate,
        timeout: Option<Duration>,
    ) -> Result<Self, EvalError> {
        Ok(Self {
            gin_writer: GinWriter::from_template_file(master_gin_template)?,
            scott_bin: scott_bin.as_ref().to_path_buf(),
            sandbox_template,
            timeout,
        })
    }

    fn stage_sandbox(
        &self,
        candidate: &Candidate,
        workdir: &Path,
    ) -> Result<ScottSandbox, EvalError> {
        let data_dir = workdir.join("data");
        let run_dir = workdir.join("run");
        let restart_dir = workdir.join("restart");
        fs::create_dir_all(&data_dir).map_err(EvalError::IoError)?;
        fs::create_dir_all(&run_dir).map_err(EvalError::IoError)?;
        fs::create_dir_all(&restart_dir).map_err(EvalError::IoError)?;

        let jobs_path = data_dir.join("jobs");
        match &self.sandbox_template.jobs_template {
            Some(template) => {
                fs::copy(template, &jobs_path).map_err(EvalError::IoError)?;
            }
            None => {
                fs::write(&jobs_path, "run.job\n").map_err(EvalError::IoError)?;
            }
        }

        fs::copy(
            &self.sandbox_template.run_job_template,
            data_dir.join("run.job"),
        )
        .map_err(EvalError::IoError)?;
        fs::copy(
            &self.sandbox_template.atoms_in_template,
            data_dir.join("atoms.in"),
        )
        .map_err(EvalError::IoError)?;

        let master_gin_path = data_dir.join("Master.gin");
        self.gin_writer
            .write_candidate(candidate, &master_gin_path)?;
        patch_scott_single_eval_run_job(&data_dir.join("run.job"))?;
        self.gin_writer
            .write_restart_xyz(candidate, restart_dir.join("seed.xyz"))?;

        Ok(ScottSandbox {
            gout_paths: vec![
                run_dir.join("gulp_klmc.gout"),
                run_dir.join("0").join("gulp_klmc.gout"),
                run_dir.join("A1_save.gout"),
                run_dir.join("0").join("A1_save.gout"),
            ],
            stderr_path: workdir.join("KLMC.err"),
            log_path: workdir.join("KLMC.log"),
        })
    }
}

impl BackendEvaluator for ScottBackend {
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError> {
        let started = Instant::now();
        let sandbox = self.stage_sandbox(candidate, workdir)?;

        let mut child = Command::new(&self.scott_bin)
            .current_dir(workdir)
            .env("OMPI_MCA_btl", "self")
            .env("OMPI_MCA_btl_base_warn_component_unused", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(EvalError::IoError)?;

        let output = if let Some(timeout) = self.timeout {
            loop {
                if let Some(status) = child.try_wait().map_err(EvalError::IoError)? {
                    let stderr = child.wait_with_output().map_err(EvalError::IoError)?.stderr;
                    break CompletedProcess { status, stderr };
                }

                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(EvalError::Timeout {
                        elapsed: started.elapsed(),
                    });
                }

                std::thread::sleep(Duration::from_millis(50));
            }
        } else {
            let output = child.wait_with_output().map_err(EvalError::IoError)?;
            CompletedProcess {
                status: output.status,
                stderr: output.stderr,
            }
        };

        if !output.status.success() {
            return Err(EvalError::ProcessFailed {
                exit_code: output.status.code(),
                stderr: build_scott_failure_context(
                    &output.stderr,
                    &sandbox.stderr_path,
                    &sandbox.log_path,
                ),
            });
        }

        let gout_path = sandbox
            .gout_paths
            .iter()
            .find(|path| path.exists())
            .cloned();

        let Some(gout_path) = gout_path else {
            let expected = sandbox
                .gout_paths
                .iter()
                .map(|path| format!("`{}`", path.display()))
                .collect::<Vec<_>>()
                .join(" or ");
            return Err(EvalError::ProcessFailed {
                exit_code: output.status.code(),
                stderr: format!(
                    "SCOTT exited successfully but neither {expected} was created. {}",
                    build_scott_failure_context(
                        &output.stderr,
                        &sandbox.stderr_path,
                        &sandbox.log_path
                    )
                ),
            });
        };

        let mut parsed = GotParser::parse_file(&gout_path)?;
        parsed.wall_time = started.elapsed();
        parsed.relaxed_candidate.label = candidate.label.clone();
        if parsed.relaxed_candidate.len() != candidate.len() {
            warn!(
                candidate = %candidate.label,
                parsed_atoms = parsed.relaxed_candidate.len(),
                input_atoms = candidate.len(),
                "SCOTT/GULP returned a reduced or mismatched atom count; preserving input candidate geometry"
            );
            parsed.relaxed_candidate = candidate.clone();
        }
        debug!(candidate = %candidate.label, energy = parsed.energy, "completed SCOTT evaluation");
        Ok(parsed)
    }
}

fn build_scott_failure_context(
    stderr: &[u8],
    klmc_err_path: &Path,
    klmc_log_path: &Path,
) -> String {
    let mut parts = Vec::new();

    let child_stderr = truncate_stderr(stderr);
    if !child_stderr.trim().is_empty() {
        parts.push(format!("child stderr: {child_stderr}"));
    }

    if let Ok(klmc_err) = fs::read_to_string(klmc_err_path) {
        let text: String = klmc_err.chars().take(500).collect();
        if !text.trim().is_empty() {
            parts.push(format!("KLMC.err: {text}"));
        }
    }

    if let Ok(klmc_log) = fs::read_to_string(klmc_log_path) {
        let tail = klmc_log
            .lines()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        if !tail.trim().is_empty() {
            parts.push(format!("KLMC.log tail: {tail}"));
        }
    }

    if parts.is_empty() {
        "no stderr, KLMC.err, or KLMC.log context was available".to_string()
    } else {
        parts.join(" ; ")
    }
}

fn patch_scott_single_eval_run_job(path: &Path) -> Result<(), EvalError> {
    let raw = fs::read_to_string(path).map_err(EvalError::IoError)?;
    let mut seen_job_type = false;
    let mut seen_restart_folder = false;
    let mut seen_dm_flag = false;
    let mut rewritten = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("JOB_TYPE:") {
            rewritten.push("JOB_TYPE:0".to_string());
            seen_job_type = true;
        } else if trimmed.starts_with("RESTART_FOLDER:") {
            rewritten.push("RESTART_FOLDER:'restart/'".to_string());
            seen_restart_folder = true;
        } else if trimmed.starts_with("DM_FLAG:") {
            rewritten.push("DM_FLAG:.FALSE.".to_string());
            seen_dm_flag = true;
        } else {
            rewritten.push(line.to_string());
        }
    }
    if !seen_job_type {
        rewritten.push("JOB_TYPE:0".to_string());
    }
    if !seen_restart_folder {
        rewritten.push("RESTART_FOLDER:'restart/'".to_string());
    }
    if !seen_dm_flag {
        rewritten.push("DM_FLAG:.FALSE.".to_string());
    }
    let mut rendered = rewritten.join("\n");
    rendered.push('\n');
    fs::write(path, rendered).map_err(EvalError::IoError)
}
