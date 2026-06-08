/*!
0D rotaxane topology definitions.

The current implementation treats a rotaxane as a threaded but non-bonded assembly state:

- one axle component sits on the threading axis
- one or more cycle components are positioned along that axis
- no covalent bond inference is created between axle and rings

This provides a useful supramolecular construction state now, while leaving room for later
threading constraints, stopper semantics, and optimizer-driven relaxation.
*/

use std::collections::BTreeMap;

use crate::building_block::BuildingBlockRecord;
use crate::domain::StkDomainError;
use crate::topology::{
    libraries::zero_d::{ZeroDTopologyBuilder, ZeroDTopologyPlan},
    vertex::{TopologyVertex, VertexKind},
};

pub fn rotaxane_plan(
    axle: BuildingBlockRecord,
    cycles: Vec<(BuildingBlockRecord, f64)>,
) -> Result<ZeroDTopologyPlan, StkDomainError> {
    if axle.local_atom_positions().is_empty() {
        return Err(StkDomainError::InvalidPlacement {
            reason: "rotaxane construction requires an axle with local atom positions",
        });
    }
    if cycles.is_empty() {
        return Err(StkDomainError::InvalidPlacement {
            reason: "rotaxane construction requires at least one macrocycle",
        });
    }
    if cycles
        .iter()
        .any(|(cycle, _)| cycle.local_atom_positions().is_empty())
    {
        return Err(StkDomainError::InvalidPlacement {
            reason: "rotaxane construction requires macrocycles with local atom positions",
        });
    }

    let mut builder = ZeroDTopologyBuilder::new();
    let mut placements = BTreeMap::<BuildingBlockRecord, Vec<usize>>::new();

    builder = builder
        .add_vertex(TopologyVertex::new(0, [0.0, 0.0, 0.0]).with_kind(VertexKind::Unaligning));
    placements.entry(axle).or_default().push(0);

    for (index, (cycle, offset)) in cycles.into_iter().enumerate() {
        let vertex_id = index + 1;
        builder = builder.add_vertex(
            TopologyVertex::new(vertex_id, [offset, 0.0, 0.0]).with_kind(VertexKind::Unaligning),
        );
        placements.entry(cycle).or_default().push(vertex_id);
    }

    for (building_block, vertex_ids) in placements {
        builder = builder.assign_building_block(building_block, vertex_ids);
    }

    Ok(builder.build())
}

pub fn single_ring_rotaxane_plan() -> ZeroDTopologyPlan {
    rotaxane_plan(axle("axle"), vec![(macrocycle("ring"), 0.0)])
        .expect("single ring rotaxane plan should be valid")
}

fn axle(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label).with_local_atom_positions(vec![
        [-3.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
    ])
}

fn macrocycle(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label).with_local_atom_positions(vec![
        [0.0, 2.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, -2.0, 0.0],
        [-2.0, 0.0, 0.0],
    ])
}

#[cfg(test)]
mod tests {
    use super::{rotaxane_plan, single_ring_rotaxane_plan};
    use crate::building_block::BuildingBlockRecord;
    use crate::domain::StkDomainError;

    #[test]
    fn single_ring_rotaxane_constructs_without_bonds() {
        let result = single_ring_rotaxane_plan()
            .construct_with_default_driver()
            .expect("construct single-ring rotaxane");
        assert_eq!(result.molecule_state().num_placements(), 2);
        assert_eq!(result.molecule_state().bonds().len(), 0);
    }

    #[test]
    fn rotaxane_requires_cycle_components() {
        let result = rotaxane_plan(
            BuildingBlockRecord::new("axle").with_local_atom_positions(vec![[0.0, 0.0, 0.0]]),
            vec![],
        );

        assert!(matches!(
            result,
            Err(StkDomainError::InvalidPlacement {
                reason: "rotaxane construction requires at least one macrocycle",
            })
        ));
    }
}
