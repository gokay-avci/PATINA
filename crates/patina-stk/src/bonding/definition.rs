/*!
Built-in bond-definition rule landing zone.
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondDefinitionRule {
    CovalentSingleBonder,
    CovalentMultiBonder,
    DativeSharedEdge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructedBond {
    pub atom_ids: (usize, usize),
    pub edge_id: usize,
    pub rule: BondDefinitionRule,
}
