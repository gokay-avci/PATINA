#![forbid(unsafe_code)]

pub mod classify;
pub mod error;
pub mod fixtures;
pub mod framework;
pub mod model;
pub mod operations;
pub mod optimize;
pub mod periodic_table;
pub mod preprocess;
pub mod subgroups;
pub mod symmetrize;
pub mod symmetry_elements;

pub use classify::{
    analyze_point_group, classify_point_group, point_group_signature,
    scan_point_groups_over_tolerance, ClassifiedPointGroup, PointGroupSignature,
    ToleranceScanSummary, ToleranceWindowHit,
};
pub use error::SyvaError;
pub use fixtures::{
    bundled_fixture, bundled_fixture_manifest, bundled_fixture_output_path, bundled_fixture_paths,
    load_fixture_input, load_fixture_output_summary, parse_fixture_input,
    parse_fixture_output_summary, BundledSyvaFixture, SyvaFixtureEquivalenceClass,
    SyvaFixtureOutputSummary, SyvaFixturePaths,
};
pub use framework::{
    classify_framework_group, symmetry_equivalence_classes,
    symmetry_equivalence_classes_for_search, AxisRole, ClassifiedFrameworkGroup,
    FrameworkAtomAssignment, FrameworkGroupComponent, FrameworkSubspaceKind, PlaneRole,
    SymmetryEquivalenceClass,
};
pub use model::{
    species_for_atomic_number, ClusterAtom, ClusterGeometry, DetectionResult, DetectionSettings,
    PointGroupLabel, SyvaInputAtom, SyvaInputGeometry, SyvaSubsetSpec,
};
pub use operations::{
    summarize_operations, summarize_representative_operation_classes, RepresentativeOperationClass,
    SymmetryOperationSummary,
};
pub use optimize::{
    optimize_subgroup_symmetry_elements, OptimizedOperationGeometry, OptimizedSubgroupSymmetry,
};
pub use periodic_table::{
    atomic_number_for_symbol, canonicalize_species_symbol, legacy_atomic_mass,
    symbol_for_atomic_number,
};
pub use preprocess::{
    preprocess_geometry, SyvaAtomClass, SyvaPreprocessedAtom, SyvaPreprocessedGeometry,
    SyvaRunSettings,
};
pub use subgroups::{enumerate_basic_subgroups, BasicSubgroupSelection};
pub use symmetrize::{
    symmetrize_subgroup_geometry, symmetrize_verified_subgroup_geometry,
    verify_symmetrized_geometry, SymmetrizedGeometryResult, SymmetrizedGeometryStatus,
    SymmetrizedGeometryVerification, SymmetryAnchorAssignment, SymmetryAnchorKind,
    SymmetryOperationPathStep, SymmetryOrbitMemberSummary, SymmetryOrbitSummary,
    SymmetryRepresentativeClassSummary,
};
pub use symmetry_elements::{
    search_symmetry_elements, ImproperRotationRecord, InversionCenterRecord, PermutationRecord,
    ProperRotationAxisRecord, ProperRotationRecord, ReflectionPlaneRecord, SymmetryElementKind,
    SymmetrySearchResult,
};

#[cfg(test)]
mod tests {
    use super::{
        symbol_for_atomic_number, ClusterAtom, ClusterGeometry, DetectionSettings, PointGroupLabel,
        SyvaError, SyvaInputGeometry,
    };

    #[test]
    fn validates_cluster_geometry() {
        let cluster = ClusterGeometry {
            label: "water".into(),
            atoms: vec![
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [0.7, 0.0, 0.5],
                },
            ],
        };

        cluster.validate().expect("cluster valid");
    }

    #[test]
    fn rejects_invalid_tolerance() {
        let error = DetectionSettings { tolerance: 0.0 }
            .validate()
            .expect_err("invalid tolerance");
        assert_eq!(error, SyvaError::InvalidTolerance(0.0));
    }

    #[test]
    fn normalizes_point_group_label() {
        let label = PointGroupLabel::new(" C2v ").expect("label");
        assert_eq!(label.as_str(), "C2v");
    }

    #[test]
    fn converts_cluster_geometry_into_syva_input_geometry() {
        let cluster = ClusterGeometry {
            label: "mgo".into(),
            atoms: vec![
                ClusterAtom {
                    species: "Mg".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [1.0, 0.0, 0.0],
                },
            ],
        };

        let input = SyvaInputGeometry::from_cluster_geometry(&cluster).expect("syva input");
        assert_eq!(input.atoms[0].atomic_number, 12);
        assert_eq!(input.atoms[1].atomic_number, 8);
        let rendered = input.render_input_text();
        assert!(rendered.starts_with("mgo\n2\n12 "));
    }

    #[test]
    fn rejects_unsupported_species_during_syva_normalization() {
        let cluster = ClusterGeometry {
            label: "mystery".into(),
            atoms: vec![ClusterAtom {
                species: "Xx".into(),
                cartesian: [0.0, 0.0, 0.0],
            }],
        };

        let error = SyvaInputGeometry::from_cluster_geometry(&cluster).expect_err("unsupported");
        assert_eq!(
            error,
            SyvaError::UnsupportedSpecies {
                index: 0,
                species: "Xx".into()
            }
        );
    }

    #[test]
    fn supports_atomic_symbol_lookup_to_element_100() {
        assert_eq!(symbol_for_atomic_number(100), Some("Fm"));
    }
}
