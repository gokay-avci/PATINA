use patina_types::{Candidate, EvalResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle, ThreadId};
use std::time::{Duration, Instant};
use tracing::warn;

use crate::backend_adapters::{
    absolute_workdir, cartesian_to_fractional, truncate_stderr, write_candidate_xyz,
    BackendEvaluator, CompletedProcess, EvalError,
};

/// Operating mode for the Janus/MACE adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JanusMode {
    /// Evaluate a single-point energy and forces without changing geometry.
    SinglePoint,
    /// Relax the structure with Janus/MACE before returning energy and forces.
    LocalOpt,
}

impl JanusMode {
    fn as_arg(self) -> &'static str {
        match self {
            Self::SinglePoint => "single_point",
            Self::LocalOpt => "local_opt",
        }
    }
}

/// Optimizer used by the Janus/MACE local-optimization adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JanusOptimizer {
    /// ASE LBFGS quasi-Newton optimizer.
    Lbfgs,
    /// ASE FIRE optimizer.
    Fire,
    /// ASE FIRE2 optimizer, if available in the installed ASE version.
    Fire2,
    /// ASE FIRE2 with ABC-FIRE enabled.
    AbcFire,
}

impl JanusOptimizer {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Lbfgs => "lbfgs",
            Self::Fire => "fire",
            Self::Fire2 => "fire2",
            Self::AbcFire => "abc-fire",
        }
    }
}

/// Static configuration for the Janus/MACE subprocess adapter.
#[derive(Debug, Clone)]
pub struct JanusMaceConfig {
    /// Python executable used to launch the adapter script.
    pub python_bin: PathBuf,
    /// Adapter script path. This stays configurable so driver/runtime code can override it without
    /// entangling Rust workflow logic with Python package installation details.
    pub adapter_script: PathBuf,
    /// Janus architecture, for example `mace_mp`.
    pub arch: String,
    /// Janus model card or local model path.
    pub model: String,
    /// Device name such as `cpu`, `cuda`, or `mps`.
    pub device: String,
    /// Numeric dtype for the Janus calculator.
    pub default_dtype: String,
    /// Whether Janus should run a single-point or local optimization.
    pub mode: JanusMode,
    /// Optimizer used for local optimization mode.
    pub optimizer: JanusOptimizer,
    /// Force threshold for local optimization mode.
    pub fmax: f64,
    /// Maximum optimization steps for local optimization mode.
    pub steps: usize,
}

impl Default for JanusMaceConfig {
    fn default() -> Self {
        Self {
            python_bin: crate::default_janus_python_bin(),
            adapter_script: crate::default_janus_adapter_script(),
            arch: "mace_mp".into(),
            model: "small".into(),
            device: "cpu".into(),
            default_dtype: "float64".into(),
            mode: JanusMode::SinglePoint,
            optimizer: JanusOptimizer::Fire2,
            fmax: 0.1,
            steps: 1000,
        }
    }
}

/// Subprocess-backed evaluator that adapts Janus/MACE through a small Python script.
///
/// This backend intentionally keeps Python and ML calculator details outside the Rust core.
/// Rust stages a normalized XYZ request, invokes the adapter, and only accepts an explicit
/// structured response back.
#[derive(Debug, Clone)]
pub struct JanusMaceBackend {
    config: JanusMaceConfig,
    timeout: Option<Duration>,
}

impl JanusMaceBackend {
    /// Creates a new Janus/MACE backend using the provided adapter settings.
    pub fn new(config: JanusMaceConfig, timeout: Option<Duration>) -> Self {
        Self { config, timeout }
    }
}

#[derive(Debug, Serialize)]
struct PersistentJanusServeRequest {
    command: &'static str,
    request_id: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct PersistentJanusServeEnvelope {
    status: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    result: Option<JanusAdapterResponse>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug)]
struct PersistentJanusWorker {
    stdin: Option<BufWriter<std::process::ChildStdin>>,
    stdout_rx: Receiver<Result<String, io::Error>>,
    stdout_thread: Option<JoinHandle<()>>,
    child: std::process::Child,
    stderr_log_path: PathBuf,
    steps: usize,
}

const PERSISTENT_JANUS_STDOUT_BUFFER_LINES: usize = 64;
const PERSISTENT_JANUS_MAX_PROTOCOL_LINE_BYTES: usize = 1 << 20;
const PERSISTENT_JANUS_SHUTDOWN_ACK_TIMEOUT: Duration = Duration::from_millis(250);
const PERSISTENT_JANUS_SHUTDOWN_EXIT_TIMEOUT: Duration = Duration::from_secs(2);

impl PersistentJanusWorker {
    fn new(
        config: &JanusMaceConfig,
        timeout: Option<Duration>,
        worker_dir: &Path,
    ) -> Result<Self, EvalError> {
        fs::create_dir_all(worker_dir).map_err(EvalError::IoError)?;
        let stderr_log_path = worker_dir.join("janus_worker.stderr.log");
        let stderr_log = fs::File::create(&stderr_log_path).map_err(EvalError::IoError)?;
        let py_cache_dir = worker_dir.join("py-cache");
        fs::create_dir_all(&py_cache_dir).map_err(EvalError::IoError)?;

        let mut child = Command::new(&config.python_bin)
            .current_dir(worker_dir)
            .env("MPLCONFIGDIR", &py_cache_dir)
            .arg(&config.adapter_script)
            .arg("--serve")
            .arg("--mode")
            .arg(config.mode.as_arg())
            .arg("--arch")
            .arg(&config.arch)
            .arg("--model")
            .arg(&config.model)
            .arg("--device")
            .arg(&config.device)
            .arg("--dtype")
            .arg(&config.default_dtype)
            .arg("--optimizer")
            .arg(config.optimizer.as_arg())
            .arg("--fmax")
            .arg(config.fmax.to_string())
            .arg("--steps")
            .arg(config.steps.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_log))
            .spawn()
            .map_err(EvalError::IoError)?;

        let stdin = child.stdin.take().ok_or_else(|| EvalError::ProcessFailed {
            exit_code: None,
            stderr: "persistent Janus worker did not expose stdin".into(),
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EvalError::ProcessFailed {
                exit_code: None,
                stderr: "persistent Janus worker did not expose stdout".into(),
            })?;

        let (tx, rx) = mpsc::sync_channel(PERSISTENT_JANUS_STDOUT_BUFFER_LINES);
        let stdout_thread = thread::spawn(move || {
            let _ = forward_worker_stdout(stdout, tx);
        });

        let mut worker = Self {
            stdin: Some(BufWriter::new(stdin)),
            stdout_rx: rx,
            stdout_thread: Some(stdout_thread),
            child,
            stderr_log_path,
            steps: config.steps,
        };
        worker.await_ready(timeout)?;
        Ok(worker)
    }

    fn await_ready(&mut self, timeout: Option<Duration>) -> Result<(), EvalError> {
        let envelope = self.recv_envelope(timeout, "ready line")?;
        if envelope.status != "ready" {
            return Err(EvalError::ProcessFailed {
                exit_code: self
                    .child
                    .try_wait()
                    .ok()
                    .flatten()
                    .and_then(|status| status.code()),
                stderr: envelope
                    .error
                    .unwrap_or_else(|| "persistent Janus worker did not report ready".into()),
            });
        }
        Ok(())
    }

    fn evaluate(
        &mut self,
        candidate: &Candidate,
        workdir: &Path,
        timeout: Option<Duration>,
    ) -> Result<EvalResult, EvalError> {
        let started = Instant::now();
        candidate.validate().map_err(|err| EvalError::ParseFailed {
            line: 0,
            reason: format!("invalid Janus candidate: {err:?}"),
        })?;

        let abs_workdir = absolute_workdir(workdir).map_err(EvalError::IoError)?;
        let xyz_path = abs_workdir.join("candidate.extxyz");
        let response_path = abs_workdir.join("janus_result.json");
        fs::create_dir_all(&abs_workdir).map_err(EvalError::IoError)?;
        write_candidate_xyz(candidate, &xyz_path)?;

        let request = PersistentJanusServeRequest {
            command: "evaluate",
            request_id: candidate.label.clone(),
            input: xyz_path.display().to_string(),
        };
        let payload = serde_json::to_string(&request).map_err(|err| EvalError::ParseFailed {
            line: 0,
            reason: format!("failed to encode persistent Janus request: {err}"),
        })?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| EvalError::ProcessFailed {
                exit_code: self
                    .child
                    .try_wait()
                    .ok()
                    .flatten()
                    .and_then(|status| status.code()),
                stderr: "persistent Janus worker stdin is no longer available".into(),
            })?;
        stdin
            .write_all(payload.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(EvalError::IoError)?;

        let envelope = self.recv_envelope(timeout, "response")?;

        if envelope.request_id.as_deref() != Some(candidate.label.as_str()) {
            return Err(EvalError::ParseFailed {
                line: 0,
                reason: format!(
                    "persistent Janus worker returned mismatched request id {:?} for candidate `{}`",
                    envelope.request_id, candidate.label
                ),
            });
        }

        if envelope.status != "ok" {
            return Err(EvalError::ProcessFailed {
                exit_code: self
                    .child
                    .try_wait()
                    .ok()
                    .flatten()
                    .and_then(|status| status.code()),
                stderr: envelope.error.unwrap_or_else(|| {
                    format!(
                        "persistent Janus worker returned non-ok status `{}`",
                        envelope.status
                    )
                }),
            });
        }

        let parsed = envelope.result.ok_or_else(|| EvalError::ParseFailed {
            line: 0,
            reason: "persistent Janus worker omitted result payload".into(),
        })?;
        fs::write(
            &response_path,
            serde_json::to_string_pretty(&parsed).map_err(|err| EvalError::ParseFailed {
                line: 0,
                reason: format!("failed to serialize persistent Janus response: {err}"),
            })?,
        )
        .map_err(EvalError::IoError)?;
        parse_janus_adapter_result(parsed, candidate, self.steps, started.elapsed())
    }

    fn recv_line(&mut self, timeout: Option<Duration>) -> Result<String, EvalError> {
        let received = if let Some(timeout) = timeout {
            self.stdout_rx.recv_timeout(timeout)
        } else {
            self.stdout_rx
                .recv()
                .map_err(|_| RecvTimeoutError::Disconnected)
        };

        match received {
            Ok(Ok(line)) => Ok(line),
            Ok(Err(err)) => Err(EvalError::IoError(err)),
            Err(RecvTimeoutError::Timeout) => Err(EvalError::Timeout {
                elapsed: timeout.unwrap_or_default(),
            }),
            Err(RecvTimeoutError::Disconnected) => Err(EvalError::ProcessFailed {
                exit_code: self
                    .child
                    .try_wait()
                    .ok()
                    .flatten()
                    .and_then(|status| status.code()),
                stderr: read_worker_stderr_excerpt(&self.stderr_log_path),
            }),
        }
    }

    fn recv_envelope(
        &mut self,
        timeout: Option<Duration>,
        context: &str,
    ) -> Result<PersistentJanusServeEnvelope, EvalError> {
        let deadline = timeout.map(|duration| Instant::now() + duration);
        loop {
            let remaining = deadline.map(|value| value.saturating_duration_since(Instant::now()));
            let line = self.recv_line(remaining)?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<PersistentJanusServeEnvelope>(trimmed) {
                Ok(envelope) => return Ok(envelope),
                Err(_) if !trimmed.starts_with('{') => continue,
                Err(err) => {
                    return Err(EvalError::ParseFailed {
                        line: 0,
                        reason: format!("failed to parse persistent Janus {context}: {err}"),
                    });
                }
            }
        }
    }

    fn shutdown(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            self.stdin.take();
            self.join_stdout_thread();
            return;
        }

        let payload = serde_json::json!({
            "command": "shutdown",
            "request_id": "shutdown",
        });
        if let Some(mut stdin) = self.stdin.take() {
            if let Ok(line) = serde_json::to_string(&payload) {
                let _ = stdin.write_all(line.as_bytes());
                let _ = stdin.write_all(b"\n");
                let _ = stdin.flush();
            }
        }

        let _ = self.recv_envelope(
            Some(PERSISTENT_JANUS_SHUTDOWN_ACK_TIMEOUT),
            "shutdown acknowledgement",
        );

        if !self.wait_for_exit(PERSISTENT_JANUS_SHUTDOWN_EXIT_TIMEOUT) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.join_stdout_thread();
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return false,
            }
        }
    }

    fn join_stdout_thread(&mut self) {
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
    }
}

fn forward_worker_stdout(
    stdout: std::process::ChildStdout,
    tx: SyncSender<Result<String, io::Error>>,
) -> io::Result<()> {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                if line.len() > PERSISTENT_JANUS_MAX_PROTOCOL_LINE_BYTES {
                    let err = io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "persistent Janus worker emitted an oversized stdout line ({} bytes)",
                            line.len()
                        ),
                    );
                    let _ = tx.send(Err(err));
                    return Ok(());
                }
                if tx.send(Ok(line)).is_err() {
                    return Ok(());
                }
            }
            Err(err) => {
                let _ = tx.send(Err(err));
                return Ok(());
            }
        }
    }
}

impl Drop for PersistentJanusWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
struct PersistentJanusSlot {
    worker: PersistentJanusWorker,
}

#[derive(Debug)]
struct PersistentJanusState {
    next_slot_index: usize,
    slots: HashMap<ThreadId, Arc<Mutex<PersistentJanusSlot>>>,
}

/// Thread-affine Janus backend that keeps one Python worker alive per calling thread.
#[derive(Debug)]
pub struct PersistentJanusMaceBackend {
    config: JanusMaceConfig,
    timeout: Option<Duration>,
    session_dir: PathBuf,
    keep_worker_dirs: bool,
    state: Mutex<PersistentJanusState>,
}

impl PersistentJanusMaceBackend {
    /// Creates a backend that lazily starts one persistent Janus worker per thread.
    pub fn new(
        config: JanusMaceConfig,
        timeout: Option<Duration>,
        session_dir: impl AsRef<Path>,
        keep_worker_dirs: bool,
    ) -> Result<Self, EvalError> {
        let session_dir = absolute_workdir(session_dir.as_ref()).map_err(EvalError::IoError)?;
        fs::create_dir_all(&session_dir).map_err(EvalError::IoError)?;
        Ok(Self {
            config,
            timeout,
            session_dir,
            keep_worker_dirs,
            state: Mutex::new(PersistentJanusState {
                next_slot_index: 0,
                slots: HashMap::new(),
            }),
        })
    }

    fn slot_for_current_thread(&self) -> Result<Arc<Mutex<PersistentJanusSlot>>, EvalError> {
        let thread_id = std::thread::current().id();
        {
            let state = self.state.lock().map_err(|_| EvalError::ProcessFailed {
                exit_code: None,
                stderr: "persistent Janus backend mutex was poisoned".into(),
            })?;
            if let Some(slot) = state.slots.get(&thread_id) {
                return Ok(Arc::clone(slot));
            }
        }

        let slot_index = {
            let mut state = self.state.lock().map_err(|_| EvalError::ProcessFailed {
                exit_code: None,
                stderr: "persistent Janus backend mutex was poisoned".into(),
            })?;
            let slot_index = state.next_slot_index;
            state.next_slot_index += 1;
            slot_index
        };
        let worker_dir = self.session_dir.join(format!("worker_{slot_index:04}"));
        let worker = PersistentJanusWorker::new(&self.config, self.timeout, &worker_dir)?;
        let slot = Arc::new(Mutex::new(PersistentJanusSlot { worker }));

        let mut state = self.state.lock().map_err(|_| EvalError::ProcessFailed {
            exit_code: None,
            stderr: "persistent Janus backend mutex was poisoned".into(),
        })?;
        let existing = state
            .slots
            .entry(thread_id)
            .or_insert_with(|| Arc::clone(&slot));
        Ok(Arc::clone(existing))
    }
}

impl BackendEvaluator for PersistentJanusMaceBackend {
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError> {
        let slot = self.slot_for_current_thread()?;
        let mut slot = slot.lock().map_err(|_| EvalError::ProcessFailed {
            exit_code: None,
            stderr: "persistent Janus worker mutex was poisoned".into(),
        })?;
        slot.worker.evaluate(candidate, workdir, self.timeout)
    }
}

impl Drop for PersistentJanusMaceBackend {
    fn drop(&mut self) {
        let slots = match self.state.get_mut() {
            Ok(state) => std::mem::take(&mut state.slots),
            Err(err) => {
                warn!("persistent Janus backend mutex was poisoned during drop");
                std::mem::take(&mut err.into_inner().slots)
            }
        };
        drop(slots);

        if !self.keep_worker_dirs {
            if let Err(err) = fs::remove_dir_all(&self.session_dir) {
                warn!(
                    path = %self.session_dir.display(),
                    error = %err,
                    "failed to remove persistent Janus session directory"
                );
            }
        }
    }
}

impl BackendEvaluator for JanusMaceBackend {
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError> {
        let started = Instant::now();
        candidate.validate().map_err(|err| EvalError::ParseFailed {
            line: 0,
            reason: format!("invalid Janus candidate: {err:?}"),
        })?;

        let abs_workdir = absolute_workdir(workdir).map_err(EvalError::IoError)?;
        let xyz_path = abs_workdir.join("candidate.extxyz");
        let response_path = abs_workdir.join("janus_result.json");
        let py_cache_dir = abs_workdir.join("py-cache");
        fs::create_dir_all(&py_cache_dir).map_err(EvalError::IoError)?;
        write_candidate_xyz(candidate, &xyz_path)?;

        let mut child = Command::new(&self.config.python_bin)
            .current_dir(&abs_workdir)
            .env("MPLCONFIGDIR", &py_cache_dir)
            .arg(&self.config.adapter_script)
            .arg("--input")
            .arg(&xyz_path)
            .arg("--output")
            .arg(&response_path)
            .arg("--mode")
            .arg(self.config.mode.as_arg())
            .arg("--arch")
            .arg(&self.config.arch)
            .arg("--model")
            .arg(&self.config.model)
            .arg("--device")
            .arg(&self.config.device)
            .arg("--dtype")
            .arg(&self.config.default_dtype)
            .arg("--optimizer")
            .arg(self.config.optimizer.as_arg())
            .arg("--fmax")
            .arg(self.config.fmax.to_string())
            .arg("--steps")
            .arg(self.config.steps.to_string())
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
                stderr: truncate_stderr(&output.stderr),
            });
        }

        if !response_path.exists() {
            return Err(EvalError::ProcessFailed {
                exit_code: output.status.code(),
                stderr: "janus adapter exited successfully but no response JSON was created".into(),
            });
        }

        let raw = fs::read_to_string(&response_path).map_err(EvalError::IoError)?;
        let parsed: JanusAdapterResponse =
            serde_json::from_str(&raw).map_err(|err| EvalError::ParseFailed {
                line: 0,
                reason: format!("failed to parse Janus adapter JSON: {err}"),
            })?;
        parse_janus_adapter_result(parsed, candidate, self.config.steps, started.elapsed())
    }
}

fn parse_janus_adapter_result(
    parsed: JanusAdapterResponse,
    candidate: &Candidate,
    steps: usize,
    wall_time: Duration,
) -> Result<EvalResult, EvalError> {
    if parsed.species.len() != parsed.coords.len() {
        return Err(EvalError::ParseFailed {
            line: 0,
            reason: "Janus adapter returned mismatched species/coordinate lengths".into(),
        });
    }

    let relaxed_candidate = match (parsed.lattice, parsed.periodic_axes) {
        (Some(lattice), Some(periodic_axes)) => Candidate {
            species: parsed.species,
            fractional_coords: parsed
                .coords
                .into_iter()
                .map(|coord| cartesian_to_fractional(coord, lattice))
                .collect::<Result<Vec<_>, _>>()?,
            lattice: Some(lattice),
            periodic_axes,
            label: candidate.label.clone(),
        },
        (Some(lattice), None) => Candidate {
            species: parsed.species,
            fractional_coords: parsed
                .coords
                .into_iter()
                .map(|coord| cartesian_to_fractional(coord, lattice))
                .collect::<Result<Vec<_>, _>>()?,
            lattice: Some(lattice),
            periodic_axes: [true, true, true],
            label: candidate.label.clone(),
        },
        (None, Some(periodic_axes)) if periodic_axes == [false, false, false] => Candidate {
            species: parsed.species,
            fractional_coords: parsed.coords,
            lattice: None,
            periodic_axes,
            label: candidate.label.clone(),
        },
        (None, Some(periodic_axes)) => {
            return Err(EvalError::ParseFailed {
                line: 0,
                reason: format!(
                    "Janus adapter returned periodic axes {periodic_axes:?} without a lattice"
                ),
            });
        }
        (None, None) => Candidate {
            species: parsed.species,
            fractional_coords: parsed.coords,
            lattice: None,
            periodic_axes: [false, false, false],
            label: candidate.label.clone(),
        },
    };
    relaxed_candidate
        .validate()
        .map_err(|err| EvalError::ParseFailed {
            line: 0,
            reason: format!("Janus adapter returned invalid relaxed geometry: {err:?}"),
        })?;

    let forces = if parsed.forces.is_empty() {
        vec![[0.0, 0.0, 0.0]; relaxed_candidate.len()]
    } else {
        if parsed.forces.len() != relaxed_candidate.len() {
            return Err(EvalError::ParseFailed {
                line: 0,
                reason: "Janus adapter returned mismatched force count".into(),
            });
        }
        parsed.forces
    };

    let result = EvalResult {
        energy: parsed.energy,
        forces,
        relaxed_candidate,
        converged: parsed.converged,
        wall_time,
    };

    if !parsed.converged {
        return Err(EvalError::NotConverged {
            energy: parsed.energy,
            n_steps: steps,
            partial_result: Some(Box::new(result)),
        });
    }

    Ok(result)
}

fn read_worker_stderr_excerpt(path: &Path) -> String {
    fs::read_to_string(path)
        .map(|text| text.chars().take(500).collect())
        .unwrap_or_else(|_| "persistent Janus worker exited without stderr excerpt".into())
}

#[derive(Debug, Deserialize, Serialize)]
struct JanusAdapterResponse {
    energy: f64,
    #[serde(default)]
    forces: Vec<[f64; 3]>,
    species: Vec<String>,
    coords: Vec<[f64; 3]>,
    #[serde(default)]
    lattice: Option<[[f64; 3]; 3]>,
    #[serde(default)]
    periodic_axes: Option<[bool; 3]>,
    #[serde(default = "default_true")]
    converged: bool,
}

fn default_true() -> bool {
    true
}
