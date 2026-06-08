use patina_topology::domain::Composition;
use patina_topology::fingerprints::{fingerprint_candidate, group_by_jaccard, SignatureRecord};
use patina_topology::generators::{
    generate_candidates, BarrelGenerator, ChemicalEdgePolicy, ConstrainedRandomGraphGenerator,
    DegreeBounds, GenerationConstraints, GenerationRequest, PlatonicGenerator,
    RandomGraphGenerator, ShellGenerator, TopologyMonteCarloGenerator,
};
use patina_topology::io::figures::write_preview_bundle;
use patina_topology::io::jsonl::{read_candidates_jsonl, write_candidates_jsonl};
use patina_topology::io::provenance::{append_checkpoint, artifact, write_manifest};
use patina_topology::io::xyz::write_classified_xyz_directory;
use patina_topology::io::xyz::xyz_frame;
use patina_topology::{fingerprint_candidate_with_symmetry, SyvaPointSymmetryBackend};
use patina_topology::{validate_geometry, GeometryValidationConfig};
use patina_topology::{CheckpointChecklist, CheckpointStatus};
use std::collections::BTreeMap;

#[test]
fn generation_request_supports_ti3n4_without_one_to_one_assumption() {
    let candidates = generate_candidates(&GenerationRequest {
        formula: "Ti3N4".into(),
        n_min: 1,
        n_max: 2,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["ring".into(), "barrel".into(), "wire".into()],
        max_candidates: 100,
        seed: Some(42),
        constraints: GenerationConstraints::for_bond_length(1.5),
    })
    .unwrap();

    assert!(candidates.iter().any(|candidate| {
        candidate.composition.total_formula() == "Ti3N4" && candidate.atoms.len() == 7
    }));
    assert!(candidates.iter().any(|candidate| {
        candidate.composition.total_formula() == "Ti6N8" && candidate.atoms.len() == 14
    }));
    for candidate in candidates {
        let ti = candidate
            .atoms
            .iter()
            .filter(|atom| atom.element.as_str() == "Ti")
            .count();
        let n = candidate
            .atoms
            .iter()
            .filter(|atom| atom.element.as_str() == "N")
            .count();
        assert_eq!(ti * 4, n * 3);
        candidate.validate().unwrap();
    }
}

#[test]
fn two_layer_square_barrel_golden_signature() {
    let candidate = BarrelGenerator::new(1.5)
        .generate(
            1,
            Composition::from_formula("AB", 4).unwrap(),
            Some(42),
            4,
            2,
        )
        .unwrap();
    let signature = fingerprint_candidate(&candidate);

    assert_eq!(candidate.atoms.len(), 8);
    assert_eq!(candidate.bonds.len(), 12);
    assert_eq!(signature.graph_basic.connected_components, 1);
    assert_eq!(signature.graph_basic.cycle_rank, 5);
    assert_eq!(signature.geometry.morphology_label, "barrel");
    assert!(signature
        .jaccard_features
        .contains(&"morphology:barrel".into()));
}

#[test]
fn jsonl_roundtrip_preserves_candidate_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("candidates.jsonl");
    let candidates = generate_candidates(&GenerationRequest {
        formula: "AB2".into(),
        n_min: 1,
        n_max: 1,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["ring".into(), "wire".into()],
        max_candidates: 10,
        seed: Some(7),
        constraints: GenerationConstraints::for_bond_length(1.5),
    })
    .unwrap();
    write_candidates_jsonl(&path, &candidates).unwrap();
    let restored = read_candidates_jsonl(&path).unwrap();
    assert_eq!(restored.len(), candidates.len());
    assert_eq!(restored[0].composition.total_formula(), "AB2");
}

#[test]
fn xyz_frame_has_comment_provenance_and_expected_line_count() {
    let candidate = generate_candidates(&GenerationRequest {
        formula: "MgO".into(),
        n_min: 2,
        n_max: 2,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["ring".into()],
        max_candidates: 1,
        seed: Some(1),
        constraints: GenerationConstraints::for_bond_length(1.5),
    })
    .unwrap()
    .remove(0);
    let frame = xyz_frame(&candidate);
    assert!(frame.comment.contains("formula=Mg2O2"));
    assert!(frame.comment.contains("generator=ring"));
    let encoded = patina_sci_kernel::codec::xyz::encode_xyz_frames(&[frame]).unwrap();
    assert_eq!(encoded.lines().count(), candidate.atoms.len() + 2);
}

#[test]
fn jaccard_grouping_connects_similar_records() {
    let candidates = generate_candidates(&GenerationRequest {
        formula: "AB".into(),
        n_min: 2,
        n_max: 2,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["ring".into(), "wire".into()],
        max_candidates: 10,
        seed: Some(1),
        constraints: GenerationConstraints::for_bond_length(1.5),
    })
    .unwrap();
    let records = candidates
        .iter()
        .map(|candidate| SignatureRecord {
            candidate_id: candidate.id.0.clone(),
            generator: candidate.generator.name.clone(),
            signature: fingerprint_candidate(candidate),
        })
        .collect::<Vec<_>>();
    let groups = group_by_jaccard(&records, 1.0);
    assert_eq!(groups.groups.len(), records.len());
}

#[test]
fn platonic_and_random_generators_preserve_composition_and_connectivity() {
    let composition = Composition::from_formula("A2B3", 2).unwrap();
    let platonic = PlatonicGenerator::new(1.5)
        .generate_family(1, composition.clone(), Some(5))
        .unwrap();
    assert!(!platonic.is_empty());
    for candidate in &platonic {
        assert_eq!(candidate.atoms.len(), 10);
        assert!(candidate.bonds.len() >= candidate.atoms.len() - 1);
        assert_eq!(
            fingerprint_candidate(candidate)
                .graph_basic
                .connected_components,
            1
        );
    }

    let random = RandomGraphGenerator::new(1.5)
        .generate(99, composition, Some(5))
        .unwrap();
    let signature = fingerprint_candidate(&random);
    assert_eq!(random.atoms.len(), 10);
    assert_eq!(signature.graph_basic.connected_components, 1);
    assert_eq!(
        signature.graph_basic.cycle_rank,
        random.bonds.len() as isize - 10 + 1
    );
}

#[test]
fn provenance_and_preview_artifacts_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let candidates = generate_candidates(&GenerationRequest {
        formula: "AB".into(),
        n_min: 2,
        n_max: 3,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["ring".into(), "wire".into(), "platonic".into()],
        max_candidates: 20,
        seed: Some(9),
        constraints: GenerationConstraints::for_bond_length(1.5),
    })
    .unwrap();
    let records = candidates
        .iter()
        .map(|candidate| SignatureRecord {
            candidate_id: candidate.id.0.clone(),
            generator: candidate.generator.name.clone(),
            signature: fingerprint_candidate(candidate),
        })
        .collect::<Vec<_>>();

    let preview = write_preview_bundle(dir.path().join("figures"), &candidates, &records, 6, 0.7)
        .expect("preview");
    assert!(preview.motif_gallery_svg.exists());
    assert!(preview.signature_summary_svg.exists());
    assert!(preview.symmetry_summary_svg.exists());
    assert!(preview.validation_summary_svg.exists());
    assert!(preview.generator_morphology_matrix_svg.exists());
    assert!(preview.topology_metrics_svg.exists());
    assert!(preview.jaccard_graph_dot.exists());
    assert!(preview.atlas_html.exists());

    let manifest = dir.path().join("manifest.json");
    write_manifest(
        &manifest,
        "test-campaign",
        "test",
        BTreeMap::from([("formula".into(), "AB".into())]),
        vec![artifact(
            "candidates.jsonl",
            "jsonl",
            "test_candidates",
            Some(candidates.len()),
        )],
        CheckpointChecklist::topology_generation().mark(
            "jsonl_written",
            CheckpointStatus::Passed,
            "test evidence",
        ),
    )
    .unwrap();
    append_checkpoint(
        dir.path().join("logs/checkpoints.jsonl"),
        "test",
        CheckpointStatus::Passed,
        "checkpoint",
        Vec::new(),
    )
    .unwrap();
    assert!(manifest.exists());
    assert!(dir.path().join("logs/checkpoints.jsonl").exists());
}

#[test]
fn syva_backend_populates_symmetry_signature_for_regular_ring() {
    let candidate = generate_candidates(&GenerationRequest {
        formula: "MgO".into(),
        n_min: 4,
        n_max: 4,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["ring".into()],
        max_candidates: 1,
        seed: Some(3),
        constraints: GenerationConstraints::for_bond_length(1.5),
    })
    .unwrap()
    .remove(0);
    let backend = SyvaPointSymmetryBackend::new(1.0e-3);
    let signature = fingerprint_candidate_with_symmetry(&candidate, Some(&backend));
    let symmetry = signature.symmetry.expect("symmetry signature");

    assert_eq!(symmetry.backend.as_deref(), Some("syva"));
    assert_eq!(symmetry.status, "ok");
    assert!(symmetry.point_group.is_some());
    assert!(symmetry.permutation_count.unwrap_or_default() >= 1);
    assert!(!symmetry.equivalence_classes.is_empty());
    assert!(signature
        .jaccard_features
        .iter()
        .any(|feature| feature.starts_with("point_group:")));
}

#[test]
fn shell_generators_create_cage_like_connected_motifs() {
    let composition = Composition::from_formula("Ti3N4", 2).unwrap();
    let generator = ShellGenerator::new(1.5);
    let candidates = vec![
        generator
            .closed_cage(1, composition.clone(), Some(11))
            .unwrap(),
        generator
            .hollow_shell(2, composition.clone(), Some(11))
            .unwrap(),
        generator
            .multi_shell(3, composition.clone(), Some(11))
            .unwrap(),
        generator
            .onion_like_shell(4, composition.clone(), Some(11))
            .unwrap(),
        generator
            .face_capped_polyhedron(5, composition.clone(), Some(11))
            .unwrap(),
        generator
            .edge_decorated_polyhedron(6, composition, Some(11))
            .unwrap(),
    ];

    for candidate in &candidates {
        candidate.validate().unwrap();
        let signature = fingerprint_candidate(candidate);
        assert_eq!(signature.graph_basic.connected_components, 1);
        assert!(signature.graph_basic.n_edges >= signature.graph_basic.n_nodes - 1);
        assert!(matches!(
            signature.geometry.morphology_label.as_str(),
            "cage" | "multi_shell"
        ));
    }
}

#[test]
fn classified_xyz_export_writes_topology_folders() {
    let dir = tempfile::tempdir().unwrap();
    let mut ring = patina_topology::generators::RingGenerator::new(1.5)
        .generate(1, Composition::from_formula("AB", 3).unwrap(), Some(1))
        .unwrap();
    ring.topology_signature = Some(fingerprint_candidate(&ring));
    let mut cage = ShellGenerator::new(1.5)
        .closed_cage(2, Composition::from_formula("AB", 4).unwrap(), Some(1))
        .unwrap();
    cage.topology_signature = Some(fingerprint_candidate(&cage));
    let candidates = [ring, cage];
    let classified = candidates
        .iter()
        .map(|candidate| {
            (
                candidate,
                candidate
                    .topology_signature
                    .as_ref()
                    .unwrap()
                    .geometry
                    .morphology_label
                    .clone(),
            )
        })
        .collect::<Vec<_>>();
    let count = write_classified_xyz_directory(dir.path(), classified).unwrap();
    assert_eq!(count, 2);
    assert!(dir.path().join("ring").exists());
    assert!(dir.path().join("cage").exists());
}

#[test]
fn constrained_random_graph_preserves_composition_and_reports_validation() {
    let candidate = ConstrainedRandomGraphGenerator::new(1.5)
        .generate(1, Composition::from_formula("Ti3N4", 2).unwrap(), Some(123))
        .unwrap();
    candidate.validate().unwrap();
    assert_eq!(candidate.atoms.len(), 14);
    assert_eq!(
        candidate
            .atoms
            .iter()
            .filter(|atom| atom.element.as_str() == "Ti")
            .count(),
        6
    );
    assert_eq!(
        candidate
            .atoms
            .iter()
            .filter(|atom| atom.element.as_str() == "N")
            .count(),
        8
    );
    let report = validate_geometry(&candidate, &GeometryValidationConfig::for_bond_length(1.5));
    assert_eq!(report.connected_components, 1);
    assert!(candidate
        .parameters
        .contains_key("constrained_random_report"));
    assert!(candidate.parameters.contains_key("validation_report"));
}

#[test]
fn topology_mc_accepts_novel_candidates_without_energy() {
    let candidates = TopologyMonteCarloGenerator::new(1.5)
        .generate_family(1, Composition::from_formula("AB2", 4).unwrap(), Some(99))
        .unwrap();
    assert!(!candidates.is_empty());
    assert!(candidates.len() <= 8);
    let mut wl = std::collections::BTreeSet::new();
    for candidate in &candidates {
        assert_eq!(candidate.generator.name, "topology_mc");
        assert!(candidate.topology_signature.is_some());
        assert!(candidate.parameters.contains_key("topology_mc_report"));
        wl.insert(
            candidate
                .topology_signature
                .as_ref()
                .unwrap()
                .hashes
                .wl_hash
                .clone(),
        );
    }
    assert_eq!(wl.len(), candidates.len());
}

#[test]
fn generation_constraints_flow_into_constrained_random_generator() {
    let mut constraints = GenerationConstraints::for_bond_length(1.5);
    constraints.edge_policy = ChemicalEdgePolicy::RequireHetero;
    constraints.target_extra_edges = Some(3);
    constraints.degree_bounds.insert(
        patina_topology::ElementSymbol::from("Ti"),
        DegreeBounds { min: 1, max: 4 },
    );
    constraints.degree_bounds.insert(
        patina_topology::ElementSymbol::from("N"),
        DegreeBounds { min: 1, max: 4 },
    );
    let candidates = generate_candidates(&GenerationRequest {
        formula: "Ti3N4".into(),
        n_min: 2,
        n_max: 2,
        bond_length: 1.5,
        bond_length_source: None,
        generator_names: vec!["constrained_random_graph".into()],
        max_candidates: 10,
        seed: Some(101),
        constraints,
    })
    .unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.generator.name, "constrained_random_graph");
    assert!(candidate
        .parameters
        .contains_key("constrained_random_report"));
    for bond in &candidate.bonds {
        assert_ne!(
            candidate.atoms[bond.i].element,
            candidate.atoms[bond.j].element
        );
    }
}
