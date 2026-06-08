/*!
Nonlinear placement landing zone.
*/

use std::collections::BTreeMap;

use nalgebra::{Rotation3, Vector3};

use crate::building_block::BuildingBlockRecord;
use crate::construction::placement::PlacementResult;
use crate::domain::StkDomainError;
use crate::placement::sorting::{edge_centroid, sort_edges_by_parent_id, sort_indices_by_angle};
use crate::topology::{edge::TopologyEdge, vertex::TopologyVertex};

pub fn place_nonlinear_building_block(
    vertex: &TopologyVertex,
    edges: &[TopologyEdge],
    building_block: &BuildingBlockRecord,
) -> Result<PlacementResult, StkDomainError> {
    if building_block.functional_group_count() <= 2 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "nonlinear placement requires more than two functional groups",
        });
    }
    if building_block.local_functional_group_vectors().len()
        != building_block.functional_group_count()
    {
        return Err(StkDomainError::InvalidPlacement {
            reason: "nonlinear placement requires local vectors for each functional group",
        });
    }
    if edges.len() != building_block.functional_group_count() {
        return Err(StkDomainError::InvalidPlacement {
            reason: "nonlinear placement requires one incident edge per functional group",
        });
    }

    let edges = sort_edges_by_parent_id(edges);
    let origin = Vector3::new(
        vertex.position()[0],
        vertex.position()[1],
        vertex.position()[2],
    );
    let aligner_index = vertex.aligner_edge().unwrap_or(0).min(edges.len() - 1);
    let target = edge_position(&edges[aligner_index]) - origin;

    let local_fg = building_block
        .local_functional_group_vectors()
        .iter()
        .map(|vector| to_vec3(*vector))
        .collect::<Vec<_>>();
    let start = local_fg[0];
    let start_angle = start.y.atan2(start.x);
    let target_angle = target.y.atan2(target.x);
    let rotation = Rotation3::from_axis_angle(&Vector3::z_axis(), target_angle - start_angle);

    let placed = building_block
        .local_atom_positions()
        .iter()
        .map(|point| {
            let rotated = rotation * to_vec3(*point) + origin;
            [rotated.x, rotated.y, rotated.z]
        })
        .collect::<Vec<_>>();

    let rotated_fg = local_fg
        .iter()
        .map(|vector| rotation * *vector)
        .collect::<Vec<_>>();
    let fg_order = sort_indices_by_angle(&rotated_fg, rotated_fg[0], Vector3::z());
    let edge_center = edge_centroid(&edges);
    let edge_vectors = edges
        .iter()
        .map(|edge| edge_position(edge) - edge_center)
        .collect::<Vec<_>>();
    let edge_order = sort_indices_by_angle(
        &edge_vectors,
        edge_position(&edges[aligner_index]) - edge_center,
        Vector3::z(),
    );

    let mut functional_group_edges = BTreeMap::new();
    for (fg_idx, edge_idx) in fg_order.into_iter().zip(edge_order) {
        functional_group_edges.insert(fg_idx, edges[edge_idx].id());
    }
    let functional_group_atom_ids = building_block
        .local_functional_group_atom_ids()
        .iter()
        .enumerate()
        .map(|(fg_id, &atom_id)| (fg_id, atom_id))
        .collect::<BTreeMap<_, _>>();
    let functional_group_bonder_atom_ids = building_block
        .local_functional_group_bonder_atom_ids()
        .iter()
        .enumerate()
        .map(|(fg_id, atom_ids)| (fg_id, atom_ids.clone()))
        .collect::<BTreeMap<_, _>>();
    let functional_group_deleter_atom_ids = building_block
        .local_functional_group_deleter_atom_ids()
        .iter()
        .enumerate()
        .map(|(fg_id, atom_ids)| (fg_id, atom_ids.clone()))
        .collect::<BTreeMap<_, _>>();
    let functional_group_bond_intents = building_block
        .local_functional_group_bond_intents()
        .iter()
        .enumerate()
        .map(|(fg_id, &bond_intent)| (fg_id, bond_intent))
        .collect::<BTreeMap<_, _>>();

    Ok(
        PlacementResult::new(placed, functional_group_edges, functional_group_atom_ids)
            .with_functional_group_metadata(
                functional_group_bonder_atom_ids,
                functional_group_deleter_atom_ids,
                functional_group_bond_intents,
            ),
    )
}

fn edge_position(edge: &TopologyEdge) -> Vector3<f64> {
    let [x, y, z] = edge
        .position_override()
        .expect("edge positions must be materialized");
    Vector3::new(x, y, z)
}

fn to_vec3(input: [f64; 3]) -> Vector3<f64> {
    Vector3::new(input[0], input[1], input[2])
}

#[cfg(test)]
mod tests {
    use crate::building_block::BuildingBlockRecord;
    use crate::topology::{
        edge::TopologyEdge,
        vertex::{TopologyVertex, VertexKind},
    };

    use super::place_nonlinear_building_block;

    #[test]
    fn nonlinear_placement_maps_all_functional_groups_to_edges() {
        let vertex = TopologyVertex::new(0, [0.0, 0.0, 0.0])
            .with_kind(VertexKind::NonLinear)
            .with_aligner_edge(0);
        let edges = vec![
            TopologyEdge::new(0, [0, 1])
                .with_parent_id(0)
                .with_position([1.0, 0.0, 0.0]),
            TopologyEdge::new(1, [0, 2])
                .with_parent_id(1)
                .with_position([0.0, 1.0, 0.0]),
            TopologyEdge::new(2, [0, 3])
                .with_parent_id(2)
                .with_position([-1.0, -1.0, 0.0]),
        ];
        let bb = BuildingBlockRecord::new("tri")
            .with_functional_group_count(3)
            .with_local_atom_positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
            .with_local_functional_group_vectors(vec![
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [-1.0, -1.0, 0.0],
            ])
            .with_local_functional_group_atom_ids(vec![0, 1, 2]);

        let placed = place_nonlinear_building_block(&vertex, &edges, &bb).unwrap();
        assert_eq!(placed.position_matrix().len(), 3);
        assert_eq!(placed.functional_group_edges().len(), 3);
    }
}
