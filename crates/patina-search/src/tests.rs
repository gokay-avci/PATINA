use super::*;
use crate::scott_ga_parity::DuplicateCandidateSnapshot;

fn base_candidate() -> Candidate {
    Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![
            [0.10, 0.10, 0.10],
            [0.60, 0.10, 0.10],
            [0.10, 0.60, 0.10],
            [0.60, 0.60, 0.10],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "base".into(),
    }
}

fn config() -> SearchConfig {
    SearchConfig {
        temperature: 10.0,
        step_size: 0.1,
        population_size: 4,
        max_steps: 3,
        seed: Some(7),
    }
}

fn result(candidate: &Candidate, energy: f64) -> EvalResult {
    EvalResult {
        energy,
        forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
        relaxed_candidate: candidate.clone(),
        converged: true,
        wall_time: std::time::Duration::from_secs(1),
    }
}

#[test]
fn basin_hopping_initializes_requested_number_of_walkers() {
    let mut bh = BasinHopping::new(base_candidate(), 7);
    let candidates = bh.initialize(&config());
    assert_eq!(candidates.len(), 4);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.label.starts_with("bh_init_")));
}

#[test]
fn basin_hopping_updates_population_and_converges() {
    let mut bh = BasinHopping::new(base_candidate(), 7);
    let cfg = config();
    let initial = bh.initialize(&cfg);
    let evaluated: Vec<_> = initial
        .iter()
        .enumerate()
        .map(|(idx, candidate)| (candidate.clone(), result(candidate, -(idx as f64) - 1.0)))
        .collect();
    let mut population = Population::default();
    bh.update(&mut population, &evaluated)
        .expect("initial update");

    assert_eq!(population.members.len(), 4);
    assert_eq!(population.generation, 1);

    for _ in 0..2 {
        let next = bh.next_candidates(&population).expect("next candidates");
        let evaluated_next: Vec<_> = next
            .iter()
            .enumerate()
            .map(|(idx, candidate)| (candidate.clone(), result(candidate, -10.0 - idx as f64)))
            .collect();
        bh.update(&mut population, &evaluated_next)
            .expect("generation update");
    }

    assert!(bh.converged(&population).expect("converged"));
}

#[test]
fn basin_hopping_accepts_against_current_walker_not_global_best() {
    let mut bh = BasinHopping::with_acceptance(base_candidate(), 7, MonteCarloAcceptance::Quench);
    let cfg = SearchConfig {
        temperature: 0.0,
        step_size: 0.1,
        population_size: 2,
        max_steps: 3,
        seed: Some(7),
    };
    let _ = bh.initialize(&cfg);
    let mut population = Population {
        members: vec![
            (result(&base_candidate(), -10.0), base_candidate()),
            (
                result(
                    &Candidate {
                        label: "walker2".into(),
                        ..base_candidate()
                    },
                    -5.0,
                ),
                Candidate {
                    label: "walker2".into(),
                    ..base_candidate()
                },
            ),
        ],
        generation: 0,
    };
    let evaluated = vec![
        (
            Candidate {
                label: "cand1".into(),
                ..base_candidate()
            },
            result(&base_candidate(), -9.0),
        ),
        (
            Candidate {
                label: "cand2".into(),
                ..base_candidate()
            },
            result(&base_candidate(), -6.0),
        ),
    ];

    let update = bh.update(&mut population, &evaluated).expect("update");

    assert_eq!(update.accepted, 1);
    assert_eq!(population.members.len(), 2);
    assert_eq!(population.members[0].0.energy, -10.0);
    assert_eq!(population.members[1].0.energy, -6.0);
}

#[test]
fn metropolis_acceptance_matches_native_downhill_and_threshold_rules() {
    let threshold_reject = metropolis_acceptance(
        -5.5,
        MonteCarloAcceptance::EnergyThreshold { threshold: -6.0 },
        0.5,
    );
    assert!(!threshold_reject.accepted);

    let threshold_accept = accept_energy_transition(
        Some(-8.0),
        -6.5,
        MonteCarloAcceptance::EnergyThreshold { threshold: -6.0 },
        0.5,
    );
    assert!(threshold_accept.accepted);

    let threshold_reject_from_transition = accept_energy_transition(
        Some(-8.0),
        -5.5,
        MonteCarloAcceptance::EnergyThreshold { threshold: -6.0 },
        0.5,
    );
    assert!(!threshold_reject_from_transition.accepted);

    let quench_uphill = metropolis_acceptance(-1.0, MonteCarloAcceptance::Quench, 0.2);
    assert!(!quench_uphill.accepted);

    let quench_downhill = metropolis_acceptance(1.0, MonteCarloAcceptance::Quench, 0.9);
    assert!(quench_downhill.accepted);
}

#[test]
fn energy_lid_only_accepts_below_threshold() {
    let mut mc = EnergyLid::new(base_candidate(), 13, -6.0);
    let cfg = SearchConfig {
        temperature: 10.0,
        step_size: 0.2,
        population_size: 1,
        max_steps: 3,
        seed: Some(13),
    };
    let _ = mc.initialize(&cfg);
    let mut pop = Population {
        members: vec![(result(&base_candidate(), -5.0), base_candidate())],
        generation: 0,
    };

    let rejected = mc
        .update(
            &mut pop,
            &[(
                Candidate {
                    label: "lid_fail".into(),
                    ..base_candidate()
                },
                result(&base_candidate(), -5.5),
            )],
        )
        .expect("rejected update");
    assert_eq!(rejected.accepted, 0);

    let accepted = mc
        .update(
            &mut pop,
            &[(
                Candidate {
                    label: "lid_ok".into(),
                    ..base_candidate()
                },
                result(&base_candidate(), -6.5),
            )],
        )
        .expect("accepted update");
    assert_eq!(accepted.accepted, 1);
    assert_eq!(pop.best().expect("best").0.energy, -6.5);
}

#[test]
fn simulated_annealing_cools_after_updates() {
    let mut sa = SimulatedAnnealing::new(base_candidate(), 17, 10.0, 0.5);
    let cfg = SearchConfig {
        temperature: 10.0,
        step_size: 0.2,
        population_size: 1,
        max_steps: 3,
        seed: Some(17),
    };
    let _ = sa.initialize(&cfg);
    let mut pop = Population {
        members: vec![(result(&base_candidate(), -5.0), base_candidate())],
        generation: 0,
    };

    let _ = sa
        .update(
            &mut pop,
            &[(
                Candidate {
                    label: "sa".into(),
                    ..base_candidate()
                },
                result(&base_candidate(), -5.5),
            )],
        )
        .expect("sa update");

    assert!((sa.current_temperature() - 5.0).abs() < 1.0e-12);
}

#[test]
fn simulated_annealing_holds_temperature_for_requested_window() {
    let mut sa = SimulatedAnnealing::new(base_candidate(), 18, 10.0, 0.5).with_hold_steps(2);
    let cfg = SearchConfig {
        temperature: 10.0,
        step_size: 0.2,
        population_size: 1,
        max_steps: 4,
        seed: Some(18),
    };
    let _ = sa.initialize(&cfg);
    let mut pop = Population {
        members: vec![(result(&base_candidate(), -5.0), base_candidate())],
        generation: 0,
    };

    let sample = [(
        Candidate {
            label: "sa_hold".into(),
            ..base_candidate()
        },
        result(&base_candidate(), -5.5),
    )];

    let _ = sa.update(&mut pop, &sample).expect("sa hold update");
    assert!((sa.current_temperature() - 10.0).abs() < 1.0e-12);
    let _ = sa.update(&mut pop, &sample).expect("sa cool update");
    assert!((sa.current_temperature() - 5.0).abs() < 1.0e-12);
    let trace = sa.trace();
    assert_eq!(trace.len(), 2);
    assert!((trace[0].temperature - 10.0).abs() < 1.0e-12);
    assert!((trace[1].temperature - 10.0).abs() < 1.0e-12);
}

#[test]
fn energy_lid_can_raise_threshold_after_hold_window() {
    let mut mc = EnergyLid::new(base_candidate(), 19, -6.0).with_ladder(1.0, 2);
    let cfg = SearchConfig {
        temperature: 10.0,
        step_size: 0.2,
        population_size: 1,
        max_steps: 4,
        seed: Some(19),
    };
    let _ = mc.initialize(&cfg);
    let mut pop = Population {
        members: vec![(result(&base_candidate(), -5.0), base_candidate())],
        generation: 0,
    };

    let rejected = [(
        Candidate {
            label: "lid_hold".into(),
            ..base_candidate()
        },
        result(&base_candidate(), -5.5),
    )];
    let _ = mc.update(&mut pop, &rejected).expect("lid hold update");
    assert_eq!(mc.current_threshold(), -6.0);
    let _ = mc.update(&mut pop, &rejected).expect("lid raise update");
    assert_eq!(mc.current_threshold(), -5.0);

    let accepted = mc
        .update(
            &mut pop,
            &[(
                Candidate {
                    label: "lid_after_raise".into(),
                    ..base_candidate()
                },
                result(&base_candidate(), -5.2),
            )],
        )
        .expect("lid accepted update");
    assert_eq!(accepted.accepted, 1);
    let last = mc.trace().last().expect("mc trace");
    assert_eq!(last.threshold, Some(-5.0));
    assert!(last.accepted);
}

#[test]
fn bh_move_class_stays_monte_carlo_before_rejection_threshold() {
    let move_class = choose_bh_move_class_from_draw(false, BhMoveClassPolicy::default(), 0.5);
    assert_eq!(move_class, BhMoveClass::MonteCarlo);
}

#[test]
fn bh_move_class_can_escalate_after_rejection_threshold() {
    let move_class = choose_bh_move_class_from_draw(
        true,
        BhMoveClassPolicy {
            dynamic_step_threshold: 20,
            moveclass_threshold: 50,
            max_dynamic_step_multiplier: 3.5,
            prob_swap_cations: 0.0,
            enable_after_rejections: 1,
            prob_swap_atoms: 1.0,
            prob_mutate_cluster: 0.0,
            prob_twist_cluster: 0.0,
            prob_translate_surface: 0.0,
            prob_rotate_surface: 0.0,
        },
        0.5,
    );
    assert_eq!(move_class, BhMoveClass::SwapAtoms);
}

#[test]
fn bh_step_control_escalates_dynamic_step_size_before_random_moveclass() {
    let state = advance_bh_step_control(
        BhStepControlState {
            rejection_counter: 3,
            random_moveclass: false,
            current_step_size: 1.0,
        },
        1.0,
        BhMoveClassPolicy {
            dynamic_step_threshold: 2,
            moveclass_threshold: 50,
            max_dynamic_step_multiplier: 3.5,
            ..BhMoveClassPolicy::default()
        },
    );
    assert_eq!(state.rejection_counter, 0);
    assert_eq!(state.current_step_size, 2.0);
    assert!(!state.random_moveclass);
}

#[test]
fn bh_swap_atoms_preserves_species_inventory() {
    let mut candidate = base_candidate();
    let original_species = candidate.species.clone();
    apply_bh_move_class(
        &mut candidate,
        BhMoveClass::SwapAtoms,
        0.1,
        &mut TinyRng::new(3),
    );
    assert_eq!(candidate.species, original_species);
    assert_eq!(candidate.len(), original_species.len());
}

#[test]
fn bh_swap_cations_only_reorders_leading_cation_block() {
    let mut candidate = Candidate {
        species: vec!["Mg".into(), "Mg".into(), "O".into(), "O".into()],
        fractional_coords: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "cations".into(),
    };
    apply_bh_move_class(
        &mut candidate,
        BhMoveClass::SwapCations,
        0.1,
        &mut TinyRng::new(3),
    );
    assert_eq!(candidate.fractional_coords[2], [2.0, 0.0, 0.0]);
    assert_eq!(candidate.fractional_coords[3], [3.0, 0.0, 0.0]);
}

#[test]
fn basin_hopping_emits_trace_rows_with_move_class_and_acceptance() {
    let mut bh = BasinHopping::with_acceptance(base_candidate(), 7, MonteCarloAcceptance::Quench)
        .with_move_policy(BhMoveClassPolicy {
            dynamic_step_threshold: 20,
            moveclass_threshold: 0,
            max_dynamic_step_multiplier: 3.5,
            prob_swap_cations: 0.0,
            enable_after_rejections: 0,
            prob_swap_atoms: 1.0,
            prob_mutate_cluster: 0.0,
            prob_twist_cluster: 0.0,
            prob_translate_surface: 0.0,
            prob_rotate_surface: 0.0,
        });
    let cfg = SearchConfig {
        temperature: 0.0,
        step_size: 0.1,
        population_size: 1,
        max_steps: 3,
        seed: Some(7),
    };
    let initial = bh.initialize(&cfg);
    let evaluated: Vec<_> = initial
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -1.0)))
        .collect();
    let mut population = Population::default();
    bh.update(&mut population, &evaluated)
        .expect("initial update");

    let rejected = bh
        .next_candidates(&population)
        .expect("rejected candidates");
    let rejected_eval: Vec<_> = rejected
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, 5.0)))
        .collect();
    bh.update(&mut population, &rejected_eval)
        .expect("rejected update");

    let next = bh.next_candidates(&population).expect("next candidates");
    let evaluated_next: Vec<_> = next
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -2.0)))
        .collect();
    bh.update(&mut population, &evaluated_next)
        .expect("accepted update");

    let trace = bh.trace();
    assert!(!trace.is_empty());
    let last = trace.last().expect("trace row");
    assert_eq!(last.step, 2);
    assert_eq!(last.walker_id, 0);
    assert_eq!(last.move_class, BhMoveClass::SwapAtoms);
    assert!(last.accepted);
}

#[test]
fn basin_hopping_increases_dynamic_step_size_and_resets_after_acceptance() {
    let mut bh = BasinHopping::with_acceptance(base_candidate(), 9, MonteCarloAcceptance::Quench)
        .with_move_policy(BhMoveClassPolicy {
            dynamic_step_threshold: 0,
            moveclass_threshold: 50,
            max_dynamic_step_multiplier: 3.5,
            prob_swap_cations: 0.0,
            enable_after_rejections: 20,
            prob_swap_atoms: 0.0,
            prob_mutate_cluster: 0.0,
            prob_twist_cluster: 0.0,
            prob_translate_surface: 0.0,
            prob_rotate_surface: 0.0,
        });
    let cfg = SearchConfig {
        temperature: 0.0,
        step_size: 0.1,
        population_size: 1,
        max_steps: 4,
        seed: Some(9),
    };
    let initial = bh.initialize(&cfg);
    let evaluated: Vec<_> = initial
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -1.0)))
        .collect();
    let mut population = Population::default();
    bh.update(&mut population, &evaluated)
        .expect("initial update");

    let rejected = bh
        .next_candidates(&population)
        .expect("rejected candidates");
    let rejected_eval: Vec<_> = rejected
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, 5.0)))
        .collect();
    bh.update(&mut population, &rejected_eval)
        .expect("rejected update");

    let grown = bh.next_candidates(&population).expect("grown candidates");
    let grown_eval: Vec<_> = grown
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -2.0)))
        .collect();
    bh.update(&mut population, &grown_eval)
        .expect("grown update");

    let trace = bh.trace();
    let last = trace.last().expect("trace row");
    assert!((last.step_size - 0.2).abs() < 1.0e-12);

    let reset = bh.next_candidates(&population).expect("reset candidates");
    let reset_eval: Vec<_> = reset
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -3.0)))
        .collect();
    bh.update(&mut population, &reset_eval)
        .expect("reset update");
    let last = bh.trace().last().expect("reset trace row");
    assert!((last.step_size - 0.1).abs() < 1.0e-12);
}

#[test]
fn basin_hopping_fixed_method_preserves_metropolis_temperature() {
    let mut bh = BasinHopping::new(base_candidate(), 13).with_method(BhMethod::Fixed);
    let cfg = SearchConfig {
        temperature: 8.0,
        step_size: 0.1,
        population_size: 1,
        max_steps: 3,
        seed: Some(13),
    };
    let initial = bh.initialize(&cfg);
    let evaluated: Vec<_> = initial
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -1.0)))
        .collect();
    let mut population = Population::default();
    bh.update(&mut population, &evaluated)
        .expect("initial update");
    let next = bh.next_candidates(&population).expect("next candidates");
    let evaluated_next: Vec<_> = next
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -2.0)))
        .collect();
    bh.update(&mut population, &evaluated_next)
        .expect("next update");
    assert_eq!(
        bh.acceptance,
        MonteCarloAcceptance::Metropolis { temperature: 8.0 }
    );
    let trace = bh.trace();
    let last = trace.last().expect("trace row");
    assert!((last.temperature - 8.0).abs() < 1.0e-12);
}

#[test]
fn basin_hopping_oscillate_method_toggles_between_metropolis_and_quench() {
    let mut bh = BasinHopping::new(base_candidate(), 15).with_method(BhMethod::Oscillate {
        high_temperature_steps: 1,
        low_temperature_steps: 2,
    });
    let cfg = SearchConfig {
        temperature: 6.0,
        step_size: 0.1,
        population_size: 1,
        max_steps: 4,
        seed: Some(15),
    };
    let initial = bh.initialize(&cfg);
    let evaluated: Vec<_> = initial
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -1.0)))
        .collect();
    let mut population = Population::default();
    bh.update(&mut population, &evaluated)
        .expect("initial update");
    assert_eq!(bh.acceptance, MonteCarloAcceptance::Quench);

    let step1 = bh.next_candidates(&population).expect("step1 candidates");
    let eval1: Vec<_> = step1
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -2.0)))
        .collect();
    bh.update(&mut population, &eval1).expect("step1 update");
    assert_eq!(bh.acceptance, MonteCarloAcceptance::Quench);

    let step2 = bh.next_candidates(&population).expect("step2 candidates");
    let eval2: Vec<_> = step2
        .iter()
        .map(|candidate| (candidate.clone(), result(candidate, -3.0)))
        .collect();
    bh.update(&mut population, &eval2).expect("step2 update");
    assert_eq!(
        bh.acceptance,
        MonteCarloAcceptance::Metropolis { temperature: 6.0 }
    );
}

#[test]
fn genetic_algorithm_initializes_and_truncates_population() {
    let mut ga = GeneticAlgorithm::new(base_candidate(), 9);
    let cfg = config();
    let initial = ga.initialize(&cfg);
    assert_eq!(initial.len(), 4);

    let evaluated: Vec<_> = initial
        .iter()
        .enumerate()
        .map(|(idx, candidate)| (candidate.clone(), result(candidate, -20.0 + idx as f64)))
        .collect();

    let mut population = Population::default();
    ga.update(&mut population, &evaluated)
        .expect("initial ga update");
    assert_eq!(population.members.len(), 4);

    let children = ga.next_candidates(&population).expect("ga children");
    let evaluated_children: Vec<_> = children
        .iter()
        .enumerate()
        .map(|(idx, candidate)| (candidate.clone(), result(candidate, -30.0 + idx as f64)))
        .collect();
    ga.update(&mut population, &evaluated_children)
        .expect("children update");

    assert_eq!(population.members.len(), 4);
    assert_eq!(population.generation, 2);
}

#[test]
fn scott_parity_selection_prefers_lower_energy_valid_members() {
    let mut ga = ScottParityGeneticAlgorithm::new(base_candidate(), 11);
    ga.initialize_population(&config());

    let members = vec![
        ScottGaMember {
            source_candidate: base_candidate(),
            result: result(&base_candidate(), -5.0),
            origin: ScottGaOrigin::Seed,
            occurrences: 1,
            lineage: WorkflowLineage::seed("seed-a"),
            topology: ScottGaTopologyIdentity::default(),
        },
        ScottGaMember {
            source_candidate: base_candidate(),
            result: EvalResult {
                converged: false,
                ..result(&base_candidate(), -100.0)
            },
            origin: ScottGaOrigin::Seed,
            occurrences: 1,
            lineage: WorkflowLineage::seed("seed-b"),
            topology: ScottGaTopologyIdentity::default(),
        },
        ScottGaMember {
            source_candidate: base_candidate(),
            result: result(&base_candidate(), -10.0),
            origin: ScottGaOrigin::Seed,
            occurrences: 1,
            lineage: WorkflowLineage::seed("seed-c"),
            topology: ScottGaTopologyIdentity::default(),
        },
    ];

    let selected = ga.selection_tournament(&members);
    assert!(!selected.is_empty());
    assert!(selected.iter().all(|&idx| idx != 1));
}

#[test]
fn scott_parity_serial_seed_generation_matches_bulk_initializer() {
    let mut base = base_candidate();
    base.fractional_coords = vec![
        [1.1, -0.3, 0.5],
        [1.6, -0.3, 0.5],
        [1.1, 0.2, 0.5],
        [1.6, 0.2, 0.5],
    ];

    let mut bulk = ScottParityGeneticAlgorithm::new(base.clone(), 11);
    let initialized = bulk.initialize_population(&config());

    let mut serial = ScottParityGeneticAlgorithm::new(base, 11);
    serial.configure_for_run(&config());
    let generated = serial
        .generate_initial_candidate(0)
        .expect("serial seed candidate");

    assert_eq!(generated, initialized[0]);
}

#[test]
fn scott_parity_merges_exact_relaxed_duplicates_and_tracks_occurrences() {
    let mut ga = ScottParityGeneticAlgorithm::with_configs(
        base_candidate(),
        13,
        ScottGaOperatorConfig::default(),
        ScottDuplicateClassifier {
            enable_pmoi: true,
            ..ScottDuplicateClassifier::default()
        },
    );
    let cfg = config();
    ga.initialize_population(&cfg);

    let parent = ScottGaMember {
        source_candidate: base_candidate(),
        result: result(&base_candidate(), -10.0),
        origin: ScottGaOrigin::Seed,
        occurrences: 1,
        lineage: WorkflowLineage::seed("seed-parent"),
        topology: ScottGaTopologyIdentity::default(),
    };

    let child_a = base_candidate();
    let child_b = child_a.clone();
    let next = ga
        .integrate_generation(
            1,
            &[parent],
            vec![
                (
                    child_a.clone(),
                    result(&child_a, -11.0),
                    ScottGaOrigin::Crosso,
                    WorkflowLineage {
                        origin_label: child_a.label.clone(),
                        generation: Some(1),
                        step: None,
                        parent_labels: vec!["seed-parent".to_string()],
                        attempt: 1,
                    },
                ),
                (
                    child_b.clone(),
                    result(&child_b, -12.0),
                    ScottGaOrigin::Crosso,
                    WorkflowLineage {
                        origin_label: child_b.label.clone(),
                        generation: Some(1),
                        step: None,
                        parent_labels: vec!["seed-parent".to_string()],
                        attempt: 1,
                    },
                ),
            ],
        )
        .expect("integrate generation");

    let duplicate_member = next
        .iter()
        .find(|member| member.result.energy == -11.0 || member.result.energy == -12.0)
        .expect("duplicate survivor present");
    assert_eq!(duplicate_member.occurrences, 3);
    assert!(!ga.trace().is_empty());
}

#[test]
fn scott_parity_repopulation_lineage_tracks_active_generation() {
    let mut ga = ScottParityGeneticAlgorithm::with_configs(
        base_candidate(),
        19,
        ScottGaOperatorConfig {
            max_repop_attempts: 16,
            ..ScottGaOperatorConfig::default()
        },
        ScottDuplicateClassifier::default(),
    );
    let cfg = config();
    ga.configure_for_run(&cfg);

    let parent = ScottGaMember {
        source_candidate: base_candidate(),
        result: result(&base_candidate(), -10.0),
        origin: ScottGaOrigin::Seed,
        occurrences: 1,
        lineage: WorkflowLineage::seed("seed-parent").with_generation(2),
        topology: ScottGaTopologyIdentity::default(),
    };

    let next = ga
        .integrate_generation(3, &[parent], Vec::new())
        .expect("integrate generation with repopulation");
    let repopulated: Vec<_> = next.iter().filter(|member| !member.is_valid()).collect();

    assert_eq!(repopulated.len(), cfg.population_size - 1);
    assert!(repopulated
        .iter()
        .all(|member| member.lineage.generation == Some(3)));
}

#[test]
fn crossover_preserves_species_and_site_count() {
    let left = Candidate {
        label: "left".into(),
        ..base_candidate()
    };
    let mut right = left.clone();
    right.label = "right".into();
    right.fractional_coords = vec![
        [1.2, 0.0, 0.0],
        [0.8, 0.2, 0.1],
        [-0.4, 1.1, 0.3],
        [0.2, -0.7, 0.4],
    ];
    let mut rng = TinyRng::new(21);
    let child = crossover_candidate(
        &left,
        &right,
        &mut rng,
        ScottGaOperatorConfig::default(),
        &ScottGeometryModel::from_base(&left),
    );

    assert_eq!(child.len(), left.len());
    let mut child_species = child.species.clone();
    let mut left_species = left.species.clone();
    child_species.sort();
    left_species.sort();
    assert_eq!(child_species, left_species);
}

#[test]
fn intrinsic_cluster_dimensions_follow_native_bbox_rule() {
    let line = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "line".into(),
    };
    let sheet = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 3.0, 0.0],
            [2.0, 3.0, 0.0],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "sheet".into(),
    };
    let bulkish = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.5],
            [0.0, 3.0, 0.2],
            [1.0, 1.0, 4.0],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "bulkish".into(),
    };

    assert_eq!(cluster_intrinsic_dims(&line, 0.0), 1);
    assert_eq!(cluster_intrinsic_dims(&sheet, 0.0), 2);
    assert_eq!(cluster_intrinsic_dims(&bulkish, 0.0), 3);
}

#[test]
fn mutation_preserves_finite_coordinates() {
    let mut candidate = base_candidate();
    let mut rng = TinyRng::new(31);
    for _ in 0..16 {
        mutate_candidate(
            &mut candidate,
            2,
            0.25,
            &mut rng,
            ScottGaOperatorConfig::default(),
        );
    }

    for row in candidate.fractional_coords {
        for value in row {
            assert!(value.is_finite());
        }
    }
}

#[test]
#[should_panic(
    expected = "Scott parity search operators support only 0D clusters or full 3D periodic cells"
)]
fn partial_periodic_mutation_is_rejected_in_parity_search() {
    let mut candidate = Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.95, 0.85, -0.20], [0.10, 0.15, 1.35]],
        lattice: Some([[5.0, 0.0, 0.0], [0.0, 7.0, 0.0], [0.0, 0.0, 20.0]]),
        periodic_axes: [true, true, false],
        label: "slab".into(),
    };
    let mut rng = TinyRng::new(41);

    mutate_candidate(
        &mut candidate,
        2,
        0.45,
        &mut rng,
        ScottGaOperatorConfig {
            mutate_expand_ratio: 0.0,
            mutate_contract_ratio: 0.0,
            mutate_swap_ratio: 0.0,
            ..ScottGaOperatorConfig::default()
        },
    );
}

#[test]
#[should_panic(
    expected = "Scott parity search operators support only 0D clusters or full 3D periodic cells"
)]
fn partial_periodic_random_foreign_structure_is_rejected_in_parity_search() {
    let base = Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.15, 0.25, 2.5], [0.45, 0.55, 4.5]],
        lattice: Some([[4.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 25.0]]),
        periodic_axes: [true, true, false],
        label: "slab".into(),
    };
    let mut rng = TinyRng::new(57);
    let geometry_model = ScottGeometryModel::from_base(&base);
    let _ = random_foreign_structure(&base, &geometry_model, &mut rng);
}

#[test]
fn periodic_standardization_and_rotation_are_noops_for_partial_periodicity() {
    let original = Candidate {
        species: vec!["Mg".into(), "O".into(), "O".into()],
        fractional_coords: vec![[0.2, 0.3, 1.5], [0.8, 0.1, 2.1], [0.4, 0.9, -0.4]],
        lattice: Some([[4.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 25.0]]),
        periodic_axes: [true, true, false],
        label: "surface".into(),
    };
    let mut standardized = original.clone();
    let mut rotated = original.clone();

    standardize_candidate_coordinates(&mut standardized);
    rotate_candidate_angles(&mut rotated, 0.73, 1.11);

    assert_eq!(standardized.fractional_coords, original.fractional_coords);
    assert_eq!(rotated.fractional_coords, original.fractional_coords);
}

#[test]
#[should_panic(
    expected = "Scott parity search operators support only 0D clusters or full 3D periodic cells"
)]
fn ga_initialize_rejects_partial_periodic_search_semantics() {
    let base = Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.2, 0.3, 1.5], [0.8, 0.1, 2.1]],
        lattice: Some([[4.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 25.0]]),
        periodic_axes: [true, true, false],
        label: "surface".into(),
    };
    let mut ga = ScottParityGeneticAlgorithm::new(base, 9);
    let _ = ga.initialize_population(&config());
}

#[test]
#[should_panic(
    expected = "Scott parity search operators support only 0D clusters or full 3D periodic cells"
)]
fn basin_hopping_new_rejects_partial_periodic_search_semantics() {
    let base = Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.2, 0.3, 1.5], [0.8, 0.1, 2.1]],
        lattice: Some([[4.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 25.0]]),
        periodic_axes: [true, true, false],
        label: "surface".into(),
    };
    let _ = BasinHopping::new(base, 7);
}

#[test]
fn pmoi_uses_species_masses_not_geometry_only() {
    let mg_o = Candidate {
        species: vec!["Mg".into(), "O".into(), "O".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "mg_o".into(),
    };
    let o_mg = Candidate {
        species: vec!["O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "o_mg".into(),
    };

    assert_ne!(normalized_pmoi(&mg_o), normalized_pmoi(&o_mg));
}

#[test]
fn standardization_is_rotation_invariant_for_pmoi() {
    let mut a = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![
            [0.0, 0.0, 0.0],
            [1.8, 0.1, 0.0],
            [0.2, 1.1, 0.4],
            [1.4, 1.9, 0.6],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "a".into(),
    };
    let mut b = a.clone();
    rotate_candidate_angles(&mut b, 0.73, 1.11);

    standardize_candidate_coordinates(&mut a);
    standardize_candidate_coordinates(&mut b);

    let pmoi_a = normalized_pmoi(&a);
    let pmoi_b = normalized_pmoi(&b);
    for (left, right) in pmoi_a.into_iter().zip(pmoi_b) {
        assert!((left - right).abs() < 1.0e-9);
    }
}

#[test]
fn pmoi_duplicate_filter_is_binary_thresholded() {
    let left = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![
            [0.0, 0.0, 0.0],
            [1.8, 0.1, 0.0],
            [0.2, 1.1, 0.4],
            [1.4, 1.9, 0.6],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "left".into(),
    };
    let mut right = left.clone();
    right.label = "right".into();
    rotate_candidate_angles(&mut right, 0.33, 0.71);

    assert!(compare_pmoi_scott_intent(&left, &right, 1.0e-9));
    assert!(compare_pmoi_scott_intent(&left, &right, 1.0e-3));
}

#[test]
fn duplicate_classifier_prefers_pmoi_before_energy_tolerance() {
    let left = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
        fractional_coords: vec![
            [0.0, 0.0, 0.0],
            [1.8, 0.1, 0.0],
            [0.2, 1.1, 0.4],
            [1.4, 1.9, 0.6],
        ],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "left".into(),
    };
    let mut right = left.clone();
    right.label = "right".into();
    rotate_candidate_angles(&mut right, 0.55, 1.02);

    let classifier = ScottDuplicateClassifier {
        enable_pmoi: true,
        energy_tolerance: 100.0,
        ..ScottDuplicateClassifier::default()
    };

    let reason = classify_duplicate_candidates(
        DuplicateCandidateSnapshot {
            candidate: &left,
            energy: -10.0,
            converged: true,
        },
        DuplicateCandidateSnapshot {
            candidate: &right,
            energy: 25.0,
            converged: true,
        },
        &classifier,
    );

    assert_eq!(reason, Some(ScottDuplicateReason::Pmoi));
}

#[test]
fn duplicate_classifier_orders_hashkey_before_pmoi_before_energy_tolerance() {
    let left = base_candidate();
    let mut pmoi_match = left.clone();
    pmoi_match.label = "pmoi".into();
    rotate_candidate_angles(&mut pmoi_match, 0.55, 1.02);

    let energy_only = Candidate::cluster(
        "energy_only",
        vec![
            "Mg".to_string(),
            "O".to_string(),
            "Mg".to_string(),
            "O".to_string(),
        ],
        vec![
            [0.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 5.0],
        ],
    );
    let classifier = ScottDuplicateClassifier::pmoi_energy_fallback_with_pmoi_tolerance(0.1);

    assert!(compare_pmoi_scott_intent(
        &left,
        &pmoi_match,
        classifier.pmoi_tolerance,
    ));
    assert!(!compare_pmoi_scott_intent(
        &left,
        &energy_only,
        classifier.pmoi_tolerance,
    ));

    let hashkey_reason = classify_duplicate_candidates_after_hashkey_probe(
        true,
        DuplicateCandidateSnapshot {
            candidate: &left,
            energy: -10.0,
            converged: true,
        },
        DuplicateCandidateSnapshot {
            candidate: &pmoi_match,
            energy: -10.0,
            converged: true,
        },
        &classifier,
    );
    let pmoi_reason = classify_duplicate_candidates_after_hashkey_probe(
        false,
        DuplicateCandidateSnapshot {
            candidate: &left,
            energy: -10.0,
            converged: true,
        },
        DuplicateCandidateSnapshot {
            candidate: &pmoi_match,
            energy: 25.0,
            converged: true,
        },
        &classifier,
    );
    let energy_reason = classify_duplicate_candidates_after_hashkey_probe(
        false,
        DuplicateCandidateSnapshot {
            candidate: &left,
            energy: -10.0,
            converged: true,
        },
        DuplicateCandidateSnapshot {
            candidate: &energy_only,
            energy: -10.005,
            converged: true,
        },
        &classifier,
    );

    assert_eq!(hashkey_reason, Some(ScottDuplicateReason::Hashkey));
    assert_eq!(pmoi_reason, Some(ScottDuplicateReason::Pmoi));
    assert_eq!(energy_reason, Some(ScottDuplicateReason::EnergyTol));
}

#[test]
fn pmoi_duplicate_filter_is_disabled_for_periodic_candidates() {
    let left = Candidate {
        species: vec!["Mg".into(), "O".into(), "Mg".into()],
        fractional_coords: vec![[0.1, 0.2, 0.3], [0.2, 0.2, 0.3], [0.3, 0.2, 0.3]],
        lattice: Some([[8.0, 0.0, 0.0], [0.0, 9.0, 0.0], [0.0, 0.0, 10.0]]),
        periodic_axes: [true, true, true],
        label: "left_periodic".into(),
    };
    let mut right = left.clone();
    right.label = "right_periodic".into();

    assert!(!compare_pmoi_scott_intent(&left, &right, 1.0));
}

#[test]
fn duplicate_classifier_rejects_cross_dimensional_matches() {
    let cluster = Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [1.5, 0.0, 0.0]],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "cluster".into(),
    };
    let bulk = Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
        lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]]),
        periodic_axes: [true, true, true],
        label: "bulk".into(),
    };
    let classifier = ScottDuplicateClassifier {
        enable_pmoi: true,
        energy_tolerance: 100.0,
        ..ScottDuplicateClassifier::default()
    };

    let reason = classify_duplicate_candidates(
        DuplicateCandidateSnapshot {
            candidate: &cluster,
            energy: -10.0,
            converged: true,
        },
        DuplicateCandidateSnapshot {
            candidate: &bulk,
            energy: -10.0,
            converged: true,
        },
        &classifier,
    );

    assert_eq!(reason, None);
}
