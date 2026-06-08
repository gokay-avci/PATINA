/*!
Placement sorting and periodic-equivalence landing zone.
*/

use std::cmp::Ordering;

use nalgebra::Vector3;

use crate::topology::edge::TopologyEdge;

pub fn sort_edges_by_parent_id(edges: &[TopologyEdge]) -> Vec<TopologyEdge> {
    let mut edges = edges.to_vec();
    edges.sort_by_key(TopologyEdge::parent_id);
    edges
}

pub fn sort_indices_by_angle(
    vectors: &[Vector3<f64>],
    reference: Vector3<f64>,
    axis: Vector3<f64>,
) -> Vec<usize> {
    let mut indexed = vectors
        .iter()
        .enumerate()
        .map(|(index, vector)| (index, angle_about_axis(reference, *vector, axis)))
        .collect::<Vec<_>>();
    indexed.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));
    indexed.into_iter().map(|(index, _)| index).collect()
}

pub fn angle_about_axis(reference: Vector3<f64>, vector: Vector3<f64>, axis: Vector3<f64>) -> f64 {
    let reference = reference.normalize();
    let vector = vector.normalize();
    let axis = axis.normalize();
    let dot = reference.dot(&vector).clamp(-1.0, 1.0);
    let theta = dot.acos();
    let projection = vector.dot(&axis);
    if theta > 0.0 && projection < 0.0 {
        2.0 * std::f64::consts::PI - theta
    } else {
        theta
    }
}

pub fn edge_centroid(edges: &[TopologyEdge]) -> Vector3<f64> {
    let mut centroid = Vector3::new(0.0, 0.0, 0.0);
    for edge in edges {
        let position = edge
            .position_override()
            .expect("edge positions should be materialized before sorting");
        centroid += Vector3::new(position[0], position[1], position[2]);
    }
    centroid / edges.len() as f64
}

#[cfg(test)]
mod tests {
    use nalgebra::Vector3;

    use super::sort_indices_by_angle;

    #[test]
    fn sort_indices_orders_vectors_clockwise_about_axis() {
        let vectors = vec![
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
        ];
        let order = sort_indices_by_angle(
            &vectors,
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        );
        assert_eq!(order, vec![0, 1, 2]);
    }
}
