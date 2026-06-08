use anyhow::{anyhow, Result};
use patina_types::{
    BasinHoppingSpec, EnergyLidSpec, FrameworkGcmcSpec, GenerateSurfaceSpec, JanusPersistentGaSpec,
    JanusRuntimeSpec, PerturbClusterSpec, SamplingBackendSpec, ScottStagedGaSpec,
    SimulatedAnnealingSpec, WorkflowRunSpec,
};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WorkflowRequest {
    ScottStagedGa(ScottStagedGaRequest),
    JanusPersistentGa(JanusPersistentGaRequest),
    BasinHopping(BasinHoppingRequest),
    EnergyLid(EnergyLidRequest),
    SimulatedAnnealing(SimulatedAnnealingRequest),
    PerturbCluster(PerturbClusterRequest),
    GenerateSurface(GenerateSurfaceRequest),
    FrameworkGcmc(FrameworkGcmcRequest),
}

impl WorkflowRequest {
    pub(crate) fn from_spec(spec: WorkflowRunSpec) -> Result<Self> {
        match spec {
            WorkflowRunSpec::ScottStagedGa(spec) => Ok(Self::ScottStagedGa(
                scott_staged_ga_request_from_spec(spec)?,
            )),
            WorkflowRunSpec::JanusPersistentGa(spec) => Ok(Self::JanusPersistentGa(
                janus_persistent_ga_request_from_spec(spec)?,
            )),
            WorkflowRunSpec::BasinHopping(spec) => {
                Ok(Self::BasinHopping(basin_hopping_request_from_spec(spec)?))
            }
            WorkflowRunSpec::EnergyLid(spec) => {
                Ok(Self::EnergyLid(energy_lid_request_from_spec(spec)?))
            }
            WorkflowRunSpec::SimulatedAnnealing(spec) => Ok(Self::SimulatedAnnealing(
                simulated_annealing_request_from_spec(spec)?,
            )),
            WorkflowRunSpec::PerturbCluster(spec) => Ok(Self::PerturbCluster(
                perturb_cluster_request_from_spec(spec),
            )),
            WorkflowRunSpec::GenerateSurface(spec) => Ok(Self::GenerateSurface(
                generate_surface_request_from_spec(spec),
            )),
            WorkflowRunSpec::FrameworkGcmc(spec) => {
                Ok(Self::FrameworkGcmc(framework_gcmc_request_from_spec(spec)?))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JanusRuntimeRequest {
    pub python_bin: Option<PathBuf>,
    pub janus_adapter_script: Option<PathBuf>,
    pub arch: String,
    pub model: String,
    pub device: String,
    pub dtype: String,
    pub mode: String,
    pub fmax: f64,
    pub steps: usize,
    pub optimizer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SamplingBackendRequest {
    pub backend: String,
    pub timeout_secs: Option<u64>,
    pub executable: Option<PathBuf>,
    pub master_gin_template: Option<PathBuf>,
    pub run_job_template: Option<PathBuf>,
    pub atoms_in_template: Option<PathBuf>,
    pub jobs_template: Option<PathBuf>,
    pub janus: JanusRuntimeRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScottStagedGaRequest {
    pub stage_dir: Option<PathBuf>,
    pub base_candidate_json: Option<PathBuf>,
    pub base_candidate_path: Option<PathBuf>,
    pub resume_from_checkpoint: Option<PathBuf>,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub ga_generations: usize,
    pub population: usize,
    pub keep_dirs: bool,
    pub timeout_secs: Option<u64>,
    pub seed: Option<u64>,
    pub temperature: f64,
    pub step_size: f64,
    pub scott_input_dir: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub master_gin_template: Option<PathBuf>,
    pub run_job_template: Option<PathBuf>,
    pub atoms_in_template: Option<PathBuf>,
    pub janus: JanusRuntimeRequest,
    pub runtime_default_backend: String,
    pub runtime_stage_backends: BTreeMap<u8, String>,
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub pmoi_tolerance: f64,
    pub enable_pmoi: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JanusPersistentGaRequest {
    pub stage_dir: Option<PathBuf>,
    pub base_candidate_json: Option<PathBuf>,
    pub base_candidate_path: Option<PathBuf>,
    pub resume_from_checkpoint: Option<PathBuf>,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub ga_generations: usize,
    pub population: usize,
    pub workers: Option<usize>,
    pub keep_dirs: bool,
    pub timeout_secs: Option<u64>,
    pub seed: Option<u64>,
    pub temperature: f64,
    pub step_size: f64,
    pub duplicate_policy_mode: String,
    pub janus: JanusRuntimeRequest,
    pub atoms_in_template: Option<PathBuf>,
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub pmoi_tolerance: f64,
    pub enable_pmoi: bool,
    pub operator_policy_backend: Option<String>,
    pub pop_replacement_ratio: Option<f64>,
    pub reinsert_elites_ratio: Option<f64>,
    pub mutation_ratio: Option<f64>,
    pub mut_selfcross_ratio: Option<f64>,
    pub max_repop_attempts: Option<usize>,
    pub crossover_attempts: Option<usize>,
    pub tournament_size_min: Option<usize>,
    pub tournament_size_max: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BasinHoppingRequest {
    pub candidate_json: PathBuf,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub bh_steps: usize,
    pub walkers: usize,
    pub temperature: f64,
    pub step_size: f64,
    pub bh_method: String,
    pub bh_accept: String,
    pub dynamic_threshold: Option<usize>,
    pub moveclass_threshold: Option<usize>,
    pub max_dynamic_step_multiplier: Option<f64>,
    pub n_high_temperature: Option<usize>,
    pub n_low_temperature: Option<usize>,
    pub prob_switch_atoms: Option<f64>,
    pub prob_switch_cations: Option<f64>,
    pub prob_mutate_cluster: Option<f64>,
    pub prob_twist_cluster: Option<f64>,
    pub prob_translate_cluster: Option<f64>,
    pub prob_rotate_cluster: Option<f64>,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
    pub backend: SamplingBackendRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EnergyLidRequest {
    pub source_run_dir: PathBuf,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub top_n: usize,
    pub lid_levels: usize,
    pub lid_increment: f64,
    pub steps_per_lid: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub quench_steps: usize,
    pub runners_per_lid: usize,
    pub seed: u64,
    pub backend: SamplingBackendRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SimulatedAnnealingRequest {
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
    pub backend: SamplingBackendRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GenerateSurfaceRequest {
    pub candidate_json: Option<PathBuf>,
    pub structure_path: Option<PathBuf>,
    pub run_dir: PathBuf,
    pub system: Option<String>,
    pub h: i32,
    pub k: i32,
    pub l: i32,
    pub thickness: f64,
    pub vacuum: f64,
    pub supercell_a: usize,
    pub supercell_b: usize,
    pub cut_strategy: String,
    pub cut_offset_fraction: Option<f64>,
    pub dedup_slab: bool,
    pub reduce_slab_inplane: bool,
    pub dedup_frac_tol: f64,
    pub dedup_wrap_z: bool,
    pub dedup_ignore_element: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PerturbClusterRequest {
    pub candidate_json: PathBuf,
    pub run_dir: PathBuf,
    pub system: Option<String>,
    pub count: usize,
    pub sigma: f64,
    pub max_displacement: Option<f64>,
    pub validate_min_distance: Option<f64>,
    pub duplicate_threshold: f64,
    pub duplicate_screening_mode: String,
    pub include_p_orbitals: bool,
    pub environment_width_cutoff: f64,
    pub environment_max_atoms_in_sphere: usize,
    pub environment_s_orbital_count: usize,
    pub environment_p_orbital_count: usize,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrameworkGcmcRequest {
    pub candidate_json: Option<PathBuf>,
    pub structure_path: Option<PathBuf>,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: Option<String>,
    pub guest: String,
    pub temperature_kelvin: f64,
    pub pressure_bar: f64,
    pub initialization_cycles: usize,
    pub production_cycles: usize,
    pub max_guest_count: usize,
    pub minimum_guest_host_distance: f64,
    pub minimum_guest_guest_distance: f64,
    pub density_nx: usize,
    pub density_ny: usize,
    pub density_nz: usize,
    pub density_binning: String,
    pub density_normalization: String,
    pub energy_histogram_bins: usize,
    pub energy_histogram_min_ev: f64,
    pub energy_histogram_max_ev: f64,
    pub number_histogram_lower: usize,
    pub number_histogram_upper: usize,
    pub seed: u64,
    pub janus: JanusRuntimeRequest,
}

fn janus_request_from_spec(spec: JanusRuntimeSpec) -> JanusRuntimeRequest {
    JanusRuntimeRequest {
        python_bin: spec.python_bin.map(|path| path.into_std_path_buf()),
        janus_adapter_script: spec
            .janus_adapter_script
            .map(|path| path.into_std_path_buf()),
        arch: spec.arch,
        model: spec.model,
        device: spec.device,
        dtype: spec.dtype,
        mode: spec.mode,
        fmax: spec.fmax,
        steps: spec.steps,
        optimizer: spec.optimizer,
    }
}

fn sampling_backend_request_from_spec(
    spec: SamplingBackendSpec,
    janus: JanusRuntimeRequest,
) -> SamplingBackendRequest {
    SamplingBackendRequest {
        backend: spec.backend,
        timeout_secs: spec.timeout_secs,
        executable: spec.executable.map(|path| path.into_std_path_buf()),
        master_gin_template: spec
            .master_gin_template
            .map(|path| path.into_std_path_buf()),
        run_job_template: spec.run_job_template.map(|path| path.into_std_path_buf()),
        atoms_in_template: spec.atoms_in_template.map(|path| path.into_std_path_buf()),
        jobs_template: spec.jobs_template.map(|path| path.into_std_path_buf()),
        janus,
    }
}

fn scott_staged_ga_request_from_spec(spec: ScottStagedGaSpec) -> Result<ScottStagedGaRequest> {
    Ok(ScottStagedGaRequest {
        stage_dir: spec.seed.stage_dir.map(|path| path.into_std_path_buf()),
        base_candidate_json: spec
            .seed
            .base_candidate_json
            .map(|path| path.into_std_path_buf()),
        base_candidate_path: spec
            .seed
            .base_candidate_path
            .map(|path| path.into_std_path_buf()),
        resume_from_checkpoint: spec
            .seed
            .resume_from_checkpoint
            .map(|path| path.into_std_path_buf()),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        workdir: spec
            .run
            .workdir
            .ok_or_else(|| anyhow!("ga.scott-monolithic requires `run.workdir`"))?
            .into_std_path_buf(),
        system: spec
            .run
            .system
            .unwrap_or_else(|| "unknown-system".to_string()),
        ga_generations: spec.ga.generations,
        population: spec.ga.population,
        keep_dirs: spec.ga.keep_dirs,
        timeout_secs: spec.backend.timeout_secs,
        seed: spec.ga.seed,
        temperature: spec.ga.temperature,
        step_size: spec.ga.step_size,
        scott_input_dir: spec
            .backend
            .scott_input_dir
            .map(|path| path.into_std_path_buf()),
        executable: spec.backend.executable.map(|path| path.into_std_path_buf()),
        master_gin_template: spec
            .backend
            .master_gin_template
            .map(|path| path.into_std_path_buf()),
        run_job_template: spec
            .backend
            .run_job_template
            .map(|path| path.into_std_path_buf()),
        atoms_in_template: spec
            .backend
            .atoms_in_template
            .map(|path| path.into_std_path_buf()),
        janus: janus_request_from_spec(spec.janus),
        runtime_default_backend: spec.routing.default_backend,
        runtime_stage_backends: spec.routing.stage_backends,
        use_dreadnaut_keys: spec.duplicate_policy.use_dreadnaut_keys,
        hashkey_radius: spec.duplicate_policy.hashkey_radius,
        hashkey_radius_const: spec.duplicate_policy.hashkey_radius_const,
        pmoi_tolerance: spec.duplicate_policy.pmoi_tolerance,
        enable_pmoi: spec.duplicate_policy.enable_pmoi,
    })
}

fn janus_persistent_ga_request_from_spec(
    spec: JanusPersistentGaSpec,
) -> Result<JanusPersistentGaRequest> {
    Ok(JanusPersistentGaRequest {
        stage_dir: spec.seed.stage_dir.map(|path| path.into_std_path_buf()),
        base_candidate_json: spec
            .seed
            .base_candidate_json
            .map(|path| path.into_std_path_buf()),
        base_candidate_path: spec
            .seed
            .base_candidate_path
            .map(|path| path.into_std_path_buf()),
        resume_from_checkpoint: spec
            .seed
            .resume_from_checkpoint
            .map(|path| path.into_std_path_buf()),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        workdir: spec
            .run
            .workdir
            .ok_or_else(|| anyhow!("ga.persistent-daemon requires `run.workdir`"))?
            .into_std_path_buf(),
        system: spec
            .run
            .system
            .unwrap_or_else(|| "unknown-system".to_string()),
        ga_generations: spec.ga.generations,
        population: spec.ga.population,
        workers: spec.backend.workers,
        keep_dirs: spec.backend.keep_dirs,
        timeout_secs: spec.backend.timeout_secs,
        seed: spec.ga.seed,
        temperature: spec.ga.temperature,
        step_size: spec.ga.step_size,
        duplicate_policy_mode: spec.duplicate_policy.mode,
        janus: janus_request_from_spec(spec.janus),
        atoms_in_template: spec
            .duplicate_policy
            .atoms_in_template
            .map(|path| path.into_std_path_buf()),
        use_dreadnaut_keys: spec.duplicate_policy.use_dreadnaut_keys,
        hashkey_radius: spec.duplicate_policy.hashkey_radius,
        hashkey_radius_const: spec.duplicate_policy.hashkey_radius_const,
        pmoi_tolerance: spec.duplicate_policy.pmoi_tolerance,
        enable_pmoi: spec.duplicate_policy.enable_pmoi,
        operator_policy_backend: spec.operator_policy.backend,
        pop_replacement_ratio: spec.operator_policy.pop_replacement_ratio,
        reinsert_elites_ratio: spec.operator_policy.reinsert_elites_ratio,
        mutation_ratio: spec.operator_policy.mutation_ratio,
        mut_selfcross_ratio: spec.operator_policy.mut_selfcross_ratio,
        max_repop_attempts: spec.operator_policy.max_repop_attempts,
        crossover_attempts: spec.operator_policy.crossover_attempts,
        tournament_size_min: spec.operator_policy.tournament_size_min,
        tournament_size_max: spec.operator_policy.tournament_size_max,
    })
}

fn basin_hopping_request_from_spec(spec: BasinHoppingSpec) -> Result<BasinHoppingRequest> {
    let janus = janus_request_from_spec(spec.janus);
    Ok(BasinHoppingRequest {
        candidate_json: spec.seed.candidate_json.into_std_path_buf(),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        workdir: spec
            .run
            .workdir
            .ok_or_else(|| anyhow!("sampling.basin-hopping requires `run.workdir`"))?
            .into_std_path_buf(),
        system: spec
            .run
            .system
            .unwrap_or_else(|| "unknown-system".to_string()),
        bh_steps: spec.sampling.bh_steps,
        walkers: spec.sampling.walkers,
        temperature: spec.sampling.temperature,
        step_size: spec.sampling.step_size,
        bh_method: spec.sampling.bh_method,
        bh_accept: spec.sampling.bh_accept,
        dynamic_threshold: spec.sampling.dynamic_threshold,
        moveclass_threshold: spec.sampling.moveclass_threshold,
        max_dynamic_step_multiplier: spec.sampling.max_dynamic_step_multiplier,
        n_high_temperature: spec.sampling.n_high_temperature,
        n_low_temperature: spec.sampling.n_low_temperature,
        prob_switch_atoms: spec.sampling.prob_switch_atoms,
        prob_switch_cations: spec.sampling.prob_switch_cations,
        prob_mutate_cluster: spec.sampling.prob_mutate_cluster,
        prob_twist_cluster: spec.sampling.prob_twist_cluster,
        prob_translate_cluster: spec.sampling.prob_translate_cluster,
        prob_rotate_cluster: spec.sampling.prob_rotate_cluster,
        enforce_container: spec.sampling.enforce_container,
        boundary: spec.sampling.boundary,
        seed: spec.sampling.seed,
        backend: sampling_backend_request_from_spec(spec.backend, janus),
    })
}

fn energy_lid_request_from_spec(spec: EnergyLidSpec) -> Result<EnergyLidRequest> {
    let janus = janus_request_from_spec(spec.janus);
    Ok(EnergyLidRequest {
        source_run_dir: spec.source.source_run_dir.into_std_path_buf(),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        workdir: spec
            .run
            .workdir
            .ok_or_else(|| anyhow!("sampling.energy-lid requires `run.workdir`"))?
            .into_std_path_buf(),
        system: spec
            .run
            .system
            .unwrap_or_else(|| "unknown-system".to_string()),
        top_n: spec.source.top_n,
        lid_levels: spec.sampling.lid_levels,
        lid_increment: spec.sampling.lid_increment,
        steps_per_lid: spec.sampling.steps_per_lid,
        step_size: spec.sampling.step_size,
        enforce_container: spec.sampling.enforce_container,
        boundary: spec.sampling.boundary,
        quench_steps: spec.sampling.quench_steps,
        runners_per_lid: spec.sampling.runners_per_lid,
        seed: spec.sampling.seed,
        backend: sampling_backend_request_from_spec(spec.backend, janus),
    })
}

fn simulated_annealing_request_from_spec(
    spec: SimulatedAnnealingSpec,
) -> Result<SimulatedAnnealingRequest> {
    let janus = janus_request_from_spec(spec.janus);
    Ok(SimulatedAnnealingRequest {
        source_run_dir: spec.source.source_run_dir.into_std_path_buf(),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        workdir: spec
            .run
            .workdir
            .ok_or_else(|| anyhow!("sampling.simulated-annealing requires `run.workdir`"))?
            .into_std_path_buf(),
        system: spec
            .run
            .system
            .unwrap_or_else(|| "unknown-system".to_string()),
        top_n: spec.source.top_n,
        anneal_steps: spec.sampling.anneal_steps,
        initial_temperature: spec.sampling.initial_temperature,
        temperature_scale: spec.sampling.temperature_scale,
        hold_steps: spec.sampling.hold_steps,
        quench_steps: spec.sampling.quench_steps,
        step_size: spec.sampling.step_size,
        enforce_container: spec.sampling.enforce_container,
        boundary: spec.sampling.boundary,
        seed: spec.sampling.seed,
        backend: sampling_backend_request_from_spec(spec.backend, janus),
    })
}

fn generate_surface_request_from_spec(spec: GenerateSurfaceSpec) -> GenerateSurfaceRequest {
    GenerateSurfaceRequest {
        candidate_json: spec
            .input
            .candidate_json
            .map(|path| path.into_std_path_buf()),
        structure_path: spec
            .input
            .structure_path
            .map(|path| path.into_std_path_buf()),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        system: spec.run.system,
        h: spec.surface.h,
        k: spec.surface.k,
        l: spec.surface.l,
        thickness: spec.surface.thickness,
        vacuum: spec.surface.vacuum,
        supercell_a: spec.surface.supercell_a,
        supercell_b: spec.surface.supercell_b,
        cut_strategy: spec.surface.cut_strategy,
        cut_offset_fraction: spec.surface.cut_offset_fraction,
        dedup_slab: spec.surface.dedup_slab,
        reduce_slab_inplane: spec.surface.reduce_slab_inplane,
        dedup_frac_tol: spec.surface.dedup_frac_tol,
        dedup_wrap_z: spec.surface.dedup_wrap_z,
        dedup_ignore_element: spec.surface.dedup_ignore_element,
    }
}

fn perturb_cluster_request_from_spec(spec: PerturbClusterSpec) -> PerturbClusterRequest {
    PerturbClusterRequest {
        candidate_json: spec.seed.candidate_json.into_std_path_buf(),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        system: spec.run.system,
        count: spec.perturbation.count,
        sigma: spec.perturbation.sigma,
        max_displacement: spec.perturbation.max_displacement,
        validate_min_distance: spec.perturbation.validate_min_distance,
        duplicate_threshold: spec.perturbation.duplicate_threshold,
        duplicate_screening_mode: spec.perturbation.duplicate_screening_mode,
        include_p_orbitals: spec.perturbation.include_p_orbitals,
        environment_width_cutoff: spec.perturbation.environment_width_cutoff,
        environment_max_atoms_in_sphere: spec.perturbation.environment_max_atoms_in_sphere,
        environment_s_orbital_count: spec.perturbation.environment_s_orbital_count,
        environment_p_orbital_count: spec.perturbation.environment_p_orbital_count,
        seed: spec.perturbation.seed,
    }
}

fn framework_gcmc_request_from_spec(spec: FrameworkGcmcSpec) -> Result<FrameworkGcmcRequest> {
    Ok(FrameworkGcmcRequest {
        candidate_json: spec
            .input
            .candidate_json
            .map(|path| path.into_std_path_buf()),
        structure_path: spec
            .input
            .structure_path
            .map(|path| path.into_std_path_buf()),
        run_dir: spec.run.run_dir.into_std_path_buf(),
        workdir: spec
            .run
            .workdir
            .ok_or_else(|| anyhow!("framework.gcmc requires `run.workdir`"))?
            .into_std_path_buf(),
        system: spec.run.system,
        guest: spec.gcmc.guest,
        temperature_kelvin: spec.gcmc.temperature_kelvin,
        pressure_bar: spec.gcmc.pressure_bar,
        initialization_cycles: spec.gcmc.initialization_cycles,
        production_cycles: spec.gcmc.production_cycles,
        max_guest_count: spec.gcmc.max_guest_count,
        minimum_guest_host_distance: spec.gcmc.minimum_guest_host_distance,
        minimum_guest_guest_distance: spec.gcmc.minimum_guest_guest_distance,
        density_nx: spec.gcmc.density_nx,
        density_ny: spec.gcmc.density_ny,
        density_nz: spec.gcmc.density_nz,
        density_binning: spec.gcmc.density_binning,
        density_normalization: spec.gcmc.density_normalization,
        energy_histogram_bins: spec.gcmc.energy_histogram_bins,
        energy_histogram_min_ev: spec.gcmc.energy_histogram_min_ev,
        energy_histogram_max_ev: spec.gcmc.energy_histogram_max_ev,
        number_histogram_lower: spec.gcmc.number_histogram_lower,
        number_histogram_upper: spec.gcmc.number_histogram_upper,
        seed: spec.gcmc.seed,
        janus: janus_request_from_spec(spec.janus),
    })
}

#[cfg(test)]
mod tests {
    use super::{WorkflowRequest, WorkflowRunSpec};
    use patina_types::{
        BasinHoppingSpec, EnergyLidSpec, FrameworkGcmcSpec, GenerateSurfaceSpec,
        JanusPersistentGaSpec, PerturbClusterSpec, ScottStagedGaSpec, SimulatedAnnealingSpec,
    };

    #[test]
    fn energy_lid_request_builds_from_shared_spec() {
        let request =
            WorkflowRequest::from_spec(WorkflowRunSpec::EnergyLid(EnergyLidSpec::default()))
                .expect("request");

        match request {
            WorkflowRequest::EnergyLid(request) => {
                assert_eq!(request.top_n, 8);
                assert_eq!(request.backend.janus.model, "small");
                assert!(request.workdir.ends_with("scratch/energy_lid_example"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn staged_ga_request_requires_workdir() {
        let mut spec = ScottStagedGaSpec::default();
        spec.run.workdir = None;

        let error = WorkflowRequest::from_spec(WorkflowRunSpec::ScottStagedGa(spec))
            .expect_err("missing workdir should fail");

        assert!(error
            .to_string()
            .contains("ga.scott-monolithic requires `run.workdir`"));
    }

    #[test]
    fn staged_ga_request_builds_from_shared_spec() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::ScottStagedGa(
            ScottStagedGaSpec::default(),
        ))
        .expect("request");

        match request {
            WorkflowRequest::ScottStagedGa(request) => {
                assert_eq!(request.runtime_default_backend, "gulp");
                assert_eq!(request.janus.model, "small");
                assert!(request.workdir.ends_with("scratch/monolithic_ga_example"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn janus_persistent_request_builds_from_shared_spec() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::JanusPersistentGa(
            JanusPersistentGaSpec::default(),
        ))
        .expect("request");

        match request {
            WorkflowRequest::JanusPersistentGa(request) => {
                assert_eq!(request.duplicate_policy_mode, "external-native-hashkey");
                assert_eq!(request.janus.model, "small");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn basin_hopping_request_builds_from_shared_spec() {
        let request =
            WorkflowRequest::from_spec(WorkflowRunSpec::BasinHopping(BasinHoppingSpec::default()))
                .expect("request");

        match request {
            WorkflowRequest::BasinHopping(request) => {
                assert_eq!(request.bh_method, "relax");
                assert_eq!(request.backend.janus.model, "small");
                assert!(request.workdir.ends_with("scratch/basin_hopping_example"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn simulated_annealing_request_builds_from_shared_spec() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::SimulatedAnnealing(
            SimulatedAnnealingSpec::default(),
        ))
        .expect("request");

        match request {
            WorkflowRequest::SimulatedAnnealing(request) => {
                assert_eq!(request.top_n, 8);
                assert_eq!(request.backend.janus.model, "small");
                assert!(request
                    .workdir
                    .ends_with("scratch/simulated_annealing_example"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn generate_surface_request_builds_from_shared_spec() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::GenerateSurface(
            GenerateSurfaceSpec::default(),
        ))
        .expect("request");

        match request {
            WorkflowRequest::GenerateSurface(request) => {
                assert_eq!(request.cut_strategy, "topology-aware");
                assert_eq!(request.h, 1);
                assert_eq!(request.k, 0);
                assert_eq!(request.l, 0);
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn perturb_cluster_request_builds_from_shared_spec() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::PerturbCluster(
            PerturbClusterSpec::default(),
        ))
        .expect("request");

        match request {
            WorkflowRequest::PerturbCluster(request) => {
                assert_eq!(request.count, 8);
                assert_eq!(request.duplicate_screening_mode, "global-overlap");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn framework_gcmc_request_builds_from_shared_spec() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::FrameworkGcmc(
            FrameworkGcmcSpec::default(),
        ))
        .expect("request");

        match request {
            WorkflowRequest::FrameworkGcmc(request) => {
                assert_eq!(request.guest, "h2");
                assert!(request.workdir.ends_with("scratch/framework_gcmc_example"));
                assert_eq!(request.janus.mode, "single-point");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }
}
