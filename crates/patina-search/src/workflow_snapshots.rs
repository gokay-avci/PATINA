use crate::{MonteCarloKernelState, SamplingSchedule};
use patina_types::{
    EnergyLidWindowState, HoldingPointRecord, RunnerBranchRecord, SimulatedAnnealingStructureState,
};

/// Builds typed snapshot records from live sampling kernels without reintroducing JSON-shaped
/// logic into the controllers themselves.
pub fn build_energy_lid_window_state(
    lid_index: usize,
    threshold: f64,
    active_basin: impl Into<String>,
    lid_kernel: &MonteCarloKernelState,
    runner_kernels: &[MonteCarloKernelState],
) -> EnergyLidWindowState {
    let holding_point = build_holding_point_record(lid_kernel);
    EnergyLidWindowState {
        lid_index,
        threshold,
        active_basin: active_basin.into(),
        holding_point: holding_point.clone(),
        lid_walker: lid_kernel.snapshot_state(),
        runner_branches: runner_kernels
            .iter()
            .enumerate()
            .filter_map(|(runner_index, runner)| {
                build_runner_branch_record(runner_index, holding_point.as_ref(), runner)
            })
            .collect(),
        runner_walkers: runner_kernels
            .iter()
            .map(MonteCarloKernelState::snapshot_state)
            .collect(),
    }
}

pub fn build_simulated_annealing_structure_state(
    anneal_kernel: &MonteCarloKernelState,
    quench_kernel: Option<&MonteCarloKernelState>,
) -> SimulatedAnnealingStructureState {
    let holding_point = build_holding_point_record(anneal_kernel);
    SimulatedAnnealingStructureState {
        holding_point: holding_point.clone(),
        anneal_walker: anneal_kernel.snapshot_state(),
        quench_branch: quench_kernel
            .and_then(|kernel| build_runner_branch_record(0, holding_point.as_ref(), kernel)),
        quench_walker: quench_kernel.map(MonteCarloKernelState::snapshot_state),
    }
}

fn build_holding_point_record(kernel: &MonteCarloKernelState) -> Option<HoldingPointRecord> {
    let member = kernel.best().or_else(|| kernel.current())?;
    let (threshold, temperature) = match kernel.schedule {
        SamplingSchedule::Quench => (None, None),
        SamplingSchedule::FixedTemperature { temperature } => (None, Some(temperature)),
        SamplingSchedule::Annealing { temperature, .. } => (None, Some(temperature)),
        SamplingSchedule::EnergyLid { threshold, .. } => (Some(threshold), None),
    };
    Some(HoldingPointRecord {
        label: member.result.relaxed_candidate.label.clone(),
        energy: member.result.energy,
        recorded_step: kernel.step,
        threshold,
        temperature,
    })
}

fn build_runner_branch_record(
    runner_index: usize,
    holding_point: Option<&HoldingPointRecord>,
    kernel: &MonteCarloKernelState,
) -> Option<RunnerBranchRecord> {
    let holding_point = holding_point?;
    let final_member = kernel.best().or_else(|| kernel.current());
    Some(RunnerBranchRecord {
        runner_index,
        origin_label: holding_point.label.clone(),
        origin_energy: holding_point.energy,
        final_label: final_member.map(|member| member.result.relaxed_candidate.label.clone()),
        final_energy: final_member.map(|member| member.result.energy),
        accepted_steps: kernel.accepted_steps,
        rejected_steps: kernel.rejected_steps,
    })
}
