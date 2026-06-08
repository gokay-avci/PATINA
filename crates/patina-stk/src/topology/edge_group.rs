/*!
Edge-group landing zone.
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeGroup {
    edge_ids: Vec<usize>,
}

impl EdgeGroup {
    pub fn new(edge_ids: Vec<usize>) -> Self {
        Self { edge_ids }
    }

    pub fn from_edge_id(edge_id: usize) -> Self {
        Self {
            edge_ids: vec![edge_id],
        }
    }

    pub fn edge_ids(&self) -> &[usize] {
        &self.edge_ids
    }
}
