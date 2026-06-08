/*!
Internal bond-application helpers landing zone.
*/

use std::collections::BTreeMap;

use crate::bonding::definition::{BondDefinitionRule, ConstructedBond};
use crate::construction::state::ConstructionState;
use crate::functional_group::FunctionalGroupBondIntent;

fn infer_bond_rule(
    left_bond_intent: FunctionalGroupBondIntent,
    right_bond_intent: FunctionalGroupBondIntent,
    left_bonder_atom_ids: &[usize],
    right_bonder_atom_ids: &[usize],
    left_deleter_atom_ids: &[usize],
    right_deleter_atom_ids: &[usize],
) -> BondDefinitionRule {
    if matches!(left_bond_intent, FunctionalGroupBondIntent::Coordination)
        || matches!(right_bond_intent, FunctionalGroupBondIntent::Coordination)
    {
        return BondDefinitionRule::DativeSharedEdge;
    }
    if left_deleter_atom_ids.is_empty() && right_deleter_atom_ids.is_empty() {
        return BondDefinitionRule::DativeSharedEdge;
    }
    if left_bonder_atom_ids.len() > 1 || right_bonder_atom_ids.len() > 1 {
        return BondDefinitionRule::CovalentMultiBonder;
    }
    BondDefinitionRule::CovalentSingleBonder
}

pub fn infer_bonds_from_shared_edges(state: &ConstructionState) -> Vec<ConstructedBond> {
    let mut per_edge = BTreeMap::<usize, Vec<usize>>::new();
    for (placement_index, placement) in state
        .molecule_state()
        .placement_instances()
        .iter()
        .enumerate()
    {
        for &edge_id in placement.functional_group_edges().values() {
            per_edge.entry(edge_id).or_default().push(placement_index);
        }
    }

    let mut bonds = Vec::new();
    for (edge_id, placement_indices) in per_edge {
        if placement_indices.len() != 2 {
            continue;
        }
        let left = &state.molecule_state().placement_instances()[placement_indices[0]];
        let right = &state.molecule_state().placement_instances()[placement_indices[1]];
        let left_fg = left
            .functional_group_edges()
            .iter()
            .find_map(|(&fg_id, &mapped_edge)| (mapped_edge == edge_id).then_some(fg_id));
        let right_fg = right
            .functional_group_edges()
            .iter()
            .find_map(|(&fg_id, &mapped_edge)| (mapped_edge == edge_id).then_some(fg_id));
        let (Some(left_fg), Some(right_fg)) = (left_fg, right_fg) else {
            continue;
        };
        let left_local_atoms = left
            .functional_group_bonder_atom_ids()
            .get(&left_fg)
            .filter(|atom_ids| !atom_ids.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                left.functional_group_atom_ids()
                    .get(&left_fg)
                    .map(|&atom_id| vec![atom_id])
                    .unwrap_or_default()
            });
        let right_local_atoms = right
            .functional_group_bonder_atom_ids()
            .get(&right_fg)
            .filter(|atom_ids| !atom_ids.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                right
                    .functional_group_atom_ids()
                    .get(&right_fg)
                    .map(|&atom_id| vec![atom_id])
                    .unwrap_or_default()
            });
        let left_deleter_atom_ids = left
            .functional_group_deleter_atom_ids()
            .get(&left_fg)
            .cloned()
            .unwrap_or_default();
        let right_deleter_atom_ids = right
            .functional_group_deleter_atom_ids()
            .get(&right_fg)
            .cloned()
            .unwrap_or_default();
        let left_bond_intent = left
            .functional_group_bond_intents()
            .get(&left_fg)
            .copied()
            .unwrap_or(FunctionalGroupBondIntent::Covalent);
        let right_bond_intent = right
            .functional_group_bond_intents()
            .get(&right_fg)
            .copied()
            .unwrap_or(FunctionalGroupBondIntent::Covalent);
        let rule = infer_bond_rule(
            left_bond_intent,
            right_bond_intent,
            &left_local_atoms,
            &right_local_atoms,
            &left_deleter_atom_ids,
            &right_deleter_atom_ids,
        );
        for (&left_local_atom, &right_local_atom) in
            left_local_atoms.iter().zip(right_local_atoms.iter())
        {
            bonds.push(ConstructedBond {
                atom_ids: (
                    left.atom_start() + left_local_atom,
                    right.atom_start() + right_local_atom,
                ),
                edge_id,
                rule,
            });
        }
    }
    bonds
}

pub fn infer_deleted_atoms_from_shared_edges(state: &ConstructionState) -> Vec<usize> {
    let mut deleted = Vec::new();
    for placement in state.molecule_state().placement_instances() {
        for atom_ids in placement.functional_group_deleter_atom_ids().values() {
            deleted.extend(
                atom_ids
                    .iter()
                    .map(|&atom_id| placement.atom_start() + atom_id),
            );
        }
    }
    deleted.sort_unstable();
    deleted.dedup();
    deleted
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::bonding::definition::BondDefinitionRule;
    use crate::bonding::internal::{
        infer_bonds_from_shared_edges, infer_deleted_atoms_from_shared_edges,
    };
    use crate::building_block::BuildingBlockRecord;
    use crate::construction::{
        graph_state::{BuildingBlockPlacement, ConstructionGraphState},
        molecule_state::ConstructionMoleculeState,
        placement::PlacementResult,
        state::ConstructionState,
    };
    use crate::functional_group::FunctionalGroupBondIntent;
    use crate::topology::{
        edge::TopologyEdge, edge_group::EdgeGroup, graph::TopologyGraphRecord,
        vertex::TopologyVertex,
    };

    #[test]
    fn infers_one_bond_for_two_placements_sharing_an_edge() {
        let graph = ConstructionGraphState::new(
            TopologyGraphRecord::new(
                vec![
                    TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                    TopologyVertex::new(1, [1.0, 0.0, 0.0]),
                ],
                vec![TopologyEdge::new(0, [0, 1])],
                vec![EdgeGroup::from_edge_id(0)],
            ),
            vec![
                BuildingBlockPlacement::new(BuildingBlockRecord::new("a"), vec![0]),
                BuildingBlockPlacement::new(BuildingBlockRecord::new("b"), vec![1]),
            ],
        )
        .unwrap();

        let left_edges = BTreeMap::from([(0usize, 0usize)]);
        let left_atoms = BTreeMap::from([(0usize, 0usize)]);
        let right_edges = BTreeMap::from([(0usize, 0usize)]);
        let right_atoms = BTreeMap::from([(0usize, 0usize)]);
        let molecule = ConstructionMoleculeState::new().with_placement_results(
            &[0, 1],
            &[BuildingBlockRecord::new("a"), BuildingBlockRecord::new("b")],
            &[
                PlacementResult::new(vec![[0.0, 0.0, 0.0]], left_edges, left_atoms),
                PlacementResult::new(vec![[1.0, 0.0, 0.0]], right_edges, right_atoms),
            ],
        );
        let state = ConstructionState::new(graph, molecule);
        let bonds = infer_bonds_from_shared_edges(&state);
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].atom_ids, (0, 1));
        assert_eq!(bonds[0].rule, BondDefinitionRule::DativeSharedEdge);
    }

    #[test]
    fn infers_multiple_bonds_and_deletions_for_multi_bonder_groups() {
        let graph = ConstructionGraphState::new(
            TopologyGraphRecord::new(
                vec![
                    TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                    TopologyVertex::new(1, [1.0, 0.0, 0.0]),
                ],
                vec![TopologyEdge::new(0, [0, 1])],
                vec![EdgeGroup::from_edge_id(0)],
            ),
            vec![
                BuildingBlockPlacement::new(BuildingBlockRecord::new("a"), vec![0]),
                BuildingBlockPlacement::new(BuildingBlockRecord::new("b"), vec![1]),
            ],
        )
        .unwrap();

        let left = PlacementResult::new(
            vec![[0.0, 0.0, 0.0], [0.2, 0.0, 0.0], [0.4, 0.0, 0.0]],
            BTreeMap::from([(0usize, 0usize)]),
            BTreeMap::from([(0usize, 0usize)]),
        )
        .with_functional_group_metadata(
            BTreeMap::from([(0usize, vec![0usize, 1usize])]),
            BTreeMap::from([(0usize, vec![2usize])]),
            BTreeMap::from([(0usize, FunctionalGroupBondIntent::Covalent)]),
        );
        let right = PlacementResult::new(
            vec![[1.0, 0.0, 0.0], [1.2, 0.0, 0.0], [1.4, 0.0, 0.0]],
            BTreeMap::from([(0usize, 0usize)]),
            BTreeMap::from([(0usize, 0usize)]),
        )
        .with_functional_group_metadata(
            BTreeMap::from([(0usize, vec![0usize, 1usize])]),
            BTreeMap::from([(0usize, vec![2usize])]),
            BTreeMap::from([(0usize, FunctionalGroupBondIntent::Covalent)]),
        );

        let molecule = ConstructionMoleculeState::new().with_placement_results(
            &[0, 1],
            &[BuildingBlockRecord::new("a"), BuildingBlockRecord::new("b")],
            &[left, right],
        );
        let state = ConstructionState::new(graph, molecule);

        let bonds = infer_bonds_from_shared_edges(&state);
        let deleted = infer_deleted_atoms_from_shared_edges(&state);
        assert_eq!(bonds.len(), 2);
        assert_eq!(bonds[0].atom_ids, (0, 3));
        assert_eq!(bonds[1].atom_ids, (1, 4));
        assert!(bonds
            .iter()
            .all(|bond| bond.rule == BondDefinitionRule::CovalentMultiBonder));
        assert_eq!(deleted, vec![2, 5]);
    }

    #[test]
    fn infers_single_bonder_covalent_rule_when_deleters_are_present() {
        let graph = ConstructionGraphState::new(
            TopologyGraphRecord::new(
                vec![
                    TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                    TopologyVertex::new(1, [1.0, 0.0, 0.0]),
                ],
                vec![TopologyEdge::new(0, [0, 1])],
                vec![EdgeGroup::from_edge_id(0)],
            ),
            vec![
                BuildingBlockPlacement::new(BuildingBlockRecord::new("amine"), vec![0]),
                BuildingBlockPlacement::new(BuildingBlockRecord::new("aldehyde"), vec![1]),
            ],
        )
        .unwrap();

        let left = PlacementResult::new(
            vec![[0.0, 0.0, 0.0], [0.2, 0.0, 0.0]],
            BTreeMap::from([(0usize, 0usize)]),
            BTreeMap::from([(0usize, 0usize)]),
        )
        .with_functional_group_metadata(
            BTreeMap::from([(0usize, vec![0usize])]),
            BTreeMap::from([(0usize, vec![1usize])]),
            BTreeMap::from([(0usize, FunctionalGroupBondIntent::Covalent)]),
        );
        let right = PlacementResult::new(
            vec![[1.0, 0.0, 0.0], [1.2, 0.0, 0.0]],
            BTreeMap::from([(0usize, 0usize)]),
            BTreeMap::from([(0usize, 0usize)]),
        )
        .with_functional_group_metadata(
            BTreeMap::from([(0usize, vec![0usize])]),
            BTreeMap::from([(0usize, vec![1usize])]),
            BTreeMap::from([(0usize, FunctionalGroupBondIntent::Covalent)]),
        );

        let molecule = ConstructionMoleculeState::new().with_placement_results(
            &[0, 1],
            &[
                BuildingBlockRecord::new("amine"),
                BuildingBlockRecord::new("aldehyde"),
            ],
            &[left, right],
        );
        let state = ConstructionState::new(graph, molecule);

        let bonds = infer_bonds_from_shared_edges(&state);
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].rule, BondDefinitionRule::CovalentSingleBonder);
    }

    #[test]
    fn explicit_coordination_intent_overrides_deleter_heuristic() {
        let graph = ConstructionGraphState::new(
            TopologyGraphRecord::new(
                vec![
                    TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                    TopologyVertex::new(1, [1.0, 0.0, 0.0]),
                ],
                vec![TopologyEdge::new(0, [0, 1])],
                vec![EdgeGroup::from_edge_id(0)],
            ),
            vec![
                BuildingBlockPlacement::new(BuildingBlockRecord::new("metal"), vec![0]),
                BuildingBlockPlacement::new(BuildingBlockRecord::new("ligand"), vec![1]),
            ],
        )
        .unwrap();

        let left = PlacementResult::new(
            vec![[0.0, 0.0, 0.0], [0.2, 0.0, 0.0]],
            BTreeMap::from([(0usize, 0usize)]),
            BTreeMap::from([(0usize, 0usize)]),
        )
        .with_functional_group_metadata(
            BTreeMap::from([(0usize, vec![0usize])]),
            BTreeMap::from([(0usize, vec![1usize])]),
            BTreeMap::from([(0usize, FunctionalGroupBondIntent::Coordination)]),
        );
        let right = PlacementResult::new(
            vec![[1.0, 0.0, 0.0], [1.2, 0.0, 0.0]],
            BTreeMap::from([(0usize, 0usize)]),
            BTreeMap::from([(0usize, 0usize)]),
        )
        .with_functional_group_metadata(
            BTreeMap::from([(0usize, vec![0usize])]),
            BTreeMap::from([(0usize, vec![1usize])]),
            BTreeMap::from([(0usize, FunctionalGroupBondIntent::Coordination)]),
        );

        let molecule = ConstructionMoleculeState::new().with_placement_results(
            &[0, 1],
            &[
                BuildingBlockRecord::new("metal"),
                BuildingBlockRecord::new("ligand"),
            ],
            &[left, right],
        );
        let state = ConstructionState::new(graph, molecule);

        let bonds = infer_bonds_from_shared_edges(&state);
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].rule, BondDefinitionRule::DativeSharedEdge);
    }
}
