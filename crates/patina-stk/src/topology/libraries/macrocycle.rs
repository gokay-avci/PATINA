/*!
0D macrocycle topology definitions.

The first Rust port keeps macrocycles in the discrete 0D regime:

- ring closure is modeled explicitly in the topology graph
- vertices are linear two-connection linker placements
- repeating-unit semantics are handled by expanding a sequence into a cycle

This captures the high-value non-periodic construction state without pulling in
`stk`'s optimizer or reaction-factory layers.
*/

use std::collections::BTreeMap;

use crate::building_block::BuildingBlockRecord;
use crate::domain::StkDomainError;
use crate::topology::{
    edge::TopologyEdge,
    libraries::zero_d::{ZeroDTopologyBuilder, ZeroDTopologyPlan},
    vertex::{TopologyVertex, VertexKind},
};

pub fn macrocycle_plan(
    cycle: Vec<BuildingBlockRecord>,
    radius: f64,
) -> Result<ZeroDTopologyPlan, StkDomainError> {
    if cycle.len() < 3 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "macrocycle construction requires at least three ring vertices",
        });
    }
    if radius <= 0.0 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "macrocycle construction requires a positive radius",
        });
    }
    if cycle
        .iter()
        .any(|building_block| building_block.functional_group_count() != 2)
    {
        return Err(StkDomainError::InvalidPlacement {
            reason: "macrocycle construction requires two-functional-group building blocks",
        });
    }

    let mut builder = ZeroDTopologyBuilder::new();
    let mut placements = BTreeMap::<BuildingBlockRecord, Vec<usize>>::new();
    let size = cycle.len();

    for (vertex_id, building_block) in cycle.into_iter().enumerate() {
        let angle = 2.0 * std::f64::consts::PI * vertex_id as f64 / size as f64;
        builder = builder.add_vertex(
            TopologyVertex::new(vertex_id, [radius * angle.cos(), radius * angle.sin(), 0.0])
                .with_kind(VertexKind::Linear)
                .with_aligner_edge(0),
        );
        placements
            .entry(building_block)
            .or_default()
            .push(vertex_id);
    }

    for edge_id in 0..size {
        builder = builder.add_edge(
            TopologyEdge::new(edge_id, [edge_id, (edge_id + 1) % size]).with_parent_id(edge_id),
        );
    }

    for (building_block, vertex_ids) in placements {
        builder = builder.assign_building_block(building_block, vertex_ids);
    }

    Ok(builder.add_stage((0..size).collect()).build())
}

pub fn triangle_macrocycle_plan() -> ZeroDTopologyPlan {
    macrocycle_plan(
        vec![
            ring_linker("triangle_linker_a"),
            ring_linker("triangle_linker_b"),
            ring_linker("triangle_linker_c"),
        ],
        2.0,
    )
    .expect("triangle macrocycle plan should be valid")
}

pub fn alternating_macrocycle_plan(
    num_repeating_units: usize,
) -> Result<ZeroDTopologyPlan, StkDomainError> {
    if num_repeating_units == 0 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "macrocycle repeating-unit count must be greater than zero",
        });
    }

    let mut cycle = Vec::with_capacity(num_repeating_units * 2);
    for _ in 0..num_repeating_units {
        cycle.push(ring_linker("macrocycle_a"));
        cycle.push(ring_linker("macrocycle_b"));
    }

    macrocycle_plan(cycle, 3.0)
}

fn ring_linker(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label)
        .with_functional_group_count(2)
        .with_local_atom_positions(vec![[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
        .with_local_functional_group_vectors(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
        .with_local_functional_group_atom_ids(vec![0, 2])
}

#[cfg(test)]
mod tests {
    use super::{alternating_macrocycle_plan, macrocycle_plan, triangle_macrocycle_plan};
    use crate::building_block::BuildingBlockRecord;
    use crate::domain::StkDomainError;

    #[test]
    fn triangle_macrocycle_constructs() {
        let result = triangle_macrocycle_plan()
            .construct_with_default_driver()
            .expect("construct triangle macrocycle");
        assert_eq!(result.molecule_state().num_placements(), 3);
        assert_eq!(result.molecule_state().bonds().len(), 3);
    }

    #[test]
    fn alternating_macrocycle_constructs() {
        let result = alternating_macrocycle_plan(3)
            .expect("valid alternating macrocycle")
            .construct_with_default_driver()
            .expect("construct alternating macrocycle");
        assert_eq!(result.molecule_state().num_placements(), 6);
        assert_eq!(result.molecule_state().bonds().len(), 6);
    }

    #[test]
    fn macrocycle_rejects_non_linear_ring_building_blocks() {
        let result = macrocycle_plan(
            vec![
                BuildingBlockRecord::new("bad").with_functional_group_count(1),
                BuildingBlockRecord::new("good").with_functional_group_count(2),
                BuildingBlockRecord::new("good-2").with_functional_group_count(2),
            ],
            2.0,
        );

        assert!(matches!(
            result,
            Err(StkDomainError::InvalidPlacement {
                reason: "macrocycle construction requires two-functional-group building blocks",
            })
        ));
    }
}
