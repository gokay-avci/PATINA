/*!
Construction molecule-state landing zone.
*/

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bonding::definition::ConstructedBond;
use crate::building_block::BuildingBlockRecord;
use crate::construction::placement::PlacementResult;
use crate::functional_group::FunctionalGroupBondIntent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementInstance {
    vertex_id: usize,
    building_block: BuildingBlockRecord,
    atom_start: usize,
    atom_count: usize,
    functional_group_edges: BTreeMap<usize, usize>,
    functional_group_atom_ids: BTreeMap<usize, usize>,
    functional_group_bonder_atom_ids: BTreeMap<usize, Vec<usize>>,
    functional_group_deleter_atom_ids: BTreeMap<usize, Vec<usize>>,
    functional_group_bond_intents: BTreeMap<usize, FunctionalGroupBondIntent>,
}

impl PlacementInstance {
    pub fn vertex_id(&self) -> usize {
        self.vertex_id
    }

    pub fn building_block(&self) -> &BuildingBlockRecord {
        &self.building_block
    }

    pub fn atom_start(&self) -> usize {
        self.atom_start
    }

    pub fn atom_count(&self) -> usize {
        self.atom_count
    }

    pub fn functional_group_edges(&self) -> &BTreeMap<usize, usize> {
        &self.functional_group_edges
    }

    pub fn functional_group_atom_ids(&self) -> &BTreeMap<usize, usize> {
        &self.functional_group_atom_ids
    }

    pub fn functional_group_bonder_atom_ids(&self) -> &BTreeMap<usize, Vec<usize>> {
        &self.functional_group_bonder_atom_ids
    }

    pub fn functional_group_deleter_atom_ids(&self) -> &BTreeMap<usize, Vec<usize>> {
        &self.functional_group_deleter_atom_ids
    }

    pub fn functional_group_bond_intents(&self) -> &BTreeMap<usize, FunctionalGroupBondIntent> {
        &self.functional_group_bond_intents
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConstructionMoleculeState {
    position_matrix: Vec<[f64; 3]>,
    edge_functional_groups: BTreeMap<usize, Vec<usize>>,
    placement_instances: Vec<PlacementInstance>,
    bonds: Vec<ConstructedBond>,
    deleted_atom_ids: Vec<usize>,
    num_placements: usize,
}

impl ConstructionMoleculeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_placement_results(
        &self,
        stage_vertex_ids: &[usize],
        stage_building_blocks: &[BuildingBlockRecord],
        results: &[PlacementResult],
    ) -> Self {
        let mut clone = self.clone();
        clone.apply_placement_results(stage_vertex_ids, stage_building_blocks, results);
        clone
    }

    pub fn apply_placement_results(
        &mut self,
        stage_vertex_ids: &[usize],
        stage_building_blocks: &[BuildingBlockRecord],
        results: &[PlacementResult],
    ) {
        for ((&vertex_id, building_block), result) in stage_vertex_ids
            .iter()
            .zip(stage_building_blocks.iter())
            .zip(results.iter())
        {
            let atom_start = self.position_matrix.len();
            let atom_count = result.position_matrix().len();
            self.position_matrix
                .extend(result.position_matrix().iter().copied());
            for (&functional_group_id, &edge_id) in result.functional_group_edges() {
                self.edge_functional_groups
                    .entry(edge_id)
                    .or_default()
                    .push(functional_group_id);
            }
            self.placement_instances.push(PlacementInstance {
                vertex_id,
                building_block: building_block.clone(),
                atom_start,
                atom_count,
                functional_group_edges: result.functional_group_edges().clone(),
                functional_group_atom_ids: result.functional_group_atom_ids().clone(),
                functional_group_bonder_atom_ids: result.functional_group_bonder_atom_ids().clone(),
                functional_group_deleter_atom_ids: result
                    .functional_group_deleter_atom_ids()
                    .clone(),
                functional_group_bond_intents: result.functional_group_bond_intents().clone(),
            });
        }
        self.num_placements += results.len();
    }

    pub fn with_bonds(&self, bonds: Vec<ConstructedBond>) -> Self {
        let mut clone = self.clone();
        clone.bonds = bonds;
        clone
    }

    pub fn with_deleted_atom_ids(&self, deleted_atom_ids: Vec<usize>) -> Self {
        let mut clone = self.clone();
        clone.deleted_atom_ids = deleted_atom_ids;
        clone
    }

    pub fn position_matrix(&self) -> &[[f64; 3]] {
        &self.position_matrix
    }

    pub fn num_placements(&self) -> usize {
        self.num_placements
    }

    pub fn edge_functional_groups(&self) -> &BTreeMap<usize, Vec<usize>> {
        &self.edge_functional_groups
    }

    pub fn placement_instances(&self) -> &[PlacementInstance] {
        &self.placement_instances
    }

    pub fn bonds(&self) -> &[ConstructedBond] {
        &self.bonds
    }

    pub fn deleted_atom_ids(&self) -> &[usize] {
        &self.deleted_atom_ids
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::ConstructionMoleculeState;
    use crate::construction::placement::PlacementResult;

    #[test]
    fn molecule_state_accumulates_position_rows_and_edge_groups() {
        let mut fg_map = BTreeMap::new();
        fg_map.insert(0, 10);
        fg_map.insert(1, 10);
        let atom_map = BTreeMap::from([(0usize, 0usize), (1usize, 1usize)]);
        let result = PlacementResult::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], fg_map, atom_map);

        let state = ConstructionMoleculeState::new().with_placement_results(
            &[5],
            &[crate::building_block::BuildingBlockRecord::new("bb")],
            &[result],
        );
        assert_eq!(state.position_matrix().len(), 2);
        assert_eq!(state.num_placements(), 1);
        assert_eq!(state.edge_functional_groups().get(&10), Some(&vec![0, 1]));
        assert_eq!(state.placement_instances()[0].vertex_id(), 5);
    }
}
