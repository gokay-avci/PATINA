use crate::domain::{
    CoordinationSignature, GeometrySignature, GraphBasicSignature, HashSignature, MotifCandidate,
    RingSignature, SpectralSignature, SymmetrySignature, TopologySignature,
};
use crate::ports::PointSymmetryBackend;
use nalgebra::{DMatrix, Matrix3, SymmetricEigen, Vector3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureRecord {
    pub candidate_id: String,
    pub generator: String,
    pub signature: TopologySignature,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupRecord {
    pub group_id: usize,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupingOutput {
    pub method: String,
    pub threshold: f64,
    pub groups: Vec<GroupRecord>,
}

struct HashSignatureInput<'a> {
    candidate: &'a MotifCandidate,
    graph: &'a GraphBasicSignature,
    rings: &'a RingSignature,
    geometry: &'a GeometrySignature,
    coordination: &'a CoordinationSignature,
    spectra: Option<&'a SpectralSignature>,
    symmetry: Option<&'a SymmetrySignature>,
    jaccard_features: &'a [String],
}

pub fn fingerprint_candidate(candidate: &MotifCandidate) -> TopologySignature {
    fingerprint_candidate_with_symmetry(candidate, None)
}

pub fn fingerprint_candidate_with_symmetry(
    candidate: &MotifCandidate,
    symmetry_backend: Option<&dyn PointSymmetryBackend>,
) -> TopologySignature {
    let adjacency = adjacency(candidate);
    let graph_basic = graph_basic(candidate, &adjacency);
    let rings = ring_signature(
        candidate,
        &adjacency,
        graph_basic.cycle_rank.max(0) as usize,
    );
    let spectra = Some(spectral_signature(candidate.atoms.len(), &candidate.bonds));
    let geometry = geometry_signature(candidate, &adjacency, &rings);
    let coordination = CoordinationSignature {
        by_element: graph_basic.degree_histogram_by_element.clone(),
        sequence_by_node: adjacency.iter().map(Vec::len).collect(),
    };
    let symmetry = symmetry_backend.map(|backend| symmetry_signature(candidate, backend));
    let jaccard_features = jaccard_features(
        candidate,
        &graph_basic,
        &rings,
        &geometry,
        &coordination,
        spectra.as_ref(),
        symmetry.as_ref(),
    );
    let hashes = hashes(HashSignatureInput {
        candidate,
        graph: &graph_basic,
        rings: &rings,
        geometry: &geometry,
        coordination: &coordination,
        spectra: spectra.as_ref(),
        symmetry: symmetry.as_ref(),
        jaccard_features: &jaccard_features,
    });
    TopologySignature {
        schema_version: "patina-topology-signature/v1".to_string(),
        formula: candidate.composition.total_formula(),
        n_atoms: candidate.atoms.len(),
        graph_basic,
        rings,
        spectra,
        hashes,
        geometry,
        coordination,
        symmetry,
        jaccard_features,
    }
}

fn symmetry_signature(
    candidate: &MotifCandidate,
    backend: &dyn PointSymmetryBackend,
) -> SymmetrySignature {
    backend
        .analyze(candidate)
        .unwrap_or_else(|error| SymmetrySignature {
            point_group: None,
            backend: Some("syva".to_string()),
            tolerance: None,
            max_deviation: None,
            operation_count: None,
            permutation_count: None,
            equivalence_classes: Vec::new(),
            is_linear: None,
            is_planar: None,
            status: "failed".to_string(),
            message: Some(error.to_string()),
        })
}

pub fn jaccard_similarity(left: &[String], right: &[String]) -> f64 {
    let left = left.iter().collect::<BTreeSet<_>>();
    let right = right.iter().collect::<BTreeSet<_>>();
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    intersection as f64 / union as f64
}

pub fn group_by_jaccard(records: &[SignatureRecord], threshold: f64) -> GroupingOutput {
    let mut dsu = DisjointSet::new(records.len());
    for i in 0..records.len() {
        for j in (i + 1)..records.len() {
            let score = jaccard_similarity(
                &records[i].signature.jaccard_features,
                &records[j].signature.jaccard_features,
            );
            if score >= threshold {
                dsu.union(i, j);
            }
        }
    }
    let mut by_root = BTreeMap::<usize, Vec<String>>::new();
    for (index, record) in records.iter().enumerate() {
        by_root
            .entry(dsu.find(index))
            .or_default()
            .push(record.candidate_id.clone());
    }
    let groups = by_root
        .into_values()
        .enumerate()
        .map(|(group_id, members)| GroupRecord { group_id, members })
        .collect();
    GroupingOutput {
        method: "jaccard".to_string(),
        threshold,
        groups,
    }
}

fn adjacency(candidate: &MotifCandidate) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); candidate.atoms.len()];
    for bond in &candidate.bonds {
        adjacency[bond.i].push(bond.j);
        adjacency[bond.j].push(bond.i);
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    adjacency
}

fn graph_basic(candidate: &MotifCandidate, adjacency: &[Vec<usize>]) -> GraphBasicSignature {
    let mut degree_histogram = BTreeMap::<usize, usize>::new();
    let mut degree_histogram_by_element = BTreeMap::<String, BTreeMap<usize, usize>>::new();
    for atom in &candidate.atoms {
        let degree = adjacency[atom.index].len();
        *degree_histogram.entry(degree).or_insert(0) += 1;
        *degree_histogram_by_element
            .entry(atom.element.to_string())
            .or_default()
            .entry(degree)
            .or_insert(0) += 1;
    }

    let mut edge_type_counts = BTreeMap::<String, usize>::new();
    for bond in &candidate.bonds {
        let left = candidate.atoms[bond.i].element.to_string();
        let right = candidate.atoms[bond.j].element.to_string();
        let key = if left <= right {
            format!("{left}-{right}")
        } else {
            format!("{right}-{left}")
        };
        *edge_type_counts.entry(key).or_insert(0) += 1;
    }

    let connected_components = connected_components(adjacency);
    let cycle_rank = candidate.bonds.len() as isize - candidate.atoms.len() as isize
        + connected_components as isize;
    GraphBasicSignature {
        n_nodes: candidate.atoms.len(),
        n_edges: candidate.bonds.len(),
        connected_components,
        cycle_rank,
        degree_histogram,
        degree_histogram_by_element,
        edge_type_counts,
    }
}

fn connected_components(adjacency: &[Vec<usize>]) -> usize {
    if adjacency.is_empty() {
        return 0;
    }
    let mut visited = vec![false; adjacency.len()];
    let mut count = 0usize;
    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }
        count += 1;
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
    count
}

fn ring_signature(
    candidate: &MotifCandidate,
    adjacency: &[Vec<usize>],
    max_cycles: usize,
) -> RingSignature {
    let mut cycles = BTreeSet::<Vec<usize>>::new();
    for bond in &candidate.bonds {
        if let Some(mut cycle) = shortest_cycle_through_edge(adjacency, bond.i, bond.j) {
            cycle.sort_unstable();
            cycles.insert(cycle);
        }
    }
    let mut lengths = cycles
        .into_iter()
        .map(|cycle| cycle.len())
        .collect::<Vec<_>>();
    lengths.sort_unstable();
    if max_cycles > 0 && lengths.len() > max_cycles {
        lengths.truncate(max_cycles);
    }
    let mut ring_size_distribution = BTreeMap::new();
    for length in &lengths {
        *ring_size_distribution.entry(*length).or_insert(0) += 1;
    }
    RingSignature {
        cycle_basis_lengths: lengths,
        ring_size_distribution,
    }
}

fn shortest_cycle_through_edge(
    adjacency: &[Vec<usize>],
    left: usize,
    right: usize,
) -> Option<Vec<usize>> {
    let mut previous = vec![usize::MAX; adjacency.len()];
    let mut queue = VecDeque::from([left]);
    previous[left] = left;
    while let Some(node) = queue.pop_front() {
        for &neighbor in &adjacency[node] {
            if (node == left && neighbor == right) || (node == right && neighbor == left) {
                continue;
            }
            if previous[neighbor] != usize::MAX {
                continue;
            }
            previous[neighbor] = node;
            if neighbor == right {
                let mut path = vec![right];
                let mut cursor = right;
                while cursor != left {
                    cursor = previous[cursor];
                    path.push(cursor);
                }
                return Some(path);
            }
            queue.push_back(neighbor);
        }
    }
    None
}

fn spectral_signature(n: usize, bonds: &[crate::domain::Bond]) -> SpectralSignature {
    let mut adjacency = DMatrix::<f64>::zeros(n, n);
    for bond in bonds {
        adjacency[(bond.i, bond.j)] = 1.0;
        adjacency[(bond.j, bond.i)] = 1.0;
    }
    let mut laplacian = DMatrix::<f64>::zeros(n, n);
    for row in 0..n {
        let degree = (0..n).map(|col| adjacency[(row, col)]).sum::<f64>();
        laplacian[(row, row)] = degree;
        for col in 0..n {
            if row != col {
                laplacian[(row, col)] = -adjacency[(row, col)];
            }
        }
    }
    let mut adjacency_spectrum = SymmetricEigen::new(adjacency)
        .eigenvalues
        .as_slice()
        .to_vec();
    let mut laplacian_spectrum = SymmetricEigen::new(laplacian)
        .eigenvalues
        .as_slice()
        .to_vec();
    sort_round(&mut adjacency_spectrum);
    sort_round(&mut laplacian_spectrum);
    SpectralSignature {
        adjacency_spectrum_hash: hash_debug(&adjacency_spectrum),
        laplacian_spectrum_hash: hash_debug(&laplacian_spectrum),
        adjacency_spectrum,
        laplacian_spectrum,
    }
}

fn geometry_signature(
    candidate: &MotifCandidate,
    adjacency: &[Vec<usize>],
    rings: &RingSignature,
) -> GeometrySignature {
    let n = candidate.atoms.len().max(1) as f64;
    let centroid = candidate.atoms.iter().fold([0.0; 3], |mut acc, atom| {
        acc[0] += atom.position[0] / n;
        acc[1] += atom.position[1] / n;
        acc[2] += atom.position[2] / n;
        acc
    });
    let mut covariance = Matrix3::<f64>::zeros();
    let mut rg_sq = 0.0;
    for atom in &candidate.atoms {
        let delta = Vector3::new(
            atom.position[0] - centroid[0],
            atom.position[1] - centroid[1],
            atom.position[2] - centroid[2],
        );
        rg_sq += delta.dot(&delta) / n;
        covariance += delta * delta.transpose() / n;
    }
    let mut moments = SymmetricEigen::new(covariance)
        .eigenvalues
        .as_slice()
        .to_vec();
    sort_round(&mut moments);
    let principal_moments = [
        *moments.first().unwrap_or(&0.0),
        *moments.get(1).unwrap_or(&0.0),
        *moments.get(2).unwrap_or(&0.0),
    ];
    let asphericity = principal_moments[2] - 0.5 * (principal_moments[0] + principal_moments[1]);
    let acylindricity = principal_moments[1] - principal_moments[0];

    let mut pair_distance_histogram = BTreeMap::<String, usize>::new();
    for i in 0..candidate.atoms.len() {
        for j in (i + 1)..candidate.atoms.len() {
            let d = distance(candidate.atoms[i].position, candidate.atoms[j].position);
            let bin = format!("{:.1}", (d * 10.0).round() / 10.0);
            *pair_distance_histogram.entry(bin).or_insert(0) += 1;
        }
    }

    let morphology_label = classify_morphology(candidate, adjacency, rings, principal_moments);
    GeometrySignature {
        centroid,
        radius_of_gyration: rg_sq.sqrt(),
        principal_moments,
        asphericity,
        acylindricity,
        pair_distance_histogram,
        morphology_label,
    }
}

fn classify_morphology(
    candidate: &MotifCandidate,
    adjacency: &[Vec<usize>],
    rings: &RingSignature,
    moments: [f64; 3],
) -> String {
    match candidate.generator.name.as_str() {
        "ring" => return "ring".to_string(),
        "barrel" => return "barrel".to_string(),
        "wire" => return "wire".to_string(),
        "closed_cage" | "hollow_shell" | "face_capped_polyhedron" | "edge_decorated_polyhedron" => {
            return "cage".to_string()
        }
        "multi_shell" | "onion_like_shell" => return "multi_shell".to_string(),
        _ => {}
    }
    if !rings.ring_size_distribution.is_empty()
        && adjacency.iter().all(|neighbors| neighbors.len() == 2)
    {
        "ring".to_string()
    } else if moments[2] > 10.0 * moments[1].max(1.0e-12) {
        "wire".to_string()
    } else if rings.ring_size_distribution.is_empty() {
        "compact".to_string()
    } else {
        "cage".to_string()
    }
}

fn jaccard_features(
    candidate: &MotifCandidate,
    graph: &GraphBasicSignature,
    rings: &RingSignature,
    geometry: &GeometrySignature,
    coordination: &CoordinationSignature,
    spectra: Option<&SpectralSignature>,
    symmetry: Option<&SymmetrySignature>,
) -> Vec<String> {
    let mut features = BTreeSet::<String>::new();
    features.insert(format!("formula:{}", candidate.composition.total_formula()));
    features.insert(format!("n_atoms:{}", candidate.atoms.len()));
    features.insert(format!("generator:{}", candidate.generator.name));
    features.insert(format!("cycle_rank:{}", graph.cycle_rank));
    features.insert(format!("morphology:{}", geometry.morphology_label));
    for (degree, count) in &graph.degree_histogram {
        features.insert(format!("degree:{degree}:{count}"));
    }
    for (size, count) in &rings.ring_size_distribution {
        features.insert(format!("ring_{size}:{count}"));
    }
    for (element, hist) in &coordination.by_element {
        for (degree, count) in hist {
            features.insert(format!("degree_{element}_{degree}:{count}"));
        }
    }
    for (edge_type, count) in &graph.edge_type_counts {
        features.insert(format!("edge_type:{edge_type}:{count}"));
    }
    if let Some(spectra) = spectra {
        features.insert(format!(
            "laplacian_spectrum:{}",
            spectra.laplacian_spectrum_hash
        ));
    }
    if let Some(symmetry) = symmetry {
        features.insert(format!("symmetry_status:{}", symmetry.status));
        if let Some(backend) = &symmetry.backend {
            features.insert(format!("symmetry_backend:{backend}"));
        }
        if let Some(point_group) = &symmetry.point_group {
            features.insert(format!("point_group:{point_group}"));
        }
        if let Some(operation_count) = symmetry.operation_count {
            features.insert(format!("symmetry_operations:{operation_count}"));
        }
        if let Some(permutation_count) = symmetry.permutation_count {
            features.insert(format!("symmetry_permutations:{permutation_count}"));
        }
    }
    features.into_iter().collect()
}

fn hashes(input: HashSignatureInput<'_>) -> HashSignature {
    let fast_hash = hash_debug(&(
        input.candidate.composition.total_formula(),
        input.candidate.atoms.len(),
        &input.graph.degree_histogram,
        &input.graph.edge_type_counts,
        input.graph.cycle_rank,
        &input.rings.ring_size_distribution,
    ));
    let wl_hash = wl_hash(input.candidate, 4);
    let geometry_distance_hash = hash_debug(&input.geometry.pair_distance_histogram);
    let full_signature_hash = hash_debug(&(
        &fast_hash,
        &wl_hash,
        &geometry_distance_hash,
        &input.coordination.sequence_by_node,
        input
            .spectra
            .map(|s| (&s.adjacency_spectrum_hash, &s.laplacian_spectrum_hash)),
        input
            .symmetry
            .and_then(|signature| signature.point_group.as_ref()),
        input.jaccard_features,
    ));
    HashSignature {
        fast_hash,
        wl_hash,
        canonical_graph_hash: None,
        geometry_distance_hash,
        full_signature_hash,
    }
}

fn wl_hash(candidate: &MotifCandidate, rounds: usize) -> String {
    let adjacency = adjacency(candidate);
    let mut colors = candidate
        .atoms
        .iter()
        .map(|atom| format!("{}:{}", atom.element, adjacency[atom.index].len()))
        .collect::<Vec<_>>();
    for _ in 0..rounds {
        let mut next = Vec::with_capacity(colors.len());
        for node in 0..colors.len() {
            let mut neighbor_colors = adjacency[node]
                .iter()
                .map(|neighbor| colors[*neighbor].clone())
                .collect::<Vec<_>>();
            neighbor_colors.sort();
            next.push(hash_debug(&(colors[node].clone(), neighbor_colors)));
        }
        colors = next;
    }
    let mut color_counts = BTreeMap::<String, usize>::new();
    for color in colors {
        *color_counts.entry(color).or_insert(0) += 1;
    }
    hash_debug(&color_counts)
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn sort_round(values: &mut [f64]) {
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    for value in values {
        *value = (*value * 1.0e8).round() / 1.0e8;
    }
}

fn hash_debug<T: std::fmt::Debug>(value: &T) -> String {
    blake3::hash(format!("{value:?}").as_bytes())
        .to_hex()
        .to_string()[..24]
        .to_string()
}

struct DisjointSet {
    parents: Vec<usize>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        Self {
            parents: (0..n).collect(),
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parents[x] != x {
            self.parents[x] = self.find(self.parents[x]);
        }
        self.parents[x]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left_root = self.find(left);
        let right_root = self.find(right);
        if left_root != right_root {
            self.parents[right_root] = left_root;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Composition;
    use crate::generators::{BarrelGenerator, RingGenerator, WireGenerator};

    #[test]
    fn simple_ring_has_one_cycle() {
        let candidate = RingGenerator::new(1.5)
            .generate(1, Composition::from_formula("AB", 2).unwrap(), Some(1))
            .unwrap();
        let signature = fingerprint_candidate(&candidate);
        assert_eq!(signature.graph_basic.n_nodes, 4);
        assert_eq!(signature.graph_basic.n_edges, 4);
        assert_eq!(signature.graph_basic.cycle_rank, 1);
        assert_eq!(signature.rings.ring_size_distribution.get(&4), Some(&1));
        assert_eq!(signature.geometry.morphology_label, "ring");
    }

    #[test]
    fn wire_has_path_coordination() {
        let candidate = WireGenerator::new(1.5)
            .generate(1, Composition::from_formula("AB", 3).unwrap(), Some(1))
            .unwrap();
        let signature = fingerprint_candidate(&candidate);
        assert_eq!(signature.graph_basic.n_nodes, 6);
        assert_eq!(signature.graph_basic.n_edges, 5);
        assert_eq!(signature.graph_basic.cycle_rank, 0);
        assert_eq!(signature.graph_basic.degree_histogram.get(&1), Some(&2));
        assert_eq!(signature.graph_basic.degree_histogram.get(&2), Some(&4));
    }

    #[test]
    fn barrel_counts_edges_and_cycle_rank() {
        let candidate = BarrelGenerator::new(1.5)
            .generate(
                1,
                Composition::from_formula("AB", 4).unwrap(),
                Some(1),
                4,
                2,
            )
            .unwrap();
        let signature = fingerprint_candidate(&candidate);
        assert_eq!(signature.graph_basic.n_nodes, 8);
        assert_eq!(signature.graph_basic.n_edges, 12);
        assert_eq!(signature.graph_basic.cycle_rank, 5);
        assert_eq!(signature.geometry.morphology_label, "barrel");
    }

    #[test]
    fn exact_jaccard_cases() {
        assert_eq!(jaccard_similarity(&[], &[]), 1.0);
        assert_eq!(jaccard_similarity(&["a".into()], &["a".into()]), 1.0);
        assert_eq!(jaccard_similarity(&["a".into()], &["b".into()]), 0.0);
    }
}
