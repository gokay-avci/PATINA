/*!
Linear placement landing zone.
*/

use std::collections::BTreeMap;

use nalgebra::{Rotation3, Unit, Vector3};

use crate::building_block::BuildingBlockRecord;
use crate::construction::placement::PlacementResult;
use crate::domain::StkDomainError;
use crate::topology::{edge::TopologyEdge, vertex::TopologyVertex};

pub fn place_linear_building_block(
    vertex: &TopologyVertex,
    edges: &[TopologyEdge],
    building_block: &BuildingBlockRecord,
) -> Result<PlacementResult, StkDomainError> {
    if building_block.functional_group_count() != 2 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "linear placement requires exactly two functional groups",
        });
    }
    if building_block.local_functional_group_vectors().len() != 2 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "linear placement requires two local functional-group vectors",
        });
    }
    if edges.len() != 2 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "linear placement requires exactly two incident edges",
        });
    }

    let local_fg = building_block.local_functional_group_vectors();
    let start = to_vec3(local_fg[0]) - to_vec3(local_fg[1]);
    let aligner_edge = vertex.aligner_edge().unwrap_or(0);
    let mut target = edge_position(&edges[0]) - edge_position(&edges[1]);
    if aligner_edge != 0 {
        target = -target;
    }
    let rotation = align_vectors(start, target);
    let origin = edge_vertex_position(vertex);
    let placed = building_block
        .local_atom_positions()
        .iter()
        .map(|point| {
            let rotated = rotation * to_vec3(*point) + origin;
            [rotated.x, rotated.y, rotated.z]
        })
        .collect::<Vec<_>>();

    let fg0_position = rotation * to_vec3(local_fg[0]) + origin;
    let mut edge_pairs = edges
        .iter()
        .map(|edge| {
            (
                edge.id(),
                (edge_position(edge) - fg0_position).norm_squared(),
            )
        })
        .collect::<Vec<_>>();
    edge_pairs.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap());
    let functional_group_edges =
        BTreeMap::from([(0usize, edge_pairs[0].0), (1usize, edge_pairs[1].0)]);
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

fn align_vectors(start: Vector3<f64>, target: Vector3<f64>) -> Rotation3<f64> {
    let start = start.normalize();
    let target = target.normalize();
    let cross = start.cross(&target);
    if cross.norm() < 1.0e-10 {
        if start.dot(&target) < 0.0 {
            Rotation3::from_axis_angle(&Vector3::z_axis(), std::f64::consts::PI)
        } else {
            Rotation3::identity()
        }
    } else {
        let axis = Unit::new_normalize(cross);
        let angle = start.dot(&target).clamp(-1.0, 1.0).acos();
        Rotation3::from_axis_angle(&axis, angle)
    }
}

fn edge_vertex_position(vertex: &TopologyVertex) -> Vector3<f64> {
    let [x, y, z] = vertex.position();
    Vector3::new(x, y, z)
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

    use super::place_linear_building_block;

    #[test]
    fn linear_placement_aligns_local_axis_with_edge_axis() {
        let vertex = TopologyVertex::new(0, [0.0, 0.0, 0.0])
            .with_kind(VertexKind::Linear)
            .with_aligner_edge(0);
        let edges = vec![
            TopologyEdge::new(0, [0, 1]).with_position([1.0, 0.0, 0.0]),
            TopologyEdge::new(1, [0, 2]).with_position([-1.0, 0.0, 0.0]),
        ];
        let bb = BuildingBlockRecord::new("linker")
            .with_functional_group_count(2)
            .with_local_atom_positions(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
            .with_local_functional_group_vectors(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
            .with_local_functional_group_atom_ids(vec![0, 1]);

        let placed = place_linear_building_block(&vertex, &edges, &bb).unwrap();
        assert!((placed.position_matrix()[0][0] - 1.0).abs() < 1.0e-12);
        assert!(placed.position_matrix()[0][1].abs() < 1.0e-12);
        assert!((placed.position_matrix()[1][0] + 1.0).abs() < 1.0e-12);
        assert!(placed.position_matrix()[1][1].abs() < 1.0e-12);
    }
}
