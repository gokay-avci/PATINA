/*!
Construction graph-state landing zone.
*/

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::building_block::BuildingBlockRecord;
use crate::domain::StkDomainError;
use crate::topology::{edge::TopologyEdge, graph::TopologyGraphRecord, vertex::TopologyVertex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingBlockPlacement {
    pub building_block: BuildingBlockRecord,
    pub vertex_ids: Vec<usize>,
}

impl BuildingBlockPlacement {
    pub fn new(building_block: BuildingBlockRecord, vertex_ids: Vec<usize>) -> Self {
        Self {
            building_block,
            vertex_ids,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructionGraphState {
    topology: TopologyGraphRecord,
    vertex_building_blocks: BTreeMap<usize, BuildingBlockRecord>,
    num_building_blocks: BTreeMap<BuildingBlockRecord, usize>,
}

impl ConstructionGraphState {
    pub fn new(
        topology: TopologyGraphRecord,
        placements: Vec<BuildingBlockPlacement>,
    ) -> Result<Self, StkDomainError> {
        topology.validate()?;

        let mut vertex_building_blocks = BTreeMap::new();
        let mut num_building_blocks = BTreeMap::new();

        for placement in placements {
            let count = placement.vertex_ids.len();
            for vertex_id in placement.vertex_ids {
                topology.vertex(vertex_id)?;
                if vertex_building_blocks
                    .insert(vertex_id, placement.building_block.clone())
                    .is_some()
                {
                    return Err(StkDomainError::DuplicateVertexId { id: vertex_id });
                }
            }
            *num_building_blocks
                .entry(placement.building_block)
                .or_insert(0) += count;
        }

        Ok(Self {
            topology,
            vertex_building_blocks,
            num_building_blocks,
        })
    }

    pub fn topology(&self) -> &TopologyGraphRecord {
        &self.topology
    }

    pub fn get_num_vertices(&self) -> usize {
        self.topology.vertices.len()
    }

    pub fn get_num_edges(&self) -> usize {
        self.topology.edges.len()
    }

    pub fn get_vertex(&self, vertex_id: usize) -> Result<&TopologyVertex, StkDomainError> {
        self.topology.vertex(vertex_id)
    }

    pub fn get_vertices<'a>(
        &'a self,
        vertex_ids: impl IntoIterator<Item = usize> + 'a,
    ) -> impl Iterator<Item = Result<&'a TopologyVertex, StkDomainError>> + 'a {
        vertex_ids
            .into_iter()
            .map(|vertex_id| self.get_vertex(vertex_id))
    }

    pub fn get_edge(&self, edge_id: usize) -> Result<&TopologyEdge, StkDomainError> {
        self.topology.edge(edge_id)
    }

    pub fn get_building_block(
        &self,
        vertex_id: usize,
    ) -> Result<&BuildingBlockRecord, StkDomainError> {
        self.vertex_building_blocks
            .get(&vertex_id)
            .ok_or(StkDomainError::VertexNotFound { vertex_id })
    }

    pub fn get_num_building_block(&self, building_block: &BuildingBlockRecord) -> usize {
        self.num_building_blocks
            .get(building_block)
            .copied()
            .unwrap_or(0)
    }

    pub fn get_edges_for_vertex(
        &self,
        vertex_id: usize,
    ) -> Result<Vec<TopologyEdge>, StkDomainError> {
        self.topology.vertex(vertex_id)?;
        let mut edges = Vec::new();
        for edge in &self.topology.edges {
            if edge.vertex_ids().contains(&vertex_id) {
                let position = self
                    .topology
                    .edge_position_for_vertex(edge.id(), vertex_id)?;
                edges.push(edge.clone().with_position(position));
            }
        }
        Ok(edges)
    }

    pub fn vertex_building_block_map(&self) -> &BTreeMap<usize, BuildingBlockRecord> {
        &self.vertex_building_blocks
    }

    pub fn building_block_counts(&self) -> &BTreeMap<BuildingBlockRecord, usize> {
        &self.num_building_blocks
    }

    pub fn edge_groups_by_edge_id(&self) -> HashMap<usize, usize> {
        self.topology.edge_groups_by_edge_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{edge::TopologyEdge, edge_group::EdgeGroup, vertex::TopologyVertex};

    fn periodic_graph() -> TopologyGraphRecord {
        TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_cell([0, 0, 0]),
                TopologyVertex::new(1, [1.0, 0.0, 0.0]).with_cell([0, 0, 0]),
                TopologyVertex::new(2, [0.5, 1.0, 0.0]).with_cell([0, 0, 0]),
            ],
            vec![
                TopologyEdge::new(0, [0, 1]),
                TopologyEdge::new(1, [1, 2]).with_periodic_shift([1, 0, 0]),
            ],
            vec![EdgeGroup::from_edge_id(0), EdgeGroup::from_edge_id(1)],
        )
        .with_lattice([[10.0, 0.0, 0.0], [0.0, 9.0, 0.0], [0.0, 0.0, 8.0]])
    }

    #[test]
    fn graph_state_tracks_vertex_assignments_and_counts() {
        let linker = BuildingBlockRecord::new("linker").with_functional_group_count(2);
        let node = BuildingBlockRecord::new("node").with_functional_group_count(3);
        let state = ConstructionGraphState::new(
            periodic_graph(),
            vec![
                BuildingBlockPlacement::new(node.clone(), vec![0, 2]),
                BuildingBlockPlacement::new(linker.clone(), vec![1]),
            ],
        )
        .unwrap();

        assert_eq!(state.get_num_vertices(), 3);
        assert_eq!(state.get_num_edges(), 2);
        assert_eq!(state.get_building_block(1).unwrap(), &linker);
        assert_eq!(state.get_num_building_block(&node), 2);
        assert_eq!(state.get_num_building_block(&linker), 1);
    }

    #[test]
    fn graph_state_returns_reference_aware_edges_for_vertex() {
        let bb = BuildingBlockRecord::new("bb");
        let state = ConstructionGraphState::new(
            periodic_graph(),
            vec![BuildingBlockPlacement::new(bb, vec![0, 1, 2])],
        )
        .unwrap();

        let vertex_one_edges = state.get_edges_for_vertex(1).unwrap();
        assert_eq!(vertex_one_edges.len(), 2);
        let periodic = vertex_one_edges
            .iter()
            .find(|edge| edge.id() == 1)
            .expect("periodic edge for vertex 1");
        assert_eq!(periodic.position_override(), Some([5.75, 0.5, 0.0]));
    }

    #[test]
    fn graph_state_rejects_duplicate_vertex_assignment() {
        let bb1 = BuildingBlockRecord::new("a");
        let bb2 = BuildingBlockRecord::new("b");
        let result = ConstructionGraphState::new(
            periodic_graph(),
            vec![
                BuildingBlockPlacement::new(bb1, vec![0, 1]),
                BuildingBlockPlacement::new(bb2, vec![1, 2]),
            ],
        );

        assert!(matches!(
            result,
            Err(StkDomainError::DuplicateVertexId { id: 1 })
        ));
    }
}
