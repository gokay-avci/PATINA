use super::{
    build_energy_lid_window_state, build_simulated_annealing_structure_state,
    MonteCarloKernelState, PopulationKernelState, ProductionKernelState, SamplingSchedule,
    WorkflowFamily, WorkflowLineage,
};
use patina_types::{Candidate, EvalResult};
use std::time::Duration;

fn candidate(label: &str) -> Candidate {
    Candidate::cluster(label, vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
}

fn result(label: &str, energy: f64) -> EvalResult {
    EvalResult {
        energy,
        forces: vec![[0.0, 0.0, 0.0]],
        relaxed_candidate: candidate(label),
        converged: true,
        wall_time: Duration::from_secs(0),
    }
}

#[test]
fn production_kernel_keeps_ranked_best_set() {
    let mut state = ProductionKernelState::new(2);
    state.queue_candidate(candidate("seed_a"), WorkflowLineage::seed("seed"));
    state.queue_candidate(candidate("seed_b"), WorkflowLineage::seed("seed"));
    state.queue_candidate(candidate("seed_c"), WorkflowLineage::seed("seed"));

    let task_a = state.next_task().expect("task a");
    let task_b = state.next_task().expect("task b");
    let task_c = state.next_task().expect("task c");
    state.record_accept(task_a, result("relaxed_a", -1.0));
    state.record_accept(task_b, result("relaxed_b", -3.0));
    state.record_accept(task_c, result("relaxed_c", -2.0));

    assert_eq!(state.accepted().len(), 3);
    assert_eq!(state.best_set().len(), 2);
    assert_eq!(state.best_set()[0].result.energy, -3.0);
    assert_eq!(state.best_set()[1].result.energy, -2.0);
}

#[test]
fn population_kernel_updates_population_and_elites() {
    let mut state = PopulationKernelState::new(WorkflowFamily::GeneticAlgorithm);
    state.begin_generation(0);
    state.queue_candidate(
        candidate("child_a"),
        WorkflowLineage::seed("seed").with_generation(1),
    );
    state.queue_candidate(
        candidate("child_b"),
        WorkflowLineage::seed("seed").with_generation(1),
    );

    let task_a = state.next_task().expect("task a");
    let task_b = state.next_task().expect("task b");
    state.record_member(task_a, result("relaxed_a", -5.0), 1);
    state.record_member(task_b, result("relaxed_b", -4.0), 1);
    state.advance_generation();

    assert_eq!(state.generation, 1);
    assert_eq!(state.population.members.len(), 2);
    assert_eq!(state.population.members[0].0.energy, -5.0);
    assert_eq!(state.elites().len(), 1);
    assert_eq!(state.elites()[0].result.energy, -5.0);
}

#[test]
fn population_kernel_begin_generation_resets_snapshot_state() {
    let mut state = PopulationKernelState::new(WorkflowFamily::GeneticAlgorithm);
    state.begin_generation(0);
    state.queue_candidate(
        candidate("child"),
        WorkflowLineage::seed("seed").with_generation(0),
    );
    let task = state.next_task().expect("task");
    state.record_member(task, result("relaxed", -1.0), 1);

    state.begin_generation(1);

    assert_eq!(state.generation, 1);
    assert!(state.population.members.is_empty());
    assert!(state.members().is_empty());
    assert!(state.elites().is_empty());
    assert_eq!(state.pending_count(), 0);
}

#[test]
fn monte_carlo_kernel_tracks_current_best_and_rejections() {
    let mut state = MonteCarloKernelState::new(
        WorkflowFamily::SimulatedAnnealing,
        SamplingSchedule::Annealing {
            temperature: 100.0,
            scale: 0.95,
            hold_steps: 4,
        },
    );
    state.seed_current(
        candidate("seed"),
        result("relaxed_seed", -1.5),
        WorkflowLineage::seed("seed"),
    );
    state
        .queue_candidate(
            candidate("start"),
            WorkflowLineage::seed("seed").with_step(0),
        )
        .expect("queue start");
    let task = state.take_task().expect("task");
    state.record_accept(task, result("relaxed_start", -2.0));

    state
        .queue_candidate(
            candidate("trial"),
            WorkflowLineage::seed("seed").with_step(1),
        )
        .expect("queue trial");
    let task = state.take_task().expect("task");
    state.record_rejection(&task, "threshold exceeded");

    assert_eq!(state.accepted_steps, 1);
    assert_eq!(state.rejected_steps, 1);
    assert_eq!(state.current().expect("current").result.energy, -2.0);
    assert_eq!(state.best().expect("best").result.energy, -2.0);
    assert_eq!(
        state
            .last_rejection()
            .expect("last rejection")
            .candidate_label,
        "trial"
    );
}

#[test]
fn monte_carlo_kernel_snapshot_preserves_schedule_and_rejection() {
    let mut state = MonteCarloKernelState::new(
        WorkflowFamily::EnergyLid,
        SamplingSchedule::EnergyLid {
            threshold: -1.2,
            increment: 0.05,
            runners_per_level: 2,
        },
    );
    state.seed_current(
        candidate("seed"),
        result("seed_relaxed", -1.5),
        WorkflowLineage::seed("seed"),
    );
    state
        .queue_candidate(
            candidate("trial"),
            WorkflowLineage::seed("seed").with_step(0),
        )
        .expect("queue");
    let task = state.take_task().expect("task");
    state.record_rejection(&task, "threshold reject");

    let snapshot = state.snapshot_state();

    assert_eq!(snapshot.accepted_steps, 0);
    assert_eq!(snapshot.rejected_steps, 1);
    assert_eq!(snapshot.current.as_ref().expect("current").energy, -1.5);
    assert_eq!(
        snapshot.last_rejection.as_ref().expect("rejection").message,
        "threshold reject"
    );
    match snapshot.schedule {
        patina_types::SamplingCheckpointSchedule::EnergyLid {
            threshold,
            increment,
            runners_per_level,
        } => {
            assert_eq!(threshold, -1.2);
            assert_eq!(increment, 0.05);
            assert_eq!(runners_per_level, 2);
        }
        other => panic!("unexpected schedule snapshot: {other:?}"),
    }
}

#[test]
fn snapshot_builders_capture_lid_and_annealing_states() {
    let mut lid = MonteCarloKernelState::new(
        WorkflowFamily::EnergyLid,
        SamplingSchedule::EnergyLid {
            threshold: -2.0,
            increment: 0.1,
            runners_per_level: 1,
        },
    );
    lid.seed_current(
        candidate("lid_seed"),
        result("lid_relaxed", -2.5),
        WorkflowLineage::seed("seed"),
    );
    let mut runner =
        MonteCarloKernelState::new(WorkflowFamily::EnergyLid, SamplingSchedule::Quench);
    runner.seed_current(
        candidate("runner_seed"),
        result("runner_relaxed", -2.7),
        WorkflowLineage::seed("seed").with_step(1),
    );

    let lid_state =
        build_energy_lid_window_state(3, -2.0, "basin_a", &lid, std::slice::from_ref(&runner));
    assert_eq!(lid_state.lid_index, 3);
    assert_eq!(lid_state.active_basin, "basin_a");
    assert_eq!(
        lid_state
            .holding_point
            .as_ref()
            .map(|record| record.label.as_str()),
        Some("lid_relaxed")
    );
    assert_eq!(lid_state.runner_branches.len(), 1);
    assert_eq!(lid_state.runner_walkers.len(), 1);

    let anneal = MonteCarloKernelState::new(
        WorkflowFamily::SimulatedAnnealing,
        SamplingSchedule::Annealing {
            temperature: 10.0,
            scale: 0.9,
            hold_steps: 5,
        },
    );
    let anneal_state = build_simulated_annealing_structure_state(&anneal, Some(&runner));
    assert!(anneal_state.holding_point.is_none());
    assert!(anneal_state.quench_branch.is_none());
    assert!(anneal_state.quench_walker.is_some());
}
