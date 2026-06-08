/*!
Topology vertex landing zone.
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VertexKind {
    Unaligning,
    Linear,
    NonLinear,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyVertex {
    id: usize,
    position: [f64; 3],
    cell: [i32; 3],
    aligner_edge: Option<usize>,
    kind: VertexKind,
}

impl TopologyVertex {
    pub fn new(id: usize, position: [f64; 3]) -> Self {
        Self {
            id,
            position,
            cell: [0, 0, 0],
            aligner_edge: None,
            kind: VertexKind::Unaligning,
        }
    }

    pub fn with_cell(mut self, cell: [i32; 3]) -> Self {
        self.cell = cell;
        self
    }

    pub fn with_aligner_edge(mut self, aligner_edge: usize) -> Self {
        self.aligner_edge = Some(aligner_edge);
        self
    }

    pub fn with_kind(mut self, kind: VertexKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn position(&self) -> [f64; 3] {
        self.position
    }

    pub fn cell(&self) -> [i32; 3] {
        self.cell
    }

    pub fn aligner_edge(&self) -> Option<usize> {
        self.aligner_edge
    }

    pub fn kind(&self) -> VertexKind {
        self.kind
    }
}
