use crate::{
    assert_native_supported_search_periodicity, crossover_candidate, geometry_is_reasonable,
    mutate_candidate, naive_split_crossover, random_foreign_structure, self_crossover_candidate,
    standardize_candidate_coordinates, DuplicatePolicy, ScottDuplicateClassifier,
    ScottDuplicateReason, ScottGaGenerationTrace, ScottGaMember, ScottGaOperatorConfig,
    ScottGaOrigin, ScottGaPendingRepopulation, ScottGaPopulationTransition, ScottGaStage,
    ScottGaTopologyIdentity, ScottGeometryModel, ScottParityGeneticAlgorithm, SearchLifecycleError,
    SearchResult, TinyRng, WorkflowLineage,
};
use patina_types::{Candidate, EvalResult, SearchConfig};
use std::cmp::Ordering;

#[derive(Debug, Default, Clone, Copy)]
struct DuplicateSummary {
    total: usize,
    hashkey: usize,
    pmoi: usize,
    energy_tol: usize,
}

impl ScottParityGeneticAlgorithm<ScottDuplicateClassifier> {
    pub fn new(base: Candidate, seed: u64) -> Self {
        Self::with_configs(
            base,
            seed,
            ScottGaOperatorConfig::default(),
            ScottDuplicateClassifier::default(),
        )
    }
}

impl<D> ScottParityGeneticAlgorithm<D>
where
    D: DuplicatePolicy,
{
    fn cfg_for(&self, operation: &'static str) -> SearchResult<&SearchConfig> {
        self.cfg.as_ref().ok_or_else(|| {
            SearchLifecycleError::not_initialized("ScottParityGeneticAlgorithm", operation)
        })
    }

    pub fn with_configs(
        base: Candidate,
        seed: u64,
        operator_cfg: ScottGaOperatorConfig,
        duplicate_policy: D,
    ) -> Self {
        Self {
            geometry_model: ScottGeometryModel::from_base(&base),
            base,
            rng: TinyRng::new(seed),
            cfg: None,
            operator_cfg,
            duplicate_policy,
            trace: Vec::new(),
        }
    }

    pub fn with_duplicate_policy(base: Candidate, seed: u64, duplicate_policy: D) -> Self {
        Self::with_configs(
            base,
            seed,
            ScottGaOperatorConfig::default(),
            duplicate_policy,
        )
    }

    pub fn initialize_population(&mut self, cfg: &SearchConfig) -> Vec<Candidate> {
        assert_native_supported_search_periodicity(&self.base, "initialize_population");
        self.cfg = Some(cfg.clone());
        self.trace.clear();
        let target = cfg.population_size.max(2);
        let mut population = Vec::with_capacity(target);

        let mut seed = self.base.clone();
        standardize_candidate_coordinates(&mut seed);
        seed.label = "ga_seed_0000".to_string();
        population.push(seed);

        for idx in 1..target {
            let mut candidate = self.generate_foreign_structure_candidate(cfg, idx);
            candidate.label = format!("ga_seed_{idx:04}");
            population.push(candidate);
        }

        population
    }

    pub fn configure_for_run(&mut self, cfg: &SearchConfig) {
        assert_native_supported_search_periodicity(&self.base, "configure_for_run");
        self.cfg = Some(cfg.clone());
        self.trace.clear();
    }

    pub fn generate_initial_candidate(&mut self, idx: usize) -> SearchResult<Candidate> {
        assert_native_supported_search_periodicity(&self.base, "generate_initial_candidate");
        let cfg = self.cfg_for("generate_initial_candidate")?.clone();
        if idx == 0 {
            let mut seed = self.base.clone();
            standardize_candidate_coordinates(&mut seed);
            seed.label = format!("ga_seed_{idx:04}");
            return Ok(seed);
        }

        let mut candidate = self.generate_foreign_structure_candidate(&cfg, idx);
        candidate.label = format!("ga_seed_{idx:04}");
        Ok(candidate)
    }

    pub fn selection_tournament(&mut self, population: &[ScottGaMember]) -> Vec<usize> {
        let ranked = self.sorted_valid_indices(population);
        if ranked.is_empty() {
            return Vec::new();
        }

        let tournament_size = ranked.len().clamp(
            self.operator_cfg.tournament_size_min,
            self.operator_cfg.tournament_size_max,
        );
        (0..ranked.len())
            .map(|slot| {
                let mut champion = ranked[slot];
                for _ in 1..tournament_size {
                    let contender = ranked[self.rng.next_usize(ranked.len())];
                    if ScottGaMember::rank_cmp(&population[contender], &population[champion])
                        == Ordering::Less
                    {
                        champion = contender;
                    }
                }
                champion
            })
            .collect()
    }

    pub fn spawn_children(
        &mut self,
        generation: usize,
        population: &[ScottGaMember],
        selected: &[usize],
    ) -> SearchResult<Vec<(Candidate, ScottGaOrigin, WorkflowLineage)>> {
        let step_size = self.cfg_for("spawn_children")?.step_size;
        let ranked = self.sorted_valid_indices(population);
        if ranked.is_empty() {
            return Ok(Vec::new());
        }
        Ok(selected
            .iter()
            .enumerate()
            .map(|(idx, &left_idx)| {
                let right_idx = ranked[self.rng.next_usize(ranked.len())];
                let left_parent = &population[left_idx].result.relaxed_candidate;
                let right_parent = &population[right_idx].result.relaxed_candidate;
                let mut child = crossover_candidate(
                    left_parent,
                    right_parent,
                    &mut self.rng,
                    self.operator_cfg,
                    &self.geometry_model,
                );
                if !geometry_is_reasonable(&child, &self.geometry_model) {
                    child = naive_split_crossover(left_parent, right_parent, &mut self.rng);
                    standardize_candidate_coordinates(&mut child);
                }
                child.label = format!("ga_gen_{generation:04}_{idx:04}");

                let mut origin = ScottGaOrigin::Crosso;
                if self.rng.next_f64() < self.operator_cfg.mutation_ratio {
                    if self.rng.next_f64() < self.operator_cfg.mut_selfcross_ratio {
                        child = self_crossover_candidate(
                            &child,
                            step_size,
                            &mut self.rng,
                            self.operator_cfg,
                        );
                        origin = ScottGaOrigin::Mutcrs;
                    } else {
                        let moves = self.estimated_mutation_moves(child.len());
                        mutate_candidate(
                            &mut child,
                            moves,
                            step_size,
                            &mut self.rng,
                            self.operator_cfg,
                        );
                        origin = ScottGaOrigin::Mutate;
                    }
                }

                (
                    child.clone(),
                    origin,
                    WorkflowLineage {
                        origin_label: child.label.clone(),
                        generation: Some(generation),
                        step: None,
                        parent_labels: vec![
                            population[left_idx].source_candidate.label.clone(),
                            population[right_idx].source_candidate.label.clone(),
                        ],
                        attempt: 1,
                    },
                )
            })
            .collect())
    }

    pub fn ingest_initial_population(
        &mut self,
        evaluated: Vec<(Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>,
    ) -> SearchResult<Vec<ScottGaMember>> {
        Ok(self
            .ingest_initial_population_with_boundary(evaluated)?
            .working_population)
    }

    pub fn ingest_initial_population_with_boundary(
        &mut self,
        evaluated: Vec<(Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>,
    ) -> SearchResult<ScottGaPopulationTransition> {
        let target_size = self
            .cfg_for("ingest_initial_population_with_boundary")?
            .population_size
            .max(2);

        let mut members: Vec<ScottGaMember> = evaluated
            .into_iter()
            .map(|(candidate, result, origin, lineage)| {
                ScottGaMember::new(candidate, result, origin, lineage)
            })
            .collect();
        let duplicate_summary = self.remove_duplicates(&mut members);
        members.sort_by(ScottGaMember::rank_cmp);
        members.truncate(target_size);
        let boundary_population = members.clone();
        let repopulated_count = self.repopulate_members(&mut members, target_size, 0)?;
        members.sort_by(ScottGaMember::rank_cmp);
        let duplicate_summary_after_repop = self.remove_duplicates(&mut members);
        members.sort_by(ScottGaMember::rank_cmp);

        self.trace.push(ScottGaGenerationTrace {
            generation: 0,
            stage: ScottGaStage::Initialize,
            selected_indices: Vec::new(),
            population_size: members.len(),
            valid_population_size: members.iter().filter(|member| member.is_valid()).count(),
            child_count: members.len(),
            duplicate_count: duplicate_summary.total + duplicate_summary_after_repop.total,
            duplicate_hashkey_count: duplicate_summary.hashkey
                + duplicate_summary_after_repop.hashkey,
            duplicate_pmoi_count: duplicate_summary.pmoi + duplicate_summary_after_repop.pmoi,
            duplicate_energy_tol_count: duplicate_summary.energy_tol
                + duplicate_summary_after_repop.energy_tol,
            repopulated_count,
            best_energy: members.first().map(|member| member.result.energy),
        });

        Ok(ScottGaPopulationTransition {
            boundary_population,
            working_population: members,
        })
    }

    pub fn integrate_generation(
        &mut self,
        generation: usize,
        population: &[ScottGaMember],
        evaluated_children: Vec<(Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>,
    ) -> SearchResult<Vec<ScottGaMember>> {
        Ok(self
            .integrate_generation_with_boundary(generation, population, evaluated_children)?
            .working_population)
    }

    pub fn integrate_generation_with_boundary(
        &mut self,
        generation: usize,
        population: &[ScottGaMember],
        evaluated_children: Vec<(Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>,
    ) -> SearchResult<ScottGaPopulationTransition> {
        let keep = self
            .cfg_for("integrate_generation_with_boundary")?
            .population_size
            .max(2);

        let mut children: Vec<ScottGaMember> = evaluated_children
            .into_iter()
            .map(|(candidate, result, origin, lineage)| {
                ScottGaMember::new(candidate, result, origin, lineage)
            })
            .collect();

        let child_duplicate_summary = self.remove_duplicates(&mut children);
        children.sort_by(ScottGaMember::rank_cmp);

        let mut next = population.to_vec();
        next.sort_by(ScottGaMember::rank_cmp);
        next.truncate(keep);
        let len_children = children.iter().filter(|member| member.is_valid()).count();
        let len_pop = next.iter().filter(|member| member.is_valid()).count();
        let offset =
            ((self.operator_cfg.pop_replacement_ratio * keep as f64).floor() as usize).min(keep);
        let replacement_count = offset.min(len_children);

        if self.operator_cfg.pop_replacement_ratio > 0.95 {
            next = children.into_iter().take(keep).collect();
        } else {
            let incoming: Vec<_> = children.into_iter().take(replacement_count).collect();
            if len_pop + replacement_count <= keep {
                for (offset_idx, child) in incoming.into_iter().enumerate() {
                    let insert_idx = len_pop + offset_idx;
                    if insert_idx < next.len() {
                        next[insert_idx] = child;
                    } else {
                        next.push(child);
                    }
                }
            } else {
                let start = keep.saturating_sub(replacement_count);
                for (offset_idx, child) in incoming.into_iter().enumerate() {
                    let insert_idx = start + offset_idx;
                    if insert_idx < next.len() {
                        next[insert_idx] = child;
                    } else {
                        next.push(child);
                    }
                }
            }
        }

        next.sort_by(ScottGaMember::rank_cmp);
        let duplicate_summary_after_merge = self.remove_duplicates(&mut next);
        next.sort_by(ScottGaMember::rank_cmp);

        let boundary_population = next.clone();
        let repopulated_count = self.repopulate_members(&mut next, keep, generation)?;
        next.sort_by(ScottGaMember::rank_cmp);
        let duplicate_summary_after_repop = self.remove_duplicates(&mut next);
        next.sort_by(ScottGaMember::rank_cmp);

        self.trace.push(ScottGaGenerationTrace {
            generation,
            stage: ScottGaStage::Replacement,
            selected_indices: Vec::new(),
            population_size: next.len(),
            valid_population_size: next.iter().filter(|member| member.is_valid()).count(),
            child_count: replacement_count,
            duplicate_count: child_duplicate_summary.total
                + duplicate_summary_after_merge.total
                + duplicate_summary_after_repop.total,
            duplicate_hashkey_count: child_duplicate_summary.hashkey
                + duplicate_summary_after_merge.hashkey
                + duplicate_summary_after_repop.hashkey,
            duplicate_pmoi_count: child_duplicate_summary.pmoi
                + duplicate_summary_after_merge.pmoi
                + duplicate_summary_after_repop.pmoi,
            duplicate_energy_tol_count: child_duplicate_summary.energy_tol
                + duplicate_summary_after_merge.energy_tol
                + duplicate_summary_after_repop.energy_tol,
            repopulated_count,
            best_energy: next.first().map(|member| member.result.energy),
        });

        Ok(ScottGaPopulationTransition {
            boundary_population,
            working_population: next,
        })
    }

    pub fn record_selection_trace(
        &mut self,
        generation: usize,
        population: &[ScottGaMember],
        selected: &[usize],
    ) {
        self.trace.push(ScottGaGenerationTrace {
            generation,
            stage: ScottGaStage::Selection,
            selected_indices: selected.to_vec(),
            population_size: population.len(),
            valid_population_size: population.iter().filter(|member| member.is_valid()).count(),
            child_count: 0,
            duplicate_count: 0,
            duplicate_hashkey_count: 0,
            duplicate_pmoi_count: 0,
            duplicate_energy_tol_count: 0,
            repopulated_count: 0,
            best_energy: population
                .iter()
                .filter(|member| member.is_valid())
                .min_by(|left, right| ScottGaMember::rank_cmp(left, right))
                .map(|member| member.result.energy),
        });
    }

    pub fn trace(&self) -> &[ScottGaGenerationTrace] {
        &self.trace
    }

    pub fn pending_repopulation(
        &self,
        population: &[ScottGaMember],
    ) -> Vec<ScottGaPendingRepopulation> {
        population
            .iter()
            .enumerate()
            .filter(|(_, member)| !member.is_valid())
            .map(|(slot_index, member)| ScottGaPendingRepopulation {
                slot_index,
                candidate: member.source_candidate.clone(),
                origin: member.origin,
                lineage: member.lineage.clone(),
            })
            .collect()
    }

    pub fn absorb_repopulation_results(
        &mut self,
        generation: usize,
        population: &mut Vec<ScottGaMember>,
        evaluated: Vec<(usize, Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>,
    ) -> SearchResult<usize> {
        let target_size = self
            .cfg_for("absorb_repopulation_results")?
            .population_size
            .max(2);
        if evaluated.is_empty() {
            return Ok(0);
        }

        for (slot_index, candidate, result, origin, lineage) in evaluated {
            if let Some(slot) = population.get_mut(slot_index) {
                slot.source_candidate = candidate;
                slot.result = result;
                slot.origin = origin;
                slot.occurrences = 1;
                slot.lineage = lineage;
                slot.topology = ScottGaTopologyIdentity::default();
            }
        }

        population.sort_by(ScottGaMember::rank_cmp);
        let duplicate_summary = self.remove_duplicates(population);
        population.sort_by(ScottGaMember::rank_cmp);
        let repopulated_count = self.repopulate_members(population, target_size, generation)?;
        population.sort_by(ScottGaMember::rank_cmp);
        let duplicate_summary_after_repop = self.remove_duplicates(population);
        population.sort_by(ScottGaMember::rank_cmp);

        if let Some(trace) = self.trace.last_mut() {
            if matches!(trace.stage, ScottGaStage::Replacement) {
                trace.population_size = population.len();
                trace.valid_population_size =
                    population.iter().filter(|member| member.is_valid()).count();
                trace.duplicate_count +=
                    duplicate_summary.total + duplicate_summary_after_repop.total;
                trace.duplicate_hashkey_count +=
                    duplicate_summary.hashkey + duplicate_summary_after_repop.hashkey;
                trace.duplicate_pmoi_count +=
                    duplicate_summary.pmoi + duplicate_summary_after_repop.pmoi;
                trace.duplicate_energy_tol_count +=
                    duplicate_summary.energy_tol + duplicate_summary_after_repop.energy_tol;
                trace.repopulated_count += repopulated_count;
                trace.best_energy = population.first().map(|member| member.result.energy);
            }
        }

        Ok(repopulated_count)
    }

    fn sorted_valid_indices(&self, population: &[ScottGaMember]) -> Vec<usize> {
        let mut ranked: Vec<_> = population
            .iter()
            .enumerate()
            .filter(|(_, member)| member.is_valid())
            .map(|(idx, _)| idx)
            .collect();
        ranked.sort_by(|&left, &right| {
            ScottGaMember::rank_cmp(&population[left], &population[right])
        });
        ranked
    }

    fn remove_duplicates(&self, members: &mut Vec<ScottGaMember>) -> DuplicateSummary {
        let mut summary = DuplicateSummary::default();
        if members.len() <= 1 {
            return summary;
        }

        let mut i = members.len();
        while i > 0 {
            i -= 1;
            if !members[i].is_valid() {
                continue;
            }

            let mut removed_current = false;
            let mut j = i;
            while j > 0 {
                j -= 1;
                if !members[j].is_valid() {
                    continue;
                }

                if let Some(reason) = self.duplicate_policy.classify(&members[i], &members[j]) {
                    let (drop_idx, keep_idx) =
                        if ScottGaMember::rank_cmp(&members[i], &members[j]) == Ordering::Less {
                            (j, i)
                        } else {
                            (i, j)
                        };

                    let dropped_occurrences = members[drop_idx].occurrences;
                    members[keep_idx].occurrences += dropped_occurrences;
                    members.remove(drop_idx);

                    summary.total += 1;
                    match reason {
                        ScottDuplicateReason::Hashkey => summary.hashkey += 1,
                        ScottDuplicateReason::Pmoi => summary.pmoi += 1,
                        ScottDuplicateReason::EnergyTol => summary.energy_tol += 1,
                    }

                    if drop_idx == i {
                        removed_current = true;
                    }
                    break;
                }
            }

            if removed_current {
                continue;
            }
        }
        summary
    }

    fn repopulate_members(
        &mut self,
        members: &mut Vec<ScottGaMember>,
        target_size: usize,
        generation: usize,
    ) -> SearchResult<usize> {
        let cfg = self.cfg_for("repopulate_members")?.clone();
        let valid_templates: Vec<Candidate> = members
            .iter()
            .filter(|member| member.is_valid())
            .map(|member| member.result.relaxed_candidate.clone())
            .collect();
        let mut repopulated = 0usize;

        for (idx, slot) in members.iter_mut().enumerate() {
            if slot.is_valid() {
                continue;
            }

            let (candidate, origin, lineage) =
                self.build_repopulation_candidate(&cfg, &valid_templates, idx, generation);
            slot.source_candidate = candidate.clone();
            slot.result = EvalResult {
                energy: f64::INFINITY,
                forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
                relaxed_candidate: candidate,
                converged: false,
                wall_time: std::time::Duration::from_secs(0),
            };
            slot.origin = origin;
            slot.occurrences = 1;
            slot.lineage = lineage;
            slot.topology = ScottGaTopologyIdentity::default();
            repopulated += 1;
        }

        while members.len() < target_size {
            let next_index = members.len();
            let (candidate, origin, lineage) =
                self.build_repopulation_candidate(&cfg, &valid_templates, next_index, generation);
            members.push(ScottGaMember {
                source_candidate: candidate.clone(),
                result: EvalResult {
                    energy: f64::INFINITY,
                    forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
                    relaxed_candidate: candidate,
                    converged: false,
                    wall_time: std::time::Duration::from_secs(0),
                },
                origin,
                occurrences: 1,
                lineage,
                topology: ScottGaTopologyIdentity::default(),
            });
            repopulated += 1;
        }

        Ok(repopulated)
    }

    fn build_repopulation_candidate(
        &mut self,
        cfg: &SearchConfig,
        valid_templates: &[Candidate],
        idx: usize,
        generation: usize,
    ) -> (Candidate, ScottGaOrigin, WorkflowLineage) {
        let use_elite_mutation = !valid_templates.is_empty()
            && self.rng.next_f64() < self.operator_cfg.reinsert_elites_ratio;

        for attempt in 0..self.operator_cfg.max_repop_attempts {
            let (mut candidate, origin, parent_labels) = if use_elite_mutation {
                let choice = self.rng.next_usize(valid_templates.len());
                let mut candidate = valid_templates[choice].clone();
                let moves = self.estimated_mutation_moves(candidate.len());
                mutate_candidate(
                    &mut candidate,
                    moves,
                    cfg.step_size.max(0.25),
                    &mut self.rng,
                    self.operator_cfg,
                );
                (
                    candidate,
                    ScottGaOrigin::RePopM,
                    vec![valid_templates[choice].label.clone()],
                )
            } else {
                (
                    self.generate_foreign_structure_candidate(cfg, idx + attempt),
                    ScottGaOrigin::RePopR,
                    Vec::new(),
                )
            };
            candidate.label = format!("ga_repop_{idx:04}_{attempt:02}");
            if geometry_is_reasonable(&candidate, &self.geometry_model) {
                return (
                    candidate.clone(),
                    origin,
                    WorkflowLineage {
                        origin_label: candidate.label,
                        generation: Some(generation),
                        step: None,
                        parent_labels,
                        attempt: attempt + 1,
                    },
                );
            }
        }

        let mut fallback = self.base.clone();
        fallback.label = format!("ga_repop_fallback_{idx:04}");
        let moves = self.estimated_mutation_moves(fallback.len());
        mutate_candidate(
            &mut fallback,
            moves,
            cfg.step_size.max(0.25),
            &mut self.rng,
            self.operator_cfg,
        );
        let lineage = WorkflowLineage {
            origin_label: fallback.label.clone(),
            generation: Some(generation),
            step: None,
            parent_labels: vec![self.base.label.clone()],
            attempt: self.operator_cfg.max_repop_attempts + 1,
        };
        (fallback, ScottGaOrigin::RePopR, lineage)
    }

    fn generate_foreign_structure_candidate(
        &mut self,
        cfg: &SearchConfig,
        idx: usize,
    ) -> Candidate {
        for attempt in 0..self.operator_cfg.max_repop_attempts {
            let mut candidate =
                random_foreign_structure(&self.base, &self.geometry_model, &mut self.rng);
            candidate.label = format!("ga_rand_{idx:04}_{attempt:02}");
            if geometry_is_reasonable(&candidate, &self.geometry_model) {
                return candidate;
            }
        }

        let mut fallback = self.base.clone();
        fallback.label = format!("ga_rand_fallback_{idx:04}");
        let moves = self.estimated_mutation_moves(fallback.len());
        mutate_candidate(
            &mut fallback,
            moves,
            cfg.step_size.max(0.25),
            &mut self.rng,
            self.operator_cfg,
        );
        fallback
    }

    fn estimated_mutation_moves(&self, atom_count: usize) -> usize {
        match atom_count {
            0..=4 => 1,
            5..=8 => 2,
            9..=12 => 3,
            13..=16 => 4,
            _ => 5,
        }
    }
}
