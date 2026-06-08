/*!
0D host-guest topology definitions.

The first Rust-native version models host-guest systems as non-bonded co-placement states:

- host and guest components are placed at explicit coordinates
- no shared-edge bond inference is requested
- the resulting state is still fully compatible with the construction engine

This gives `patina-stk` a usable landing zone for supramolecular inclusion complexes without
prematurely coupling the implementation to docking, force-field, or reaction semantics.
*/

use std::collections::BTreeMap;

use crate::building_block::BuildingBlockRecord;
use crate::domain::StkDomainError;
use crate::topology::{
    libraries::zero_d::{ZeroDTopologyBuilder, ZeroDTopologyPlan},
    vertex::{TopologyVertex, VertexKind},
};

pub fn host_guest_plan(
    host: BuildingBlockRecord,
    host_position: [f64; 3],
    guests: Vec<(BuildingBlockRecord, [f64; 3])>,
) -> Result<ZeroDTopologyPlan, StkDomainError> {
    let guest_count = guests.len();

    if host.local_atom_positions().is_empty() {
        return Err(StkDomainError::InvalidPlacement {
            reason: "host-guest construction requires a host with local atom positions",
        });
    }
    if guests.is_empty() {
        return Err(StkDomainError::InvalidPlacement {
            reason: "host-guest construction requires at least one guest",
        });
    }
    if guests
        .iter()
        .any(|(guest, _)| guest.local_atom_positions().is_empty())
    {
        return Err(StkDomainError::InvalidPlacement {
            reason: "host-guest construction requires guests with local atom positions",
        });
    }

    let mut builder = ZeroDTopologyBuilder::new();
    let mut placements = BTreeMap::<BuildingBlockRecord, Vec<usize>>::new();

    builder =
        builder.add_vertex(TopologyVertex::new(0, host_position).with_kind(VertexKind::Unaligning));
    placements.entry(host).or_default().push(0);

    for (index, (guest, position)) in guests.into_iter().enumerate() {
        let vertex_id = index + 1;
        builder = builder
            .add_vertex(TopologyVertex::new(vertex_id, position).with_kind(VertexKind::Unaligning));
        placements.entry(guest).or_default().push(vertex_id);
    }

    for (building_block, vertex_ids) in placements {
        builder = builder.assign_building_block(building_block, vertex_ids);
    }

    Ok(builder.add_stage((0..=guest_count).collect()).build())
}

pub fn single_guest_complex_plan() -> ZeroDTopologyPlan {
    host_guest_plan(
        host("host_cavity"),
        [0.0, 0.0, 0.0],
        vec![(guest("guest"), [0.0, 0.0, 0.5])],
    )
    .expect("single guest complex plan should be valid")
}

fn host(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label).with_local_atom_positions(vec![
        [-2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, -2.0, 0.0],
    ])
}

fn guest(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label)
        .with_local_atom_positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
}

#[cfg(test)]
mod tests {
    use super::{host_guest_plan, single_guest_complex_plan};
    use crate::building_block::BuildingBlockRecord;
    use crate::domain::StkDomainError;

    #[test]
    fn single_guest_complex_constructs_without_bonds() {
        let result = single_guest_complex_plan()
            .construct_with_default_driver()
            .expect("construct host guest complex");
        assert_eq!(result.molecule_state().num_placements(), 2);
        assert_eq!(result.molecule_state().bonds().len(), 0);
    }

    #[test]
    fn host_guest_requires_guest_components() {
        let result = host_guest_plan(
            BuildingBlockRecord::new("host").with_local_atom_positions(vec![[0.0, 0.0, 0.0]]),
            [0.0, 0.0, 0.0],
            vec![],
        );

        assert!(matches!(
            result,
            Err(StkDomainError::InvalidPlacement {
                reason: "host-guest construction requires at least one guest",
            })
        ));
    }
}
