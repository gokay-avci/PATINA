/*!
Topology edge landing zone.
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyEdge {
    id: usize,
    vertex_ids: [usize; 2],
    periodic_shift: [i32; 3],
    parent_id: usize,
    position: Option<[f64; 3]>,
}

impl TopologyEdge {
    pub fn new(id: usize, vertex_ids: [usize; 2]) -> Self {
        Self {
            id,
            vertex_ids,
            periodic_shift: [0, 0, 0],
            parent_id: id,
            position: None,
        }
    }

    pub fn with_periodic_shift(mut self, periodic_shift: [i32; 3]) -> Self {
        self.periodic_shift = periodic_shift;
        self
    }

    pub fn with_parent_id(mut self, parent_id: usize) -> Self {
        self.parent_id = parent_id;
        self
    }

    pub fn with_position(mut self, position: [f64; 3]) -> Self {
        self.position = Some(position);
        self
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn vertex_ids(&self) -> [usize; 2] {
        self.vertex_ids
    }

    pub fn periodic_shift(&self) -> [i32; 3] {
        self.periodic_shift
    }

    pub fn parent_id(&self) -> usize {
        self.parent_id
    }

    pub fn position_override(&self) -> Option<[f64; 3]> {
        self.position
    }

    pub fn is_periodic(&self) -> bool {
        self.periodic_shift.iter().any(|shift| *shift != 0)
    }
}
