use anyhow::{anyhow, Context, Result};
use patina_external::{BackendEvaluator, GulpBackend};
use patina_types::BhScientificConfig;
use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::driver_support::{
    absolutize_path, build_external_hashkey_with_radius_offset, candidate_hashkey_cache_key,
    ensure_native_scott_search_candidate, read_candidate_json, required_arg, EvalBackendKind,
    JanusModeSetting, JanusOptimizerSetting,
};
use super::scott_topology_types::AtomSpecRecord;
use super::workflow_provenance::{build_workflow_provenance, WorkflowProvenanceSpec};

#[derive(Debug, Clone)]
pub(crate) struct LocalSamplingBackendConfig {
    pub backend: EvalBackendKind,
    pub timeout: Option<Duration>,
    pub executable: Option<PathBuf>,
    pub master_gin_template: Option<PathBuf>,
    pub run_job_template: Option<PathBuf>,
    pub atoms_in_template: Option<PathBuf>,
    pub jobs_template: Option<PathBuf>,
    pub python_bin: Option<PathBuf>,
    pub janus_adapter_script: Option<PathBuf>,
    pub janus_arch: String,
    pub janus_model: String,
    pub janus_device: String,
    pub janus_dtype: String,
    pub janus_mode: JanusModeSetting,
    pub janus_optimizer: JanusOptimizerSetting,
    pub janus_fmax: f64,
    pub janus_steps: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct BasinHoppingRunRequest {
    pub candidate_json: PathBuf,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub scientific_config: BhScientificConfig,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
    pub backend: LocalSamplingBackendConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct EnergyLidRunRequest {
    pub source_run_dir: PathBuf,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub top_n: usize,
    pub lid_levels: usize,
    pub lid_increment: f64,
    pub steps_per_lid: usize,
    pub quench_steps: usize,
    pub runners_per_lid: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
    pub backend: LocalSamplingBackendConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct SimulatedAnnealingRunRequest {
    pub source_run_dir: PathBuf,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub top_n: usize,
    pub anneal_steps: usize,
    pub initial_temperature: f64,
    pub temperature_scale: f64,
    pub hold_steps: usize,
    pub quench_steps: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
    pub backend: LocalSamplingBackendConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct ScanSurfaceRunRequest {
    pub initial_candidate_json: PathBuf,
    pub restart_candidate_json: Vec<PathBuf>,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub steps_per_scan: usize,
    pub temperature: f64,
    pub base_step_size: f64,
    pub dynamic_threshold: usize,
    pub move_mode: super::ports::ScanSurfaceMoveMode,
    pub acceptance_mode: super::ports::ScanSurfaceAcceptanceMode,
    pub evaluate_initial_state: bool,
    pub center: [f64; 3],
    pub boundary: [f64; 3],
    pub above_surface: bool,
    pub seed: u64,
    pub backend: LocalSamplingBackendConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct SolidSolutionsRunRequest {
    pub initial_candidate_json: PathBuf,
    pub proposal_candidate_json: Vec<PathBuf>,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub steps: usize,
    pub temperature: f64,
    pub move_mode: super::ports::SolidSolutionsMoveMode,
    pub acceptance_mode: super::ports::SolidSolutionsAcceptanceMode,
    pub max_exchanges: usize,
    pub skip_evaluation: bool,
    pub imported_hashkeys: Vec<String>,
    pub imported_hashkey_files: Vec<PathBuf>,
    pub atoms_in_template: Option<PathBuf>,
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub geometry_min_distance: Option<f64>,
    pub statistics_enabled: bool,
    pub statistics_backup_interval: usize,
    pub rdf_config: Option<super::solid_solutions::SolidSolutionsRdfConfig>,
    pub seed: u64,
    pub backend: LocalSamplingBackendConfig,
}

struct LocalSamplingStartPort;

impl super::ports::SamplingStartPort for LocalSamplingStartPort {
    fn load_starts(
        &self,
        source_run_dir: &Path,
        top_n: usize,
    ) -> Result<Vec<super::energy_lid::EnergyLidStart>> {
        super::energy_lid::load_top_structures_from_ga_run(source_run_dir, top_n)
    }
}

struct LocalSamplingEvaluationPort<'a> {
    search_backend: &'a dyn BackendEvaluator,
    relax_backend: &'a dyn BackendEvaluator,
}

impl super::ports::SamplingEvaluationPort for LocalSamplingEvaluationPort<'_> {
    fn evaluate(
        &self,
        request: &super::ports::SamplingEvaluationRequest,
    ) -> Result<patina_types::EvalResult> {
        request.validate_shape()?;
        fs::create_dir_all(&request.eval_dir).with_context(|| {
            format!(
                "failed to create sampling evaluation dir `{}`",
                request.eval_dir.display()
            )
        })?;
        let backend = if request.intent.uses_relaxation_backend() {
            self.relax_backend
        } else {
            self.search_backend
        };
        backend
            .evaluate(&request.candidate, &request.eval_dir)
            .map_err(Into::into)
    }
}

struct NoopSamplingEvaluationPort;

impl super::ports::SamplingEvaluationPort for NoopSamplingEvaluationPort {
    fn evaluate(
        &self,
        request: &super::ports::SamplingEvaluationRequest,
    ) -> Result<patina_types::EvalResult> {
        Err(anyhow!(
            "sampling evaluation was requested for {:?}/{:?} while evaluation is disabled",
            request.workflow,
            request.intent
        ))
    }
}

enum LocalSamplingArtifactKind {
    EnergyLid { parameters: serde_json::Value },
    SimulatedAnnealing { parameters: serde_json::Value },
}

struct LocalSamplingArtifactSink {
    run_dir: PathBuf,
    workdir: PathBuf,
    system: String,
    source_run_dir: PathBuf,
    backend: EvalBackendKind,
    backend_metadata: Option<serde_json::Value>,
    engine: String,
    kind: LocalSamplingArtifactKind,
}

impl super::ports::SamplingArtifactSink for LocalSamplingArtifactSink {
    fn persist_energy_lid_run(
        &self,
        execution: &super::energy_lid::EnergyLidWorkflowExecution,
    ) -> Result<()> {
        let LocalSamplingArtifactKind::EnergyLid { parameters } = &self.kind else {
            return Err(anyhow!(
                "simulated annealing artifact sink cannot persist energy-lid execution"
            ));
        };
        let mut typed_window_kernel_count = 0usize;
        let mut typed_runner_kernel_count = 0usize;
        for structure in &execution.structures {
            anyhow::ensure!(
                structure.summary.windows.len() == structure.windows.len(),
                "energy-lid summary/kernel window count mismatch for `{}`",
                structure.summary.source.label
            );
            for (summary_window, kernel_window) in structure
                .summary
                .windows
                .iter()
                .zip(structure.windows.iter())
            {
                anyhow::ensure!(
                    summary_window.lid_index == kernel_window.summary.lid_index,
                    "energy-lid window kernel drift for `{}` lid {}",
                    structure.summary.source.label,
                    summary_window.lid_index
                );
                let _accepted_steps = kernel_window.state.lid_walker.accepted_steps;
                typed_window_kernel_count += 1;
                typed_runner_kernel_count += kernel_window.state.runner_walkers.len();
            }
        }
        fs::create_dir_all(self.run_dir.join("raw").join("energy_lid"))?;
        fs::create_dir_all(self.run_dir.join("traces").join("energy_lid"))?;
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("energy_lid"),
                "workflow_id": "sampling.energy-lid",
                "mode": "energy_lid",
                "system": self.system,
                "engine": self.engine,
                "source_run_dir": self.source_run_dir,
                "backend": self.backend,
                "backend_metadata": self.backend_metadata,
                "provenance": serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "sampling.energy-lid",
                    workflow_owner: "sampling_energy_lid",
                    backend_id: Some(self.backend.provenance_id()),
                    backend_mode: None,
                    lane_mode: None,
                    duplicate_policy_mode: None,
                    parallel_contract: None,
                    run_dir: Some(self.run_dir.as_path()),
                    workdir: Some(self.workdir.as_path()),
                    source_run_dir: Some(self.source_run_dir.as_path()),
                    source_path: None,
                    candidate_json: None,
                    template_bundle: None,
                })?)?,
                "artifacts": {
                    "energy_lid_summary": "raw/energy_lid_summary.json",
                    "energy_lid_traces": "traces/energy_lid/"
                },
                "typed_kernel_window_count": typed_window_kernel_count,
                "typed_kernel_runner_count": typed_runner_kernel_count,
                "parameters": parameters,
            }))?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;
        fs::write(
            self.run_dir.join("raw").join("energy_lid_summary.json"),
            serde_json::to_string_pretty(&execution.summary)
                .context("failed to serialize energy-lid summary")?,
        )?;
        for (index, structure) in execution.structures.iter().enumerate() {
            fs::write(
                self.run_dir
                    .join("raw")
                    .join("energy_lid")
                    .join(format!("structure_{index:02}_summary.json")),
                serde_json::to_string_pretty(&structure.summary.windows)?,
            )?;
            let trace_root = self.run_dir.join(format!(
                "traces/energy_lid/structure_{index:02}_{}",
                structure.summary.source.label
            ));
            super::rust_mc_artifacts::write_rust_mc_trace_csv(
                &trace_root,
                &structure.combined_trace,
            )?;
        }
        Ok(())
    }

    fn persist_simulated_annealing_run(
        &self,
        execution: &super::energy_lid::SimulatedAnnealingWorkflowExecution,
    ) -> Result<()> {
        let LocalSamplingArtifactKind::SimulatedAnnealing { parameters } = &self.kind else {
            return Err(anyhow!(
                "energy-lid artifact sink cannot persist simulated annealing execution"
            ));
        };
        let typed_anneal_kernel_count = execution.structures.len();
        let typed_quench_kernel_count = execution
            .structures
            .iter()
            .filter(|structure| structure.state.quench_walker.is_some())
            .count();
        let typed_accepted_steps: usize = execution
            .structures
            .iter()
            .map(|structure| structure.state.anneal_walker.accepted_steps)
            .sum();
        fs::create_dir_all(self.run_dir.join("raw").join("simulated_annealing"))?;
        fs::create_dir_all(self.run_dir.join("traces").join("simulated_annealing"))?;
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("simulated_annealing"),
                "workflow_id": "sampling.simulated-annealing",
                "mode": "simulated_annealing",
                "system": self.system,
                "engine": self.engine,
                "source_run_dir": self.source_run_dir,
                "backend": self.backend,
                "backend_metadata": self.backend_metadata,
                "provenance": serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "sampling.simulated-annealing",
                    workflow_owner: "sampling_simulated_annealing",
                    backend_id: Some(self.backend.provenance_id()),
                    backend_mode: None,
                    lane_mode: None,
                    duplicate_policy_mode: None,
                    parallel_contract: None,
                    run_dir: Some(self.run_dir.as_path()),
                    workdir: Some(self.workdir.as_path()),
                    source_run_dir: Some(self.source_run_dir.as_path()),
                    source_path: None,
                    candidate_json: None,
                    template_bundle: None,
                })?)?,
                "artifacts": {
                    "simulated_annealing_summary": "raw/simulated_annealing_summary.json",
                    "simulated_annealing_traces": "traces/simulated_annealing/"
                },
                "typed_anneal_kernel_count": typed_anneal_kernel_count,
                "typed_quench_kernel_count": typed_quench_kernel_count,
                "typed_accepted_steps": typed_accepted_steps,
                "parameters": parameters,
            }))?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;
        fs::write(
            self.run_dir
                .join("raw")
                .join("simulated_annealing_summary.json"),
            serde_json::to_string_pretty(&execution.summary)
                .context("failed to serialize simulated annealing summary")?,
        )?;
        for (index, structure) in execution.structures.iter().enumerate() {
            fs::write(
                self.run_dir
                    .join("raw")
                    .join("simulated_annealing")
                    .join(format!("structure_{index:02}_summary.json")),
                serde_json::to_string_pretty(&structure.summary)?,
            )?;
            let trace_root = self.run_dir.join(format!(
                "traces/simulated_annealing/structure_{index:02}_{}",
                structure.summary.source.label
            ));
            super::rust_mc_artifacts::write_rust_mc_trace_csv(&trace_root, &structure.trace)?;
        }
        Ok(())
    }
}

struct LocalBasinHoppingArtifactSink {
    run_dir: PathBuf,
    workdir: PathBuf,
    system: String,
    candidate_json: PathBuf,
    backend: EvalBackendKind,
    backend_metadata: Option<serde_json::Value>,
    engine: String,
    parameters: serde_json::Value,
}

struct LocalBasinHoppingLayoutPort {
    workdir: PathBuf,
}

impl super::ports::BasinHoppingEvaluationLayoutPort for LocalBasinHoppingLayoutPort {
    fn initial_eval_dir(&self, walker_id: usize) -> PathBuf {
        self.workdir
            .join(format!("walker_{walker_id:04}"))
            .join("initial")
    }

    fn step_eval_dir(&self, walker_id: usize, step_index: usize) -> PathBuf {
        self.workdir
            .join(format!("walker_{walker_id:04}"))
            .join(format!("step_{step_index:04}"))
    }
}

struct NullBasinHoppingIdentityPort;

impl super::ports::BasinHoppingArtifactSink for LocalBasinHoppingArtifactSink {
    fn persist_basin_hopping_run(
        &self,
        execution: &super::basin_hopping::BasinHoppingWorkflowExecution,
    ) -> Result<()> {
        let typed_walker_kernel_count = execution.walkers.len();
        let typed_accepted_steps: usize = execution
            .walkers
            .iter()
            .map(|walker| walker.kernel_state.accepted_steps)
            .sum();
        let typed_rejected_steps: usize = execution
            .walkers
            .iter()
            .map(|walker| walker.kernel_state.rejected_steps)
            .sum();
        anyhow::ensure!(
            typed_accepted_steps == execution.summary.accepted_steps,
            "basin-hopping accepted-step kernel drift detected"
        );
        anyhow::ensure!(
            typed_rejected_steps == execution.summary.rejected_steps,
            "basin-hopping rejected-step kernel drift detected"
        );
        fs::create_dir_all(self.run_dir.join("raw"))?;
        fs::create_dir_all(self.run_dir.join("outputs").join("walker_states"))?;
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("basin_hopping"),
                "workflow_id": "sampling.basin-hopping",
                "mode": "basin_hopping",
                "system": self.system,
                "engine": self.engine,
                "candidate_json": self.candidate_json,
                "backend": self.backend,
                "backend_metadata": self.backend_metadata,
                "provenance": serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "sampling.basin-hopping",
                    workflow_owner: "sampling_basin_hopping",
                    backend_id: Some(self.backend.provenance_id()),
                    backend_mode: None,
                    lane_mode: None,
                    duplicate_policy_mode: None,
                    parallel_contract: None,
                    run_dir: Some(self.run_dir.as_path()),
                    workdir: Some(self.workdir.as_path()),
                    source_run_dir: None,
                    source_path: None,
                    candidate_json: Some(self.candidate_json.as_path()),
                    template_bundle: None,
                })?)?,
                "artifacts": {
                    "basin_hopping_summary": "raw/basin_hopping_summary.json",
                    "walker_trace": "traces/walker_trace.csv",
                    "walker_states": "outputs/walker_states/"
                },
                "typed_walker_kernel_count": typed_walker_kernel_count,
                "typed_accepted_steps": typed_accepted_steps,
                "typed_rejected_steps": typed_rejected_steps,
                "parameters": self.parameters,
            }))?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;
        fs::write(
            self.run_dir.join("raw").join("basin_hopping_summary.json"),
            serde_json::to_string_pretty(&execution.summary)
                .context("failed to serialize basin-hopping summary")?,
        )?;
        fs::write(
            self.run_dir.join("raw").join("bh_walker_states.json"),
            serde_json::to_string_pretty(
                &execution
                    .walkers
                    .iter()
                    .map(|walker| walker.walker_state.clone())
                    .collect::<Vec<_>>(),
            )?,
        )?;
        for (walker_id, walker) in execution.walkers.iter().enumerate() {
            fs::write(
                self.run_dir
                    .join("outputs")
                    .join("walker_states")
                    .join(format!("walker_{walker_id:04}.json")),
                serde_json::to_string_pretty(&walker.walker_state)?,
            )?;
        }
        super::rust_bh_artifacts::write_rust_bh_walker_trace_csv(&self.run_dir, &execution.trace)?;
        Ok(())
    }
}

impl super::ports::BasinHoppingIdentityPort for NullBasinHoppingIdentityPort {
    fn compare_matched_relaxation(
        &self,
        _request: &super::ports::BasinHoppingIdentityRequest,
    ) -> Result<Option<super::ports::BasinHoppingMatchedRelaxationComparison>> {
        Ok(None)
    }
}

struct LocalScanSurfaceMovePort {
    config: super::scan_surface::ScanSurfaceMoveConfig,
    rng: RefCell<LocalWorkflowRng>,
}

impl super::ports::ScanSurfaceMovePort for LocalScanSurfaceMovePort {
    fn propose_candidate(
        &self,
        request: &super::ports::ScanSurfaceMoveRequest,
    ) -> Result<patina_types::Candidate> {
        let mut rng = self.rng.borrow_mut();
        super::scan_surface::propose_scan_surface_candidate(
            request,
            self.config,
            super::scan_surface::ScanSurfaceMoveDraws {
                rotation: [rng.next_f64(), rng.next_f64()],
                displacement: [rng.next_f64(), rng.next_f64(), rng.next_f64()],
            },
        )
    }
}

struct LocalScanSurfaceArtifactSink {
    run_dir: PathBuf,
    system: String,
    initial_candidate_json: PathBuf,
    restart_candidate_json: Vec<PathBuf>,
    backend: EvalBackendKind,
    backend_metadata: Option<serde_json::Value>,
    parameters: serde_json::Value,
}

impl super::ports::ScanSurfaceArtifactSink for LocalScanSurfaceArtifactSink {
    fn persist_scan_surface_run(
        &self,
        execution: &super::scan_surface::ScanSurfaceWorkflowExecution,
    ) -> Result<()> {
        fs::create_dir_all(self.run_dir.join("raw").join("scan_surface"))?;
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("scan_surface"),
                "mode": "scan_surface",
                "system": self.system,
                "engine": format!("Rust ScanSurface ({})", self.backend.engine_label()),
                "initial_candidate_json": self.initial_candidate_json,
                "restart_candidate_json": self.restart_candidate_json,
                "backend": self.backend,
                "backend_metadata": self.backend_metadata,
                "artifacts": {
                    "scan_surface_summary": "raw/scan_surface_summary.json",
                    "scan_surface_scans": "raw/scan_surface/"
                },
                "parameters": self.parameters,
            }))?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;
        fs::write(
            self.run_dir.join("raw").join("scan_surface_summary.json"),
            serde_json::to_string_pretty(&execution.summary)
                .context("failed to serialize scan-surface summary")?,
        )?;
        for scan in &execution.scans {
            fs::write(
                self.run_dir
                    .join("raw")
                    .join("scan_surface")
                    .join(format!("scan_{:04}.json", scan.scan_index)),
                serde_json::to_string_pretty(scan)
                    .context("failed to serialize scan-surface scan")?,
            )?;
        }
        Ok(())
    }
}

struct LocalSolidSolutionsLibraryPort {
    imported_hashkeys: Vec<String>,
}

impl super::ports::SolidSolutionsLibraryPort for LocalSolidSolutionsLibraryPort {
    fn load_imported_hashkeys(
        &self,
        _request: &super::ports::SolidSolutionsLibraryRequest,
    ) -> Result<Vec<String>> {
        Ok(self.imported_hashkeys.clone())
    }
}

struct LocalSolidSolutionsIdentityPort {
    atom_specs: Option<Vec<AtomSpecRecord>>,
    use_dreadnaut_keys: bool,
    hashkey_radius: String,
    hashkey_radius_const: f64,
    scratch_dir: PathBuf,
}

impl super::ports::SolidSolutionsIdentityPort for LocalSolidSolutionsIdentityPort {
    fn build_hashkey(&self, candidate: &patina_types::Candidate) -> Result<Option<String>> {
        let Some(atom_specs) = &self.atom_specs else {
            return Ok(None);
        };
        if !self.use_dreadnaut_keys {
            return Ok(None);
        }
        let hkg_path = patina_dreadnaut::bundled_dreadnaut_path();
        fs::create_dir_all(&self.scratch_dir).with_context(|| {
            format!(
                "failed to create solid-solution hashkey scratch dir `{}`",
                self.scratch_dir.display()
            )
        })?;
        build_external_hashkey_with_radius_offset(
            candidate,
            Some(atom_specs),
            &self.hashkey_radius,
            self.hashkey_radius_const,
            self.hashkey_radius_const,
            &hkg_path,
            &self.scratch_dir,
            &format!(
                "solid_solution_{:016x}",
                candidate_hashkey_cache_key(candidate)
            ),
        )
            .and_then(|hashkey| {
                hashkey.ok_or_else(|| {
                    anyhow!(
                    "solid-solution Dreadnaut hashkey generation returned no hash for `{}` using configured path `{}`",
                    candidate.label,
                    hkg_path.display()
                )
                })
                .map(Some)
        })
    }
}

struct LocalSolidSolutionsGeometryPort {
    minimum_distance: Option<f64>,
}

impl super::ports::SolidSolutionsGeometryPort for LocalSolidSolutionsGeometryPort {
    fn validate_geometry(&self, candidate: &patina_types::Candidate) -> Result<()> {
        candidate.validate().map_err(|error| {
            anyhow!(
                "invalid solid-solution candidate `{}`: {error:?}",
                candidate.label
            )
        })?;
        if candidate.has_partial_periodicity() {
            return Err(anyhow!(
                "solid-solution candidate `{}` uses unsupported partial periodicity",
                candidate.label
            ));
        }
        if let Some(minimum_distance) = self.minimum_distance {
            if !minimum_distance.is_finite() || minimum_distance < 0.0 {
                return Err(anyhow!(
                    "solid-solution minimum distance must be finite and non-negative"
                ));
            }
            if let Some(observed) = minimum_pair_distance(candidate) {
                if observed < minimum_distance {
                    return Err(anyhow!(
                        "solid-solution candidate `{}` violates minimum distance: observed {observed}, required {minimum_distance}",
                        candidate.label
                    ));
                }
            }
        }
        Ok(())
    }
}

struct LocalSolidSolutionsMovePort {
    rng: RefCell<LocalWorkflowRng>,
}

impl super::ports::SolidSolutionsMovePort for LocalSolidSolutionsMovePort {
    fn propose_candidate(
        &self,
        request: &super::ports::SolidSolutionsMoveRequest,
    ) -> Result<patina_types::Candidate> {
        let mut rng = self.rng.borrow_mut();
        let exchanges = 1 + rng.next_usize(request.max_exchanges);
        let draw_count = match request.move_mode {
            super::ports::SolidSolutionsMoveMode::MixSolution => exchanges.saturating_mul(2),
            super::ports::SolidSolutionsMoveMode::RandomizeSolution => {
                request.initial_candidate.len()
            }
        }
        .max(1);
        let draws = (0..draw_count).map(|_| rng.next_f64()).collect::<Vec<_>>();
        super::solid_solutions::propose_solid_solution_candidate(request, exchanges, &draws)
    }
}

struct LocalSolidSolutionsArtifactSink {
    run_dir: PathBuf,
    system: String,
    initial_candidate_json: PathBuf,
    proposal_candidate_json: Vec<PathBuf>,
    backend: Option<EvalBackendKind>,
    backend_metadata: Option<serde_json::Value>,
    parameters: serde_json::Value,
    hashkey_metadata: serde_json::Value,
    temperature: f64,
    statistics_enabled: bool,
    statistics_backup_interval: usize,
    rdf_config: Option<super::solid_solutions::SolidSolutionsRdfConfig>,
}

impl super::ports::SolidSolutionsArtifactSink for LocalSolidSolutionsArtifactSink {
    fn persist_solid_solutions_run(
        &self,
        execution: &super::solid_solutions::SolidSolutionsWorkflowExecution,
    ) -> Result<()> {
        let statistics_rows = self
            .statistics_enabled
            .then(|| super::solid_solutions::solid_solution_statistics_rows(execution));
        let hashkey_statistics_rows = self.statistics_enabled.then(|| {
            super::solid_solutions::solid_solution_hashkey_statistics_rows(&execution.state)
        });
        let rdf_report = self
            .rdf_config
            .as_ref()
            .map(|config| {
                super::solid_solutions::build_solid_solutions_rdf_report(
                    execution,
                    config,
                    self.temperature,
                )
            })
            .transpose()?;
        let mut artifacts = serde_json::Map::new();
        artifacts.insert(
            "solid_solutions_summary".to_string(),
            serde_json::json!("raw/solid_solutions_summary.json"),
        );
        artifacts.insert(
            "solid_solutions_initial".to_string(),
            serde_json::json!("raw/solid_solutions_initial.json"),
        );
        artifacts.insert(
            "solid_solutions_state".to_string(),
            serde_json::json!("raw/solid_solutions_state.json"),
        );
        artifacts.insert(
            "solid_solutions_metadata".to_string(),
            serde_json::json!("raw/solid_solutions_metadata.json"),
        );
        if self.statistics_enabled {
            artifacts.insert(
                "ss_statistics".to_string(),
                serde_json::json!("raw/ssStatistics.csv"),
            );
            artifacts.insert(
                "ss_hashkey_statistics".to_string(),
                serde_json::json!("raw/ssHashkeyStatistics.csv"),
            );
        }
        if rdf_report.is_some() {
            artifacts.insert(
                "solid_solutions_rdf".to_string(),
                serde_json::json!("raw/solid_solutions_rdf.json"),
            );
            artifacts.insert(
                "d_rdf_plot".to_string(),
                serde_json::json!("raw/D-RDF.plot"),
            );
            artifacts.insert(
                "t_rdf_plot".to_string(),
                serde_json::json!("raw/T-RDF.plot"),
            );
        }
        fs::create_dir_all(self.run_dir.join("raw"))?;
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("solid_solutions"),
                "mode": "solid_solutions",
                "system": self.system,
                "engine": self.backend.map(|backend| format!("Rust SolidSolutions ({})", backend.engine_label())).unwrap_or_else(|| "Rust SolidSolutions (skip-evaluation)".to_string()),
                "initial_candidate_json": self.initial_candidate_json,
                "proposal_candidate_json": self.proposal_candidate_json,
                "backend": self.backend,
                "backend_metadata": self.backend_metadata,
                "artifacts": artifacts,
                "parameters": self.parameters,
            }))?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;
        fs::write(
            self.run_dir
                .join("raw")
                .join("solid_solutions_summary.json"),
            serde_json::to_string_pretty(&execution.summary)
                .context("failed to serialize solid-solutions summary")?,
        )?;
        fs::write(
            self.run_dir
                .join("raw")
                .join("solid_solutions_initial.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "source": &execution.initial_source,
                "evaluation": &execution.initial_evaluation,
            }))
            .context("failed to serialize solid-solutions initial state")?,
        )?;
        fs::write(
            self.run_dir.join("raw").join("solid_solutions_state.json"),
            serde_json::to_string_pretty(&execution.state)
                .context("failed to serialize solid-solutions state")?,
        )?;
        fs::write(
            self.run_dir.join("raw").join("solid_solutions_metadata.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "mode": "solid_solutions",
                "system": self.system,
                "engine": self.backend.map(|backend| backend.engine_label()).unwrap_or("skip-evaluation"),
                "hashkey": self.hashkey_metadata,
                "statistics": {
                    "enabled": self.statistics_enabled,
                    "backup_interval": self.statistics_backup_interval,
                },
                "rdf": {
                    "enabled": self.rdf_config.is_some(),
                    "config": &self.rdf_config,
                    "report": rdf_report.as_ref().map(|_| "raw/solid_solutions_rdf.json"),
                },
                "summary": &execution.summary,
            }))
            .context("failed to serialize solid-solutions metadata")?,
        )?;
        if let (Some(rows), Some(hashkey_rows)) = (&statistics_rows, &hashkey_statistics_rows) {
            write_solid_solution_statistics_csv(
                &self.run_dir.join("raw").join("ssStatistics.csv"),
                rows,
            )?;
            write_solid_solution_hashkey_statistics_csv(
                &self.run_dir.join("raw").join("ssHashkeyStatistics.csv"),
                hashkey_rows,
            )?;
            write_solid_solution_statistics_backups(
                &self.run_dir.join("raw"),
                rows,
                self.statistics_backup_interval,
            )?;
        }
        if let Some(report) = &rdf_report {
            fs::write(
                self.run_dir.join("raw").join("solid_solutions_rdf.json"),
                serde_json::to_string_pretty(report)
                    .context("failed to serialize solid-solutions RDF report")?,
            )?;
            write_solid_solutions_trdf_plot(
                &self.run_dir.join("raw").join("D-RDF.plot"),
                &report.d_rdf,
                report.temperature,
                &report.pair_labels,
            )?;
            write_solid_solutions_trdf_plot(
                &self.run_dir.join("raw").join("T-RDF.plot"),
                &report.t_rdf,
                report.temperature,
                &report.pair_labels,
            )?;
            for pair_label in &report.pair_labels {
                write_solid_solutions_pair_trdf_plot(
                    &self
                        .run_dir
                        .join("raw")
                        .join(format!("D-RDF-{}.plot", sanitize_plot_label(pair_label))),
                    &report.d_rdf,
                    pair_label,
                )?;
                write_solid_solutions_pair_trdf_plot(
                    &self
                        .run_dir
                        .join("raw")
                        .join(format!("T-RDF-{}.plot", sanitize_plot_label(pair_label))),
                    &report.t_rdf,
                    pair_label,
                )?;
            }
        }
        Ok(())
    }
}

fn write_solid_solution_statistics_csv(
    path: &Path,
    rows: &[super::solid_solutions::SolidSolutionStatisticsRow],
) -> Result<()> {
    let include_hashkey = rows.iter().any(|row| row.hashkey.is_some());
    let mut output = String::new();
    if include_hashkey {
        writeln!(&mut output, "ClusterNo,ClusterID,Energy,Hashkey")?;
    } else {
        writeln!(&mut output, "ClusterNo,ClusterID,Energy")?;
    }
    for row in rows {
        if include_hashkey {
            writeln!(
                &mut output,
                "{},{},{},{}",
                row.cluster_no,
                csv_field(&row.cluster_id),
                row.energy,
                csv_field(row.hashkey.as_deref().unwrap_or(""))
            )?;
        } else {
            writeln!(
                &mut output,
                "{},{},{}",
                row.cluster_no,
                csv_field(&row.cluster_id),
                row.energy
            )?;
        }
    }
    fs::write(path, output).with_context(|| {
        format!(
            "failed to write solid-solution statistics `{}`",
            path.display()
        )
    })
}

fn write_solid_solution_hashkey_statistics_csv(
    path: &Path,
    rows: &[super::solid_solutions::SolidSolutionHashkeyStatisticsRow],
) -> Result<()> {
    let mut output = String::new();
    writeln!(&mut output, "TYPE,Hashkey,Occurance")?;
    for row in rows {
        writeln!(
            &mut output,
            "{},{},{}",
            csv_field(&row.source_type),
            csv_field(&row.hashkey),
            row.occurrences
        )?;
    }
    fs::write(path, output).with_context(|| {
        format!(
            "failed to write solid-solution hashkey statistics `{}`",
            path.display()
        )
    })
}

fn write_solid_solution_statistics_backups(
    raw_dir: &Path,
    rows: &[super::solid_solutions::SolidSolutionStatisticsRow],
    interval: usize,
) -> Result<()> {
    if interval == 0 {
        return Ok(());
    }
    for step in (interval..=rows.len()).step_by(interval) {
        write_solid_solution_statistics_csv(
            &raw_dir.join(format!("ssStatistics{step}.csv")),
            &rows[..step],
        )?;
    }
    Ok(())
}

fn write_solid_solutions_trdf_plot(
    path: &Path,
    dataset: &super::solid_solutions::SolidSolutionsTrdfDataset,
    temperature: f64,
    pair_labels: &[String],
) -> Result<()> {
    let mut output = String::new();
    writeln!(&mut output, "# {} structures", dataset.structure_count)?;
    writeln!(&mut output, "# {temperature} K")?;
    writeln!(&mut output, "# {} eV", dataset.reference_energy)?;
    writeln!(&mut output, "# {}", dataset.norm)?;
    for pair_label in pair_labels {
        writeln!(&mut output, "# {}", pair_label.replace('-', " "))?;
    }
    writeln!(&mut output)?;
    for bin in &dataset.bins {
        let value = normalized_plot_value(bin.total, dataset.norm);
        if dataset.sigma >= 0.001 || value > 0.00001 {
            writeln!(&mut output, "{} {}", bin.radius, value)?;
        }
    }
    fs::write(path, output)
        .with_context(|| format!("failed to write TRDF plot `{}`", path.display()))
}

fn write_solid_solutions_pair_trdf_plot(
    path: &Path,
    dataset: &super::solid_solutions::SolidSolutionsTrdfDataset,
    pair_label: &str,
) -> Result<()> {
    let mut output = String::new();
    writeln!(&mut output, "# {} {}", dataset.bins.len(), dataset.norm)?;
    for bin in &dataset.bins {
        let raw_value = bin.pair_values.get(pair_label).copied().unwrap_or(0.0);
        let value = normalized_plot_value(raw_value, dataset.norm);
        if dataset.sigma >= 0.001 || value > 0.00001 {
            writeln!(&mut output, "{} {}", bin.radius, value)?;
        }
    }
    fs::write(path, output)
        .with_context(|| format!("failed to write pair TRDF plot `{}`", path.display()))
}

fn normalized_plot_value(value: f64, norm: f64) -> f64 {
    if norm <= 0.0 || !norm.is_finite() {
        0.0
    } else {
        value / norm
    }
}

fn sanitize_plot_label(label: &str) -> String {
    let sanitized = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "pair".to_string()
    } else {
        sanitized
    }
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub(crate) fn run_basin_hopping(
    request: BasinHoppingRunRequest,
) -> Result<super::basin_hopping::BasinHoppingRunSummary> {
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let candidate = read_candidate_json(&request.candidate_json)?;
    ensure_native_scott_search_candidate(&candidate, "run-basin-hopping")?;
    let (backend, backend_metadata) = build_backend(&request.backend)?;
    let evaluation_port = LocalSamplingEvaluationPort {
        search_backend: backend.as_ref(),
        relax_backend: backend.as_ref(),
    };
    let artifact_sink = LocalBasinHoppingArtifactSink {
        run_dir: request.run_dir.clone(),
        workdir: request.workdir.clone(),
        system: request.system.clone(),
        candidate_json: request.candidate_json.clone(),
        backend: request.backend.backend,
        backend_metadata,
        engine: format!(
            "Rust BasinHopping ({})",
            request.backend.backend.engine_label()
        ),
        parameters: serde_json::json!({
            "scientific_config": request.scientific_config.clone(),
            "enforce_container": request.enforce_container,
            "boundary": request.boundary,
            "seed": request.seed
        }),
    };
    let layout_port = LocalBasinHoppingLayoutPort {
        workdir: request.workdir.clone(),
    };
    let identity_port = NullBasinHoppingIdentityPort;
    Ok(super::basin_hopping::BasinHoppingWorkflowService
        .execute(
            &super::basin_hopping::BasinHoppingWorkflowRequest {
                base_candidate: candidate,
                scientific_config: request.scientific_config,
                enforce_container: request.enforce_container,
                boundary: request.boundary,
                seed: request.seed,
            },
            &super::basin_hopping::BasinHoppingSummaryContext {
                system: request.system,
                backend_label: request.backend.backend.engine_label().to_string(),
            },
            &evaluation_port,
            &layout_port,
            &identity_port,
            &artifact_sink,
        )?
        .summary)
}

pub(crate) fn run_energy_lid(
    request: EnergyLidRunRequest,
) -> Result<super::energy_lid::EnergyLidRunSummary> {
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let (backend, backend_metadata) = build_backend(&request.backend)?;
    let lid_backend_override = maybe_build_energy_lid_search_backend(&request.backend)?;
    let lid_backend: &dyn BackendEvaluator = lid_backend_override
        .as_deref()
        .unwrap_or_else(|| backend.as_ref());
    let start_port = LocalSamplingStartPort;
    let evaluation_port = LocalSamplingEvaluationPort {
        search_backend: lid_backend,
        relax_backend: backend.as_ref(),
    };
    let artifact_sink = LocalSamplingArtifactSink {
        run_dir: request.run_dir.clone(),
        workdir: request.workdir.clone(),
        system: request.system.clone(),
        source_run_dir: request.source_run_dir.clone(),
        backend: request.backend.backend,
        backend_metadata,
        engine: format!(
            "Rust EnergyLid ({})",
            request.backend.backend.engine_label()
        ),
        kind: LocalSamplingArtifactKind::EnergyLid {
            parameters: serde_json::json!({
                "top_n": request.top_n,
                "lid_levels": request.lid_levels,
                "lid_increment": request.lid_increment,
                "steps_per_lid": request.steps_per_lid,
                "step_size": request.step_size,
                "enforce_container": request.enforce_container,
                "boundary": request.boundary,
                "quench_steps": request.quench_steps,
                "runners_per_lid": request.runners_per_lid,
                "seed": request.seed
            }),
        },
    };
    Ok(super::energy_lid::SamplingWorkflowService
        .execute_energy_lid(
            &super::energy_lid::EnergyLidWorkflowRequest {
                source_run_dir: request.source_run_dir,
                workdir: request.workdir,
                system: request.system,
                backend_kind: request.backend.backend.into(),
                top_n: request.top_n,
                lid_levels: request.lid_levels,
                lid_increment: request.lid_increment,
                steps_per_lid: request.steps_per_lid,
                quench_steps: request.quench_steps,
                runners_per_lid: request.runners_per_lid,
                step_size: request.step_size,
                enforce_container: request.enforce_container,
                boundary: request.boundary,
                seed: request.seed,
            },
            &start_port,
            &evaluation_port,
            &artifact_sink,
        )?
        .summary)
}

pub(crate) fn run_simulated_annealing(
    request: SimulatedAnnealingRunRequest,
) -> Result<super::energy_lid::SimulatedAnnealingRunSummary> {
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let (backend, backend_metadata) = build_backend(&request.backend)?;
    let start_port = LocalSamplingStartPort;
    let evaluation_port = LocalSamplingEvaluationPort {
        search_backend: backend.as_ref(),
        relax_backend: backend.as_ref(),
    };
    let artifact_sink = LocalSamplingArtifactSink {
        run_dir: request.run_dir.clone(),
        workdir: request.workdir.clone(),
        system: request.system.clone(),
        source_run_dir: request.source_run_dir.clone(),
        backend: request.backend.backend,
        backend_metadata,
        engine: format!(
            "Rust SimulatedAnnealing ({})",
            request.backend.backend.engine_label()
        ),
        kind: LocalSamplingArtifactKind::SimulatedAnnealing {
            parameters: serde_json::json!({
                "top_n": request.top_n,
                "anneal_steps": request.anneal_steps,
                "initial_temperature": request.initial_temperature,
                "temperature_scale": request.temperature_scale,
                "hold_steps": request.hold_steps,
                "quench_steps": request.quench_steps,
                "step_size": request.step_size,
                "enforce_container": request.enforce_container,
                "boundary": request.boundary,
                "seed": request.seed
            }),
        },
    };
    Ok(super::energy_lid::SamplingWorkflowService
        .execute_simulated_annealing(
            &super::energy_lid::SimulatedAnnealingWorkflowRequest {
                source_run_dir: request.source_run_dir,
                workdir: request.workdir,
                system: request.system,
                backend_kind: request.backend.backend.into(),
                top_n: request.top_n,
                anneal_steps: request.anneal_steps,
                initial_temperature: request.initial_temperature,
                temperature_scale: request.temperature_scale,
                hold_steps: request.hold_steps,
                quench_steps: request.quench_steps,
                step_size: request.step_size,
                enforce_container: request.enforce_container,
                boundary: request.boundary,
                seed: request.seed,
            },
            &start_port,
            &evaluation_port,
            &artifact_sink,
        )?
        .summary)
}

pub(crate) fn run_scan_surface(
    request: ScanSurfaceRunRequest,
) -> Result<super::scan_surface::ScanSurfaceRunSummary> {
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let initial_candidate = read_candidate_json(&request.initial_candidate_json)?;
    let restart_candidates = request
        .restart_candidate_json
        .iter()
        .map(|path| read_candidate_json(path))
        .collect::<Result<Vec<_>>>()?;
    let (backend, backend_metadata) = build_backend(&request.backend)?;
    let evaluation_port = LocalSamplingEvaluationPort {
        search_backend: backend.as_ref(),
        relax_backend: backend.as_ref(),
    };
    let move_port = LocalScanSurfaceMovePort {
        config: super::scan_surface::ScanSurfaceMoveConfig {
            center: request.center,
            boundary: request.boundary,
            above_surface: request.above_surface,
        },
        rng: RefCell::new(LocalWorkflowRng::new(request.seed)),
    };
    let artifact_sink = LocalScanSurfaceArtifactSink {
        run_dir: request.run_dir.clone(),
        system: request.system.clone(),
        initial_candidate_json: request.initial_candidate_json.clone(),
        restart_candidate_json: request.restart_candidate_json.clone(),
        backend: request.backend.backend,
        backend_metadata,
        parameters: serde_json::json!({
            "steps_per_scan": request.steps_per_scan,
            "temperature": request.temperature,
            "base_step_size": request.base_step_size,
            "dynamic_threshold": request.dynamic_threshold,
            "move_mode": request.move_mode,
            "acceptance_mode": request.acceptance_mode,
            "evaluate_initial_state": request.evaluate_initial_state,
            "center": request.center,
            "boundary": request.boundary,
            "above_surface": request.above_surface,
            "seed": request.seed
        }),
    };

    Ok(super::scan_surface::ScanSurfaceWorkflowService
        .execute(
            &super::scan_surface::ScanSurfaceWorkflowRequest {
                initial_candidate,
                restart_candidates,
                workdir: request.workdir,
                steps_per_scan: request.steps_per_scan,
                temperature: request.temperature,
                base_step_size: request.base_step_size,
                dynamic_threshold: request.dynamic_threshold,
                move_mode: request.move_mode,
                acceptance_mode: request.acceptance_mode,
                evaluate_initial_state: request.evaluate_initial_state,
                seed: request.seed,
            },
            &move_port,
            &evaluation_port,
            &artifact_sink,
        )?
        .summary)
}

pub(crate) fn run_solid_solutions(
    request: SolidSolutionsRunRequest,
) -> Result<super::solid_solutions::SolidSolutionsRunSummary> {
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let initial_candidate = read_candidate_json(&request.initial_candidate_json)?;
    let proposals = request
        .proposal_candidate_json
        .iter()
        .map(|path| read_candidate_json(path))
        .collect::<Result<Vec<_>>>()?;
    if !request.use_dreadnaut_keys
        && (!request.imported_hashkeys.is_empty() || !request.imported_hashkey_files.is_empty())
    {
        return Err(anyhow!(
            "solid-solution imported hashkeys require --use-dreadnaut-keys so generated candidates can be compared"
        ));
    }
    let imported_hashkeys = load_solid_solution_imported_hashkeys(
        request.imported_hashkeys.clone(),
        &request.imported_hashkey_files,
    )?;
    let atom_specs = request
        .atoms_in_template
        .as_ref()
        .map(|path| {
            absolutize_path(path)
                .and_then(|path| super::scott_topology_export::parse_atoms_file(&path))
        })
        .transpose()?;
    let hashkey_metadata = serde_json::json!({
        "mode": if request.use_dreadnaut_keys { "patina_dreadnaut" } else { "disabled" },
        "use_dreadnaut_keys": request.use_dreadnaut_keys,
        "atoms_in_template": &request.atoms_in_template,
        "radius_mode": &request.hashkey_radius,
        "radius_const": request.hashkey_radius_const,
        "solid_solution_radius_offset": request.hashkey_radius_const,
        "native_parity_note": "SolidSolution.f90 adds HASHKEY_RADIUS_CONST on top of getHashkeyRadius(); Rust now canonicalizes through patina-dreadnaut directly",
    });
    let library_port = LocalSolidSolutionsLibraryPort { imported_hashkeys };
    let identity_port = LocalSolidSolutionsIdentityPort {
        atom_specs,
        use_dreadnaut_keys: request.use_dreadnaut_keys,
        hashkey_radius: request.hashkey_radius.clone(),
        hashkey_radius_const: request.hashkey_radius_const,
        scratch_dir: request.workdir.join("hashkey_runtime"),
    };
    let geometry_port = LocalSolidSolutionsGeometryPort {
        minimum_distance: request.geometry_min_distance,
    };
    let move_port = LocalSolidSolutionsMovePort {
        rng: RefCell::new(LocalWorkflowRng::new(request.seed)),
    };
    let artifact_sink = LocalSolidSolutionsArtifactSink {
        run_dir: request.run_dir.clone(),
        system: request.system.clone(),
        initial_candidate_json: request.initial_candidate_json.clone(),
        proposal_candidate_json: request.proposal_candidate_json.clone(),
        backend: (!request.skip_evaluation).then_some(request.backend.backend),
        backend_metadata: None,
        parameters: serde_json::json!({
            "steps": request.steps,
            "temperature": request.temperature,
            "move_mode": request.move_mode,
            "acceptance_mode": request.acceptance_mode,
            "max_exchanges": request.max_exchanges,
            "skip_evaluation": request.skip_evaluation,
            "imported_hashkey_count": request.imported_hashkeys.len(),
            "imported_hashkey_file_count": request.imported_hashkey_files.len(),
            "atoms_in_template": request.atoms_in_template,
            "use_dreadnaut_keys": request.use_dreadnaut_keys,
            "hashkey_radius": request.hashkey_radius,
            "hashkey_radius_const": request.hashkey_radius_const,
            "geometry_min_distance": request.geometry_min_distance,
            "statistics_enabled": request.statistics_enabled,
            "statistics_backup_interval": request.statistics_backup_interval,
            "rdf_config": &request.rdf_config,
            "seed": request.seed
        }),
        hashkey_metadata,
        temperature: request.temperature,
        statistics_enabled: request.statistics_enabled,
        statistics_backup_interval: request.statistics_backup_interval,
        rdf_config: request.rdf_config.clone(),
    };
    let workflow_request = super::solid_solutions::SolidSolutionsWorkflowRequest {
        initial_candidate,
        proposals,
        workdir: request.workdir,
        steps: request.steps,
        temperature: request.temperature,
        move_mode: request.move_mode,
        acceptance_mode: request.acceptance_mode,
        max_exchanges: request.max_exchanges,
        seed: request.seed,
        skip_evaluation: request.skip_evaluation,
        imported_hashkeys: Vec::new(),
    };

    if request.skip_evaluation {
        return Ok(super::solid_solutions::SolidSolutionsWorkflowService
            .execute(
                &workflow_request,
                super::solid_solutions::SolidSolutionsWorkflowPorts {
                    library_port: &library_port,
                    identity_port: &identity_port,
                    geometry_port: &geometry_port,
                    move_port: &move_port,
                    evaluation_port: &NoopSamplingEvaluationPort,
                    artifact_sink: &artifact_sink,
                },
            )?
            .summary);
    }

    let (backend, backend_metadata) = build_backend(&request.backend)?;
    let artifact_sink = LocalSolidSolutionsArtifactSink {
        backend_metadata,
        ..artifact_sink
    };
    let evaluation_port = LocalSamplingEvaluationPort {
        search_backend: backend.as_ref(),
        relax_backend: backend.as_ref(),
    };
    Ok(super::solid_solutions::SolidSolutionsWorkflowService
        .execute(
            &workflow_request,
            super::solid_solutions::SolidSolutionsWorkflowPorts {
                library_port: &library_port,
                identity_port: &identity_port,
                geometry_port: &geometry_port,
                move_port: &move_port,
                evaluation_port: &evaluation_port,
                artifact_sink: &artifact_sink,
            },
        )?
        .summary)
}

fn build_backend(
    config: &LocalSamplingBackendConfig,
) -> Result<(Box<dyn BackendEvaluator>, Option<serde_json::Value>)> {
    super::backend_runs::build_backend(
        config.backend.into(),
        config.timeout,
        config.executable.clone(),
        config.master_gin_template.clone(),
        config.run_job_template.clone(),
        config.atoms_in_template.clone(),
        config.jobs_template.clone(),
        config.python_bin.clone(),
        config.janus_adapter_script.clone(),
        config.janus_arch.clone(),
        config.janus_model.clone(),
        config.janus_device.clone(),
        config.janus_dtype.clone(),
        config.janus_mode.into(),
        config.janus_optimizer.into(),
        config.janus_fmax,
        config.janus_steps,
    )
}

fn load_solid_solution_imported_hashkeys(
    explicit_hashkeys: Vec<String>,
    files: &[PathBuf],
) -> Result<Vec<String>> {
    let mut hashkeys = explicit_hashkeys
        .into_iter()
        .filter_map(|hashkey| {
            let trimmed = hashkey.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect::<Vec<_>>();
    for file in files {
        let raw = fs::read_to_string(file)
            .with_context(|| format!("failed to read hashkey file `{}`", file.display()))?;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let fields = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
            let candidate = match fields.as_slice() {
                ["TYPE", "Hashkey", ..] | ["type", "hashkey", ..] => None,
                [_, hashkey, ..] if !hashkey.is_empty() => Some(*hashkey),
                [hashkey] if !hashkey.is_empty() => Some(*hashkey),
                _ => None,
            };
            if let Some(hashkey) = candidate {
                hashkeys.push(hashkey.to_string());
            }
        }
    }
    Ok(hashkeys)
}

fn minimum_pair_distance(candidate: &patina_types::Candidate) -> Option<f64> {
    let mut minimum_distance: Option<f64> = None;
    for left in 0..candidate.fractional_coords.len() {
        for right in (left + 1)..candidate.fractional_coords.len() {
            let distance = pair_distance(candidate, left, right);
            minimum_distance = Some(match minimum_distance {
                Some(current_min) => current_min.min(distance),
                None => distance,
            });
        }
    }
    minimum_distance
}

fn pair_distance(candidate: &patina_types::Candidate, left: usize, right: usize) -> f64 {
    match candidate.lattice {
        Some(lattice) => {
            let left_cart =
                patina_search::fractional_to_cartesian(lattice, candidate.fractional_coords[left]);
            let right_cart =
                patina_search::fractional_to_cartesian(lattice, candidate.fractional_coords[right]);
            patina_search::minimum_image_cartesian_distance_sq_with_axes(
                left_cart,
                right_cart,
                Some(lattice),
                candidate.periodic_axes,
            )
            .sqrt()
        }
        None => {
            let left_coords = candidate.fractional_coords[left];
            let right_coords = candidate.fractional_coords[right];
            let dx = left_coords[0] - right_coords[0];
            let dy = left_coords[1] - right_coords[1];
            let dz = left_coords[2] - right_coords[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        }
    }
}

#[derive(Debug, Clone)]
struct LocalWorkflowRng {
    state: u64,
}

impl LocalWorkflowRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    fn next_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

fn maybe_build_energy_lid_search_backend(
    config: &LocalSamplingBackendConfig,
) -> Result<Option<Box<dyn BackendEvaluator>>> {
    match config.backend {
        EvalBackendKind::Gulp => {
            let executable = absolutize_path(&required_arg(
                config.executable.clone(),
                "executable",
                "gulp",
            )?)?;
            let master_gin_template = absolutize_path(&required_arg(
                config.master_gin_template.clone(),
                "master-gin-template",
                "gulp",
            )?)?;
            Ok(Some(Box::new(
                GulpBackend::new(&master_gin_template, &executable, config.timeout)?
                    .with_single_point_energy(),
            )))
        }
        _ => Ok(None),
    }
}
