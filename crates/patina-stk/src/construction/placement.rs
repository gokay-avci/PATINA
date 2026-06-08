/*!
Construction placement-result landing zone.
*/

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::functional_group::FunctionalGroupBondIntent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementResult {
    position_matrix: Vec<[f64; 3]>,
    functional_group_edges: BTreeMap<usize, usize>,
    functional_group_atom_ids: BTreeMap<usize, usize>,
    functional_group_bonder_atom_ids: BTreeMap<usize, Vec<usize>>,
    functional_group_deleter_atom_ids: BTreeMap<usize, Vec<usize>>,
    functional_group_bond_intents: BTreeMap<usize, FunctionalGroupBondIntent>,
}

impl PlacementResult {
    pub fn new(
        position_matrix: Vec<[f64; 3]>,
        functional_group_edges: BTreeMap<usize, usize>,
        functional_group_atom_ids: BTreeMap<usize, usize>,
    ) -> Self {
        Self {
            position_matrix,
            functional_group_edges,
            functional_group_bonder_atom_ids: functional_group_atom_ids
                .iter()
                .map(|(&fg_id, &atom_id)| (fg_id, vec![atom_id]))
                .collect(),
            functional_group_deleter_atom_ids: BTreeMap::new(),
            functional_group_bond_intents: functional_group_atom_ids
                .keys()
                .map(|&fg_id| (fg_id, FunctionalGroupBondIntent::Covalent))
                .collect(),
            functional_group_atom_ids,
        }
    }

    pub fn with_functional_group_metadata(
        mut self,
        functional_group_bonder_atom_ids: BTreeMap<usize, Vec<usize>>,
        functional_group_deleter_atom_ids: BTreeMap<usize, Vec<usize>>,
        functional_group_bond_intents: BTreeMap<usize, FunctionalGroupBondIntent>,
    ) -> Self {
        self.functional_group_atom_ids = functional_group_bonder_atom_ids
            .iter()
            .filter_map(|(&fg_id, atom_ids)| {
                atom_ids.first().copied().map(|atom_id| (fg_id, atom_id))
            })
            .collect();
        self.functional_group_bonder_atom_ids = functional_group_bonder_atom_ids;
        self.functional_group_deleter_atom_ids = functional_group_deleter_atom_ids;
        self.functional_group_bond_intents = functional_group_bond_intents;
        self
    }

    pub fn position_matrix(&self) -> &[[f64; 3]] {
        &self.position_matrix
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
