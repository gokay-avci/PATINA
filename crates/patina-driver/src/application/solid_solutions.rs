use anyhow::{anyhow, Result};
use patina_search::{accept_energy_transition, MonteCarloAcceptance};
use patina_types::{
    Candidate, EvalResult, EvaluationRecord, SolidSolutionDuplicateRecord,
    SolidSolutionDuplicateSource, SolidSolutionRunState, SolidSolutionStepDecision,
    SolidSolutionStepState, StructureRecord,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::ports::{
    SamplingEvaluationIntent, SamplingEvaluationPort, SamplingEvaluationRequest,
    SamplingWorkflowIntent, SolidSolutionsAcceptanceMode, SolidSolutionsArtifactSink,
    SolidSolutionsGeometryPort, SolidSolutionsIdentityPort, SolidSolutionsLibraryPort,
    SolidSolutionsLibraryRequest, SolidSolutionsMoveMode, SolidSolutionsMovePort,
    SolidSolutionsMoveRequest,
};
use super::topology_identity::{TopologyIdentityMatch, TopologyIdentityStore};

#[derive(Debug, Clone)]
pub struct SolidSolutionsWorkflowRequest {
    pub initial_candidate: Candidate,
    pub proposals: Vec<Candidate>,
    pub workdir: PathBuf,
    pub steps: usize,
    pub temperature: f64,
    pub move_mode: SolidSolutionsMoveMode,
    pub acceptance_mode: SolidSolutionsAcceptanceMode,
    pub max_exchanges: usize,
    pub seed: u64,
    pub skip_evaluation: bool,
    pub imported_hashkeys: Vec<String>,
}

impl SolidSolutionsWorkflowRequest {
    pub fn validate(&self) -> Result<()> {
        validate_supported_candidate(&self.initial_candidate, "solid-solution initial candidate")?;
        if self.workdir.as_os_str().is_empty() {
            return Err(anyhow!("solid-solution workdir must not be empty"));
        }
        if self.proposals.is_empty() && self.steps == 0 {
            return Err(anyhow!(
                "solid-solution workflow requires either explicit proposals or a positive step count"
            ));
        }
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err(anyhow!(
                "solid-solution temperature must be finite and non-negative"
            ));
        }
        if self.max_exchanges == 0 {
            return Err(anyhow!("solid-solution max_exchanges must be positive"));
        }
        for (index, proposal) in self.proposals.iter().enumerate() {
            validate_supported_candidate(
                proposal,
                &format!("solid-solution proposal at step {index}"),
            )?;
        }
        if self
            .imported_hashkeys
            .iter()
            .any(|hashkey| hashkey.trim().is_empty())
        {
            return Err(anyhow!(
                "solid-solution imported hashkeys must not contain empty entries"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SolidSolutionsRunSummary {
    pub proposal_count: usize,
    pub imported_hashkey_count: usize,
    pub skip_evaluation: bool,
    pub accepted_steps: usize,
    pub rejected_acceptance_steps: usize,
    pub rejected_geometry_steps: usize,
    pub rejected_duplicate_imported_steps: usize,
    pub rejected_duplicate_current_run_steps: usize,
    pub evaluation_failed_steps: usize,
    pub skipped_evaluation_steps: usize,
    pub initial_energy: Option<f64>,
    pub best_energy: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SolidSolutionsWorkflowExecution {
    pub summary: SolidSolutionsRunSummary,
    pub initial_source: StructureRecord,
    pub initial_evaluation: Option<EvaluationRecord>,
    pub state: SolidSolutionRunState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SolidSolutionStatisticsRow {
    pub cluster_no: usize,
    pub cluster_id: String,
    pub energy: f64,
    pub hashkey: Option<String>,
    pub decision: SolidSolutionStepDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SolidSolutionHashkeyStatisticsRow {
    pub source_type: String,
    pub hashkey: String,
    pub occurrences: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SolidSolutionsRdfConfig {
    pub cutoff: f64,
    pub step: f64,
    pub steps: usize,
    pub sigma: f64,
    pub zero: f64,
    pub accuracy: f64,
    pub histogram: bool,
}

impl SolidSolutionsRdfConfig {
    pub fn with_values(
        cutoff: f64,
        step: Option<f64>,
        steps: usize,
        sigma: f64,
        zero: f64,
        accuracy: f64,
        histogram: bool,
    ) -> Result<Self> {
        let config = Self {
            cutoff,
            step: step.unwrap_or_else(|| rdf_default_step(zero, cutoff, steps)),
            steps,
            sigma,
            zero,
            accuracy,
            histogram,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.histogram {
            return Err(anyhow!(
                "solid-solution RDF currently supports native default histogram output only"
            ));
        }
        if self.steps < 2 {
            return Err(anyhow!(
                "solid-solution RDF requires at least two output steps"
            ));
        }
        if !self.cutoff.is_finite() || self.cutoff <= 0.0 {
            return Err(anyhow!(
                "solid-solution RDF cutoff must be finite and positive"
            ));
        }
        if !self.zero.is_finite() || self.zero < 0.0 {
            return Err(anyhow!(
                "solid-solution RDF zero offset must be finite and non-negative"
            ));
        }
        if self.cutoff <= self.zero {
            return Err(anyhow!(
                "solid-solution RDF cutoff must be greater than the zero offset"
            ));
        }
        if !self.step.is_finite() || self.step <= 0.0 {
            return Err(anyhow!(
                "solid-solution RDF step must be finite and positive"
            ));
        }
        if !self.sigma.is_finite() || self.sigma < 0.0 {
            return Err(anyhow!(
                "solid-solution RDF sigma must be finite and non-negative"
            ));
        }
        if !self.accuracy.is_finite() || self.accuracy <= 0.0 {
            return Err(anyhow!(
                "solid-solution RDF accuracy must be finite and positive"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SolidSolutionsRdfReport {
    pub config: SolidSolutionsRdfConfig,
    pub temperature: f64,
    pub structure_count: usize,
    pub pair_labels: Vec<String>,
    pub d_rdf: SolidSolutionsTrdfDataset,
    pub t_rdf: SolidSolutionsTrdfDataset,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SolidSolutionsTrdfDataset {
    pub label: String,
    pub sigma: f64,
    pub structure_count: usize,
    pub reference_energy: f64,
    pub norm: f64,
    pub bins: Vec<SolidSolutionsRdfBin>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SolidSolutionsRdfBin {
    pub radius: f64,
    pub total: f64,
    pub pair_values: BTreeMap<String, f64>,
}

pub struct SolidSolutionsWorkflowService;

pub struct SolidSolutionsWorkflowPorts<'a> {
    pub library_port: &'a dyn SolidSolutionsLibraryPort,
    pub identity_port: &'a dyn SolidSolutionsIdentityPort,
    pub geometry_port: &'a dyn SolidSolutionsGeometryPort,
    pub move_port: &'a dyn SolidSolutionsMovePort,
    pub evaluation_port: &'a dyn SamplingEvaluationPort,
    pub artifact_sink: &'a dyn SolidSolutionsArtifactSink,
}

impl SolidSolutionsWorkflowService {
    pub fn execute(
        &self,
        request: &SolidSolutionsWorkflowRequest,
        ports: SolidSolutionsWorkflowPorts<'_>,
    ) -> Result<SolidSolutionsWorkflowExecution> {
        let execution = execute_solid_solutions_workflow(request, &ports)?;
        ports
            .artifact_sink
            .persist_solid_solutions_run(&execution)?;
        Ok(execution)
    }
}

#[derive(Debug, Clone)]
pub struct SolidSolutionsDuplicateLibrary {
    store: TopologyIdentityStore,
}

impl SolidSolutionsDuplicateLibrary {
    pub fn from_imported_hashkeys(hashkeys: impl IntoIterator<Item = String>) -> Self {
        Self {
            store: TopologyIdentityStore::from_imported_hashkeys(hashkeys),
        }
    }

    pub fn classify_hashkey(&self, hashkey: &str) -> Option<SolidSolutionDuplicateRecord> {
        match self.store.probe(hashkey) {
            Some(TopologyIdentityMatch::ImportedLibrary {
                hashkey,
                occurrences,
            }) => Some(SolidSolutionDuplicateRecord {
                hashkey: hashkey.to_string(),
                source: SolidSolutionDuplicateSource::ImportedLibrary,
                occurrences,
            }),
            Some(TopologyIdentityMatch::CurrentRunHistory {
                hashkey,
                occurrences,
            }) => Some(SolidSolutionDuplicateRecord {
                hashkey: hashkey.to_string(),
                source: SolidSolutionDuplicateSource::CurrentRun,
                occurrences,
            }),
            _ => None,
        }
    }

    pub fn register_current_hashkey(&mut self, hashkey: &str) -> Result<()> {
        self.store.register_current_run_hashkey(hashkey)
    }

    pub fn snapshot(&self, steps: Vec<SolidSolutionStepState>) -> SolidSolutionRunState {
        SolidSolutionRunState {
            imported_hashkeys: self
                .store
                .imported_library_records()
                .iter()
                .map(|(hashkey, occurrences)| SolidSolutionDuplicateRecord {
                    hashkey: hashkey.clone(),
                    source: SolidSolutionDuplicateSource::ImportedLibrary,
                    occurrences: *occurrences,
                })
                .collect(),
            current_run_hashkeys: self
                .store
                .current_run_history_records()
                .iter()
                .map(|(hashkey, occurrences)| SolidSolutionDuplicateRecord {
                    hashkey: hashkey.clone(),
                    source: SolidSolutionDuplicateSource::CurrentRun,
                    occurrences: *occurrences,
                })
                .collect(),
            steps,
        }
    }
}

fn execute_solid_solutions_workflow(
    request: &SolidSolutionsWorkflowRequest,
    ports: &SolidSolutionsWorkflowPorts<'_>,
) -> Result<SolidSolutionsWorkflowExecution> {
    request.validate()?;
    let imported_hashkeys = {
        let mut imported = request.imported_hashkeys.clone();
        imported.extend(
            ports
                .library_port
                .load_imported_hashkeys(&SolidSolutionsLibraryRequest)?,
        );
        imported
    };
    let mut duplicate_library =
        SolidSolutionsDuplicateLibrary::from_imported_hashkeys(imported_hashkeys);

    let initial_evaluation = if request.skip_evaluation {
        None
    } else {
        Some(ports.evaluation_port.evaluate(&SamplingEvaluationRequest {
            candidate: request.initial_candidate.clone(),
            eval_dir: request.workdir.join("initial"),
            workflow: SamplingWorkflowIntent::SolidSolutions,
            intent: SamplingEvaluationIntent::InitialState,
            step_index: None,
            lid_index: None,
            runner_index: None,
        })?)
    };

    let mut rng = SolidSolutionsRng::new(request.seed);
    let mut best_energy = initial_evaluation.as_ref().map(|result| result.energy);
    let mut current_candidate = initial_evaluation
        .as_ref()
        .map(|result| result.relaxed_candidate.clone())
        .unwrap_or_else(|| request.initial_candidate.clone());
    let mut current_energy = initial_evaluation
        .as_ref()
        .map(|result| result.energy)
        .unwrap_or(0.0);
    let step_count = solid_solution_step_count(request);
    let mut steps = Vec::with_capacity(step_count);
    for step in 0..step_count {
        let proposal =
            solid_solution_step_proposal(request, step, &current_candidate, ports.move_port)?;
        if ports.geometry_port.validate_geometry(&proposal).is_err() {
            steps.push(solid_solution_geometry_rejection(
                step,
                &proposal,
                "geometry rejected",
            ));
            continue;
        }

        let hashkey = normalize_hashkey(ports.identity_port.build_hashkey(&proposal)?)?;
        if let Some(ref hashkey_value) = hashkey {
            if let Some(duplicate) = duplicate_library.classify_hashkey(hashkey_value) {
                steps.push(solid_solution_duplicate_rejection(
                    step,
                    &proposal,
                    hashkey_value.clone(),
                    duplicate,
                ));
                continue;
            }
            duplicate_library.register_current_hashkey(hashkey_value)?;
        }

        if request.skip_evaluation {
            steps.push(solid_solution_skipped_evaluation(
                step,
                &proposal,
                hashkey.clone(),
            ));
            current_candidate = proposal.clone();
            current_energy = 0.0;
            continue;
        }

        match ports.evaluation_port.evaluate(&SamplingEvaluationRequest {
            candidate: proposal.clone(),
            eval_dir: request.workdir.join(format!("step_{step:04}")),
            workflow: SamplingWorkflowIntent::SolidSolutions,
            intent: SamplingEvaluationIntent::SamplingStep,
            step_index: Some(step),
            lid_index: None,
            runner_index: None,
        }) {
            Ok(result) => {
                best_energy = Some(match best_energy {
                    Some(current_best) => current_best.min(result.energy),
                    None => result.energy,
                });
                let accepted = solid_solution_accepts(
                    current_energy,
                    result.energy,
                    request.acceptance_mode,
                    request.temperature,
                    rng.next_f64(),
                )?;
                if accepted {
                    current_candidate = result.relaxed_candidate.clone();
                    current_energy = result.energy;
                    steps.push(solid_solution_accepted_step(
                        step, &proposal, &result, hashkey,
                    ));
                } else {
                    steps.push(solid_solution_acceptance_rejection(
                        step, &proposal, &result, hashkey,
                    ));
                }
            }
            Err(_) => {
                steps.push(solid_solution_evaluation_failure(step, &proposal, hashkey));
            }
        }
    }

    let state = duplicate_library.snapshot(steps);
    let summary =
        summarize_solid_solutions_run(request, initial_evaluation.as_ref(), &state, best_energy);

    Ok(SolidSolutionsWorkflowExecution {
        summary,
        initial_source: StructureRecord::from(&request.initial_candidate),
        initial_evaluation: initial_evaluation.as_ref().map(EvaluationRecord::from),
        state,
    })
}

fn solid_solution_step_count(request: &SolidSolutionsWorkflowRequest) -> usize {
    if request.proposals.is_empty() {
        request.steps
    } else {
        request.proposals.len()
    }
}

fn solid_solution_step_proposal(
    request: &SolidSolutionsWorkflowRequest,
    step: usize,
    current_candidate: &Candidate,
    move_port: &dyn SolidSolutionsMovePort,
) -> Result<Candidate> {
    if let Some(proposal) = request.proposals.get(step) {
        return Ok(proposal.clone());
    }
    move_port.propose_candidate(&SolidSolutionsMoveRequest {
        move_mode: request.move_mode,
        step_index: step,
        current_candidate: current_candidate.clone(),
        initial_candidate: request.initial_candidate.clone(),
        max_exchanges: request.max_exchanges,
    })
}

fn solid_solution_accepts(
    current_energy: f64,
    candidate_energy: f64,
    mode: SolidSolutionsAcceptanceMode,
    temperature: f64,
    draw: f64,
) -> Result<bool> {
    if !current_energy.is_finite() || !candidate_energy.is_finite() {
        return Err(anyhow!(
            "solid-solution acceptance requires finite energies; current={current_energy}, candidate={candidate_energy}"
        ));
    }
    let accepted = match mode {
        SolidSolutionsAcceptanceMode::RecordAll => true,
        SolidSolutionsAcceptanceMode::DownhillOnly => candidate_energy < current_energy,
        SolidSolutionsAcceptanceMode::Metropolis => {
            accept_energy_transition(
                Some(current_energy),
                candidate_energy,
                MonteCarloAcceptance::Metropolis {
                    temperature: temperature.max(1.0e-12),
                },
                draw,
            )
            .accepted
        }
    };
    Ok(accepted)
}

fn summarize_solid_solutions_run(
    request: &SolidSolutionsWorkflowRequest,
    initial_evaluation: Option<&EvalResult>,
    state: &SolidSolutionRunState,
    best_energy: Option<f64>,
) -> SolidSolutionsRunSummary {
    let mut accepted_steps = 0;
    let mut rejected_acceptance_steps = 0;
    let mut rejected_geometry_steps = 0;
    let mut rejected_duplicate_imported_steps = 0;
    let mut rejected_duplicate_current_run_steps = 0;
    let mut evaluation_failed_steps = 0;
    let mut skipped_evaluation_steps = 0;

    for step in &state.steps {
        match step.decision {
            SolidSolutionStepDecision::Accepted => accepted_steps += 1,
            SolidSolutionStepDecision::RejectedAcceptance => rejected_acceptance_steps += 1,
            SolidSolutionStepDecision::RejectedGeometry => rejected_geometry_steps += 1,
            SolidSolutionStepDecision::RejectedDuplicateImportedLibrary => {
                rejected_duplicate_imported_steps += 1;
            }
            SolidSolutionStepDecision::RejectedDuplicateCurrentRun => {
                rejected_duplicate_current_run_steps += 1;
            }
            SolidSolutionStepDecision::EvaluationFailed => evaluation_failed_steps += 1,
            SolidSolutionStepDecision::SkippedEvaluation => skipped_evaluation_steps += 1,
        }
    }

    SolidSolutionsRunSummary {
        proposal_count: request.proposals.len(),
        imported_hashkey_count: state.imported_hashkeys.len(),
        skip_evaluation: request.skip_evaluation,
        accepted_steps,
        rejected_acceptance_steps,
        rejected_geometry_steps,
        rejected_duplicate_imported_steps,
        rejected_duplicate_current_run_steps,
        evaluation_failed_steps,
        skipped_evaluation_steps,
        initial_energy: initial_evaluation.map(|result| result.energy),
        best_energy,
    }
}

fn normalize_hashkey(hashkey: Option<String>) -> Result<Option<String>> {
    match hashkey {
        Some(hashkey) if hashkey.trim().is_empty() => {
            Err(anyhow!("solid-solution hashkey must not be empty"))
        }
        Some(hashkey) => Ok(Some(hashkey)),
        None => Ok(None),
    }
}

pub fn solid_solution_statistics_rows(
    execution: &SolidSolutionsWorkflowExecution,
) -> Vec<SolidSolutionStatisticsRow> {
    execution
        .state
        .steps
        .iter()
        .map(|step| SolidSolutionStatisticsRow {
            cluster_no: step.step + 1,
            cluster_id: solid_solution_statistics_cluster_id(step),
            energy: step
                .evaluation
                .as_ref()
                .map(|evaluation| evaluation.energy)
                .unwrap_or(0.0),
            hashkey: step.hashkey.clone(),
            decision: step.decision,
        })
        .collect()
}

pub fn solid_solution_hashkey_statistics_rows(
    state: &SolidSolutionRunState,
) -> Vec<SolidSolutionHashkeyStatisticsRow> {
    state
        .imported_hashkeys
        .iter()
        .map(|record| SolidSolutionHashkeyStatisticsRow {
            source_type: "P".to_string(),
            hashkey: record.hashkey.clone(),
            occurrences: record.occurrences,
        })
        .chain(
            state
                .current_run_hashkeys
                .iter()
                .map(|record| SolidSolutionHashkeyStatisticsRow {
                    source_type: "N".to_string(),
                    hashkey: record.hashkey.clone(),
                    occurrences: record.occurrences,
                }),
        )
        .collect()
}

fn solid_solution_statistics_cluster_id(step: &SolidSolutionStepState) -> String {
    step.evaluation
        .as_ref()
        .map(|evaluation| evaluation.label.clone())
        .unwrap_or_else(|| step.source.label.clone())
}

pub fn build_solid_solutions_rdf_report(
    execution: &SolidSolutionsWorkflowExecution,
    config: &SolidSolutionsRdfConfig,
    temperature: f64,
) -> Result<SolidSolutionsRdfReport> {
    config.validate()?;
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(anyhow!(
            "solid-solution RDF temperature must be finite and positive"
        ));
    }

    let samples = solid_solution_rdf_samples(execution);
    let Some(first_sample) = samples.first() else {
        return Err(anyhow!(
            "solid-solution RDF requires at least one evaluated structure"
        ));
    };
    let pair_specs = rdf_pair_specs(&first_sample.candidate);
    if pair_specs.is_empty() {
        return Err(anyhow!(
            "solid-solution RDF requires at least one non-vacancy species"
        ));
    }
    let pair_labels = pair_specs
        .iter()
        .map(|pair| pair.label.clone())
        .collect::<Vec<_>>();
    let reference_energy = first_sample.energy;
    let mut d_rdf = SolidSolutionsTrdfAccumulator::new(
        "D-RDF",
        0.0,
        reference_energy,
        pair_labels.clone(),
        config.steps,
    );
    let mut t_rdf = SolidSolutionsTrdfAccumulator::new(
        "T-RDF",
        config.sigma,
        reference_energy,
        pair_labels.clone(),
        config.steps,
    );

    for sample in &samples {
        let contributions =
            solid_solution_rdf_contributions(&sample.candidate, &pair_specs, config)?;
        let weight = rdf_boltzmann_weight(reference_energy, sample.energy, temperature)?;
        d_rdf.add_structure(&contributions, weight, config);
        t_rdf.add_structure(&contributions, weight, config);
    }

    Ok(SolidSolutionsRdfReport {
        config: config.clone(),
        temperature,
        structure_count: samples.len(),
        pair_labels,
        d_rdf: d_rdf.into_dataset(config),
        t_rdf: t_rdf.into_dataset(config),
    })
}

#[derive(Debug, Clone)]
struct SolidSolutionsRdfSample {
    candidate: Candidate,
    energy: f64,
}

#[derive(Debug, Clone)]
struct SolidSolutionsRdfPairSpec {
    label: String,
    center_species: String,
    neighbor_species: String,
}

#[derive(Debug, Clone)]
struct SolidSolutionsRdfContribution {
    pair_label: String,
    distance: f64,
    value: f64,
}

#[derive(Debug, Clone)]
struct SolidSolutionsTrdfAccumulator {
    label: String,
    sigma: f64,
    structure_count: usize,
    reference_energy: f64,
    norm: f64,
    pair_labels: Vec<String>,
    bins: Vec<BTreeMap<String, f64>>,
}

impl SolidSolutionsTrdfAccumulator {
    fn new(
        label: impl Into<String>,
        sigma: f64,
        reference_energy: f64,
        pair_labels: Vec<String>,
        steps: usize,
    ) -> Self {
        Self {
            label: label.into(),
            sigma,
            structure_count: 0,
            reference_energy,
            norm: 0.0,
            pair_labels,
            bins: (0..steps).map(|_| BTreeMap::new()).collect(),
        }
    }

    fn add_structure(
        &mut self,
        contributions: &[SolidSolutionsRdfContribution],
        weight: f64,
        config: &SolidSolutionsRdfConfig,
    ) {
        self.structure_count += 1;
        self.norm += weight;
        if self.sigma < 0.001 {
            self.add_histogram_structure(contributions, weight, config);
        } else {
            self.add_smeared_structure(contributions, weight, config);
        }
    }

    fn add_histogram_structure(
        &mut self,
        contributions: &[SolidSolutionsRdfContribution],
        weight: f64,
        config: &SolidSolutionsRdfConfig,
    ) {
        for contribution in contributions {
            if let Some(bin_index) = rdf_histogram_bin_index(contribution.distance, config) {
                if let Some(bin) = self.bins.get_mut(bin_index) {
                    *bin.entry(contribution.pair_label.clone()).or_insert(0.0) +=
                        contribution.value * weight;
                }
            }
        }
    }

    fn add_smeared_structure(
        &mut self,
        contributions: &[SolidSolutionsRdfContribution],
        weight: f64,
        config: &SolidSolutionsRdfConfig,
    ) {
        let inv_sigma = 1.0 / self.sigma;
        let amplitude = RDF_GAUSSIAN_NORM * inv_sigma;
        for contribution in contributions {
            for (bin_index, bin) in self.bins.iter_mut().enumerate() {
                let radius = rdf_bin_radius(config, bin_index);
                let x = (radius - contribution.distance) * inv_sigma;
                let value = contribution.value * weight * amplitude * (-x * x).exp();
                if value > 1.0e-300 {
                    *bin.entry(contribution.pair_label.clone()).or_insert(0.0) += value;
                }
            }
        }
    }

    fn into_dataset(self, config: &SolidSolutionsRdfConfig) -> SolidSolutionsTrdfDataset {
        let bins = self
            .bins
            .into_iter()
            .enumerate()
            .map(|(index, pair_values)| SolidSolutionsRdfBin {
                radius: rdf_bin_radius(config, index),
                total: self
                    .pair_labels
                    .iter()
                    .filter_map(|pair_label| pair_values.get(pair_label).copied())
                    .sum(),
                pair_values,
            })
            .collect();
        SolidSolutionsTrdfDataset {
            label: self.label,
            sigma: self.sigma,
            structure_count: self.structure_count,
            reference_energy: self.reference_energy,
            norm: self.norm,
            bins,
        }
    }
}

const RDF_FOUR_PI: f64 = std::f64::consts::PI * 4.0;
const RDF_GAUSSIAN_NORM: f64 = 0.564189583548;
const RDF_BOLTZMANN_EV_PER_K: f64 = 0.00008617333262;

fn solid_solution_rdf_samples(
    execution: &SolidSolutionsWorkflowExecution,
) -> Vec<SolidSolutionsRdfSample> {
    execution
        .initial_evaluation
        .iter()
        .map(|evaluation| SolidSolutionsRdfSample {
            candidate: Candidate::from(&evaluation.structure),
            energy: evaluation.energy,
        })
        .chain(execution.state.steps.iter().filter_map(|step| {
            step.evaluation
                .as_ref()
                .map(|evaluation| SolidSolutionsRdfSample {
                    candidate: Candidate::from(&evaluation.structure),
                    energy: evaluation.energy,
                })
        }))
        .collect()
}

fn rdf_pair_specs(candidate: &Candidate) -> Vec<SolidSolutionsRdfPairSpec> {
    let mut species_order = Vec::new();
    for species in &candidate.species {
        if species.eq_ignore_ascii_case("X") {
            continue;
        }
        if species_order.iter().all(|seen| seen != species) {
            species_order.push(species.clone());
        }
    }

    let mut pair_specs = Vec::new();
    for neighbor_index in 0..species_order.len() {
        for center_index in 0..=neighbor_index {
            let center_species = species_order[center_index].clone();
            let neighbor_species = species_order[neighbor_index].clone();
            pair_specs.push(SolidSolutionsRdfPairSpec {
                label: format!("{center_species}-{neighbor_species}"),
                center_species,
                neighbor_species,
            });
        }
    }
    pair_specs
}

fn solid_solution_rdf_contributions(
    candidate: &Candidate,
    pair_specs: &[SolidSolutionsRdfPairSpec],
    config: &SolidSolutionsRdfConfig,
) -> Result<Vec<SolidSolutionsRdfContribution>> {
    candidate
        .validate()
        .map_err(|error| anyhow!("invalid RDF candidate `{}`: {error:?}", candidate.label))?;
    let Some(lattice) = candidate.lattice else {
        return Err(anyhow!(
            "solid-solution RDF candidate `{}` must have lattice vectors",
            candidate.label
        ));
    };
    if !candidate.periodic_axes.iter().all(|axis| *axis) {
        return Err(anyhow!(
            "solid-solution RDF candidate `{}` must be fully periodic",
            candidate.label
        ));
    }
    let volume = lattice_volume(lattice);
    if !volume.is_finite() || volume <= 0.0 {
        return Err(anyhow!(
            "solid-solution RDF candidate `{}` has invalid cell volume",
            candidate.label
        ));
    }
    let normalization = RDF_FOUR_PI * candidate.len() as f64 / volume;
    let mut contributions = Vec::new();
    for center_index in 0..candidate.len() {
        let center_species = &candidate.species[center_index];
        if center_species.eq_ignore_ascii_case("X") {
            continue;
        }
        let center_cart = patina_search::fractional_to_cartesian(
            lattice,
            candidate.fractional_coords[center_index],
        );
        for neighbor_index in 0..candidate.len() {
            if center_index == neighbor_index {
                continue;
            }
            let neighbor_species = &candidate.species[neighbor_index];
            if neighbor_species.eq_ignore_ascii_case("X") {
                continue;
            }
            let Some(pair_spec) = pair_specs.iter().find(|pair_spec| {
                pair_spec.center_species == center_species.as_str()
                    && pair_spec.neighbor_species == neighbor_species.as_str()
            }) else {
                continue;
            };
            let neighbor_cart = patina_search::fractional_to_cartesian(
                lattice,
                candidate.fractional_coords[neighbor_index],
            );
            let distance_sq = patina_search::minimum_image_cartesian_distance_sq_with_axes(
                center_cart,
                neighbor_cart,
                Some(lattice),
                candidate.periodic_axes,
            );
            let distance = distance_sq.sqrt();
            if distance <= config.zero || distance > config.cutoff {
                continue;
            }
            let denominator = normalization * distance * distance;
            if denominator <= 0.0 || !denominator.is_finite() {
                continue;
            }
            push_grouped_rdf_contribution(
                &mut contributions,
                pair_spec.label.clone(),
                distance,
                1.0 / denominator,
                config.accuracy,
            );
        }
    }
    Ok(contributions)
}

fn push_grouped_rdf_contribution(
    contributions: &mut Vec<SolidSolutionsRdfContribution>,
    pair_label: String,
    distance: f64,
    value: f64,
    accuracy: f64,
) {
    if let Some(existing) = contributions.iter_mut().find(|contribution| {
        contribution.pair_label == pair_label
            && (contribution.distance - distance).abs() <= accuracy
    }) {
        existing.value += value;
    } else {
        contributions.push(SolidSolutionsRdfContribution {
            pair_label,
            distance,
            value,
        });
    }
}

fn rdf_boltzmann_weight(reference_energy: f64, energy: f64, temperature: f64) -> Result<f64> {
    if !reference_energy.is_finite() || !energy.is_finite() {
        return Err(anyhow!(
            "solid-solution RDF Boltzmann weights require finite energies"
        ));
    }
    let denominator = RDF_BOLTZMANN_EV_PER_K * temperature;
    if denominator <= 0.0 || !denominator.is_finite() {
        return Err(anyhow!(
            "solid-solution RDF Boltzmann denominator must be finite and positive"
        ));
    }
    let exponent = (reference_energy - energy) / denominator;
    if exponent > 700.0 {
        return Err(anyhow!(
            "solid-solution RDF Boltzmann exponent is too large for stable accumulation"
        ));
    }
    if exponent < -745.0 {
        Ok(0.0)
    } else {
        Ok(exponent.exp())
    }
}

fn rdf_histogram_bin_index(distance: f64, config: &SolidSolutionsRdfConfig) -> Option<usize> {
    if !distance.is_finite() || distance < config.zero {
        return None;
    }
    let bin = ((distance - config.zero + 0.5 * config.step) / config.step).floor();
    if bin < 0.0 {
        return None;
    }
    let index = bin as usize;
    (index < config.steps).then_some(index)
}

fn rdf_bin_radius(config: &SolidSolutionsRdfConfig, index: usize) -> f64 {
    config.zero + config.step * index as f64
}

fn rdf_default_step(zero: f64, cutoff: f64, steps: usize) -> f64 {
    if steps > 1 && cutoff > zero && cutoff.is_finite() && zero.is_finite() {
        (cutoff - zero) / (steps as f64 - 1.0)
    } else {
        1.0
    }
}

fn lattice_volume(lattice: [[f64; 3]; 3]) -> f64 {
    let cross = [
        lattice[1][1] * lattice[2][2] - lattice[1][2] * lattice[2][1],
        lattice[1][2] * lattice[2][0] - lattice[1][0] * lattice[2][2],
        lattice[1][0] * lattice[2][1] - lattice[1][1] * lattice[2][0],
    ];
    (lattice[0][0] * cross[0] + lattice[0][1] * cross[1] + lattice[0][2] * cross[2]).abs()
}

fn validate_supported_candidate(candidate: &Candidate, context: &str) -> Result<()> {
    candidate
        .validate()
        .map_err(|error| anyhow!("invalid {context} `{}`: {error:?}", candidate.label))?;
    if candidate.has_partial_periodicity() {
        return Err(anyhow!(
            "{context} `{}` uses unsupported partial periodicity",
            candidate.label
        ));
    }
    Ok(())
}

pub fn solid_solution_geometry_rejection(
    step: usize,
    candidate: &Candidate,
    message: impl Into<String>,
) -> SolidSolutionStepState {
    let _message = message.into();
    SolidSolutionStepState {
        step,
        decision: SolidSolutionStepDecision::RejectedGeometry,
        source: StructureRecord::from(candidate),
        evaluation: None,
        hashkey: None,
        duplicate: None,
    }
}

pub fn solid_solution_duplicate_rejection(
    step: usize,
    candidate: &Candidate,
    hashkey: impl Into<String>,
    duplicate: SolidSolutionDuplicateRecord,
) -> SolidSolutionStepState {
    let hashkey = hashkey.into();
    let decision = match duplicate.source {
        SolidSolutionDuplicateSource::ImportedLibrary => {
            SolidSolutionStepDecision::RejectedDuplicateImportedLibrary
        }
        SolidSolutionDuplicateSource::CurrentRun => {
            SolidSolutionStepDecision::RejectedDuplicateCurrentRun
        }
    };
    SolidSolutionStepState {
        step,
        decision,
        source: StructureRecord::from(candidate),
        evaluation: None,
        hashkey: Some(hashkey),
        duplicate: Some(duplicate),
    }
}

pub fn solid_solution_evaluation_failure(
    step: usize,
    candidate: &Candidate,
    hashkey: Option<String>,
) -> SolidSolutionStepState {
    SolidSolutionStepState {
        step,
        decision: SolidSolutionStepDecision::EvaluationFailed,
        source: StructureRecord::from(candidate),
        evaluation: None,
        hashkey,
        duplicate: None,
    }
}

pub fn solid_solution_acceptance_rejection(
    step: usize,
    candidate: &Candidate,
    result: &EvalResult,
    hashkey: Option<String>,
) -> SolidSolutionStepState {
    SolidSolutionStepState {
        step,
        decision: SolidSolutionStepDecision::RejectedAcceptance,
        source: StructureRecord::from(candidate),
        evaluation: Some(EvaluationRecord::from(result)),
        hashkey,
        duplicate: None,
    }
}

pub fn solid_solution_skipped_evaluation(
    step: usize,
    candidate: &Candidate,
    hashkey: Option<String>,
) -> SolidSolutionStepState {
    SolidSolutionStepState {
        step,
        decision: SolidSolutionStepDecision::SkippedEvaluation,
        source: StructureRecord::from(candidate),
        evaluation: None,
        hashkey,
        duplicate: None,
    }
}

pub fn propose_solid_solution_candidate(
    request: &SolidSolutionsMoveRequest,
    exchanges: usize,
    draws: &[f64],
) -> Result<Candidate> {
    if request.max_exchanges == 0 {
        return Err(anyhow!("solid-solution max_exchanges must be positive"));
    }
    if exchanges == 0 || exchanges > request.max_exchanges {
        return Err(anyhow!(
            "solid-solution exchanges must be in 1..={}; got {exchanges}",
            request.max_exchanges
        ));
    }
    for (index, draw) in draws.iter().enumerate() {
        if !draw.is_finite() || *draw < 0.0 || *draw >= 1.0 {
            return Err(anyhow!(
                "solid-solution random draw {index} must be in [0, 1); got {draw}"
            ));
        }
    }

    let mut proposal = match request.move_mode {
        SolidSolutionsMoveMode::MixSolution => {
            mix_solution_candidate(&request.current_candidate, exchanges, draws)?
        }
        SolidSolutionsMoveMode::RandomizeSolution => {
            randomize_solution_candidate(&request.initial_candidate, draws)?
        }
    };
    proposal.label = format!(
        "solid_solution_step_{:04}_{}",
        request.step_index,
        match request.move_mode {
            SolidSolutionsMoveMode::MixSolution => "mixed",
            SolidSolutionsMoveMode::RandomizeSolution => "randomized",
        }
    );
    validate_supported_candidate(&proposal, "solid-solution proposal")?;
    Ok(proposal)
}

fn mix_solution_candidate(
    source: &Candidate,
    exchanges: usize,
    draws: &[f64],
) -> Result<Candidate> {
    validate_supported_candidate(source, "solid-solution mix source")?;
    if source.len() < 2 {
        return Err(anyhow!(
            "solid-solution mix requires at least two atomic sites"
        ));
    }
    let mut order = (0..source.len()).collect::<Vec<_>>();
    let mut available = (0..source.len()).collect::<Vec<_>>();
    let mut draw_index = 0usize;
    for _ in 0..exchanges {
        if available.len() < 2 {
            return Err(anyhow!(
                "solid-solution mix exhausted available exchange sites"
            ));
        }
        let first_position = remove_drawn_index(&mut available, draws, &mut draw_index)?;
        let compatible = available
            .iter()
            .copied()
            .filter(|&candidate_position| {
                source.species[first_position] != source.species[candidate_position]
            })
            .collect::<Vec<_>>();
        if compatible.is_empty() {
            return Err(anyhow!(
                "solid-solution mix cannot find a different-species exchange partner for site {first_position}"
            ));
        }
        let compatible_index = draw_index_from(draws, &mut draw_index, compatible.len())?;
        let second_position = compatible[compatible_index];
        order.swap(first_position, second_position);
        available.retain(|&position| position != second_position);
    }

    Ok(candidate_with_coordinate_order(source, &order))
}

fn randomize_solution_candidate(source: &Candidate, draws: &[f64]) -> Result<Candidate> {
    validate_supported_candidate(source, "solid-solution randomization source")?;
    let mut available = (0..source.len()).collect::<Vec<_>>();
    let mut order = Vec::with_capacity(source.len());
    let mut draw_index = 0usize;
    while !available.is_empty() {
        order.push(remove_drawn_index(&mut available, draws, &mut draw_index)?);
    }
    Ok(candidate_with_coordinate_order(source, &order))
}

fn candidate_with_coordinate_order(source: &Candidate, order: &[usize]) -> Candidate {
    let mut candidate = source.clone();
    candidate.fractional_coords = order
        .iter()
        .map(|&source_index| source.fractional_coords[source_index])
        .collect();
    candidate
}

fn remove_drawn_index(
    available: &mut Vec<usize>,
    draws: &[f64],
    draw_index: &mut usize,
) -> Result<usize> {
    let selected = draw_index_from(draws, draw_index, available.len())?;
    Ok(available.remove(selected))
}

fn draw_index_from(draws: &[f64], draw_index: &mut usize, upper: usize) -> Result<usize> {
    if upper == 0 {
        return Err(anyhow!("solid-solution draw requested from an empty set"));
    }
    let Some(draw) = draws.get(*draw_index) else {
        return Err(anyhow!(
            "solid-solution move requires more random draws; exhausted at draw {}",
            *draw_index
        ));
    };
    *draw_index += 1;
    let index = (*draw * upper as f64).floor() as usize;
    Ok(index.min(upper.saturating_sub(1)))
}

#[derive(Debug, Clone)]
struct SolidSolutionsRng {
    state: u64,
}

impl SolidSolutionsRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let bits = self.state >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }
}

pub fn solid_solution_accepted_step(
    step: usize,
    candidate: &Candidate,
    result: &EvalResult,
    hashkey: Option<String>,
) -> SolidSolutionStepState {
    SolidSolutionStepState {
        step,
        decision: SolidSolutionStepDecision::Accepted,
        source: StructureRecord::from(candidate),
        evaluation: Some(EvaluationRecord::from(result)),
        hashkey,
        duplicate: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_solid_solutions_rdf_report, solid_solution_accepted_step,
        solid_solution_duplicate_rejection, solid_solution_evaluation_failure,
        solid_solution_geometry_rejection, solid_solution_hashkey_statistics_rows,
        solid_solution_skipped_evaluation, solid_solution_statistics_rows,
        SolidSolutionsDuplicateLibrary, SolidSolutionsRdfConfig, SolidSolutionsRunSummary,
        SolidSolutionsWorkflowExecution, SolidSolutionsWorkflowPorts,
        SolidSolutionsWorkflowRequest, SolidSolutionsWorkflowService,
    };
    use crate::application::ports::{
        SamplingEvaluationIntent, SamplingEvaluationPort, SamplingEvaluationRequest,
        SamplingWorkflowIntent, SolidSolutionsAcceptanceMode, SolidSolutionsArtifactSink,
        SolidSolutionsGeometryPort, SolidSolutionsIdentityPort, SolidSolutionsLibraryPort,
        SolidSolutionsLibraryRequest, SolidSolutionsMoveMode, SolidSolutionsMovePort,
        SolidSolutionsMoveRequest,
    };
    use patina_types::{
        Candidate, EvalResult, SolidSolutionDuplicateSource, SolidSolutionStepDecision,
    };
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::time::Duration;

    fn candidate(label: &str) -> Candidate {
        Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: label.into(),
        }
    }

    fn result(label: &str, energy: f64) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; 2],
            relaxed_candidate: candidate(label),
            converged: true,
            wall_time: Duration::from_secs(0),
        }
    }

    fn periodic_candidate(label: &str) -> Candidate {
        Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            lattice: Some([[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]]),
            periodic_axes: [true, true, true],
            label: label.into(),
        }
    }

    fn periodic_result(label: &str, energy: f64) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; 2],
            relaxed_candidate: periodic_candidate(label),
            converged: true,
            wall_time: Duration::from_secs(0),
        }
    }

    fn empty_summary() -> SolidSolutionsRunSummary {
        SolidSolutionsRunSummary {
            proposal_count: 0,
            imported_hashkey_count: 0,
            skip_evaluation: false,
            accepted_steps: 0,
            rejected_acceptance_steps: 0,
            rejected_geometry_steps: 0,
            rejected_duplicate_imported_steps: 0,
            rejected_duplicate_current_run_steps: 0,
            evaluation_failed_steps: 0,
            skipped_evaluation_steps: 0,
            initial_energy: None,
            best_energy: None,
        }
    }

    #[test]
    fn duplicate_library_distinguishes_imported_and_current_run_matches() {
        let mut library =
            SolidSolutionsDuplicateLibrary::from_imported_hashkeys(["hk-a".to_string()]);
        let imported = library
            .classify_hashkey("hk-a")
            .expect("imported duplicate");
        assert_eq!(
            imported.source,
            SolidSolutionDuplicateSource::ImportedLibrary
        );
        assert_eq!(imported.occurrences, 1);

        library
            .register_current_hashkey("hk-b")
            .expect("register current hashkey");
        library
            .register_current_hashkey("hk-b")
            .expect("register current hashkey again");
        let current = library.classify_hashkey("hk-b").expect("current duplicate");
        assert_eq!(current.source, SolidSolutionDuplicateSource::CurrentRun);
        assert_eq!(current.occurrences, 2);
    }

    #[test]
    fn duplicate_rejection_preserves_duplicate_provenance() {
        let duplicate =
            SolidSolutionsDuplicateLibrary::from_imported_hashkeys(["hk-a".to_string()])
                .classify_hashkey("hk-a")
                .expect("duplicate");
        let state = solid_solution_duplicate_rejection(3, &candidate("trial"), "hk-a", duplicate);
        assert_eq!(
            state.decision,
            SolidSolutionStepDecision::RejectedDuplicateImportedLibrary
        );
        assert_eq!(
            state.duplicate.expect("duplicate").source,
            SolidSolutionDuplicateSource::ImportedLibrary
        );
    }

    #[test]
    fn accepted_and_skipped_steps_build_typed_step_state() {
        let accepted = solid_solution_accepted_step(
            1,
            &candidate("seed"),
            &result("relaxed", -1.2),
            Some("hk-1".into()),
        );
        assert_eq!(accepted.decision, SolidSolutionStepDecision::Accepted);
        assert_eq!(accepted.evaluation.expect("evaluation").energy, -1.2);

        let skipped = solid_solution_skipped_evaluation(2, &candidate("seed"), Some("hk-2".into()));
        assert_eq!(
            skipped.decision,
            SolidSolutionStepDecision::SkippedEvaluation
        );
        assert!(skipped.evaluation.is_none());

        let geometry = solid_solution_geometry_rejection(3, &candidate("seed"), "geometry");
        assert_eq!(
            geometry.decision,
            SolidSolutionStepDecision::RejectedGeometry
        );

        let failed = solid_solution_evaluation_failure(4, &candidate("seed"), Some("hk-4".into()));
        assert_eq!(failed.decision, SolidSolutionStepDecision::EvaluationFailed);
    }

    #[test]
    fn snapshot_exposes_library_and_step_state() {
        let request = SolidSolutionsWorkflowRequest {
            initial_candidate: candidate("seed"),
            proposals: Vec::new(),
            workdir: PathBuf::from("/tmp/solid_solutions"),
            steps: 1,
            temperature: 300.0,
            move_mode: SolidSolutionsMoveMode::MixSolution,
            acceptance_mode: SolidSolutionsAcceptanceMode::RecordAll,
            max_exchanges: 1,
            seed: 7,
            skip_evaluation: true,
            imported_hashkeys: vec!["hk-lib".into()],
        };
        request.validate().expect("valid request");
        assert!(request.skip_evaluation);
        let mut library = SolidSolutionsDuplicateLibrary::from_imported_hashkeys(
            request.imported_hashkeys.clone(),
        );
        library
            .register_current_hashkey("hk-run")
            .expect("register current");
        let snapshot = library.snapshot(vec![solid_solution_skipped_evaluation(
            0,
            &request.initial_candidate,
            Some("hk-run".into()),
        )]);

        assert_eq!(snapshot.imported_hashkeys.len(), 1);
        assert_eq!(snapshot.current_run_hashkeys.len(), 1);
        assert_eq!(snapshot.steps.len(), 1);
    }

    #[test]
    fn statistics_rows_preserve_legacy_hashkey_sources() {
        let mut library =
            SolidSolutionsDuplicateLibrary::from_imported_hashkeys(["hk-lib".to_string()]);
        library
            .register_current_hashkey("hk-run")
            .expect("register current hashkey");
        let duplicate = library
            .classify_hashkey("hk-lib")
            .expect("imported duplicate");
        let state = library.snapshot(vec![
            solid_solution_accepted_step(
                0,
                &candidate("proposal-a"),
                &result("relaxed-a", -1.2),
                Some("hk-run".into()),
            ),
            solid_solution_duplicate_rejection(1, &candidate("proposal-b"), "hk-lib", duplicate),
        ]);
        let execution = SolidSolutionsWorkflowExecution {
            summary: empty_summary(),
            initial_source: (&candidate("seed")).into(),
            initial_evaluation: None,
            state,
        };

        let rows = solid_solution_statistics_rows(&execution);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cluster_no, 1);
        assert_eq!(rows[0].cluster_id, "relaxed-a");
        assert_eq!(rows[0].hashkey.as_deref(), Some("hk-run"));
        assert_eq!(rows[1].energy, 0.0);

        let hashkey_rows = solid_solution_hashkey_statistics_rows(&execution.state);
        assert!(hashkey_rows
            .iter()
            .any(|row| row.source_type == "P" && row.hashkey == "hk-lib"));
        assert!(hashkey_rows
            .iter()
            .any(|row| row.source_type == "N" && row.hashkey == "hk-run"));
    }

    #[test]
    fn rdf_report_accumulates_d_and_t_rdf_for_evaluated_periodic_structures() {
        let initial_result = periodic_result("seed-relaxed", -2.0);
        let step_result = periodic_result("step-relaxed", -2.0);
        let mut library = SolidSolutionsDuplicateLibrary::from_imported_hashkeys(Vec::new());
        library
            .register_current_hashkey("hk-run")
            .expect("register current hashkey");
        let execution = SolidSolutionsWorkflowExecution {
            summary: SolidSolutionsRunSummary {
                initial_energy: Some(-2.0),
                best_energy: Some(-2.0),
                ..empty_summary()
            },
            initial_source: (&periodic_candidate("seed")).into(),
            initial_evaluation: Some((&initial_result).into()),
            state: library.snapshot(vec![solid_solution_accepted_step(
                0,
                &periodic_candidate("step"),
                &step_result,
                Some("hk-run".into()),
            )]),
        };
        let config =
            SolidSolutionsRdfConfig::with_values(9.0, Some(1.0), 10, 0.1, 0.0, 0.00001, true)
                .expect("valid RDF config");

        let report =
            build_solid_solutions_rdf_report(&execution, &config, 300.0).expect("RDF report");

        assert_eq!(report.structure_count, 2);
        assert!(report.pair_labels.iter().any(|label| label == "Mg-O"));
        let d_rdf_pair_total = report
            .d_rdf
            .bins
            .iter()
            .filter_map(|bin| bin.pair_values.get("Mg-O").copied())
            .sum::<f64>();
        assert!(d_rdf_pair_total > 0.0);
        assert!(report.t_rdf.norm > 0.0);
    }

    #[derive(Default)]
    struct LibraryPortStub {
        imported_hashkeys: Vec<String>,
    }

    impl SolidSolutionsLibraryPort for LibraryPortStub {
        fn load_imported_hashkeys(
            &self,
            _request: &SolidSolutionsLibraryRequest,
        ) -> anyhow::Result<Vec<String>> {
            Ok(self.imported_hashkeys.clone())
        }
    }

    #[derive(Default)]
    struct IdentityPortStub {
        map: BTreeMap<String, Option<String>>,
    }

    impl SolidSolutionsIdentityPort for IdentityPortStub {
        fn build_hashkey(&self, candidate: &Candidate) -> anyhow::Result<Option<String>> {
            Ok(self
                .map
                .get(&candidate.label)
                .cloned()
                .unwrap_or_else(|| Some(candidate.label.clone())))
        }
    }

    #[derive(Default)]
    struct GeometryPortStub {
        rejected_labels: BTreeSet<String>,
    }

    impl SolidSolutionsGeometryPort for GeometryPortStub {
        fn validate_geometry(&self, candidate: &Candidate) -> anyhow::Result<()> {
            if self.rejected_labels.contains(&candidate.label) {
                anyhow::bail!("geometry rejected");
            }
            Ok(())
        }
    }

    struct MovePortStub;

    impl SolidSolutionsMovePort for MovePortStub {
        fn propose_candidate(
            &self,
            request: &SolidSolutionsMoveRequest,
        ) -> anyhow::Result<Candidate> {
            let mut candidate = request.current_candidate.clone();
            candidate.label = format!("generated-{}", request.step_index);
            Ok(candidate)
        }
    }

    #[derive(Default)]
    struct SamplingPortStub {
        requests: RefCell<Vec<SamplingEvaluationRequest>>,
        failures: BTreeSet<String>,
    }

    impl SamplingEvaluationPort for SamplingPortStub {
        fn evaluate(&self, request: &SamplingEvaluationRequest) -> anyhow::Result<EvalResult> {
            self.requests.borrow_mut().push(request.clone());
            if self.failures.contains(&request.candidate.label) {
                anyhow::bail!("backend failed");
            }
            let energy = match request.intent {
                SamplingEvaluationIntent::InitialState => -10.0,
                SamplingEvaluationIntent::SamplingStep => {
                    -(request.step_index.unwrap_or(0) as f64) - 1.0
                }
                SamplingEvaluationIntent::QuenchStep | SamplingEvaluationIntent::Relaxation => {
                    unreachable!(
                        "solid-solutions tests should not route quench or relaxation intents"
                    )
                }
            };
            Ok(result(
                &format!("{}-relaxed", request.candidate.label),
                energy,
            ))
        }
    }

    #[derive(Default)]
    struct ArtifactSinkStub {
        persisted: RefCell<Vec<SolidSolutionsWorkflowExecution>>,
    }

    impl SolidSolutionsArtifactSink for ArtifactSinkStub {
        fn persist_solid_solutions_run(
            &self,
            execution: &SolidSolutionsWorkflowExecution,
        ) -> anyhow::Result<()> {
            self.persisted.borrow_mut().push(execution.clone());
            Ok(())
        }
    }

    #[test]
    fn workflow_routes_typed_sampling_intents_and_duplicate_provenance() {
        let request = SolidSolutionsWorkflowRequest {
            initial_candidate: candidate("seed"),
            proposals: vec![
                candidate("proposal-a"),
                candidate("proposal-imported"),
                candidate("proposal-current"),
            ],
            workdir: PathBuf::from("/tmp/solid_solutions"),
            steps: 0,
            temperature: 300.0,
            move_mode: SolidSolutionsMoveMode::MixSolution,
            acceptance_mode: SolidSolutionsAcceptanceMode::RecordAll,
            max_exchanges: 1,
            seed: 7,
            skip_evaluation: false,
            imported_hashkeys: Vec::new(),
        };
        let library_port = LibraryPortStub {
            imported_hashkeys: vec!["hk-imported".into()],
        };
        let identity_port = IdentityPortStub {
            map: BTreeMap::from([
                ("proposal-a".into(), Some("hk-a".into())),
                ("proposal-imported".into(), Some("hk-imported".into())),
                ("proposal-current".into(), Some("hk-a".into())),
            ]),
        };
        let geometry_port = GeometryPortStub::default();
        let sampling_port = SamplingPortStub::default();
        let artifact_sink = ArtifactSinkStub::default();

        let execution = SolidSolutionsWorkflowService
            .execute(
                &request,
                SolidSolutionsWorkflowPorts {
                    library_port: &library_port,
                    identity_port: &identity_port,
                    geometry_port: &geometry_port,
                    move_port: &MovePortStub,
                    evaluation_port: &sampling_port,
                    artifact_sink: &artifact_sink,
                },
            )
            .expect("workflow executes");

        let requests = sampling_port.requests.borrow();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].workflow, SamplingWorkflowIntent::SolidSolutions);
        assert_eq!(requests[0].intent, SamplingEvaluationIntent::InitialState);
        assert_eq!(requests[1].workflow, SamplingWorkflowIntent::SolidSolutions);
        assert_eq!(requests[1].intent, SamplingEvaluationIntent::SamplingStep);
        assert_eq!(requests[1].step_index, Some(0));

        assert_eq!(execution.summary.accepted_steps, 1);
        assert_eq!(execution.summary.rejected_duplicate_imported_steps, 1);
        assert_eq!(execution.summary.rejected_duplicate_current_run_steps, 1);
        assert_eq!(execution.initial_source.label, "seed");
        assert_eq!(
            execution
                .initial_evaluation
                .as_ref()
                .expect("initial evaluation")
                .energy,
            -10.0
        );
        assert_eq!(execution.state.current_run_hashkeys.len(), 1);
        assert_eq!(
            execution.state.steps[1].decision,
            SolidSolutionStepDecision::RejectedDuplicateImportedLibrary
        );
        assert_eq!(
            execution.state.steps[2].decision,
            SolidSolutionStepDecision::RejectedDuplicateCurrentRun
        );
        assert_eq!(artifact_sink.persisted.borrow().len(), 1);
    }

    #[test]
    fn workflow_skips_sampling_when_skip_evaluation_is_enabled() {
        let request = SolidSolutionsWorkflowRequest {
            initial_candidate: candidate("seed"),
            proposals: vec![candidate("proposal-a"), candidate("proposal-b")],
            workdir: PathBuf::from("/tmp/solid_solutions"),
            steps: 0,
            temperature: 300.0,
            move_mode: SolidSolutionsMoveMode::MixSolution,
            acceptance_mode: SolidSolutionsAcceptanceMode::RecordAll,
            max_exchanges: 1,
            seed: 7,
            skip_evaluation: true,
            imported_hashkeys: vec!["hk-imported".into()],
        };
        let library_port = LibraryPortStub::default();
        let identity_port = IdentityPortStub {
            map: BTreeMap::from([
                ("proposal-a".into(), Some("hk-a".into())),
                ("proposal-b".into(), Some("hk-imported".into())),
            ]),
        };
        let geometry_port = GeometryPortStub::default();
        let sampling_port = SamplingPortStub::default();
        let artifact_sink = ArtifactSinkStub::default();

        let execution = SolidSolutionsWorkflowService
            .execute(
                &request,
                SolidSolutionsWorkflowPorts {
                    library_port: &library_port,
                    identity_port: &identity_port,
                    geometry_port: &geometry_port,
                    move_port: &MovePortStub,
                    evaluation_port: &sampling_port,
                    artifact_sink: &artifact_sink,
                },
            )
            .expect("workflow executes");

        assert!(sampling_port.requests.borrow().is_empty());
        assert_eq!(execution.summary.skipped_evaluation_steps, 1);
        assert_eq!(execution.summary.rejected_duplicate_imported_steps, 1);
        assert!(execution.initial_evaluation.is_none());
        assert_eq!(
            execution.state.steps[0].decision,
            SolidSolutionStepDecision::SkippedEvaluation
        );
    }

    #[test]
    fn workflow_records_geometry_and_backend_failures_without_aborting() {
        let request = SolidSolutionsWorkflowRequest {
            initial_candidate: candidate("seed"),
            proposals: vec![candidate("bad-geometry"), candidate("bad-eval")],
            workdir: PathBuf::from("/tmp/solid_solutions"),
            steps: 0,
            temperature: 300.0,
            move_mode: SolidSolutionsMoveMode::MixSolution,
            acceptance_mode: SolidSolutionsAcceptanceMode::RecordAll,
            max_exchanges: 1,
            seed: 7,
            skip_evaluation: false,
            imported_hashkeys: Vec::new(),
        };
        let library_port = LibraryPortStub::default();
        let identity_port = IdentityPortStub {
            map: BTreeMap::from([("bad-eval".into(), Some("hk-bad".into()))]),
        };
        let geometry_port = GeometryPortStub {
            rejected_labels: BTreeSet::from(["bad-geometry".into()]),
        };
        let sampling_port = SamplingPortStub {
            requests: RefCell::new(Vec::new()),
            failures: BTreeSet::from(["bad-eval".into()]),
        };
        let artifact_sink = ArtifactSinkStub::default();

        let execution = SolidSolutionsWorkflowService
            .execute(
                &request,
                SolidSolutionsWorkflowPorts {
                    library_port: &library_port,
                    identity_port: &identity_port,
                    geometry_port: &geometry_port,
                    move_port: &MovePortStub,
                    evaluation_port: &sampling_port,
                    artifact_sink: &artifact_sink,
                },
            )
            .expect("workflow executes");

        assert_eq!(execution.summary.rejected_geometry_steps, 1);
        assert_eq!(execution.summary.evaluation_failed_steps, 1);
        assert_eq!(
            execution.state.steps[0].decision,
            SolidSolutionStepDecision::RejectedGeometry
        );
        assert_eq!(
            execution.state.steps[1].decision,
            SolidSolutionStepDecision::EvaluationFailed
        );
        assert_eq!(execution.state.current_run_hashkeys.len(), 1);
    }

    #[test]
    fn workflow_can_generate_moves_and_reject_by_acceptance_policy() {
        let request = SolidSolutionsWorkflowRequest {
            initial_candidate: candidate("seed"),
            proposals: Vec::new(),
            workdir: PathBuf::from("/tmp/solid_solutions"),
            steps: 1,
            temperature: 300.0,
            move_mode: SolidSolutionsMoveMode::MixSolution,
            acceptance_mode: SolidSolutionsAcceptanceMode::DownhillOnly,
            max_exchanges: 1,
            seed: 7,
            skip_evaluation: false,
            imported_hashkeys: Vec::new(),
        };
        let library_port = LibraryPortStub::default();
        let identity_port = IdentityPortStub::default();
        let geometry_port = GeometryPortStub::default();
        let sampling_port = SamplingPortStub::default();
        let artifact_sink = ArtifactSinkStub::default();

        let execution = SolidSolutionsWorkflowService
            .execute(
                &request,
                SolidSolutionsWorkflowPorts {
                    library_port: &library_port,
                    identity_port: &identity_port,
                    geometry_port: &geometry_port,
                    move_port: &MovePortStub,
                    evaluation_port: &sampling_port,
                    artifact_sink: &artifact_sink,
                },
            )
            .expect("workflow executes");

        assert_eq!(sampling_port.requests.borrow().len(), 2);
        assert_eq!(execution.summary.accepted_steps, 0);
        assert_eq!(execution.summary.rejected_acceptance_steps, 1);
        assert_eq!(
            execution.state.steps[0].decision,
            SolidSolutionStepDecision::RejectedAcceptance
        );
    }
}
