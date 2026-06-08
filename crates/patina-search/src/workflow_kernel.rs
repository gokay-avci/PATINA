use patina_types::{
    Candidate, EvalResult, EvaluationRecord, Population, SamplingCheckpointSchedule,
    SamplingRejectionRecord, SamplingWalkerState, SamplingWorkflowFamily,
};
use std::collections::VecDeque;

/// Workflow families that should own typed Rust controller state in process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowFamily {
    ProductionRun,
    GeneticAlgorithm,
    BasinHopping,
    SolidSolutions,
    ScanSurface,
    SimulatedAnnealing,
    EnergyLid,
    HybridGaProduction,
}

impl WorkflowFamily {
    /// Returns `true` when the workflow is population-driven.
    pub fn is_population_based(self) -> bool {
        matches!(self, Self::GeneticAlgorithm | Self::HybridGaProduction)
    }

    /// Returns `true` when the workflow follows a Monte-Carlo-like walker lifecycle.
    pub fn is_sampling_family(self) -> bool {
        matches!(
            self,
            Self::BasinHopping
                | Self::SolidSolutions
                | Self::ScanSurface
                | Self::SimulatedAnnealing
                | Self::EnergyLid
        )
    }
}

/// Fine-grained controller phase used for typed dispatch and provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowPhase {
    SeedIntake,
    Evaluation,
    Selection,
    Mutation,
    SamplingStep,
    BestSetUpdate,
    Completed,
}

/// Minimal lineage carried alongside in-process workflow tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowLineage {
    pub origin_label: String,
    pub generation: Option<usize>,
    pub step: Option<usize>,
    pub parent_labels: Vec<String>,
    pub attempt: usize,
}

impl WorkflowLineage {
    pub fn seed(label: impl Into<String>) -> Self {
        Self {
            origin_label: label.into(),
            generation: None,
            step: None,
            parent_labels: Vec::new(),
            attempt: 1,
        }
    }

    pub fn with_generation(mut self, generation: usize) -> Self {
        self.generation = Some(generation);
        self
    }

    pub fn with_step(mut self, step: usize) -> Self {
        self.step = Some(step);
        self
    }
}

/// Typed evaluation task emitted by workflow controllers.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowEvaluationTask {
    pub family: WorkflowFamily,
    pub phase: WorkflowPhase,
    pub candidate: Candidate,
    pub lineage: WorkflowLineage,
}

/// Accepted member stored by typed workflow kernels.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowMember {
    pub source: Candidate,
    pub result: EvalResult,
    pub lineage: WorkflowLineage,
    pub occurrences: usize,
}

/// Compact rejection record retained by workflow kernels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRejection {
    pub family: WorkflowFamily,
    pub phase: WorkflowPhase,
    pub candidate_label: String,
    pub attempt: usize,
    pub message: String,
}

/// Sampling schedule state that belongs in the typed controller kernel, not in JSON artifacts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SamplingSchedule {
    Quench,
    FixedTemperature {
        temperature: f64,
    },
    Annealing {
        temperature: f64,
        scale: f64,
        hold_steps: usize,
    },
    EnergyLid {
        threshold: f64,
        increment: f64,
        runners_per_level: usize,
    },
}

/// Errors returned by typed workflow kernels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowKernelError {
    TaskAlreadyQueued { family: WorkflowFamily },
}

/// Typed controller state for `ProductionRun.f90`-style workflows.
#[derive(Debug, Clone)]
pub struct ProductionKernelState {
    pending: VecDeque<WorkflowEvaluationTask>,
    accepted: Vec<WorkflowMember>,
    rejected: Vec<WorkflowRejection>,
    best_set: Vec<WorkflowMember>,
    best_set_limit: usize,
}

impl ProductionKernelState {
    pub fn new(best_set_limit: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            accepted: Vec::new(),
            rejected: Vec::new(),
            best_set: Vec::new(),
            best_set_limit,
        }
    }

    pub fn queue_candidate(&mut self, candidate: Candidate, lineage: WorkflowLineage) {
        self.pending.push_back(WorkflowEvaluationTask {
            family: WorkflowFamily::ProductionRun,
            phase: WorkflowPhase::Evaluation,
            candidate,
            lineage,
        });
    }

    pub fn next_task(&mut self) -> Option<WorkflowEvaluationTask> {
        self.pending.pop_front()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn accepted(&self) -> &[WorkflowMember] {
        &self.accepted
    }

    pub fn rejected(&self) -> &[WorkflowRejection] {
        &self.rejected
    }

    pub fn best_set(&self) -> &[WorkflowMember] {
        &self.best_set
    }

    pub fn record_accept(&mut self, task: WorkflowEvaluationTask, result: EvalResult) {
        let member = WorkflowMember {
            source: task.candidate,
            result,
            lineage: task.lineage,
            occurrences: 1,
        };
        self.accepted.push(member.clone());
        insert_ranked_by_energy(&mut self.best_set, member, self.best_set_limit);
    }

    pub fn record_rejection(&mut self, task: &WorkflowEvaluationTask, message: impl Into<String>) {
        self.rejected.push(WorkflowRejection {
            family: task.family,
            phase: task.phase,
            candidate_label: task.candidate.label.clone(),
            attempt: task.lineage.attempt,
            message: message.into(),
        });
    }
}

/// Typed controller state for `Population.f90`-style workflows.
#[derive(Debug, Clone)]
pub struct PopulationKernelState {
    pub family: WorkflowFamily,
    pub generation: usize,
    pub population: Population,
    pending: VecDeque<WorkflowEvaluationTask>,
    members: Vec<WorkflowMember>,
    elites: Vec<WorkflowMember>,
}

impl PopulationKernelState {
    pub fn new(family: WorkflowFamily) -> Self {
        debug_assert!(family.is_population_based());
        Self {
            family,
            generation: 0,
            population: Population::default(),
            pending: VecDeque::new(),
            members: Vec::new(),
            elites: Vec::new(),
        }
    }

    /// Resets the state to represent one concrete generation snapshot.
    pub fn begin_generation(&mut self, generation: usize) {
        self.generation = generation;
        self.population = Population::default();
        self.pending.clear();
        self.members.clear();
        self.elites.clear();
    }

    pub fn queue_candidate(&mut self, candidate: Candidate, lineage: WorkflowLineage) {
        self.pending.push_back(WorkflowEvaluationTask {
            family: self.family,
            phase: WorkflowPhase::Evaluation,
            candidate,
            lineage,
        });
    }

    pub fn next_task(&mut self) -> Option<WorkflowEvaluationTask> {
        self.pending.pop_front()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn members(&self) -> &[WorkflowMember] {
        &self.members
    }

    pub fn elites(&self) -> &[WorkflowMember] {
        &self.elites
    }

    pub fn advance_generation(&mut self) {
        self.generation += 1;
    }

    pub fn record_member(
        &mut self,
        task: WorkflowEvaluationTask,
        result: EvalResult,
        elite_limit: usize,
    ) {
        self.population
            .insert(result.clone(), task.candidate.clone());
        let member = WorkflowMember {
            source: task.candidate,
            result,
            lineage: task.lineage,
            occurrences: 1,
        };
        self.members.push(member.clone());
        insert_ranked_by_energy(&mut self.elites, member, elite_limit);
    }
}

/// Typed controller state for `MonteCarlo.f90`-style workflows.
#[derive(Debug, Clone)]
pub struct MonteCarloKernelState {
    pub family: WorkflowFamily,
    pub schedule: SamplingSchedule,
    pub step: usize,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
    pending: Option<WorkflowEvaluationTask>,
    current: Option<WorkflowMember>,
    best: Option<WorkflowMember>,
    last_rejection: Option<WorkflowRejection>,
}

impl MonteCarloKernelState {
    pub fn new(family: WorkflowFamily, schedule: SamplingSchedule) -> Self {
        debug_assert!(family.is_sampling_family());
        Self {
            family,
            schedule,
            step: 0,
            accepted_steps: 0,
            rejected_steps: 0,
            pending: None,
            current: None,
            best: None,
            last_rejection: None,
        }
    }

    pub fn queue_candidate(
        &mut self,
        candidate: Candidate,
        lineage: WorkflowLineage,
    ) -> Result<(), WorkflowKernelError> {
        if self.pending.is_some() {
            return Err(WorkflowKernelError::TaskAlreadyQueued {
                family: self.family,
            });
        }
        self.pending = Some(WorkflowEvaluationTask {
            family: self.family,
            phase: WorkflowPhase::SamplingStep,
            candidate,
            lineage,
        });
        Ok(())
    }

    pub fn take_task(&mut self) -> Option<WorkflowEvaluationTask> {
        self.pending.take()
    }

    pub fn current(&self) -> Option<&WorkflowMember> {
        self.current.as_ref()
    }

    pub fn best(&self) -> Option<&WorkflowMember> {
        self.best.as_ref()
    }

    pub fn last_rejection(&self) -> Option<&WorkflowRejection> {
        self.last_rejection.as_ref()
    }

    pub fn seed_current(
        &mut self,
        candidate: Candidate,
        result: EvalResult,
        lineage: WorkflowLineage,
    ) {
        let member = WorkflowMember {
            source: candidate,
            result,
            lineage,
            occurrences: 1,
        };
        if self
            .best
            .as_ref()
            .map(|best| member.result.energy < best.result.energy)
            .unwrap_or(true)
        {
            self.best = Some(member.clone());
        }
        self.current = Some(member);
        self.last_rejection = None;
    }

    pub fn record_accept(&mut self, task: WorkflowEvaluationTask, result: EvalResult) {
        self.accepted_steps += 1;
        self.step = self.step.max(task.lineage.step.unwrap_or(self.step)) + 1;
        let member = WorkflowMember {
            source: task.candidate,
            result,
            lineage: task.lineage,
            occurrences: 1,
        };
        if self
            .best
            .as_ref()
            .map(|best| member.result.energy < best.result.energy)
            .unwrap_or(true)
        {
            self.best = Some(member.clone());
        }
        self.current = Some(member);
        self.last_rejection = None;
    }

    pub fn record_rejection(&mut self, task: &WorkflowEvaluationTask, message: impl Into<String>) {
        self.rejected_steps += 1;
        self.step = self.step.max(task.lineage.step.unwrap_or(self.step)) + 1;
        self.last_rejection = Some(WorkflowRejection {
            family: task.family,
            phase: task.phase,
            candidate_label: task.candidate.label.clone(),
            attempt: task.lineage.attempt,
            message: message.into(),
        });
    }

    pub fn snapshot_state(&self) -> SamplingWalkerState {
        SamplingWalkerState {
            family: sampling_family_record(self.family),
            schedule: sampling_schedule_record(self.schedule),
            step: self.step,
            accepted_steps: self.accepted_steps,
            rejected_steps: self.rejected_steps,
            current: self
                .current
                .as_ref()
                .map(|member| EvaluationRecord::from(&member.result)),
            best: self
                .best
                .as_ref()
                .map(|member| EvaluationRecord::from(&member.result)),
            last_rejection: self
                .last_rejection
                .as_ref()
                .map(|rejection| SamplingRejectionRecord {
                    candidate_label: rejection.candidate_label.clone(),
                    attempt: rejection.attempt,
                    message: rejection.message.clone(),
                }),
        }
    }
}

fn sampling_family_record(family: WorkflowFamily) -> SamplingWorkflowFamily {
    match family {
        WorkflowFamily::BasinHopping => SamplingWorkflowFamily::BasinHopping,
        WorkflowFamily::SolidSolutions => SamplingWorkflowFamily::SolidSolutions,
        WorkflowFamily::ScanSurface => SamplingWorkflowFamily::ScanSurface,
        WorkflowFamily::SimulatedAnnealing => SamplingWorkflowFamily::SimulatedAnnealing,
        WorkflowFamily::EnergyLid => SamplingWorkflowFamily::EnergyLid,
        WorkflowFamily::ProductionRun
        | WorkflowFamily::GeneticAlgorithm
        | WorkflowFamily::HybridGaProduction => {
            unreachable!("non-sampling workflow family cannot be snapshotted as a sampling walker")
        }
    }
}

fn sampling_schedule_record(schedule: SamplingSchedule) -> SamplingCheckpointSchedule {
    match schedule {
        SamplingSchedule::Quench => SamplingCheckpointSchedule::Quench,
        SamplingSchedule::FixedTemperature { temperature } => {
            SamplingCheckpointSchedule::FixedTemperature { temperature }
        }
        SamplingSchedule::Annealing {
            temperature,
            scale,
            hold_steps,
        } => SamplingCheckpointSchedule::Annealing {
            temperature,
            scale,
            hold_steps,
        },
        SamplingSchedule::EnergyLid {
            threshold,
            increment,
            runners_per_level,
        } => SamplingCheckpointSchedule::EnergyLid {
            threshold,
            increment,
            runners_per_level,
        },
    }
}

fn insert_ranked_by_energy(entries: &mut Vec<WorkflowMember>, entry: WorkflowMember, limit: usize) {
    entries.push(entry);
    entries.sort_by(|left, right| {
        left.result
            .energy
            .partial_cmp(&right.result.energy)
            .unwrap_or(std::cmp::Ordering::Greater)
    });
    if limit > 0 && entries.len() > limit {
        entries.truncate(limit);
    }
}
