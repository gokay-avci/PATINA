use super::common::{atom, base_parameters, bond, decorate_sites, distance, motif};
use crate::domain::{Composition, MotifCandidate, TopologyResult};
use serde_json::json;
use std::collections::BTreeSet;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy)]
pub struct ShellGenerator {
    pub bond_length: f64,
}

impl ShellGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self { bond_length }
    }

    pub fn closed_cage(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        self.single_shell(index, composition, seed, "closed_cage", 3)
    }

    pub fn hollow_shell(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        self.single_shell(index, composition, seed, "hollow_shell", 4)
    }

    pub fn multi_shell(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let n = composition.total_atoms;
        let inner = (n / 4).clamp(1, n.saturating_sub(1).max(1));
        let outer = n - inner;
        let outer_radius = shell_radius(outer, self.bond_length);
        let mut positions = fibonacci_sphere(inner, outer_radius * 0.55);
        positions.extend(fibonacci_sphere(outer, outer_radius));
        self.layered_shell(index, composition, seed, "multi_shell", positions, &[inner])
    }

    pub fn onion_like_shell(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let n = composition.total_atoms;
        let first = (n / 8).max(1);
        let second = (n / 4).max(2).min(n.saturating_sub(first + 1));
        let third = n - first - second;
        let outer_radius = shell_radius(third.max(3), self.bond_length);
        let mut positions = fibonacci_sphere(first, outer_radius * 0.35);
        positions.extend(fibonacci_sphere(second, outer_radius * 0.67));
        positions.extend(fibonacci_sphere(third, outer_radius));
        self.layered_shell(
            index,
            composition,
            seed,
            "onion_like_shell",
            positions,
            &[first, first + second],
        )
    }

    pub fn face_capped_polyhedron(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let mut positions = cube_vertices();
        positions.extend(vec![
            [1.55, 0.0, 0.0],
            [-1.55, 0.0, 0.0],
            [0.0, 1.55, 0.0],
            [0.0, -1.55, 0.0],
            [0.0, 0.0, 1.55],
            [0.0, 0.0, -1.55],
        ]);
        self.polyhedral_subset(
            index,
            composition,
            seed,
            "face_capped_polyhedron",
            positions,
        )
    }

    pub fn edge_decorated_polyhedron(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let vertices = cube_vertices();
        let mut positions = vertices.clone();
        for i in 0..vertices.len() {
            for j in (i + 1)..vertices.len() {
                let d = distance(vertices[i], vertices[j]);
                if (d - 2.0).abs() < 1.0e-8 {
                    positions.push(midpoint(vertices[i], vertices[j]));
                }
            }
        }
        self.polyhedral_subset(
            index,
            composition,
            seed,
            "edge_decorated_polyhedron",
            positions,
        )
    }

    fn single_shell(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
        generator_name: &str,
        target_degree: usize,
    ) -> TopologyResult<MotifCandidate> {
        let n = composition.total_atoms;
        let radius = shell_radius(n, self.bond_length);
        let positions = fibonacci_sphere(n, radius);
        let edges = nearest_edges(&positions, target_degree);
        self.build(index, composition, seed, generator_name, positions, edges)
    }

    fn layered_shell(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
        generator_name: &str,
        positions: Vec<[f64; 3]>,
        boundaries: &[usize],
    ) -> TopologyResult<MotifCandidate> {
        let mut layer_ranges = Vec::new();
        let mut start = 0usize;
        for &end in boundaries {
            layer_ranges.push(start..end);
            start = end;
        }
        layer_ranges.push(start..positions.len());

        let mut edges = BTreeSet::<(usize, usize)>::new();
        for range in &layer_ranges {
            let local = positions[range.clone()].to_vec();
            for (i, j) in nearest_edges(&local, 3) {
                edges.insert((range.start + i, range.start + j));
            }
        }
        for pair in layer_ranges.windows(2) {
            let lower = pair[0].clone();
            let upper = pair[1].clone();
            for i in lower {
                let j = upper
                    .clone()
                    .min_by(|a, b| {
                        distance(positions[i], positions[*a])
                            .partial_cmp(&distance(positions[i], positions[*b]))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or(i);
                edges.insert((i.min(j), i.max(j)));
            }
        }
        connect_components(&positions, &mut edges);
        self.build(
            index,
            composition,
            seed,
            generator_name,
            positions,
            edges.into_iter().collect(),
        )
    }

    fn polyhedral_subset(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
        generator_name: &str,
        mut positions: Vec<[f64; 3]>,
    ) -> TopologyResult<MotifCandidate> {
        positions.sort_by(|left, right| {
            radius_sq(*left)
                .partial_cmp(&radius_sq(*right))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let n = composition.total_atoms;
        if positions.len() < n {
            positions.extend(fibonacci_sphere(n - positions.len(), 2.0));
        }
        positions.truncate(n);
        scale_to_min_distance(&mut positions, self.bond_length);
        let edges = nearest_edges(&positions, 4);
        self.build(index, composition, seed, generator_name, positions, edges)
    }

    fn build(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
        generator_name: &str,
        positions: Vec<[f64; 3]>,
        edges: Vec<(usize, usize)>,
    ) -> TopologyResult<MotifCandidate> {
        let labels = decorate_sites(&composition);
        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| atom(i, element, positions[i], "shell_site"))
            .collect::<Vec<_>>();
        let bonds = edges
            .into_iter()
            .map(|(i, j)| bond(i, j, &positions, generator_name))
            .collect::<Vec<_>>();
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("shell_family".to_string(), json!(generator_name));
        parameters.insert("embedding".to_string(), json!("deterministic_shell"));
        Ok(motif(
            index,
            generator_name,
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        ))
    }
}

fn shell_radius(n: usize, bond_length: f64) -> f64 {
    (bond_length * (n.max(4) as f64).sqrt() / PI).max(bond_length)
}

fn fibonacci_sphere(n: usize, radius: f64) -> Vec<[f64; 3]> {
    if n == 0 {
        return Vec::new();
    }
    let golden_angle = PI * (3.0 - 5.0_f64.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - (2.0 * i as f64 + 1.0) / n as f64;
            let r = (1.0 - y * y).sqrt();
            let theta = golden_angle * i as f64;
            [
                radius * r * theta.cos(),
                radius * y,
                radius * r * theta.sin(),
            ]
        })
        .collect()
}

fn nearest_edges(positions: &[[f64; 3]], target_degree: usize) -> Vec<(usize, usize)> {
    if positions.len() < 2 {
        return Vec::new();
    }
    let mut edges = BTreeSet::<(usize, usize)>::new();
    for i in 0..positions.len() {
        let mut neighbors = (0..positions.len())
            .filter(|&j| i != j)
            .map(|j| (distance(positions[i], positions[j]), j))
            .collect::<Vec<_>>();
        neighbors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for &(_, j) in neighbors.iter().take(target_degree) {
            edges.insert((i.min(j), i.max(j)));
        }
    }
    connect_components(positions, &mut edges);
    edges.into_iter().collect()
}

fn connect_components(positions: &[[f64; 3]], edges: &mut BTreeSet<(usize, usize)>) {
    loop {
        let components = components(positions.len(), edges);
        if components.len() <= 1 {
            break;
        }
        let mut best = (f64::INFINITY, 0usize, 0usize);
        for &i in &components[0] {
            for component in components.iter().skip(1) {
                for &j in component {
                    let d = distance(positions[i], positions[j]);
                    if d < best.0 {
                        best = (d, i, j);
                    }
                }
            }
        }
        edges.insert((best.1.min(best.2), best.1.max(best.2)));
    }
}

fn components(n: usize, edges: &BTreeSet<(usize, usize)>) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); n];
    for &(i, j) in edges {
        adjacency[i].push(j);
        adjacency[j].push(i);
    }
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        seen[start] = true;
        while let Some(node) = stack.pop() {
            component.push(node);
            for &neighbor in &adjacency[node] {
                if !seen[neighbor] {
                    seen[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        out.push(component);
    }
    out
}

fn cube_vertices() -> Vec<[f64; 3]> {
    [-1.0, 1.0]
        .into_iter()
        .flat_map(|x| {
            [-1.0, 1.0]
                .into_iter()
                .flat_map(move |y| [-1.0, 1.0].into_iter().map(move |z| [x, y, z]))
        })
        .collect()
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        0.5 * (left[0] + right[0]),
        0.5 * (left[1] + right[1]),
        0.5 * (left[2] + right[2]),
    ]
}

fn scale_to_min_distance(positions: &mut [[f64; 3]], target: f64) {
    let mut min_d = f64::INFINITY;
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            min_d = min_d.min(distance(positions[i], positions[j]));
        }
    }
    if !min_d.is_finite() || min_d <= 0.0 {
        return;
    }
    let scale = target / min_d;
    for position in positions {
        position[0] *= scale;
        position[1] *= scale;
        position[2] *= scale;
    }
}

fn radius_sq(site: [f64; 3]) -> f64 {
    site[0] * site[0] + site[1] * site[1] + site[2] * site[2]
}
