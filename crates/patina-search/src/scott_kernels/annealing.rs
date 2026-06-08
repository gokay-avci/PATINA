use crate::workflow_kernel::{SamplingSchedule, WorkflowLineage};

use super::MonteCarloAcceptance;

const MIN_ANNEALING_TEMPERATURE: f64 = 1.0e-12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnealingScheduleConfig {
    pub temperature_scale: f64,
    pub hold_steps: usize,
}

impl AnnealingScheduleConfig {
    pub fn new(temperature_scale: f64, hold_steps: usize) -> Self {
        Self {
            temperature_scale,
            hold_steps: hold_steps.max(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnealingScheduleState {
    pub current_temperature: f64,
    pub steps_at_current_temperature: usize,
}

pub fn initial_annealing_schedule(initial_temperature: f64) -> AnnealingScheduleState {
    AnnealingScheduleState {
        current_temperature: initial_temperature.max(MIN_ANNEALING_TEMPERATURE),
        steps_at_current_temperature: 0,
    }
}

pub fn advance_annealing_schedule(
    state: AnnealingScheduleState,
    config: AnnealingScheduleConfig,
) -> AnnealingScheduleState {
    let mut next = AnnealingScheduleState {
        steps_at_current_temperature: state.steps_at_current_temperature + 1,
        ..state
    };
    if next.steps_at_current_temperature >= config.hold_steps {
        next.current_temperature =
            (next.current_temperature * config.temperature_scale).max(MIN_ANNEALING_TEMPERATURE);
        next.steps_at_current_temperature = 0;
    }
    next
}

pub fn annealing_acceptance(state: AnnealingScheduleState) -> MonteCarloAcceptance {
    MonteCarloAcceptance::Metropolis {
        temperature: state.current_temperature,
    }
}

pub fn annealing_sampling_schedule(
    initial_temperature: f64,
    temperature_scale: f64,
    hold_steps: usize,
) -> SamplingSchedule {
    SamplingSchedule::Annealing {
        temperature: initial_temperature.max(MIN_ANNEALING_TEMPERATURE),
        scale: temperature_scale,
        hold_steps: hold_steps.max(1),
    }
}

pub fn annealing_final_temperature_after_steps(
    initial_temperature: f64,
    temperature_scale: f64,
    hold_steps: usize,
    completed_steps: usize,
) -> f64 {
    let cooling_events = completed_steps / hold_steps.max(1);
    let initial_temperature = initial_temperature.max(MIN_ANNEALING_TEMPERATURE);
    if cooling_events == 0 {
        return initial_temperature;
    }
    if temperature_scale <= 0.0 {
        return MIN_ANNEALING_TEMPERATURE;
    }
    (initial_temperature * temperature_scale.powf(cooling_events as f64))
        .max(MIN_ANNEALING_TEMPERATURE)
}

pub fn annealing_seed_lineage(origin_label: impl AsRef<str>) -> WorkflowLineage {
    WorkflowLineage::seed(origin_label.as_ref().to_string())
}

pub fn annealing_step_lineage(origin_label: impl AsRef<str>, step_index: usize) -> WorkflowLineage {
    annealing_seed_lineage(origin_label).with_step(step_index)
}

pub fn annealing_quench_seed_lineage(
    origin_label: impl AsRef<str>,
    anneal_steps: usize,
) -> WorkflowLineage {
    annealing_seed_lineage(origin_label).with_step(anneal_steps)
}

pub fn annealing_quench_step_lineage(
    origin_label: impl AsRef<str>,
    anneal_steps: usize,
    quench_step: usize,
) -> WorkflowLineage {
    annealing_seed_lineage(origin_label).with_step(anneal_steps + quench_step)
}

#[cfg(test)]
mod tests {
    use super::{
        advance_annealing_schedule, annealing_final_temperature_after_steps,
        annealing_quench_step_lineage, initial_annealing_schedule, AnnealingScheduleConfig,
    };

    #[test]
    fn schedule_holds_temperature_until_window_completes() {
        let cfg = AnnealingScheduleConfig::new(0.5, 2);
        let state = initial_annealing_schedule(10.0);

        let held = advance_annealing_schedule(state, cfg);
        assert_eq!(held.current_temperature, 10.0);
        assert_eq!(held.steps_at_current_temperature, 1);

        let cooled = advance_annealing_schedule(held, cfg);
        assert_eq!(cooled.current_temperature, 5.0);
        assert_eq!(cooled.steps_at_current_temperature, 0);
    }

    #[test]
    fn final_temperature_matches_completed_hold_windows() {
        assert_eq!(
            annealing_final_temperature_after_steps(10.0, 0.5, 2, 5),
            2.5
        );
    }

    #[test]
    fn quench_lineage_continues_after_anneal_steps() {
        let lineage = annealing_quench_step_lineage("seed", 12, 3);
        assert_eq!(lineage.origin_label, "seed");
        assert_eq!(lineage.step, Some(15));
    }
}
