/*!
Topology graph landing zone.
*/

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::domain::StkDomainError;

use super::periodic::lattice_shift_cartesian;
use super::{edge::TopologyEdge, edge_group::EdgeGroup, vertex::TopologyVertex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TopologyGraphRecord {
    pub lattice: Option<[[f64; 3]; 3]>,
    pub vertices: Vec<TopologyVertex>,
    pub edges: Vec<TopologyEdge>,
    pub edge_groups: Vec<EdgeGroup>,
}

impl TopologyGraphRecord {
    pub fn new(
        vertices: Vec<TopologyVertex>,
        edges: Vec<TopologyEdge>,
        edge_groups: Vec<EdgeGroup>,
    ) -> Self {
        Self {
            lattice: None,
            vertices,
            edges,
            edge_groups,
        }
    }

    pub fn with_lattice(mut self, lattice: [[f64; 3]; 3]) -> Self {
        self.lattice = Some(lattice);
        self
    }

    pub fn validate(&self) -> Result<(), StkDomainError> {
        let mut vertex_ids = HashSet::new();
        for vertex in &self.vertices {
            if !vertex_ids.insert(vertex.id()) {
                return Err(StkDomainError::DuplicateVertexId { id: vertex.id() });
            }
        }

        let mut edge_ids = HashSet::new();
        for edge in &self.edges {
            if !edge_ids.insert(edge.id()) {
                return Err(StkDomainError::DuplicateEdgeId { id: edge.id() });
            }
            for vertex_id in edge.vertex_ids() {
                if !vertex_ids.contains(&vertex_id) {
                    return Err(StkDomainError::MissingVertexReference {
                        edge_id: edge.id(),
                        vertex_id,
                    });
                }
            }
            if edge.is_periodic() && self.lattice.is_none() {
                return Err(StkDomainError::MissingLatticeForPeriodicEdge { edge_id: edge.id() });
            }
        }

        let known_edges = self
            .edges
            .iter()
            .map(TopologyEdge::id)
            .collect::<HashSet<_>>();
        let mut covered_edges = HashSet::new();
        for edge_group in &self.edge_groups {
            let mut local = HashSet::new();
            for &edge_id in edge_group.edge_ids() {
                if !known_edges.contains(&edge_id) {
                    return Err(StkDomainError::MissingEdgeGroupEdge { edge_id });
                }
                if !local.insert(edge_id) {
                    return Err(StkDomainError::DuplicateEdgeGroupEdge { edge_id });
                }
                covered_edges.insert(edge_id);
            }
        }
        if covered_edges.len() != known_edges.len() {
            return Err(StkDomainError::MissingEdgeGroupCoverage);
        }

        Ok(())
    }

    pub fn vertex(&self, vertex_id: usize) -> Result<&TopologyVertex, StkDomainError> {
        self.vertices
            .iter()
            .find(|vertex| vertex.id() == vertex_id)
            .ok_or(StkDomainError::VertexNotFound { vertex_id })
    }

    pub fn edge(&self, edge_id: usize) -> Result<&TopologyEdge, StkDomainError> {
        self.edges
            .iter()
            .find(|edge| edge.id() == edge_id)
            .ok_or(StkDomainError::EdgeNotFound { edge_id })
    }

    pub fn vertex_edges(&self, vertex_id: usize) -> Result<Vec<&TopologyEdge>, StkDomainError> {
        self.vertex(vertex_id)?;
        Ok(self
            .edges
            .iter()
            .filter(|edge| edge.vertex_ids().contains(&vertex_id))
            .collect())
    }

    pub fn edge_position(&self, edge_id: usize) -> Result<[f64; 3], StkDomainError> {
        let edge = self.edge(edge_id)?;
        let [left_id, right_id] = edge.vertex_ids();
        let left = self.vertex(left_id)?.position();
        let right = self.vertex(right_id)?.position();
        Ok(edge
            .position_override()
            .unwrap_or_else(|| midpoint(left, right)))
    }

    pub fn edge_position_for_vertex(
        &self,
        edge_id: usize,
        reference_vertex_id: usize,
    ) -> Result<[f64; 3], StkDomainError> {
        let edge = self.edge(edge_id)?;
        let [id1, id2] = edge.vertex_ids();
        if reference_vertex_id != id1 && reference_vertex_id != id2 {
            return Err(StkDomainError::MissingVertexReference {
                edge_id,
                vertex_id: reference_vertex_id,
            });
        }
        if !edge.is_periodic() {
            return self.edge_position(edge_id);
        }

        let lattice = self
            .lattice
            .ok_or(StkDomainError::MissingLatticeForPeriodicEdge { edge_id })?;
        let reference = self.vertex(reference_vertex_id)?;
        let other = self.vertex(if reference_vertex_id == id1 { id2 } else { id1 })?;
        let direction = if reference_vertex_id == id1 { 1 } else { -1 };
        let periodicity = edge.periodic_shift();
        let end_cell = add_i32(reference.cell(), scale_i32(periodicity, direction));
        let cell_shift = sub_i32(end_cell, other.cell());
        let other_shifted = add_f64(
            other.position(),
            lattice_shift_cartesian(lattice, cell_shift),
        );
        Ok(midpoint(reference.position(), other_shifted))
    }

    pub fn edge_groups_by_edge_id(&self) -> HashMap<usize, usize> {
        let mut mapping = HashMap::new();
        for (group_index, edge_group) in self.edge_groups.iter().enumerate() {
            for &edge_id in edge_group.edge_ids() {
                mapping.insert(edge_id, group_index);
            }
        }
        mapping
    }
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        (left[0] + right[0]) / 2.0,
        (left[1] + right[1]) / 2.0,
        (left[2] + right[2]) / 2.0,
    ]
}

fn add_f64(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn add_i32(left: [i32; 3], right: [i32; 3]) -> [i32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub_i32(left: [i32; 3], right: [i32; 3]) -> [i32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale_i32(value: [i32; 3], scale: i32) -> [i32; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{edge_group::EdgeGroup, vertex::TopologyVertex};

    fn base_graph() -> TopologyGraphRecord {
        TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_cell([0, 0, 0]),
                TopologyVertex::new(1, [4.0, 0.0, 0.0]).with_cell([0, 0, 0]),
            ],
            vec![TopologyEdge::new(0, [0, 1])],
            vec![EdgeGroup::from_edge_id(0)],
        )
    }

    #[test]
    fn graph_validation_rejects_missing_edge_group_coverage() {
        let graph = TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                TopologyVertex::new(1, [1.0, 0.0, 0.0]),
            ],
            vec![TopologyEdge::new(0, [0, 1])],
            vec![],
        );

        assert!(matches!(
            graph.validate(),
            Err(StkDomainError::MissingEdgeGroupCoverage)
        ));
    }

    #[test]
    fn nonperiodic_edge_position_is_midpoint() {
        let graph = base_graph();
        assert_eq!(graph.edge_position(0).unwrap(), [2.0, 0.0, 0.0]);
        assert_eq!(
            graph.edge_position_for_vertex(0, 0).unwrap(),
            [2.0, 0.0, 0.0]
        );
        assert_eq!(
            graph.edge_position_for_vertex(0, 1).unwrap(),
            [2.0, 0.0, 0.0]
        );
    }

    #[test]
    fn periodic_edge_position_depends_on_reference_vertex() {
        let graph = TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_cell([0, 0, 0]),
                TopologyVertex::new(1, [1.0, 0.0, 0.0]).with_cell([0, 0, 0]),
            ],
            vec![TopologyEdge::new(0, [0, 1]).with_periodic_shift([1, 0, 0])],
            vec![EdgeGroup::from_edge_id(0)],
        )
        .with_lattice([[10.0, 0.0, 0.0], [0.0, 9.0, 0.0], [0.0, 0.0, 8.0]]);

        assert_eq!(graph.validate(), Ok(()));
        assert_eq!(
            graph.edge_position_for_vertex(0, 0).unwrap(),
            [5.5, 0.0, 0.0]
        );
        assert_eq!(
            graph.edge_position_for_vertex(0, 1).unwrap(),
            [-4.5, 0.0, 0.0]
        );
    }

    #[test]
    fn periodic_edge_position_respects_vertex_cells() {
        let graph = TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_cell([0, 0, 0]),
                TopologyVertex::new(1, [1.0, 0.0, 0.0]).with_cell([1, 0, 0]),
            ],
            vec![TopologyEdge::new(0, [0, 1]).with_periodic_shift([0, 0, 0])],
            vec![EdgeGroup::from_edge_id(0)],
        );

        assert_eq!(
            graph.edge_position_for_vertex(0, 0).unwrap(),
            [0.5, 0.0, 0.0]
        );
    }

    #[test]
    fn edge_groups_map_back_to_group_index() {
        let graph = TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                TopologyVertex::new(1, [1.0, 0.0, 0.0]),
                TopologyVertex::new(2, [2.0, 0.0, 0.0]),
            ],
            vec![TopologyEdge::new(0, [0, 1]), TopologyEdge::new(1, [1, 2])],
            vec![EdgeGroup::new(vec![0, 1])],
        );
        let mapping = graph.edge_groups_by_edge_id();
        assert_eq!(mapping.get(&0), Some(&0));
        assert_eq!(mapping.get(&1), Some(&0));
    }
}
