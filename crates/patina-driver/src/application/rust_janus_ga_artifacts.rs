use anyhow::{Context, Result};
use patina_sci_kernel::codec::xyz::{write_candidate_xyz, XyzCoordinateMode, XyzEncodeOptions};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::ga_execution::{
    RustJanusGaExecution, RustJanusGenerationArtifact, RustJanusOriginMetricRow,
};
use super::ports::{GaArtifactSink, GaRunArtifactSink};
use super::report_types::RustJanusSearchSummary;
use super::workflow_provenance::{
    build_workflow_provenance, TemplateBundleProvenance, WorkflowProvenanceSpec,
};

pub struct RustJanusIncrementalArtifactMetadata<'a> {
    pub run_dir: &'a Path,
    pub system: &'a str,
    pub search_cfg: &'a patina_types::SearchConfig,
    pub requested_generations: usize,
    pub backend_mode_label: &'a str,
    pub operator_policy: &'a crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    pub hashkey_config:
        Option<&'a crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>,
}

pub struct RustJanusIncrementalArtifactSink<'a> {
    pub metadata: RustJanusIncrementalArtifactMetadata<'a>,
}

pub struct RustJanusGaBackendArtifactMetadata<'a> {
    pub python_bin: &'a Path,
    pub adapter_script: &'a Path,
    pub arch: &'a str,
    pub model: &'a str,
    pub device: &'a str,
    pub dtype: &'a str,
    pub mode_label: &'a str,
    pub optimizer_label: &'a str,
    pub fmax: f64,
    pub steps: usize,
    pub worker_session_dir: &'a Path,
}

pub struct RustJanusGaArtifactContext<'a> {
    pub run_dir: &'a Path,
    pub system: &'a str,
    pub base_candidate_source_path: Option<&'a Path>,
    pub search_cfg: &'a patina_types::SearchConfig,
    pub workers: usize,
    pub requested_generations: usize,
    pub population_size: usize,
    pub temperature: f64,
    pub step_size: f64,
    pub enable_pmoi: bool,
    pub pmoi_tolerance: f64,
    pub lane_mode: crate::application::rust_janus_ga::RustJanusLaneMode,
    pub duplicate_policy_mode: crate::application::rust_janus_ga::RustJanusDuplicatePolicyMode,
    pub duplicate_filter_stack: &'a [crate::application::filter_taxonomy::FilterDescriptor],
    pub operator_policy: &'a crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    pub hashkey_config:
        Option<&'a crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>,
    pub backend: RustJanusGaBackendArtifactMetadata<'a>,
}

pub struct StagedScottIncrementalArtifactSink<'a> {
    pub run_dir: &'a Path,
    pub system: &'a str,
    pub search_cfg: &'a patina_types::SearchConfig,
    pub requested_generations: usize,
    pub operator_policy: &'a crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    pub backend_mode_label: &'a str,
    pub hashkey_config:
        Option<&'a crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>,
    pub duplicate_trace: &'a crate::application::rust_janus_ga::SharedScottDuplicateTrace,
    pub procedure_traces:
        Arc<Mutex<Vec<crate::application::scott_ga_runtime::StagedScottProcedureTraceRecord>>>,
}

struct GenerationExecutionSlice<'a> {
    artifact: &'a RustJanusGenerationArtifact,
    responses: &'a [patina_types::WorkerResponse],
    origin_metrics: &'a [RustJanusOriginMetricRow],
    artifact_history: &'a [RustJanusGenerationArtifact],
    boundary_population: &'a [patina_search::ScottGaMember],
    population: &'a [patina_search::ScottGaMember],
}

fn generation_execution_slice(
    execution: &RustJanusGaExecution,
    generation_index: usize,
) -> Result<GenerationExecutionSlice<'_>> {
    Ok(GenerationExecutionSlice {
        artifact: execution
            .generation_artifacts
            .get(generation_index)
            .with_context(|| format!("missing generation artifact at index {generation_index}"))?,
        responses: execution
            .generation_responses
            .get(generation_index)
            .with_context(|| format!("missing generation responses at index {generation_index}"))?,
        origin_metrics: execution
            .generation_origin_metrics
            .get(generation_index)
            .with_context(|| {
                format!("missing generation origin metrics at index {generation_index}")
            })?,
        artifact_history: execution
            .generation_artifacts
            .get(..=generation_index)
            .with_context(|| {
                format!("missing generation artifact history through index {generation_index}")
            })?,
        boundary_population: execution
            .generation_boundary_populations
            .get(generation_index)
            .with_context(|| {
                format!("missing generation boundary population at index {generation_index}")
            })?,
        population: execution
            .generation_populations
            .get(generation_index)
            .with_context(|| {
                format!("missing generation population at index {generation_index}")
            })?,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct StagedScottGaSummaryReport {
    pub workflow_id: &'static str,
    pub workflow_owner: &'static str,
    pub backend_routing: patina_runtime::ScottBackendRoutingPolicy,
    pub requested_generations: usize,
    pub population_size: usize,
    pub system: String,
    pub workdir: std::path::PathBuf,
    pub run_dir: std::path::PathBuf,
    pub generation_count: usize,
    pub typed_generation_snapshot_count: usize,
    pub final_population_size: usize,
    pub best_final_energy: Option<f64>,
    pub procedure_trace_count: usize,
    pub procedure_trace_failure_count: usize,
    pub duplicate_trace_count: usize,
    pub duplicate_trace_summary: StagedScottDuplicateTraceSummary,
    pub emulate_gate: Option<crate::application::ga_emulate_gate::EmulateGateRunSummary>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StagedScottDuplicateTraceSummary {
    pub by_reason: BTreeMap<String, usize>,
    pub exact_hashkey_duplicate_count: usize,
    pub fallback_duplicate_count: usize,
    pub exact_hashkey_nonmatch_count: usize,
    pub exact_hashkey_unknown_count: usize,
}

pub struct StagedScottGaArtifactSink<'a> {
    pub metadata: &'a StagedScottGaArtifactMetadata,
    pub base_candidate_source_path: Option<&'a Path>,
    pub search_cfg: &'a patina_types::SearchConfig,
    pub operator_policy: &'a crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    pub hashkey_config:
        Option<&'a crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>,
    pub lane_mode: crate::application::rust_janus_ga::RustJanusLaneMode,
    pub duplicate_policy_mode: crate::application::rust_janus_ga::RustJanusDuplicatePolicyMode,
    pub duplicate_filter_stack: &'a [crate::application::filter_taxonomy::FilterDescriptor],
    pub duplicate_traces: &'a [crate::application::rust_janus_ga::ScottGaDuplicateTraceRecord],
    pub routing_policy: &'a patina_runtime::ScottBackendRoutingPolicy,
    pub procedure_plan: &'a patina_evaluator::ScottProcedurePlan,
    pub procedure_traces:
        &'a [crate::application::scott_ga_runtime::StagedScottProcedureTraceRecord],
    pub emulate_gate_summary:
        Option<&'a crate::application::ga_emulate_gate::EmulateGateRunSummary>,
    pub emulate_gate_decision_traces:
        Option<&'a [crate::application::ga_emulate_gate::EmulateGateDecisionTrace]>,
    pub emulate_gate_batch_traces:
        Option<&'a [crate::application::ga_emulate_gate::EmulateGateBatchTrace]>,
    pub workflow_policy: crate::application::workflow_policy::GaWorkflowPolicy,
}

#[derive(Debug, Clone)]
pub struct StagedScottGaArtifactMetadata {
    pub run_dir: std::path::PathBuf,
    pub workdir: std::path::PathBuf,
    pub system: String,
    pub ga_generations: usize,
    pub population: usize,
    pub temperature: f64,
    pub step_size: f64,
    pub runtime_default_backend: String,
    pub runtime_stage_backend: Vec<String>,
    pub run_job_template: Option<std::path::PathBuf>,
    pub master_gin_template: Option<std::path::PathBuf>,
    pub atoms_in_template: Option<std::path::PathBuf>,
}

impl GaRunArtifactSink for StagedScottGaArtifactSink<'_> {
    type Summary = StagedScottGaSummaryReport;

    fn persist_run(&self, execution: &RustJanusGaExecution) -> Result<Self::Summary> {
        ensure_generation_snapshot_alignment(execution)?;
        let backend_mode_label = match self.routing_policy.default_backend {
            patina_evaluator::ScottBackendMode::Gulp => "gulp",
            patina_evaluator::ScottBackendMode::JanusMace => "janus_mace",
        };
        let mut hashkey_computer = self
            .hashkey_config
            .map(crate::application::rust_janus_ga_checkpoint::GaHashkeyComputer::new)
            .transpose()?;
        let checkpoint =
            crate::application::rust_janus_ga_checkpoint::build_checkpoint_with_hashkeys(
                crate::application::rust_janus_ga_checkpoint::RustGaCheckpointBuildRequest {
                    context:
                        crate::application::rust_janus_ga_checkpoint::RustGaCheckpointContext {
                            workflow_id: "ga.scott-monolithic",
                            workflow_owner: "scott_monolithic_ga",
                            backend: "scott_runtime",
                            backend_mode: backend_mode_label,
                            system: &self.metadata.system,
                            requested_generations: self.metadata.ga_generations,
                        },
                    search_cfg: self.search_cfg,
                    operator_policy: self.operator_policy,
                    generations: &execution.generation_artifacts,
                    final_population: &execution.final_population,
                    hashkeys: hashkey_computer.as_mut(),
                },
            )?;
        let total_responses = execution
            .generation_responses
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let telemetry = crate::application::report_types::summarize_batch_responses(
            &total_responses,
            execution
                .generation_artifacts
                .iter()
                .map(|row| row.elapsed_secs)
                .sum(),
        );
        let origin_metrics = execution
            .generation_origin_metrics
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let procedure_trace_failure_count = self
            .procedure_traces
            .iter()
            .filter(|trace| {
                !matches!(
                    trace.status,
                    crate::application::scott_ga_runtime::StagedScottProcedureTraceStatus::Accepted
                )
            })
            .count();
        let duplicate_trace_summary = summarize_staged_scott_duplicate_trace(self.duplicate_traces);
        let emulate_gate_enabled = self.emulate_gate_summary.is_some();
        let search_summary = serde_json::json!({
            "workflow_id": "ga.scott-monolithic",
            "workflow_owner": "scott_monolithic_ga",
            "backend": "scott_runtime",
            "lane_mode": self.lane_mode.as_str(),
            "workflow_policy": self.workflow_policy,
            "duplicate_policy_mode": self.duplicate_policy_mode.as_str(),
            "duplicate_filter_stack": self.duplicate_filter_stack,
            "workers": serde_json::Value::Null,
            "requested_generations": self.metadata.ga_generations,
            "population_size": self.metadata.population,
            "seed": self.search_cfg.seed.unwrap_or(1),
            "system": self.metadata.system,
            "telemetry": telemetry,
            "generations": &execution.generation_artifacts,
            "origin_metrics": origin_metrics,
            "operator_policy": self.operator_policy,
            "routing_policy": self.routing_policy,
            "procedure_plan": self.procedure_plan,
            "procedure_trace_count": self.procedure_traces.len(),
            "procedure_trace_failure_count": procedure_trace_failure_count,
            "duplicate_trace_count": self.duplicate_traces.len(),
            "duplicate_trace_summary": &duplicate_trace_summary,
            "emulate_gate_enabled": emulate_gate_enabled,
            "emulate_gate_summary": self.emulate_gate_summary,
            "emulate_gate_decision_trace_count": self.emulate_gate_decision_traces.map(|records| records.len()),
            "emulate_gate_batch_trace_count": self.emulate_gate_batch_traces.map(|records| records.len()),
            "typed_generation_snapshot_count": execution.generation_kernel_states.len(),
            "runtime_default_backend": &self.metadata.runtime_default_backend,
            "runtime_stage_backend": &self.metadata.runtime_stage_backend,
        });
        let run_name = self
            .metadata
            .run_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unnamed-scott-monolithic-ga");
        write_generic_ga_artifacts(
            &self.metadata.run_dir,
            self.base_candidate_source_path,
            &GenericGaArtifactManifest {
                run_name,
                workflow_id: "ga.scott-monolithic",
                workflow_owner: "scott_monolithic_ga",
                workflow_scope: "Rust-owned GA controller with monolithic Scott evaluator/runtime generation execution",
                backend: "scott_runtime",
                parallel_contract: "monolithic_scott_runtime_generation_dispatch",
                system: &self.metadata.system,
                requested_generations: self.metadata.ga_generations,
                population_size: self.metadata.population,
                seed: self.search_cfg.seed,
                lane_mode: self.lane_mode.as_str(),
                duplicate_policy_mode: self.duplicate_policy_mode.as_str(),
                search_config_temperature: self.metadata.temperature,
                search_config_step_size: self.metadata.step_size,
                extra: serde_json::json!({
                    "routing_policy": self.routing_policy,
                    "procedure_plan": self.procedure_plan,
                    "procedure_trace": "raw/staged_scott_procedure_trace.json",
                    "duplicate_trace": "raw/staged_scott_duplicate_trace.json",
                    "duplicate_trace_summary": "raw/staged_scott_duplicate_summary.json",
                    "emulate_gate_summary": emulate_gate_enabled.then_some("raw/emulate_gate_summary.json"),
                    "emulate_gate_decision_trace": emulate_gate_enabled.then_some("raw/emulate_gate_decision_trace.json"),
                    "emulate_gate_batch_trace": emulate_gate_enabled.then_some("raw/emulate_gate_batch_trace.json"),
                    "runtime_default_backend": &self.metadata.runtime_default_backend,
                    "runtime_stage_backend": &self.metadata.runtime_stage_backend,
                }),
                provenance: serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "ga.scott-monolithic",
                    workflow_owner: "scott_monolithic_ga",
                    backend_id: Some("scott_runtime"),
                    backend_mode: Some(backend_mode_label),
                    lane_mode: Some(self.lane_mode.as_str()),
                    duplicate_policy_mode: Some(self.duplicate_policy_mode.as_str()),
                    parallel_contract: Some("monolithic_scott_runtime_generation_dispatch"),
                    run_dir: Some(self.metadata.run_dir.as_path()),
                    workdir: Some(self.metadata.workdir.as_path()),
                    source_run_dir: None,
                    source_path: None,
                    candidate_json: self.base_candidate_source_path,
                    template_bundle: Some(TemplateBundleProvenance {
                        run_job: self.metadata.run_job_template.as_deref(),
                        master_gin: self.metadata.master_gin_template.as_deref(),
                        atoms_in: self.metadata.atoms_in_template.as_deref(),
                        jobs: None,
                    }),
                })?)?,
            },
            self.hashkey_config,
            self.operator_policy,
            &execution.generation_artifacts,
            &execution.generation_responses,
            &execution.generation_origin_metrics,
            &execution.generation_boundary_populations,
            &execution.generation_populations,
            &execution.controller_trace,
            &execution.final_population,
            &checkpoint,
            &search_summary,
        )?;
        fs::write(
            self.metadata
                .run_dir
                .join("raw")
                .join("staged_scott_procedure_trace.json"),
            serde_json::to_string_pretty(self.procedure_traces)
                .context("failed to serialize staged Scott procedure trace")?,
        )
        .with_context(|| {
            format!(
                "failed to write staged Scott procedure trace in `{}`",
                self.metadata.run_dir.join("raw").display()
            )
        })?;
        fs::write(
            self.metadata
                .run_dir
                .join("raw")
                .join("staged_scott_duplicate_trace.json"),
            serde_json::to_string_pretty(self.duplicate_traces)
                .context("failed to serialize staged Scott duplicate trace")?,
        )
        .with_context(|| {
            format!(
                "failed to write staged Scott duplicate trace in `{}`",
                self.metadata.run_dir.join("raw").display()
            )
        })?;
        fs::write(
            self.metadata
                .run_dir
                .join("raw")
                .join("staged_scott_duplicate_summary.json"),
            serde_json::to_string_pretty(&duplicate_trace_summary)
                .context("failed to serialize staged Scott duplicate summary")?,
        )
        .with_context(|| {
            format!(
                "failed to write staged Scott duplicate summary in `{}`",
                self.metadata.run_dir.join("raw").display()
            )
        })?;
        if let Some(summary) = self.emulate_gate_summary {
            fs::write(
                self.metadata
                    .run_dir
                    .join("raw")
                    .join("emulate_gate_summary.json"),
                serde_json::to_string_pretty(summary)
                    .context("failed to serialize emulate gate summary")?,
            )
            .with_context(|| {
                format!(
                    "failed to write emulate gate summary in `{}`",
                    self.metadata.run_dir.join("raw").display()
                )
            })?;
        }
        if let Some(traces) = self.emulate_gate_decision_traces {
            fs::write(
                self.metadata
                    .run_dir
                    .join("raw")
                    .join("emulate_gate_decision_trace.json"),
                serde_json::to_string_pretty(traces)
                    .context("failed to serialize emulate gate decision trace")?,
            )
            .with_context(|| {
                format!(
                    "failed to write emulate gate decision trace in `{}`",
                    self.metadata.run_dir.join("raw").display()
                )
            })?;
        }
        if let Some(traces) = self.emulate_gate_batch_traces {
            fs::write(
                self.metadata
                    .run_dir
                    .join("raw")
                    .join("emulate_gate_batch_trace.json"),
                serde_json::to_string_pretty(traces)
                    .context("failed to serialize emulate gate batch trace")?,
            )
            .with_context(|| {
                format!(
                    "failed to write emulate gate batch trace in `{}`",
                    self.metadata.run_dir.join("raw").display()
                )
            })?;
        }

        let summary = StagedScottGaSummaryReport {
            workflow_id: "ga.scott-monolithic",
            workflow_owner: "scott_monolithic_ga",
            backend_routing: self.routing_policy.clone(),
            requested_generations: self.metadata.ga_generations,
            population_size: self.metadata.population,
            system: self.metadata.system.clone(),
            workdir: self.metadata.workdir.clone(),
            run_dir: self.metadata.run_dir.clone(),
            generation_count: execution.generation_artifacts.len(),
            typed_generation_snapshot_count: execution.generation_kernel_states.len(),
            final_population_size: execution.final_population.len(),
            best_final_energy: execution
                .final_population
                .first()
                .map(|member| member.result.energy),
            procedure_trace_count: self.procedure_traces.len(),
            procedure_trace_failure_count,
            duplicate_trace_count: self.duplicate_traces.len(),
            duplicate_trace_summary,
            emulate_gate: self.emulate_gate_summary.cloned(),
        };

        fs::write(
            self.metadata
                .run_dir
                .join("raw")
                .join("staged_ga_summary.json"),
            serde_json::to_string_pretty(&summary)
                .context("failed to serialize staged Scott GA summary")?,
        )
        .with_context(|| {
            format!(
                "failed to write staged Scott GA summary in `{}`",
                self.metadata.run_dir.join("raw").display()
            )
        })?;

        Ok(summary)
    }
}

fn ensure_generation_snapshot_alignment(execution: &RustJanusGaExecution) -> Result<()> {
    let artifact_count = execution.generation_artifacts.len();
    let boundary_population_count = execution.generation_boundary_populations.len();
    let population_count = execution.generation_populations.len();
    let boundary_kernel_count = execution.generation_boundary_kernel_states.len();
    let kernel_count = execution.generation_kernel_states.len();
    if artifact_count != boundary_population_count
        || artifact_count != population_count
        || artifact_count != boundary_kernel_count
        || artifact_count != kernel_count
    {
        return Err(anyhow::anyhow!(
            "GA artifact state mismatch: artifacts={artifact_count}, boundary_populations={boundary_population_count}, populations={population_count}, boundary_typed_snapshots={boundary_kernel_count}, typed_snapshots={kernel_count}"
        ));
    }
    Ok(())
}

fn summarize_staged_scott_duplicate_trace(
    traces: &[crate::application::rust_janus_ga::ScottGaDuplicateTraceRecord],
) -> StagedScottDuplicateTraceSummary {
    let mut summary = StagedScottDuplicateTraceSummary::default();
    for trace in traces {
        *summary
            .by_reason
            .entry(
                match trace.reason {
                    patina_evaluator::ScottDuplicateReason::Hashkey => "hashkey",
                    patina_evaluator::ScottDuplicateReason::Pmoi => "pmoi",
                    patina_evaluator::ScottDuplicateReason::EnergyTolerance => "energy_tolerance",
                }
                .to_string(),
            )
            .or_insert(0) += 1;
        match trace.exact_hashkey_match {
            Some(true) => summary.exact_hashkey_duplicate_count += 1,
            Some(false) => summary.exact_hashkey_nonmatch_count += 1,
            None => summary.exact_hashkey_unknown_count += 1,
        }
        if !matches!(
            trace.reason,
            patina_evaluator::ScottDuplicateReason::Hashkey
        ) {
            summary.fallback_duplicate_count += 1;
        }
    }
    summary
}

fn snapshot_staged_procedure_trace(
    traces: &Arc<Mutex<Vec<crate::application::scott_ga_runtime::StagedScottProcedureTraceRecord>>>,
) -> Vec<crate::application::scott_ga_runtime::StagedScottProcedureTraceRecord> {
    match traces.lock() {
        Ok(records) => records.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn write_staged_scott_live_diagnostics(
    run_dir: &Path,
    generation: usize,
    procedure_traces: &[crate::application::scott_ga_runtime::StagedScottProcedureTraceRecord],
    duplicate_traces: &[crate::application::rust_janus_ga::ScottGaDuplicateTraceRecord],
) -> Result<()> {
    let raw_dir = run_dir.join("raw");
    fs::create_dir_all(&raw_dir)?;

    fs::write(
        raw_dir.join("staged_scott_procedure_trace.json"),
        serde_json::to_string_pretty(procedure_traces)
            .context("failed to serialize live staged Scott procedure trace")?,
    )?;
    let generation_procedure_traces = procedure_traces
        .iter()
        .filter(|trace| trace.generation == Some(generation))
        .cloned()
        .collect::<Vec<_>>();
    fs::write(
        raw_dir.join(format!("generation_{generation:04}_procedure_trace.json")),
        serde_json::to_string_pretty(&generation_procedure_traces)
            .context("failed to serialize generation staged Scott procedure trace")?,
    )?;

    let duplicate_summary = summarize_staged_scott_duplicate_trace(duplicate_traces);
    fs::write(
        raw_dir.join("staged_scott_duplicate_trace.json"),
        serde_json::to_string_pretty(duplicate_traces)
            .context("failed to serialize live staged Scott duplicate trace")?,
    )?;
    fs::write(
        raw_dir.join("staged_scott_duplicate_summary.json"),
        serde_json::to_string_pretty(&duplicate_summary)
            .context("failed to serialize live staged Scott duplicate summary")?,
    )?;

    Ok(())
}

impl GaArtifactSink for RustJanusIncrementalArtifactSink<'_> {
    fn persist_generation(
        &self,
        generation_index: usize,
        execution: &RustJanusGaExecution,
    ) -> Result<()> {
        let generation = generation_execution_slice(execution, generation_index)?;
        write_incremental_generation_artifacts(
            &self.metadata,
            generation.artifact,
            generation.responses,
            generation.origin_metrics,
            generation.artifact_history,
            generation.boundary_population,
            generation.population,
        )
    }
}

impl GaArtifactSink for StagedScottIncrementalArtifactSink<'_> {
    fn persist_generation(
        &self,
        generation_index: usize,
        execution: &RustJanusGaExecution,
    ) -> Result<()> {
        let generation_slice = generation_execution_slice(execution, generation_index)?;
        write_incremental_generation_artifacts_with_metadata(
            self.run_dir,
            &IncrementalGaCheckpointMetadata {
                workflow_id: "ga.scott-monolithic",
                workflow_owner: "scott_monolithic_ga",
                backend: "scott_runtime",
                backend_mode: self.backend_mode_label,
                system: self.system,
                requested_generations: self.requested_generations,
                hashkey_config: self.hashkey_config,
            },
            self.search_cfg,
            self.operator_policy,
            generation_slice.artifact,
            generation_slice.responses,
            generation_slice.origin_metrics,
            generation_slice.artifact_history,
            generation_slice.boundary_population,
            generation_slice.population,
        )?;
        let generation = generation_slice.artifact.generation;
        let procedure_traces = snapshot_staged_procedure_trace(&self.procedure_traces);
        let duplicate_traces =
            crate::application::rust_janus_ga::snapshot_duplicate_trace(self.duplicate_trace);
        write_staged_scott_live_diagnostics(
            self.run_dir,
            generation,
            &procedure_traces,
            &duplicate_traces,
        )
    }
}

struct IncrementalGaCheckpointMetadata<'a> {
    workflow_id: &'a str,
    workflow_owner: &'a str,
    backend: &'a str,
    backend_mode: &'a str,
    system: &'a str,
    requested_generations: usize,
    hashkey_config:
        Option<&'a crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>,
}

fn write_generation_failure_summary(
    raw_dir: &Path,
    generation_artifact: &RustJanusGenerationArtifact,
    responses: &[patina_types::WorkerResponse],
) -> Result<()> {
    let failure_summary = crate::application::report_types::build_generation_failure_summary(
        generation_artifact.generation,
        &generation_artifact.phase,
        responses,
    );
    fs::write(
        raw_dir.join(format!(
            "generation_{:04}_failure_summary.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(&failure_summary)
            .context("failed to serialize generation failure summary")?,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn write_incremental_generation_artifacts(
    metadata: &RustJanusIncrementalArtifactMetadata<'_>,
    generation_artifact: &RustJanusGenerationArtifact,
    responses: &[patina_types::WorkerResponse],
    origin_metrics: &[RustJanusOriginMetricRow],
    generations_so_far: &[RustJanusGenerationArtifact],
    boundary_population: &[patina_search::ScottGaMember],
    working_population: &[patina_search::ScottGaMember],
) -> Result<()> {
    write_incremental_generation_artifacts_with_metadata(
        metadata.run_dir,
        &IncrementalGaCheckpointMetadata {
            workflow_id: "ga.persistent-daemon",
            workflow_owner: "persistent_daemon_ga",
            backend: "janus_mace",
            backend_mode: metadata.backend_mode_label,
            system: metadata.system,
            requested_generations: metadata.requested_generations,
            hashkey_config: metadata.hashkey_config,
        },
        metadata.search_cfg,
        metadata.operator_policy,
        generation_artifact,
        responses,
        origin_metrics,
        generations_so_far,
        boundary_population,
        working_population,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_incremental_generation_artifacts_with_metadata(
    run_dir: &Path,
    checkpoint_metadata: &IncrementalGaCheckpointMetadata<'_>,
    search_cfg: &patina_types::SearchConfig,
    operator_policy: &crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    generation_artifact: &RustJanusGenerationArtifact,
    responses: &[patina_types::WorkerResponse],
    origin_metrics: &[RustJanusOriginMetricRow],
    generations_so_far: &[RustJanusGenerationArtifact],
    boundary_population: &[patina_search::ScottGaMember],
    working_population: &[patina_search::ScottGaMember],
) -> Result<()> {
    let raw_dir = run_dir.join("raw");
    let checkpoint_dir = raw_dir.join("checkpoints");
    let population_dir = run_dir
        .join("outputs")
        .join("ga_population_snapshots")
        .join(format!("generation_{:04}", generation_artifact.generation));
    fs::create_dir_all(&raw_dir)?;
    fs::create_dir_all(&checkpoint_dir)?;
    fs::create_dir_all(&population_dir)?;

    let mut hashkey_computer = checkpoint_metadata
        .hashkey_config
        .map(crate::application::rust_janus_ga_checkpoint::GaHashkeyComputer::new)
        .transpose()?;
    let boundary_generation_state =
        crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
            generation_artifact.generation,
            boundary_population,
            hashkey_computer.as_mut(),
        )?;
    let checkpoint = crate::application::rust_janus_ga_checkpoint::build_checkpoint_with_hashkeys(
        crate::application::rust_janus_ga_checkpoint::RustGaCheckpointBuildRequest {
            context: crate::application::rust_janus_ga_checkpoint::RustGaCheckpointContext {
                workflow_id: checkpoint_metadata.workflow_id,
                workflow_owner: checkpoint_metadata.workflow_owner,
                backend: checkpoint_metadata.backend,
                backend_mode: checkpoint_metadata.backend_mode,
                system: checkpoint_metadata.system,
                requested_generations: checkpoint_metadata.requested_generations,
            },
            search_cfg,
            operator_policy,
            generations: generations_so_far,
            final_population: working_population,
            hashkeys: hashkey_computer.as_mut(),
        },
    )?;
    let working_generation_state = checkpoint.generation_state.clone();

    fs::write(
        raw_dir.join(format!(
            "generation_{:04}_responses.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(responses)?,
    )?;
    write_generation_failure_summary(&raw_dir, generation_artifact, responses)?;
    fs::write(
        raw_dir.join(format!(
            "generation_{:04}_summary.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(generation_artifact)?,
    )?;
    fs::write(
        raw_dir.join(format!(
            "generation_{:04}_origin_metrics.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(origin_metrics)?,
    )?;
    fs::write(
        raw_dir.join(format!(
            "generation_{:04}_state.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(&working_generation_state)?,
    )?;
    fs::write(
        raw_dir.join(format!(
            "generation_{:04}_boundary_state.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(&boundary_generation_state)?,
    )?;
    fs::write(
        checkpoint_dir.join(format!(
            "rust_ga_checkpoint_gen_{:04}.json",
            generation_artifact.generation
        )),
        serde_json::to_string_pretty(&checkpoint)?,
    )?;
    fs::write(
        raw_dir.join("rust_ga_checkpoint_latest.json"),
        serde_json::to_string_pretty(&checkpoint)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_state_latest.json"),
        serde_json::to_string_pretty(&working_generation_state)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_boundary_state_latest.json"),
        serde_json::to_string_pretty(&boundary_generation_state)?,
    )?;

    write_population_snapshot_xyz(working_population, &population_dir)?;

    Ok(())
}

fn copy_base_candidate_source(
    source_path: Option<&Path>,
    raw_dir: &Path,
) -> Result<Option<String>> {
    let Some(source_path) = source_path else {
        return Ok(None);
    };
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("input");
    let file_name = format!("base_candidate.{extension}");
    let destination = raw_dir.join(&file_name);
    fs::copy(source_path, &destination).with_context(|| {
        format!(
            "failed to copy base candidate `{}` into `{}`",
            source_path.display(),
            raw_dir.display()
        )
    })?;
    Ok(Some(format!("raw/{file_name}")))
}

#[allow(clippy::too_many_arguments)]
pub fn write_rust_janus_ga_artifacts(
    context: &RustJanusGaArtifactContext<'_>,
    generations: &[RustJanusGenerationArtifact],
    generation_responses: &[Vec<patina_types::WorkerResponse>],
    generation_origin_metrics: &[Vec<RustJanusOriginMetricRow>],
    generation_boundary_populations: &[Vec<patina_search::ScottGaMember>],
    controller_trace: &[patina_search::ScottGaGenerationTrace],
    final_population: &[patina_search::ScottGaMember],
    elapsed_secs: f64,
) -> Result<()> {
    let traces_dir = context.run_dir.join("traces");
    let raw_dir = context.run_dir.join("raw");
    let structures_dir = context.run_dir.join("outputs").join("structures");
    fs::create_dir_all(&traces_dir)?;
    fs::create_dir_all(&raw_dir)?;
    fs::create_dir_all(&structures_dir)?;

    let base_candidate_artifact =
        copy_base_candidate_source(context.base_candidate_source_path, &raw_dir)?;

    let total_responses = generation_responses
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let telemetry =
        crate::application::report_types::summarize_batch_responses(&total_responses, elapsed_secs);
    let origin_metrics = generation_origin_metrics
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let janus_guardrails = crate::application::janus_guardrails::build_janus_guardrail_report(
        context.backend.mode_label,
        &generations
            .iter()
            .map(
                |artifact| crate::application::janus_guardrails::JanusGuardrailGenerationInput {
                    generation: artifact.generation,
                    phase: artifact.phase.as_str(),
                    request_count: artifact.request_count,
                    converged_count: artifact.converged_count,
                    population_size: artifact.population_size,
                    duplicate_count: artifact.duplicate_count,
                    repopulated_count: artifact.repopulated_count,
                },
            )
            .collect::<Vec<_>>(),
        &origin_metrics
            .iter()
            .map(
                |row| crate::application::janus_guardrails::JanusOriginMetricInput {
                    generation: row.generation,
                    phase: row.phase.as_str(),
                    origin: row.origin.as_str(),
                    request_count: row.request_count,
                    converged_count: row.converged_count,
                    survivor_count: row.survivor_count,
                },
            )
            .collect::<Vec<_>>(),
    );
    let generation_completed = generations
        .iter()
        .map(|row| row.generation)
        .max()
        .unwrap_or(0);
    let mut hashkey_computer = context
        .hashkey_config
        .map(crate::application::rust_janus_ga_checkpoint::GaHashkeyComputer::new)
        .transpose()?;
    let generation_boundary_state =
        crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
            generation_completed,
            generation_boundary_populations
                .last()
                .map(Vec::as_slice)
                .unwrap_or(final_population),
            hashkey_computer.as_mut(),
        )?;
    let checkpoint = crate::application::rust_janus_ga_checkpoint::build_checkpoint_with_hashkeys(
        crate::application::rust_janus_ga_checkpoint::RustGaCheckpointBuildRequest {
            context: crate::application::rust_janus_ga_checkpoint::RustGaCheckpointContext {
                workflow_id: "ga.persistent-daemon",
                workflow_owner: "persistent_daemon_ga",
                backend: "janus_mace",
                backend_mode: context.backend.mode_label,
                system: context.system,
                requested_generations: context.requested_generations,
            },
            search_cfg: context.search_cfg,
            operator_policy: context.operator_policy,
            generations,
            final_population,
            hashkeys: hashkey_computer.as_mut(),
        },
    )?;
    let generation_state = checkpoint.generation_state.clone();
    let summary = RustJanusSearchSummary {
        workflow_id: "ga.persistent-daemon",
        workflow_owner: "persistent_daemon_ga",
        backend: "janus_mace",
        lane_mode: context.lane_mode.as_str().to_string(),
        duplicate_policy_mode: context.duplicate_policy_mode.as_str().to_string(),
        duplicate_filter_stack: context.duplicate_filter_stack.to_vec(),
        workers: context.workers,
        requested_generations: context.requested_generations,
        population_size: context.population_size,
        seed: context.search_cfg.seed.unwrap_or(1),
        system: context.system.to_string(),
        telemetry,
        generations: generations.to_vec(),
        origin_metrics: origin_metrics.clone(),
        janus_guardrails: janus_guardrails.clone(),
        operator_policy: context.operator_policy.clone(),
    };

    let provenance = serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
        workflow_id: "ga.persistent-daemon",
        workflow_owner: "persistent_daemon_ga",
        backend_id: Some("janus_mace"),
        backend_mode: Some(context.backend.mode_label),
        lane_mode: Some(context.lane_mode.as_str()),
        duplicate_policy_mode: Some(context.duplicate_policy_mode.as_str()),
        parallel_contract: Some("persistent_daemon_workers_only"),
        run_dir: Some(context.run_dir),
        workdir: Some(context.backend.worker_session_dir),
        source_run_dir: None,
        source_path: None,
        candidate_json: context.base_candidate_source_path,
        template_bundle: None,
    })?)?;
    let manifest = serde_json::json!({
        "run_name": context.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("unnamed-rust-janus-search"),
        "workflow_id": "ga.persistent-daemon",
        "workflow_owner": "persistent_daemon_ga",
        "workflow_scope": "Rust-owned GA controller with daemon-style persistent worker evaluation",
        "lane_mode": context.lane_mode.as_str(),
        "duplicate_policy_mode": context.duplicate_policy_mode.as_str(),
        "duplicate_filter_stack": context.duplicate_filter_stack,
        "enable_pmoi": context.enable_pmoi,
        "pmoi_tolerance": context.pmoi_tolerance,
        "native_scott_reference_preserved": true,
        "backend": "janus_mace",
        "parallel_contract": "persistent_daemon_workers_only",
        "system": context.system,
        "workers": summary.workers,
        "population_size": context.population_size,
        "requested_generations": context.requested_generations,
        "search_config": {
            "temperature": context.temperature,
            "step_size": context.step_size,
            "seed": context.search_cfg.seed,
        },
        "backend_metadata": {
            "python_bin": context.backend.python_bin,
            "adapter_script": context.backend.adapter_script,
            "arch": context.backend.arch,
            "model": context.backend.model,
            "device": context.backend.device,
            "dtype": context.backend.dtype,
            "mode": context.backend.mode_label,
            "optimizer": context.backend.optimizer_label,
            "fmax": context.backend.fmax,
            "steps": context.backend.steps,
            "worker_session_dir": context.backend.worker_session_dir,
        },
        "operator_policy": context.operator_policy,
        "provenance": provenance,
        "artifacts": {
            "generation_metrics": "traces/generation_metrics.csv",
            "controller_trace": "traces/controller_trace.csv",
            "origin_metrics": "traces/origin_metrics.csv",
            "janus_guardrails": "raw/janus_guardrails.json",
            "janus_guardrail_metrics": "traces/janus_guardrails.csv",
            "ga_generation_state": "raw/ga_generation_state.json",
            "ga_generation_state_latest": "raw/ga_generation_state_latest.json",
            "ga_generation_boundary_state": "raw/ga_generation_boundary_state.json",
            "ga_generation_boundary_state_latest": "raw/ga_generation_boundary_state_latest.json",
            "generation_state_per_generation": "raw/generation_XXXX_state.json",
            "generation_boundary_state_per_generation": "raw/generation_XXXX_boundary_state.json",
            "generation_summary_per_generation": "raw/generation_XXXX_summary.json",
            "generation_failure_summary_per_generation": "raw/generation_XXXX_failure_summary.json",
            "generation_origin_metrics_per_generation": "raw/generation_XXXX_origin_metrics.json",
            "generation_checkpoints": "raw/checkpoints/rust_ga_checkpoint_gen_XXXX.json",
            "latest_restart_checkpoint": "raw/rust_ga_checkpoint_latest.json",
            "rust_ga_checkpoint": "raw/rust_ga_checkpoint.json",
            "search_summary": "raw/search_summary.json",
            "generation_responses": "raw/generation_XXXX_responses.json",
            "structures": "outputs/structures/",
            "population_snapshots": "outputs/ga_population_snapshots/generation_XXXX/",
            "top_unique_candidates": "outputs/top_unique_candidates/",
            "base_candidate": base_candidate_artifact
        }
    });
    fs::write(
        context.run_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    fs::write(
        raw_dir.join("search_summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    fs::write(
        raw_dir.join("janus_guardrails.json"),
        serde_json::to_string_pretty(&janus_guardrails)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_state.json"),
        serde_json::to_string_pretty(&generation_state)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_boundary_state.json"),
        serde_json::to_string_pretty(&generation_boundary_state)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_boundary_state_latest.json"),
        serde_json::to_string_pretty(&generation_boundary_state)?,
    )?;
    fs::write(
        raw_dir.join("rust_ga_checkpoint.json"),
        serde_json::to_string_pretty(&checkpoint)?,
    )?;

    let metrics_path = traces_dir.join("generation_metrics.csv");
    let mut metrics = fs::File::create(&metrics_path)?;
    writeln!(
        metrics,
        "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,boundary_population_size,boundary_valid_population_size,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count"
    )?;
    for artifact in generations {
        writeln!(
            metrics,
            "{},{},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            artifact.generation,
            artifact.phase,
            artifact.elapsed_secs,
            artifact.request_count,
            artifact.success_count,
            artifact.failure_count,
            artifact.converged_count,
            optional_f64_csv(artifact.best_energy),
            optional_f64_csv(artifact.mean_energy),
            optional_f64_csv(artifact.worst_energy),
            artifact.boundary_population_size,
            artifact.boundary_valid_population_size,
            artifact.population_size,
            artifact.valid_population_size,
            artifact.duplicate_count,
            artifact.duplicate_hashkey_count,
            artifact.duplicate_pmoi_count,
            artifact.duplicate_energy_tol_count,
            artifact.repopulated_count
        )?;
    }

    let trace_path = traces_dir.join("controller_trace.csv");
    let mut trace_file = fs::File::create(&trace_path)?;
    writeln!(
        trace_file,
        "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices"
    )?;
    for trace in controller_trace {
        let selected = trace
            .selected_indices
            .iter()
            .map(|idx| idx.to_string())
            .collect::<Vec<_>>()
            .join("|");
        writeln!(
            trace_file,
            "{},{:?},{},{},{},{},{},{},{},{},{},{}",
            trace.generation,
            trace.stage,
            trace.population_size,
            trace.valid_population_size,
            trace.child_count,
            trace.duplicate_count,
            trace.duplicate_hashkey_count,
            trace.duplicate_pmoi_count,
            trace.duplicate_energy_tol_count,
            trace.repopulated_count,
            optional_f64_csv(trace.best_energy),
            selected
        )?;
    }

    let origin_metrics_path = traces_dir.join("origin_metrics.csv");
    let mut origin_metrics_file = fs::File::create(&origin_metrics_path)?;
    writeln!(
        origin_metrics_file,
        "generation,phase,origin,request_count,success_count,converged_count,boundary_survivor_count,working_survivor_count"
    )?;
    for row in &origin_metrics {
        writeln!(
            origin_metrics_file,
            "{},{},{},{},{},{},{},{}",
            row.generation,
            row.phase,
            row.origin,
            row.request_count,
            row.success_count,
            row.converged_count,
            row.survivor_count,
            row.working_survivor_count
        )?;
    }

    let guardrail_metrics_path = traces_dir.join("janus_guardrails.csv");
    let mut guardrail_metrics_file = fs::File::create(&guardrail_metrics_path)?;
    writeln!(
        guardrail_metrics_file,
        "generation,phase,duplicate_rate_vs_requests,repopulation_rate_vs_population,converged_rate,refill_request_share,refill_converged_share,refill_survivor_share,warnings"
    )?;
    for row in &janus_guardrails.generations {
        writeln!(
            guardrail_metrics_file,
            "{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{}",
            row.generation,
            row.phase,
            row.duplicate_rate_vs_requests,
            row.repopulation_rate_vs_population,
            row.converged_rate,
            row.refill_request_share,
            row.refill_converged_share,
            row.refill_survivor_share,
            row.warnings.join("|")
        )?;
    }

    for (generation, responses) in generation_responses.iter().enumerate() {
        fs::write(
            raw_dir.join(format!("generation_{generation:04}_responses.json")),
            serde_json::to_string_pretty(responses)?,
        )?;
        if let Some(artifact) = generations.get(generation) {
            write_generation_failure_summary(&raw_dir, artifact, responses)?;
        }
        if let Some(boundary_population) = generation_boundary_populations.get(generation) {
            let boundary_generation_state =
                crate::application::rust_janus_ga_checkpoint::build_generation_state(
                    generation,
                    boundary_population,
                );
            fs::write(
                raw_dir.join(format!("generation_{generation:04}_boundary_state.json")),
                serde_json::to_string_pretty(&boundary_generation_state)?,
            )?;
        }
    }

    for (rank, member) in final_population.iter().take(10).enumerate() {
        write_candidate_xyz_simple(
            &member.result.relaxed_candidate,
            &structures_dir.join(format!(
                "final_rank_{rank:02}_{}.xyz",
                member.origin.as_str()
            )),
            Some(member.result.energy),
        )?;
    }
    write_top_unique_candidates(
        final_population,
        &context
            .run_dir
            .join("outputs")
            .join("top_unique_candidates"),
    )?;

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct GenericGaArtifactManifest<'a> {
    pub run_name: &'a str,
    pub workflow_id: &'a str,
    pub workflow_owner: &'a str,
    pub workflow_scope: &'a str,
    pub backend: &'a str,
    pub parallel_contract: &'a str,
    pub system: &'a str,
    pub requested_generations: usize,
    pub population_size: usize,
    pub seed: Option<u64>,
    pub lane_mode: &'a str,
    pub duplicate_policy_mode: &'a str,
    pub search_config_temperature: f64,
    pub search_config_step_size: f64,
    pub provenance: serde_json::Value,
    pub extra: serde_json::Value,
}

#[allow(clippy::too_many_arguments)]
pub fn write_generic_ga_artifacts(
    run_dir: &Path,
    base_candidate_source_path: Option<&Path>,
    manifest: &GenericGaArtifactManifest<'_>,
    hashkey_config: Option<&crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>,
    operator_policy: &crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    generations: &[RustJanusGenerationArtifact],
    generation_responses: &[Vec<patina_types::WorkerResponse>],
    generation_origin_metrics: &[Vec<RustJanusOriginMetricRow>],
    generation_boundary_populations: &[Vec<patina_search::ScottGaMember>],
    generation_populations: &[Vec<patina_search::ScottGaMember>],
    controller_trace: &[patina_search::ScottGaGenerationTrace],
    final_population: &[patina_search::ScottGaMember],
    checkpoint: &crate::application::rust_janus_ga_checkpoint::RustGaCheckpoint,
    search_summary: &serde_json::Value,
) -> Result<()> {
    let traces_dir = run_dir.join("traces");
    let raw_dir = run_dir.join("raw");
    let structures_dir = run_dir.join("outputs").join("structures");
    fs::create_dir_all(&traces_dir)?;
    fs::create_dir_all(&raw_dir)?;
    fs::create_dir_all(&structures_dir)?;

    let base_candidate_artifact = copy_base_candidate_source(base_candidate_source_path, &raw_dir)?;

    let manifest_json = serde_json::json!({
        "run_name": manifest.run_name,
        "workflow_id": manifest.workflow_id,
        "workflow_owner": manifest.workflow_owner,
        "workflow_scope": manifest.workflow_scope,
        "lane_mode": manifest.lane_mode,
        "duplicate_policy_mode": manifest.duplicate_policy_mode,
        "backend": manifest.backend,
        "parallel_contract": manifest.parallel_contract,
        "system": manifest.system,
        "population_size": manifest.population_size,
        "requested_generations": manifest.requested_generations,
        "search_config": {
            "temperature": manifest.search_config_temperature,
            "step_size": manifest.search_config_step_size,
            "seed": manifest.seed,
        },
        "operator_policy": operator_policy,
        "provenance": manifest.provenance,
        "artifacts": {
            "generation_metrics": "traces/generation_metrics.csv",
            "controller_trace": "traces/controller_trace.csv",
            "origin_metrics": "traces/origin_metrics.csv",
            "ga_generation_state": "raw/ga_generation_state.json",
            "ga_generation_state_latest": "raw/ga_generation_state_latest.json",
            "ga_generation_boundary_state": "raw/ga_generation_boundary_state.json",
            "ga_generation_boundary_state_latest": "raw/ga_generation_boundary_state_latest.json",
            "generation_state_per_generation": "raw/generation_XXXX_state.json",
            "generation_boundary_state_per_generation": "raw/generation_XXXX_boundary_state.json",
            "generation_summary_per_generation": "raw/generation_XXXX_summary.json",
            "generation_failure_summary_per_generation": "raw/generation_XXXX_failure_summary.json",
            "generation_origin_metrics_per_generation": "raw/generation_XXXX_origin_metrics.json",
            "generation_checkpoints": "raw/checkpoints/rust_ga_checkpoint_gen_XXXX.json",
            "latest_restart_checkpoint": "raw/rust_ga_checkpoint_latest.json",
            "rust_ga_checkpoint": "raw/rust_ga_checkpoint.json",
            "search_summary": "raw/search_summary.json",
            "generation_responses": "raw/generation_XXXX_responses.json",
            "structures": "outputs/structures/",
            "population_snapshots": "outputs/ga_population_snapshots/generation_XXXX/",
            "top_unique_candidates": "outputs/top_unique_candidates/",
            "base_candidate": base_candidate_artifact
        },
        "extra": manifest.extra,
    });
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest_json)?,
    )?;
    fs::write(
        raw_dir.join("search_summary.json"),
        serde_json::to_string_pretty(search_summary)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_state.json"),
        serde_json::to_string_pretty(&checkpoint.generation_state)?,
    )?;
    let final_boundary_population = generation_boundary_populations
        .last()
        .map(Vec::as_slice)
        .unwrap_or(final_population);
    let mut hashkey_computer = hashkey_config
        .map(crate::application::rust_janus_ga_checkpoint::GaHashkeyComputer::new)
        .transpose()?;
    fs::write(
        raw_dir.join("ga_generation_boundary_state.json"),
        serde_json::to_string_pretty(
            &crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
                checkpoint.generation_completed,
                final_boundary_population,
                hashkey_computer.as_mut(),
            )?,
        )?,
    )?;
    fs::write(
        raw_dir.join("rust_ga_checkpoint.json"),
        serde_json::to_string_pretty(checkpoint)?,
    )?;

    let metrics_path = traces_dir.join("generation_metrics.csv");
    let mut metrics = fs::File::create(&metrics_path)?;
    writeln!(
        metrics,
        "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,boundary_population_size,boundary_valid_population_size,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count"
    )?;
    for artifact in generations {
        writeln!(
            metrics,
            "{},{},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            artifact.generation,
            artifact.phase,
            artifact.elapsed_secs,
            artifact.request_count,
            artifact.success_count,
            artifact.failure_count,
            artifact.converged_count,
            optional_f64_csv(artifact.best_energy),
            optional_f64_csv(artifact.mean_energy),
            optional_f64_csv(artifact.worst_energy),
            artifact.boundary_population_size,
            artifact.boundary_valid_population_size,
            artifact.population_size,
            artifact.valid_population_size,
            artifact.duplicate_count,
            artifact.duplicate_hashkey_count,
            artifact.duplicate_pmoi_count,
            artifact.duplicate_energy_tol_count,
            artifact.repopulated_count
        )?;
    }

    let trace_path = traces_dir.join("controller_trace.csv");
    let mut trace_file = fs::File::create(&trace_path)?;
    writeln!(
        trace_file,
        "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices"
    )?;
    for trace in controller_trace {
        let selected = trace
            .selected_indices
            .iter()
            .map(|idx| idx.to_string())
            .collect::<Vec<_>>()
            .join("|");
        writeln!(
            trace_file,
            "{},{:?},{},{},{},{},{},{},{},{},{},{}",
            trace.generation,
            trace.stage,
            trace.population_size,
            trace.valid_population_size,
            trace.child_count,
            trace.duplicate_count,
            trace.duplicate_hashkey_count,
            trace.duplicate_pmoi_count,
            trace.duplicate_energy_tol_count,
            trace.repopulated_count,
            optional_f64_csv(trace.best_energy),
            selected
        )?;
    }

    let origin_metrics_path = traces_dir.join("origin_metrics.csv");
    let mut origin_metrics_file = fs::File::create(&origin_metrics_path)?;
    writeln!(
        origin_metrics_file,
        "generation,phase,origin,request_count,success_count,converged_count,boundary_survivor_count,working_survivor_count"
    )?;
    for row in generation_origin_metrics.iter().flatten() {
        writeln!(
            origin_metrics_file,
            "{},{},{},{},{},{},{},{}",
            row.generation,
            row.phase,
            row.origin,
            row.request_count,
            row.success_count,
            row.converged_count,
            row.survivor_count,
            row.working_survivor_count
        )?;
    }

    let checkpoint_dir = raw_dir.join("checkpoints");
    fs::create_dir_all(&checkpoint_dir)?;
    for (generation_index, responses) in generation_responses.iter().enumerate() {
        fs::write(
            raw_dir.join(format!("generation_{generation_index:04}_responses.json")),
            serde_json::to_string_pretty(responses)?,
        )?;
        if let Some(artifact) = generations.get(generation_index) {
            write_generation_failure_summary(&raw_dir, artifact, responses)?;
            fs::write(
                raw_dir.join(format!("generation_{generation_index:04}_summary.json")),
                serde_json::to_string_pretty(artifact)?,
            )?;
        }
        if let Some(origin_metrics) = generation_origin_metrics.get(generation_index) {
            fs::write(
                raw_dir.join(format!(
                    "generation_{generation_index:04}_origin_metrics.json"
                )),
                serde_json::to_string_pretty(origin_metrics)?,
            )?;
        }
        if let Some(population) = generation_populations.get(generation_index) {
            let mut per_generation_hashkey_computer = hashkey_config
                .map(crate::application::rust_janus_ga_checkpoint::GaHashkeyComputer::new)
                .transpose()?;
            let generation_state =
                crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
                    generation_index,
                    population,
                    per_generation_hashkey_computer.as_mut(),
                )?;
            fs::write(
                raw_dir.join(format!("generation_{generation_index:04}_state.json")),
                serde_json::to_string_pretty(&generation_state)?,
            )?;
            if let Some(boundary_population) = generation_boundary_populations.get(generation_index)
            {
                let boundary_generation_state = crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
                        generation_index,
                        boundary_population,
                        per_generation_hashkey_computer.as_mut(),
                    )?;
                fs::write(
                    raw_dir.join(format!(
                        "generation_{generation_index:04}_boundary_state.json"
                    )),
                    serde_json::to_string_pretty(&boundary_generation_state)?,
                )?;
            }
            fs::write(
                checkpoint_dir.join(format!("rust_ga_checkpoint_gen_{generation_index:04}.json")),
                serde_json::to_string_pretty(
                    &crate::application::rust_janus_ga_checkpoint::build_checkpoint_with_hashkeys(
                        crate::application::rust_janus_ga_checkpoint::RustGaCheckpointBuildRequest {
                            context: crate::application::rust_janus_ga_checkpoint::RustGaCheckpointContext {
                                workflow_id: manifest.workflow_id,
                                workflow_owner: manifest.workflow_owner,
                                backend: manifest.backend,
                                backend_mode: &checkpoint.janus_mode,
                                system: manifest.system,
                                requested_generations: manifest.requested_generations,
                            },
                            search_cfg: &checkpoint.search_config,
                            operator_policy,
                            generations: &generations[..=generation_index
                                .min(generations.len().saturating_sub(1))],
                            final_population: population,
                            hashkeys: per_generation_hashkey_computer.as_mut(),
                        },
                    )?,
                )?,
            )?;

            let population_dir = run_dir
                .join("outputs")
                .join("ga_population_snapshots")
                .join(format!("generation_{generation_index:04}"));
            fs::create_dir_all(&population_dir)?;
            write_population_snapshot_xyz(population, &population_dir)?;
        }
    }

    fs::write(
        raw_dir.join("rust_ga_checkpoint_latest.json"),
        serde_json::to_string_pretty(checkpoint)?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_state_latest.json"),
        serde_json::to_string_pretty(
            &crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
                checkpoint.generation_completed,
                final_population,
                hashkey_computer.as_mut(),
            )?,
        )?,
    )?;
    fs::write(
        raw_dir.join("ga_generation_boundary_state_latest.json"),
        serde_json::to_string_pretty(
            &crate::application::rust_janus_ga_checkpoint::build_generation_state_with_hashkeys(
                checkpoint.generation_completed,
                final_boundary_population,
                hashkey_computer.as_mut(),
            )?,
        )?,
    )?;

    for (rank, member) in final_population.iter().take(10).enumerate() {
        write_candidate_xyz_simple(
            &member.result.relaxed_candidate,
            &structures_dir.join(format!(
                "final_rank_{rank:02}_{}.xyz",
                member.origin.as_str()
            )),
            Some(member.result.energy),
        )?;
    }
    write_top_unique_candidates(
        final_population,
        &run_dir.join("outputs").join("top_unique_candidates"),
    )?;

    Ok(())
}

fn write_population_snapshot_xyz(
    final_population: &[patina_search::ScottGaMember],
    population_dir: &Path,
) -> Result<()> {
    for (rank, member) in final_population.iter().take(20).enumerate() {
        write_candidate_xyz_simple(
            &member.result.relaxed_candidate,
            &population_dir.join(format!(
                "rank_{rank:02}_{}_occ{:02}.xyz",
                member.origin.as_str(),
                member.occurrences
            )),
            Some(member.result.energy),
        )?;
    }
    Ok(())
}

fn write_top_unique_candidates(
    final_population: &[patina_search::ScottGaMember],
    output_dir: &Path,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    let mut csv = fs::File::create(output_dir.join("top_unique_candidates.csv"))?;
    writeln!(
        csv,
        "unique_rank,population_rank,origin,occurrences,energy,converged,label"
    )?;

    let mut seen = std::collections::HashSet::new();
    let mut unique_rank = 0usize;
    for (population_rank, member) in final_population.iter().enumerate() {
        let key = serde_json::to_string(&patina_types::StructureRecord::from(
            &member.result.relaxed_candidate,
        ))?;
        if !seen.insert(key) {
            continue;
        }
        unique_rank += 1;
        writeln!(
            csv,
            "{},{},{},{},{:.12},{},{}",
            unique_rank,
            population_rank,
            member.origin.as_str(),
            member.occurrences,
            member.result.energy,
            member.result.converged,
            member.result.relaxed_candidate.label
        )?;
        write_candidate_xyz_simple(
            &member.result.relaxed_candidate,
            &output_dir.join(format!(
                "{:02}_{}.xyz",
                unique_rank,
                sanitize_path_label(&member.result.relaxed_candidate.label)
            )),
            Some(member.result.energy),
        )?;
    }

    Ok(())
}

fn optional_f64_csv(value: Option<f64>) -> String {
    value.map(|v| format!("{v:.12}")).unwrap_or_default()
}

fn sanitize_path_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

fn write_candidate_xyz_simple(
    candidate: &patina_types::Candidate,
    path: &Path,
    energy: Option<f64>,
) -> Result<()> {
    let comment = if energy.is_some() {
        None
    } else {
        Some("generated_by=persistent_daemon_ga".to_string())
    };
    write_candidate_xyz(
        candidate,
        path,
        &XyzEncodeOptions {
            comment,
            coordinate_mode: XyzCoordinateMode::Stored,
            extxyz_fields: false,
            label: None,
            energy,
        },
    )
    .map_err(|err| anyhow::anyhow!("failed to write xyz `{}`: {err:?}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        summarize_staged_scott_duplicate_trace,
        write_incremental_generation_artifacts_with_metadata, IncrementalGaCheckpointMetadata,
        RustJanusGaArtifactContext, RustJanusGaBackendArtifactMetadata,
        RustJanusGenerationArtifact, RustJanusOriginMetricRow, StagedScottGaArtifactMetadata,
        StagedScottGaArtifactSink,
    };
    use crate::application::janus_operator_policy::{
        select_janus_ga_operator_policy, GaOperatorBackendProfile,
    };
    use crate::application::ports::GaRunArtifactSink;
    use crate::application::rust_janus_ga::ScottGaDuplicateTraceRecord;
    use crate::application::scott_ga_runtime::{
        StagedScottProcedureTraceRecord, StagedScottProcedureTraceStatus,
    };
    use crate::application::workflow_policy::GaWorkflowPolicy;
    use camino::Utf8PathBuf;
    use indexmap::IndexMap;
    use patina_evaluator::{
        FinalStageFailurePolicy, ScottBackendMode, ScottEvaluatorPlan, ScottEvaluatorSettings,
        ScottLatticeMode, ScottProcedureIntent, ScottProcedurePlan, StageEngine, StageIndex,
        StagePlan, StageSelection,
    };
    use patina_runtime::ScottBackendRoutingPolicy;
    use patina_search::{
        PopulationKernelState, ScottGaGenerationTrace, ScottGaMember, ScottGaOrigin,
        ScottGaTopologyIdentity, WorkflowFamily, WorkflowLineage,
    };
    use tempfile::tempdir;

    #[cfg(unix)]
    fn write_fake_dreadnaut(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("fake_dreadnaut.sh");
        std::fs::write(&path, "#!/bin/sh\ncat >/dev/null\nprintf '[1 2]\\n'\n")
            .expect("write fake dreadnaut");
        let mut permissions = std::fs::metadata(&path)
            .expect("fake dreadnaut metadata")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod fake dreadnaut");
        path
    }

    fn sample_candidate(label: &str) -> patina_types::Candidate {
        patina_types::Candidate::cluster(
            label,
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
        )
    }

    fn sample_member(label: &str, energy: f64, origin: ScottGaOrigin) -> ScottGaMember {
        ScottGaMember {
            source_candidate: sample_candidate(label),
            result: patina_types::EvalResult {
                energy,
                forces: vec![[0.0, 0.0, 0.0]; 2],
                relaxed_candidate: sample_candidate(&format!("{label}_relaxed")),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin,
            occurrences: 1,
            lineage: WorkflowLineage::seed(label.to_string()).with_generation(0),
            topology: ScottGaTopologyIdentity::default(),
        }
    }

    fn sample_worker_response(
        label: &str,
        generation: usize,
        energy: f64,
    ) -> patina_types::WorkerResponse {
        patina_types::WorkerResponse {
            request_id: format!("request_{generation}_{label}"),
            generation: Some(generation),
            worker_slot: Some(0),
            outcome: patina_types::WorkerOutcome::Success {
                result: patina_types::EvalResult {
                    energy,
                    forces: vec![[0.0, 0.0, 0.0]; 2],
                    relaxed_candidate: sample_candidate(&format!("{label}_relaxed")),
                    converged: true,
                    wall_time: std::time::Duration::from_secs(0),
                },
            },
        }
    }

    fn sample_origin_metric(generation: usize) -> RustJanusOriginMetricRow {
        RustJanusOriginMetricRow {
            generation,
            phase: "initialize".into(),
            origin: "seed".into(),
            request_count: 1,
            success_count: 1,
            converged_count: 1,
            survivor_count: 1,
            working_survivor_count: 1,
        }
    }

    fn sample_generation_artifact(generation: usize) -> RustJanusGenerationArtifact {
        RustJanusGenerationArtifact {
            generation,
            phase: "initialize".into(),
            elapsed_secs: 0.1,
            request_count: 1,
            success_count: 1,
            failure_count: 0,
            failure_kind_counts: std::collections::BTreeMap::new(),
            converged_count: 1,
            best_energy: Some(-3.0),
            mean_energy: Some(-3.0),
            worst_energy: Some(-3.0),
            boundary_population_size: 1,
            boundary_valid_population_size: 1,
            population_size: 1,
            valid_population_size: 1,
            duplicate_count: 0,
            duplicate_hashkey_count: 0,
            duplicate_pmoi_count: 0,
            duplicate_energy_tol_count: 0,
            repopulated_count: 0,
        }
    }

    fn sample_kernel_state(member: &ScottGaMember, generation: usize) -> PopulationKernelState {
        let mut state = PopulationKernelState::new(WorkflowFamily::GeneticAlgorithm);
        state.begin_generation(generation);
        state.queue_candidate(member.source_candidate.clone(), member.lineage.clone());
        let task = state.next_task().expect("queued task");
        state.record_member(task, member.result.clone(), 1);
        state
    }

    fn sample_staged_procedure_plan() -> ScottProcedurePlan {
        ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template: Utf8PathBuf::from("/tmp/Master.gin"),
                atoms_in: None,
                work_root: Utf8PathBuf::from("/tmp"),
                settings: ScottEvaluatorSettings {
                    backend_mode: ScottBackendMode::Gulp,
                    procedure_intent: ScottProcedureIntent::GeneticAlgorithm,
                    lattice_mode: ScottLatticeMode::Cluster,
                    max_relaxation_attempts: 1,
                    gnorm_tolerance: 1.0e-4,
                    final_stage_failure_policy: FinalStageFailurePolicy::KeepPreviousAccepted,
                    retrieve_relaxed_geometry: true,
                    keep_stage_artifacts: true,
                },
            },
            stages: StageSelection {
                stages: vec![StagePlan {
                    stage: StageIndex(1),
                    engine: StageEngine::Gulp,
                    refine_if_energy_below: None,
                    energy_min_threshold: None,
                    energy_max_threshold: None,
                    keep_only_if_final_stage: false,
                }],
            },
        }
    }

    #[test]
    fn duplicate_trace_summary_separates_hashkey_and_fallback_paths() {
        let traces = vec![
            ScottGaDuplicateTraceRecord {
                left_source_label: "a".into(),
                right_source_label: "b".into(),
                left_relaxed_label: "a_relaxed".into(),
                right_relaxed_label: "b_relaxed".into(),
                left_energy: -10.0,
                right_energy: -9.9,
                left_hashkey: Some("hk-1".into()),
                right_hashkey: Some("hk-1".into()),
                exact_hashkey_match: Some(true),
                reason: patina_evaluator::ScottDuplicateReason::Hashkey,
            },
            ScottGaDuplicateTraceRecord {
                left_source_label: "c".into(),
                right_source_label: "d".into(),
                left_relaxed_label: "c_relaxed".into(),
                right_relaxed_label: "d_relaxed".into(),
                left_energy: -8.0,
                right_energy: -8.0,
                left_hashkey: Some("hk-2".into()),
                right_hashkey: Some("hk-3".into()),
                exact_hashkey_match: Some(false),
                reason: patina_evaluator::ScottDuplicateReason::Pmoi,
            },
            ScottGaDuplicateTraceRecord {
                left_source_label: "e".into(),
                right_source_label: "f".into(),
                left_relaxed_label: "e_relaxed".into(),
                right_relaxed_label: "f_relaxed".into(),
                left_energy: -7.0,
                right_energy: -7.0,
                left_hashkey: None,
                right_hashkey: None,
                exact_hashkey_match: None,
                reason: patina_evaluator::ScottDuplicateReason::EnergyTolerance,
            },
        ];

        let summary = summarize_staged_scott_duplicate_trace(&traces);

        assert_eq!(summary.by_reason.get("hashkey"), Some(&1));
        assert_eq!(summary.by_reason.get("pmoi"), Some(&1));
        assert_eq!(summary.by_reason.get("energy_tolerance"), Some(&1));
        assert_eq!(summary.exact_hashkey_duplicate_count, 1);
        assert_eq!(summary.fallback_duplicate_count, 2);
        assert_eq!(summary.exact_hashkey_nonmatch_count, 1);
        assert_eq!(summary.exact_hashkey_unknown_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn incremental_generation_artifacts_persist_hashkeys_in_generation_states() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        let scratch_dir = dir.path().join("hashkey_scratch");
        let dreadnaut_path = write_fake_dreadnaut(dir.path());
        let hashkey_config =
            crate::application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig {
                radius_mode: "IR".into(),
                radius_const: 0.4,
                dreadnaut_path,
                scratch_dir,
            };
        let member = ScottGaMember {
            source_candidate: sample_candidate("source"),
            result: patina_types::EvalResult {
                energy: -3.0,
                forces: vec![[0.0, 0.0, 0.0]; 2],
                relaxed_candidate: sample_candidate("relaxed"),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin: ScottGaOrigin::Mutate,
            occurrences: 1,
            lineage: WorkflowLineage::seed("source").with_generation(0),
            topology: ScottGaTopologyIdentity::default(),
        };
        let generation_artifact = RustJanusGenerationArtifact {
            generation: 0,
            phase: "initialize".into(),
            elapsed_secs: 0.1,
            request_count: 1,
            success_count: 1,
            failure_count: 0,
            failure_kind_counts: std::collections::BTreeMap::new(),
            converged_count: 1,
            best_energy: Some(-3.0),
            mean_energy: Some(-3.0),
            worst_energy: Some(-3.0),
            boundary_population_size: 1,
            boundary_valid_population_size: 1,
            population_size: 1,
            valid_population_size: 1,
            duplicate_count: 0,
            duplicate_hashkey_count: 0,
            duplicate_pmoi_count: 0,
            duplicate_energy_tol_count: 0,
            repopulated_count: 0,
        };
        let search_cfg = patina_types::SearchConfig {
            temperature: 10.0,
            step_size: 0.1,
            population_size: 1,
            max_steps: 1,
            seed: Some(7),
        };
        let operator_policy =
            crate::application::janus_operator_policy::select_janus_ga_operator_policy(
                crate::application::janus_operator_policy::GaOperatorBackendProfile::Gulp,
                crate::DriverJanusMode::LocalOpt,
                0.1,
            );

        write_incremental_generation_artifacts_with_metadata(
            &run_dir,
            &IncrementalGaCheckpointMetadata {
                workflow_id: "ga.scott-monolithic",
                workflow_owner: "scott_monolithic_ga",
                backend: "scott_runtime",
                backend_mode: "gulp",
                system: "(MgO)1",
                requested_generations: 1,
                hashkey_config: Some(&hashkey_config),
            },
            &search_cfg,
            &operator_policy,
            &generation_artifact,
            &[],
            &[],
            std::slice::from_ref(&generation_artifact),
            std::slice::from_ref(&member),
            std::slice::from_ref(&member),
        )
        .expect("write incremental artifacts");

        let generation_state: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(run_dir.join("raw/generation_0000_state.json"))
                .expect("read generation state"),
        )
        .expect("parse generation state");
        let checkpoint: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                run_dir.join("raw/checkpoints/rust_ga_checkpoint_gen_0000.json"),
            )
            .expect("read generation checkpoint"),
        )
        .expect("parse generation checkpoint");

        assert!(
            generation_state["population"][0]["topology"]["canonical_hashkey"]
                .as_str()
                .is_some()
        );
        assert!(
            checkpoint["generation_state"]["population"][0]["topology"]["canonical_hashkey"]
                .as_str()
                .is_some()
        );
    }

    #[test]
    fn staged_scott_manifest_records_workflow_id_and_provenance() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        let workdir = dir.path().join("work");
        let search_cfg = patina_types::SearchConfig {
            temperature: 10.0,
            step_size: 0.1,
            population_size: 1,
            max_steps: 1,
            seed: Some(17),
        };
        let member = sample_member("seed", -3.0, ScottGaOrigin::Seed);
        let artifact = sample_generation_artifact(0);
        let execution = crate::application::ga_execution::RustJanusGaExecution {
            generation_artifacts: vec![artifact.clone()],
            generation_responses: vec![vec![sample_worker_response("seed", 0, -3.0)]],
            generation_origin_metrics: vec![vec![sample_origin_metric(0)]],
            generation_boundary_populations: vec![vec![member.clone()]],
            generation_populations: vec![vec![member.clone()]],
            generation_boundary_kernel_states: vec![sample_kernel_state(&member, 0)],
            generation_kernel_states: vec![sample_kernel_state(&member, 0)],
            controller_trace: Vec::<ScottGaGenerationTrace>::new(),
            final_population: vec![member.clone()],
        };
        let operator_policy = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::Gulp,
            crate::DriverJanusMode::LocalOpt,
            search_cfg.step_size,
        );
        let sink = StagedScottGaArtifactSink {
            metadata: &StagedScottGaArtifactMetadata {
                run_dir: run_dir.clone(),
                workdir: workdir.clone(),
                system: "demo-staged-ga".into(),
                ga_generations: 1,
                population: 1,
                temperature: 10.0,
                step_size: 0.1,
                runtime_default_backend: "gulp".into(),
                runtime_stage_backend: vec!["1=gulp".into()],
                run_job_template: Some(dir.path().join("run.job")),
                master_gin_template: Some(dir.path().join("Master.gin")),
                atoms_in_template: Some(dir.path().join("atoms.in")),
            },
            base_candidate_source_path: None,
            search_cfg: &search_cfg,
            operator_policy: &operator_policy,
            hashkey_config: None,
            lane_mode: crate::application::rust_janus_ga::RustJanusLaneMode::StandaloneCapable,
            duplicate_policy_mode:
                crate::application::rust_janus_ga::RustJanusDuplicatePolicyMode::ExternalNativeHashkey,
            duplicate_filter_stack: &[],
            duplicate_traces: &[],
            routing_policy: &ScottBackendRoutingPolicy {
                default_backend: ScottBackendMode::Gulp,
                stage_overrides: IndexMap::new(),
            },
            procedure_plan: &sample_staged_procedure_plan(),
            procedure_traces: &[StagedScottProcedureTraceRecord {
                request_id: "request_0_seed".into(),
                generation: Some(0),
                request_workdir: workdir.display().to_string(),
                input_label: "seed".into(),
                status: StagedScottProcedureTraceStatus::Accepted,
                procedure_state_digest: None,
                failure_message: None,
            }],
            emulate_gate_summary: None,
            emulate_gate_decision_traces: None,
            emulate_gate_batch_traces: None,
            workflow_policy: GaWorkflowPolicy::scott_staged_runtime(),
        };

        let summary = GaRunArtifactSink::persist_run(&sink, &execution).expect("persist run");
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(run_dir.join("manifest.json")).expect("read manifest"),
        )
        .expect("manifest json");

        assert_eq!(summary.workflow_id, "ga.scott-monolithic");
        assert_eq!(manifest["workflow_id"], "ga.scott-monolithic");
        assert_eq!(manifest["provenance"]["workflow_id"], "ga.scott-monolithic");
        assert_eq!(manifest["provenance"]["backend_id"], "scott_runtime");
        assert_eq!(
            manifest["provenance"]["template_bundle"]["run_job"],
            run_dir
                .parent()
                .unwrap()
                .join("run.job")
                .display()
                .to_string()
        );
    }

    #[test]
    fn janus_persistent_manifest_records_workflow_id_and_provenance() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        let worker_session_dir = dir.path().join("worker_session");
        let search_cfg = patina_types::SearchConfig {
            temperature: 10.0,
            step_size: 0.1,
            population_size: 1,
            max_steps: 1,
            seed: Some(23),
        };
        let member = sample_member("seed", -3.0, ScottGaOrigin::Seed);
        let artifact = sample_generation_artifact(0);
        let operator_policy = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::Gulp,
            crate::DriverJanusMode::LocalOpt,
            search_cfg.step_size,
        );

        super::write_rust_janus_ga_artifacts(
            &RustJanusGaArtifactContext {
                run_dir: &run_dir,
                system: "demo-janus-ga",
                base_candidate_source_path: None,
                search_cfg: &search_cfg,
                workers: 1,
                requested_generations: 1,
                population_size: 1,
                temperature: 10.0,
                step_size: 0.1,
                enable_pmoi: true,
                pmoi_tolerance: 0.02,
                lane_mode: crate::application::rust_janus_ga::RustJanusLaneMode::StandaloneCapable,
                duplicate_policy_mode:
                    crate::application::rust_janus_ga::RustJanusDuplicatePolicyMode::ExternalNativeHashkey,
                duplicate_filter_stack: &[],
                operator_policy: &operator_policy,
                hashkey_config: None,
                backend: RustJanusGaBackendArtifactMetadata {
                    python_bin: std::path::Path::new("/usr/bin/python3"),
                    adapter_script: std::path::Path::new("/tmp/janus_adapter.py"),
                    arch: "mace_mp",
                    model: "small",
                    device: "cpu",
                    dtype: "float64",
                    mode_label: "local-opt",
                    optimizer_label: "abc-fire",
                    fmax: 0.1,
                    steps: 100,
                    worker_session_dir: &worker_session_dir,
                },
            },
            std::slice::from_ref(&artifact),
            &[vec![sample_worker_response("seed", 0, -3.0)]],
            &[vec![sample_origin_metric(0)]],
            &[vec![member.clone()]],
            &[],
            &[member],
            0.1,
        )
        .expect("write artifacts");

        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(run_dir.join("manifest.json")).expect("read manifest"),
        )
        .expect("manifest json");

        assert_eq!(manifest["workflow_id"], "ga.persistent-daemon");
        assert_eq!(
            manifest["provenance"]["workflow_id"],
            "ga.persistent-daemon"
        );
        assert_eq!(manifest["provenance"]["backend_id"], "janus_mace");
        assert_eq!(
            manifest["provenance"]["parallel_contract"],
            "persistent_daemon_workers_only"
        );
    }
}
