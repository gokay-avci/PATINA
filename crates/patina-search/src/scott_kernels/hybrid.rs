use crate::{
    crossover_candidate, geometry_is_reasonable, standardize_candidate_coordinates,
    ScottGaOperatorConfig, ScottGeometryModel, TinyRng,
};
use patina_types::{
    Candidate, HybridChildOrigin, HybridCrossoverAttemptRecord, HybridCrossoverScientificMode,
};

/// Input to the first Scott hybrid provenance kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridCrossoverAttemptInput<'a> {
    pub scientific_mode: HybridCrossoverScientificMode,
    pub ga_generation: Option<usize>,
    pub attempt_index: Option<usize>,
    pub child_origin_label: &'a str,
    pub selected_for_production: bool,
    pub parent_left_label: Option<&'a str>,
    pub parent_right_label: Option<&'a str>,
}

/// Fixed-parent Scott hybrid crossover config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedParentHybridCrossoverConfig {
    pub ga_generation: Option<usize>,
    pub attempt_count: usize,
    pub target_children: usize,
    pub seed: u64,
}

/// Accepted child emitted by the fixed-parent hybrid kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridAcceptedChild {
    pub candidate: Candidate,
    pub attempt: HybridCrossoverAttemptRecord,
}

/// Normalizes Scott GA-style origin labels into compact hybrid provenance.
pub fn hybrid_child_origin_from_label(label: &str) -> HybridChildOrigin {
    match label {
        "SEED" => HybridChildOrigin::Seed,
        "CROSSO" => HybridChildOrigin::Crosso,
        "MUTATE" => HybridChildOrigin::Mutate,
        "MUTCRS" => HybridChildOrigin::Mutcrs,
        "REPOPM" => HybridChildOrigin::RePopM,
        "REPOPR" => HybridChildOrigin::RePopR,
        _ => HybridChildOrigin::Unknown,
    }
}

/// Builds the first typed hybrid crossover-attempt provenance record.
pub fn build_hybrid_crossover_attempt_record(
    input: HybridCrossoverAttemptInput<'_>,
) -> HybridCrossoverAttemptRecord {
    HybridCrossoverAttemptRecord {
        scientific_mode: input.scientific_mode,
        ga_generation: input.ga_generation,
        attempt_index: input.attempt_index,
        child_origin: hybrid_child_origin_from_label(input.child_origin_label),
        selected_for_production: input.selected_for_production,
        parent_left_label: input.parent_left_label.map(ToOwned::to_owned),
        parent_right_label: input.parent_right_label.map(ToOwned::to_owned),
    }
}

/// Runs the first fixed-parent Scott hybrid crossover loop inside the Rust search core.
pub fn run_fixed_parent_hybrid_crossover(
    left_parent: &Candidate,
    right_parent: &Candidate,
    config: FixedParentHybridCrossoverConfig,
) -> Vec<HybridAcceptedChild> {
    if config.attempt_count == 0 || config.target_children == 0 {
        return Vec::new();
    }

    let mut rng = TinyRng::new(config.seed);
    let geometry_model = ScottGeometryModel::from_base(left_parent);
    let mut accepted: Vec<HybridAcceptedChild> = Vec::with_capacity(config.target_children);

    for attempt_index in 1..=config.attempt_count {
        let mut child = crossover_candidate(
            left_parent,
            right_parent,
            &mut rng,
            ScottGaOperatorConfig::default(),
            &geometry_model,
        );
        standardize_candidate_coordinates(&mut child);

        if !geometry_is_reasonable(&child, &geometry_model)
            || same_candidate_structure(&child, left_parent)
            || same_candidate_structure(&child, right_parent)
            || accepted
                .iter()
                .any(|existing| same_candidate_structure(&existing.candidate, &child))
        {
            continue;
        }

        child.label = format!(
            "hybrid_crossover_gen_{:04}_child_{:04}_attempt_{:04}",
            config.ga_generation.unwrap_or(0),
            accepted.len() + 1,
            attempt_index
        );
        let attempt = build_hybrid_crossover_attempt_record(HybridCrossoverAttemptInput {
            scientific_mode: HybridCrossoverScientificMode::EnforcedCrossover,
            ga_generation: config.ga_generation,
            attempt_index: Some(attempt_index),
            child_origin_label: "CROSSO",
            selected_for_production: true,
            parent_left_label: Some(&left_parent.label),
            parent_right_label: Some(&right_parent.label),
        });
        accepted.push(HybridAcceptedChild {
            candidate: child,
            attempt,
        });

        if accepted.len() >= config.target_children {
            break;
        }
    }

    accepted
}

fn same_candidate_structure(left: &Candidate, right: &Candidate) -> bool {
    left.species == right.species
        && left.fractional_coords == right.fractional_coords
        && left.lattice == right.lattice
        && left.periodic_axes == right.periodic_axes
}

#[cfg(test)]
mod tests {
    use super::{
        build_hybrid_crossover_attempt_record, hybrid_child_origin_from_label,
        run_fixed_parent_hybrid_crossover, FixedParentHybridCrossoverConfig,
        HybridCrossoverAttemptInput,
    };
    use patina_types::{Candidate, HybridChildOrigin, HybridCrossoverScientificMode};

    #[test]
    fn hybrid_child_origin_maps_scott_ga_labels() {
        assert_eq!(
            hybrid_child_origin_from_label("SEED"),
            HybridChildOrigin::Seed
        );
        assert_eq!(
            hybrid_child_origin_from_label("CROSSO"),
            HybridChildOrigin::Crosso
        );
        assert_eq!(
            hybrid_child_origin_from_label("MUTCRS"),
            HybridChildOrigin::Mutcrs
        );
        assert_eq!(
            hybrid_child_origin_from_label("unknown"),
            HybridChildOrigin::Unknown
        );
    }

    #[test]
    fn crossover_attempt_record_preserves_current_downstream_hybrid_limits() {
        let record = build_hybrid_crossover_attempt_record(HybridCrossoverAttemptInput {
            scientific_mode: HybridCrossoverScientificMode::GaDownstreamSelection,
            ga_generation: Some(4),
            attempt_index: None,
            child_origin_label: "CROSSO",
            selected_for_production: true,
            parent_left_label: None,
            parent_right_label: None,
        });

        assert_eq!(
            record.scientific_mode,
            HybridCrossoverScientificMode::GaDownstreamSelection
        );
        assert_eq!(record.ga_generation, Some(4));
        assert_eq!(record.attempt_index, None);
        assert_eq!(record.child_origin, HybridChildOrigin::Crosso);
        assert!(record.selected_for_production);
        assert_eq!(record.parent_left_label, None);
        assert_eq!(record.parent_right_label, None);
    }

    fn multi_atom_candidate(label: &str, coords: Vec<[f64; 3]>) -> Candidate {
        Candidate::cluster(
            label,
            vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
            coords,
        )
    }

    #[test]
    fn fixed_parent_hybrid_crossover_emits_typed_children() {
        let left = multi_atom_candidate(
            "left-parent",
            vec![
                [0.0, 0.0, 0.0],
                [1.8, 0.1, 0.0],
                [0.2, 1.1, 0.4],
                [1.4, 1.9, 0.6],
            ],
        );
        let right = multi_atom_candidate(
            "right-parent",
            vec![
                [0.4, 0.3, 0.2],
                [2.1, 0.9, 0.5],
                [0.7, 1.8, 1.1],
                [1.9, 2.4, 1.6],
            ],
        );

        let children = run_fixed_parent_hybrid_crossover(
            &left,
            &right,
            FixedParentHybridCrossoverConfig {
                ga_generation: Some(3),
                attempt_count: 24,
                target_children: 2,
                seed: 17,
            },
        );

        assert_eq!(children.len(), 2);
        for (index, child) in children.iter().enumerate() {
            assert_eq!(
                child.attempt.scientific_mode,
                HybridCrossoverScientificMode::EnforcedCrossover
            );
            assert_eq!(child.attempt.ga_generation, Some(3));
            assert!(child.attempt.attempt_index.is_some());
            assert_eq!(child.attempt.child_origin, HybridChildOrigin::Crosso);
            assert!(child.attempt.selected_for_production);
            assert_eq!(
                child.attempt.parent_left_label.as_deref(),
                Some("left-parent")
            );
            assert_eq!(
                child.attempt.parent_right_label.as_deref(),
                Some("right-parent")
            );
            assert!(child
                .candidate
                .label
                .contains(&format!("child_{:04}", index + 1)));
            assert_ne!(child.candidate.fractional_coords, left.fractional_coords);
            assert_ne!(child.candidate.fractional_coords, right.fractional_coords);
        }
    }

    #[test]
    fn fixed_parent_hybrid_crossover_respects_zero_target_or_attempts() {
        let left = multi_atom_candidate(
            "left-parent",
            vec![
                [0.0, 0.0, 0.0],
                [1.8, 0.1, 0.0],
                [0.2, 1.1, 0.4],
                [1.4, 1.9, 0.6],
            ],
        );
        let right = multi_atom_candidate(
            "right-parent",
            vec![
                [0.4, 0.3, 0.2],
                [2.1, 0.9, 0.5],
                [0.7, 1.8, 1.1],
                [1.9, 2.4, 1.6],
            ],
        );

        assert!(run_fixed_parent_hybrid_crossover(
            &left,
            &right,
            FixedParentHybridCrossoverConfig {
                ga_generation: None,
                attempt_count: 0,
                target_children: 2,
                seed: 5,
            },
        )
        .is_empty());
        assert!(run_fixed_parent_hybrid_crossover(
            &left,
            &right,
            FixedParentHybridCrossoverConfig {
                ga_generation: None,
                attempt_count: 8,
                target_children: 0,
                seed: 5,
            },
        )
        .is_empty());
    }
}
