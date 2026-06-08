use crate::candidate_ops::{
    assert_native_supported_search_periodicity, perturb_candidate, ScottGeometryModel, TinyRng,
};
use crate::operator_kernels::crossover_candidate;
use crate::scott_ga_parity::ScottGaOperatorConfig;
use crate::scott_kernels::{
    advance_annealing_schedule, advance_bh_step_control, advance_energy_lid_schedule,
    annealing_acceptance, apply_bh_move_class, energy_lid_acceptance, finalize_bh_step_control,
    initial_annealing_schedule, initial_energy_lid_schedule, metropolis_acceptance,
    sample_bh_move_plan, AnnealingScheduleConfig, AnnealingScheduleState, BhEnergyComparison,
    BhStepControlState, EnergyLidScheduleConfig, EnergyLidScheduleState, MonteCarloAcceptance,
};
use crate::{SearchLifecycleError, SearchResult};
use patina_types::{Candidate, EvalResult, Population, SearchConfig};
use std::cmp::Ordering;

/// Result bundle returned by a search algorithm after evaluating a batch.
#[derive(Debug, Clone)]
pub struct SearchUpdate {
    /// Best energy observed in this update, if any.
    pub best_energy: Option<f64>,
    /// Number of successful evaluations incorporated into the population.
    pub accepted: usize,
}

#[derive(Debug, Clone)]
pub struct BhStepTrace {
    pub step: usize,
    pub walker_id: usize,
    pub accepted: bool,
    pub energy: Option<f64>,
    pub best_energy: Option<f64>,
    pub temperature: f64,
    pub step_size: f64,
    pub move_class: BhMoveClass,
    pub label: String,
    pub reason: String,
    pub matched_relaxation_level: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct McStepTrace {
    pub step: usize,
    pub accepted: bool,
    pub energy: Option<f64>,
    pub best_energy: Option<f64>,
    pub temperature: f64,
    pub threshold: Option<f64>,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BhMoveClass {
    MonteCarlo,
    SwapCations,
    SwapAtoms,
    MutateCluster,
    TwistCluster,
    TranslateCluster,
    RotateCluster,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BhMethod {
    Relax,
    Fixed,
    Oscillate {
        high_temperature_steps: usize,
        low_temperature_steps: usize,
    },
}

impl BhMethod {
    pub fn uses_relaxation(self) -> bool {
        !matches!(self, Self::Fixed)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BhMoveClassPolicy {
    pub dynamic_step_threshold: usize,
    pub moveclass_threshold: usize,
    pub max_dynamic_step_multiplier: f64,
    pub prob_swap_cations: f64,
    pub enable_after_rejections: usize,
    pub prob_translate_surface: f64,
    pub prob_rotate_surface: f64,
    pub prob_swap_atoms: f64,
    pub prob_mutate_cluster: f64,
    pub prob_twist_cluster: f64,
}

impl Default for BhMoveClassPolicy {
    fn default() -> Self {
        Self {
            dynamic_step_threshold: 20,
            moveclass_threshold: 50,
            max_dynamic_step_multiplier: 3.5,
            prob_swap_cations: 0.1,
            enable_after_rejections: 20,
            prob_translate_surface: 0.0,
            prob_rotate_surface: 0.0,
            prob_swap_atoms: 0.1,
            prob_mutate_cluster: 0.2,
            prob_twist_cluster: 0.2,
        }
    }
}

/// Shared interface for Rust-side search controllers.
pub trait SearchAlgorithm {
    /// Creates the initial candidate batch.
    fn initialize(&mut self, cfg: &SearchConfig) -> Vec<Candidate>;
    /// Produces the next candidate batch from the current population.
    fn next_candidates(&mut self, pop: &Population) -> SearchResult<Vec<Candidate>>;
    /// Applies successful evaluations to the population.
    fn update(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
    ) -> SearchResult<SearchUpdate>;
    /// Reports whether the algorithm has converged.
    fn converged(&self, pop: &Population) -> SearchResult<bool>;
}

/// Basin-hopping controller using deterministic multi-start walkers.
#[derive(Debug, Clone)]
pub struct BasinHopping {
    base: Candidate,
    rng: TinyRng,
    cfg: Option<SearchConfig>,
    step: usize,
    pub(crate) acceptance: MonteCarloAcceptance,
    base_acceptance: MonteCarloAcceptance,
    method: BhMethod,
    move_policy: BhMoveClassPolicy,
    move_regime: BhStepControlState,
    last_move_classes: Vec<BhMoveClass>,
    last_step_sizes: Vec<f64>,
    trace: Vec<BhStepTrace>,
}

impl BasinHopping {
    /// Creates a BH controller around a base candidate.
    pub fn new(base: Candidate, seed: u64) -> Self {
        assert_native_supported_search_periodicity(&base, "BasinHopping::new");
        Self {
            base,
            rng: TinyRng::new(seed),
            cfg: None,
            step: 0,
            acceptance: MonteCarloAcceptance::Metropolis { temperature: 1.0 },
            base_acceptance: MonteCarloAcceptance::Metropolis { temperature: 1.0 },
            method: BhMethod::Relax,
            move_policy: BhMoveClassPolicy::default(),
            move_regime: BhStepControlState {
                rejection_counter: 0,
                random_moveclass: false,
                current_step_size: 0.0,
            },
            last_move_classes: Vec::new(),
            last_step_sizes: Vec::new(),
            trace: Vec::new(),
        }
    }

    pub fn with_acceptance(base: Candidate, seed: u64, acceptance: MonteCarloAcceptance) -> Self {
        assert_native_supported_search_periodicity(&base, "BasinHopping::with_acceptance");
        Self {
            base,
            rng: TinyRng::new(seed),
            cfg: None,
            step: 0,
            acceptance,
            base_acceptance: acceptance,
            method: BhMethod::Relax,
            move_policy: BhMoveClassPolicy::default(),
            move_regime: BhStepControlState {
                rejection_counter: 0,
                random_moveclass: false,
                current_step_size: 0.0,
            },
            last_move_classes: Vec::new(),
            last_step_sizes: Vec::new(),
            trace: Vec::new(),
        }
    }

    pub fn with_move_policy(mut self, move_policy: BhMoveClassPolicy) -> Self {
        self.move_policy = move_policy;
        self
    }

    pub fn with_method(mut self, method: BhMethod) -> Self {
        self.method = method;
        self
    }

    pub fn trace(&self) -> &[BhStepTrace] {
        &self.trace
    }

    pub fn configured_acceptance(&self) -> MonteCarloAcceptance {
        self.base_acceptance
    }

    pub fn active_acceptance(&self) -> MonteCarloAcceptance {
        self.acceptance
    }

    pub fn method(&self) -> BhMethod {
        self.method
    }

    pub fn move_policy(&self) -> BhMoveClassPolicy {
        self.move_policy
    }

    pub fn move_regime(&self) -> BhStepControlState {
        self.move_regime
    }

    pub fn current_step(&self) -> usize {
        self.step
    }

    pub fn update_with_energy_comparisons(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
        comparisons: &[BhEnergyComparison],
    ) -> SearchResult<SearchUpdate> {
        self.update_impl(pop, evaluated, comparisons)
    }

    fn refresh_acceptance_from_cfg(&mut self, cfg: &SearchConfig) {
        self.acceptance = match self.base_acceptance {
            MonteCarloAcceptance::Metropolis { .. } => MonteCarloAcceptance::Metropolis {
                temperature: cfg.temperature,
            },
            other => other,
        };
    }

    fn advance_method_state(&mut self, cfg: &SearchConfig) {
        match self.method {
            BhMethod::Relax => {}
            BhMethod::Fixed => {}
            BhMethod::Oscillate {
                high_temperature_steps,
                low_temperature_steps,
            } => match self.acceptance {
                MonteCarloAcceptance::Metropolis { .. } => {
                    if high_temperature_steps > 0
                        && self.step.is_multiple_of(high_temperature_steps)
                    {
                        self.acceptance = MonteCarloAcceptance::Quench;
                    }
                }
                MonteCarloAcceptance::Quench => {
                    if low_temperature_steps > 0 && self.step.is_multiple_of(low_temperature_steps)
                    {
                        self.refresh_acceptance_from_cfg(cfg);
                    }
                }
                MonteCarloAcceptance::EnergyThreshold { .. } => {}
            },
        }
    }
}

impl SearchAlgorithm for BasinHopping {
    fn initialize(&mut self, cfg: &SearchConfig) -> Vec<Candidate> {
        self.cfg = Some(cfg.clone());
        self.trace.clear();
        self.last_move_classes.clear();
        self.last_step_sizes.clear();
        self.move_regime = BhStepControlState {
            rejection_counter: 0,
            random_moveclass: false,
            current_step_size: cfg.step_size,
        };
        self.refresh_acceptance_from_cfg(cfg);
        let n_walkers = cfg.population_size.max(1);
        (0..n_walkers)
            .map(|idx| {
                let mut candidate = self.base.clone();
                candidate.label = format!("bh_init_{idx:04}");
                perturb_candidate(&mut candidate, cfg.step_size, &mut self.rng);
                candidate
            })
            .collect()
    }

    fn next_candidates(&mut self, pop: &Population) -> SearchResult<Vec<Candidate>> {
        let cfg = self.cfg.as_ref().ok_or_else(|| {
            SearchLifecycleError::not_initialized("BasinHopping", "next_candidates")
        })?;
        let population_size = cfg.population_size.max(1);
        let step_size = cfg.step_size;
        let sources: Vec<Candidate> = if pop.members.is_empty() {
            vec![self.base.clone()]
        } else {
            pop.members
                .iter()
                .take(population_size)
                .map(|(_, candidate)| candidate.clone())
                .collect()
        };

        self.step += 1;
        self.last_move_classes.clear();
        self.last_step_sizes.clear();
        self.move_regime = advance_bh_step_control(self.move_regime, step_size, self.move_policy);
        Ok(sources
            .into_iter()
            .enumerate()
            .map(|(idx, mut candidate)| {
                candidate.label = format!("bh_step_{:04}_{idx:04}", self.step);
                let move_plan =
                    sample_bh_move_plan(self.move_regime, self.move_policy, self.rng.next_f64());
                self.last_move_classes.push(move_plan.move_class);
                self.last_step_sizes.push(move_plan.step_size);
                apply_bh_move_class(
                    &mut candidate,
                    move_plan.move_class,
                    move_plan.step_size,
                    &mut self.rng,
                );
                candidate
            })
            .collect())
    }

    fn update(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
    ) -> SearchResult<SearchUpdate> {
        let comparisons = evaluated
            .iter()
            .enumerate()
            .map(|(idx, (_, result))| {
                let current_energy = pop.members.get(idx).map(|(current, _)| current.energy);
                BhEnergyComparison::from_final_energies(current_energy, result.energy)
            })
            .collect::<Vec<_>>();
        self.update_impl(pop, evaluated, &comparisons)
    }

    fn converged(&self, pop: &Population) -> SearchResult<bool> {
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| SearchLifecycleError::not_initialized("BasinHopping", "converged"))?;
        Ok(pop.generation >= cfg.max_steps)
    }
}

impl BasinHopping {
    fn update_impl(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
        comparisons: &[BhEnergyComparison],
    ) -> SearchResult<SearchUpdate> {
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| SearchLifecycleError::not_initialized("BasinHopping", "update"))?;
        let population_size = cfg.population_size.max(1);
        let base_step_size = cfg.step_size;
        let base_temperature = cfg.temperature;
        let max_steps = cfg.max_steps;
        let seed = cfg.seed;
        let mut walkers: Vec<(EvalResult, Candidate)> = if pop.members.is_empty() {
            Vec::new()
        } else {
            pop.members.iter().take(population_size).cloned().collect()
        };
        let trace_acceptance = self.acceptance;
        let mut accepted = 0usize;
        let mut accepted_flags = Vec::with_capacity(evaluated.len());

        for (idx, (candidate, result)) in evaluated.iter().enumerate() {
            let comparison = comparisons.get(idx).copied().unwrap_or_else(|| {
                BhEnergyComparison::from_final_energies(
                    walkers
                        .get(idx)
                        .map(|(current_result, _)| current_result.energy),
                    result.energy,
                )
            });
            let accept = crate::decide_bh_acceptance_from_comparison(
                comparison,
                self.acceptance,
                self.rng.next_f64(),
            )
            .accepted;

            if accept {
                if let Some(slot) = walkers.get_mut(idx) {
                    *slot = (result.clone(), candidate.clone());
                } else {
                    walkers.push((result.clone(), candidate.clone()));
                }
                accepted += 1;
            }
            accepted_flags.push(accept);
        }

        walkers.sort_by(|(left, _), (right, _)| {
            left.energy
                .partial_cmp(&right.energy)
                .unwrap_or(Ordering::Greater)
        });
        let keep = population_size;
        if walkers.len() > keep {
            walkers.truncate(keep);
        }
        pop.members = walkers;
        pop.generation += 1;
        self.move_regime = finalize_bh_step_control(self.move_regime, accepted > 0, base_step_size);

        let method_cfg = SearchConfig {
            temperature: base_temperature,
            step_size: base_step_size,
            population_size,
            max_steps,
            seed,
        };
        self.advance_method_state(&method_cfg);
        let best_energy = pop.best().map(|(result, _)| result.energy);
        for (idx, (_, result)) in evaluated.iter().enumerate() {
            let move_class = self
                .last_move_classes
                .get(idx)
                .copied()
                .unwrap_or(BhMoveClass::MonteCarlo);
            let step_size = self
                .last_step_sizes
                .get(idx)
                .copied()
                .unwrap_or(base_step_size);
            let accepted_step = accepted_flags.get(idx).copied().unwrap_or(false);
            self.trace.push(BhStepTrace {
                step: self.step,
                walker_id: idx,
                accepted: accepted_step,
                energy: Some(result.energy),
                best_energy,
                temperature: match trace_acceptance {
                    MonteCarloAcceptance::Metropolis { temperature } => temperature,
                    MonteCarloAcceptance::Quench => 0.0,
                    MonteCarloAcceptance::EnergyThreshold { threshold } => threshold,
                },
                step_size,
                move_class,
                label: evaluated[idx].0.label.clone(),
                reason: if accepted_step {
                    "accepted".into()
                } else {
                    "rejected".into()
                },
                matched_relaxation_level: comparisons
                    .get(idx)
                    .and_then(|comparison| comparison.matched_relaxation_level),
            });
        }

        Ok(SearchUpdate {
            best_energy,
            accepted,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MonteCarlo {
    base: Candidate,
    rng: TinyRng,
    cfg: Option<SearchConfig>,
    step: usize,
    pub(crate) acceptance: MonteCarloAcceptance,
    trace: Vec<McStepTrace>,
}

impl MonteCarlo {
    pub fn new(base: Candidate, seed: u64, acceptance: MonteCarloAcceptance) -> Self {
        assert_native_supported_search_periodicity(&base, "MonteCarlo::new");
        Self {
            base,
            rng: TinyRng::new(seed),
            cfg: None,
            step: 0,
            acceptance,
            trace: Vec::new(),
        }
    }

    pub fn trace(&self) -> &[McStepTrace] {
        &self.trace
    }
}

impl SearchAlgorithm for MonteCarlo {
    fn initialize(&mut self, cfg: &SearchConfig) -> Vec<Candidate> {
        self.cfg = Some(cfg.clone());
        self.trace.clear();
        self.step = 0;
        vec![self.base.clone()]
    }

    fn next_candidates(&mut self, pop: &Population) -> SearchResult<Vec<Candidate>> {
        let step_size = self
            .cfg
            .as_ref()
            .ok_or_else(|| SearchLifecycleError::not_initialized("MonteCarlo", "next_candidates"))?
            .step_size;
        let mut candidate = pop
            .members
            .first()
            .map(|(_, candidate)| candidate.clone())
            .unwrap_or_else(|| self.base.clone());
        self.step += 1;
        candidate.label = format!("mc_step_{:04}", self.step);
        perturb_candidate(&mut candidate, step_size, &mut self.rng);
        Ok(vec![candidate])
    }

    fn update(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
    ) -> SearchResult<SearchUpdate> {
        let temperature = self
            .cfg
            .as_ref()
            .ok_or_else(|| SearchLifecycleError::not_initialized("MonteCarlo", "update"))?
            .temperature;
        let Some((candidate, result)) = evaluated.first() else {
            return Ok(SearchUpdate {
                best_energy: pop.best().map(|(result, _)| result.energy),
                accepted: 0,
            });
        };
        let effective_acceptance = match self.acceptance {
            MonteCarloAcceptance::Metropolis { .. } => {
                MonteCarloAcceptance::Metropolis { temperature }
            }
            other => other,
        };

        let accept = match pop.best() {
            None => true,
            Some((current, _)) => match effective_acceptance {
                MonteCarloAcceptance::EnergyThreshold { threshold } => result.energy < threshold,
                other => {
                    metropolis_acceptance(
                        current.energy - result.energy,
                        other,
                        self.rng.next_f64(),
                    )
                    .accepted
                }
            },
        };

        if accept {
            pop.members = vec![(result.clone(), candidate.clone())];
        }
        pop.generation += 1;
        self.trace.push(McStepTrace {
            step: self.step,
            accepted: accept,
            energy: Some(result.energy),
            best_energy: pop.best().map(|(result, _)| result.energy),
            temperature: match effective_acceptance {
                MonteCarloAcceptance::Metropolis { temperature } => temperature,
                MonteCarloAcceptance::Quench => 0.0,
                MonteCarloAcceptance::EnergyThreshold { .. } => 0.0,
            },
            threshold: match effective_acceptance {
                MonteCarloAcceptance::EnergyThreshold { threshold } => Some(threshold),
                _ => None,
            },
            label: candidate.label.clone(),
            reason: if accept {
                "accepted".into()
            } else {
                "rejected".into()
            },
        });

        Ok(SearchUpdate {
            best_energy: pop.best().map(|(result, _)| result.energy),
            accepted: usize::from(accept),
        })
    }

    fn converged(&self, pop: &Population) -> SearchResult<bool> {
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| SearchLifecycleError::not_initialized("MonteCarlo", "converged"))?;
        Ok(pop.generation >= cfg.max_steps)
    }
}

#[derive(Debug, Clone)]
pub struct SimulatedAnnealing {
    inner: MonteCarlo,
    schedule_config: AnnealingScheduleConfig,
    schedule_state: AnnealingScheduleState,
}

impl SimulatedAnnealing {
    pub fn new(
        base: Candidate,
        seed: u64,
        initial_temperature: f64,
        temperature_scale: f64,
    ) -> Self {
        Self {
            inner: MonteCarlo::new(
                base,
                seed,
                MonteCarloAcceptance::Metropolis {
                    temperature: initial_temperature,
                },
            ),
            schedule_config: AnnealingScheduleConfig::new(temperature_scale, 1),
            schedule_state: initial_annealing_schedule(initial_temperature),
        }
    }

    pub fn with_hold_steps(mut self, hold_steps: usize) -> Self {
        self.schedule_config =
            AnnealingScheduleConfig::new(self.schedule_config.temperature_scale, hold_steps);
        self
    }

    pub fn trace(&self) -> &[McStepTrace] {
        self.inner.trace()
    }

    pub fn current_temperature(&self) -> f64 {
        self.schedule_state.current_temperature
    }
}

impl SearchAlgorithm for SimulatedAnnealing {
    fn initialize(&mut self, cfg: &SearchConfig) -> Vec<Candidate> {
        self.schedule_state = initial_annealing_schedule(cfg.temperature);
        self.inner.acceptance = annealing_acceptance(self.schedule_state);
        self.inner.initialize(cfg)
    }

    fn next_candidates(&mut self, pop: &Population) -> SearchResult<Vec<Candidate>> {
        self.inner.next_candidates(pop)
    }

    fn update(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
    ) -> SearchResult<SearchUpdate> {
        self.inner.acceptance = annealing_acceptance(self.schedule_state);
        let update = self.inner.update(pop, evaluated)?;
        self.schedule_state = advance_annealing_schedule(self.schedule_state, self.schedule_config);
        Ok(update)
    }

    fn converged(&self, pop: &Population) -> SearchResult<bool> {
        self.inner.converged(pop)
    }
}

#[derive(Debug, Clone)]
pub struct EnergyLid {
    inner: MonteCarlo,
    schedule_config: EnergyLidScheduleConfig,
    schedule_state: EnergyLidScheduleState,
}

impl EnergyLid {
    pub fn new(base: Candidate, seed: u64, threshold: f64) -> Self {
        Self {
            inner: MonteCarlo::new(
                base,
                seed,
                MonteCarloAcceptance::EnergyThreshold { threshold },
            ),
            schedule_config: EnergyLidScheduleConfig::new(0.0, 1),
            schedule_state: initial_energy_lid_schedule(threshold),
        }
    }

    pub fn with_ladder(mut self, lid_increment: f64, hold_steps: usize) -> Self {
        self.schedule_config = EnergyLidScheduleConfig::new(lid_increment, hold_steps);
        self
    }

    pub fn trace(&self) -> &[McStepTrace] {
        self.inner.trace()
    }

    pub fn current_threshold(&self) -> f64 {
        self.schedule_state.current_threshold
    }
}

impl SearchAlgorithm for EnergyLid {
    fn initialize(&mut self, cfg: &SearchConfig) -> Vec<Candidate> {
        self.schedule_state = EnergyLidScheduleState {
            steps_at_current_lid: 0,
            ..self.schedule_state
        };
        self.inner.acceptance = energy_lid_acceptance(self.schedule_state);
        self.inner.initialize(cfg)
    }

    fn next_candidates(&mut self, pop: &Population) -> SearchResult<Vec<Candidate>> {
        self.inner.next_candidates(pop)
    }

    fn update(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
    ) -> SearchResult<SearchUpdate> {
        self.inner.acceptance = energy_lid_acceptance(self.schedule_state);
        let update = self.inner.update(pop, evaluated)?;
        self.schedule_state =
            advance_energy_lid_schedule(self.schedule_state, self.schedule_config);
        Ok(update)
    }

    fn converged(&self, pop: &Population) -> SearchResult<bool> {
        self.inner.converged(pop)
    }
}

/// Simple genetic algorithm controller.
#[derive(Debug, Clone)]
pub struct GeneticAlgorithm {
    base: Candidate,
    rng: TinyRng,
    cfg: Option<SearchConfig>,
}

impl GeneticAlgorithm {
    /// Creates a GA controller around a base candidate.
    pub fn new(base: Candidate, seed: u64) -> Self {
        Self {
            base,
            rng: TinyRng::new(seed),
            cfg: None,
        }
    }
}

impl SearchAlgorithm for GeneticAlgorithm {
    fn initialize(&mut self, cfg: &SearchConfig) -> Vec<Candidate> {
        self.cfg = Some(cfg.clone());
        (0..cfg.population_size.max(2))
            .map(|idx| {
                let mut candidate = self.base.clone();
                candidate.label = format!("ga_init_{idx:04}");
                perturb_candidate(&mut candidate, cfg.step_size, &mut self.rng);
                candidate
            })
            .collect()
    }

    fn next_candidates(&mut self, pop: &Population) -> SearchResult<Vec<Candidate>> {
        let cfg = self.cfg.as_ref().ok_or_else(|| {
            SearchLifecycleError::not_initialized("GeneticAlgorithm", "next_candidates")
        })?;
        let population_size = cfg.population_size.max(2);
        let step_size = cfg.step_size;
        let parents: Vec<Candidate> = if pop.members.is_empty() {
            vec![self.base.clone(), self.base.clone()]
        } else {
            pop.members
                .iter()
                .take(population_size)
                .map(|(_, candidate)| candidate.clone())
                .collect()
        };

        Ok((0..population_size)
            .map(|idx| {
                let left = self.rng.next_usize(parents.len());
                let right = self.rng.next_usize(parents.len());
                let mut child = crossover_candidate(
                    &parents[left],
                    &parents[right],
                    &mut self.rng,
                    ScottGaOperatorConfig::default(),
                    &ScottGeometryModel::from_base(&self.base),
                );
                child.label = format!("ga_gen_{:04}_{idx:04}", pop.generation + 1);
                perturb_candidate(&mut child, step_size * 0.5, &mut self.rng);
                child
            })
            .collect())
    }

    fn update(
        &mut self,
        pop: &mut Population,
        evaluated: &[(Candidate, EvalResult)],
    ) -> SearchResult<SearchUpdate> {
        let keep = self
            .cfg
            .as_ref()
            .ok_or_else(|| SearchLifecycleError::not_initialized("GeneticAlgorithm", "update"))?
            .population_size
            .max(2);
        let mut next_population = Population {
            members: pop.members.clone(),
            generation: pop.generation + 1,
        };

        for (candidate, result) in evaluated {
            next_population.insert(result.clone(), candidate.clone());
        }

        if next_population.members.len() > keep {
            next_population.members.truncate(keep);
        }

        let accepted = evaluated.len();
        *pop = next_population;

        Ok(SearchUpdate {
            best_energy: pop.best().map(|(result, _)| result.energy),
            accepted,
        })
    }

    fn converged(&self, pop: &Population) -> SearchResult<bool> {
        let cfg = self.cfg.as_ref().ok_or_else(|| {
            SearchLifecycleError::not_initialized("GeneticAlgorithm", "converged")
        })?;
        Ok(pop.generation >= cfg.max_steps)
    }
}
