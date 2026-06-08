use crate::{
    application::workflow_request::{
        BasinHoppingRequest, EnergyLidRequest, FrameworkGcmcRequest, GenerateSurfaceRequest,
        JanusPersistentGaRequest, JanusRuntimeRequest, PerturbClusterRequest,
        SamplingBackendRequest, ScottStagedGaRequest, SimulatedAnnealingRequest, WorkflowRequest,
    },
    application::{
        local_framework_surface_runtime::{
            self, FrameworkGcmcRunRequest, PeriodicStructureInput, SurfaceGenerationRunRequest,
        },
        local_perturbation_runtime::{self, ClusterPerturbationRunRequest},
        local_sampling_runtime::{
            self, BasinHoppingRunRequest, EnergyLidRunRequest, LocalSamplingBackendConfig,
            SimulatedAnnealingRunRequest,
        },
        workflow_ga_runtime,
    },
    DriverJanusMode, DriverJanusOptimizer, DriverSurfaceCutStrategy, EvalBackendKind,
};
use anyhow::{anyhow, Result};
use clap::ValueEnum;
use patina_perturber::DuplicateScreeningMode;
use patina_raspa::{DensityGridBinning, DensityGridNormalization};
use patina_surface::{
    DedupConfig, MillerIndex, SlabReductionConfig, SurfaceGenerationConfig, SurfaceSupercellConfig,
    SurfaceTerminationBias,
};
use patina_types::{BhAcceptanceRuleRecord, BhMethodRecord, WorkflowRunSpec};
use std::path::PathBuf;

pub(crate) enum WorkflowExecutionPlan {
    ScottStagedGa(ScottStagedGaRequest),
    JanusPersistentGa(JanusPersistentGaRequest),
    BasinHopping(BasinHoppingRunRequest),
    EnergyLid(EnergyLidRunRequest),
    SimulatedAnnealing(SimulatedAnnealingRunRequest),
    PerturbCluster(ClusterPerturbationRunRequest),
    GenerateSurface(SurfaceGenerationRunRequest),
    FrameworkGcmc(FrameworkGcmcRunRequest),
}

impl WorkflowExecutionPlan {
    pub(crate) fn from_spec(spec: WorkflowRunSpec) -> Result<Self> {
        Self::from_request(WorkflowRequest::from_spec(spec)?)
    }

    pub(crate) fn from_request(request: WorkflowRequest) -> Result<Self> {
        match request {
            WorkflowRequest::ScottStagedGa(request) => Ok(Self::ScottStagedGa(request)),
            WorkflowRequest::JanusPersistentGa(request) => Ok(Self::JanusPersistentGa(request)),
            WorkflowRequest::BasinHopping(request) => Ok(Self::BasinHopping(
                basin_hopping_run_request_from_request(request)?,
            )),
            WorkflowRequest::EnergyLid(request) => Ok(Self::EnergyLid(
                energy_lid_run_request_from_request(request)?,
            )),
            WorkflowRequest::SimulatedAnnealing(request) => Ok(Self::SimulatedAnnealing(
                simulated_annealing_run_request_from_request(request)?,
            )),
            WorkflowRequest::PerturbCluster(request) => Ok(Self::PerturbCluster(
                perturb_cluster_run_request_from_request(request)?,
            )),
            WorkflowRequest::GenerateSurface(request) => Ok(Self::GenerateSurface(
                generate_surface_run_request_from_request(request)?,
            )),
            WorkflowRequest::FrameworkGcmc(request) => Ok(Self::FrameworkGcmc(
                framework_gcmc_run_request_from_request(request)?,
            )),
        }
    }

    pub(crate) fn execute(self) -> Result<()> {
        match self {
            Self::ScottStagedGa(request) => {
                let summary = workflow_ga_runtime::execute_scott_staged_ga(request)?;
                println!("{}", serde_json::to_string_pretty(&summary)?);
                Ok(())
            }
            Self::JanusPersistentGa(request) => {
                let summary = workflow_ga_runtime::execute_janus_persistent_ga(request)?;
                println!("{}", serde_json::to_string_pretty(&summary)?);
                Ok(())
            }
            Self::BasinHopping(request) => {
                let summary = local_sampling_runtime::run_basin_hopping(request)?;
                println!("{}", serde_json::to_string_pretty(&summary)?);
                Ok(())
            }
            Self::EnergyLid(request) => {
                let summary = local_sampling_runtime::run_energy_lid(request)?;
                println!("{}", serde_json::to_string_pretty(&summary)?);
                Ok(())
            }
            Self::SimulatedAnnealing(request) => {
                let summary = local_sampling_runtime::run_simulated_annealing(request)?;
                println!("{}", serde_json::to_string_pretty(&summary)?);
                Ok(())
            }
            Self::PerturbCluster(request) => {
                let duplicate_screening_mode = match request.duplicate_screening_mode {
                    DuplicateScreeningMode::GlobalOverlap => "global_overlap",
                    DuplicateScreeningMode::EnvironmentAssignment => "environment_assignment",
                };
                let execution = local_perturbation_runtime::run_cluster_perturbation(request)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "source_label": execution.source_cluster.label,
                        "count": execution.count,
                        "source_fingerprint_dimension": execution.source_fingerprint.values.len(),
                        "duplicate_threshold": execution.duplicate_threshold,
                        "duplicate_screening_mode": duplicate_screening_mode,
                        "duplicate_vs_source_count": execution.variant_analyses.iter().filter(|analysis| analysis.duplicate_vs_source.duplicate).count(),
                        "variant_labels": execution.batch.variants.iter().map(|variant| variant.label.clone()).collect::<Vec<_>>()
                    }))?
                );
                Ok(())
            }
            Self::GenerateSurface(request) => {
                let execution = local_framework_surface_runtime::generate_surface(request)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "slab_label": execution.result.slab.label,
                        "atom_count": execution.result.slab.atoms.len(),
                        "periodic_axes": execution.result.slab.periodic_axes,
                        "topology_safe_cut": execution.result.diagnostics.topology_safe_cut,
                        "chosen_cut_offset_angstrom": execution.result.diagnostics.chosen_cut_offset_angstrom,
                        "interplanar_spacing_angstrom": execution.result.diagnostics.interplanar_spacing_angstrom,
                        "layer_count": execution.result.diagnostics.layer_count,
                        "warnings": execution.result.diagnostics.warnings,
                    }))?
                );
                Ok(())
            }
            Self::FrameworkGcmc(request) => {
                let execution = local_framework_surface_runtime::run_framework_gcmc(request)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&execution.result.summary)?
                );
                Ok(())
            }
        }
    }
}

fn parse_sampling_backend_config(
    backend: SamplingBackendRequest,
) -> Result<LocalSamplingBackendConfig> {
    let backend_name = backend.backend;
    let backend_kind = EvalBackendKind::from_str(&backend_name, true)
        .map_err(|_| anyhow!("unsupported sampling backend `{backend_name}`"))?;
    let JanusRuntimeRequest {
        python_bin,
        janus_adapter_script,
        arch,
        model,
        device,
        dtype,
        mode,
        fmax,
        steps,
        optimizer,
    } = backend.janus;
    let janus_mode_name = mode;
    let janus_mode = DriverJanusMode::from_str(&janus_mode_name, true)
        .map_err(|_| anyhow!("unsupported Janus mode `{janus_mode_name}`"))?;
    let janus_optimizer_name = optimizer;
    let janus_optimizer = DriverJanusOptimizer::from_str(&janus_optimizer_name, true)
        .map_err(|_| anyhow!("unsupported Janus optimizer `{janus_optimizer_name}`"))?;

    Ok(LocalSamplingBackendConfig {
        backend: backend_kind.into(),
        timeout: backend.timeout_secs.map(std::time::Duration::from_secs),
        executable: backend.executable,
        master_gin_template: backend.master_gin_template,
        run_job_template: backend.run_job_template,
        atoms_in_template: backend.atoms_in_template,
        jobs_template: backend.jobs_template,
        python_bin,
        janus_adapter_script,
        janus_arch: arch,
        janus_model: model,
        janus_device: device,
        janus_dtype: dtype,
        janus_mode: janus_mode.into(),
        janus_fmax: fmax,
        janus_steps: steps,
        janus_optimizer: janus_optimizer.into(),
    })
}

fn basin_hopping_run_request_from_request(
    request: BasinHoppingRequest,
) -> Result<BasinHoppingRunRequest> {
    let scientific_config = bh_scientific_config_from_request(&request)?;
    let backend = parse_sampling_backend_config(request.backend)?;
    Ok(BasinHoppingRunRequest {
        candidate_json: request.candidate_json,
        run_dir: request.run_dir,
        workdir: request.workdir,
        system: request.system,
        scientific_config,
        enforce_container: request.enforce_container,
        boundary: request.boundary,
        seed: request.seed,
        backend,
    })
}

fn bh_scientific_config_from_request(
    request: &BasinHoppingRequest,
) -> Result<patina_types::BhScientificConfig> {
    let mut config = crate::application::basin_hopping::default_bh_scientific_config(
        request.bh_steps.max(1),
        request.walkers.max(1),
        request.temperature,
        request.step_size,
    );
    config.method = match request.bh_method.as_str() {
        "relax" => BhMethodRecord::Relax,
        "fixed" => BhMethodRecord::Fixed,
        "oscillate" => BhMethodRecord::Oscillate {
            high_temperature_steps: request.n_high_temperature.ok_or_else(|| {
                anyhow!(
                    "sampling.basin-hopping requires `sampling.n_high_temperature` when `sampling.bh_method = \"oscillate\"`"
                )
            })?,
            low_temperature_steps: request.n_low_temperature.ok_or_else(|| {
                anyhow!(
                    "sampling.basin-hopping requires `sampling.n_low_temperature` when `sampling.bh_method = \"oscillate\"`"
                )
            })?,
        },
        other => return Err(anyhow!("unsupported basin-hopping method `{other}`")),
    };
    config.acceptance_rule = match request.bh_accept.as_str() {
        "metropolis" => BhAcceptanceRuleRecord::Metropolis {
            temperature: request.temperature,
        },
        "quench" => BhAcceptanceRuleRecord::Quench,
        other => return Err(anyhow!("unsupported basin-hopping acceptance `{other}`")),
    };
    if let Some(value) = request.dynamic_threshold {
        config.dynamic_threshold = value;
    }
    if let Some(value) = request.moveclass_threshold {
        config.moveclass_threshold = value;
    }
    if let Some(value) = request.max_dynamic_step_multiplier {
        config.max_dynamic_step_multiplier = value;
    }
    if let Some(value) = request.prob_switch_atoms {
        config.prob_switch_atoms = value;
    }
    if let Some(value) = request.prob_switch_cations {
        config.prob_switch_cations = value;
    }
    if let Some(value) = request.prob_mutate_cluster {
        config.prob_mutate_cluster = value;
    }
    if let Some(value) = request.prob_twist_cluster {
        config.prob_twist_cluster = value;
    }
    if let Some(value) = request.prob_translate_cluster {
        config.prob_translate_cluster = value;
    }
    if let Some(value) = request.prob_rotate_cluster {
        config.prob_rotate_cluster = value;
    }
    Ok(config)
}

fn energy_lid_run_request_from_request(request: EnergyLidRequest) -> Result<EnergyLidRunRequest> {
    let backend = parse_sampling_backend_config(request.backend)?;
    Ok(EnergyLidRunRequest {
        source_run_dir: request.source_run_dir,
        run_dir: request.run_dir,
        workdir: request.workdir,
        system: request.system,
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
        backend,
    })
}

fn simulated_annealing_run_request_from_request(
    request: SimulatedAnnealingRequest,
) -> Result<SimulatedAnnealingRunRequest> {
    let backend = parse_sampling_backend_config(request.backend)?;
    Ok(SimulatedAnnealingRunRequest {
        source_run_dir: request.source_run_dir,
        run_dir: request.run_dir,
        workdir: request.workdir,
        system: request.system,
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
        backend,
    })
}

fn periodic_structure_input_from_request(
    candidate_json: Option<PathBuf>,
    structure_path: Option<PathBuf>,
) -> PeriodicStructureInput {
    PeriodicStructureInput {
        candidate_json,
        structure_path,
    }
}

fn generate_surface_run_request_from_request(
    request: GenerateSurfaceRequest,
) -> Result<SurfaceGenerationRunRequest> {
    Ok(SurfaceGenerationRunRequest {
        input: periodic_structure_input_from_request(
            request.candidate_json,
            request.structure_path,
        ),
        run_dir: request.run_dir,
        system: request.system,
        config: SurfaceGenerationConfig {
            miller: MillerIndex::new(request.h, request.k, request.l)?,
            thickness_angstrom: request.thickness,
            vacuum_angstrom: request.vacuum,
            supercell: SurfaceSupercellConfig {
                repeat_a: request.supercell_a,
                repeat_b: request.supercell_b,
            },
            cut_strategy: DriverSurfaceCutStrategy::from_str(&request.cut_strategy, true)
                .map_err(|_| {
                    anyhow!(
                        "unsupported surface cut strategy `{}`",
                        request.cut_strategy
                    )
                })?
                .into(),
            cut_offset_fraction: request.cut_offset_fraction,
            slab_reduction: SlabReductionConfig {
                dedup_slab: request.dedup_slab,
                reduce_slab_inplane: request.reduce_slab_inplane,
                dedup: DedupConfig {
                    frac_tol: request.dedup_frac_tol,
                    inplane_only: !request.dedup_wrap_z,
                    require_same_element: !request.dedup_ignore_element,
                },
            },
            reconstruction: patina_surface::SurfaceReconstructionMode::None,
            termination_bias: SurfaceTerminationBias::Neutral,
        },
    })
}

fn perturb_cluster_run_request_from_request(
    request: PerturbClusterRequest,
) -> Result<ClusterPerturbationRunRequest> {
    let duplicate_screening_mode = match request.duplicate_screening_mode.as_str() {
        "global-overlap" | "global_overlap" => DuplicateScreeningMode::GlobalOverlap,
        "environment-assignment" | "environment_assignment" => {
            DuplicateScreeningMode::EnvironmentAssignment
        }
        other => {
            return Err(anyhow!(
                "unsupported cluster duplicate screening mode `{other}`"
            ))
        }
    };
    Ok(ClusterPerturbationRunRequest {
        candidate_json: request.candidate_json,
        run_dir: request.run_dir,
        system: request.system,
        count: request.count,
        sigma: request.sigma,
        max_displacement: request.max_displacement,
        validate_min_distance: request.validate_min_distance,
        duplicate_threshold: request.duplicate_threshold,
        include_p_orbitals: request.include_p_orbitals,
        duplicate_screening_mode,
        environment_width_cutoff: request.environment_width_cutoff,
        environment_max_atoms_in_sphere: request.environment_max_atoms_in_sphere,
        environment_s_orbital_count: request.environment_s_orbital_count,
        environment_p_orbital_count: request.environment_p_orbital_count,
        seed: request.seed,
    })
}

fn framework_gcmc_run_request_from_request(
    request: FrameworkGcmcRequest,
) -> Result<FrameworkGcmcRunRequest> {
    let density_binning = match request.density_binning.as_str() {
        "standard" => DensityGridBinning::Standard,
        "equitable" => DensityGridBinning::Equitable,
        other => return Err(anyhow!("unsupported density binning `{other}`")),
    };
    let density_normalization = match request.density_normalization.as_str() {
        "max" => DensityGridNormalization::Max,
        "number-density" | "number_density" => DensityGridNormalization::NumberDensity,
        other => return Err(anyhow!("unsupported density normalization `{other}`")),
    };
    let janus_mode = DriverJanusMode::from_str(&request.janus.mode, true)
        .map_err(|_| anyhow!("unsupported Janus mode `{}`", request.janus.mode))?;
    let janus_optimizer = DriverJanusOptimizer::from_str(&request.janus.optimizer, true)
        .map_err(|_| anyhow!("unsupported Janus optimizer `{}`", request.janus.optimizer))?;
    Ok(FrameworkGcmcRunRequest {
        input: periodic_structure_input_from_request(
            request.candidate_json,
            request.structure_path,
        ),
        run_dir: request.run_dir,
        workdir: request.workdir,
        system: request.system,
        guest: request.guest,
        temperature_kelvin: request.temperature_kelvin,
        pressure_bar: request.pressure_bar,
        initialization_cycles: request.initialization_cycles,
        production_cycles: request.production_cycles,
        max_guest_count: request.max_guest_count,
        minimum_guest_host_distance: request.minimum_guest_host_distance,
        minimum_guest_guest_distance: request.minimum_guest_guest_distance,
        density_dimensions: [request.density_nx, request.density_ny, request.density_nz],
        density_binning,
        density_normalization,
        energy_histogram_bins: request.energy_histogram_bins,
        energy_histogram_range: (
            request.energy_histogram_min_ev,
            request.energy_histogram_max_ev,
        ),
        number_histogram_limits: (
            request.number_histogram_lower,
            request.number_histogram_upper,
        ),
        seed: request.seed,
        python_bin: request.janus.python_bin,
        janus_adapter_script: request.janus.janus_adapter_script,
        janus_arch: request.janus.arch,
        janus_model: request.janus.model,
        janus_device: request.janus.device,
        janus_dtype: request.janus.dtype,
        janus_mode: janus_mode.into(),
        janus_fmax: request.janus.fmax,
        janus_steps: request.janus.steps,
        janus_optimizer: janus_optimizer.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::WorkflowExecutionPlan;
    use crate::application::{
        workflow_cli::load_workflow_run_spec, workflow_request::WorkflowRequest,
    };
    use camino::Utf8Path;
    use patina_perturber::DuplicateScreeningMode;
    use patina_raspa::DensityGridBinning;
    use patina_types::{
        BasinHoppingSpec, BhMethodRecord, Candidate, EnergyLidSpec, FrameworkGcmcSpec,
        GenerateSurfaceSpec, JanusPersistentGaSpec, PerturbClusterSpec, ScottStagedGaSpec,
        SimulatedAnnealingSpec, WorkflowRunSpec,
    };
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "patina_workflow_execution_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_cluster_candidate_json(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("cluster_seed.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&Candidate::cluster(
                "demo_cluster",
                vec!["Mg".to_string(), "O".to_string(), "O".to_string()],
                vec![[0.0, 0.0, 0.0], [1.8, 0.0, 0.0], [0.0, 1.6, 0.2]],
            ))
            .expect("candidate json"),
        )
        .expect("write cluster candidate");
        path
    }

    fn write_framework_candidate_json(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("framework_parent.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&Candidate::fully_periodic(
                "demo_framework",
                vec![
                    "Mg".to_string(),
                    "O".to_string(),
                    "Mg".to_string(),
                    "O".to_string(),
                ],
                vec![
                    [0.0, 0.0, 0.0],
                    [0.5, 0.5, 0.5],
                    [0.5, 0.0, 0.5],
                    [0.0, 0.5, 0.5],
                ],
                [[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]],
            ))
            .expect("candidate json"),
        )
        .expect("write framework candidate");
        path
    }

    #[test]
    fn energy_lid_plan_builds_from_shared_spec() {
        let plan =
            WorkflowExecutionPlan::from_spec(WorkflowRunSpec::EnergyLid(EnergyLidSpec::default()))
                .expect("plan");

        match plan {
            WorkflowExecutionPlan::EnergyLid(request) => {
                assert_eq!(request.top_n, 8);
                assert_eq!(request.lid_levels, 8);
                assert_eq!(request.backend.janus_model, "small");
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn energy_lid_plan_builds_from_request() {
        let request =
            WorkflowRequest::from_spec(WorkflowRunSpec::EnergyLid(EnergyLidSpec::default()))
                .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::EnergyLid(request) => {
                assert_eq!(request.top_n, 8);
                assert_eq!(request.lid_levels, 8);
                assert_eq!(request.backend.janus_model, "small");
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn basin_hopping_oscillate_plan_builds_from_shared_spec() {
        let mut spec = BasinHoppingSpec::default();
        spec.sampling.bh_method = "oscillate".to_string();
        spec.sampling.n_high_temperature = Some(5);
        spec.sampling.n_low_temperature = Some(3);
        spec.sampling.dynamic_threshold = Some(7);
        let plan =
            WorkflowExecutionPlan::from_spec(WorkflowRunSpec::BasinHopping(spec)).expect("plan");

        match plan {
            WorkflowExecutionPlan::BasinHopping(request) => {
                assert_eq!(request.scientific_config.dynamic_threshold, 7);
                match request.scientific_config.method {
                    BhMethodRecord::Oscillate {
                        high_temperature_steps,
                        low_temperature_steps,
                    } => {
                        assert_eq!(high_temperature_steps, 5);
                        assert_eq!(low_temperature_steps, 3);
                    }
                    other => panic!("unexpected basin-hopping method: {other:?}"),
                }
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn basin_hopping_plan_builds_from_request() {
        let request =
            WorkflowRequest::from_spec(WorkflowRunSpec::BasinHopping(BasinHoppingSpec::default()))
                .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::BasinHopping(request) => {
                assert_eq!(request.scientific_config.max_steps, 40);
                assert_eq!(request.backend.janus_model, "small");
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn scott_staged_plan_builds_from_request() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::ScottStagedGa(
            ScottStagedGaSpec::default(),
        ))
        .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::ScottStagedGa(request) => {
                assert_eq!(request.runtime_default_backend, "gulp");
                assert_eq!(request.janus.model, "small");
                assert!(request.workdir.ends_with("scratch/monolithic_ga_example"));
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn janus_persistent_plan_builds_from_shared_spec() {
        let plan = WorkflowExecutionPlan::from_spec(WorkflowRunSpec::JanusPersistentGa(
            JanusPersistentGaSpec::default(),
        ))
        .expect("plan");

        match plan {
            WorkflowExecutionPlan::JanusPersistentGa(request) => {
                assert_eq!(request.janus.model, "small");
                assert_eq!(request.duplicate_policy_mode, "external-native-hashkey");
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn janus_persistent_plan_builds_from_request() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::JanusPersistentGa(
            JanusPersistentGaSpec::default(),
        ))
        .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::JanusPersistentGa(request) => {
                assert_eq!(request.janus.model, "small");
                assert_eq!(request.duplicate_policy_mode, "external-native-hashkey");
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn perturb_cluster_plan_builds_from_shared_spec() {
        let plan = WorkflowExecutionPlan::from_spec(WorkflowRunSpec::PerturbCluster(
            PerturbClusterSpec::default(),
        ))
        .expect("plan");

        match plan {
            WorkflowExecutionPlan::PerturbCluster(request) => {
                assert_eq!(request.count, 8);
                assert_eq!(
                    request.duplicate_screening_mode,
                    DuplicateScreeningMode::GlobalOverlap
                );
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn perturb_cluster_plan_builds_from_request() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::PerturbCluster(
            PerturbClusterSpec::default(),
        ))
        .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::PerturbCluster(request) => {
                assert_eq!(request.count, 8);
                assert_eq!(
                    request.duplicate_screening_mode,
                    DuplicateScreeningMode::GlobalOverlap
                );
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn framework_gcmc_plan_builds_from_shared_spec() {
        let plan = WorkflowExecutionPlan::from_spec(WorkflowRunSpec::FrameworkGcmc(
            FrameworkGcmcSpec::default(),
        ))
        .expect("plan");

        match plan {
            WorkflowExecutionPlan::FrameworkGcmc(request) => {
                assert_eq!(request.guest, "h2");
                assert_eq!(request.density_binning, DensityGridBinning::Equitable);
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn framework_gcmc_plan_builds_from_request() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::FrameworkGcmc(
            FrameworkGcmcSpec::default(),
        ))
        .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::FrameworkGcmc(request) => {
                assert_eq!(request.guest, "h2");
                assert_eq!(request.density_binning, DensityGridBinning::Equitable);
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn simulated_annealing_plan_builds_from_request() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::SimulatedAnnealing(
            SimulatedAnnealingSpec::default(),
        ))
        .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::SimulatedAnnealing(request) => {
                assert_eq!(request.top_n, 8);
                assert_eq!(request.backend.janus_model, "small");
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn generate_surface_plan_builds_from_request() {
        let request = WorkflowRequest::from_spec(WorkflowRunSpec::GenerateSurface(
            GenerateSurfaceSpec::default(),
        ))
        .expect("request");
        let plan = WorkflowExecutionPlan::from_request(request).expect("plan");

        match plan {
            WorkflowExecutionPlan::GenerateSurface(request) => {
                assert_eq!(request.config.miller.h, 1);
                assert_eq!(
                    request.config.cut_strategy,
                    patina_surface::SurfaceCutStrategy::TopologyAware
                );
            }
            other => panic!(
                "unexpected workflow plan: {other_variant:?}",
                other_variant = core::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cluster_perturbation_shared_cli_spec_executes_end_to_end() {
        let dir = temp_dir("cluster_spec");
        let run_dir = dir.join("run");
        let candidate_json = write_cluster_candidate_json(&dir);
        let spec_path = dir.join("workflow.toml");
        fs::write(
            &spec_path,
            format!(
                r#"workflow = "structure.perturb-cluster"

[run]
run_dir = "{run_dir}"
system = "demo-cluster"

[seed]
candidate_json = "{candidate_json}"

[perturbation]
count = 2
sigma = 0.12
max_displacement = 0.2
duplicate_threshold = 0.000001
duplicate_screening_mode = "global-overlap"
include_p_orbitals = false
environment_width_cutoff = 1.0
environment_max_atoms_in_sphere = 8
environment_s_orbital_count = 1
environment_p_orbital_count = 0
seed = 7
"#,
                run_dir = run_dir.display(),
                candidate_json = candidate_json.display()
            ),
        )
        .expect("write workflow spec");

        let spec = load_workflow_run_spec(Utf8Path::from_path(&spec_path).expect("utf8 spec path"))
            .expect("load spec");
        WorkflowExecutionPlan::from_spec(spec)
            .expect("plan")
            .execute()
            .expect("execute workflow");

        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(run_dir.join("manifest.json")).expect("read manifest"),
        )
        .expect("manifest json");
        assert_eq!(manifest["workflow_id"], "structure.perturb-cluster");
        assert_eq!(
            manifest["provenance"]["workflow_id"],
            "structure.perturb-cluster"
        );
        assert_eq!(manifest["summary"]["variant_count"], 2);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn surface_generation_shared_cli_spec_executes_end_to_end() {
        let dir = temp_dir("surface_spec");
        let run_dir = dir.join("run");
        let candidate_json = write_framework_candidate_json(&dir);
        let spec_path = dir.join("workflow.toml");
        fs::write(
            &spec_path,
            format!(
                r#"workflow = "framework.generate-surface"

[run]
run_dir = "{run_dir}"
system = "demo-surface"

[input]
candidate_json = "{candidate_json}"

[surface]
h = 1
k = 0
l = 0
thickness = 8.0
vacuum = 10.0
supercell_a = 1
supercell_b = 1
cut_strategy = "topology-aware"
dedup_slab = true
reduce_slab_inplane = true
dedup_frac_tol = 0.0001
dedup_wrap_z = false
dedup_ignore_element = false
"#,
                run_dir = run_dir.display(),
                candidate_json = candidate_json.display()
            ),
        )
        .expect("write workflow spec");

        let spec = load_workflow_run_spec(Utf8Path::from_path(&spec_path).expect("utf8 spec path"))
            .expect("load spec");
        WorkflowExecutionPlan::from_spec(spec)
            .expect("plan")
            .execute()
            .expect("execute workflow");

        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(run_dir.join("manifest.json")).expect("read manifest"),
        )
        .expect("manifest json");
        assert_eq!(manifest["workflow_id"], "framework.generate-surface");
        assert_eq!(
            manifest["provenance"]["workflow_id"],
            "framework.generate-surface"
        );
        assert!(run_dir.join("outputs").join("slab_candidate.json").exists());

        fs::remove_dir_all(dir).ok();
    }
}
