/*!
Non-periodic 0D topology plans and builder utilities.

This module is intentionally prioritized ahead of periodic COF work.
It gives `patina-stk` a natural home for cluster, cage-like, and other discrete
assembly states without coupling the first usable path to lattice management.
*/

use crate::building_block::BuildingBlockRecord;
use crate::chemistry::{coordination_center, CoordinationGeometry};
use crate::construction::{
    engine::SerialConstructionEngine,
    graph_state::{BuildingBlockPlacement, ConstructionGraphState},
    result::ConstructionResult,
};
use crate::domain::StkDomainError;
use crate::placement::driver::DefaultPlacementDriver;
use crate::topology::{
    edge::TopologyEdge, edge_group::EdgeGroup, graph::TopologyGraphRecord, vertex::TopologyVertex,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ZeroDTopologyPlan {
    graph: TopologyGraphRecord,
    placements: Vec<BuildingBlockPlacement>,
    stages: Vec<Vec<usize>>,
}

impl ZeroDTopologyPlan {
    pub fn new(
        graph: TopologyGraphRecord,
        placements: Vec<BuildingBlockPlacement>,
        stages: Vec<Vec<usize>>,
    ) -> Self {
        Self {
            graph,
            placements,
            stages,
        }
    }

    pub fn graph(&self) -> &TopologyGraphRecord {
        &self.graph
    }

    pub fn placements(&self) -> &[BuildingBlockPlacement] {
        &self.placements
    }

    pub fn stages(&self) -> &[Vec<usize>] {
        &self.stages
    }

    pub fn graph_state(&self) -> Result<ConstructionGraphState, StkDomainError> {
        ConstructionGraphState::new(self.graph.clone(), self.placements.clone())
    }

    pub fn engine(&self) -> SerialConstructionEngine {
        SerialConstructionEngine::new(self.stages.clone())
    }

    pub fn construct_with_default_driver(&self) -> Result<ConstructionResult, StkDomainError> {
        let graph_state = self.graph_state()?;
        self.engine().run(graph_state, &DefaultPlacementDriver)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ZeroDTopologyBuilder {
    vertices: Vec<TopologyVertex>,
    edges: Vec<TopologyEdge>,
    edge_groups: Vec<EdgeGroup>,
    placements: Vec<BuildingBlockPlacement>,
    stages: Vec<Vec<usize>>,
}

impl ZeroDTopologyBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_vertex(mut self, vertex: TopologyVertex) -> Self {
        self.vertices.push(vertex);
        self
    }

    pub fn add_edge(mut self, edge: TopologyEdge) -> Self {
        self.edges.push(edge);
        self
    }

    pub fn add_edge_group(mut self, edge_group: EdgeGroup) -> Self {
        self.edge_groups.push(edge_group);
        self
    }

    pub fn assign_building_block(
        mut self,
        building_block: BuildingBlockRecord,
        vertex_ids: Vec<usize>,
    ) -> Self {
        self.placements
            .push(BuildingBlockPlacement::new(building_block, vertex_ids));
        self
    }

    pub fn add_stage(mut self, stage: Vec<usize>) -> Self {
        self.stages.push(stage);
        self
    }

    pub fn build(self) -> ZeroDTopologyPlan {
        let edge_groups = if self.edge_groups.is_empty() {
            self.edges
                .iter()
                .map(|edge| EdgeGroup::from_edge_id(edge.id()))
                .collect()
        } else {
            self.edge_groups
        };
        let stages = if self.stages.is_empty() {
            let mut all_vertices = self
                .vertices
                .iter()
                .map(TopologyVertex::id)
                .collect::<Vec<_>>();
            all_vertices.sort_unstable();
            vec![all_vertices]
        } else {
            self.stages
        };

        ZeroDTopologyPlan::new(
            TopologyGraphRecord::new(self.vertices, self.edges, edge_groups),
            self.placements,
            stages,
        )
    }
}

pub fn three_site_linear_bridge_plan() -> ZeroDTopologyPlan {
    use crate::topology::vertex::VertexKind;

    ZeroDTopologyBuilder::new()
        .add_vertex(TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(
            TopologyVertex::new(1, [1.5, 0.0, 0.0])
                .with_kind(VertexKind::Linear)
                .with_aligner_edge(0),
        )
        .add_vertex(TopologyVertex::new(2, [3.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_edge(TopologyEdge::new(0, [0, 1]))
        .add_edge(TopologyEdge::new(1, [1, 2]))
        .assign_building_block(
            BuildingBlockRecord::new("terminal")
                .with_functional_group_count(1)
                .with_local_atom_positions(vec![[0.0, 0.0, 0.0]])
                .with_local_functional_group_atom_ids(vec![0]),
            vec![0, 2],
        )
        .assign_building_block(
            BuildingBlockRecord::new("linker")
                .with_functional_group_count(2)
                .with_local_atom_positions(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
                .with_local_functional_group_vectors(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
                .with_local_functional_group_atom_ids(vec![0, 1]),
            vec![1],
        )
        .add_stage(vec![0, 2])
        .add_stage(vec![1])
        .build()
}

pub fn trigonal_zero_d_plan() -> ZeroDTopologyPlan {
    use crate::topology::vertex::VertexKind;

    ZeroDTopologyBuilder::new()
        .add_vertex(
            TopologyVertex::new(0, [0.0, 0.0, 0.0])
                .with_kind(VertexKind::NonLinear)
                .with_aligner_edge(0),
        )
        .add_vertex(TopologyVertex::new(1, [2.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(TopologyVertex::new(2, [-1.0, 1.732, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(TopologyVertex::new(3, [-1.0, -1.732, 0.0]).with_kind(VertexKind::Unaligning))
        .add_edge(TopologyEdge::new(0, [0, 1]).with_parent_id(0))
        .add_edge(TopologyEdge::new(1, [0, 2]).with_parent_id(1))
        .add_edge(TopologyEdge::new(2, [0, 3]).with_parent_id(2))
        .assign_building_block(
            BuildingBlockRecord::new("core")
                .with_functional_group_count(3)
                .with_local_atom_positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
                .with_local_functional_group_vectors(vec![
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [-1.0, -1.0, 0.0],
                ])
                .with_local_functional_group_atom_ids(vec![0, 1, 2]),
            vec![0],
        )
        .assign_building_block(
            BuildingBlockRecord::new("terminal")
                .with_functional_group_count(1)
                .with_local_atom_positions(vec![[0.0, 0.0, 0.0]])
                .with_local_functional_group_atom_ids(vec![0]),
            vec![1, 2, 3],
        )
        .add_stage(vec![1, 2, 3])
        .add_stage(vec![0])
        .build()
}

pub fn square_planar_zero_d_plan() -> ZeroDTopologyPlan {
    use crate::topology::vertex::VertexKind;

    ZeroDTopologyBuilder::new()
        .add_vertex(
            TopologyVertex::new(0, [0.0, 0.0, 0.0])
                .with_kind(VertexKind::NonLinear)
                .with_aligner_edge(0),
        )
        .add_vertex(TopologyVertex::new(1, [2.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(TopologyVertex::new(2, [0.0, 2.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(TopologyVertex::new(3, [-2.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_vertex(TopologyVertex::new(4, [0.0, -2.0, 0.0]).with_kind(VertexKind::Unaligning))
        .add_edge(TopologyEdge::new(0, [0, 1]).with_parent_id(0))
        .add_edge(TopologyEdge::new(1, [0, 2]).with_parent_id(1))
        .add_edge(TopologyEdge::new(2, [0, 3]).with_parent_id(2))
        .add_edge(TopologyEdge::new(3, [0, 4]).with_parent_id(3))
        .assign_building_block(
            coordination_center("metal_center", CoordinationGeometry::SquarePlanar),
            vec![0],
        )
        .assign_building_block(
            BuildingBlockRecord::new("ligand")
                .with_functional_group_count(1)
                .with_local_atom_positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
                .with_local_functional_group_atom_ids(vec![0]),
            vec![1, 2, 3, 4],
        )
        .add_stage(vec![1, 2, 3, 4])
        .add_stage(vec![0])
        .build()
}

#[cfg(test)]
mod tests {
    use super::{
        square_planar_zero_d_plan, three_site_linear_bridge_plan, trigonal_zero_d_plan,
        ZeroDTopologyBuilder,
    };
    use crate::bonding::definition::BondDefinitionRule;
    use crate::topology::{edge::TopologyEdge, vertex::TopologyVertex};

    #[test]
    fn builder_defaults_to_single_edge_groups_and_single_stage() {
        let plan = ZeroDTopologyBuilder::new()
            .add_vertex(TopologyVertex::new(0, [0.0, 0.0, 0.0]))
            .add_vertex(TopologyVertex::new(1, [1.0, 0.0, 0.0]))
            .add_edge(TopologyEdge::new(0, [0, 1]))
            .build();

        assert_eq!(plan.graph().edge_groups.len(), 1);
        assert_eq!(plan.stages(), &[vec![0, 1]]);
    }

    #[test]
    fn linear_bridge_plan_constructs_as_nonperiodic_zero_d_state() {
        let result = three_site_linear_bridge_plan()
            .construct_with_default_driver()
            .expect("construct linear bridge");
        assert_eq!(result.molecule_state().num_placements(), 3);
        assert_eq!(result.molecule_state().position_matrix().len(), 4);
        assert_eq!(result.molecule_state().bonds().len(), 2);
    }

    #[test]
    fn trigonal_zero_d_plan_constructs_as_nonperiodic_zero_d_state() {
        let result = trigonal_zero_d_plan()
            .construct_with_default_driver()
            .expect("construct trigonal zero d");
        assert_eq!(result.molecule_state().num_placements(), 4);
        assert_eq!(result.molecule_state().position_matrix().len(), 6);
        assert_eq!(result.molecule_state().bonds().len(), 3);
    }

    #[test]
    fn square_planar_zero_d_plan_constructs_as_nonperiodic_zero_d_state() {
        let result = square_planar_zero_d_plan()
            .construct_with_default_driver()
            .expect("construct square planar zero d");
        assert_eq!(result.molecule_state().num_placements(), 5);
        assert_eq!(result.molecule_state().bonds().len(), 4);
        assert!(result
            .molecule_state()
            .bonds()
            .iter()
            .all(|bond| bond.rule == BondDefinitionRule::DativeSharedEdge));
        assert!(result
            .molecule_state()
            .bonds()
            .iter()
            .all(|bond| bond.atom_ids.0 == 8 || bond.atom_ids.1 == 8));
    }
}
