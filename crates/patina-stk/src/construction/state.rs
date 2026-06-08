/*!
Combined construction state.
*/

use serde::{Deserialize, Serialize};

use super::{graph_state::ConstructionGraphState, molecule_state::ConstructionMoleculeState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructionState {
    graph_state: ConstructionGraphState,
    molecule_state: ConstructionMoleculeState,
}

impl ConstructionState {
    pub fn new(
        graph_state: ConstructionGraphState,
        molecule_state: ConstructionMoleculeState,
    ) -> Self {
        Self {
            graph_state,
            molecule_state,
        }
    }

    pub fn graph_state(&self) -> &ConstructionGraphState {
        &self.graph_state
    }

    pub fn molecule_state(&self) -> &ConstructionMoleculeState {
        &self.molecule_state
    }
}
