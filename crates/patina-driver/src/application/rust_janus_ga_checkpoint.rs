use anyhow::{anyhow, Context, Result};
use patina_search::{ScottGaMember, ScottGaOrigin, ScottGaTopologyIdentity, WorkflowLineage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::driver_support::candidate_hashkey_cache_key;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct RustGaCheckpointContext<'a> {
    pub workflow_id: &'a str,
    pub workflow_owner: &'a str,
    pub backend: &'a str,
    pub backend_mode: &'a str,
    pub system: &'a str,
    pub requested_generations: usize,
}

pub struct RustGaCheckpointBuildRequest<'a> {
    pub context: RustGaCheckpointContext<'a>,
    pub search_cfg: &'a patina_types::SearchConfig,
    pub operator_policy: &'a crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    pub generations: &'a [crate::application::ga_execution::RustJanusGenerationArtifact],
    pub final_population: &'a [ScottGaMember],
    pub hashkeys: Option<&'a mut GaHashkeyComputer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustGaCheckpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    pub workflow_owner: String,
    pub checkpoint_version: u32,
    pub generation_completed: usize,
    pub population_size: usize,
    pub requested_generations: usize,
    pub seed: Option<u64>,
    pub system: String,
    pub backend: String,
    pub janus_mode: String,
    pub search_config: patina_types::SearchConfig,
    pub operator_policy: crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    pub generation_state: patina_types::GaGenerationState,
}

#[derive(Debug, Clone)]
pub struct GaHashkeyArtifactConfig {
    pub radius_mode: String,
    pub radius_const: f64,
    pub dreadnaut_path: PathBuf,
    pub scratch_dir: PathBuf,
}

pub struct GaHashkeyComputer {
    radius_mode: String,
    radius_const: f64,
    dreadnaut_path: PathBuf,
    scratch_dir: PathBuf,
    cache: HashMap<u64, String>,
}

impl GaHashkeyComputer {
    pub fn new(config: &GaHashkeyArtifactConfig) -> Result<Self> {
        fs::create_dir_all(&config.scratch_dir).with_context(|| {
            format!(
                "failed to create GA hashkey scratch dir `{}`",
                config.scratch_dir.display()
            )
        })?;
        Ok(Self {
            radius_mode: config.radius_mode.clone(),
            radius_const: config.radius_const,
            dreadnaut_path: config.dreadnaut_path.clone(),
            scratch_dir: config.scratch_dir.clone(),
            cache: HashMap::new(),
        })
    }

    pub fn topology_for_member(
        &mut self,
        member: &ScottGaMember,
    ) -> Result<patina_types::GaMemberTopologyRecord> {
        if has_complete_topology(&member.topology) {
            return Ok(patina_types::GaMemberTopologyRecord {
                canonical_hashkey: member.topology.canonical_hashkey.clone(),
                source_hashkey: member.topology.source_hashkey.clone(),
                relaxed_hashkey: member.topology.relaxed_hashkey.clone(),
            });
        }

        let source_hashkey = self.compute_candidate_hashkey(
            &member.source_candidate,
            &format!("{}_source", member.source_candidate.label),
        )?;
        let relaxed_hashkey = self.compute_candidate_hashkey(
            &member.result.relaxed_candidate,
            &format!("{}_relaxed", member.result.relaxed_candidate.label),
        )?;
        let canonical_hashkey = if member.result.converged {
            relaxed_hashkey.clone().or(source_hashkey.clone())
        } else {
            source_hashkey.clone().or(relaxed_hashkey.clone())
        };

        Ok(patina_types::GaMemberTopologyRecord {
            canonical_hashkey,
            source_hashkey,
            relaxed_hashkey,
        })
    }

    fn compute_candidate_hashkey(
        &mut self,
        candidate: &patina_types::Candidate,
        suffix: &str,
    ) -> Result<Option<String>> {
        let cache_key = candidate_hashkey_cache_key(candidate);
        if let Some(existing) = self.cache.get(&cache_key) {
            return Ok(Some(existing.clone()));
        }

        let atom_specs = patina_dreadnaut::infer_atom_specs_for_candidate(candidate, None)
            .with_context(|| {
                format!(
                    "failed to infer GA member atom specs for `{}`",
                    candidate.label
                )
            })?;
        let radius = patina_dreadnaut::compute_hashkey_radius(
            candidate,
            &atom_specs,
            &self.radius_mode,
            self.radius_const,
        )
        .with_context(|| {
            format!(
                "failed to compute GA member hashkey radius for `{}`",
                candidate.label
            )
        })?;
        let graph_text =
            patina_dreadnaut::build_dreadnaut_graph_text(candidate, radius, &atom_specs);
        let graph_path = self
            .scratch_dir
            .join(format!("ga_member_{:016x}_{suffix}.dreadnaut", cache_key));
        fs::write(&graph_path, graph_text).with_context(|| {
            format!(
                "failed to write GA member Dreadnaut graph `{}`",
                graph_path.display()
            )
        })?;
        let hashkey =
            patina_dreadnaut::canonical_hashkey_from_graph_file(&self.dreadnaut_path, &graph_path)
                .with_context(|| {
                    format!(
                        "failed to compute GA member hashkey for `{}` via `{}`",
                        candidate.label,
                        self.dreadnaut_path.display()
                    )
                })?;
        if hashkey.trim().is_empty() {
            return Err(anyhow!(
                "empty GA member hashkey generated for `{}` via `{}`",
                candidate.label,
                self.dreadnaut_path.display()
            ));
        }
        self.cache.insert(cache_key, hashkey.clone());
        Ok(Some(hashkey))
    }
}

pub fn load_checkpoint(path: &Path) -> Result<RustGaCheckpoint> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Rust GA checkpoint `{}`", path.display()))?;
    let checkpoint: RustGaCheckpoint = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse Rust GA checkpoint `{}`", path.display()))?;
    validate_checkpoint(&checkpoint)
        .with_context(|| format!("invalid Rust GA checkpoint `{}`", path.display()))?;
    Ok(checkpoint)
}

#[cfg(test)]
pub fn build_checkpoint(
    context: &RustGaCheckpointContext<'_>,
    search_cfg: &patina_types::SearchConfig,
    operator_policy: &crate::application::janus_operator_policy::JanusGaOperatorPolicy,
    generations: &[crate::application::ga_execution::RustJanusGenerationArtifact],
    final_population: &[ScottGaMember],
) -> RustGaCheckpoint {
    build_checkpoint_with_hashkeys(RustGaCheckpointBuildRequest {
        context: *context,
        search_cfg,
        operator_policy,
        generations,
        final_population,
        hashkeys: None,
    })
    .expect("building checkpoint without GA hashkeys should not fail")
}

pub fn build_checkpoint_with_hashkeys(
    request: RustGaCheckpointBuildRequest<'_>,
) -> Result<RustGaCheckpoint> {
    let RustGaCheckpointBuildRequest {
        context,
        search_cfg,
        operator_policy,
        generations,
        final_population,
        hashkeys,
    } = request;
    let generation_completed = generations
        .iter()
        .map(|row| row.generation)
        .max()
        .unwrap_or(0);

    Ok(RustGaCheckpoint {
        workflow_id: Some(context.workflow_id.to_string()),
        workflow_owner: context.workflow_owner.to_string(),
        checkpoint_version: 1,
        generation_completed,
        population_size: final_population.len(),
        requested_generations: context.requested_generations,
        seed: search_cfg.seed,
        system: context.system.to_string(),
        backend: context.backend.to_string(),
        janus_mode: context.backend_mode.to_string(),
        search_config: search_cfg.clone(),
        operator_policy: operator_policy.clone(),
        generation_state: build_generation_state_with_hashkeys(
            generation_completed,
            final_population,
            hashkeys,
        )?,
    })
}

pub fn build_generation_state(
    generation_completed: usize,
    final_population: &[ScottGaMember],
) -> patina_types::GaGenerationState {
    build_generation_state_with_hashkeys(generation_completed, final_population, None)
        .expect("building generation state without GA hashkeys should not fail")
}

pub fn build_generation_state_with_hashkeys(
    generation_completed: usize,
    final_population: &[ScottGaMember],
    mut hashkeys: Option<&mut GaHashkeyComputer>,
) -> Result<patina_types::GaGenerationState> {
    let population = final_population
        .iter()
        .enumerate()
        .map(|(member_id, member)| checkpoint_member(member_id, member, hashkeys.as_deref_mut()))
        .collect::<Result<Vec<_>>>()?;
    let elite_count = final_population.len().clamp(1, 3);
    let elites = final_population
        .iter()
        .take(elite_count)
        .enumerate()
        .map(|(member_id, member)| checkpoint_member(member_id, member, hashkeys.as_deref_mut()))
        .collect::<Result<Vec<_>>>()?;
    let repopulation = final_population
        .iter()
        .enumerate()
        .filter_map(|(member_id, member)| population_repopulation_record(member_id, member))
        .collect::<Vec<_>>();
    Ok(patina_types::GaGenerationState {
        generation: generation_completed,
        population,
        elites,
        repopulation,
    })
}

pub fn checkpoint_population(checkpoint: &RustGaCheckpoint) -> Result<Vec<ScottGaMember>> {
    validate_checkpoint(checkpoint)?;
    checkpoint
        .generation_state
        .population
        .iter()
        .map(|member| {
            let origin = ScottGaOrigin::parse_label(&member.origin).ok_or_else(|| {
                anyhow!(
                    "unsupported checkpoint origin `{}` in Rust GA checkpoint",
                    member.origin
                )
            })?;
            Ok(ScottGaMember {
                source_candidate: patina_types::Candidate::from(&member.source),
                result: member.evaluation.to_eval_result(),
                origin,
                occurrences: member.occurrences,
                lineage: workflow_lineage_from_record(member.lineage.as_ref().ok_or_else(
                    || {
                        anyhow!(
                            "checkpoint member {} is missing controller-critical lineage metadata",
                            member.member_id
                        )
                    },
                )?),
                topology: ScottGaTopologyIdentity {
                    canonical_hashkey: member.topology.canonical_hashkey.clone(),
                    source_hashkey: member.topology.source_hashkey.clone(),
                    relaxed_hashkey: member.topology.relaxed_hashkey.clone(),
                },
            })
        })
        .collect()
}

pub fn validate_checkpoint(checkpoint: &RustGaCheckpoint) -> Result<()> {
    if checkpoint.checkpoint_version != 1 {
        return Err(anyhow!(
            "unsupported Rust GA checkpoint version {}; expected 1",
            checkpoint.checkpoint_version
        ));
    }
    if checkpoint.workflow_owner.trim().is_empty() {
        return Err(anyhow!("checkpoint workflow_owner is empty"));
    }
    if checkpoint
        .workflow_id
        .as_deref()
        .is_some_and(|workflow_id| workflow_id.trim().is_empty())
    {
        return Err(anyhow!("checkpoint workflow_id is empty"));
    }
    if checkpoint.backend.trim().is_empty() {
        return Err(anyhow!("checkpoint backend is empty"));
    }
    if checkpoint.janus_mode.trim().is_empty() {
        return Err(anyhow!("checkpoint janus_mode is empty"));
    }
    if checkpoint.system.trim().is_empty() {
        return Err(anyhow!("checkpoint system is empty"));
    }
    if checkpoint.generation_state.generation != checkpoint.generation_completed {
        return Err(anyhow!(
            "checkpoint generation mismatch: header generation_completed={} but generation_state.generation={}",
            checkpoint.generation_completed,
            checkpoint.generation_state.generation
        ));
    }
    if checkpoint.requested_generations < checkpoint.generation_completed {
        return Err(anyhow!(
            "checkpoint requested_generations {} is less than completed generation {}",
            checkpoint.requested_generations,
            checkpoint.generation_completed
        ));
    }
    if checkpoint.population_size != checkpoint.generation_state.population.len() {
        return Err(anyhow!(
            "checkpoint population_size {} does not match generation_state population length {}",
            checkpoint.population_size,
            checkpoint.generation_state.population.len()
        ));
    }
    if checkpoint.population_size == 0 {
        return Err(anyhow!("checkpoint population is empty"));
    }
    validate_generation_state(
        &checkpoint.generation_state,
        checkpoint.generation_completed,
    )
}

fn validate_generation_state(
    state: &patina_types::GaGenerationState,
    generation_completed: usize,
) -> Result<()> {
    let mut seen_member_ids = BTreeSet::new();
    for (slot, member) in state.population.iter().enumerate() {
        validate_member_state(member, slot, generation_completed)?;
        if !seen_member_ids.insert(member.member_id) {
            return Err(anyhow!(
                "checkpoint population contains duplicate member_id {}",
                member.member_id
            ));
        }
    }
    for (slot, member) in state.elites.iter().enumerate() {
        validate_member_state(member, slot, generation_completed).with_context(|| {
            format!(
                "invalid elite checkpoint member at elite slot {} with member_id {}",
                slot, member.member_id
            )
        })?;
    }
    for record in &state.repopulation {
        if !seen_member_ids.contains(&record.member_id) {
            return Err(anyhow!(
                "checkpoint repopulation record references missing member_id {}",
                record.member_id
            ));
        }
        if record.source_candidate_label.trim().is_empty() {
            return Err(anyhow!(
                "checkpoint repopulation record for member_id {} has empty source_candidate_label",
                record.member_id
            ));
        }
        if record.evaluation_label.trim().is_empty() {
            return Err(anyhow!(
                "checkpoint repopulation record for member_id {} has empty evaluation_label",
                record.member_id
            ));
        }
        if record.converged && record.energy.is_none() {
            return Err(anyhow!(
                "checkpoint repopulation record for member_id {} is converged but has no energy",
                record.member_id
            ));
        }
    }
    Ok(())
}

fn validate_member_state(
    member: &patina_types::GaMemberState,
    slot: usize,
    generation_completed: usize,
) -> Result<()> {
    if member.member_id != slot {
        return Err(anyhow!(
            "checkpoint member_id {} does not match population slot {}",
            member.member_id,
            slot
        ));
    }
    if ScottGaOrigin::parse_label(&member.origin).is_none() {
        return Err(anyhow!(
            "unsupported checkpoint origin `{}` for member_id {}",
            member.origin,
            member.member_id
        ));
    }
    if member.occurrences == 0 {
        return Err(anyhow!(
            "checkpoint member_id {} has zero occurrences",
            member.member_id
        ));
    }
    let source_candidate = patina_types::Candidate::from(&member.source);
    source_candidate.validate().map_err(|error| {
        anyhow!(
            "checkpoint source candidate for member_id {} failed validation: {error:?}",
            member.member_id
        )
    })?;
    let relaxed_candidate = patina_types::Candidate::from(&member.evaluation.structure);
    relaxed_candidate.validate().map_err(|error| {
        anyhow!(
            "checkpoint evaluation structure for member_id {} failed validation: {error:?}",
            member.member_id
        )
    })?;
    if member.source.label.trim().is_empty() {
        return Err(anyhow!(
            "checkpoint source label is empty for member_id {}",
            member.member_id
        ));
    }
    if member.evaluation.label.trim().is_empty() {
        return Err(anyhow!(
            "checkpoint evaluation label is empty for member_id {}",
            member.member_id
        ));
    }
    if member.evaluation.converged && !member.evaluation.energy.is_finite() {
        return Err(anyhow!(
            "checkpoint member_id {} is converged but has non-finite energy",
            member.member_id
        ));
    }
    let lineage = member.lineage.as_ref().ok_or_else(|| {
        anyhow!(
            "checkpoint member_id {} is missing controller-critical lineage metadata",
            member.member_id
        )
    })?;
    validate_lineage_record(lineage, member, generation_completed)?;
    validate_topology_record(&member.topology, member.member_id)?;
    Ok(())
}

fn validate_lineage_record(
    lineage: &patina_types::GaMemberLineageRecord,
    member: &patina_types::GaMemberState,
    generation_completed: usize,
) -> Result<()> {
    if lineage.origin_label.trim().is_empty() {
        return Err(anyhow!(
            "checkpoint lineage origin_label is empty for member_id {}",
            member.member_id
        ));
    }
    if lineage.generation.is_none() {
        return Err(anyhow!(
            "checkpoint lineage generation is missing for member_id {}",
            member.member_id
        ));
    }
    if lineage
        .generation
        .is_some_and(|generation| generation > generation_completed)
    {
        return Err(anyhow!(
            "checkpoint lineage generation {:?} exceeds completed generation {} for member_id {}",
            lineage.generation,
            generation_completed,
            member.member_id
        ));
    }
    if lineage.attempt == 0 {
        return Err(anyhow!(
            "checkpoint lineage attempt is zero for member_id {}",
            member.member_id
        ));
    }
    for parent_label in &lineage.parent_labels {
        if parent_label.trim().is_empty() {
            return Err(anyhow!(
                "checkpoint lineage contains an empty parent label for member_id {}",
                member.member_id
            ));
        }
    }
    Ok(())
}

fn validate_topology_record(
    topology: &patina_types::GaMemberTopologyRecord,
    member_id: usize,
) -> Result<()> {
    for (field, value) in [
        ("canonical_hashkey", topology.canonical_hashkey.as_deref()),
        ("source_hashkey", topology.source_hashkey.as_deref()),
        ("relaxed_hashkey", topology.relaxed_hashkey.as_deref()),
    ] {
        if value.is_some_and(|hashkey| hashkey.trim().is_empty()) {
            return Err(anyhow!(
                "checkpoint topology field {field} is empty for member_id {member_id}"
            ));
        }
    }
    Ok(())
}

fn checkpoint_member(
    member_id: usize,
    member: &ScottGaMember,
    hashkeys: Option<&mut GaHashkeyComputer>,
) -> Result<patina_types::GaMemberState> {
    let topology = match hashkeys {
        Some(hashkey_computer) => hashkey_computer.topology_for_member(member)?,
        None => patina_types::GaMemberTopologyRecord {
            canonical_hashkey: member.topology.canonical_hashkey.clone(),
            source_hashkey: member.topology.source_hashkey.clone(),
            relaxed_hashkey: member.topology.relaxed_hashkey.clone(),
        },
    };
    Ok(patina_types::GaMemberState {
        member_id,
        origin: member.origin.as_str().to_string(),
        occurrences: member.occurrences,
        source: patina_types::StructureRecord::from(&member.source_candidate),
        evaluation: patina_types::EvaluationRecord::from(&member.result),
        lineage: Some(lineage_record(&member.lineage)),
        topology,
    })
}

fn population_repopulation_record(
    member_id: usize,
    member: &ScottGaMember,
) -> Option<patina_types::PopulationRepopulationRecord> {
    let source = match member.origin {
        ScottGaOrigin::RePopM => patina_types::PopulationRepopulationSource::EliteMutation,
        ScottGaOrigin::RePopR => patina_types::PopulationRepopulationSource::RandomStructure,
        _ => return None,
    };

    Some(patina_types::PopulationRepopulationRecord {
        member_id,
        source,
        source_candidate_label: member.source_candidate.label.clone(),
        evaluation_label: member.result.relaxed_candidate.label.clone(),
        converged: member.result.converged,
        energy: member.result.converged.then_some(member.result.energy),
    })
}

fn has_complete_topology(topology: &ScottGaTopologyIdentity) -> bool {
    topology.canonical_hashkey.is_some()
        && topology.source_hashkey.is_some()
        && topology.relaxed_hashkey.is_some()
}

fn lineage_record(lineage: &WorkflowLineage) -> patina_types::GaMemberLineageRecord {
    patina_types::GaMemberLineageRecord {
        origin_label: lineage.origin_label.clone(),
        generation: lineage.generation,
        step: lineage.step,
        parent_labels: lineage.parent_labels.clone(),
        attempt: lineage.attempt,
    }
}

fn workflow_lineage_from_record(record: &patina_types::GaMemberLineageRecord) -> WorkflowLineage {
    WorkflowLineage {
        origin_label: record.origin_label.clone(),
        generation: record.generation,
        step: record.step,
        parent_labels: record.parent_labels.clone(),
        attempt: record.attempt,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_checkpoint, build_generation_state, checkpoint_population, load_checkpoint,
        validate_checkpoint, RustGaCheckpoint, RustGaCheckpointContext,
    };
    use patina_search::{ScottGaMember, ScottGaOrigin, ScottGaTopologyIdentity, WorkflowLineage};

    fn sample_candidate(label: &str) -> patina_types::Candidate {
        patina_types::Candidate::cluster(
            label,
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
        )
    }

    fn sample_checkpoint() -> RustGaCheckpoint {
        RustGaCheckpoint {
            workflow_id: Some("ga.persistent-daemon".to_string()),
            workflow_owner: "persistent_daemon_ga".to_string(),
            checkpoint_version: 1,
            generation_completed: 3,
            population_size: 1,
            requested_generations: 5,
            seed: Some(7),
            system: "(MgO)1".to_string(),
            backend: "janus_mace".to_string(),
            janus_mode: "local-opt".to_string(),
            search_config: patina_types::SearchConfig {
                temperature: 10.0,
                step_size: 0.1,
                population_size: 4,
                max_steps: 5,
                seed: Some(7),
            },
            operator_policy:
                crate::application::janus_operator_policy::select_janus_ga_operator_policy(
                    crate::application::janus_operator_policy::GaOperatorBackendProfile::JanusMace,
                    crate::DriverJanusMode::LocalOpt,
                    0.1,
                ),
            generation_state: patina_types::GaGenerationState {
                generation: 3,
                population: vec![patina_types::GaMemberState {
                    member_id: 0,
                    origin: "MUTATE".to_string(),
                    occurrences: 2,
                    source: patina_types::StructureRecord::from(&sample_candidate("source")),
                    evaluation: patina_types::EvaluationRecord {
                        label: "relaxed".to_string(),
                        energy: -1.25,
                        converged: true,
                        structure: patina_types::StructureRecord::from(&sample_candidate(
                            "relaxed",
                        )),
                        backend_run_dir: None,
                        primary_output_path: None,
                    },
                    lineage: Some(patina_types::GaMemberLineageRecord {
                        origin_label: "source".to_string(),
                        generation: Some(3),
                        step: None,
                        parent_labels: vec!["parent_a".to_string(), "parent_b".to_string()],
                        attempt: 1,
                    }),
                    topology: patina_types::GaMemberTopologyRecord {
                        canonical_hashkey: Some("hk-relaxed".to_string()),
                        source_hashkey: Some("hk-source".to_string()),
                        relaxed_hashkey: Some("hk-relaxed".to_string()),
                    },
                }],
                elites: Vec::new(),
                repopulation: Vec::new(),
            },
        }
    }

    #[test]
    fn checkpoint_population_rebuilds_members() {
        let checkpoint = sample_checkpoint();
        let population = checkpoint_population(&checkpoint).expect("checkpoint rebuild");
        assert_eq!(population.len(), 1);
        assert_eq!(population[0].origin.as_str(), "MUTATE");
        assert_eq!(population[0].occurrences, 2);
        assert!(population[0].result.converged);
        assert_eq!(population[0].result.energy, -1.25);
        assert_eq!(population[0].lineage.parent_labels.len(), 2);
        assert_eq!(
            population[0].topology.canonical_hashkey.as_deref(),
            Some("hk-relaxed")
        );
    }

    #[test]
    fn checkpoint_load_roundtrips_valid_metadata() {
        let checkpoint = sample_checkpoint();
        validate_checkpoint(&checkpoint).expect("valid checkpoint");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rust_ga_checkpoint.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&checkpoint).expect("serialize checkpoint"),
        )
        .expect("write checkpoint");

        let loaded = load_checkpoint(&path).expect("load checkpoint");

        assert_eq!(loaded.generation_completed, checkpoint.generation_completed);
        assert_eq!(
            loaded.generation_state.population[0]
                .lineage
                .as_ref()
                .expect("lineage")
                .parent_labels,
            vec!["parent_a".to_string(), "parent_b".to_string()]
        );
    }

    #[test]
    fn checkpoint_validation_rejects_missing_lineage_metadata() {
        let mut checkpoint = sample_checkpoint();
        checkpoint.generation_state.population[0].lineage = None;

        let error = validate_checkpoint(&checkpoint).expect_err("missing lineage must fail");

        assert!(
            error.to_string().contains("lineage"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn checkpoint_population_rejects_missing_lineage_metadata() {
        let mut checkpoint = sample_checkpoint();
        checkpoint.generation_state.population[0].lineage = None;

        let error = checkpoint_population(&checkpoint).expect_err("missing lineage must fail");

        assert!(
            error.to_string().contains("lineage"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn build_generation_state_captures_population_and_elites() {
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
            lineage: WorkflowLineage::seed("source").with_generation(4),
            topology: ScottGaTopologyIdentity::default(),
        };
        let state = build_generation_state(4, &[member]);
        assert_eq!(state.generation, 4);
        assert_eq!(state.population.len(), 1);
        assert_eq!(state.elites.len(), 1);
        assert_eq!(state.population[0].origin, "MUTATE");
        assert!(state.repopulation.is_empty());
    }

    #[test]
    fn build_generation_state_captures_repopulation_records() {
        let repop_mutation = ScottGaMember {
            source_candidate: sample_candidate("repop_mut_source"),
            result: patina_types::EvalResult {
                energy: -4.0,
                forces: vec![[0.0, 0.0, 0.0]; 2],
                relaxed_candidate: sample_candidate("repop_mut_relaxed"),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin: ScottGaOrigin::RePopM,
            occurrences: 1,
            lineage: WorkflowLineage::seed("repop_mut_source"),
            topology: ScottGaTopologyIdentity::default(),
        };
        let repop_random = ScottGaMember {
            source_candidate: sample_candidate("repop_rand_source"),
            result: patina_types::EvalResult {
                energy: f64::INFINITY,
                forces: vec![[0.0, 0.0, 0.0]; 2],
                relaxed_candidate: sample_candidate("repop_rand_relaxed"),
                converged: false,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin: ScottGaOrigin::RePopR,
            occurrences: 1,
            lineage: WorkflowLineage::seed("repop_rand_source"),
            topology: ScottGaTopologyIdentity::default(),
        };

        let state = build_generation_state(5, &[repop_mutation, repop_random]);

        assert_eq!(state.repopulation.len(), 2);
        assert_eq!(state.repopulation[0].member_id, 0);
        assert_eq!(
            state.repopulation[0].source,
            patina_types::PopulationRepopulationSource::EliteMutation
        );
        assert_eq!(
            state.repopulation[0].source_candidate_label,
            "repop_mut_source"
        );
        assert_eq!(state.repopulation[0].evaluation_label, "repop_mut_relaxed");
        assert_eq!(state.repopulation[0].energy, Some(-4.0));
        assert_eq!(state.repopulation[1].member_id, 1);
        assert_eq!(
            state.repopulation[1].source,
            patina_types::PopulationRepopulationSource::RandomStructure
        );
        assert!(!state.repopulation[1].converged);
        assert_eq!(state.repopulation[1].energy, None);
    }

    #[test]
    fn build_checkpoint_uses_typed_context() {
        let member = ScottGaMember {
            source_candidate: sample_candidate("source"),
            result: patina_types::EvalResult {
                energy: -2.5,
                forces: vec![[0.0, 0.0, 0.0]; 2],
                relaxed_candidate: sample_candidate("relaxed"),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin: ScottGaOrigin::Mutate,
            occurrences: 1,
            lineage: WorkflowLineage::seed("source").with_generation(2),
            topology: ScottGaTopologyIdentity::default(),
        };
        let checkpoint = build_checkpoint(
            &RustGaCheckpointContext {
                workflow_id: "ga.persistent-daemon",
                workflow_owner: "persistent_daemon_ga",
                backend: "janus_mace",
                backend_mode: "local-opt",
                system: "(MgO)1",
                requested_generations: 6,
            },
            &patina_types::SearchConfig {
                temperature: 10.0,
                step_size: 0.1,
                population_size: 4,
                max_steps: 6,
                seed: Some(7),
            },
            &crate::application::janus_operator_policy::select_janus_ga_operator_policy(
                crate::application::janus_operator_policy::GaOperatorBackendProfile::JanusMace,
                crate::DriverJanusMode::LocalOpt,
                0.1,
            ),
            &[
                crate::application::ga_execution::RustJanusGenerationArtifact {
                    generation: 2,
                    phase: "evolve".into(),
                    elapsed_secs: 1.0,
                    request_count: 1,
                    success_count: 1,
                    failure_count: 0,
                    failure_kind_counts: std::collections::BTreeMap::new(),
                    converged_count: 1,
                    best_energy: Some(-2.5),
                    mean_energy: Some(-2.5),
                    worst_energy: Some(-2.5),
                    boundary_population_size: 1,
                    boundary_valid_population_size: 1,
                    population_size: 1,
                    valid_population_size: 1,
                    duplicate_count: 0,
                    duplicate_hashkey_count: 0,
                    duplicate_pmoi_count: 0,
                    duplicate_energy_tol_count: 0,
                    repopulated_count: 0,
                },
            ],
            &[member],
        );
        assert_eq!(checkpoint.workflow_owner, "persistent_daemon_ga");
        assert_eq!(checkpoint.requested_generations, 6);
        assert_eq!(checkpoint.backend, "janus_mace");
        assert_eq!(checkpoint.janus_mode, "local-opt");
    }
}
