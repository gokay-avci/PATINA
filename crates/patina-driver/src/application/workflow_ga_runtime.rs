use anyhow::{anyhow, Context, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::driver_support::{
    ensure_native_scott_search_candidate, verify_hashkey_dreadnaut_adapter,
};
use super::local_staged_runtime::{self, StagedGaRuntimeRequest, StagedRuntimeBackendConfig};
use super::rust_janus_ga::{
    self, RustJanusGaBackendSetupRequest, RustJanusGaCoreRequest, RustJanusGaOperatorOverrides,
};
use super::rust_janus_ga_artifacts::{
    self, StagedScottGaArtifactMetadata, StagedScottGaArtifactSink,
};
use super::workflow_input::{
    resolve_single_candidate_input, CandidateInputSelection, ResolvedCandidateInput,
    SingleCandidateInputContract,
};
use super::workflow_request::{JanusPersistentGaRequest, ScottStagedGaRequest};

pub(crate) fn execute_janus_persistent_ga(
    request: JanusPersistentGaRequest,
) -> Result<serde_json::Value> {
    let core_request = core_request_from_janus_request(&request)?;
    let backend_request = backend_setup_request_from_janus_request(&request)?;
    let setup = rust_janus_ga::prepare_rust_janus_ga_setup(&core_request, &backend_request)?;
    let started = std::time::Instant::now();
    let worker_session_dir = setup.pool.session_dir().to_path_buf();
    let search_cfg = setup.core.controller_bootstrap.search_cfg.clone();
    let python_bin = setup.python_bin.clone();
    let adapter_script = setup.adapter_script.clone();
    let lane_mode = setup.core.lane_metadata.lane_mode;
    let duplicate_policy_mode = setup.core.evidence_policy.duplicate_policy_mode;
    let duplicate_filter_stack = setup.core.evidence_policy.duplicate_filter_stack.clone();
    let operator_policy = setup.core.lane_metadata.operator_policy.clone();
    let hashkey_config = build_ga_artifact_hashkey_config(
        request.use_dreadnaut_keys,
        &request.hashkey_radius,
        request.hashkey_radius_const,
        request.run_dir.join("raw").join("hashkey_identity_runtime"),
    )?;
    let execution = super::rust_janus_ga_execution::execute_rust_janus_ga_search(
        setup,
        super::rust_janus_ga_execution::RustJanusGaExecutionRequest {
            requested_generations: request.ga_generations,
        },
        rust_janus_ga_artifacts::RustJanusIncrementalArtifactMetadata {
            run_dir: &request.run_dir,
            system: &request.system,
            search_cfg: &search_cfg,
            requested_generations: request.ga_generations,
            backend_mode_label: &request.janus.mode,
            operator_policy: &operator_policy,
            hashkey_config: hashkey_config.as_ref(),
        },
    )?;

    rust_janus_ga_artifacts::write_rust_janus_ga_artifacts(
        &rust_janus_ga_artifacts::RustJanusGaArtifactContext {
            run_dir: &request.run_dir,
            system: &request.system,
            base_candidate_source_path: core_request.base_candidate_source_path.as_deref(),
            search_cfg: &search_cfg,
            workers: request.workers.unwrap_or_else(default_worker_count),
            requested_generations: request.ga_generations,
            population_size: request.population,
            temperature: request.temperature,
            step_size: request.step_size,
            enable_pmoi: request.enable_pmoi,
            pmoi_tolerance: request.pmoi_tolerance,
            lane_mode,
            duplicate_policy_mode,
            duplicate_filter_stack: &duplicate_filter_stack,
            operator_policy: &operator_policy,
            hashkey_config: hashkey_config.as_ref(),
            backend: rust_janus_ga_artifacts::RustJanusGaBackendArtifactMetadata {
                python_bin: &python_bin,
                adapter_script: &adapter_script,
                arch: &request.janus.arch,
                model: &request.janus.model,
                device: &request.janus.device,
                dtype: &request.janus.dtype,
                mode_label: &request.janus.mode,
                optimizer_label: &request.janus.optimizer,
                fmax: request.janus.fmax,
                steps: request.janus.steps,
                worker_session_dir: &worker_session_dir,
            },
        },
        &execution.generation_artifacts,
        &execution.generation_responses,
        &execution.generation_origin_metrics,
        &execution.generation_boundary_populations,
        &execution.controller_trace,
        &execution.final_population,
        started.elapsed().as_secs_f64(),
    )?;

    Ok(serde_json::json!({
        "status": "ok",
        "workflow_owner": "persistent_daemon_ga",
        "run_dir": request.run_dir,
        "workdir": request.workdir,
        "generations_recorded": execution.generation_artifacts.len(),
        "workers": request.workers.unwrap_or_else(default_worker_count)
    }))
}

pub(crate) fn execute_scott_staged_ga(
    request: ScottStagedGaRequest,
) -> Result<rust_janus_ga_artifacts::StagedScottGaSummaryReport> {
    std::fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    std::fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let core_request = core_request_from_staged_request(&request)?;
    let core = rust_janus_ga::prepare_rust_janus_ga_core(&core_request)?;
    let duplicate_trace = core.evidence_policy.duplicate_trace.clone();
    let (executor, routing_policy, procedure_plan, utf8_workdir) =
        local_staged_runtime::build_staged_ga_runtime_setup(&StagedGaRuntimeRequest {
            workdir: request.workdir.clone(),
            keep_dirs: request.keep_dirs,
            backend: staged_backend_config_from_request(&request)?,
        })?;
    let evaluator = super::scott_ga_runtime::StagedScottGaEvaluator::new(
        executor,
        procedure_plan.clone(),
        utf8_workdir.join("ga_stage_runtime"),
    );
    let search_cfg = core.controller_bootstrap.search_cfg.clone();
    let operator_policy = core.lane_metadata.operator_policy.clone();
    let lane_mode = core.lane_metadata.lane_mode;
    let duplicate_policy_mode = core.evidence_policy.duplicate_policy_mode;
    let duplicate_filter_stack = core.evidence_policy.duplicate_filter_stack.clone();
    let workflow_service = super::ga_workflow::GaWorkflowService::new(
        super::workflow_policy::GaWorkflowPolicy::scott_staged_runtime(),
    );
    let hashkey_config = build_ga_artifact_hashkey_config(
        request.use_dreadnaut_keys,
        &request.hashkey_radius,
        request.hashkey_radius_const,
        request.run_dir.join("raw").join("hashkey_identity_runtime"),
    )?;
    let backend_mode_label = match routing_policy.default_backend {
        patina_evaluator::ScottBackendMode::Gulp => "gulp",
        patina_evaluator::ScottBackendMode::JanusMace => "janus_mace",
    };
    let incremental_artifact_sink = rust_janus_ga_artifacts::StagedScottIncrementalArtifactSink {
        run_dir: &request.run_dir,
        system: &request.system,
        search_cfg: &search_cfg,
        requested_generations: request.ga_generations,
        operator_policy: &operator_policy,
        backend_mode_label,
        hashkey_config: hashkey_config.as_ref(),
        duplicate_trace: &duplicate_trace,
        procedure_traces: evaluator.shared_procedure_traces(),
    };
    let execution = workflow_service.execute_with_generation_sink(
        super::ga_workflow::GaWorkflowRequest {
            requested_generations: request.ga_generations,
        },
        core,
        &evaluator,
        Some(&incremental_artifact_sink),
    )?;
    let procedure_traces = evaluator.procedure_traces();
    let duplicate_traces = rust_janus_ga::snapshot_duplicate_trace(&duplicate_trace);

    let runtime_stage_backend = request
        .runtime_stage_backends
        .iter()
        .map(|(stage, backend)| format!("{stage}={backend}"))
        .collect::<Vec<_>>();
    let artifact_metadata = StagedScottGaArtifactMetadata {
        run_dir: request.run_dir.clone(),
        workdir: request.workdir.clone(),
        system: request.system.clone(),
        ga_generations: request.ga_generations,
        population: request.population,
        temperature: request.temperature,
        step_size: request.step_size,
        runtime_default_backend: request.runtime_default_backend.clone(),
        runtime_stage_backend,
        run_job_template: request.run_job_template.clone(),
        master_gin_template: request.master_gin_template.clone(),
        atoms_in_template: request.atoms_in_template.clone(),
    };
    let artifact_sink = StagedScottGaArtifactSink {
        metadata: &artifact_metadata,
        base_candidate_source_path: core_request.base_candidate_source_path.as_deref(),
        search_cfg: &search_cfg,
        operator_policy: &operator_policy,
        hashkey_config: hashkey_config.as_ref(),
        lane_mode,
        duplicate_policy_mode,
        duplicate_filter_stack: &duplicate_filter_stack,
        duplicate_traces: &duplicate_traces,
        routing_policy: &routing_policy,
        procedure_plan: &procedure_plan,
        procedure_traces: &procedure_traces,
        emulate_gate_summary: None,
        emulate_gate_decision_traces: None,
        emulate_gate_batch_traces: None,
        workflow_policy: workflow_service.policy(),
    };
    super::ports::GaRunArtifactSink::persist_run(&artifact_sink, &execution)
}

fn core_request_from_janus_request(
    request: &JanusPersistentGaRequest,
) -> Result<RustJanusGaCoreRequest> {
    let base_candidate_input = resolve_cluster_ga_base_candidate(
        request.stage_dir.as_deref(),
        request.base_candidate_json.as_deref(),
        request.base_candidate_path.as_deref(),
        request.resume_from_checkpoint.as_deref(),
    )?;
    Ok(RustJanusGaCoreRequest {
        base_candidate: base_candidate_input
            .as_ref()
            .map(|resolved| resolved.candidate.clone()),
        base_candidate_source_path: base_candidate_input
            .as_ref()
            .map(|resolved| resolved.source_path.clone()),
        workdir: request.workdir.clone(),
        run_dir: request.run_dir.clone(),
        resume_from_checkpoint: request.resume_from_checkpoint.clone(),
        requested_generations: request.ga_generations,
        population_size: request.population,
        seed: request.seed,
        temperature: request.temperature,
        step_size: request.step_size,
        operator_policy_backend: request.operator_policy_backend.clone(),
        janus_mode: parse_janus_mode(&request.janus.mode)?,
        atoms_in_template: request.atoms_in_template.clone(),
        use_dreadnaut_keys: request.use_dreadnaut_keys,
        hashkey_radius: request.hashkey_radius.clone(),
        hashkey_radius_const: request.hashkey_radius_const,
        pmoi_tolerance: request.pmoi_tolerance,
        enable_pmoi: request.enable_pmoi,
        operator_overrides: RustJanusGaOperatorOverrides {
            pop_replacement_ratio: request.pop_replacement_ratio,
            reinsert_elites_ratio: request.reinsert_elites_ratio,
            max_repop_attempts: request.max_repop_attempts,
            mutation_ratio: request.mutation_ratio,
            mut_selfcross_ratio: request.mut_selfcross_ratio,
            crossover_attempts: request.crossover_attempts,
            tournament_size_min: request.tournament_size_min,
            tournament_size_max: request.tournament_size_max,
        },
    })
}

fn core_request_from_staged_request(
    request: &ScottStagedGaRequest,
) -> Result<RustJanusGaCoreRequest> {
    let base_candidate_input = resolve_cluster_ga_base_candidate(
        request.stage_dir.as_deref(),
        request.base_candidate_json.as_deref(),
        request.base_candidate_path.as_deref(),
        request.resume_from_checkpoint.as_deref(),
    )?;
    Ok(RustJanusGaCoreRequest {
        base_candidate: base_candidate_input
            .as_ref()
            .map(|resolved| resolved.candidate.clone()),
        base_candidate_source_path: base_candidate_input
            .as_ref()
            .map(|resolved| resolved.source_path.clone()),
        workdir: request.workdir.clone(),
        run_dir: request.run_dir.clone(),
        resume_from_checkpoint: request.resume_from_checkpoint.clone(),
        requested_generations: request.ga_generations,
        population_size: request.population,
        seed: request.seed,
        temperature: request.temperature,
        step_size: request.step_size,
        operator_policy_backend: None,
        janus_mode: parse_janus_mode(&request.janus.mode)?,
        atoms_in_template: request.atoms_in_template.clone(),
        use_dreadnaut_keys: request.use_dreadnaut_keys,
        hashkey_radius: request.hashkey_radius.clone(),
        hashkey_radius_const: request.hashkey_radius_const,
        pmoi_tolerance: request.pmoi_tolerance,
        enable_pmoi: request.enable_pmoi,
        operator_overrides: RustJanusGaOperatorOverrides::default(),
    })
}

fn backend_setup_request_from_janus_request(
    request: &JanusPersistentGaRequest,
) -> Result<RustJanusGaBackendSetupRequest> {
    Ok(RustJanusGaBackendSetupRequest {
        workdir: request.workdir.clone(),
        workers: request.workers,
        keep_dirs: request.keep_dirs,
        timeout_secs: request.timeout_secs,
        python_bin: request.janus.python_bin.clone(),
        janus_adapter_script: request.janus.janus_adapter_script.clone(),
        janus_arch: request.janus.arch.clone(),
        janus_model: request.janus.model.clone(),
        janus_device: request.janus.device.clone(),
        janus_dtype: request.janus.dtype.clone(),
        janus_mode: parse_janus_mode(&request.janus.mode)?,
        janus_optimizer: parse_janus_optimizer(&request.janus.optimizer)?,
        janus_fmax: request.janus.fmax,
        janus_steps: request.janus.steps,
    })
}

fn staged_backend_config_from_request(
    request: &ScottStagedGaRequest,
) -> Result<StagedRuntimeBackendConfig> {
    Ok(StagedRuntimeBackendConfig {
        timeout: request.timeout_secs.map(Duration::from_secs),
        scott_input_dir: request.scott_input_dir.clone(),
        executable: request.executable.clone(),
        master_gin_template: request.master_gin_template.clone(),
        run_job_template: request.run_job_template.clone(),
        atoms_in_template: request.atoms_in_template.clone(),
        runtime_default_backend: request.runtime_default_backend.clone(),
        runtime_stage_backend: request
            .runtime_stage_backends
            .iter()
            .map(|(stage, backend)| format!("{stage}={backend}"))
            .collect(),
        python_bin: request.janus.python_bin.clone(),
        janus_adapter_script: request.janus.janus_adapter_script.clone(),
        janus_arch: request.janus.arch.clone(),
        janus_model: request.janus.model.clone(),
        janus_device: request.janus.device.clone(),
        janus_dtype: request.janus.dtype.clone(),
        janus_mode: parse_janus_mode(&request.janus.mode)?.into(),
        janus_optimizer: parse_janus_optimizer(&request.janus.optimizer)?.into(),
        janus_fmax: request.janus.fmax,
        janus_steps: request.janus.steps,
    })
}

fn resolve_cluster_ga_base_candidate(
    stage_dir: Option<&Path>,
    base_candidate_json: Option<&Path>,
    base_candidate_path: Option<&Path>,
    resume_from_checkpoint: Option<&Path>,
) -> Result<Option<ResolvedCandidateInput>> {
    if resume_from_checkpoint.is_some() {
        return Ok(None);
    }
    let mut selection = CandidateInputSelection::default();
    if let Some(path) = base_candidate_json {
        selection.candidate_json = Some(path.to_path_buf());
        selection.structure_path = None;
    }
    if let Some(path) = base_candidate_path {
        selection.candidate_json = None;
        selection.structure_path = Some(path.to_path_buf());
    }
    let resolved = resolve_single_candidate_input(
        SingleCandidateInputContract::cluster_ga_seed(),
        stage_dir,
        &selection,
    )?;
    ensure_native_scott_search_candidate(&resolved.candidate, "Rust cluster GA input")?;
    Ok(Some(resolved))
}

fn build_ga_artifact_hashkey_config(
    use_dreadnaut_keys: bool,
    radius_mode: &str,
    radius_const: f64,
    scratch_dir: PathBuf,
) -> Result<Option<super::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>> {
    if !use_dreadnaut_keys {
        return Ok(None);
    }
    let bundled_path = patina_dreadnaut::bundled_dreadnaut_path();
    let dreadnaut_path =
        verify_hashkey_dreadnaut_adapter(&bundled_path, "GA hashkey artifact preflight")?;
    Ok(Some(
        super::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig {
            radius_mode: radius_mode.to_string(),
            radius_const,
            dreadnaut_path,
            scratch_dir,
        },
    ))
}

fn parse_janus_mode(mode: &str) -> Result<crate::DriverJanusMode> {
    crate::DriverJanusMode::from_str(mode, true)
        .map_err(|_| anyhow!("unsupported Janus mode `{mode}`"))
}

fn parse_janus_optimizer(optimizer: &str) -> Result<crate::DriverJanusOptimizer> {
    crate::DriverJanusOptimizer::from_str(optimizer, true)
        .map_err(|_| anyhow!("unsupported Janus optimizer `{optimizer}`"))
}

fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .max(1)
}
