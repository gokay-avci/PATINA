use anyhow::{anyhow, Context, Result};
use patina_evaluator::{
    MasterTemplateLayout, MasterTemplateSection, ScottEvalOutcome, ScottScienceEvidence,
    ScottStateDigest, ScottTopologyProvenance, ScottValidityCheck, ScottValidityCheckKind,
};
use patina_runtime::ScottBackendRoutingPolicy;
pub use patina_search::ProductionDataMiningPlan as DataMiningPlan;
use patina_search::{
    ProductionAtomSpec, ProductionKernelState, WorkflowEvaluationTask, WorkflowLineage,
};
pub use patina_search::{
    ProductionBestEntry, ProductionBestSet, ProductionBestSetConfig, ProductionBestSetDecision,
    ProductionBestSetMatch,
};
use patina_types::{RestartSeedProvenance, RestartSeedSourceKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::ports::{
    ProductionAcceptedArtifactRecord, ProductionArtifactSink, ProductionEvaluationPort,
    ProductionEvaluationRequest, ProductionIdentityPort, ProductionProgressPort,
};
use super::scott_topology_types::AtomSpecRecord;
use super::topology_identity::{
    TopologyArchiveRecord, TopologyIdentityMatch, TopologyIdentityStore,
};
use super::workflow_tasks::take_queued_task;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionRunConfig {
    pub max_best_clusters: usize,
    pub best_energy_cutoff: f64,
    pub best_energy_tolerance: f64,
    pub use_top_analysis: bool,
    pub hashkey_radius_mode: String,
    pub hashkey_radius_const: f64,
    pub hkg_path: Option<PathBuf>,
    pub collapse_threshold: f64,
    pub dspecies_threshold: f64,
    pub enforce_master: bool,
    pub data_mining_enabled: bool,
    pub data_mining_replace_atoms: String,
    pub data_mining_recentre: bool,
}

impl Default for ProductionRunConfig {
    fn default() -> Self {
        Self {
            max_best_clusters: 0,
            best_energy_cutoff: f64::INFINITY,
            best_energy_tolerance: 1.0e-4,
            use_top_analysis: false,
            hashkey_radius_mode: "IR".into(),
            hashkey_radius_const: 0.0,
            hkg_path: None,
            collapse_threshold: 0.0,
            dspecies_threshold: 0.0,
            enforce_master: false,
            data_mining_enabled: false,
            data_mining_replace_atoms: String::new(),
            data_mining_recentre: false,
        }
    }
}

impl ProductionRunConfig {
    pub fn from_run_job_text(text: &str) -> Self {
        Self {
            max_best_clusters: parse_run_job_usize(text, "NUMBER_BEST_CLUSTERS").unwrap_or(0),
            best_energy_cutoff: parse_run_job_f64(text, "R_BEST_CUTOFF").unwrap_or(f64::INFINITY),
            best_energy_tolerance: parse_run_job_f64(text, "R_BEST_EDIF").unwrap_or(1.0e-4),
            use_top_analysis: parse_run_job_bool(text, "L_USE_TOP_ANALYSIS").unwrap_or(false),
            hashkey_radius_mode: parse_run_job_string(text, "C_HASHKEY_RADIUS")
                .unwrap_or_else(|| "IR".to_string()),
            hashkey_radius_const: parse_run_job_f64(text, "HASHKEY_RADIUS_CONST").unwrap_or(0.0),
            hkg_path: parse_run_job_string(text, "HKG_PATH").map(PathBuf::from),
            collapse_threshold: parse_run_job_f64(text, "COLLAPSE").unwrap_or(0.0),
            dspecies_threshold: parse_run_job_f64(text, "DSPECIES").unwrap_or(0.0),
            enforce_master: parse_run_job_bool(text, "L_ENFORCE_MASTER").unwrap_or(false),
            data_mining_enabled: parse_run_job_bool(text, "DM_FLAG").unwrap_or(false),
            data_mining_replace_atoms: parse_run_job_string(text, "DM_REPLACE_ATOMS")
                .unwrap_or_default(),
            data_mining_recentre: parse_run_job_bool(text, "DM_RECENTRE").unwrap_or(false),
        }
    }

    pub fn best_set_config(&self) -> ProductionBestSetConfig {
        ProductionBestSetConfig {
            max_best_clusters: self.max_best_clusters,
            best_energy_cutoff: self.best_energy_cutoff,
            best_energy_tolerance: self.best_energy_tolerance,
            use_top_analysis: self.use_top_analysis,
        }
    }
}

pub fn build_data_mining_plan(
    cfg: &ProductionRunConfig,
    atom_specs: &[AtomSpecRecord],
) -> Result<Option<DataMiningPlan>> {
    if !cfg.data_mining_enabled {
        return Ok(None);
    }
    let atom_specs = atom_specs
        .iter()
        .map(|record| ProductionAtomSpec {
            species: record.species.clone(),
            ionic_radius: record.ionic_radius,
        })
        .collect::<Vec<_>>();
    Ok(Some(patina_search::build_production_data_mining_plan(
        &cfg.data_mining_replace_atoms,
        cfg.data_mining_recentre,
        &atom_specs,
    )?))
}

pub fn extract_master_species(layout: &MasterTemplateLayout) -> Vec<String> {
    let atom_block = layout
        .sections
        .get(&MasterTemplateSection::AtomBlock)
        .cloned()
        .unwrap_or_default();
    patina_search::extract_master_species_from_atom_block(&atom_block)
}

pub fn enforce_master_species(
    candidate: &mut patina_types::Candidate,
    master_species: &[String],
) -> Result<()> {
    patina_search::enforce_production_master_species(candidate, master_species).map_err(Into::into)
}

pub fn apply_data_mining_to_species(species: &mut [String], plan: &DataMiningPlan) {
    patina_search::apply_production_data_mining_to_species(species, plan);
}

pub fn apply_data_mining(
    candidate: &mut patina_types::Candidate,
    plan: &DataMiningPlan,
) -> Result<()> {
    patina_search::apply_production_data_mining(candidate, plan).map_err(Into::into)
}

pub fn collect_restart_seed_paths(restart_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(restart_dir)
        .with_context(|| format!("failed to read restart dir `{}`", restart_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "xyz" | "extxyz" | "cif" | "car" | "arc" | "can"
                    )
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

pub fn read_restart_done_entries(restart_dir: &Path) -> Result<BTreeSet<String>> {
    let done_path = restart_dir.join("data-done");
    if !done_path.exists() {
        return Ok(BTreeSet::new());
    }
    let text = fs::read_to_string(&done_path)
        .with_context(|| format!("failed to read `{}`", done_path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub fn append_restart_done_entry(restart_dir: &Path, source_name: &str) -> Result<()> {
    let done_path = restart_dir.join("data-done");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&done_path)
        .with_context(|| format!("failed to open `{}` for append", done_path.display()))?;
    writeln!(file, "{source_name}")
        .with_context(|| format!("failed to append to `{}`", done_path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProductionRestartState {
    pub counter: Option<usize>,
    pub random_start: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ProductionSeedInput {
    pub source_name: Option<String>,
    pub provenance: RestartSeedProvenance,
    pub candidate: patina_types::Candidate,
}

impl ProductionSeedInput {
    pub fn inline_candidate(
        source_name: Option<String>,
        candidate: patina_types::Candidate,
    ) -> Self {
        let source_label = candidate.label.clone();
        Self {
            source_name: source_name.clone(),
            provenance: RestartSeedProvenance {
                source_kind: RestartSeedSourceKind::InlineCandidate,
                source_name,
                source_label,
            },
            candidate,
        }
    }

    pub fn restart_artifact(source_name: String, candidate: patina_types::Candidate) -> Self {
        let source_label = candidate.label.clone();
        Self {
            source_name: Some(source_name.clone()),
            provenance: RestartSeedProvenance {
                source_kind: RestartSeedSourceKind::RestartArtifact,
                source_name: Some(source_name),
                source_label,
            },
            candidate,
        }
    }

    pub fn hybrid_selection(
        source_name: String,
        source_label: String,
        candidate: patina_types::Candidate,
    ) -> Self {
        Self {
            source_name: Some(source_name.clone()),
            provenance: RestartSeedProvenance {
                source_kind: RestartSeedSourceKind::HybridGaSelection,
                source_name: Some(source_name),
                source_label,
            },
            candidate,
        }
    }

    pub fn hybrid_crossover_child(
        source_name: String,
        source_label: String,
        candidate: patina_types::Candidate,
    ) -> Self {
        Self {
            source_name: Some(source_name.clone()),
            provenance: RestartSeedProvenance {
                source_kind: RestartSeedSourceKind::HybridGaCrossover,
                source_name: Some(source_name),
                source_label,
            },
            candidate,
        }
    }
}

pub fn read_restart_state(restart_dir: &Path) -> Result<Option<ProductionRestartState>> {
    let path = restart_dir.join("restart-data");
    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut state = ProductionRestartState::default();
    for line in text.lines() {
        let trimmed = line.trim();
        let Some((lhs, rhs)) = trimmed.split_once(':') else {
            continue;
        };
        match lhs.trim() {
            "COUNTER" => {
                state.counter =
                    Some(rhs.trim().parse().with_context(|| {
                        format!("failed to parse COUNTER in `{}`", path.display())
                    })?)
            }
            "RANDOM_START" => state.random_start = parse_bool_token(rhs.trim()),
            _ => {}
        }
    }
    Ok(Some(state))
}

pub fn write_restart_state(restart_dir: &Path, state: &ProductionRestartState) -> Result<()> {
    let path = restart_dir.join("restart-data");
    let mut rendered = String::new();
    if let Some(counter) = state.counter {
        rendered.push_str(&format!("COUNTER:{counter}\n"));
    }
    if let Some(random_start) = state.random_start {
        rendered.push_str(&format!(
            "RANDOM_START:{}\n",
            if random_start { "TRUE" } else { "FALSE" }
        ));
    }
    fs::write(&path, rendered).with_context(|| format!("failed to write `{}`", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionDecisionKind {
    Accepted,
    Rejected,
    TopologySkipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionDecision {
    pub kind: ProductionDecisionKind,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct EvaluatedCandidateRecord {
    pub task: WorkflowEvaluationTask,
    pub seed_provenance: RestartSeedProvenance,
    pub workdir: PathBuf,
    pub outcome: ScottEvalOutcome,
    pub decision: ProductionDecision,
    pub final_energy: Option<f64>,
    pub final_hashkey: Option<String>,
    pub best_set_rank: Option<usize>,
    pub best_set_decision: Option<ProductionBestSetDecision>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StagedProductionCandidateSummary {
    pub candidate_label: String,
    pub workdir: PathBuf,
    pub seed_provenance: Option<RestartSeedProvenance>,
    pub input_label: String,
    pub accepted_label: Option<String>,
    pub final_stage: Option<u8>,
    pub relax_failed: bool,
    pub final_energy: Option<f64>,
    pub completed_stages: usize,
    pub failure_count: usize,
    pub rollback_to_stage: Option<u8>,
    pub last_failure_message: Option<String>,
    pub procedure_digest: ScottStateDigest,
    pub science_evidence: ScottScienceEvidence,
    pub relaxation_stages: Vec<patina_types::RelaxationStageRecord>,
    pub topology_comparison: Option<patina_types::TopologyComparisonRecord>,
    pub best_set_decision: Option<patina_types::BestSetDecisionRecord>,
    pub hashkey: Option<String>,
    pub decision: ProductionDecision,
    pub best_set_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StagedProductionSummary {
    pub candidate_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub topology_skip_count: usize,
    pub best_set_size: usize,
    pub system: String,
    pub workdir: PathBuf,
    pub routing_policy: patina_runtime::ScottBackendRoutingPolicy,
    pub production_config: ProductionRunConfig,
    pub restart_state_before: Option<ProductionRestartState>,
    pub restart_state_after: Option<ProductionRestartState>,
    pub candidates: Vec<StagedProductionCandidateSummary>,
}

#[derive(Debug, Clone)]
pub struct ProductionWorkflowTracker {
    candidate_count: usize,
    system: String,
    workdir: PathBuf,
    production_config: ProductionRunConfig,
    restart_state_before: Option<ProductionRestartState>,
    restart_state_after: Option<ProductionRestartState>,
    topology_skip_count: usize,
    candidate_summaries: Vec<StagedProductionCandidateSummary>,
    best_set: ProductionBestSet,
    topology_identity: TopologyIdentityStore,
    kernel: ProductionKernelState,
}

#[derive(Debug, Clone)]
pub struct ProductionWorkflowExecution {
    pub summary: StagedProductionSummary,
    pub best_entries: Vec<ProductionBestEntry>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProductionWorkflowService;

impl ProductionWorkflowTracker {
    pub fn new(
        candidate_count: usize,
        system: impl Into<String>,
        workdir: PathBuf,
        production_config: ProductionRunConfig,
        restart_state_before: Option<ProductionRestartState>,
    ) -> Self {
        let best_set_limit = production_config.max_best_clusters;
        Self {
            candidate_count,
            system: system.into(),
            workdir,
            production_config,
            restart_state_after: restart_state_before.clone(),
            restart_state_before,
            topology_skip_count: 0,
            candidate_summaries: Vec::with_capacity(candidate_count),
            best_set: ProductionBestSet::default(),
            topology_identity: TopologyIdentityStore::default(),
            kernel: ProductionKernelState::new(best_set_limit),
        }
    }

    pub fn begin_candidate(
        &mut self,
        candidate: patina_types::Candidate,
    ) -> Result<WorkflowEvaluationTask> {
        let lineage = WorkflowLineage::seed(candidate.label.clone());
        self.kernel.queue_candidate(candidate, lineage);
        take_queued_task(self.kernel.next_task(), "production candidate evaluation")
    }

    pub fn compare_topology_hashkey(
        &self,
        candidate_hashkey: Option<&str>,
    ) -> Option<TopologyIdentityMatch> {
        if !self.production_config.use_top_analysis {
            return None;
        }
        let hashkey = candidate_hashkey?;
        match self.topology_identity.probe(hashkey) {
            Some(TopologyIdentityMatch::BestArchive(record)) => {
                Some(TopologyIdentityMatch::BestArchive(record))
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn probe_topology_hashkey(&self, hashkey: &str) -> Option<TopologyIdentityMatch> {
        self.topology_identity.probe(hashkey)
    }

    #[cfg(test)]
    pub fn register_blacklist_hashkey(&mut self, hashkey: &str) -> Result<()> {
        self.topology_identity.register_blacklist_hashkey(hashkey)
    }

    pub fn consider_best_entry(&mut self, entry: ProductionBestEntry) -> ProductionBestSetDecision {
        let decision = self
            .best_set
            .consider(self.production_config.best_set_config(), entry);
        if self.production_config.use_top_analysis {
            self.refresh_topology_archive();
        }
        decision
    }

    pub fn best_entries(&self) -> &[ProductionBestEntry] {
        self.best_set.entries()
    }

    fn refresh_topology_archive(&mut self) {
        self.topology_identity.replace_best_archive(
            self.best_set
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    entry.hashkey.as_ref().map(|hashkey| TopologyArchiveRecord {
                        rank: index + 1,
                        candidate_label: entry.candidate_label.clone(),
                        hashkey: hashkey.clone(),
                    })
                }),
        );
    }

    pub fn update_restart_progress(&mut self, next_counter: usize) {
        self.restart_state_after = Some(ProductionRestartState {
            counter: Some(next_counter),
            random_start: Some(false),
        });
    }

    pub fn restart_state_after(&self) -> Option<&ProductionRestartState> {
        self.restart_state_after.as_ref()
    }

    pub fn record_topology_skip(
        &mut self,
        task: WorkflowEvaluationTask,
        seed_provenance: &RestartSeedProvenance,
        seed_counter: usize,
        workdir: PathBuf,
        candidate_hashkey: Option<String>,
        topology_match: TopologyIdentityMatch,
    ) {
        self.topology_skip_count += 1;
        let input_label = task.candidate.label.clone();
        let duplicate_action = topology_skip_action(&topology_match);
        let best_set_decision = topology_skip_best_set_decision(&topology_match);
        let best_set_rank = best_set_decision.as_ref().and_then(|record| record.rank);
        let matched_candidate_label = best_set_decision
            .as_ref()
            .and_then(|record| record.matched_candidate_label.clone());
        let matched_hashkey = candidate_hashkey
            .clone()
            .or_else(|| Some(topology_skip_hashkey(&topology_match).into()));
        let science_evidence = ScottScienceEvidence {
            validity_checks: Vec::new(),
            topology: Some(ScottTopologyProvenance {
                topology_analysis_requested: self.production_config.use_top_analysis,
                topology_skip: true,
                input_hashkey: candidate_hashkey.clone(),
                final_hashkey: candidate_hashkey.clone(),
            }),
            duplicate: matched_hashkey.clone().map(|hashkey| {
                patina_evaluator::ScottDuplicateProvenance {
                    reason: patina_evaluator::ScottDuplicateReason::Hashkey,
                    action: duplicate_action,
                    matched_candidate_label,
                    matched_hashkey: Some(hashkey),
                    matched_rank: best_set_rank,
                }
            }),
        };
        self.candidate_summaries
            .push(StagedProductionCandidateSummary {
                candidate_label: input_label.clone(),
                workdir,
                seed_provenance: Some(seed_provenance.clone()),
                input_label: input_label.clone(),
                accepted_label: None,
                final_stage: None,
                relax_failed: false,
                final_energy: None,
                completed_stages: 0,
                failure_count: 0,
                rollback_to_stage: None,
                last_failure_message: None,
                procedure_digest: ScottStateDigest {
                    request_id: format!("production-{}-{seed_counter}", input_label),
                    input_label,
                    current_stage: None,
                    accepted_stage: None,
                    accepted_label: None,
                    stage_attempt_count: 0,
                    failure_count: 0,
                    procedure_gate_count: 0,
                    procedure_rejection_count: 0,
                    validity_failure_count: 0,
                    relax_failed: false,
                    rollback_to_stage: None,
                    last_failure_message: None,
                    last_procedure_gate_message: None,
                    topology_skip: true,
                    input_hashkey: candidate_hashkey.clone(),
                    final_hashkey: candidate_hashkey.clone(),
                    duplicate_reason: Some(patina_evaluator::ScottDuplicateReason::Hashkey),
                    duplicate_action: Some(duplicate_action),
                    duplicate_rank: best_set_rank,
                },
                science_evidence: science_evidence.clone(),
                relaxation_stages: Vec::new(),
                topology_comparison: build_topology_comparison_record(&science_evidence),
                best_set_decision,
                hashkey: candidate_hashkey,
                decision: ProductionDecision {
                    kind: ProductionDecisionKind::TopologySkipped,
                    reason: topology_skip_reason(&topology_match).into(),
                },
                best_set_rank,
            });
    }

    pub fn record_evaluated_candidate(&mut self, record: EvaluatedCandidateRecord) {
        let EvaluatedCandidateRecord {
            task,
            seed_provenance,
            workdir,
            outcome,
            decision,
            final_energy,
            final_hashkey,
            best_set_rank,
            best_set_decision,
        } = record;
        match decision.kind {
            ProductionDecisionKind::Accepted => {
                if let Some(final_result) = outcome.final_result.as_ref() {
                    self.kernel
                        .record_accept(task.clone(), final_result.clone());
                } else {
                    self.kernel.record_rejection(
                        &task,
                        "accepted production decision without final result",
                    );
                }
            }
            ProductionDecisionKind::Rejected => {
                self.kernel.record_rejection(&task, decision.reason.clone());
            }
            ProductionDecisionKind::TopologySkipped => {}
        }

        let procedure_digest = outcome.state_digest();
        let relaxation_stages = build_relaxation_stage_records(&outcome);
        let topology_comparison = build_topology_comparison_record(&outcome.state.science_evidence);
        let best_set_decision = best_set_decision
            .as_ref()
            .map(build_best_set_decision_record);
        self.candidate_summaries
            .push(StagedProductionCandidateSummary {
                candidate_label: task.candidate.label,
                workdir,
                seed_provenance: Some(seed_provenance),
                input_label: procedure_digest.input_label.clone(),
                accepted_label: procedure_digest.accepted_label.clone(),
                final_stage: outcome.final_stage.map(|stage| stage.get()),
                relax_failed: outcome.relax_failed,
                final_energy,
                completed_stages: outcome.stage_attempt_count(),
                failure_count: outcome.failure_count(),
                rollback_to_stage: outcome.rollback_to_stage().map(|stage| stage.get()),
                last_failure_message: procedure_digest.last_failure_message.clone(),
                procedure_digest,
                science_evidence: outcome.state.science_evidence.clone(),
                relaxation_stages,
                topology_comparison,
                best_set_decision,
                hashkey: final_hashkey,
                decision,
                best_set_rank,
            });
    }

    pub fn build_summary(
        self,
        routing_policy: patina_runtime::ScottBackendRoutingPolicy,
    ) -> StagedProductionSummary {
        debug_assert_eq!(
            self.kernel.accepted().len() + self.kernel.rejected().len() + self.topology_skip_count,
            self.candidate_summaries.len()
        );
        StagedProductionSummary {
            candidate_count: self.candidate_count,
            success_count: self.kernel.accepted().len(),
            failure_count: self.kernel.rejected().len(),
            topology_skip_count: self.topology_skip_count,
            best_set_size: self.best_set.entries().len(),
            system: self.system,
            workdir: self.workdir,
            routing_policy,
            production_config: self.production_config,
            restart_state_before: self.restart_state_before,
            restart_state_after: self.restart_state_after,
            candidates: self.candidate_summaries,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let classified_count =
            self.kernel.accepted().len() + self.kernel.rejected().len() + self.topology_skip_count;
        if classified_count != self.candidate_summaries.len() {
            return Err(anyhow!(
                "production tracker invariant failed: classified_count={} but candidate_summaries={}",
                classified_count,
                self.candidate_summaries.len()
            ));
        }
        if self.candidate_summaries.len() > self.candidate_count {
            return Err(anyhow!(
                "production tracker invariant failed: candidate_summaries={} exceeds candidate_count={}",
                self.candidate_summaries.len(),
                self.candidate_count
            ));
        }
        if self.production_config.max_best_clusters > 0
            && self.best_set.entries().len() > self.production_config.max_best_clusters
        {
            return Err(anyhow!(
                "production tracker invariant failed: best_set_size={} exceeds configured max_best_clusters={}",
                self.best_set.entries().len(),
                self.production_config.max_best_clusters
            ));
        }
        Ok(())
    }
}

impl ProductionWorkflowService {
    #[allow(clippy::too_many_arguments)]
    pub fn execute<E, I, P, A>(
        &self,
        seeds: &[ProductionSeedInput],
        system: &str,
        workdir: &Path,
        production_config: &ProductionRunConfig,
        restart_state_before: Option<ProductionRestartState>,
        restart_counter_base: usize,
        fallback_routing_policy: ScottBackendRoutingPolicy,
        evaluation_port: &E,
        identity_port: &I,
        progress_port: &P,
        artifact_sink: &A,
    ) -> Result<ProductionWorkflowExecution>
    where
        E: ProductionEvaluationPort,
        I: ProductionIdentityPort,
        P: ProductionProgressPort,
        A: ProductionArtifactSink,
    {
        let mut tracker = ProductionWorkflowTracker::new(
            seeds.len(),
            system.to_string(),
            workdir.to_path_buf(),
            production_config.clone(),
            restart_state_before,
        );
        let mut routing_policy_for_report = None;

        for (index, seed) in seeds.iter().enumerate() {
            let seed_counter = restart_counter_base + index;
            let candidate_workdir = workdir.join(format!("candidate_{seed_counter:04}"));
            fs::create_dir_all(&candidate_workdir).with_context(|| {
                format!(
                    "failed to create candidate workdir `{}`",
                    candidate_workdir.display()
                )
            })?;

            let request = ProductionEvaluationRequest {
                index,
                seed_counter,
                candidate: seed.candidate.clone(),
                candidate_workdir: candidate_workdir.clone(),
            };
            let task = tracker.begin_candidate(request.candidate.clone())?;
            let candidate_hashkey = identity_port.build_input_hashkey(&request)?;

            if let Some(topology_match) =
                tracker.compare_topology_hashkey(candidate_hashkey.as_deref())
            {
                tracker.record_topology_skip(
                    task,
                    &seed.provenance,
                    seed_counter,
                    candidate_workdir,
                    candidate_hashkey,
                    topology_match,
                );
                tracker.update_restart_progress(seed_counter + 1);
                progress_port.mark_seed_consumed(
                    seed.source_name.as_deref(),
                    tracker.restart_state_after(),
                )?;
                continue;
            }

            let mut evaluated =
                evaluation_port
                    .evaluate_candidate(&request)
                    .with_context(|| {
                        format!(
                            "staged Scott production evaluation failed for candidate `{}`",
                            request.candidate.label
                        )
                    })?;
            if routing_policy_for_report.is_none() {
                routing_policy_for_report = Some(evaluated.routing_policy.clone());
            }
            evaluated.outcome.validate_consistency().with_context(|| {
                format!(
                    "staged Scott production evaluator returned inconsistent Scott state for candidate `{}`",
                    request.candidate.label
                )
            })?;

            let final_hashkey = identity_port.build_final_hashkey(
                &request,
                &evaluated,
                candidate_hashkey.as_deref(),
            )?;
            let (decision, final_energy) = classify_production_outcome(
                production_config,
                &mut evaluated.outcome,
                candidate_hashkey.clone(),
                final_hashkey.clone(),
            );
            let mut best_set_rank = None;
            let mut best_set_decision = None;

            if matches!(decision.kind, ProductionDecisionKind::Accepted) {
                if let Some(final_result) = evaluated.outcome.final_result.as_ref() {
                    best_set_decision = Some(tracker.consider_best_entry(ProductionBestEntry {
                        candidate_label: request.candidate.label.clone(),
                        final_stage: evaluated.outcome.final_stage.map(|stage| stage.get()),
                        energy: final_result.energy,
                        hashkey: final_hashkey.clone(),
                        relaxed_candidate: final_result.relaxed_candidate.clone(),
                    }));
                    if let Some(ProductionBestSetDecision::MatchedExisting(existing_match)) =
                        best_set_decision.as_ref()
                    {
                        evaluated.outcome.state.science_evidence.duplicate =
                            Some(patina_evaluator::ScottDuplicateProvenance {
                                reason: patina_evaluator::ScottDuplicateReason::Hashkey,
                                action:
                                    patina_evaluator::ScottDuplicateAction::MatchedExistingBestSet,
                                matched_candidate_label: Some(
                                    existing_match.entry.candidate_label.clone(),
                                ),
                                matched_hashkey: existing_match.entry.hashkey.clone(),
                                matched_rank: Some(existing_match.rank),
                            });
                    }
                    best_set_rank = best_set_decision
                        .as_ref()
                        .and_then(ProductionBestSetDecision::rank);
                    artifact_sink.persist_accepted_candidate(
                        &ProductionAcceptedArtifactRecord {
                            index,
                            system: system.to_string(),
                            candidate: request.candidate.clone(),
                            candidate_workdir: request.candidate_workdir.clone(),
                            default_backend: evaluated.default_backend,
                            routing_policy: evaluated.routing_policy.clone(),
                            procedure_plan: evaluated.procedure_plan.clone(),
                            outcome: evaluated.outcome.clone(),
                            decision: decision.clone(),
                            final_hashkey: final_hashkey.clone(),
                            best_set_rank,
                        },
                    )?;
                }
            }

            tracker.record_evaluated_candidate(EvaluatedCandidateRecord {
                task,
                seed_provenance: seed.provenance.clone(),
                workdir: request.candidate_workdir,
                outcome: evaluated.outcome,
                decision,
                final_energy,
                final_hashkey,
                best_set_rank,
                best_set_decision,
            });
            tracker.update_restart_progress(seed_counter + 1);
            progress_port
                .mark_seed_consumed(seed.source_name.as_deref(), tracker.restart_state_after())?;
        }

        tracker.validate()?;
        let best_entries = tracker.best_entries().to_vec();
        let summary =
            tracker.build_summary(routing_policy_for_report.unwrap_or(fallback_routing_policy));
        Ok(ProductionWorkflowExecution {
            summary,
            best_entries,
        })
    }
}

pub fn classify_production_outcome(
    cfg: &ProductionRunConfig,
    outcome: &mut ScottEvalOutcome,
    input_hashkey: Option<String>,
    final_hashkey: Option<String>,
) -> (ProductionDecision, Option<f64>) {
    outcome
        .state
        .set_topology_provenance(ScottTopologyProvenance {
            topology_analysis_requested: cfg.use_top_analysis,
            topology_skip: false,
            input_hashkey,
            final_hashkey: final_hashkey.clone(),
        });

    let Some(result) = outcome.final_result.as_ref() else {
        return (
            ProductionDecision {
                kind: ProductionDecisionKind::Rejected,
                reason: outcome
                    .last_failure_message()
                    .map(|message| format!("Energy not defined! Last Scott failure: {message}"))
                    .unwrap_or_else(|| "Energy not defined!".into()),
            },
            None,
        );
    };

    let relaxed = &result.relaxed_candidate;
    if outcome.relax_failed {
        return (
            ProductionDecision {
                kind: ProductionDecisionKind::Rejected,
                reason: outcome
                    .last_failure_message()
                    .map(|message| {
                        format!("Relaxation failed in the staged evaluator procedure: {message}")
                    })
                    .unwrap_or_else(|| {
                        "Relaxation failed in the staged evaluator procedure".into()
                    }),
            },
            Some(result.energy),
        );
    }
    if cfg.collapse_threshold > 0.0 {
        let minimum_pair_distance = minimum_pair_distance(relaxed, false);
        let passed = minimum_pair_distance
            .map(|distance| distance >= cfg.collapse_threshold)
            .unwrap_or(true);
        outcome.state.record_validity_check(ScottValidityCheck {
            kind: ScottValidityCheckKind::Collapse,
            passed,
            message: if passed {
                "collapse threshold satisfied".into()
            } else {
                "cluster collapsed below configured threshold".into()
            },
            threshold: Some(cfg.collapse_threshold),
            observed_value: minimum_pair_distance,
        });
        if !passed {
            return (
                ProductionDecision {
                    kind: ProductionDecisionKind::Rejected,
                    reason: "Cluster has collapsed!".into(),
                },
                Some(result.energy),
            );
        }
    }
    if cfg.dspecies_threshold > 0.0 {
        let minimum_same_species_distance = minimum_pair_distance(relaxed, true);
        let passed = minimum_same_species_distance
            .map(|distance| distance >= cfg.dspecies_threshold)
            .unwrap_or(true);
        outcome.state.record_validity_check(ScottValidityCheck {
            kind: ScottValidityCheckKind::LikeSpeciesTransfer,
            passed,
            message: if passed {
                "like-species transfer threshold satisfied".into()
            } else {
                "like-species pair violates configured transfer threshold".into()
            },
            threshold: Some(cfg.dspecies_threshold),
            observed_value: minimum_same_species_distance,
        });
        if !passed {
            return (
                ProductionDecision {
                    kind: ProductionDecisionKind::Rejected,
                    reason: "Transfer of electrons!".into(),
                },
                Some(result.energy),
            );
        }
    }
    if cfg.use_top_analysis {
        let passed = final_hashkey.is_some();
        outcome.state.record_validity_check(ScottValidityCheck {
            kind: ScottValidityCheckKind::HashkeyAvailable,
            passed,
            message: if passed {
                "topology hashkey generated".into()
            } else {
                "topology hashkey required but not available".into()
            },
            threshold: None,
            observed_value: None,
        });
        if !passed {
            return (
                ProductionDecision {
                    kind: ProductionDecisionKind::Rejected,
                    reason: "Topological analysis was enabled but no hashkey could be generated"
                        .into(),
                },
                Some(result.energy),
            );
        }
    }

    (
        ProductionDecision {
            kind: ProductionDecisionKind::Accepted,
            reason: "accepted".into(),
        },
        Some(result.energy),
    )
}

fn build_relaxation_stage_records(
    outcome: &ScottEvalOutcome,
) -> Vec<patina_types::RelaxationStageRecord> {
    outcome
        .state
        .stage_evaluations
        .iter()
        .map(|entry| patina_types::RelaxationStageRecord {
            stage: entry.status.stage.get(),
            attempt: entry.status.attempt,
            backend_status: map_relaxation_backend_status(entry.status.backend_status),
            convergence: map_relaxation_convergence(entry.status.convergence),
            accepted_stage: outcome.state.accepted_stage == Some(entry.status.stage)
                && matches!(
                    entry.status.convergence,
                    patina_evaluator::NormalizedConvergence::Accepted
                ),
            energy: entry.status.energy,
            gnorm: entry.status.gnorm,
            relaxed_label: entry.status.relaxed_label.clone(),
            primary_output_path: entry.status.primary_output_path.clone(),
        })
        .collect()
}

fn build_topology_comparison_record(
    science_evidence: &ScottScienceEvidence,
) -> Option<patina_types::TopologyComparisonRecord> {
    let topology = science_evidence.topology.as_ref()?;
    let duplicate = science_evidence.duplicate.as_ref();
    Some(patina_types::TopologyComparisonRecord {
        analysis_requested: topology.topology_analysis_requested,
        topology_skip: topology.topology_skip,
        input_hashkey: topology.input_hashkey.clone(),
        final_hashkey: topology.final_hashkey.clone(),
        reason: duplicate.map(|record| map_topology_comparison_reason(record.reason)),
        action: duplicate.map(|record| map_topology_comparison_action(record.action)),
        matched_candidate_label: duplicate
            .and_then(|record| record.matched_candidate_label.clone()),
        matched_hashkey: duplicate.and_then(|record| record.matched_hashkey.clone()),
        matched_rank: duplicate.and_then(|record| record.matched_rank),
    })
}

fn build_best_set_decision_record(
    decision: &ProductionBestSetDecision,
) -> patina_types::BestSetDecisionRecord {
    match decision {
        ProductionBestSetDecision::Rejected => patina_types::BestSetDecisionRecord {
            kind: patina_types::BestSetDecisionKind::Rejected,
            rank: None,
            matched_candidate_label: None,
            matched_hashkey: None,
        },
        ProductionBestSetDecision::MatchedExisting(existing_match) => {
            build_best_set_match_record(existing_match)
        }
        ProductionBestSetDecision::Inserted { rank } => patina_types::BestSetDecisionRecord {
            kind: patina_types::BestSetDecisionKind::Inserted,
            rank: Some(*rank),
            matched_candidate_label: None,
            matched_hashkey: None,
        },
    }
}

fn build_best_set_match_record(
    existing_match: &ProductionBestSetMatch,
) -> patina_types::BestSetDecisionRecord {
    patina_types::BestSetDecisionRecord {
        kind: patina_types::BestSetDecisionKind::MatchedExisting,
        rank: Some(existing_match.rank),
        matched_candidate_label: Some(existing_match.entry.candidate_label.clone()),
        matched_hashkey: existing_match.entry.hashkey.clone(),
    }
}

fn build_archive_match_record(
    record: &TopologyArchiveRecord,
) -> patina_types::BestSetDecisionRecord {
    patina_types::BestSetDecisionRecord {
        kind: patina_types::BestSetDecisionKind::MatchedExisting,
        rank: Some(record.rank),
        matched_candidate_label: Some(record.candidate_label.clone()),
        matched_hashkey: Some(record.hashkey.clone()),
    }
}

fn topology_skip_action(
    topology_match: &TopologyIdentityMatch,
) -> patina_evaluator::ScottDuplicateAction {
    match topology_match {
        TopologyIdentityMatch::BestArchive(_) => {
            patina_evaluator::ScottDuplicateAction::MatchedBestArchiveInputHashkey
        }
        TopologyIdentityMatch::Blacklist { .. } => {
            patina_evaluator::ScottDuplicateAction::MatchedBlacklist
        }
        TopologyIdentityMatch::ImportedLibrary { .. } => {
            patina_evaluator::ScottDuplicateAction::MatchedImportedLibrary
        }
        TopologyIdentityMatch::CurrentRunHistory { .. } => {
            patina_evaluator::ScottDuplicateAction::MatchedCurrentRunHistory
        }
    }
}

fn topology_skip_best_set_decision(
    topology_match: &TopologyIdentityMatch,
) -> Option<patina_types::BestSetDecisionRecord> {
    match topology_match {
        TopologyIdentityMatch::BestArchive(record) => Some(build_archive_match_record(record)),
        TopologyIdentityMatch::Blacklist { .. }
        | TopologyIdentityMatch::ImportedLibrary { .. }
        | TopologyIdentityMatch::CurrentRunHistory { .. } => None,
    }
}

fn topology_skip_hashkey(topology_match: &TopologyIdentityMatch) -> &str {
    match topology_match {
        TopologyIdentityMatch::BestArchive(record) => record.hashkey.as_str(),
        TopologyIdentityMatch::Blacklist { hashkey }
        | TopologyIdentityMatch::ImportedLibrary { hashkey, .. }
        | TopologyIdentityMatch::CurrentRunHistory { hashkey, .. } => hashkey.as_str(),
    }
}

fn topology_skip_reason(topology_match: &TopologyIdentityMatch) -> &'static str {
    match topology_match {
        TopologyIdentityMatch::BestArchive(_) => {
            "input hashkey already matches the current best archive"
        }
        TopologyIdentityMatch::Blacklist { .. } => {
            "input hashkey is present in the topology blacklist"
        }
        TopologyIdentityMatch::ImportedLibrary { .. } => {
            "input hashkey already appears in the imported topology library"
        }
        TopologyIdentityMatch::CurrentRunHistory { .. } => {
            "input hashkey already appears in current-run topology history"
        }
    }
}

fn map_relaxation_backend_status(
    status: patina_evaluator::StageBackendStatus,
) -> patina_types::RelaxationBackendStatusRecord {
    match status {
        patina_evaluator::StageBackendStatus::Converged => {
            patina_types::RelaxationBackendStatusRecord::Converged
        }
        patina_evaluator::StageBackendStatus::ConvergedWithGradientWarning => {
            patina_types::RelaxationBackendStatusRecord::ConvergedWithGradientWarning
        }
        patina_evaluator::StageBackendStatus::RequiresMoreCycles => {
            patina_types::RelaxationBackendStatusRecord::RequiresMoreCycles
        }
        patina_evaluator::StageBackendStatus::Crashed => {
            patina_types::RelaxationBackendStatusRecord::Crashed
        }
        patina_evaluator::StageBackendStatus::InvalidEnergy => {
            patina_types::RelaxationBackendStatusRecord::InvalidEnergy
        }
        patina_evaluator::StageBackendStatus::RejectedByProcedure => {
            patina_types::RelaxationBackendStatusRecord::RejectedByProcedure
        }
    }
}

fn map_relaxation_convergence(
    convergence: patina_evaluator::NormalizedConvergence,
) -> patina_types::RelaxationConvergenceRecord {
    match convergence {
        patina_evaluator::NormalizedConvergence::Accepted => {
            patina_types::RelaxationConvergenceRecord::Accepted
        }
        patina_evaluator::NormalizedConvergence::RetryableFailure => {
            patina_types::RelaxationConvergenceRecord::RetryableFailure
        }
        patina_evaluator::NormalizedConvergence::FinalFailure => {
            patina_types::RelaxationConvergenceRecord::FinalFailure
        }
    }
}

fn map_topology_comparison_reason(
    reason: patina_evaluator::ScottDuplicateReason,
) -> patina_types::TopologyComparisonReasonRecord {
    match reason {
        patina_evaluator::ScottDuplicateReason::Hashkey => {
            patina_types::TopologyComparisonReasonRecord::Hashkey
        }
        patina_evaluator::ScottDuplicateReason::Pmoi => {
            patina_types::TopologyComparisonReasonRecord::Pmoi
        }
        patina_evaluator::ScottDuplicateReason::EnergyTolerance => {
            patina_types::TopologyComparisonReasonRecord::EnergyTolerance
        }
    }
}

fn map_topology_comparison_action(
    action: patina_evaluator::ScottDuplicateAction,
) -> patina_types::TopologyComparisonActionRecord {
    match action {
        patina_evaluator::ScottDuplicateAction::MatchedBestArchiveInputHashkey => {
            patina_types::TopologyComparisonActionRecord::MatchedBestArchiveInputHashkey
        }
        patina_evaluator::ScottDuplicateAction::MatchedBlacklist => {
            patina_types::TopologyComparisonActionRecord::MatchedBlacklist
        }
        patina_evaluator::ScottDuplicateAction::MatchedImportedLibrary => {
            patina_types::TopologyComparisonActionRecord::MatchedImportedLibrary
        }
        patina_evaluator::ScottDuplicateAction::MatchedCurrentRunHistory => {
            patina_types::TopologyComparisonActionRecord::MatchedCurrentRunHistory
        }
        patina_evaluator::ScottDuplicateAction::MatchedExistingBestSet => {
            patina_types::TopologyComparisonActionRecord::MatchedExistingBestSet
        }
    }
}

pub fn write_staged_production_artifacts(
    run_dir: &Path,
    summary: &StagedProductionSummary,
    best_entries: &[ProductionBestEntry],
) -> Result<()> {
    let raw_dir = run_dir.join("raw");
    let outputs_dir = run_dir.join("outputs");
    let top_structures_dir = outputs_dir.join("top_structures");
    fs::create_dir_all(&raw_dir)
        .with_context(|| format!("failed to create raw directory `{}`", raw_dir.display()))?;
    fs::create_dir_all(&top_structures_dir).with_context(|| {
        format!(
            "failed to create top structures directory `{}`",
            top_structures_dir.display()
        )
    })?;

    let manifest = serde_json::json!({
        "run_name": run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("unnamed-staged-production"),
        "workflow_owner": "scott_staged_production",
        "workflow_scope": "staged Scott production-style batch evaluation",
        "system": summary.system,
        "candidate_count": summary.candidate_count,
        "success_count": summary.success_count,
        "failure_count": summary.failure_count,
        "topology_skip_count": summary.topology_skip_count,
        "best_set_size": summary.best_set_size,
        "routing_policy": &summary.routing_policy,
        "production_config": &summary.production_config,
        "artifacts": {
            "summary": "raw/staged_production_summary.json",
            "prod_statistics": "raw/prodStatistics.csv",
            "best_set": "outputs/top_structures/",
            "best_set_summary": "raw/production_best_set.json"
        }
    });
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)
            .context("failed to serialize staged production manifest")?,
    )
    .with_context(|| {
        format!(
            "failed to write staged production manifest in `{}`",
            run_dir.display()
        )
    })?;

    fs::write(
        raw_dir.join("staged_production_summary.json"),
        serde_json::to_string_pretty(summary)
            .context("failed to serialize staged production summary")?,
    )
    .with_context(|| {
        format!(
            "failed to write staged production summary in `{}`",
            raw_dir.display()
        )
    })?;

    let stats_path = raw_dir.join("prodStatistics.csv");
    let mut stats = fs::File::create(&stats_path)
        .with_context(|| format!("failed to create `{}`", stats_path.display()))?;
    writeln!(stats, "ClusterNo,ClusterID,hashkey,edefn,Energy")
        .context("failed to write prodStatistics header")?;
    for (index, candidate) in summary.candidates.iter().enumerate() {
        let energy = candidate.final_energy.unwrap_or(0.0);
        let stage = candidate.final_stage.unwrap_or(0);
        writeln!(
            stats,
            "{},{},{},{},{:.10}",
            index + 1,
            candidate.candidate_label,
            candidate.hashkey.as_deref().unwrap_or(""),
            stage,
            energy
        )
        .context("failed to write prodStatistics row")?;
    }

    let best_summary = best_entries
        .iter()
        .enumerate()
        .map(|(rank, entry)| {
            serde_json::json!({
                "rank": rank + 1,
                "candidate_label": entry.candidate_label,
                "final_stage": entry.final_stage,
                "energy": entry.energy,
                "hashkey": entry.hashkey,
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        raw_dir.join("production_best_set.json"),
        serde_json::to_string_pretty(&best_summary)
            .context("failed to serialize production best-set summary")?,
    )
    .with_context(|| {
        format!(
            "failed to write production best-set summary in `{}`",
            raw_dir.display()
        )
    })?;

    for (rank, entry) in best_entries.iter().take(20).enumerate() {
        write_candidate_xyz(
            &entry.relaxed_candidate,
            &top_structures_dir.join(format!(
                "rank_{:02}_stage_{:02}_{}.xyz",
                rank + 1,
                entry.final_stage.unwrap_or(0),
                sanitize_label(&entry.candidate_label)
            )),
            Some(entry.energy),
        )?;
    }

    Ok(())
}

fn minimum_pair_distance(
    candidate: &patina_types::Candidate,
    same_species_only: bool,
) -> Option<f64> {
    let mut minimum_distance: Option<f64> = None;
    for i in 0..candidate.fractional_coords.len() {
        for j in (i + 1)..candidate.fractional_coords.len() {
            if same_species_only && candidate.species[i] != candidate.species[j] {
                continue;
            }
            let distance = pair_distance(candidate, i, j);
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

fn write_candidate_xyz(
    candidate: &patina_types::Candidate,
    output_path: &Path,
    energy: Option<f64>,
) -> Result<()> {
    let mut rendered = String::new();
    rendered.push_str(&format!("{}\n", candidate.species.len()));
    match energy {
        Some(value) => {
            rendered.push_str(&format!("label={} energy={value:.10}\n", candidate.label))
        }
        None => rendered.push_str(&format!("label={}\n", candidate.label)),
    }
    for (species, coords) in candidate
        .species
        .iter()
        .zip(candidate.fractional_coords.iter())
    {
        rendered.push_str(&format!(
            "{} {:.10} {:.10} {:.10}\n",
            species, coords[0], coords[1], coords[2]
        ));
    }
    fs::write(output_path, rendered)
        .with_context(|| format!("failed to write `{}`", output_path.display()))
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn parse_run_job_string(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        let (lhs, rhs) = trimmed.split_once(':')?;
        if lhs.trim() != key {
            return None;
        }
        Some(rhs.trim().trim_matches('\'').to_string())
    })
}

fn parse_run_job_f64(text: &str, key: &str) -> Option<f64> {
    parse_run_job_string(text, key)?.parse().ok()
}

fn parse_run_job_usize(text: &str, key: &str) -> Option<usize> {
    parse_run_job_string(text, key)?.parse().ok()
}

fn parse_run_job_bool(text: &str, key: &str) -> Option<bool> {
    parse_bool_token(&parse_run_job_string(text, key)?)
}

fn parse_bool_token(raw: &str) -> Option<bool> {
    match raw.to_ascii_uppercase().as_str() {
        ".TRUE." | "TRUE" | "T" | "1" => Some(true),
        ".FALSE." | "FALSE" | "F" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::application::topology_identity::TopologyIdentityMatch;
    use crate::AtomSpecRecord;

    use super::{
        append_restart_done_entry, apply_data_mining, apply_data_mining_to_species,
        build_data_mining_plan, classify_production_outcome, collect_restart_seed_paths,
        enforce_master_species, extract_master_species, read_restart_done_entries,
        read_restart_state, write_restart_state, DataMiningPlan, ProductionBestEntry,
        ProductionBestSet, ProductionBestSetDecision, ProductionDecisionKind,
        ProductionRestartState, ProductionRunConfig, ProductionSeedInput,
        ProductionWorkflowService, ProductionWorkflowTracker,
    };
    use camino::Utf8PathBuf;
    use indexmap::IndexMap;
    use patina_evaluator::MasterTemplateLayout;
    use patina_evaluator::{
        FinalStageFailurePolicy, NormalizedConvergence, ScottBackendMode, ScottEvalOutcome,
        ScottEvaluationState, ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode,
        ScottProcedureIntent, ScottProcedurePlan, ScottStageStatus, ScottValidityCheckKind,
        StageBackendStatus, StageEngine, StageIndex, StagePlan, StageSelection,
    };
    use patina_runtime::ScottBackendRoutingPolicy;
    use patina_types::{Candidate, EvalResult};
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::tempdir;

    fn atom_specs() -> Vec<AtomSpecRecord> {
        vec![
            AtomSpecRecord {
                species: "Mg".into(),
                covalent_radius: 1.4,
                ionic_radius: 0.72,
            },
            AtomSpecRecord {
                species: "O".into(),
                covalent_radius: 0.66,
                ionic_radius: 1.4,
            },
            AtomSpecRecord {
                species: "Si".into(),
                covalent_radius: 1.11,
                ionic_radius: 0.4,
            },
            AtomSpecRecord {
                species: "F".into(),
                covalent_radius: 0.57,
                ionic_radius: 1.33,
            },
        ]
    }

    fn candidate(label: &str, distance: f64) -> Candidate {
        Candidate {
            species: vec!["Mg".into(), "Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [distance, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: label.into(),
        }
    }

    fn accepted_outcome(label: &str, distance: f64, energy: f64) -> ScottEvalOutcome {
        let relaxed_candidate = candidate(label, distance);
        let result = EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; 2],
            relaxed_candidate: relaxed_candidate.clone(),
            converged: true,
            wall_time: Duration::from_secs(1),
        };
        let mut state = ScottEvaluationState::new(
            "test-request".into(),
            relaxed_candidate.clone(),
            StageIndex(1),
        );
        state.record_stage_evaluation(
            ScottStageStatus {
                stage: StageIndex(1),
                attempt: 1,
                backend_status: StageBackendStatus::Converged,
                convergence: NormalizedConvergence::Accepted,
                energy: Some(energy),
                gnorm: Some(0.0),
                relaxed_label: Some(relaxed_candidate.label.clone()),
                primary_output_path: Some("/tmp/test.got".into()),
            },
            Some(result.clone()),
        );
        state.set_accepted_result(StageIndex(1), result.clone());
        let provenance = state.provenance();
        ScottEvalOutcome {
            final_result: Some(result),
            final_stage: Some(StageIndex(1)),
            relax_failed: false,
            provenance,
            state,
        }
    }

    fn dummy_routing_policy() -> ScottBackendRoutingPolicy {
        ScottBackendRoutingPolicy {
            default_backend: ScottBackendMode::Gulp,
            stage_overrides: IndexMap::new(),
        }
    }

    fn dummy_procedure_plan() -> ScottProcedurePlan {
        ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template: Utf8PathBuf::from("/tmp/Master.gin"),
                atoms_in: None,
                work_root: Utf8PathBuf::from("/tmp"),
                settings: ScottEvaluatorSettings {
                    backend_mode: ScottBackendMode::Gulp,
                    procedure_intent: ScottProcedureIntent::ProductionRun,
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

    struct FakeEvaluationPort {
        input_hashkeys: Vec<Option<String>>,
        final_hashkeys: Vec<Option<String>>,
        outcomes: Vec<ScottEvalOutcome>,
        evaluated_indices: RefCell<Vec<usize>>,
    }

    impl crate::application::ports::ProductionEvaluationPort for FakeEvaluationPort {
        fn evaluate_candidate(
            &self,
            request: &crate::application::ports::ProductionEvaluationRequest,
        ) -> anyhow::Result<crate::application::ports::ProductionEvaluationOutput> {
            self.evaluated_indices.borrow_mut().push(request.index);
            Ok(crate::application::ports::ProductionEvaluationOutput {
                default_backend: ScottBackendMode::Gulp,
                routing_policy: dummy_routing_policy(),
                procedure_plan: dummy_procedure_plan(),
                outcome: self.outcomes[request.index].clone(),
            })
        }
    }

    impl crate::application::ports::ProductionIdentityPort for FakeEvaluationPort {
        fn build_input_hashkey(
            &self,
            request: &crate::application::ports::ProductionEvaluationRequest,
        ) -> anyhow::Result<Option<String>> {
            Ok(self.input_hashkeys[request.index].clone())
        }

        fn build_final_hashkey(
            &self,
            request: &crate::application::ports::ProductionEvaluationRequest,
            _evaluated: &crate::application::ports::ProductionEvaluationOutput,
            fallback_hashkey: Option<&str>,
        ) -> anyhow::Result<Option<String>> {
            Ok(self.final_hashkeys[request.index]
                .clone()
                .or_else(|| fallback_hashkey.map(ToOwned::to_owned)))
        }
    }

    #[derive(Default)]
    struct FakeProgressPort {
        consumed: RefCell<Vec<(Option<String>, Option<usize>)>>,
    }

    impl crate::application::ports::ProductionProgressPort for FakeProgressPort {
        fn mark_seed_consumed(
            &self,
            source_name: Option<&str>,
            restart_state: Option<&ProductionRestartState>,
        ) -> anyhow::Result<()> {
            self.consumed.borrow_mut().push((
                source_name.map(ToOwned::to_owned),
                restart_state.and_then(|state| state.counter),
            ));
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeArtifactSink {
        accepted_indices: RefCell<Vec<usize>>,
    }

    impl crate::application::ports::ProductionArtifactSink for FakeArtifactSink {
        fn persist_accepted_candidate(
            &self,
            artifact: &crate::application::ports::ProductionAcceptedArtifactRecord,
        ) -> anyhow::Result<()> {
            self.accepted_indices.borrow_mut().push(artifact.index);
            Ok(())
        }
    }

    #[test]
    fn parses_production_controls_from_run_job() {
        let config = ProductionRunConfig::from_run_job_text(
            r#"
            NUMBER_BEST_CLUSTERS:20
            R_BEST_CUTOFF:-10.0
            R_BEST_EDIF:0.0002
            L_USE_TOP_ANALYSIS:.TRUE.
            C_HASHKEY_RADIUS:IR
            HASHKEY_RADIUS_CONST:0.4
            COLLAPSE:1.5
            DSPECIES:1.2
            L_ENFORCE_MASTER:.TRUE.
            DM_FLAG:.TRUE.
            DM_REPLACE_ATOMS:'Mg-Si;O-F'
            DM_RECENTRE:.TRUE.
            "#,
        );

        assert_eq!(config.max_best_clusters, 20);
        assert_eq!(config.best_energy_cutoff, -10.0);
        assert_eq!(config.best_energy_tolerance, 0.0002);
        assert!(config.use_top_analysis);
        assert_eq!(config.hashkey_radius_mode, "IR");
        assert_eq!(config.hashkey_radius_const, 0.4);
        assert_eq!(config.hkg_path, None);
        assert_eq!(config.collapse_threshold, 1.5);
        assert_eq!(config.dspecies_threshold, 1.2);
        assert!(config.enforce_master);
        assert!(config.data_mining_enabled);
        assert_eq!(config.data_mining_replace_atoms, "Mg-Si;O-F");
        assert!(config.data_mining_recentre);
    }

    #[test]
    fn extracts_master_species_from_template_atom_block() {
        let layout = MasterTemplateLayout::parse(
            r#"
            opti
            cartesian
            Mg core 0.0 0.0 0.0
            O core 1.0 1.0 1.0
            extra
            species
            Mg core 2.0
            O core -2.0
            "#,
        )
        .unwrap();

        assert_eq!(extract_master_species(&layout), vec!["Mg", "O"]);
    }

    #[test]
    fn enforce_master_species_replaces_candidate_symbols() {
        let mut candidate = Candidate {
            species: vec!["Si".into(), "F".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "seed".into(),
        };

        enforce_master_species(&mut candidate, &["Mg".into(), "O".into()]).unwrap();
        assert_eq!(candidate.species, vec!["Mg", "O"]);
    }

    #[test]
    fn data_mining_plan_matches_native_radius_ratio() {
        let cfg = ProductionRunConfig {
            data_mining_enabled: true,
            data_mining_replace_atoms: "Mg-Si;O-F".into(),
            data_mining_recentre: true,
            ..ProductionRunConfig::default()
        };

        let plan = build_data_mining_plan(&cfg, &atom_specs())
            .unwrap()
            .unwrap();
        assert_eq!(
            plan.replacements,
            vec![
                ("Mg".to_string(), "Si".to_string()),
                ("O".to_string(), "F".to_string())
            ]
        );
        assert!((plan.coordinate_scale - ((0.4 + 1.33) / (0.72 + 1.4))).abs() < 1.0e-12);
        assert!(plan.recentre);
    }

    #[test]
    fn data_mining_updates_master_species_in_place() {
        let plan = DataMiningPlan {
            replacements: vec![("Mg".into(), "Si".into()), ("O".into(), "F".into())],
            coordinate_scale: 1.0,
            recentre: false,
        };
        let mut species = vec!["Mg".into(), "O".into(), "Mg".into()];

        apply_data_mining_to_species(&mut species, &plan);
        assert_eq!(species, vec!["Si", "F", "Si"]);
    }

    #[test]
    fn data_mining_replaces_species_and_rescales_cluster() {
        let mut candidate = Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "seed".into(),
        };
        let plan = DataMiningPlan {
            replacements: vec![("Mg".into(), "Si".into()), ("O".into(), "F".into())],
            coordinate_scale: 0.5,
            recentre: true,
        };

        apply_data_mining(&mut candidate, &plan).unwrap();
        assert_eq!(candidate.species, vec!["Si", "F"]);
        assert_eq!(candidate.fractional_coords[0], [-0.5, 0.0, 0.0]);
        assert_eq!(candidate.fractional_coords[1], [0.5, 0.0, 0.0]);
    }

    #[test]
    fn restart_seed_paths_and_done_entries_round_trip() {
        let dir = tempdir().unwrap();
        let xyz = dir.path().join("a.xyz");
        let car = dir.path().join("a.car");
        let arc = dir.path().join("a.arc");
        let cif = dir.path().join("b.cif");
        let can = dir.path().join("b.can");
        let junk = dir.path().join("note.txt");
        std::fs::write(&xyz, "1\ncomment\nMg 0 0 0\n").unwrap();
        std::fs::write(
            &car,
            "!BIOSYM archive 3\nPBC=OFF\nseed\n!DATE\nMg1 0 0 0 XXXX 1 xx Mg 0.000\nend\nend\n",
        )
        .unwrap();
        std::fs::write(&arc, "!BIOSYM archive 3\nPBC=ON\n0.0\n!DATE\nPBC 4 4 4 90 90 90 (unknown)\nMg 0 0 0 CORE 1 Mg Mg 0.0\nend\nend\n").unwrap();
        std::fs::write(&cif, "data_x\n_cell_length_a 4\n_cell_length_b 4\n_cell_length_c 4\n_cell_angle_alpha 90\n_cell_angle_beta 90\n_cell_angle_gamma 90\nloop_\n_atom_site_type_symbol\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\nMg 0 0 0\n").unwrap();
        std::fs::write(
            &can,
            "seed\n1 0.0\n-10.0\n0.1\n0 0 0\n0 0 0\n1\nMg\nc\n0 0 0\n0 0 0\n1\n0\n0\n",
        )
        .unwrap();
        std::fs::write(&junk, "ignore").unwrap();

        let seeds = collect_restart_seed_paths(dir.path()).unwrap();
        assert_eq!(seeds.len(), 5);
        let seed_names = seeds
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            seed_names,
            vec!["a.arc", "a.car", "a.xyz", "b.can", "b.cif"]
        );

        append_restart_done_entry(dir.path(), "a.xyz").unwrap();
        append_restart_done_entry(dir.path(), "b.cif").unwrap();
        let done = read_restart_done_entries(dir.path()).unwrap();
        assert!(done.contains("a.xyz"));
        assert!(done.contains("b.cif"));
    }

    #[test]
    fn restart_state_round_trips_counter_and_random_start() {
        let dir = tempdir().unwrap();
        let state = ProductionRestartState {
            counter: Some(17),
            random_start: Some(false),
        };

        write_restart_state(dir.path(), &state).unwrap();
        let restored = read_restart_state(dir.path()).unwrap();

        assert_eq!(restored, Some(state));
    }

    #[test]
    fn collapse_gate_rejects_candidate() {
        let cfg = ProductionRunConfig {
            collapse_threshold: 1.5,
            ..ProductionRunConfig::default()
        };
        let mut outcome = accepted_outcome("bad", 1.0, -12.0);
        let (decision, energy) = classify_production_outcome(&cfg, &mut outcome, None, None);

        assert_eq!(decision.kind, ProductionDecisionKind::Rejected);
        assert_eq!(decision.reason, "Cluster has collapsed!");
        assert_eq!(energy, Some(-12.0));
        assert_eq!(outcome.state.science_evidence.validity_checks.len(), 1);
        assert_eq!(
            outcome.state.science_evidence.validity_checks[0].kind,
            ScottValidityCheckKind::Collapse
        );
        assert!(!outcome.state.science_evidence.validity_checks[0].passed);
    }

    #[test]
    fn dspecies_gate_rejects_like_species_transfer() {
        let cfg = ProductionRunConfig {
            dspecies_threshold: 1.5,
            ..ProductionRunConfig::default()
        };
        let mut outcome = accepted_outcome("bad", 1.0, -12.0);
        let (decision, _) = classify_production_outcome(&cfg, &mut outcome, None, None);

        assert_eq!(decision.kind, ProductionDecisionKind::Rejected);
        assert_eq!(decision.reason, "Transfer of electrons!");
        assert_eq!(outcome.state.science_evidence.validity_checks.len(), 1);
        assert_eq!(
            outcome.state.science_evidence.validity_checks[0].kind,
            ScottValidityCheckKind::LikeSpeciesTransfer
        );
    }

    #[test]
    fn topology_hashkey_evidence_is_recorded_for_accepted_candidate() {
        let cfg = ProductionRunConfig {
            use_top_analysis: true,
            ..ProductionRunConfig::default()
        };
        let mut outcome = accepted_outcome("good", 2.0, -12.0);
        let (decision, energy) = classify_production_outcome(
            &cfg,
            &mut outcome,
            Some("input-hk".into()),
            Some("final-hk".into()),
        );

        assert_eq!(decision.kind, ProductionDecisionKind::Accepted);
        assert_eq!(energy, Some(-12.0));
        assert_eq!(outcome.state.science_evidence.validity_checks.len(), 1);
        assert_eq!(
            outcome.state.science_evidence.validity_checks[0].kind,
            ScottValidityCheckKind::HashkeyAvailable
        );
        assert!(outcome.state.science_evidence.validity_checks[0].passed);
        let topology = outcome
            .state
            .science_evidence
            .topology
            .as_ref()
            .expect("topology provenance");
        assert!(topology.topology_analysis_requested);
        assert_eq!(topology.input_hashkey.as_deref(), Some("input-hk"));
        assert_eq!(topology.final_hashkey.as_deref(), Some("final-hk"));
    }

    #[test]
    fn best_set_respects_limit_cutoff_and_hashkey_dedup() {
        let cfg = ProductionRunConfig {
            max_best_clusters: 2,
            best_energy_cutoff: -5.0,
            best_energy_tolerance: 1.0e-4,
            use_top_analysis: true,
            ..ProductionRunConfig::default()
        };
        let mut best_set = ProductionBestSet::default();

        let first_rank = best_set.consider(
            cfg.best_set_config(),
            ProductionBestEntry {
                candidate_label: "a".into(),
                final_stage: Some(1),
                energy: -10.0,
                hashkey: Some("hk-1".into()),
                relaxed_candidate: candidate("a", 2.0),
            },
        );
        let duplicate_rank = best_set.consider(
            cfg.best_set_config(),
            ProductionBestEntry {
                candidate_label: "b".into(),
                final_stage: Some(1),
                energy: -9.0,
                hashkey: Some("hk-1".into()),
                relaxed_candidate: candidate("b", 2.5),
            },
        );
        let rejected_rank = best_set.consider(
            cfg.best_set_config(),
            ProductionBestEntry {
                candidate_label: "c".into(),
                final_stage: Some(1),
                energy: -1.0,
                hashkey: Some("hk-3".into()),
                relaxed_candidate: candidate("c", 3.0),
            },
        );

        assert_eq!(first_rank.rank(), Some(1));
        assert!(matches!(
            duplicate_rank,
            ProductionBestSetDecision::MatchedExisting(ref existing)
                if existing.rank == 1 && existing.entry.candidate_label == "a"
        ));
        assert_eq!(rejected_rank, ProductionBestSetDecision::Rejected);
        assert_eq!(best_set.entries().len(), 1);
        let matched = best_set.find_hashkey_match("hk-1").expect("hashkey match");
        assert_eq!(matched.rank, 1);
        assert_eq!(matched.entry.candidate_label, "a");
    }

    #[test]
    fn best_set_matches_same_stage_energy_without_topology() {
        let cfg = ProductionRunConfig {
            max_best_clusters: 3,
            best_energy_cutoff: -5.0,
            best_energy_tolerance: 1.0e-4,
            use_top_analysis: false,
            ..ProductionRunConfig::default()
        };
        let mut best_set = ProductionBestSet::default();

        let first_rank = best_set.consider(
            cfg.best_set_config(),
            ProductionBestEntry {
                candidate_label: "a".into(),
                final_stage: Some(2),
                energy: -10.0,
                hashkey: None,
                relaxed_candidate: candidate("a", 2.0),
            },
        );
        let duplicate_rank = best_set.consider(
            cfg.best_set_config(),
            ProductionBestEntry {
                candidate_label: "b".into(),
                final_stage: Some(2),
                energy: -9.99995,
                hashkey: None,
                relaxed_candidate: candidate("b", 2.5),
            },
        );
        let other_stage_rank = best_set.consider(
            cfg.best_set_config(),
            ProductionBestEntry {
                candidate_label: "c".into(),
                final_stage: Some(3),
                energy: -9.99995,
                hashkey: None,
                relaxed_candidate: candidate("c", 3.0),
            },
        );

        assert_eq!(first_rank.rank(), Some(1));
        assert!(matches!(
            duplicate_rank,
            ProductionBestSetDecision::MatchedExisting(ref existing)
                if existing.rank == 1 && existing.entry.candidate_label == "a"
        ));
        assert_eq!(other_stage_rank.rank(), Some(2));
        assert_eq!(best_set.entries().len(), 2);
    }

    #[test]
    fn production_tracker_builds_summary_from_kernel_state() {
        let mut tracker = ProductionWorkflowTracker::new(
            3,
            "MgO",
            PathBuf::from("/tmp/production"),
            ProductionRunConfig {
                max_best_clusters: 2,
                use_top_analysis: true,
                ..ProductionRunConfig::default()
            },
            Some(ProductionRestartState {
                counter: Some(10),
                random_start: Some(false),
            }),
        );

        let accepted_seed = ProductionSeedInput::restart_artifact(
            "accepted.xyz".into(),
            candidate("accepted", 2.0),
        );
        let accepted_task = tracker
            .begin_candidate(accepted_seed.candidate.clone())
            .expect("accepted task");
        let accepted_best_set_decision = tracker.consider_best_entry(ProductionBestEntry {
            candidate_label: "accepted".into(),
            final_stage: Some(1),
            energy: -10.0,
            hashkey: Some("hk-1".into()),
            relaxed_candidate: candidate("accepted", 2.0),
        });
        let accepted_best_set_rank = accepted_best_set_decision.rank();
        tracker.record_evaluated_candidate(super::EvaluatedCandidateRecord {
            task: accepted_task,
            seed_provenance: accepted_seed.provenance.clone(),
            workdir: PathBuf::from("/tmp/production/candidate_0010"),
            outcome: accepted_outcome("accepted", 2.0, -10.0),
            decision: super::ProductionDecision {
                kind: ProductionDecisionKind::Accepted,
                reason: "accepted".into(),
            },
            final_energy: Some(-10.0),
            final_hashkey: Some("hk-1".into()),
            best_set_rank: accepted_best_set_rank,
            best_set_decision: Some(accepted_best_set_decision),
        });

        let rejected_seed = ProductionSeedInput::restart_artifact(
            "rejected.xyz".into(),
            candidate("rejected", 1.0),
        );
        let rejected_task = tracker
            .begin_candidate(rejected_seed.candidate.clone())
            .expect("rejected task");
        tracker.record_evaluated_candidate(super::EvaluatedCandidateRecord {
            task: rejected_task,
            seed_provenance: rejected_seed.provenance.clone(),
            workdir: PathBuf::from("/tmp/production/candidate_0011"),
            outcome: accepted_outcome("rejected", 1.0, -1.0),
            decision: super::ProductionDecision {
                kind: ProductionDecisionKind::Rejected,
                reason: "Cluster has collapsed!".into(),
            },
            final_energy: Some(-1.0),
            final_hashkey: None,
            best_set_rank: None,
            best_set_decision: None,
        });

        let skipped_seed =
            ProductionSeedInput::restart_artifact("skipped.xyz".into(), candidate("skipped", 2.5));
        let skipped_task = tracker
            .begin_candidate(skipped_seed.candidate.clone())
            .expect("skipped task");
        let topology_match = tracker
            .probe_topology_hashkey("hk-1")
            .expect("topology match");
        tracker.record_topology_skip(
            skipped_task,
            &skipped_seed.provenance,
            12,
            PathBuf::from("/tmp/production/candidate_0012"),
            Some("hk-1".into()),
            topology_match,
        );
        tracker.update_restart_progress(13);
        tracker.validate().expect("tracker invariants");

        let summary = tracker.build_summary(ScottBackendRoutingPolicy {
            default_backend: ScottBackendMode::Gulp,
            stage_overrides: IndexMap::new(),
        });

        assert_eq!(summary.candidate_count, 3);
        assert_eq!(summary.success_count, 1);
        assert_eq!(summary.failure_count, 1);
        assert_eq!(summary.topology_skip_count, 1);
        assert_eq!(summary.best_set_size, 1);
        assert_eq!(summary.candidates.len(), 3);
        assert_eq!(
            summary.candidates[0]
                .seed_provenance
                .as_ref()
                .map(|record| record.source_name.as_deref()),
            Some(Some("accepted.xyz"))
        );
        assert_eq!(summary.candidates[0].relaxation_stages.len(), 1);
        assert_eq!(summary.candidates[0].relaxation_stages[0].stage, 1);
        assert!(summary.candidates[0].relaxation_stages[0].accepted_stage);
        assert!(summary.candidates[0].topology_comparison.is_none());
        let accepted_best_set_decision = summary.candidates[0]
            .best_set_decision
            .as_ref()
            .expect("accepted best-set decision");
        assert_eq!(
            accepted_best_set_decision.kind,
            patina_types::BestSetDecisionKind::Inserted
        );
        assert_eq!(accepted_best_set_decision.rank, Some(1));
        assert!(summary.candidates[1].best_set_decision.is_none());
        assert!(summary.candidates[2].relaxation_stages.is_empty());
        let topology = summary.candidates[2]
            .topology_comparison
            .as_ref()
            .expect("topology comparison");
        assert!(topology.analysis_requested);
        assert!(topology.topology_skip);
        assert_eq!(
            topology.action,
            Some(patina_types::TopologyComparisonActionRecord::MatchedBestArchiveInputHashkey)
        );
        let skipped_best_set_decision = summary.candidates[2]
            .best_set_decision
            .as_ref()
            .expect("topology-skip best-set decision");
        assert_eq!(
            skipped_best_set_decision.kind,
            patina_types::BestSetDecisionKind::MatchedExisting
        );
        assert_eq!(skipped_best_set_decision.rank, Some(1));
        assert_eq!(
            skipped_best_set_decision.matched_candidate_label.as_deref(),
            Some("accepted")
        );
        assert_eq!(
            skipped_best_set_decision.matched_hashkey.as_deref(),
            Some("hk-1")
        );
        assert_eq!(
            summary.restart_state_after,
            Some(ProductionRestartState {
                counter: Some(13),
                random_start: Some(false),
            })
        );
    }

    #[test]
    fn production_precheck_only_skips_best_archive_matches() {
        let mut tracker = ProductionWorkflowTracker::new(
            1,
            "MgO",
            PathBuf::from("/tmp/production"),
            ProductionRunConfig {
                max_best_clusters: 2,
                use_top_analysis: true,
                ..ProductionRunConfig::default()
            },
            None,
        );
        tracker.consider_best_entry(ProductionBestEntry {
            candidate_label: "accepted".into(),
            final_stage: Some(1),
            energy: -10.0,
            hashkey: Some("hk-archive".into()),
            relaxed_candidate: candidate("accepted", 2.0),
        });
        tracker
            .register_blacklist_hashkey("hk-black")
            .expect("blacklist");

        assert!(matches!(
            tracker.compare_topology_hashkey(Some("hk-archive")),
            Some(TopologyIdentityMatch::BestArchive(record))
                if record.rank == 1 && record.candidate_label == "accepted"
        ));
        assert!(tracker.compare_topology_hashkey(Some("hk-black")).is_none());
        assert!(tracker.compare_topology_hashkey(None).is_none());
    }

    #[test]
    fn topology_skip_records_blacklist_semantics_without_best_set_match() {
        let mut tracker = ProductionWorkflowTracker::new(
            1,
            "MgO",
            PathBuf::from("/tmp/production"),
            ProductionRunConfig {
                use_top_analysis: true,
                ..ProductionRunConfig::default()
            },
            None,
        );
        let skipped_seed = ProductionSeedInput::restart_artifact(
            "blacklisted.xyz".into(),
            candidate("blocked", 2.5),
        );
        let skipped_task = tracker
            .begin_candidate(skipped_seed.candidate.clone())
            .expect("skipped task");

        tracker.record_topology_skip(
            skipped_task,
            &skipped_seed.provenance,
            0,
            PathBuf::from("/tmp/production/candidate_0000"),
            Some("hk-black".into()),
            TopologyIdentityMatch::Blacklist {
                hashkey: "hk-black".into(),
            },
        );
        tracker.validate().expect("tracker invariants");

        let summary = tracker.build_summary(dummy_routing_policy());
        let skipped = &summary.candidates[0];
        let topology = skipped
            .topology_comparison
            .as_ref()
            .expect("topology comparison");

        assert_eq!(
            topology.action,
            Some(patina_types::TopologyComparisonActionRecord::MatchedBlacklist)
        );
        assert!(skipped.best_set_decision.is_none());
        assert_eq!(skipped.best_set_rank, None);
        assert_eq!(
            skipped.decision.reason,
            "input hashkey is present in the topology blacklist"
        );
    }

    #[test]
    fn production_workflow_service_routes_accept_skip_and_reject() {
        let service = ProductionWorkflowService;
        let seeds = vec![
            ProductionSeedInput::restart_artifact("seed_a.xyz".into(), candidate("accepted", 2.0)),
            ProductionSeedInput::restart_artifact("seed_b.xyz".into(), candidate("skipped", 2.1)),
            ProductionSeedInput::restart_artifact("seed_c.xyz".into(), candidate("rejected", 1.0)),
        ];
        let evaluation_port = FakeEvaluationPort {
            input_hashkeys: vec![Some("hk-1".into()), Some("hk-1".into()), None],
            final_hashkeys: vec![Some("hk-1".into()), Some("hk-1".into()), None],
            outcomes: vec![
                accepted_outcome("accepted", 2.0, -10.0),
                accepted_outcome("skipped", 2.1, -9.0),
                accepted_outcome("rejected", 1.0, -1.0),
            ],
            evaluated_indices: RefCell::new(Vec::new()),
        };
        let progress_port = FakeProgressPort::default();
        let artifact_sink = FakeArtifactSink::default();
        let workdir = tempdir().expect("tempdir");

        let execution = service
            .execute(
                &seeds,
                "MgO",
                workdir.path(),
                &ProductionRunConfig {
                    max_best_clusters: 2,
                    best_energy_cutoff: -5.0,
                    use_top_analysis: true,
                    collapse_threshold: 1.5,
                    ..ProductionRunConfig::default()
                },
                Some(ProductionRestartState {
                    counter: Some(0),
                    random_start: Some(false),
                }),
                0,
                dummy_routing_policy(),
                &evaluation_port,
                &evaluation_port,
                &progress_port,
                &artifact_sink,
            )
            .expect("execute production workflow");

        assert_eq!(execution.summary.success_count, 1);
        assert_eq!(execution.summary.failure_count, 1);
        assert_eq!(execution.summary.topology_skip_count, 1);
        assert_eq!(execution.summary.best_set_size, 1);
        assert_eq!(execution.summary.candidates.len(), 3);
        assert_eq!(
            execution.summary.candidates[1]
                .seed_provenance
                .as_ref()
                .map(|record| record.source_name.as_deref()),
            Some(Some("seed_b.xyz"))
        );
        assert_eq!(execution.summary.candidates[0].relaxation_stages.len(), 1);
        assert_eq!(
            execution.summary.candidates[0]
                .topology_comparison
                .as_ref()
                .and_then(|record| record.final_hashkey.as_deref()),
            Some("hk-1")
        );
        assert_eq!(
            execution.summary.candidates[0]
                .best_set_decision
                .as_ref()
                .map(|record| record.kind),
            Some(patina_types::BestSetDecisionKind::Inserted)
        );
        assert!(execution.summary.candidates[1].relaxation_stages.is_empty());
        assert_eq!(
            execution.summary.candidates[1]
                .topology_comparison
                .as_ref()
                .and_then(|record| record.action),
            Some(patina_types::TopologyComparisonActionRecord::MatchedBestArchiveInputHashkey)
        );
        assert_eq!(
            execution.summary.candidates[1]
                .best_set_decision
                .as_ref()
                .map(|record| record.kind),
            Some(patina_types::BestSetDecisionKind::MatchedExisting)
        );
        assert!(execution.summary.candidates[2].best_set_decision.is_none());
        assert_eq!(*evaluation_port.evaluated_indices.borrow(), vec![0, 2]);
        assert_eq!(*artifact_sink.accepted_indices.borrow(), vec![0]);
        assert_eq!(progress_port.consumed.borrow().len(), 3);
        assert_eq!(
            execution.summary.restart_state_after,
            Some(ProductionRestartState {
                counter: Some(3),
                random_start: Some(false),
            })
        );
    }
}
