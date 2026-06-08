use anyhow::{anyhow, Context, Result};
use camino::Utf8PathBuf;
use indexmap::IndexMap;
use patina_evaluator::{
    MasterTemplateLayout, NativeScottProcedureConfig, ScottBackendMode, ScottLatticeMode,
    ScottProcedureIntent, ScottProcedurePlan, ScottProcedureRequest,
};
use patina_external::{GulpBackend, JanusMaceConfig};
use patina_runtime::{run_local_procedure, RoutedBackendStageExecutor, ScottBackendRoutingPolicy};
use patina_types::{Candidate, HybridCrossoverScientificMode};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::driver_support::{
    absolutize_path, build_external_hashkey, candidate_dimensionality_label,
    candidate_dimensionality_support_label, latest_surrogate_artifact_dir,
    parse_runtime_stage_overrides, parse_scott_backend_mode_label, required_arg, EvalBackendKind,
    JanusModeSetting, JanusOptimizerSetting, ScottBackendSettings, ScottRuntimeRoutingSettings,
};
use super::scott_topology_types::AtomSpecRecord;

#[derive(Debug, Clone)]
pub(crate) struct StagedRuntimeBackendConfig {
    pub timeout: Option<Duration>,
    pub scott_input_dir: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub master_gin_template: Option<PathBuf>,
    pub run_job_template: Option<PathBuf>,
    pub atoms_in_template: Option<PathBuf>,
    pub runtime_default_backend: String,
    pub runtime_stage_backend: Vec<String>,
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
pub(crate) struct StagedGaRuntimeRequest {
    pub workdir: PathBuf,
    pub keep_dirs: bool,
    pub backend: StagedRuntimeBackendConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct StagedRuntimeRequest {
    pub workdir: PathBuf,
    pub candidate: Candidate,
    pub procedure_intent: ScottProcedureIntent,
    pub backend: StagedRuntimeBackendConfig,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StagedRuntimeSetup {
    pub default_backend: ScottBackendMode,
    pub routing_policy: ScottBackendRoutingPolicy,
    pub procedure_plan: ScottProcedurePlan,
    pub backend_settings: ScottBackendSettings,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedStagedScottInputs {
    pub master_gin_template: PathBuf,
    pub run_job_template: PathBuf,
    pub atoms_in_template: Option<PathBuf>,
}

fn parse_janus_mode_setting(raw: &str) -> Option<JanusModeSetting> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "single-point" | "single_point" | "singlepoint" => Some(JanusModeSetting::SinglePoint),
        "local-opt" | "local_opt" | "localopt" => Some(JanusModeSetting::LocalOpt),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_scott_runtime_executor(
    runtime_routing: &ScottRuntimeRoutingSettings,
    backend_settings: &ScottBackendSettings,
    timeout: Option<Duration>,
    gulp_executable: Option<PathBuf>,
    master_gin_template: Option<PathBuf>,
    python_bin: Option<PathBuf>,
    janus_adapter_script: Option<PathBuf>,
    janus_arch: String,
    janus_model: String,
    janus_device: String,
    janus_dtype: String,
    janus_mode: JanusModeSetting,
    janus_optimizer: JanusOptimizerSetting,
    janus_fmax: f64,
    janus_steps: usize,
) -> Result<(RoutedBackendStageExecutor, ScottBackendRoutingPolicy)> {
    let default_backend = runtime_routing
        .default_backend
        .as_deref()
        .map(parse_scott_backend_mode_label)
        .transpose()?
        .or_else(|| {
            backend_settings
                .evaluator_backend
                .as_deref()
                .and_then(|label| parse_scott_backend_mode_label(label).ok())
        })
        .unwrap_or(ScottBackendMode::Gulp);

    let mut policy = ScottBackendRoutingPolicy {
        default_backend,
        stage_overrides: IndexMap::new(),
    };
    for (stage, backend) in &runtime_routing.stage_overrides {
        policy = policy.with_stage_backend(*stage, parse_scott_backend_mode_label(backend)?);
    }

    let mut required_backends = BTreeSet::new();
    required_backends.insert(policy.default_backend);
    required_backends.extend(policy.stage_overrides.values().copied());

    let mut executor = RoutedBackendStageExecutor::new().with_routing_policy(policy.clone());
    for backend_mode in required_backends {
        match backend_mode {
            ScottBackendMode::Gulp => {
                let executable = absolutize_path(&required_arg(
                    gulp_executable.clone(),
                    "gulp executable",
                    "scott runtime",
                )?)?;
                let master_gin_template = absolutize_path(&required_arg(
                    master_gin_template.clone(),
                    "master-gin-template",
                    "scott runtime",
                )?)?;
                let atoms_in_template = backend_settings
                    .atoms_in_template
                    .clone()
                    .map(|path| absolutize_path(&path))
                    .transpose()?;
                let mut backend = GulpBackend::new(&master_gin_template, &executable, timeout)?
                    .with_io_names("gulp_klmc.gin", "gulp_klmc.gout");
                backend = match atoms_in_template {
                    Some(atoms_in_template) => {
                        backend.with_scott_sidecars(&master_gin_template, Some(&atoms_in_template))
                    }
                    None => backend.with_scott_sidecars(&master_gin_template, None::<&PathBuf>),
                };
                executor = executor.with_backend(ScottBackendMode::Gulp, Arc::new(backend));
            }
            ScottBackendMode::JanusMace => {
                let python_bin = absolutize_path(
                    &python_bin
                        .clone()
                        .or_else(|| backend_settings.janus_python.clone())
                        .unwrap_or_else(patina_external::default_janus_python_bin),
                )?;
                let adapter_script = absolutize_path(
                    &janus_adapter_script
                        .clone()
                        .or_else(|| backend_settings.janus_adapter.clone())
                        .unwrap_or_else(patina_external::default_janus_adapter_script),
                )?;
                let janus_mode_value = backend_settings
                    .janus_mode
                    .as_deref()
                    .and_then(parse_janus_mode_setting)
                    .unwrap_or(janus_mode);
                let config = JanusMaceConfig {
                    python_bin,
                    adapter_script,
                    arch: backend_settings
                        .janus_arch
                        .clone()
                        .unwrap_or(janus_arch.clone()),
                    model: backend_settings
                        .janus_model
                        .clone()
                        .unwrap_or(janus_model.clone()),
                    device: backend_settings
                        .janus_device
                        .clone()
                        .unwrap_or(janus_device.clone()),
                    default_dtype: backend_settings
                        .janus_dtype
                        .clone()
                        .unwrap_or(janus_dtype.clone()),
                    mode: janus_mode_value.into(),
                    optimizer: janus_optimizer.into(),
                    fmax: backend_settings.janus_fmax.unwrap_or(janus_fmax),
                    steps: backend_settings.janus_steps.unwrap_or(janus_steps),
                };
                executor = executor.with_adaptive_janus_backend(
                    config,
                    timeout,
                    patina_runtime::JanusRetryPolicy {
                        additional_step_multiples_per_retry: 1,
                        max_steps: Some(
                            backend_settings
                                .janus_steps
                                .unwrap_or(janus_steps)
                                .saturating_mul(4),
                        ),
                    },
                );
            }
        }
    }

    Ok((executor, policy))
}

pub(crate) fn build_staged_ga_runtime_setup(
    request: &StagedGaRuntimeRequest,
) -> Result<(
    RoutedBackendStageExecutor,
    ScottBackendRoutingPolicy,
    ScottProcedurePlan,
    Utf8PathBuf,
)> {
    let resolved_inputs = resolve_staged_scott_inputs(
        request.backend.scott_input_dir.as_deref(),
        request.backend.master_gin_template.clone(),
        request.backend.run_job_template.clone(),
        request.backend.atoms_in_template.clone(),
        "staged Scott GA",
    )?;
    let runtime_routing = ScottRuntimeRoutingSettings {
        default_backend: Some(request.backend.runtime_default_backend.clone()),
        stage_overrides: parse_runtime_stage_overrides(&request.backend.runtime_stage_backend)?,
    };
    let backend_settings = ScottBackendSettings {
        evaluator_backend: Some(request.backend.runtime_default_backend.clone()),
        atoms_in_template: resolved_inputs.atoms_in_template.clone(),
        janus_python: request.backend.python_bin.clone(),
        janus_adapter: request.backend.janus_adapter_script.clone(),
        janus_mode: Some(request.backend.janus_mode.as_str().to_string()),
        janus_arch: Some(request.backend.janus_arch.clone()),
        janus_model: Some(request.backend.janus_model.clone()),
        janus_device: Some(request.backend.janus_device.clone()),
        janus_dtype: Some(request.backend.janus_dtype.clone()),
        janus_fmax: Some(request.backend.janus_fmax),
        janus_steps: Some(request.backend.janus_steps),
    };
    let (executor, routing_policy) = build_scott_runtime_executor(
        &runtime_routing,
        &backend_settings,
        request.backend.timeout,
        request.backend.executable.clone(),
        Some(resolved_inputs.master_gin_template.clone()),
        request.backend.python_bin.clone(),
        request.backend.janus_adapter_script.clone(),
        request.backend.janus_arch.clone(),
        request.backend.janus_model.clone(),
        request.backend.janus_device.clone(),
        request.backend.janus_dtype.clone(),
        request.backend.janus_mode,
        request.backend.janus_optimizer,
        request.backend.janus_fmax,
        request.backend.janus_steps,
    )?;
    let utf8_workdir = Utf8PathBuf::from_path_buf(absolutize_path(&request.workdir)?)
        .map_err(|_| anyhow!("staged Scott GA workdir must be valid UTF-8"))?;
    let run_job_template = Utf8PathBuf::from_path_buf(resolved_inputs.run_job_template.clone())
        .map_err(|path| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
    let master_gin_template = Utf8PathBuf::from_path_buf(resolved_inputs.master_gin_template)
        .map_err(|path| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
    let atoms_in_template = resolved_inputs
        .atoms_in_template
        .map(Utf8PathBuf::from_path_buf)
        .transpose()
        .map_err(|path| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
    let mut native_cfg = NativeScottProcedureConfig::from_run_job_path(&run_job_template)
        .map_err(|error| anyhow!("failed to parse staged Scott GA run.job: {error}"))?;
    native_cfg.keep_stage_artifacts = request.keep_dirs;
    let plan = native_cfg
        .into_procedure_plan(
            routing_policy.default_backend,
            ScottProcedureIntent::GeneticAlgorithm,
            ScottLatticeMode::Cluster,
            master_gin_template,
            atoms_in_template,
            utf8_workdir.clone(),
        )
        .map_err(|error| anyhow!("failed to build staged Scott GA procedure plan: {error:?}"))?;

    Ok((executor, routing_policy, plan, utf8_workdir))
}

pub(crate) fn resolve_staged_scott_inputs(
    scott_input_dir: Option<&Path>,
    master_gin_template: Option<PathBuf>,
    run_job_template: Option<PathBuf>,
    atoms_in_template: Option<PathBuf>,
    command_label: &str,
) -> Result<ResolvedStagedScottInputs> {
    let input_dir = scott_input_dir.map(absolutize_path).transpose()?;
    let master_gin_template = resolve_required_scott_input_path(
        master_gin_template,
        input_dir.as_deref(),
        "Master.gin",
        "master-gin-template",
        command_label,
    )?;
    let run_job_template = resolve_required_scott_input_path(
        run_job_template,
        input_dir.as_deref(),
        "run.job",
        "run-job-template",
        command_label,
    )?;
    let atoms_in_template = match atoms_in_template {
        Some(path) => Some(absolutize_path(&path)?),
        None => input_dir
            .as_deref()
            .map(|dir| dir.join("atoms.in"))
            .filter(|path| path.exists()),
    };

    let master_gin_utf8 = Utf8PathBuf::from_path_buf(master_gin_template.clone())
        .map_err(|path| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
    let _template_summary = MasterTemplateLayout::from_path(&master_gin_utf8)
        .with_context(|| {
            format!(
                "failed to parse staged Scott master template `{}`",
                path_display(&master_gin_template)
            )
        })?
        .summary();

    Ok(ResolvedStagedScottInputs {
        master_gin_template,
        run_job_template,
        atoms_in_template,
    })
}

fn resolve_required_scott_input_path(
    explicit_path: Option<PathBuf>,
    input_dir: Option<&Path>,
    bundled_name: &str,
    flag_name: &str,
    command_label: &str,
) -> Result<PathBuf> {
    match explicit_path {
        Some(path) => absolutize_path(&path),
        None => input_dir.map(|dir| dir.join(bundled_name)).ok_or_else(|| {
            anyhow!("`--{flag_name}` or `--scott-input-dir` is required for {command_label}")
        }),
    }
}

fn path_display(path: &Path) -> String {
    path.display().to_string()
}

pub(crate) fn build_staged_runtime_setup(
    request: &StagedRuntimeRequest,
) -> Result<(RoutedBackendStageExecutor, StagedRuntimeSetup, Utf8PathBuf)> {
    let resolved_inputs = resolve_staged_scott_inputs(
        request.backend.scott_input_dir.as_deref(),
        request.backend.master_gin_template.clone(),
        request.backend.run_job_template.clone(),
        request.backend.atoms_in_template.clone(),
        "staged Scott runtime",
    )?;
    let runtime_routing = ScottRuntimeRoutingSettings {
        default_backend: Some(request.backend.runtime_default_backend.clone()),
        stage_overrides: parse_runtime_stage_overrides(&request.backend.runtime_stage_backend)?,
    };

    let backend_settings = ScottBackendSettings {
        evaluator_backend: Some(request.backend.runtime_default_backend.clone()),
        atoms_in_template: resolved_inputs.atoms_in_template.clone(),
        janus_python: request.backend.python_bin.clone(),
        janus_adapter: request.backend.janus_adapter_script.clone(),
        janus_mode: Some(request.backend.janus_mode.as_str().to_string()),
        janus_arch: Some(request.backend.janus_arch.clone()),
        janus_model: Some(request.backend.janus_model.clone()),
        janus_device: Some(request.backend.janus_device.clone()),
        janus_dtype: Some(request.backend.janus_dtype.clone()),
        janus_fmax: Some(request.backend.janus_fmax),
        janus_steps: Some(request.backend.janus_steps),
    };

    let (executor, routing_policy) = build_scott_runtime_executor(
        &runtime_routing,
        &backend_settings,
        request.backend.timeout,
        request.backend.executable.clone(),
        Some(resolved_inputs.master_gin_template.clone()),
        request.backend.python_bin.clone(),
        request.backend.janus_adapter_script.clone(),
        request.backend.janus_arch.clone(),
        request.backend.janus_model.clone(),
        request.backend.janus_device.clone(),
        request.backend.janus_dtype.clone(),
        request.backend.janus_mode,
        request.backend.janus_optimizer,
        request.backend.janus_fmax,
        request.backend.janus_steps,
    )?;

    let default_backend = parse_scott_backend_mode_label(&request.backend.runtime_default_backend)?;
    let master_gin_template = Utf8PathBuf::from_path_buf(resolved_inputs.master_gin_template)
        .map_err(|path: PathBuf| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
    let run_job_template = Utf8PathBuf::from_path_buf(resolved_inputs.run_job_template)
        .map_err(|path: PathBuf| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
    let abs_workdir = absolutize_path(&request.workdir)?;
    let utf8_workdir = Utf8PathBuf::from_path_buf(abs_workdir)
        .map_err(|path: PathBuf| anyhow!("path `{}` is not valid UTF-8", path.display()))?;

    let native_cfg = NativeScottProcedureConfig::from_run_job_path(&run_job_template)
        .map_err(|error| anyhow!("failed to parse staged Scott runtime run.job: {error}"))?;
    let procedure_plan = native_cfg
        .into_procedure_plan(
            default_backend,
            request.procedure_intent,
            match request.candidate.declared_dimensionality() {
                patina_types::StructureDimensionality::ZeroD => ScottLatticeMode::Cluster,
                patina_types::StructureDimensionality::ThreeD => ScottLatticeMode::Periodic3d,
                patina_types::StructureDimensionality::OneD
                | patina_types::StructureDimensionality::TwoD => {
                    return Err(anyhow!(
                        "staged Scott runtime does not support {} candidates in the native-parity lane; support_status={}",
                        candidate_dimensionality_label(&request.candidate),
                        candidate_dimensionality_support_label(&request.candidate),
                    ));
                }
            },
            master_gin_template,
            resolved_inputs
                .atoms_in_template
                .map(Utf8PathBuf::from_path_buf)
                .transpose()
                .map_err(|path: PathBuf| anyhow!("path `{}` is not valid UTF-8", path.display()))?,
            utf8_workdir.clone(),
        )
        .map_err(|err| anyhow!("failed to build staged Scott procedure plan: {err:?}"))?;

    Ok((
        executor,
        StagedRuntimeSetup {
            default_backend,
            routing_policy,
            procedure_plan,
            backend_settings,
        },
        utf8_workdir,
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct LocalStagedProductionPortConfig {
    pub runtime: StagedRuntimeBackendConfig,
    pub production_cfg: super::scott_production::ProductionRunConfig,
    pub atom_specs: Option<Vec<AtomSpecRecord>>,
    pub topology_hkg_path: Option<PathBuf>,
    pub hashkey_runtime_dir: PathBuf,
}

pub(crate) struct LocalStagedProductionPort {
    pub config: LocalStagedProductionPortConfig,
}

impl super::ports::ProductionEvaluationPort for LocalStagedProductionPort {
    fn evaluate_candidate(
        &self,
        request: &super::ports::ProductionEvaluationRequest,
    ) -> Result<super::ports::ProductionEvaluationOutput> {
        let runtime_request = StagedRuntimeRequest {
            workdir: request.candidate_workdir.clone(),
            candidate: request.candidate.clone(),
            procedure_intent: ScottProcedureIntent::ProductionRun,
            backend: self.config.runtime.clone(),
        };
        let (executor, setup, utf8_workdir) = build_staged_runtime_setup(&runtime_request)?;
        let procedure_request = ScottProcedureRequest {
            candidate: request.candidate.clone(),
            request_id: format!(
                "production-{}-{}",
                request.candidate.label, request.seed_counter
            ),
            workdir: utf8_workdir,
        };
        let outcome = run_local_procedure(&setup.procedure_plan, &procedure_request, &executor)?;
        Ok(super::ports::ProductionEvaluationOutput {
            default_backend: setup.default_backend,
            routing_policy: setup.routing_policy,
            procedure_plan: setup.procedure_plan,
            outcome,
        })
    }
}

impl super::ports::ProductionIdentityPort for LocalStagedProductionPort {
    fn build_input_hashkey(
        &self,
        request: &super::ports::ProductionEvaluationRequest,
    ) -> Result<Option<String>> {
        if !self.config.production_cfg.use_top_analysis {
            return Ok(None);
        }
        match (
            self.config.atom_specs.as_deref(),
            self.config.topology_hkg_path.as_deref(),
        ) {
            (Some(atom_specs), Some(hkg_path)) => build_external_hashkey(
                &request.candidate,
                Some(atom_specs),
                &self.config.production_cfg.hashkey_radius_mode,
                self.config.production_cfg.hashkey_radius_const,
                hkg_path,
                &self.config.hashkey_runtime_dir,
                &format!("production_{:04}_input", request.index),
            ),
            _ => Ok(None),
        }
    }

    fn build_final_hashkey(
        &self,
        request: &super::ports::ProductionEvaluationRequest,
        evaluated: &super::ports::ProductionEvaluationOutput,
        fallback_hashkey: Option<&str>,
    ) -> Result<Option<String>> {
        if !self.config.production_cfg.use_top_analysis {
            return Ok(None);
        }
        match (
            evaluated.outcome.final_result.as_ref(),
            self.config.atom_specs.as_deref(),
            self.config.topology_hkg_path.as_deref(),
        ) {
            (Some(result), Some(atom_specs), Some(hkg_path)) => build_external_hashkey(
                &result.relaxed_candidate,
                Some(atom_specs),
                &self.config.production_cfg.hashkey_radius_mode,
                self.config.production_cfg.hashkey_radius_const,
                hkg_path,
                &self.config.hashkey_runtime_dir,
                &format!("production_{:04}_final", request.index),
            ),
            _ => Ok(fallback_hashkey.map(ToOwned::to_owned)),
        }
    }
}

pub(crate) struct LocalProductionProgressPort {
    pub restart_dir: Option<PathBuf>,
}

impl super::ports::ProductionProgressPort for LocalProductionProgressPort {
    fn mark_seed_consumed(
        &self,
        source_name: Option<&str>,
        restart_state: Option<&super::scott_production::ProductionRestartState>,
    ) -> Result<()> {
        let Some(restart_dir) = self.restart_dir.as_deref() else {
            return Ok(());
        };
        if let Some(source_name) = source_name {
            super::scott_production::append_restart_done_entry(restart_dir, source_name)?;
        }
        if let Some(state) = restart_state {
            super::scott_production::write_restart_state(restart_dir, state)?;
        }
        Ok(())
    }
}

pub(crate) struct LocalProductionArtifactSink {
    pub run_dir: Option<PathBuf>,
}

impl super::ports::ProductionArtifactSink for LocalProductionArtifactSink {
    fn persist_accepted_candidate(
        &self,
        artifact: &super::ports::ProductionAcceptedArtifactRecord,
    ) -> Result<()> {
        let Some(run_dir) = self.run_dir.as_ref() else {
            return Ok(());
        };
        let procedure_digest = artifact.outcome.state_digest();
        let final_result = artifact.outcome.final_result.as_ref();
        let candidate_run_dir = run_dir.join(format!("candidate_{:04}", artifact.index));
        super::single_eval_artifacts::write_single_eval_artifacts(
            &candidate_run_dir,
            &super::single_eval_artifacts::SingleEvalArtifactArgs {
                workdir: artifact.candidate_workdir.clone(),
                mode: "production".into(),
                system: artifact.system.clone(),
                engine: format!("Scott staged ({:?})", artifact.default_backend),
                backend: if artifact.default_backend == ScottBackendMode::JanusMace {
                    EvalBackendKind::JanusMace
                } else {
                    EvalBackendKind::Gulp
                },
                backend_metadata: Some(serde_json::json!({
                    "routing_policy": artifact.routing_policy,
                    "procedure_plan": artifact.procedure_plan,
                    "procedure_outcome": artifact.outcome,
                    "procedure_state_digest": procedure_digest.clone(),
                    "production_decision": artifact.decision,
                    "production_hashkey": artifact.final_hashkey,
                    "best_set_rank": artifact.best_set_rank,
                })),
                procedure_state_digest: Some(procedure_digest),
                failure_reason: None,
            },
            &artifact.candidate,
            final_result,
        )?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct HybridGaProductionSummaryReport {
    pub workflow_owner: &'static str,
    pub influence_mode: &'static str,
    pub system: String,
    pub run_dir: PathBuf,
    pub ga_run_dir: PathBuf,
    pub production_run_dir: PathBuf,
    pub emulate_artifact_dir: Option<PathBuf>,
    pub ga_generation_count: usize,
    pub ga_population_size: usize,
    pub scientific_mode: HybridCrossoverScientificMode,
    pub selected_seed_count: usize,
    pub selected_seed_labels: Vec<String>,
    pub accepted_child_count: usize,
    pub accepted_child_labels: Vec<String>,
    pub production_input_count: usize,
    pub production_success_count: usize,
    pub production_failure_count: usize,
    pub best_set_size: usize,
}

pub(crate) struct LocalHybridGaProductionArtifactSink {
    pub run_dir: PathBuf,
    pub influence_mode: &'static str,
    pub emulate_output_dir: Option<PathBuf>,
}

impl super::ports::HybridGaProductionArtifactSink for LocalHybridGaProductionArtifactSink {
    fn persist_hybrid_ga_production_run(
        &self,
        execution: &super::hybrid_ga_production::HybridGaProductionExecution,
    ) -> Result<()> {
        fs::create_dir_all(&self.run_dir).with_context(|| {
            format!(
                "failed to create hybrid run dir `{}`",
                self.run_dir.display()
            )
        })?;
        let emulate_artifact_dir = self
            .emulate_output_dir
            .as_deref()
            .map(latest_surrogate_artifact_dir)
            .transpose()?
            .flatten();
        let summary = HybridGaProductionSummaryReport {
            workflow_owner: "hybrid_ga_production",
            influence_mode: self.influence_mode,
            system: execution.summary.system.clone(),
            run_dir: self.run_dir.clone(),
            ga_run_dir: self.run_dir.join("ga"),
            production_run_dir: self.run_dir.join("production"),
            emulate_artifact_dir,
            ga_generation_count: execution.ga_execution.generation_artifacts.len(),
            ga_population_size: execution.summary.ga_population_size,
            scientific_mode: execution.summary.scientific_mode,
            selected_seed_count: execution.summary.selected_seed_count,
            selected_seed_labels: execution
                .selected_seeds
                .iter()
                .map(|seed| seed.selected_candidate_label.clone())
                .collect(),
            accepted_child_count: execution.summary.accepted_child_count,
            accepted_child_labels: execution
                .accepted_children
                .iter()
                .map(|child| child.child_candidate_label.clone())
                .collect(),
            production_input_count: execution.summary.production_input_count,
            production_success_count: execution.summary.production_success_count,
            production_failure_count: execution.summary.production_failure_count,
            best_set_size: execution.summary.best_set_size,
        };
        fs::write(
            self.run_dir.join("hybrid_summary.json"),
            serde_json::to_string_pretty(&summary).context("failed to serialize hybrid summary")?,
        )
        .with_context(|| {
            format!(
                "failed to write `{}`",
                self.run_dir.join("hybrid_summary.json").display()
            )
        })?;
        fs::write(
            self.run_dir.join("selected_seeds.json"),
            serde_json::to_string_pretty(&execution.selected_seeds)
                .context("failed to serialize selected seeds")?,
        )
        .with_context(|| {
            format!(
                "failed to write `{}`",
                self.run_dir.join("selected_seeds.json").display()
            )
        })?;
        fs::write(
            self.run_dir.join("accepted_children.json"),
            serde_json::to_string_pretty(&execution.accepted_children)
                .context("failed to serialize accepted hybrid children")?,
        )
        .with_context(|| {
            format!(
                "failed to write `{}`",
                self.run_dir.join("accepted_children.json").display()
            )
        })?;
        Ok(())
    }
}
