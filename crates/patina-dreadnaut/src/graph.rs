use anyhow::{anyhow, bail, Result};
use patina_search::{
    compute_structure_hashkey_radius, minimum_image_cartesian_distance_sq, HashkeyRadiusMode,
};
use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::inputs::normalize_species_label;
use crate::AtomSpec;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyNode {
    pub index: usize,
    pub species: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyGraph {
    pub radius: f64,
    pub nodes: Vec<TopologyNode>,
    pub edges: BTreeSet<(usize, usize)>,
    pub color_partitions: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairMargin {
    pub left: usize,
    pub right: usize,
    pub left_species: String,
    pub right_species: String,
    pub distance: f64,
    pub margin: f64,
    pub is_edge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEditDistance {
    pub edge_additions: Vec<(usize, usize)>,
    pub edge_deletions: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub connected_components: usize,
    pub degree_histogram_by_species: BTreeMap<String, BTreeMap<usize, usize>>,
}

pub fn compute_hashkey_radius(
    candidate: &Candidate,
    atom_specs: &[AtomSpec],
    radius_mode: &str,
    radius_const: f64,
) -> Result<f64> {
    let species_counts = normalized_species_counts(candidate);
    compute_structure_hashkey_radius(
        &species_counts,
        atom_specs,
        HashkeyRadiusMode::from_label(radius_mode),
        radius_const,
    )
    .map_err(|err| anyhow!(err.to_string()))
}

pub fn build_dreadnaut_graph_text(
    candidate: &Candidate,
    radius: f64,
    atom_specs: &[AtomSpec],
) -> String {
    let graph = build_graph(candidate, radius, atom_specs);
    let n = graph.nodes.len();
    let mut lines = Vec::new();
    lines.push("l=1000".to_string());
    lines.push("c".to_string());
    lines.push(format!("n={} g", n));
    for i in 0..n {
        let mut neighbors = Vec::new();
        for j in 0..n {
            if i == j {
                continue;
            }
            if graph.edges.contains(&(i.min(j), i.max(j))) {
                neighbors.push(j.to_string());
            }
        }
        let terminator = if i + 1 < n { ";" } else { "." };
        if neighbors.is_empty() {
            lines.push(format!("{i} : {terminator}"));
        } else {
            lines.push(format!("{i} : {} {terminator}", neighbors.join(" ")));
        }
    }

    let partitions = graph
        .color_partitions
        .into_iter()
        .map(|indices| {
            indices
                .into_iter()
                .map(|idx| format!("{idx},"))
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    lines.push(format!("f=[{}]", partitions.join("|")));
    lines.push("x".to_string());
    lines.push("z".to_string());
    lines.join("\n")
}

pub fn build_dreadnaut_graph_text_from_parts(
    node_count: usize,
    edges: &[(usize, usize)],
    color_partitions: &[Vec<usize>],
) -> String {
    let mut adjacency = vec![BTreeSet::<usize>::new(); node_count];
    for &(left, right) in edges {
        if left < node_count && right < node_count && left != right {
            adjacency[left].insert(right);
            adjacency[right].insert(left);
        }
    }

    let mut lines = Vec::new();
    lines.push("l=1000".to_string());
    lines.push("c".to_string());
    lines.push(format!("n={} g", node_count));
    for (index, neighbors) in adjacency.iter().enumerate() {
        let terminator = if index + 1 < node_count { ";" } else { "." };
        if neighbors.is_empty() {
            lines.push(format!("{index} : {terminator}"));
        } else {
            let neighbors = neighbors
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!("{index} : {neighbors} {terminator}"));
        }
    }
    let partitions = color_partitions
        .iter()
        .map(|indices| {
            indices
                .iter()
                .map(|idx| format!("{idx},"))
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    lines.push(format!("f=[{}]", partitions.join("|")));
    lines.push("x".to_string());
    lines.push("z".to_string());
    lines.join("\n")
}

pub fn build_graph(candidate: &Candidate, radius: f64, atom_specs: &[AtomSpec]) -> TopologyGraph {
    let filtered = filtered_atoms(candidate);
    let mut species_to_indices = BTreeMap::<String, Vec<usize>>::new();
    let nodes = filtered
        .iter()
        .enumerate()
        .map(|(index, (species, _))| {
            species_to_indices
                .entry(species.clone())
                .or_default()
                .push(index);
            TopologyNode {
                index,
                species: species.clone(),
            }
        })
        .collect::<Vec<_>>();

    let radius_sq = radius * radius;
    let mut edges = BTreeSet::new();
    for i in 0..filtered.len() {
        for j in (i + 1)..filtered.len() {
            let dist_sq = minimum_image_cartesian_distance_sq(
                filtered[i].1,
                filtered[j].1,
                candidate.lattice,
            );
            if dist_sq <= radius_sq + 1.0e-12 {
                edges.insert((i, j));
            }
        }
    }

    let mut color_partitions = atom_specs
        .iter()
        .enumerate()
        .map(|(order, record)| {
            let indices = species_to_indices
                .get(&record.species)
                .cloned()
                .unwrap_or_default();
            (indices.len(), order, indices)
        })
        .collect::<Vec<_>>();
    color_partitions.sort_by_key(|(count, order, _)| (*count, *order));
    let color_partitions = color_partitions
        .into_iter()
        .map(|(_, _, indices)| indices)
        .collect();

    TopologyGraph {
        radius,
        nodes,
        edges,
        color_partitions,
    }
}

pub fn pair_cutoff_margins(candidate: &Candidate, radius: f64) -> Vec<PairMargin> {
    let filtered = filtered_atoms(candidate);
    let mut margins = Vec::new();
    for i in 0..filtered.len() {
        for j in (i + 1)..filtered.len() {
            let dist_sq = minimum_image_cartesian_distance_sq(
                filtered[i].1,
                filtered[j].1,
                candidate.lattice,
            );
            let distance = dist_sq.sqrt();
            let margin = radius - distance;
            margins.push(PairMargin {
                left: i,
                right: j,
                left_species: filtered[i].0.clone(),
                right_species: filtered[j].0.clone(),
                distance,
                margin,
                is_edge: margin >= -1.0e-12,
            });
        }
    }
    margins
}

pub fn graph_edit_distance(
    left: &TopologyGraph,
    right: &TopologyGraph,
) -> Result<GraphEditDistance> {
    if left.nodes.len() != right.nodes.len() {
        bail!(
            "graph node count mismatch: {} vs {}",
            left.nodes.len(),
            right.nodes.len()
        );
    }
    for (left_node, right_node) in left.nodes.iter().zip(right.nodes.iter()) {
        if left_node.species != right_node.species {
            bail!(
                "graph species mismatch at node {}: `{}` vs `{}`",
                left_node.index,
                left_node.species,
                right_node.species
            );
        }
    }
    let edge_additions = right.edges.difference(&left.edges).copied().collect();
    let edge_deletions = left.edges.difference(&right.edges).copied().collect();
    Ok(GraphEditDistance {
        edge_additions,
        edge_deletions,
    })
}

pub fn graph_summary(graph: &TopologyGraph) -> GraphSummary {
    let mut adjacency = vec![Vec::<usize>::new(); graph.nodes.len()];
    for &(left, right) in &graph.edges {
        adjacency[left].push(right);
        adjacency[right].push(left);
    }

    let mut degree_histogram_by_species = BTreeMap::<String, BTreeMap<usize, usize>>::new();
    for node in &graph.nodes {
        let degree = adjacency[node.index].len();
        *degree_histogram_by_species
            .entry(node.species.clone())
            .or_default()
            .entry(degree)
            .or_insert(0) += 1;
    }

    let mut visited = vec![false; graph.nodes.len()];
    let mut connected_components = 0usize;
    for start in 0..graph.nodes.len() {
        if visited[start] {
            continue;
        }
        connected_components += 1;
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        while let Some(node) = queue.pop_front() {
            for &neighbor in &adjacency[node] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    GraphSummary {
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
        connected_components,
        degree_histogram_by_species,
    }
}

fn filtered_atoms(candidate: &Candidate) -> Vec<(String, [f64; 3])> {
    let mut filtered = Vec::new();
    for (species, coords) in candidate
        .species
        .iter()
        .zip(candidate.fractional_coords.iter())
    {
        let species = normalize_species_label(species);
        if species.is_empty() || species.eq_ignore_ascii_case("X") {
            continue;
        }
        // Collapse duplicated core/shell coordinates so topology reflects unique sites.
        let is_duplicate_shell_site = filtered.iter().any(|(existing_species, existing_coords)| {
            existing_species == &species && squared_distance(*existing_coords, *coords) <= 1.0e-8
        });
        if !is_duplicate_shell_site {
            filtered.push((species, *coords));
        }
    }
    filtered
}

fn normalized_species_counts(candidate: &Candidate) -> BTreeMap<String, usize> {
    candidate
        .species
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut acc, species| {
            let normalized = normalize_species_label(species);
            if !normalized.is_empty() && !normalized.eq_ignore_ascii_case("X") {
                *acc.entry(normalized).or_insert(0) += 1;
            }
            acc
        })
}

fn squared_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::{
        build_dreadnaut_graph_text, build_dreadnaut_graph_text_from_parts, build_graph,
        graph_edit_distance, graph_summary, pair_cutoff_margins,
    };
    use crate::AtomSpec;
    use patina_types::Candidate;

    fn atom_specs() -> Vec<AtomSpec> {
        vec![
            AtomSpec {
                species: "O".into(),
                covalent_radius: 0.66,
                ionic_radius: 1.4,
            },
            AtomSpec {
                species: "Mg".into(),
                covalent_radius: 1.41,
                ionic_radius: 0.86,
            },
        ]
    }

    fn candidate() -> Candidate {
        Candidate::from_parts(
            "sample",
            vec!["O".into(), "Mg".into()],
            vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            None,
            [false, false, false],
        )
    }

    #[test]
    fn graph_text_contains_partition_footer() {
        let graph = build_dreadnaut_graph_text(&candidate(), 3.0, &atom_specs());
        assert!(graph.contains("\nf=["));
        assert!(graph.ends_with("\nz"));
    }

    #[test]
    fn graph_text_from_parts_uses_supplied_edges_and_partitions() {
        let graph = build_dreadnaut_graph_text_from_parts(
            4,
            &[(0, 1), (1, 2), (2, 3)],
            &[vec![0, 3], vec![1, 2]],
        );

        assert!(graph.contains("n=4 g"));
        assert!(graph.contains("0 : 1 ;"));
        assert!(graph.contains("1 : 0 2 ;"));
        assert!(graph.contains("f=[0,3,|1,2,]"));
    }

    #[test]
    fn pair_margins_mark_edges_and_non_edges() {
        let margins = pair_cutoff_margins(&candidate(), 0.5);
        assert_eq!(margins.len(), 1);
        assert!(!margins[0].is_edge);
        assert!(margins[0].margin < 0.0);

        let margins = pair_cutoff_margins(&candidate(), 2.0);
        assert!(margins[0].is_edge);
        assert!(margins[0].margin > 0.0);
    }

    #[test]
    fn graph_edit_distance_reports_single_flip() {
        let base = build_graph(&candidate(), 0.5, &atom_specs());
        let changed = build_graph(&candidate(), 2.0, &atom_specs());
        let diff = graph_edit_distance(&base, &changed).expect("diff");
        assert_eq!(diff.edge_additions, vec![(0, 1)]);
        assert!(diff.edge_deletions.is_empty());
    }

    #[test]
    fn graph_summary_counts_components() {
        let disconnected = build_graph(&candidate(), 0.5, &atom_specs());
        let connected = build_graph(&candidate(), 2.0, &atom_specs());
        assert_eq!(graph_summary(&disconnected).connected_components, 2);
        assert_eq!(graph_summary(&connected).connected_components, 1);
    }

    #[test]
    fn build_graph_collapses_colocated_shell_sites() {
        let candidate = Candidate::from_parts(
            "shell-like",
            vec![
                "Ga_core".into(),
                "Ga_shel".into(),
                "As_core".into(),
                "As_shel".into(),
            ],
            vec![
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 2.45],
                [0.0, 0.0, 2.45],
            ],
            None,
            [false, false, false],
        );
        let graph = build_graph(&candidate, 3.0, &atom_specs());
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }
}
