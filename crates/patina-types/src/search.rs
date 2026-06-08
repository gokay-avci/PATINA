use crate::{Candidate, EvalResult};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Population state for population-based search algorithms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Population {
    /// Evaluated members kept sorted by ascending energy.
    pub members: Vec<(EvalResult, Candidate)>,
    /// Generation counter.
    pub generation: usize,
}

impl Population {
    /// Inserts a new member and maintains ascending energy order.
    pub fn insert(&mut self, result: EvalResult, candidate: Candidate) {
        self.members.push((result, candidate));
        self.members.sort_by(|(left, _), (right, _)| {
            left.energy
                .partial_cmp(&right.energy)
                .unwrap_or(Ordering::Greater)
        });
    }

    /// Returns the best member, if any.
    pub fn best(&self) -> Option<&(EvalResult, Candidate)> {
        self.members.first()
    }
}

/// Hyperparameters shared by search algorithms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Search temperature for Monte Carlo or acceptance criteria.
    pub temperature: f64,
    /// Perturbation magnitude for BH-style moves.
    pub step_size: f64,
    /// Population size for GA.
    pub population_size: usize,
    /// Maximum search steps or generations.
    pub max_steps: usize,
    /// Optional random seed for deterministic runs.
    pub seed: Option<u64>,
}
