/*!
Constructed-assembly result landing zone.
*/

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bonding::definition::{BondDefinitionRule, ConstructedBond};
use crate::construction::{
    graph_state::ConstructionGraphState, molecule_state::ConstructionMoleculeState,
    state::ConstructionState,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructionResult {
    state: ConstructionState,
}

impl ConstructionResult {
    pub fn new(
        graph_state: ConstructionGraphState,
        molecule_state: ConstructionMoleculeState,
    ) -> Self {
        Self {
            state: ConstructionState::new(graph_state, molecule_state),
        }
    }

    pub fn graph_state(&self) -> &ConstructionGraphState {
        self.state.graph_state()
    }

    pub fn molecule_state(&self) -> &ConstructionMoleculeState {
        self.state.molecule_state()
    }

    pub fn bonds(&self) -> &[ConstructedBond] {
        self.molecule_state().bonds()
    }

    pub fn deleted_atom_ids(&self) -> &[usize] {
        self.molecule_state().deleted_atom_ids()
    }

    pub fn bond_counts_by_rule(&self) -> BTreeMap<BondDefinitionRule, usize> {
        let mut counts = BTreeMap::new();
        for bond in self.bonds() {
            *counts.entry(bond.rule).or_insert(0) += 1;
        }
        counts
    }

    pub fn state(&self) -> &ConstructionState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::bonding::definition::{BondDefinitionRule, ConstructedBond};
    use crate::construction::{
        graph_state::ConstructionGraphState, molecule_state::ConstructionMoleculeState,
    };
    use crate::topology::{
        edge::TopologyEdge, edge_group::EdgeGroup, graph::TopologyGraphRecord,
        vertex::TopologyVertex,
    };

    use super::ConstructionResult;

    #[test]
    fn construction_result_summarizes_bonds_by_rule() {
        let graph = ConstructionGraphState::new(
            TopologyGraphRecord::new(
                vec![
                    TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                    TopologyVertex::new(1, [1.0, 0.0, 0.0]),
                ],
                vec![TopologyEdge::new(0, [0, 1])],
                vec![EdgeGroup::from_edge_id(0)],
            ),
            Vec::new(),
        )
        .unwrap();
        let molecule = ConstructionMoleculeState::new()
            .with_bonds(vec![
                ConstructedBond {
                    atom_ids: (0, 1),
                    edge_id: 0,
                    rule: BondDefinitionRule::CovalentSingleBonder,
                },
                ConstructedBond {
                    atom_ids: (2, 3),
                    edge_id: 1,
                    rule: BondDefinitionRule::CovalentSingleBonder,
                },
                ConstructedBond {
                    atom_ids: (4, 5),
                    edge_id: 2,
                    rule: BondDefinitionRule::DativeSharedEdge,
                },
            ])
            .with_deleted_atom_ids(vec![6, 7]);
        let result = ConstructionResult::new(graph, molecule);

        assert_eq!(result.deleted_atom_ids(), &[6, 7]);
        assert_eq!(
            result.bond_counts_by_rule(),
            BTreeMap::from([
                (BondDefinitionRule::CovalentSingleBonder, 2usize),
                (BondDefinitionRule::DativeSharedEdge, 1usize),
            ]),
        );
    }
}
