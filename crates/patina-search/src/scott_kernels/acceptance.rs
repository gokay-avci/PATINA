/// Shared Monte Carlo-style acceptance rules used across Scott-shaped workflows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MonteCarloAcceptance {
    Quench,
    Metropolis { temperature: f64 },
    EnergyThreshold { threshold: f64 },
}

/// Decision returned by a pure acceptance kernel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcceptanceDecision {
    pub accepted: bool,
    pub delta: f64,
    pub probability: f64,
}

/// Energy comparison evidence used for BH acceptance decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BhEnergyComparison {
    pub current_energy: Option<f64>,
    pub candidate_energy: f64,
    pub matched_relaxation_level: Option<usize>,
}

impl BhEnergyComparison {
    pub fn from_final_energies(current_energy: Option<f64>, candidate_energy: f64) -> Self {
        Self {
            current_energy,
            candidate_energy,
            matched_relaxation_level: None,
        }
    }
}

pub fn metropolis_acceptance(
    delta_old_minus_new_or_candidate_energy: f64,
    mode: MonteCarloAcceptance,
    random_draw: f64,
) -> AcceptanceDecision {
    match mode {
        MonteCarloAcceptance::EnergyThreshold { threshold } => AcceptanceDecision {
            accepted: delta_old_minus_new_or_candidate_energy < threshold,
            delta: delta_old_minus_new_or_candidate_energy,
            probability: if delta_old_minus_new_or_candidate_energy < threshold {
                1.0
            } else {
                0.0
            },
        },
        MonteCarloAcceptance::Quench => AcceptanceDecision {
            accepted: delta_old_minus_new_or_candidate_energy > 0.0,
            delta: delta_old_minus_new_or_candidate_energy,
            probability: if delta_old_minus_new_or_candidate_energy > 0.0 {
                1.0
            } else {
                0.0
            },
        },
        MonteCarloAcceptance::Metropolis { temperature } => {
            if delta_old_minus_new_or_candidate_energy.abs() < f64::EPSILON
                || delta_old_minus_new_or_candidate_energy > 0.0
            {
                AcceptanceDecision {
                    accepted: true,
                    delta: delta_old_minus_new_or_candidate_energy,
                    probability: 1.0,
                }
            } else {
                let k_t = temperature.max(1.0e-12);
                let probability = (delta_old_minus_new_or_candidate_energy / k_t).exp();
                AcceptanceDecision {
                    accepted: probability > random_draw,
                    delta: delta_old_minus_new_or_candidate_energy,
                    probability,
                }
            }
        }
    }
}

pub fn accept_energy_transition(
    current_energy: Option<f64>,
    candidate_energy: f64,
    mode: MonteCarloAcceptance,
    random_draw: f64,
) -> AcceptanceDecision {
    match current_energy {
        Some(current_energy) => match mode {
            MonteCarloAcceptance::EnergyThreshold { .. } => {
                metropolis_acceptance(candidate_energy, mode, random_draw)
            }
            _ => metropolis_acceptance(current_energy - candidate_energy, mode, random_draw),
        },
        None => AcceptanceDecision {
            accepted: true,
            delta: 0.0,
            probability: 1.0,
        },
    }
}

pub fn accept_bh_energy_comparison(
    comparison: BhEnergyComparison,
    mode: MonteCarloAcceptance,
    random_draw: f64,
) -> AcceptanceDecision {
    accept_energy_transition(
        comparison.current_energy,
        comparison.candidate_energy,
        mode,
        random_draw,
    )
}
