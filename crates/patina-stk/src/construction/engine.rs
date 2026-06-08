/*!
Construction engine landing zone.
*/

use crate::bonding::internal::{
    infer_bonds_from_shared_edges, infer_deleted_atoms_from_shared_edges,
};
use crate::building_block::BuildingBlockRecord;
use crate::construction::{
    graph_state::ConstructionGraphState, molecule_state::ConstructionMoleculeState,
    placement::PlacementResult, result::ConstructionResult,
};
use crate::domain::StkDomainError;
use crate::topology::{edge::TopologyEdge, vertex::TopologyVertex};

pub trait PlacementDriver {
    fn place_building_block(
        &self,
        vertex: &TopologyVertex,
        edges: &[TopologyEdge],
        building_block: &BuildingBlockRecord,
    ) -> Result<PlacementResult, StkDomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialConstructionEngine {
    stages: Vec<Vec<usize>>,
}

impl SerialConstructionEngine {
    pub fn new(stages: Vec<Vec<usize>>) -> Self {
        Self { stages }
    }

    pub fn stages(&self) -> &[Vec<usize>] {
        &self.stages
    }

    pub fn run(
        &self,
        graph_state: ConstructionGraphState,
        driver: &impl PlacementDriver,
    ) -> Result<ConstructionResult, StkDomainError> {
        let mut molecule_state = ConstructionMoleculeState::new();
        for stage in &self.stages {
            let mut stage_results = Vec::with_capacity(stage.len());
            let mut stage_building_blocks = Vec::with_capacity(stage.len());
            for &vertex_id in stage {
                let vertex = graph_state.get_vertex(vertex_id)?;
                let building_block = graph_state.get_building_block(vertex_id)?;
                let edges = graph_state.get_edges_for_vertex(vertex_id)?;
                stage_building_blocks.push(building_block.clone());
                stage_results.push(driver.place_building_block(vertex, &edges, building_block)?);
            }
            molecule_state.apply_placement_results(stage, &stage_building_blocks, &stage_results);
        }
        let provisional = ConstructionResult::new(graph_state, molecule_state);
        let bonds = infer_bonds_from_shared_edges(provisional.state());
        let deleted_atom_ids = infer_deleted_atoms_from_shared_edges(provisional.state());
        let molecule_state = provisional
            .molecule_state()
            .clone()
            .with_bonds(bonds)
            .with_deleted_atom_ids(deleted_atom_ids);
        Ok(ConstructionResult::new(
            provisional.graph_state().clone(),
            molecule_state,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::SerialConstructionEngine;
    use crate::building_block::BuildingBlockRecord;
    use crate::construction::graph_state::{BuildingBlockPlacement, ConstructionGraphState};
    use crate::placement::driver::DefaultPlacementDriver;
    use crate::topology::{
        edge::TopologyEdge,
        edge_group::EdgeGroup,
        graph::TopologyGraphRecord,
        vertex::{TopologyVertex, VertexKind},
    };

    fn graph_state() -> ConstructionGraphState {
        let topology = TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning),
                TopologyVertex::new(1, [1.0, 0.0, 0.0])
                    .with_kind(VertexKind::Linear)
                    .with_aligner_edge(0),
                TopologyVertex::new(2, [2.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning),
            ],
            vec![TopologyEdge::new(0, [0, 1]), TopologyEdge::new(1, [1, 2])],
            vec![EdgeGroup::from_edge_id(0), EdgeGroup::from_edge_id(1)],
        );
        ConstructionGraphState::new(
            topology,
            vec![
                BuildingBlockPlacement::new(
                    BuildingBlockRecord::new("node")
                        .with_functional_group_count(1)
                        .with_local_atom_positions(vec![[0.0, 0.0, 0.0]])
                        .with_local_functional_group_atom_ids(vec![0]),
                    vec![2],
                ),
                BuildingBlockPlacement::new(
                    BuildingBlockRecord::new("linker")
                        .with_functional_group_count(2)
                        .with_local_atom_positions(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
                        .with_local_functional_group_vectors(vec![
                            [-1.0, 0.0, 0.0],
                            [1.0, 0.0, 0.0],
                        ])
                        .with_local_functional_group_atom_ids(vec![0, 1]),
                    vec![1],
                ),
                BuildingBlockPlacement::new(
                    BuildingBlockRecord::new("seed")
                        .with_functional_group_count(1)
                        .with_local_atom_positions(vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
                        .with_local_functional_group_atom_ids(vec![0]),
                    vec![0],
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn serial_engine_runs_stages_and_accumulates_results() {
        let engine = SerialConstructionEngine::new(vec![vec![0, 2], vec![1]]);
        let result = engine.run(graph_state(), &DefaultPlacementDriver).unwrap();
        assert_eq!(result.molecule_state().num_placements(), 3);
        assert_eq!(result.molecule_state().position_matrix().len(), 5);
        assert_eq!(
            result.molecule_state().position_matrix()[0],
            [0.0, 0.0, 0.0]
        );
        assert_eq!(
            result.molecule_state().position_matrix()[4],
            [2.0, 0.0, 0.0]
        );
        assert_eq!(result.molecule_state().bonds().len(), 2);
    }
}
