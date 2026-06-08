use patina_perturber::{
    eliminate_by_threshold, pairwise_distances, ClusterPerturbationEngine, ClusterStructure,
    DefaultClusterPerturbationEngine, FingerprintVector, OverlapMatrixFingerprintEngine,
    PerturbationConfig, StructureFingerprintEngine,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LegacyParitySnapshot {
    source: ClusterStructure,
    variants: Vec<ClusterStructure>,
    fingerprints_without_p: Vec<FingerprintVector>,
    fingerprints_with_p: Vec<FingerprintVector>,
    pairwise_distances_with_p: Vec<Vec<f64>>,
    survivors_without_p: Vec<ClusterStructure>,
    perturbation_config: PerturbationConfig,
    perturbed_variants: Vec<ClusterStructure>,
}

fn snapshot() -> LegacyParitySnapshot {
    serde_json::from_str(include_str!("fixtures/legacy_parity_snapshot.json"))
        .expect("legacy parity snapshot fixture must deserialize")
}

fn assert_vectors_close(left: &[f64], right: &[f64]) {
    assert_eq!(left.len(), right.len(), "vector length mismatch");
    for (index, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        assert!(
            (l - r).abs() < 1.0e-10,
            "value mismatch at index {index}: left={l:.16e}, right={r:.16e}"
        );
    }
}

fn assert_structures_close(left: &ClusterStructure, right: &ClusterStructure) {
    assert_eq!(left.atoms.len(), right.atoms.len(), "atom-count mismatch");
    assert_eq!(left.label, right.label, "label mismatch");
    for (index, (lhs, rhs)) in left.atoms.iter().zip(right.atoms.iter()).enumerate() {
        assert_eq!(
            lhs.species, rhs.species,
            "species mismatch at atom {index}: left={}, right={}",
            lhs.species, rhs.species
        );
        for (axis, (l, r)) in lhs.cartesian.iter().zip(rhs.cartesian.iter()).enumerate() {
            assert!(
                (l - r).abs() < 1.0e-10,
                "coordinate mismatch at atom {index} axis {axis}: left={l:.16e}, right={r:.16e}"
            );
        }
    }
}

#[test]
fn overlap_fingerprints_match_frozen_legacy_fixture_without_p_orbitals() {
    let snapshot = snapshot();
    let engine = OverlapMatrixFingerprintEngine {
        include_p_orbitals: false,
    };

    let current = snapshot
        .variants
        .iter()
        .map(|structure| engine.fingerprint(structure).expect("current fingerprint"))
        .collect::<Vec<_>>();

    for (current_fp, expected_fp) in current.iter().zip(snapshot.fingerprints_without_p.iter()) {
        assert_vectors_close(&current_fp.values, &expected_fp.values);
    }
}

#[test]
fn overlap_fingerprints_match_frozen_legacy_fixture_with_p_orbitals() {
    let snapshot = snapshot();
    let engine = OverlapMatrixFingerprintEngine {
        include_p_orbitals: true,
    };

    let current = snapshot
        .variants
        .iter()
        .map(|structure| engine.fingerprint(structure).expect("current fingerprint"))
        .collect::<Vec<_>>();

    for (current_fp, expected_fp) in current.iter().zip(snapshot.fingerprints_with_p.iter()) {
        assert_vectors_close(&current_fp.values, &expected_fp.values);
    }
}

#[test]
fn pairwise_distances_match_frozen_legacy_fixture() {
    let snapshot = snapshot();
    let engine = OverlapMatrixFingerprintEngine {
        include_p_orbitals: true,
    };
    let current_fps = snapshot
        .variants
        .iter()
        .map(|structure| engine.fingerprint(structure).expect("current fingerprint"))
        .collect::<Vec<_>>();

    let current_distances = pairwise_distances(&current_fps).expect("current distances");

    assert_eq!(
        current_distances.len(),
        snapshot.pairwise_distances_with_p.len()
    );
    for (current_row, expected_row) in current_distances
        .iter()
        .zip(snapshot.pairwise_distances_with_p.iter())
    {
        assert_vectors_close(current_row, expected_row);
    }
}

#[test]
fn elimination_matches_frozen_legacy_fixture() {
    let snapshot = snapshot();
    let engine = OverlapMatrixFingerprintEngine {
        include_p_orbitals: false,
    };
    let current_fps = snapshot
        .variants
        .iter()
        .map(|structure| engine.fingerprint(structure).expect("current fingerprint"))
        .collect::<Vec<_>>();
    let current_distances = pairwise_distances(&current_fps).expect("current distances");
    let current_survivors =
        eliminate_by_threshold(&snapshot.variants, &current_distances, 1.0e-2).expect("survive");

    assert_eq!(current_survivors.len(), snapshot.survivors_without_p.len());
    for (current, expected) in current_survivors
        .iter()
        .zip(snapshot.survivors_without_p.iter())
    {
        assert_structures_close(current, expected);
    }
}

#[test]
fn perturbation_matches_frozen_legacy_fixture() {
    let snapshot = snapshot();
    let current = DefaultClusterPerturbationEngine
        .generate(&snapshot.source, snapshot.perturbation_config, 3)
        .expect("current perturbation batch");

    assert_eq!(current.variants.len(), snapshot.perturbed_variants.len());
    for (current_variant, expected_variant) in current
        .variants
        .iter()
        .zip(snapshot.perturbed_variants.iter())
    {
        assert_structures_close(current_variant, expected_variant);
    }
}
