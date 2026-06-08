use std::path::PathBuf;

use patina_stk::bonding::definition::BondDefinitionRule;
use patina_stk::chemistry::{amine_coordination_donor_pattern, prepare_coordination_donor_block};
use patina_stk::functional_group::FunctionalGroupBondIntent;
use patina_stk::ports::chemistry::{ChemistryToolkitPort, PythonRdkitToolkit};
use patina_stk::topology::edge::TopologyEdge;
use patina_stk::topology::libraries::zero_d::ZeroDTopologyBuilder;
use patina_stk::topology::vertex::{TopologyVertex, VertexKind};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has workspace crates parent")
        .parent()
        .expect("crates dir has workspace parent")
        .to_path_buf()
}

#[test]
fn python_rdkit_runtime_prepares_coordination_building_block() {
    let root = workspace_root();
    let toolkit = PythonRdkitToolkit::workspace_default(&root);
    if !toolkit.python_bin().exists() {
        eprintln!(
            "skipping live RDKit runtime test because `{}` does not exist",
            toolkit.python_bin().display()
        );
        return;
    }

    let status = toolkit.status().expect("runtime status");
    assert_eq!(status.runtime, "patina-stk-python");
    assert!(!status.rdkit.trim().is_empty());

    let canonical = toolkit
        .canonicalize_smiles("NCCN")
        .expect("canonicalize smiles");
    assert_eq!(canonical, "NCCN");

    let nitrogen_pattern = amine_coordination_donor_pattern("amine_donor");
    let matches = toolkit
        .detect_functional_groups(&canonical, std::slice::from_ref(&nitrogen_pattern))
        .expect("detect functional groups");
    assert_eq!(matches.len(), 2);
    assert!(matches
        .iter()
        .all(|matched| matched.bond_intent == FunctionalGroupBondIntent::Coordination));

    let block = prepare_coordination_donor_block(
        &toolkit,
        "ethylenediamine",
        &canonical,
        7,
        &[nitrogen_pattern],
    )
    .expect("prepare building block");
    assert_eq!(block.label(), "ethylenediamine");
    assert_eq!(block.functional_group_count(), 2);
    assert_eq!(block.local_atom_positions().len(), 4);
    assert_eq!(
        block.local_functional_group_bond_intents(),
        &[
            FunctionalGroupBondIntent::Coordination,
            FunctionalGroupBondIntent::Coordination
        ]
    );
    assert_eq!(
        block.local_functional_group_bonder_atom_ids(),
        &[vec![0], vec![3]]
    );
    assert_eq!(block.placer_atom_ids(), &[0, 3]);
}

#[test]
fn rdkit_prepared_building_blocks_construct_zero_d_topology() {
    let root = workspace_root();
    let toolkit = PythonRdkitToolkit::workspace_default(&root);
    if !toolkit.python_bin().exists() {
        eprintln!(
            "skipping RDKit-prepared construction test because `{}` does not exist",
            toolkit.python_bin().display()
        );
        return;
    }

    let nitrogen_pattern = amine_coordination_donor_pattern("amine_donor");
    let terminal = prepare_coordination_donor_block(
        &toolkit,
        "ammonia_terminal",
        "N",
        11,
        std::slice::from_ref(&nitrogen_pattern),
    )
    .expect("prepare terminal");
    let linker = prepare_coordination_donor_block(
        &toolkit,
        "ethylenediamine_linker",
        "NCCN",
        13,
        &[nitrogen_pattern],
    )
    .expect("prepare linker");

    let plan = ZeroDTopologyBuilder::new()
        .add_vertex(TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(
            TopologyVertex::new(1, [1.5, 0.0, 0.0])
                .with_kind(VertexKind::Linear)
                .with_aligner_edge(0),
        )
        .add_vertex(TopologyVertex::new(2, [3.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_edge(TopologyEdge::new(0, [0, 1]))
        .add_edge(TopologyEdge::new(1, [1, 2]))
        .assign_building_block(terminal, vec![0, 2])
        .assign_building_block(linker, vec![1])
        .add_stage(vec![0, 2])
        .add_stage(vec![1])
        .build();

    let result = plan.construct_with_default_driver().expect("construct");
    assert_eq!(result.molecule_state().num_placements(), 3);
    assert_eq!(result.bonds().len(), 2);
    assert!(result.deleted_atom_ids().is_empty());
    assert_eq!(
        result
            .bond_counts_by_rule()
            .get(&BondDefinitionRule::DativeSharedEdge),
        Some(&2)
    );
}
