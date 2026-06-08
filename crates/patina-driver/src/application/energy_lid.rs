use anyhow::{anyhow, Context, Result};
use patina_search::{
    annealing_final_temperature_after_steps, annealing_quench_seed_lineage,
    annealing_quench_step_lineage, annealing_sampling_schedule, annealing_seed_lineage,
    annealing_step_lineage, build_energy_lid_window_state,
    build_simulated_annealing_structure_state, energy_lid_runner_seed_lineage,
    energy_lid_runner_step_lineage, energy_lid_sampling_schedule, energy_lid_seed_lineage,
    energy_lid_step_lineage, energy_lid_threshold_for_lid, EnergyLid, McStepTrace, MonteCarlo,
    MonteCarloAcceptance, MonteCarloKernelState, SamplingSchedule, SearchAlgorithm,
    SimulatedAnnealing, WorkflowFamily,
};
use patina_types::{
    Candidate, EnergyLidWindowState, Population, SearchConfig, SimulatedAnnealingStructureState,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::ports::{
    SamplingArtifactSink, SamplingEvaluationIntent, SamplingEvaluationPort,
    SamplingEvaluationRequest, SamplingStartPort, SamplingWorkflowIntent,
};
use super::workflow_tasks::queue_single_sampling_task;

#[derive(Debug, Clone, Serialize)]
pub struct EnergyLidSourceStructure {
    pub label: String,
    pub origin: String,
    pub first_gen: usize,
    pub last_gen: usize,
    pub occurrences: usize,
    pub starting_energy: f64,
    pub declared_dimensionality: String,
    pub periodic_axes: [bool; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct EnergyLidWindowSummary {
    pub lid_index: usize,
    pub threshold: f64,
    pub accepted_steps: usize,
    pub runner_count: usize,
    pub active_basin: String,
    pub runner_best_energy: Option<f64>,
    pub best_energy: Option<f64>,
    pub final_energy: Option<f64>,
    pub transitions: Vec<EnergyLidBasinTransition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnergyLidStructureSummary {
    pub source: EnergyLidSourceStructure,
    pub source_basin: String,
    pub final_energy: f64,
    pub windows: Vec<EnergyLidWindowSummary>,
    pub trace_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnergyLidBasinSummary {
    pub basin_label: String,
    pub representative_label: String,
    pub representative_energy: f64,
    pub discovery: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnergyLidBasinTransition {
    pub threshold: f64,
    pub from_basin: String,
    pub to_basin: String,
    pub runner_index: usize,
    pub relaxed_label: String,
    pub relaxed_path: String,
    pub relaxed_energy: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnergyLidRunSummary {
    pub system: String,
    pub backend: crate::EvalBackendKind,
    pub top_n: usize,
    pub zero_d_source_count: usize,
    pub periodic_3d_source_count: usize,
    pub lid_levels: usize,
    pub lid_increment: f64,
    pub steps_per_lid: usize,
    pub quench_steps: usize,
    pub runners_per_lid: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub basins: Vec<EnergyLidBasinSummary>,
    pub structures: Vec<EnergyLidStructureSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulatedAnnealingStructureSummary {
    pub source: EnergyLidSourceStructure,
    pub accepted_steps: usize,
    pub best_energy: f64,
    pub final_energy: f64,
    pub final_temperature: f64,
    pub trace_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulatedAnnealingRunSummary {
    pub system: String,
    pub backend: crate::EvalBackendKind,
    pub top_n: usize,
    pub zero_d_source_count: usize,
    pub periodic_3d_source_count: usize,
    pub anneal_steps: usize,
    pub initial_temperature: f64,
    pub temperature_scale: f64,
    pub hold_steps: usize,
    pub quench_steps: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub structures: Vec<SimulatedAnnealingStructureSummary>,
}

#[derive(Debug, Clone)]
pub struct EnergyLidStart {
    pub candidate: Candidate,
    pub source: EnergyLidSourceStructure,
}

#[derive(Debug, Clone)]
struct BasinRecord {
    basin_label: String,
    representative_label: String,
    representative_energy: f64,
    discovery: String,
}

#[derive(Debug, Clone)]
pub struct EnergyLidWorkflowRequest {
    pub source_run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub backend_kind: crate::EvalBackendKind,
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
}

#[derive(Debug, Clone)]
pub struct SimulatedAnnealingWorkflowRequest {
    pub source_run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: String,
    pub backend_kind: crate::EvalBackendKind,
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
}

#[derive(Debug, Clone)]
pub struct EnergyLidWindowExecution {
    pub summary: EnergyLidWindowSummary,
    pub state: EnergyLidWindowState,
}

#[derive(Debug, Clone)]
pub struct EnergyLidStructureExecution {
    pub summary: EnergyLidStructureSummary,
    pub windows: Vec<EnergyLidWindowExecution>,
    pub combined_trace: Vec<McStepTrace>,
}

#[derive(Debug, Clone)]
pub struct EnergyLidWorkflowExecution {
    pub summary: EnergyLidRunSummary,
    pub structures: Vec<EnergyLidStructureExecution>,
}

#[derive(Debug, Clone)]
pub struct SimulatedAnnealingStructureExecution {
    pub summary: SimulatedAnnealingStructureSummary,
    pub state: SimulatedAnnealingStructureState,
    pub trace: Vec<McStepTrace>,
}

#[derive(Debug, Clone)]
pub struct SimulatedAnnealingWorkflowExecution {
    pub summary: SimulatedAnnealingRunSummary,
    pub structures: Vec<SimulatedAnnealingStructureExecution>,
}

pub struct SamplingWorkflowService;

impl SamplingWorkflowService {
    pub fn execute_energy_lid(
        &self,
        request: &EnergyLidWorkflowRequest,
        start_port: &dyn SamplingStartPort,
        evaluation_port: &dyn SamplingEvaluationPort,
        artifact_sink: &dyn SamplingArtifactSink,
    ) -> Result<EnergyLidWorkflowExecution> {
        let starts = start_port.load_starts(&request.source_run_dir, request.top_n)?;
        if starts.is_empty() {
            return Err(anyhow!(
                "no converged structures were recoverable from `{}`",
                request.source_run_dir.display()
            ));
        }

        let execution = execute_energy_lid_workflow(&starts, request, evaluation_port)?;
        artifact_sink.persist_energy_lid_run(&execution)?;
        Ok(execution)
    }

    pub fn execute_simulated_annealing(
        &self,
        request: &SimulatedAnnealingWorkflowRequest,
        start_port: &dyn SamplingStartPort,
        evaluation_port: &dyn SamplingEvaluationPort,
        artifact_sink: &dyn SamplingArtifactSink,
    ) -> Result<SimulatedAnnealingWorkflowExecution> {
        let starts = start_port.load_starts(&request.source_run_dir, request.top_n)?;
        if starts.is_empty() {
            return Err(anyhow!(
                "no converged native-compatible structures were recoverable from `{}`",
                request.source_run_dir.display()
            ));
        }

        let execution = execute_simulated_annealing_workflow(&starts, request, evaluation_port)?;
        artifact_sink.persist_simulated_annealing_run(&execution)?;
        Ok(execution)
    }
}

pub fn load_top_structures_from_ga_run(
    run_dir: &Path,
    top_n: usize,
) -> Result<Vec<EnergyLidStart>> {
    let typed_starts = load_top_structures_from_generation_state(run_dir, top_n)?;
    if !typed_starts.is_empty() {
        return Ok(typed_starts);
    }

    load_top_structures_from_legacy_scott_export(run_dir, top_n)
}

fn load_top_structures_from_generation_state(
    run_dir: &Path,
    top_n: usize,
) -> Result<Vec<EnergyLidStart>> {
    let raw_dir = run_dir.join("raw");
    let mut unique: BTreeMap<String, EnergyLidStart> = BTreeMap::new();
    let mut paths = fs::read_dir(&raw_dir)
        .with_context(|| format!("failed to read `{}`", raw_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("generation_") && name.ends_with("_state.json"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let state: patina_types::GaGenerationState = serde_json::from_str(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read `{}`", path.display()))?,
        )
        .with_context(|| format!("failed to parse `{}`", path.display()))?;
        let generation = state.generation;
        for member in state.population.iter().chain(state.elites.iter()) {
            let mut candidate = Candidate::from(&member.evaluation.structure);
            if candidate.species.is_empty() || candidate.fractional_coords.is_empty() {
                candidate = Candidate::from(&member.source);
            }
            let label = member.source.label.clone();
            let energy = member.evaluation.energy;
            if candidate.species.is_empty() || candidate.fractional_coords.is_empty() {
                continue;
            }
            if !candidate.supports_native_scott_search() {
                continue;
            }
            let entry = unique.entry(label.clone()).or_insert(EnergyLidStart {
                candidate: candidate.clone(),
                source: EnergyLidSourceStructure {
                    label: label.clone(),
                    origin: member.origin.clone(),
                    first_gen: generation,
                    last_gen: generation,
                    occurrences: 0,
                    starting_energy: energy,
                    declared_dimensionality: dimensionality_label(&candidate),
                    periodic_axes: candidate.periodic_axes,
                },
            });
            if energy < entry.source.starting_energy {
                entry.candidate = candidate.clone();
                entry.source.starting_energy = energy;
            }
            entry.source.first_gen = entry.source.first_gen.min(generation);
            entry.source.last_gen = entry.source.last_gen.max(generation);
            entry.source.occurrences += member.occurrences;
        }
    }

    let mut starts = unique.into_values().collect::<Vec<_>>();
    starts.sort_by(|left, right| {
        left.source
            .starting_energy
            .partial_cmp(&right.source.starting_energy)
            .unwrap_or(std::cmp::Ordering::Greater)
    });
    starts.truncate(top_n);
    Ok(starts)
}

fn load_top_structures_from_legacy_scott_export(
    run_dir: &Path,
    top_n: usize,
) -> Result<Vec<EnergyLidStart>> {
    let raw_dir = run_dir.join("raw");
    let structures_dir = run_dir.join("outputs").join("structures");
    let structure_index = super::scott_topology_export::index_structure_snapshots(&structures_dir)?;
    if structure_index.is_empty() {
        return Ok(Vec::new());
    }

    let rows = load_legacy_scott_top_structure_rows(&raw_dir)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut starts = Vec::new();
    for row in rows {
        let Some(path) = structure_index.get(&row.label) else {
            continue;
        };
        let candidate = super::scott_topology_export::parse_candidate_snapshot(path)?;
        if candidate.species.is_empty() || candidate.fractional_coords.is_empty() {
            continue;
        }
        if !candidate.supports_native_scott_search() {
            continue;
        }
        starts.push(EnergyLidStart {
            candidate: candidate.clone(),
            source: EnergyLidSourceStructure {
                label: row.label,
                origin: "legacy_scott_top_structure".to_string(),
                first_gen: 0,
                last_gen: 0,
                occurrences: 1,
                starting_energy: row.energy,
                declared_dimensionality: dimensionality_label(&candidate),
                periodic_axes: candidate.periodic_axes,
            },
        });
    }

    starts.sort_by(|left, right| {
        left.source
            .starting_energy
            .partial_cmp(&right.source.starting_energy)
            .unwrap_or(std::cmp::Ordering::Greater)
    });
    starts.truncate(top_n);
    Ok(starts)
}

#[derive(Debug, Clone)]
struct LegacyScottTopStructureRow {
    label: String,
    energy: f64,
}

fn load_legacy_scott_top_structure_rows(raw_dir: &Path) -> Result<Vec<LegacyScottTopStructureRow>> {
    let statistics_path = raw_dir.join("top_structures_statistics");
    if statistics_path.exists() {
        return parse_legacy_scott_top_structure_statistics(&statistics_path);
    }

    let energies_path = raw_dir.join("top_structures_energies");
    if energies_path.exists() {
        return parse_legacy_scott_top_structure_energies(&energies_path);
    }

    Ok(Vec::new())
}

fn parse_legacy_scott_top_structure_statistics(
    path: &Path,
) -> Result<Vec<LegacyScottTopStructureRow>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if idx == 0 || line.trim().is_empty() {
            continue;
        }
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        if parts[1].eq_ignore_ascii_case("x") {
            continue;
        }
        let Some(energy) = parts[2].parse::<f64>().ok() else {
            continue;
        };
        rows.push(LegacyScottTopStructureRow {
            label: parts[1].to_string(),
            energy,
        });
    }
    Ok(rows)
}

fn parse_legacy_scott_top_structure_energies(
    path: &Path,
) -> Result<Vec<LegacyScottTopStructureRow>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in content.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 {
            continue;
        }
        let Some(energy) = parts.last().and_then(|value| value.parse::<f64>().ok()) else {
            continue;
        };
        rows.push(LegacyScottTopStructureRow {
            label: parts[0].to_string(),
            energy,
        });
    }
    Ok(rows)
}

fn execute_energy_lid_workflow(
    starts: &[EnergyLidStart],
    request: &EnergyLidWorkflowRequest,
    evaluation_port: &dyn SamplingEvaluationPort,
) -> Result<EnergyLidWorkflowExecution> {
    let mut summaries = Vec::with_capacity(starts.len());
    let mut structure_executions = Vec::with_capacity(starts.len());
    let mut basins = Vec::<BasinRecord>::new();
    for start in starts {
        basins.push(BasinRecord {
            basin_label: start.source.label.clone(),
            representative_label: start.source.label.clone(),
            representative_energy: start.source.starting_energy,
            discovery: "source".to_string(),
        });
    }

    for (start_index, start) in starts.iter().enumerate() {
        let structure_workdir = request
            .workdir
            .join(format!("structure_{start_index:02}_{}", start.source.label));
        let initial_relaxed = evaluation_port
            .evaluate(&SamplingEvaluationRequest {
                candidate: start.candidate.clone(),
                eval_dir: structure_workdir.join("initial"),
                workflow: SamplingWorkflowIntent::EnergyLid,
                intent: SamplingEvaluationIntent::InitialState,
                step_index: None,
                lid_index: None,
                runner_index: None,
            })
            .map_err(|error| {
                anyhow!(
                    "failed to relax initial energy-lid start `{}`: {error}",
                    start.source.label
                )
            })?;
        let mut current_result = initial_relaxed.clone();
        let mut current_candidate = initial_relaxed.relaxed_candidate.clone();
        let source_basin = start.source.label.clone();
        let mut active_basin = source_basin.clone();
        let mut windows = Vec::with_capacity(request.lid_levels);
        let mut window_executions = Vec::with_capacity(request.lid_levels);
        let mut combined_trace = Vec::new();

        for lid_index in 0..request.lid_levels {
            let threshold = energy_lid_threshold_for_lid(
                initial_relaxed.energy,
                request.lid_increment,
                lid_index,
            );
            let mut controller = EnergyLid::new(
                current_candidate.clone(),
                request.seed + start_index as u64 + lid_index as u64,
                threshold,
            );
            let cfg = SearchConfig {
                temperature: 0.0,
                step_size: request.step_size,
                population_size: 1,
                max_steps: request.steps_per_lid,
                seed: Some(request.seed + start_index as u64 + lid_index as u64),
            };
            let _ = controller.initialize(&cfg);
            let mut population = Population {
                members: vec![(current_result.clone(), current_candidate.clone())],
                generation: 0,
            };
            let mut lid_kernel_state = MonteCarloKernelState::new(
                WorkflowFamily::EnergyLid,
                energy_lid_sampling_schedule(
                    threshold,
                    request.lid_increment,
                    request.runners_per_lid,
                ),
            );
            lid_kernel_state.seed_current(
                current_candidate.clone(),
                current_result.clone(),
                energy_lid_seed_lineage(&start.source.label),
            );
            let trace_start = controller.trace().len();
            let mut accepted_steps = 0usize;
            for step_index in 0..request.steps_per_lid {
                let candidates = controller.next_candidates(&population)?;
                let Some(mut candidate) = candidates.into_iter().next() else {
                    break;
                };
                if request.enforce_container {
                    apply_cluster_container(&mut candidate, request.boundary);
                }
                let lineage = energy_lid_step_lineage(&start.source.label, step_index);
                let task = queue_single_sampling_task(
                    &mut lid_kernel_state,
                    candidate.clone(),
                    lineage,
                    "energy-lid sampling step",
                )?;
                let eval = evaluation_port.evaluate(&SamplingEvaluationRequest {
                    candidate: candidate.clone(),
                    eval_dir: structure_workdir
                        .join(format!("lid_{lid_index:03}"))
                        .join(format!("step_{step_index:03}")),
                    workflow: SamplingWorkflowIntent::EnergyLid,
                    intent: SamplingEvaluationIntent::SamplingStep,
                    step_index: Some(step_index),
                    lid_index: Some(lid_index),
                    runner_index: None,
                });
                let result = match eval {
                    Ok(result) => result,
                    Err(error) => {
                        lid_kernel_state
                            .record_rejection(&task, format!("evaluation failed: {error}"));
                        continue;
                    }
                };
                let update = controller.update(&mut population, &[(candidate, result.clone())])?;
                accepted_steps += update.accepted;
                if update.accepted > 0 {
                    lid_kernel_state.record_accept(task, result);
                } else {
                    lid_kernel_state
                        .record_rejection(&task, "rejected by energy-lid acceptance policy");
                }
            }
            let trace_slice = &controller.trace()[trace_start..];
            combined_trace.extend_from_slice(trace_slice);
            let (best_result, best_candidate) = population
                .best()
                .cloned()
                .ok_or_else(|| anyhow!("energy-lid population became empty"))?;
            let window_active_basin = active_basin.clone();
            let mut runner_best_energy = None;
            let mut transitions = Vec::<EnergyLidBasinTransition>::new();
            let mut runner_kernel_states = Vec::with_capacity(request.runners_per_lid);

            for runner_index in 0..request.runners_per_lid {
                let runner_seed = request.seed
                    + start_index as u64
                    + lid_index as u64
                    + (runner_index as u64 + 1) * 10_000;
                let mut runner = MonteCarlo::new(
                    best_candidate.clone(),
                    runner_seed,
                    MonteCarloAcceptance::Quench,
                );
                let runner_cfg = SearchConfig {
                    temperature: 0.0,
                    step_size: request.step_size,
                    population_size: 1,
                    max_steps: request.quench_steps,
                    seed: Some(runner_seed),
                };
                let _ = runner.initialize(&runner_cfg);
                let mut runner_population = Population {
                    members: vec![(best_result.clone(), best_candidate.clone())],
                    generation: 0,
                };
                let mut runner_kernel_state =
                    MonteCarloKernelState::new(WorkflowFamily::EnergyLid, SamplingSchedule::Quench);
                runner_kernel_state.seed_current(
                    best_candidate.clone(),
                    best_result.clone(),
                    energy_lid_runner_seed_lineage(&start.source.label, lid_index),
                );
                let runner_trace_start = runner.trace().len();
                for quench_step in 0..request.quench_steps {
                    let candidates = runner.next_candidates(&runner_population)?;
                    let Some(mut candidate) = candidates.into_iter().next() else {
                        break;
                    };
                    if request.enforce_container {
                        apply_cluster_container(&mut candidate, request.boundary);
                    }
                    let lineage = energy_lid_runner_step_lineage(&start.source.label, quench_step);
                    let task = queue_single_sampling_task(
                        &mut runner_kernel_state,
                        candidate.clone(),
                        lineage,
                        "energy-lid runner quench step",
                    )?;
                    let eval = evaluation_port.evaluate(&SamplingEvaluationRequest {
                        candidate: candidate.clone(),
                        eval_dir: structure_workdir
                            .join(format!("lid_{lid_index:03}"))
                            .join(format!("runner_{runner_index:03}"))
                            .join(format!("quench_{quench_step:03}")),
                        workflow: SamplingWorkflowIntent::EnergyLid,
                        intent: SamplingEvaluationIntent::QuenchStep,
                        step_index: Some(quench_step),
                        lid_index: Some(lid_index),
                        runner_index: Some(runner_index),
                    });
                    let result = match eval {
                        Ok(result) => result,
                        Err(error) => {
                            runner_kernel_state
                                .record_rejection(&task, format!("evaluation failed: {error}"));
                            continue;
                        }
                    };
                    let update =
                        runner.update(&mut runner_population, &[(candidate, result.clone())])?;
                    if update.accepted > 0 {
                        runner_kernel_state.record_accept(task, result);
                    } else {
                        runner_kernel_state
                            .record_rejection(&task, "rejected by quench acceptance policy");
                    }
                }
                combined_trace.extend_from_slice(&runner.trace()[runner_trace_start..]);
                let Some((_, runner_best_candidate)) = runner_population.best().cloned() else {
                    runner_kernel_states.push(runner_kernel_state);
                    continue;
                };
                let relaxed_result = match evaluation_port.evaluate(&SamplingEvaluationRequest {
                    candidate: runner_best_candidate,
                    eval_dir: structure_workdir
                        .join(format!("lid_{lid_index:03}"))
                        .join(format!("runner_{runner_index:03}"))
                        .join("relaxed"),
                    workflow: SamplingWorkflowIntent::EnergyLid,
                    intent: SamplingEvaluationIntent::Relaxation,
                    step_index: None,
                    lid_index: Some(lid_index),
                    runner_index: Some(runner_index),
                }) {
                    Ok(result) => result,
                    Err(_) => {
                        runner_kernel_states.push(runner_kernel_state);
                        continue;
                    }
                };
                let source_energy = start.source.starting_energy;
                if (relaxed_result.energy - source_energy).abs() > 1.0e-6 {
                    let runner_basin = format!(
                        "{}__lid_{:03}_runner_{:03}",
                        start.source.label, lid_index, runner_index
                    );
                    basins.push(BasinRecord {
                        basin_label: runner_basin.clone(),
                        representative_label: relaxed_result.relaxed_candidate.label.clone(),
                        representative_energy: relaxed_result.energy,
                        discovery: "runner_relaxed".to_string(),
                    });
                    transitions.push(EnergyLidBasinTransition {
                        threshold,
                        from_basin: window_active_basin.clone(),
                        to_basin: runner_basin,
                        runner_index,
                        relaxed_label: relaxed_result.relaxed_candidate.label.clone(),
                        relaxed_path: energy_lid_runner_relaxed_path(
                            start_index,
                            &start.source.label,
                            lid_index,
                            runner_index,
                        ),
                        relaxed_energy: relaxed_result.energy,
                    });
                }
                runner_best_energy = Some(
                    runner_best_energy
                        .map(|value: f64| value.min(relaxed_result.energy))
                        .unwrap_or(relaxed_result.energy),
                );
                runner_kernel_states.push(runner_kernel_state);
            }

            current_result = best_result.clone();
            current_candidate = best_candidate.clone();
            let window_summary = EnergyLidWindowSummary {
                lid_index,
                threshold,
                accepted_steps,
                runner_count: request.runners_per_lid,
                active_basin: window_active_basin.clone(),
                runner_best_energy,
                best_energy: Some(best_result.energy),
                final_energy: Some(current_result.energy),
                transitions,
            };
            windows.push(window_summary.clone());
            window_executions.push(EnergyLidWindowExecution {
                summary: window_summary,
                state: build_energy_lid_window_state(
                    lid_index,
                    threshold,
                    window_active_basin.clone(),
                    &lid_kernel_state,
                    &runner_kernel_states,
                ),
            });
            active_basin = window_active_basin;
        }

        let structure_summary = EnergyLidStructureSummary {
            source: start.source.clone(),
            source_basin,
            final_energy: current_result.energy,
            windows: windows.clone(),
            trace_path: energy_lid_trace_path(start_index, &start.source.label),
        };
        summaries.push(structure_summary.clone());
        structure_executions.push(EnergyLidStructureExecution {
            summary: structure_summary,
            windows: window_executions,
            combined_trace,
        });
    }

    Ok(EnergyLidWorkflowExecution {
        summary: EnergyLidRunSummary {
            system: request.system.clone(),
            backend: request.backend_kind,
            top_n: starts.len(),
            zero_d_source_count: starts
                .iter()
                .filter(|start| start.candidate.is_zero_d())
                .count(),
            periodic_3d_source_count: starts
                .iter()
                .filter(|start| start.candidate.is_three_d_periodic())
                .count(),
            lid_levels: request.lid_levels,
            lid_increment: request.lid_increment,
            steps_per_lid: request.steps_per_lid,
            quench_steps: request.quench_steps,
            runners_per_lid: request.runners_per_lid,
            step_size: request.step_size,
            enforce_container: request.enforce_container,
            boundary: request.boundary,
            basins: basins
                .iter()
                .map(|basin| EnergyLidBasinSummary {
                    basin_label: basin.basin_label.clone(),
                    representative_label: basin.representative_label.clone(),
                    representative_energy: basin.representative_energy,
                    discovery: basin.discovery.clone(),
                })
                .collect(),
            structures: summaries,
        },
        structures: structure_executions,
    })
}

fn execute_simulated_annealing_workflow(
    starts: &[EnergyLidStart],
    request: &SimulatedAnnealingWorkflowRequest,
    evaluation_port: &dyn SamplingEvaluationPort,
) -> Result<SimulatedAnnealingWorkflowExecution> {
    let mut summaries = Vec::with_capacity(starts.len());
    let mut structure_executions = Vec::with_capacity(starts.len());
    for (start_index, start) in starts.iter().enumerate() {
        let structure_workdir = request
            .workdir
            .join(format!("structure_{start_index:02}_{}", start.source.label));

        let mut controller = SimulatedAnnealing::new(
            start.candidate.clone(),
            request.seed + start_index as u64,
            request.initial_temperature,
            request.temperature_scale,
        )
        .with_hold_steps(request.hold_steps);
        let cfg = SearchConfig {
            temperature: request.initial_temperature,
            step_size: request.step_size,
            population_size: 1,
            max_steps: request.anneal_steps,
            seed: Some(request.seed + start_index as u64),
        };
        let _ = controller.initialize(&cfg);
        let initial_result = evaluation_port.evaluate(&SamplingEvaluationRequest {
            candidate: start.candidate.clone(),
            eval_dir: structure_workdir.join("initial"),
            workflow: SamplingWorkflowIntent::SimulatedAnnealing,
            intent: SamplingEvaluationIntent::InitialState,
            step_index: None,
            lid_index: None,
            runner_index: None,
        })?;
        let mut population = Population {
            members: vec![(initial_result.clone(), start.candidate.clone())],
            generation: 0,
        };
        let mut anneal_kernel_state = MonteCarloKernelState::new(
            WorkflowFamily::SimulatedAnnealing,
            annealing_sampling_schedule(
                request.initial_temperature,
                request.temperature_scale,
                request.hold_steps,
            ),
        );
        anneal_kernel_state.seed_current(
            start.candidate.clone(),
            initial_result,
            annealing_seed_lineage(&start.source.label),
        );
        let mut accepted_steps = 0usize;
        for step_index in 0..request.anneal_steps {
            let candidates = controller.next_candidates(&population)?;
            let Some(mut candidate) = candidates.into_iter().next() else {
                break;
            };
            if request.enforce_container {
                apply_cluster_container(&mut candidate, request.boundary);
            }
            let lineage = annealing_step_lineage(&start.source.label, step_index);
            let task = queue_single_sampling_task(
                &mut anneal_kernel_state,
                candidate.clone(),
                lineage,
                "simulated-annealing sampling step",
            )?;
            let eval = evaluation_port.evaluate(&SamplingEvaluationRequest {
                candidate: candidate.clone(),
                eval_dir: structure_workdir.join(format!("anneal_step_{step_index:03}")),
                workflow: SamplingWorkflowIntent::SimulatedAnnealing,
                intent: SamplingEvaluationIntent::SamplingStep,
                step_index: Some(step_index),
                lid_index: None,
                runner_index: None,
            });
            let result = match eval {
                Ok(result) => result,
                Err(error) => {
                    anneal_kernel_state
                        .record_rejection(&task, format!("evaluation failed: {error}"));
                    continue;
                }
            };
            let update = controller.update(&mut population, &[(candidate, result.clone())])?;
            accepted_steps += update.accepted;
            if update.accepted > 0 {
                anneal_kernel_state.record_accept(task, result);
            } else {
                anneal_kernel_state
                    .record_rejection(&task, "rejected by annealing acceptance policy");
            }
        }

        let mut quench_kernel_state = None;
        if request.quench_steps > 0 {
            let Some((best_result, best_candidate)) = population.best().cloned() else {
                continue;
            };
            let runner_seed = request.seed + start_index as u64 + 100_000;
            let mut runner = MonteCarlo::new(
                best_candidate.clone(),
                runner_seed,
                MonteCarloAcceptance::Quench,
            );
            let runner_cfg = SearchConfig {
                temperature: 0.0,
                step_size: request.step_size,
                population_size: 1,
                max_steps: request.quench_steps,
                seed: Some(runner_seed),
            };
            let _ = runner.initialize(&runner_cfg);
            let mut runner_population = Population {
                members: vec![(best_result.clone(), best_candidate.clone())],
                generation: 0,
            };
            let mut kernel = MonteCarloKernelState::new(
                WorkflowFamily::SimulatedAnnealing,
                SamplingSchedule::Quench,
            );
            kernel.seed_current(
                best_candidate,
                best_result,
                annealing_quench_seed_lineage(&start.source.label, request.anneal_steps),
            );
            for quench_step in 0..request.quench_steps {
                let candidates = runner.next_candidates(&runner_population)?;
                let Some(mut candidate) = candidates.into_iter().next() else {
                    break;
                };
                if request.enforce_container {
                    apply_cluster_container(&mut candidate, request.boundary);
                }
                let lineage = annealing_quench_step_lineage(
                    &start.source.label,
                    request.anneal_steps,
                    quench_step,
                );
                let task = queue_single_sampling_task(
                    &mut kernel,
                    candidate.clone(),
                    lineage,
                    "simulated-annealing quench step",
                )?;
                let eval = evaluation_port.evaluate(&SamplingEvaluationRequest {
                    candidate: candidate.clone(),
                    eval_dir: structure_workdir.join(format!("quench_step_{quench_step:03}")),
                    workflow: SamplingWorkflowIntent::SimulatedAnnealing,
                    intent: SamplingEvaluationIntent::QuenchStep,
                    step_index: Some(request.anneal_steps + quench_step),
                    lid_index: None,
                    runner_index: Some(0),
                });
                let result = match eval {
                    Ok(result) => result,
                    Err(error) => {
                        kernel.record_rejection(&task, format!("evaluation failed: {error}"));
                        continue;
                    }
                };
                let update =
                    runner.update(&mut runner_population, &[(candidate, result.clone())])?;
                if update.accepted > 0 {
                    kernel.record_accept(task, result);
                } else {
                    kernel.record_rejection(&task, "rejected by quench acceptance policy");
                }
            }
            population = runner_population;
            quench_kernel_state = Some(kernel);
        }

        let (best_result, _) = population
            .best()
            .cloned()
            .ok_or_else(|| anyhow!("simulated annealing population became empty"))?;
        let structure_summary = SimulatedAnnealingStructureSummary {
            source: start.source.clone(),
            accepted_steps,
            best_energy: best_result.energy,
            final_energy: best_result.energy,
            final_temperature: annealing_final_temperature_after_steps(
                request.initial_temperature,
                request.temperature_scale,
                request.hold_steps,
                request.anneal_steps,
            ),
            trace_path: simulated_annealing_trace_path(start_index, &start.source.label),
        };
        summaries.push(structure_summary.clone());
        let structure_state = build_simulated_annealing_structure_state(
            &anneal_kernel_state,
            quench_kernel_state.as_ref(),
        );
        structure_executions.push(SimulatedAnnealingStructureExecution {
            summary: structure_summary,
            state: structure_state,
            trace: controller.trace().to_vec(),
        });
    }

    Ok(SimulatedAnnealingWorkflowExecution {
        summary: SimulatedAnnealingRunSummary {
            system: request.system.clone(),
            backend: request.backend_kind,
            top_n: starts.len(),
            zero_d_source_count: starts
                .iter()
                .filter(|start| start.candidate.is_zero_d())
                .count(),
            periodic_3d_source_count: starts
                .iter()
                .filter(|start| start.candidate.is_three_d_periodic())
                .count(),
            anneal_steps: request.anneal_steps,
            initial_temperature: request.initial_temperature,
            temperature_scale: request.temperature_scale,
            hold_steps: request.hold_steps,
            quench_steps: request.quench_steps,
            step_size: request.step_size,
            enforce_container: request.enforce_container,
            boundary: request.boundary,
            structures: summaries,
        },
        structures: structure_executions,
    })
}

fn dimensionality_label(candidate: &Candidate) -> String {
    match candidate.declared_dimensionality() {
        patina_types::StructureDimensionality::ZeroD => "zero_d",
        patina_types::StructureDimensionality::OneD => "one_d",
        patina_types::StructureDimensionality::TwoD => "two_d",
        patina_types::StructureDimensionality::ThreeD => "three_d",
    }
    .to_string()
}

fn energy_lid_trace_path(start_index: usize, label: &str) -> String {
    format!("traces/energy_lid/structure_{start_index:02}_{label}/traces/mc_trace.csv")
}

fn simulated_annealing_trace_path(start_index: usize, label: &str) -> String {
    format!("traces/simulated_annealing/structure_{start_index:02}_{label}/traces/mc_trace.csv")
}

fn energy_lid_runner_relaxed_path(
    start_index: usize,
    label: &str,
    lid_index: usize,
    runner_index: usize,
) -> String {
    format!(
        "work/structure_{start_index:02}_{label}/lid_{lid_index:03}/runner_{runner_index:03}/relaxed"
    )
}

fn apply_cluster_container(candidate: &mut Candidate, boundary: Option<f64>) {
    if !candidate.is_zero_d() {
        return;
    }
    let Some(limit) = boundary.filter(|value| value.is_finite() && *value > 0.0) else {
        return;
    };
    for coord in &mut candidate.fractional_coords {
        coord[0] = coord[0].clamp(-limit, limit);
        coord[1] = coord[1].clamp(-limit, limit);
        coord[2] = coord[2].clamp(-limit, limit);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        execute_simulated_annealing_workflow, load_top_structures_from_ga_run,
        EnergyLidSourceStructure, EnergyLidStart, SimulatedAnnealingWorkflowRequest,
    };
    use crate::application::ports::{SamplingEvaluationPort, SamplingEvaluationRequest};
    use patina_types::{Candidate, EvalResult, EvaluationRecord, GaGenerationState, GaMemberState};
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;

    #[derive(Debug, Clone, Copy)]
    struct IdentitySamplingPort;

    impl SamplingEvaluationPort for IdentitySamplingPort {
        fn evaluate(&self, request: &SamplingEvaluationRequest) -> anyhow::Result<EvalResult> {
            let candidate = &request.candidate;
            let energy = candidate
                .fractional_coords
                .iter()
                .flat_map(|coord| coord.iter())
                .copied()
                .sum::<f64>();
            let _intent = (request.workflow, request.intent);
            let scale = 1.0;
            Ok(EvalResult {
                energy: energy * scale,
                forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
                relaxed_candidate: candidate.clone(),
                converged: true,
                wall_time: Duration::from_secs(0),
            })
        }
    }

    fn cluster_candidate(label: &str) -> Candidate {
        Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.1, 0.2, 0.3]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: label.into(),
        }
    }

    fn periodic_3d_candidate(label: &str) -> Candidate {
        Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.25, 0.25, 0.25]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]]),
            periodic_axes: [true, true, true],
            label: label.into(),
        }
    }

    fn partial_periodic_candidate(label: &str) -> Candidate {
        Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.25, 0.25, 0.25]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]]),
            periodic_axes: [true, true, false],
            label: label.into(),
        }
    }

    fn write_xyz_snapshot(path: &std::path::Path, candidate: &Candidate) {
        let comment = match candidate.lattice {
            Some(lattice) => format!(
                "Lattice=\"{} {} {} {} {} {} {} {} {}\" pbc=\"{} {} {}\"",
                lattice[0][0],
                lattice[0][1],
                lattice[0][2],
                lattice[1][0],
                lattice[1][1],
                lattice[1][2],
                lattice[2][0],
                lattice[2][1],
                lattice[2][2],
                if candidate.periodic_axes[0] { "T" } else { "F" },
                if candidate.periodic_axes[1] { "T" } else { "F" },
                if candidate.periodic_axes[2] { "T" } else { "F" },
            ),
            None => String::new(),
        };
        let body = if let Some(lattice) = candidate.lattice {
            candidate
                .species
                .iter()
                .zip(&candidate.fractional_coords)
                .map(|(species, coord)| {
                    let cart = patina_search::fractional_to_cartesian(lattice, *coord);
                    format!("{species} {} {} {}", cart[0], cart[1], cart[2])
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            candidate
                .species
                .iter()
                .zip(&candidate.fractional_coords)
                .map(|(species, coord)| format!("{species} {} {} {}", coord[0], coord[1], coord[2]))
                .collect::<Vec<_>>()
                .join("\n")
        };
        fs::write(
            path,
            format!("{}\n{}\n{}\n", candidate.len(), comment, body),
        )
        .expect("write xyz snapshot");
    }

    fn member_state(
        candidate: &Candidate,
        energy: f64,
        origin: &str,
        occurrences: usize,
    ) -> GaMemberState {
        member_state_with_relaxed(candidate, candidate, energy, origin, occurrences)
    }

    fn member_state_with_relaxed(
        source: &Candidate,
        relaxed: &Candidate,
        energy: f64,
        origin: &str,
        occurrences: usize,
    ) -> GaMemberState {
        GaMemberState {
            member_id: 0,
            origin: origin.to_string(),
            occurrences,
            source: patina_types::StructureRecord::from(source),
            evaluation: EvaluationRecord {
                label: relaxed.label.clone(),
                energy,
                converged: true,
                structure: patina_types::StructureRecord::from(relaxed),
                backend_run_dir: None,
                primary_output_path: None,
            },
            lineage: None,
            topology: patina_types::GaMemberTopologyRecord::default(),
        }
    }

    #[test]
    fn load_top_structures_filters_partial_periodic_and_keeps_zero_d_and_3d() {
        let dir = tempdir().expect("tempdir");
        let raw_dir = dir.path().join("raw");
        fs::create_dir_all(&raw_dir).expect("raw dir");

        let cluster = cluster_candidate("cluster");
        let periodic = periodic_3d_candidate("periodic");
        let partial = partial_periodic_candidate("partial");
        let state = GaGenerationState {
            generation: 3,
            population: vec![
                member_state(&cluster, -10.0, "seed", 1),
                member_state(&periodic, -9.0, "seed", 1),
                member_state(&partial, -8.0, "seed", 1),
            ],
            elites: Vec::new(),
            repopulation: Vec::new(),
        };
        fs::write(
            raw_dir.join("generation_0003_state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");

        let starts = load_top_structures_from_ga_run(dir.path(), 8).expect("load starts");

        assert_eq!(starts.len(), 2);
        assert!(starts.iter().any(|start| start.source.label == "cluster"));
        assert!(starts.iter().any(|start| start.source.label == "periodic"));
        assert!(!starts.iter().any(|start| start.source.label == "partial"));
        let periodic_start = starts
            .iter()
            .find(|start| start.source.label == "periodic")
            .expect("periodic start");
        assert_eq!(periodic_start.source.declared_dimensionality, "three_d");
        assert_eq!(periodic_start.source.periodic_axes, [true, true, true]);
    }

    #[test]
    fn load_top_structures_uses_relaxed_geometry_from_generation_state() {
        let dir = tempdir().expect("tempdir");
        let raw_dir = dir.path().join("raw");
        fs::create_dir_all(&raw_dir).expect("raw dir");

        let source = cluster_candidate("source_cluster");
        let mut relaxed = cluster_candidate("relaxed_cluster");
        relaxed.fractional_coords[0] = [0.9, 0.8, 0.7];

        let state = GaGenerationState {
            generation: 2,
            population: vec![member_state_with_relaxed(
                &source, &relaxed, -11.0, "mutate", 1,
            )],
            elites: Vec::new(),
            repopulation: Vec::new(),
        };
        fs::write(
            raw_dir.join("generation_0002_state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");

        let starts = load_top_structures_from_ga_run(dir.path(), 4).expect("load starts");

        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].source.label, "source_cluster");
        assert_eq!(starts[0].candidate.label, "relaxed_cluster");
        assert_eq!(starts[0].candidate.fractional_coords[0], [0.9, 0.8, 0.7]);
    }

    #[test]
    fn load_top_structures_falls_back_to_legacy_scott_export() {
        let dir = tempdir().expect("tempdir");
        let raw_dir = dir.path().join("raw");
        let structures_dir = dir.path().join("outputs").join("structures");
        fs::create_dir_all(&raw_dir).expect("raw dir");
        fs::create_dir_all(&structures_dir).expect("structures dir");

        let cluster = cluster_candidate("cluster");
        let periodic = periodic_3d_candidate("periodic");
        let partial = partial_periodic_candidate("partial");
        write_xyz_snapshot(&structures_dir.join("cluster.xyz"), &cluster);
        write_xyz_snapshot(&structures_dir.join("periodic.xyz"), &periodic);
        write_xyz_snapshot(&structures_dir.join("partial.xyz"), &partial);
        fs::write(
            raw_dir.join("top_structures_statistics"),
            "rank label energy\n1 cluster -10.0\n2 periodic -9.0\n3 partial -8.0\n",
        )
        .expect("write legacy statistics");

        let starts = load_top_structures_from_ga_run(dir.path(), 8).expect("load starts");

        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0].source.label, "cluster");
        assert_eq!(starts[0].source.origin, "legacy_scott_top_structure");
        assert!(starts.iter().any(|start| start.source.label == "periodic"));
        assert!(!starts.iter().any(|start| start.source.label == "partial"));
        let periodic_start = starts
            .iter()
            .find(|start| start.source.label == "periodic")
            .expect("periodic start");
        assert_eq!(periodic_start.source.declared_dimensionality, "three_d");
        assert_eq!(periodic_start.source.periodic_axes, [true, true, true]);
        assert_eq!(periodic_start.candidate.periodic_axes, [true, true, true]);
    }

    #[test]
    fn simulated_annealing_summary_reports_zero_d_and_periodic_3d_sources() {
        let dir = tempdir().expect("tempdir");
        let workdir = dir.path().join("work");

        let starts = vec![
            EnergyLidStart {
                candidate: cluster_candidate("cluster"),
                source: EnergyLidSourceStructure {
                    label: "cluster".into(),
                    origin: "seed".into(),
                    first_gen: 0,
                    last_gen: 0,
                    occurrences: 1,
                    starting_energy: -10.0,
                    declared_dimensionality: "zero_d".into(),
                    periodic_axes: [false, false, false],
                },
            },
            EnergyLidStart {
                candidate: periodic_3d_candidate("periodic"),
                source: EnergyLidSourceStructure {
                    label: "periodic".into(),
                    origin: "seed".into(),
                    first_gen: 0,
                    last_gen: 0,
                    occurrences: 1,
                    starting_energy: -9.0,
                    declared_dimensionality: "three_d".into(),
                    periodic_axes: [true, true, true],
                },
            },
        ];

        let execution = execute_simulated_annealing_workflow(
            &starts,
            &SimulatedAnnealingWorkflowRequest {
                source_run_dir: dir.path().join("ga"),
                workdir: workdir.clone(),
                system: "MgO".into(),
                backend_kind: crate::EvalBackendKind::Gulp,
                top_n: 2,
                anneal_steps: 3,
                initial_temperature: 10.0,
                temperature_scale: 0.5,
                hold_steps: 1,
                quench_steps: 0,
                step_size: 0.1,
                enforce_container: false,
                boundary: None,
                seed: 17,
            },
            &IdentitySamplingPort,
        )
        .expect("execute simulated annealing");
        let summary = execution.summary;

        assert_eq!(summary.top_n, 2);
        assert_eq!(summary.zero_d_source_count, 1);
        assert_eq!(summary.periodic_3d_source_count, 1);
        assert_eq!(summary.structures.len(), 2);
        assert_eq!(execution.structures.len(), 2);
        for structure in &summary.structures {
            assert!(
                structure
                    .trace_path
                    .starts_with("traces/simulated_annealing/structure_"),
                "unexpected trace path {}",
                structure.trace_path
            );
        }
    }
}
