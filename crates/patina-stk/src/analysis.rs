/*!
Topology and geometry descriptors for downstream graph and chemistry tooling.

The records in this module intentionally avoid depending on `patina-dreadnaut`.
They provide stable, typed inputs for graph automorphism work while also exposing
the first molecular internal-coordinate terms needed for chemistry operations.
*/

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::bonding::definition::{BondDefinitionRule, ConstructedBond};
use crate::construction::result::ConstructionResult;
use crate::topology::graph::TopologyGraphRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyVertexDescriptor {
    pub vertex_id: usize,
    pub kind: String,
    pub building_block_label: Option<String>,
    pub degree: usize,
    pub edge_ids: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyEdgeDescriptor {
    pub edge_id: usize,
    pub vertex_ids: [usize; 2],
    pub parent_id: usize,
    pub edge_group: Option<usize>,
    pub periodic_shift: [i32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomorphismGraphView {
    pub vertex_order: Vec<usize>,
    pub edges: Vec<(usize, usize)>,
    pub color_partitions: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionTopologyDescriptor {
    pub vertices: Vec<TopologyVertexDescriptor>,
    pub edges: Vec<TopologyEdgeDescriptor>,
    pub degree_histogram: BTreeMap<usize, usize>,
    pub automorphism_view: AutomorphismGraphView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructedBondTerm {
    pub atom_ids: (usize, usize),
    pub edge_id: usize,
    pub rule: String,
    pub length: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructedAngleTerm {
    pub atom_ids: (usize, usize, usize),
    pub angle_degrees: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructedDihedralTerm {
    pub atom_ids: (usize, usize, usize, usize),
    pub dihedral_degrees: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructionGeometryDescriptor {
    pub atom_count: usize,
    pub degree_by_atom: BTreeMap<usize, usize>,
    pub bonds: Vec<ConstructedBondTerm>,
    pub angles: Vec<ConstructedAngleTerm>,
    pub dihedrals: Vec<ConstructedDihedralTerm>,
}

pub fn describe_construction_topology(
    result: &ConstructionResult,
) -> ConstructionTopologyDescriptor {
    describe_topology_graph(
        result.graph_state().topology(),
        Some(
            result
                .graph_state()
                .vertex_building_block_map()
                .iter()
                .map(|(&vertex_id, building_block)| (vertex_id, building_block.label().to_string()))
                .collect(),
        ),
    )
}

pub fn describe_topology_graph(
    graph: &TopologyGraphRecord,
    building_block_labels: Option<BTreeMap<usize, String>>,
) -> ConstructionTopologyDescriptor {
    let edge_groups_by_edge_id = graph.edge_groups_by_edge_id();
    let mut degree_by_vertex = BTreeMap::<usize, usize>::new();
    let mut edge_ids_by_vertex = BTreeMap::<usize, Vec<usize>>::new();
    for vertex in &graph.vertices {
        degree_by_vertex.insert(vertex.id(), 0);
        edge_ids_by_vertex.insert(vertex.id(), Vec::new());
    }
    for edge in &graph.edges {
        for vertex_id in edge.vertex_ids() {
            *degree_by_vertex.entry(vertex_id).or_insert(0) += 1;
            edge_ids_by_vertex
                .entry(vertex_id)
                .or_default()
                .push(edge.id());
        }
    }

    let mut vertices = graph
        .vertices
        .iter()
        .map(|vertex| {
            let mut edge_ids = edge_ids_by_vertex
                .get(&vertex.id())
                .cloned()
                .unwrap_or_default();
            edge_ids.sort_unstable();
            TopologyVertexDescriptor {
                vertex_id: vertex.id(),
                kind: format!("{:?}", vertex.kind()).to_lowercase(),
                building_block_label: building_block_labels
                    .as_ref()
                    .and_then(|labels| labels.get(&vertex.id()).cloned()),
                degree: degree_by_vertex.get(&vertex.id()).copied().unwrap_or(0),
                edge_ids,
            }
        })
        .collect::<Vec<_>>();
    vertices.sort_by_key(|vertex| vertex.vertex_id);

    let mut edges = graph
        .edges
        .iter()
        .map(|edge| TopologyEdgeDescriptor {
            edge_id: edge.id(),
            vertex_ids: edge.vertex_ids(),
            parent_id: edge.parent_id(),
            edge_group: edge_groups_by_edge_id.get(&edge.id()).copied(),
            periodic_shift: edge.periodic_shift(),
        })
        .collect::<Vec<_>>();
    edges.sort_by_key(|edge| edge.edge_id);

    let mut degree_histogram = BTreeMap::new();
    for degree in degree_by_vertex.values() {
        *degree_histogram.entry(*degree).or_insert(0) += 1;
    }
    let automorphism_view = automorphism_view_from_descriptors(&vertices, &edges);

    ConstructionTopologyDescriptor {
        vertices,
        edges,
        degree_histogram,
        automorphism_view,
    }
}

pub fn describe_construction_geometry(
    result: &ConstructionResult,
) -> ConstructionGeometryDescriptor {
    let positions = result.molecule_state().position_matrix();
    let adjacency = atom_adjacency(result.bonds());
    let degree_by_atom = adjacency
        .iter()
        .map(|(&atom_id, neighbors)| (atom_id, neighbors.len()))
        .collect::<BTreeMap<_, _>>();
    let bonds = result
        .bonds()
        .iter()
        .map(|bond| ConstructedBondTerm {
            atom_ids: ordered_pair(bond.atom_ids),
            edge_id: bond.edge_id,
            rule: bond_rule_label(bond.rule).to_string(),
            length: bond_length(positions, bond.atom_ids),
        })
        .collect();

    let angles = enumerate_angles(&adjacency, positions);
    let dihedrals = enumerate_dihedrals(&adjacency, positions);

    ConstructionGeometryDescriptor {
        atom_count: positions.len(),
        degree_by_atom,
        bonds,
        angles,
        dihedrals,
    }
}

fn automorphism_view_from_descriptors(
    vertices: &[TopologyVertexDescriptor],
    edges: &[TopologyEdgeDescriptor],
) -> AutomorphismGraphView {
    let vertex_order = vertices
        .iter()
        .map(|vertex| vertex.vertex_id)
        .collect::<Vec<_>>();
    let index_by_vertex = vertex_order
        .iter()
        .enumerate()
        .map(|(index, &vertex_id)| (vertex_id, index))
        .collect::<BTreeMap<_, _>>();
    let graph_edges = edges
        .iter()
        .filter_map(|edge| {
            let left = index_by_vertex.get(&edge.vertex_ids[0]).copied()?;
            let right = index_by_vertex.get(&edge.vertex_ids[1]).copied()?;
            Some(ordered_pair((left, right)))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut color_classes = BTreeMap::<(String, Option<String>, usize), Vec<usize>>::new();
    for (index, vertex) in vertices.iter().enumerate() {
        color_classes
            .entry((
                vertex.kind.clone(),
                vertex.building_block_label.clone(),
                vertex.degree,
            ))
            .or_default()
            .push(index);
    }

    AutomorphismGraphView {
        vertex_order,
        edges: graph_edges,
        color_partitions: color_classes.into_values().collect(),
    }
}

fn atom_adjacency(bonds: &[ConstructedBond]) -> BTreeMap<usize, BTreeSet<usize>> {
    let mut adjacency = BTreeMap::<usize, BTreeSet<usize>>::new();
    for bond in bonds {
        let (left, right) = ordered_pair(bond.atom_ids);
        adjacency.entry(left).or_default().insert(right);
        adjacency.entry(right).or_default().insert(left);
    }
    adjacency
}

fn enumerate_angles(
    adjacency: &BTreeMap<usize, BTreeSet<usize>>,
    positions: &[[f64; 3]],
) -> Vec<ConstructedAngleTerm> {
    let mut angles = Vec::new();
    for (&center, neighbors) in adjacency {
        let neighbors = neighbors.iter().copied().collect::<Vec<_>>();
        for left_index in 0..neighbors.len() {
            for right_index in (left_index + 1)..neighbors.len() {
                let left = neighbors[left_index];
                let right = neighbors[right_index];
                angles.push(ConstructedAngleTerm {
                    atom_ids: (left, center, right),
                    angle_degrees: angle_degrees(positions, left, center, right),
                });
            }
        }
    }
    angles
}

fn enumerate_dihedrals(
    adjacency: &BTreeMap<usize, BTreeSet<usize>>,
    positions: &[[f64; 3]],
) -> Vec<ConstructedDihedralTerm> {
    let mut seen = BTreeSet::new();
    let mut dihedrals = Vec::new();
    for (&left_center, right_centers) in adjacency {
        for &right_center in right_centers {
            if left_center > right_center {
                continue;
            }
            let Some(left_neighbors) = adjacency.get(&left_center) else {
                continue;
            };
            let Some(right_neighbors) = adjacency.get(&right_center) else {
                continue;
            };
            for &left in left_neighbors {
                if left == right_center {
                    continue;
                }
                for &right in right_neighbors {
                    if right == left_center || right == left {
                        continue;
                    }
                    let forward = (left, left_center, right_center, right);
                    let reverse = (right, right_center, left_center, left);
                    let canonical = forward.min(reverse);
                    if seen.insert(canonical) {
                        dihedrals.push(ConstructedDihedralTerm {
                            atom_ids: canonical,
                            dihedral_degrees: dihedral_degrees(
                                positions,
                                canonical.0,
                                canonical.1,
                                canonical.2,
                                canonical.3,
                            ),
                        });
                    }
                }
            }
        }
    }
    dihedrals
}

fn bond_length(positions: &[[f64; 3]], atom_ids: (usize, usize)) -> Option<f64> {
    let left = positions.get(atom_ids.0)?;
    let right = positions.get(atom_ids.1)?;
    Some(norm(sub(*right, *left)))
}

fn angle_degrees(positions: &[[f64; 3]], left: usize, center: usize, right: usize) -> Option<f64> {
    let left_vector = sub(*positions.get(left)?, *positions.get(center)?);
    let right_vector = sub(*positions.get(right)?, *positions.get(center)?);
    let denominator = norm(left_vector) * norm(right_vector);
    if denominator <= f64::EPSILON {
        return None;
    }
    let cosine = (dot(left_vector, right_vector) / denominator).clamp(-1.0, 1.0);
    Some(cosine.acos().to_degrees())
}

fn dihedral_degrees(
    positions: &[[f64; 3]],
    atom_1: usize,
    atom_2: usize,
    atom_3: usize,
    atom_4: usize,
) -> Option<f64> {
    let p1 = *positions.get(atom_1)?;
    let p2 = *positions.get(atom_2)?;
    let p3 = *positions.get(atom_3)?;
    let p4 = *positions.get(atom_4)?;

    let b1 = sub(p2, p1);
    let b2 = sub(p3, p2);
    let b3 = sub(p4, p3);
    let n1 = cross(b1, b2);
    let n2 = cross(b2, b3);
    let b2_norm = norm(b2);
    let n1_norm = norm(n1);
    let n2_norm = norm(n2);
    if b2_norm <= f64::EPSILON || n1_norm <= f64::EPSILON || n2_norm <= f64::EPSILON {
        return None;
    }

    let b2_unit = scale(b2, 1.0 / b2_norm);
    let x = dot(n1, n2);
    let y = dot(cross(n1, b2_unit), n2);
    Some(y.atan2(x).to_degrees())
}

fn ordered_pair(pair: (usize, usize)) -> (usize, usize) {
    if pair.0 <= pair.1 {
        pair
    } else {
        (pair.1, pair.0)
    }
}

fn bond_rule_label(rule: BondDefinitionRule) -> &'static str {
    match rule {
        BondDefinitionRule::CovalentSingleBonder => "covalent_single_bonder",
        BondDefinitionRule::CovalentMultiBonder => "covalent_multi_bonder",
        BondDefinitionRule::DativeSharedEdge => "dative_shared_edge",
    }
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn norm(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{describe_construction_geometry, describe_construction_topology};
    use crate::bonding::definition::{BondDefinitionRule, ConstructedBond};
    use crate::building_block::BuildingBlockRecord;
    use crate::construction::{
        graph_state::ConstructionGraphState, molecule_state::ConstructionMoleculeState,
        result::ConstructionResult,
    };
    use crate::topology::{
        edge::TopologyEdge, edge_group::EdgeGroup, graph::TopologyGraphRecord,
        vertex::TopologyVertex,
    };

    #[test]
    fn topology_descriptor_preserves_automorphism_coloring_inputs() {
        let result = super::super::topology::libraries::zero_d::square_planar_zero_d_plan()
            .construct_with_default_driver()
            .expect("construct");

        let descriptor = describe_construction_topology(&result);

        assert_eq!(descriptor.vertices.len(), 5);
        assert_eq!(descriptor.edges.len(), 4);
        assert_eq!(descriptor.degree_histogram.get(&1), Some(&4));
        assert_eq!(descriptor.degree_histogram.get(&4), Some(&1));
        assert_eq!(descriptor.automorphism_view.edges.len(), 4);
        assert!(descriptor
            .automorphism_view
            .color_partitions
            .iter()
            .any(|partition| partition.len() == 4));
    }

    #[test]
    fn geometry_descriptor_enumerates_bonds_angles_and_dihedrals() {
        let graph = TopologyGraphRecord::new(
            vec![
                TopologyVertex::new(0, [0.0, 0.0, 0.0]),
                TopologyVertex::new(1, [1.0, 0.0, 0.0]),
            ],
            vec![TopologyEdge::new(0, [0, 1])],
            vec![EdgeGroup::from_edge_id(0)],
        );
        let graph_state = ConstructionGraphState::new(
            graph,
            vec![
                crate::construction::graph_state::BuildingBlockPlacement::new(
                    BuildingBlockRecord::new("chain"),
                    vec![0, 1],
                ),
            ],
        )
        .expect("graph state");
        let molecule = ConstructionMoleculeState::new()
            .with_bonds(vec![
                ConstructedBond {
                    atom_ids: (0, 1),
                    edge_id: 0,
                    rule: BondDefinitionRule::CovalentSingleBonder,
                },
                ConstructedBond {
                    atom_ids: (1, 2),
                    edge_id: 1,
                    rule: BondDefinitionRule::CovalentSingleBonder,
                },
                ConstructedBond {
                    atom_ids: (2, 3),
                    edge_id: 2,
                    rule: BondDefinitionRule::CovalentSingleBonder,
                },
            ])
            .with_placement_results(
                &[0],
                &[BuildingBlockRecord::new("chain")],
                &[crate::construction::placement::PlacementResult::new(
                    vec![
                        [0.0, 0.0, 0.0],
                        [1.0, 0.0, 0.0],
                        [1.0, 1.0, 0.0],
                        [1.0, 1.0, 1.0],
                    ],
                    Default::default(),
                    Default::default(),
                )],
            );
        let result = ConstructionResult::new(graph_state, molecule);

        let descriptor = describe_construction_geometry(&result);

        assert_eq!(descriptor.atom_count, 4);
        assert_eq!(descriptor.bonds.len(), 3);
        assert_eq!(descriptor.angles.len(), 2);
        assert_eq!(descriptor.dihedrals.len(), 1);
        assert_eq!(descriptor.degree_by_atom.get(&1), Some(&2));
        assert_eq!(descriptor.bonds[0].length, Some(1.0));
        assert!(descriptor.angles[0].angle_degrees.is_some());
        assert!(descriptor.dihedrals[0].dihedral_degrees.is_some());
    }
}
