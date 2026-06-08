use crate::workflow_kernel::{SamplingSchedule, WorkflowLineage};

use super::MonteCarloAcceptance;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyLidScheduleConfig {
    pub lid_increment: f64,
    pub hold_steps: usize,
}

impl EnergyLidScheduleConfig {
    pub fn new(lid_increment: f64, hold_steps: usize) -> Self {
        Self {
            lid_increment,
            hold_steps: hold_steps.max(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyLidScheduleState {
    pub current_threshold: f64,
    pub steps_at_current_lid: usize,
}

pub fn initial_energy_lid_schedule(initial_threshold: f64) -> EnergyLidScheduleState {
    EnergyLidScheduleState {
        current_threshold: initial_threshold,
        steps_at_current_lid: 0,
    }
}

pub fn advance_energy_lid_schedule(
    state: EnergyLidScheduleState,
    config: EnergyLidScheduleConfig,
) -> EnergyLidScheduleState {
    let mut next = EnergyLidScheduleState {
        steps_at_current_lid: state.steps_at_current_lid + 1,
        ..state
    };
    if next.steps_at_current_lid >= config.hold_steps {
        next.current_threshold += config.lid_increment;
        next.steps_at_current_lid = 0;
    }
    next
}

pub fn energy_lid_acceptance(state: EnergyLidScheduleState) -> MonteCarloAcceptance {
    MonteCarloAcceptance::EnergyThreshold {
        threshold: state.current_threshold,
    }
}

pub fn energy_lid_threshold_for_lid(
    starting_energy: f64,
    lid_increment: f64,
    lid_index: usize,
) -> f64 {
    starting_energy + ((lid_index + 1) as f64 * lid_increment)
}

pub fn energy_lid_sampling_schedule(
    threshold: f64,
    lid_increment: f64,
    runners_per_lid: usize,
) -> SamplingSchedule {
    SamplingSchedule::EnergyLid {
        threshold,
        increment: lid_increment,
        runners_per_level: runners_per_lid,
    }
}

pub fn energy_lid_seed_lineage(origin_label: impl AsRef<str>) -> WorkflowLineage {
    WorkflowLineage::seed(origin_label.as_ref().to_string())
}

pub fn energy_lid_step_lineage(
    origin_label: impl AsRef<str>,
    step_index: usize,
) -> WorkflowLineage {
    energy_lid_seed_lineage(origin_label).with_step(step_index)
}

pub fn energy_lid_runner_seed_lineage(
    origin_label: impl AsRef<str>,
    lid_index: usize,
) -> WorkflowLineage {
    energy_lid_seed_lineage(origin_label).with_step(lid_index)
}

pub fn energy_lid_runner_step_lineage(
    origin_label: impl AsRef<str>,
    quench_step: usize,
) -> WorkflowLineage {
    energy_lid_seed_lineage(origin_label).with_step(quench_step)
}

#[cfg(test)]
mod tests {
    use super::{
        advance_energy_lid_schedule, energy_lid_runner_seed_lineage, energy_lid_threshold_for_lid,
        initial_energy_lid_schedule, EnergyLidScheduleConfig,
    };

    #[test]
    fn schedule_raises_threshold_after_hold_window() {
        let cfg = EnergyLidScheduleConfig::new(1.0, 2);
        let state = initial_energy_lid_schedule(-6.0);

        let held = advance_energy_lid_schedule(state, cfg);
        assert_eq!(held.current_threshold, -6.0);
        assert_eq!(held.steps_at_current_lid, 1);

        let raised = advance_energy_lid_schedule(held, cfg);
        assert_eq!(raised.current_threshold, -5.0);
        assert_eq!(raised.steps_at_current_lid, 0);
    }

    #[test]
    fn threshold_ladder_is_one_based_by_lid_level() {
        assert_eq!(energy_lid_threshold_for_lid(-10.0, 0.25, 0), -9.75);
        assert_eq!(energy_lid_threshold_for_lid(-10.0, 0.25, 3), -9.0);
    }

    #[test]
    fn runner_seed_lineage_anchors_on_lid_level() {
        let lineage = energy_lid_runner_seed_lineage("basin", 4);
        assert_eq!(lineage.origin_label, "basin");
        assert_eq!(lineage.step, Some(4));
    }
}
