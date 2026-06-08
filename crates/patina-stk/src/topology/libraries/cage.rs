/*!
0D cage-family topology definitions.

These are the highest-priority supramolecular families for the current `patina-stk`
campaign because they stay in the discrete-construction regime while still stressing
alignment, placement staging, and bond-definition logic.
*/

use crate::building_block::BuildingBlockRecord;
use crate::topology::{
    edge::TopologyEdge,
    libraries::zero_d::{trigonal_zero_d_plan, ZeroDTopologyBuilder, ZeroDTopologyPlan},
    vertex::{TopologyVertex, VertexKind},
};

pub fn trigonal_cage_plan() -> ZeroDTopologyPlan {
    trigonal_zero_d_plan()
}

pub fn m2l4_lantern_plan() -> ZeroDTopologyPlan {
    ZeroDTopologyBuilder::new()
        .add_vertex(
            TopologyVertex::new(0, [0.0, 1.0, 0.0])
                .with_kind(VertexKind::NonLinear)
                .with_aligner_edge(0),
        )
        .add_vertex(
            TopologyVertex::new(1, [0.0, -1.0, 0.0])
                .with_kind(VertexKind::NonLinear)
                .with_aligner_edge(0),
        )
        .add_vertex(TopologyVertex::new(2, [2.0, 0.0, 0.0]).with_kind(VertexKind::Linear))
        .add_vertex(TopologyVertex::new(3, [0.0, 0.0, 2.0]).with_kind(VertexKind::Linear))
        .add_vertex(TopologyVertex::new(4, [-2.0, 0.0, 0.0]).with_kind(VertexKind::Linear))
        .add_vertex(TopologyVertex::new(5, [0.0, 0.0, -2.0]).with_kind(VertexKind::Linear))
        .add_edge(TopologyEdge::new(0, [0, 2]).with_parent_id(0))
        .add_edge(TopologyEdge::new(1, [0, 3]).with_parent_id(1))
        .add_edge(TopologyEdge::new(2, [0, 4]).with_parent_id(2))
        .add_edge(TopologyEdge::new(3, [0, 5]).with_parent_id(3))
        .add_edge(TopologyEdge::new(4, [1, 2]).with_parent_id(4))
        .add_edge(TopologyEdge::new(5, [1, 3]).with_parent_id(5))
        .add_edge(TopologyEdge::new(6, [1, 4]).with_parent_id(6))
        .add_edge(TopologyEdge::new(7, [1, 5]).with_parent_id(7))
        .assign_building_block(
            BuildingBlockRecord::new("metal_node")
                .with_functional_group_count(4)
                .with_local_atom_positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [-1.0, 0.0, 0.0],
                    [0.0, 0.0, -1.0],
                ])
                .with_local_functional_group_vectors(vec![
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [-1.0, 0.0, 0.0],
                    [0.0, 0.0, -1.0],
                ])
                .with_local_functional_group_atom_ids(vec![1, 2, 3, 4]),
            vec![0, 1],
        )
        .assign_building_block(
            BuildingBlockRecord::new("bridge_linker")
                .with_functional_group_count(2)
                .with_local_atom_positions(vec![[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
                .with_local_functional_group_vectors(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
                .with_local_functional_group_atom_ids(vec![0, 2]),
            vec![2, 3, 4, 5],
        )
        .add_stage(vec![2, 3, 4, 5])
        .add_stage(vec![0, 1])
        .build()
}

#[cfg(test)]
mod tests {
    use super::{m2l4_lantern_plan, trigonal_cage_plan};

    #[test]
    fn trigonal_cage_plan_constructs() {
        let result = trigonal_cage_plan()
            .construct_with_default_driver()
            .expect("construct trigonal cage");
        assert_eq!(result.molecule_state().bonds().len(), 3);
    }

    #[test]
    fn lantern_plan_constructs() {
        let result = m2l4_lantern_plan()
            .construct_with_default_driver()
            .expect("construct lantern");
        assert_eq!(result.molecule_state().num_placements(), 6);
        assert_eq!(result.molecule_state().bonds().len(), 8);
    }
}
