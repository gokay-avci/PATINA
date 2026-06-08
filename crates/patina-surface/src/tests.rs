use crate::analysis::diagnostics::run_diagnostics_with_config;
use crate::cut::find_safe_offsets;
use crate::engine::analyze_surface_bond_diagnostics;
use crate::generation::{compute_geometry, LatticeOps};
use crate::graph::bonding::{
    build_bond_graph, build_bond_graph_from_neighbour_list, BondingConfig,
};
use crate::graph::connectivity::{component_count, largest_component_size};
use crate::graph::neighbours::{build_knn_neighbour_list, NeighbourConfig};
use crate::reduction::dedup::dedup_atoms_inplace;
use crate::{
    DedupConfig, DefaultSurfaceGenerationEngine, DipoleCancellationPolicy,
    HeuristicSurfacePolarityAnalyzer, IonicMoveKind, IonicReconstructionCandidate,
    IonicReconstructionConfig, IonicReconstructionRequest, IonicReconstructionResult,
    IonicSiteModification, MillerIndex, PaperLikeSurfaceProtocol, SlabReductionConfig, SurfaceAtom,
    SurfaceCutStrategy, SurfaceFace, SurfaceGenerationConfig, SurfaceGenerationEngine,
    SurfaceInterfaceError, SurfaceParentStructure, SurfacePolarityAnalyzer, SurfacePolarityClass,
    SurfacePolarityReport, SurfaceReconstructionMode, SurfaceSlab, SurfaceTerminationBias,
};
use patina_raspa::{FrameworkAtom, PeriodicFramework, RaspaPeriodicity};
use patina_types::Candidate;

fn load_fixture_request(name: &str) -> crate::SurfaceGenerationRequest {
    let text = match name {
        "mgo_100" => include_str!("../tests/fixtures/mgo_100_request.json"),
        "mgo_210" => include_str!("../tests/fixtures/mgo_210_request.json"),
        _ => panic!("unknown fixture: {name}"),
    };
    serde_json::from_str(text).expect("valid fixture request")
}

fn periodic_candidate() -> Candidate {
    Candidate {
        label: "framework".into(),
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
        lattice: Some([[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]]),
        periodic_axes: [true, true, true],
    }
}

#[test]
fn miller_index_rejects_zero_vector() {
    let error = MillerIndex::new(0, 0, 0).expect_err("must reject");
    assert!(matches!(error, SurfaceInterfaceError::ZeroMillerIndex));
}

#[test]
fn parent_structure_accepts_three_d_candidate() {
    let parent = SurfaceParentStructure::try_from_candidate(&periodic_candidate()).expect("parent");
    assert_eq!(parent.atoms.len(), 2);
    assert_eq!(parent.periodic_axes, [true, true, true]);
}

#[test]
fn parent_structure_rejects_partial_periodicity() {
    let mut candidate = periodic_candidate();
    candidate.periodic_axes = [true, true, false];
    let error = SurfaceParentStructure::try_from_candidate(&candidate).expect_err("reject");
    assert!(matches!(
        error,
        SurfaceInterfaceError::PartialPeriodicityUnsupported { .. }
    ));
}

#[test]
fn parent_structure_accepts_periodic_framework_conversion() {
    let framework = PeriodicFramework {
        label: "framework".into(),
        lattice: [[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]],
        periodic_axes: [true, true, true],
        periodicity: RaspaPeriodicity::ThreeD,
        atoms: vec![
            FrameworkAtom {
                species: "Mg".into(),
                fractional: [0.0, 0.0, 0.0],
            },
            FrameworkAtom {
                species: "O".into(),
                fractional: [0.5, 0.5, 0.5],
            },
        ],
    };
    let parent = SurfaceParentStructure::try_from_periodic_framework(&framework).expect("parent");
    assert_eq!(parent.to_candidate(), framework.to_candidate());
}

#[test]
fn parent_structure_checked_conversion_rejects_partial_periodicity() {
    let parent = SurfaceParentStructure {
        label: "partial".into(),
        lattice: [[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]],
        periodic_axes: [true, true, false],
        atoms: vec![crate::SurfaceFrameworkAtom {
            species: "Mg".into(),
            fractional: [0.0, 0.0, 0.0],
        }],
    };

    let error = parent
        .try_into_framework3d()
        .expect_err("partial periodic parent must fail checked conversion");
    assert!(matches!(
        error,
        SurfaceInterfaceError::PartialPeriodicityUnsupported { .. }
    ));
}

#[test]
fn surface_config_validates_positive_geometry() {
    let config = SurfaceGenerationConfig {
        miller: MillerIndex::new(1, 1, 0).expect("miller"),
        reconstruction: SurfaceReconstructionMode::None,
        ..SurfaceGenerationConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn surface_config_rejects_zero_supercell_repeat() {
    let mut config = SurfaceGenerationConfig::default();
    config.supercell.repeat_a = 0;
    let error = config.validate().expect_err("must reject");
    assert!(matches!(
        error,
        SurfaceInterfaceError::InvalidSupercell {
            repeat_a: 0,
            repeat_b: 1
        }
    ));
}

#[test]
fn surface_slab_exports_partial_periodic_candidate() {
    let slab = SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[4.2, 0.0, 0.0], [0.0, 8.4, 0.0], [0.0, 0.0, 24.0]],
        periodic_axes: [true, true, false],
        atoms: vec![SurfaceAtom {
            species: "Mg".into(),
            fractional: [0.25, 0.5, 0.75],
            cartesian: [0.0, 0.0, 0.0],
            source_fractional: Some([0.0, 0.0, 0.0]),
        }],
        thickness_angstrom: 10.0,
        vacuum_angstrom: 14.0,
    };
    let candidate = slab.to_candidate();
    assert_eq!(candidate.periodic_axes, [true, true, false]);
    assert_eq!(candidate.species, vec!["Mg"]);
    assert_eq!(candidate.fractional_coords, vec![[0.25, 0.5, 0.75]]);
}

#[test]
fn default_surface_engine_generates_a_slab_from_bulk_framework() {
    let engine = DefaultSurfaceGenerationEngine;
    let result = engine
        .generate_surface(&crate::SurfaceGenerationRequest {
            parent: SurfaceParentStructure::try_from_candidate(&periodic_candidate())
                .expect("parent"),
            config: SurfaceGenerationConfig {
                miller: MillerIndex::new(1, 0, 0).expect("miller"),
                thickness_angstrom: 8.0,
                vacuum_angstrom: 10.0,
                supercell: crate::SurfaceSupercellConfig::default(),
                cut_strategy: SurfaceCutStrategy::TopologyAware,
                cut_offset_fraction: None,
                slab_reduction: SlabReductionConfig::default(),
                reconstruction: SurfaceReconstructionMode::None,
                termination_bias: SurfaceTerminationBias::Neutral,
            },
        })
        .expect("surface");

    assert_eq!(result.slab.periodic_axes, [true, true, false]);
    assert!(!result.slab.atoms.is_empty());
    assert!(result.diagnostics.interplanar_spacing_angstrom.unwrap() > 0.0);
    assert!(result.diagnostics.layer_count.unwrap() >= 1);
}

#[test]
fn surface_supercell_scales_inplane_lattice_and_atom_count() {
    let engine = DefaultSurfaceGenerationEngine;
    let base = engine
        .generate_surface(&crate::SurfaceGenerationRequest {
            parent: SurfaceParentStructure::try_from_candidate(&periodic_candidate())
                .expect("parent"),
            config: SurfaceGenerationConfig {
                miller: MillerIndex::new(1, 0, 0).expect("miller"),
                thickness_angstrom: 8.0,
                vacuum_angstrom: 10.0,
                supercell: crate::SurfaceSupercellConfig {
                    repeat_a: 1,
                    repeat_b: 1,
                },
                cut_strategy: SurfaceCutStrategy::TopologyAware,
                cut_offset_fraction: None,
                slab_reduction: SlabReductionConfig::default(),
                reconstruction: SurfaceReconstructionMode::None,
                termination_bias: SurfaceTerminationBias::Neutral,
            },
        })
        .expect("surface");
    let tiled = engine
        .generate_surface(&crate::SurfaceGenerationRequest {
            parent: SurfaceParentStructure::try_from_candidate(&periodic_candidate())
                .expect("parent"),
            config: SurfaceGenerationConfig {
                miller: MillerIndex::new(1, 0, 0).expect("miller"),
                thickness_angstrom: 8.0,
                vacuum_angstrom: 10.0,
                supercell: crate::SurfaceSupercellConfig {
                    repeat_a: 2,
                    repeat_b: 3,
                },
                cut_strategy: SurfaceCutStrategy::TopologyAware,
                cut_offset_fraction: None,
                slab_reduction: SlabReductionConfig::default(),
                reconstruction: SurfaceReconstructionMode::None,
                termination_bias: SurfaceTerminationBias::Neutral,
            },
        })
        .expect("surface");

    let base_a = nalgebra::Vector3::from_row_slice(&base.slab.lattice[0]).norm();
    let base_b = nalgebra::Vector3::from_row_slice(&base.slab.lattice[1]).norm();
    let tiled_a = nalgebra::Vector3::from_row_slice(&tiled.slab.lattice[0]).norm();
    let tiled_b = nalgebra::Vector3::from_row_slice(&tiled.slab.lattice[1]).norm();

    assert!((tiled_a - 2.0 * base_a).abs() < 1.0e-6);
    assert!((tiled_b - 3.0 * base_b).abs() < 1.0e-6);
    assert_eq!(tiled.slab.atoms.len(), 6 * base.slab.atoms.len());
    assert_eq!(base.slab.thickness_angstrom, tiled.slab.thickness_angstrom);
    assert_eq!(base.slab.vacuum_angstrom, tiled.slab.vacuum_angstrom);
}

#[test]
fn fixture_mgo_100_has_no_topology_safe_gap_and_expected_spacing() {
    let request = load_fixture_request("mgo_100");
    let lattice = LatticeOps::new(request.parent.lattice).expect("lattice");
    let mut warnings = Vec::new();
    let geometry = compute_geometry(&lattice, &request.config, &mut warnings).expect("geometry");

    let cuts = find_safe_offsets(&request.parent, &lattice, geometry.slab_normal);
    assert!(
        cuts.is_empty(),
        "mgo (100) should expose no vdW-safe void cuts"
    );
    assert!((geometry.d_hkl - 4.2316).abs() < 1.0e-6);
    assert_eq!(geometry.n_layers, 4);
}

#[test]
fn fixture_mgo_100_engine_uses_zero_offset_fallback() {
    let engine = DefaultSurfaceGenerationEngine;
    let request = load_fixture_request("mgo_100");
    let result = engine.generate_surface(&request).expect("surface");

    assert_eq!(result.diagnostics.topology_safe_cut, Some(false));
    assert_eq!(result.diagnostics.chosen_cut_offset_angstrom, Some(0.0));
    assert_eq!(result.diagnostics.layer_count, Some(4));
    assert!(!result.slab.atoms.is_empty());
}

#[test]
fn fixture_mgo_210_generates_non_empty_high_index_slab() {
    let engine = DefaultSurfaceGenerationEngine;
    let request = load_fixture_request("mgo_210");
    let result = engine.generate_surface(&request).expect("surface");

    assert_eq!(result.slab.periodic_axes, [true, true, false]);
    assert!(result.diagnostics.interplanar_spacing_angstrom.unwrap() > 0.0);
    assert!(result.diagnostics.layer_count.unwrap() >= 1);
    assert!(!result.slab.atoms.is_empty());
    assert!(result.diagnostics.graph_diagnostics.is_some());
    assert!(result.diagnostics.surface_bond_summary.is_some());
}

#[test]
fn ionic_reconstruction_contracts_hold_optional_downstream_state() {
    let slab = SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[4.2, 0.0, 0.0], [0.0, 8.4, 0.0], [0.0, 0.0, 24.0]],
        periodic_axes: [true, true, false],
        atoms: vec![SurfaceAtom {
            species: "O".into(),
            fractional: [0.2, 0.3, 0.4],
            cartesian: [1.0, 2.0, 3.0],
            source_fractional: Some([0.1, 0.2, 0.3]),
        }],
        thickness_angstrom: 10.0,
        vacuum_angstrom: 14.0,
    };
    let polarity = SurfacePolarityReport {
        classification: SurfacePolarityClass::Indeterminate,
        residual_dipole_proxy_z: Some(0.4),
        top_species_counts: vec![("O".into(), 1)],
        bottom_species_counts: Vec::new(),
        warnings: vec!["formal charges not supplied".into()],
    };
    let config = IonicReconstructionConfig {
        target_face: SurfaceFace::Top,
        dipole_policy: DipoleCancellationPolicy::BestEffort,
        candidate_limit: 8,
        max_modified_sites: 2,
        maintain_stoichiometry: true,
        supercell: [2, 1],
        frozen_layer_count: 1,
    };
    let request = IonicReconstructionRequest {
        slab: slab.clone(),
        config: config.clone(),
    };
    let result = IonicReconstructionResult {
        input_slab: slab.clone(),
        initial_polarity: Some(polarity.clone()),
        candidates: vec![IonicReconstructionCandidate {
            slab: slab.clone(),
            modifications: vec![IonicSiteModification {
                kind: IonicMoveKind::Vacancy,
                species: "O".into(),
                face: SurfaceFace::Top,
                source_atom_index: Some(0),
                target_fractional: None,
                notes: vec!["test".into()],
            }],
            polarity: Some(polarity),
            warnings: Vec::new(),
        }],
        warnings: vec!["downstream only".into()],
    };

    assert_eq!(request.config, config);
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        result.candidates[0].modifications[0].kind,
        IonicMoveKind::Vacancy
    );
}

#[test]
fn paper_like_protocol_captures_zno_surface_workflow_defaults() {
    let protocol = PaperLikeSurfaceProtocol::zno_polar_0001_paper_2017();
    assert_eq!(protocol.material_label, "ZnO");
    assert_eq!(protocol.target_surface.as_array(), [0, 0, 1]);
    assert_eq!(protocol.region_model.total_layers, 6);
    assert_eq!(protocol.region_model.relaxed_layers, 3);
    assert_eq!(protocol.sampling.supercell, [5, 5]);
    assert_eq!(protocol.sampling.samples_per_occupancy, 10_000);
    assert!(protocol.compensating_charge_grid.enabled);
}

#[test]
fn surface_slab_roundtrips_from_partial_periodic_candidate() {
    let candidate = Candidate::periodic(
        "slab_input",
        vec!["Zn".into()],
        vec![[0.25, 0.5, 0.75]],
        [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
        [true, true, false],
    );
    let slab = SurfaceSlab::try_from_candidate(&candidate).expect("slab");
    assert_eq!(slab.to_candidate(), candidate);
}

#[test]
fn surface_slab_checked_conversion_rejects_wrong_periodic_dimension() {
    let slab = SurfaceSlab {
        label: "bad-slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, false, false],
        atoms: vec![SurfaceAtom {
            species: "Zn".into(),
            fractional: [0.25, 0.5, 0.75],
            cartesian: [1.0, 2.0, 15.0],
            source_fractional: None,
        }],
        thickness_angstrom: 8.0,
        vacuum_angstrom: 12.0,
    };

    let error = slab
        .try_into_slab2d()
        .expect_err("one-dimensional periodic slab must fail checked conversion");
    assert!(matches!(
        error,
        SurfaceInterfaceError::PartialPeriodicityUnsupported { .. }
            | SurfaceInterfaceError::NonTwoDimensionalSlabCandidate { .. }
    ));
}

#[test]
fn heuristic_surface_polarity_analyzer_reports_species_counts() {
    let analyzer = HeuristicSurfacePolarityAnalyzer;
    let report = analyzer
        .analyze_surface_polarity(&SurfaceSlab {
            label: "slab".into(),
            parent_label: "framework".into(),
            miller: MillerIndex::new(1, 0, 0).expect("miller"),
            lattice: [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
            periodic_axes: [true, true, false],
            atoms: vec![
                SurfaceAtom {
                    species: "Zn".into(),
                    fractional: [0.0, 0.0, 0.2],
                    cartesian: [0.0, 0.0, 4.0],
                    source_fractional: None,
                },
                SurfaceAtom {
                    species: "O".into(),
                    fractional: [0.0, 0.0, 0.8],
                    cartesian: [0.0, 0.0, 16.0],
                    source_fractional: None,
                },
            ],
            thickness_angstrom: 8.0,
            vacuum_angstrom: 12.0,
        })
        .expect("report");

    assert_eq!(report.classification, SurfacePolarityClass::Polar);
    assert_eq!(report.top_species_counts, vec![("O".into(), 1)]);
    assert_eq!(report.bottom_species_counts, vec![("Zn".into(), 1)]);
}

#[test]
fn slab_dedup_reduces_inplane_wrapped_duplicates() {
    let mut slab = SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, true, false],
        atoms: vec![
            SurfaceAtom {
                species: "Zn".into(),
                fractional: [0.1, 0.2, 0.4],
                cartesian: [0.4, 0.8, 8.0],
                source_fractional: Some([0.1, 0.2, 0.4]),
            },
            SurfaceAtom {
                species: "Zn".into(),
                fractional: [1.1, 0.2, 0.4],
                cartesian: [4.4, 0.8, 8.0],
                source_fractional: Some([1.1, 0.2, 0.4]),
            },
        ],
        thickness_angstrom: 8.0,
        vacuum_angstrom: 12.0,
    };

    let report = dedup_atoms_inplace(
        &mut slab,
        &DedupConfig {
            frac_tol: 1.0e-3,
            inplane_only: true,
            require_same_element: true,
        },
    )
    .expect("dedup");

    assert_eq!(report.removed, 1);
    assert_eq!(slab.atoms.len(), 1);
    assert_eq!(slab.atoms[0].fractional, [0.1, 0.2, 0.4]);
}

#[test]
fn surface_bond_diagnostics_flags_undercoordinated_face_atoms() {
    let diagnostics = analyze_surface_bond_diagnostics(&SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, true, false],
        atoms: vec![
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.0, 0.0, 0.2],
                cartesian: [0.0, 0.0, 4.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.0, 0.0, 0.8],
                cartesian: [0.0, 0.0, 16.0],
                source_fractional: None,
            },
        ],
        thickness_angstrom: 8.0,
        vacuum_angstrom: 12.0,
    })
    .expect("diagnostics");

    assert_eq!(diagnostics.bottom_indices.len(), 1);
    assert_eq!(diagnostics.top_indices.len(), 1);
    assert_eq!(diagnostics.dangling_candidates.len(), 2);
}

#[test]
fn graph_neighbour_list_builds_mutual_neighbors_for_surface_slab() {
    let slab = SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, true, false],
        atoms: vec![
            SurfaceAtom {
                species: "Mg".into(),
                fractional: [0.0, 0.0, 0.3],
                cartesian: [0.0, 0.0, 6.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.5, 0.0, 0.3],
                cartesian: [2.0, 0.0, 6.0],
                source_fractional: None,
            },
        ],
        thickness_angstrom: 6.0,
        vacuum_angstrom: 14.0,
    };

    let neighbours = build_knn_neighbour_list(
        &slab,
        &NeighbourConfig {
            k: 4,
            max_radius: Some(3.0),
            surface_refine: true,
            ..NeighbourConfig::default()
        },
    )
    .expect("neighbours");

    assert_eq!(neighbours.k, 4);
    assert_eq!(neighbours.neighbours.len(), 2);
    assert_eq!(neighbours.neighbours[0].len(), 1);
    assert_eq!(neighbours.neighbours[1].len(), 1);
    assert_eq!(neighbours.neighbours[0][0].j, 1);
    assert!(neighbours.neighbours[0][0].r > 0.0);
    assert!(neighbours.neighbours[0][0].mic.norm() > 0.0);
    assert!(neighbours.neighbours[0][0].weight > 0.0);
}

#[test]
fn graph_bond_graph_from_neighbours_matches_direct_components() {
    let slab = SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[6.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, true, false],
        atoms: vec![
            SurfaceAtom {
                species: "Mg".into(),
                fractional: [0.10, 0.10, 0.3],
                cartesian: [0.6, 0.6, 6.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.35, 0.10, 0.3],
                cartesian: [2.1, 0.6, 6.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "Mg".into(),
                fractional: [0.70, 0.70, 0.7],
                cartesian: [4.2, 4.2, 14.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.95, 0.70, 0.7],
                cartesian: [5.7, 4.2, 14.0],
                source_fractional: None,
            },
        ],
        thickness_angstrom: 8.0,
        vacuum_angstrom: 12.0,
    };

    let neighbours = build_knn_neighbour_list(
        &slab,
        &NeighbourConfig {
            k: 4,
            max_radius: Some(2.6),
            ..NeighbourConfig::default()
        },
    )
    .expect("neighbours");
    let direct = build_bond_graph(&slab, &BondingConfig::default()).expect("direct graph");
    let from_neighbours =
        build_bond_graph_from_neighbour_list(&slab, &neighbours, &BondingConfig::default())
            .expect("graph from neighbours");

    assert_eq!(direct.edges.len(), from_neighbours.edges.len());
    assert_eq!(direct.connected_components().len(), 2);
    assert_eq!(from_neighbours.connected_components().len(), 2);
    assert_eq!(largest_component_size(&direct), 2);
    assert_eq!(
        component_count(&slab, &BondingConfig::default()).expect("components"),
        2
    );
}

#[test]
fn graph_diagnostics_report_detects_fragmented_components_and_isolated_atoms() {
    let slab = SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex::new(1, 0, 0).expect("miller"),
        lattice: [[8.0, 0.0, 0.0], [0.0, 8.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, true, false],
        atoms: vec![
            SurfaceAtom {
                species: "Mg".into(),
                fractional: [0.10, 0.10, 0.3],
                cartesian: [0.8, 0.8, 6.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.30, 0.10, 0.3],
                cartesian: [2.4, 0.8, 6.0],
                source_fractional: None,
            },
            SurfaceAtom {
                species: "O".into(),
                fractional: [0.85, 0.85, 0.7],
                cartesian: [6.8, 6.8, 14.0],
                source_fractional: None,
            },
        ],
        thickness_angstrom: 8.0,
        vacuum_angstrom: 12.0,
    };

    let report = run_diagnostics_with_config(&slab, BondingConfig::default()).expect("report");
    let dataset = report.to_surface_dataset();

    assert_eq!(report.n_components, 2);
    assert_eq!(report.largest_component, 2);
    assert_eq!(report.isolated_atoms, vec![2]);
    assert_eq!(dataset.n_bonds, 1);
    assert!(dataset.element_counts.contains(&("O".to_string(), 2)));
}
