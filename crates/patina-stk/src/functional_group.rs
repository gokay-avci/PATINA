/*!
Functional-group records and matching semantics.

Phase 1 will add bonder, deleter, and placer atom roles here.
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionalGroupBondIntent {
    Covalent,
    Coordination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalGroupRecord {
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalGroupPattern {
    pub kind: String,
    pub smarts: String,
    pub bond_intent: FunctionalGroupBondIntent,
    pub bonder_indices: Vec<usize>,
    pub deleter_indices: Vec<usize>,
    pub placer_indices: Vec<usize>,
}

impl FunctionalGroupPattern {
    pub fn new(kind: impl Into<String>, smarts: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            smarts: smarts.into(),
            bond_intent: FunctionalGroupBondIntent::Covalent,
            bonder_indices: Vec::new(),
            deleter_indices: Vec::new(),
            placer_indices: Vec::new(),
        }
    }

    pub fn with_bond_intent(mut self, bond_intent: FunctionalGroupBondIntent) -> Self {
        self.bond_intent = bond_intent;
        self
    }

    pub fn with_bonder_indices(mut self, bonder_indices: Vec<usize>) -> Self {
        self.bonder_indices = bonder_indices;
        self
    }

    pub fn with_deleter_indices(mut self, deleter_indices: Vec<usize>) -> Self {
        self.deleter_indices = deleter_indices;
        self
    }

    pub fn with_placer_indices(mut self, placer_indices: Vec<usize>) -> Self {
        self.placer_indices = placer_indices;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalGroupMatch {
    pub kind: String,
    pub bond_intent: FunctionalGroupBondIntent,
    pub atom_ids: Vec<usize>,
    pub bonder_atom_ids: Vec<usize>,
    pub deleter_atom_ids: Vec<usize>,
    pub placer_atom_ids: Vec<usize>,
}
