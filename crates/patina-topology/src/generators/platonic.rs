use super::common::{atom, base_parameters, bond, decorate_sites, distance, motif};
use crate::domain::{Composition, MotifCandidate, TopologyResult};
use serde_json::json;

#[derive(Debug, Clone, Copy)]
pub struct PlatonicGenerator {
    pub bond_length: f64,
}

impl PlatonicGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self { bond_length }
    }

    pub fn generate_family(
        self,
        start_index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<Vec<MotifCandidate>> {
        let shapes = [
            ("tetrahedron", tetrahedron_sites()),
            ("cube", cube_sites()),
            ("octahedron", octahedron_sites()),
            ("icosahedron", icosahedron_sites()),
        ];
        let mut out = Vec::new();
        for (shape, mut sites) in shapes {
            if sites.len() < composition.total_atoms {
                continue;
            }
            sites.sort_by(|left, right| {
                radius_sq(*left)
                    .partial_cmp(&radius_sq(*right))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let selected = scale_sites(sites[..composition.total_atoms].to_vec(), self.bond_length);
            out.push(self.build(
                start_index + out.len(),
                composition.clone(),
                seed,
                shape,
                selected,
            )?);
        }
        Ok(out)
    }

    fn build(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
        shape: &str,
        positions: Vec<[f64; 3]>,
    ) -> TopologyResult<MotifCandidate> {
        let labels = decorate_sites(&composition);
        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| atom(i, element, positions[i], "platonic_site"))
            .collect::<Vec<_>>();
        let edges = connected_distance_edges(&positions);
        let bonds = edges
            .into_iter()
            .map(|(i, j)| bond(i, j, &positions, "platonic_skeleton"))
            .collect::<Vec<_>>();
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("shape".to_string(), json!(shape));
        parameters.insert(
            "site_set".to_string(),
            json!("vertices_edge_midpoints_face_centres_body_centre_subset"),
        );
        Ok(motif(
            index,
            "platonic",
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        ))
    }
}

fn connected_distance_edges(positions: &[[f64; 3]]) -> Vec<(usize, usize)> {
    if positions.len() < 2 {
        return Vec::new();
    }
    let mut all = Vec::new();
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            all.push((distance(positions[i], positions[j]), i, j));
        }
    }
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let target = all[0].0 * 1.08;
    let mut edges = all
        .iter()
        .filter(|(d, _, _)| *d <= target)
        .map(|(_, i, j)| (*i, *j))
        .collect::<Vec<_>>();
    let mut dsu = LocalDsu::new(positions.len());
    for &(i, j) in &edges {
        dsu.union(i, j);
    }
    for (_, i, j) in all {
        if dsu.find(i) != dsu.find(j) {
            edges.push((i, j));
            dsu.union(i, j);
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}

fn scale_sites(mut sites: Vec<[f64; 3]>, bond_length: f64) -> Vec<[f64; 3]> {
    if sites.len() < 2 {
        return sites;
    }
    let mut min_d = f64::INFINITY;
    for i in 0..sites.len() {
        for j in (i + 1)..sites.len() {
            min_d = min_d.min(distance(sites[i], sites[j]));
        }
    }
    let scale = if min_d.is_finite() && min_d > 0.0 {
        bond_length / min_d
    } else {
        1.0
    };
    for site in &mut sites {
        site[0] *= scale;
        site[1] *= scale;
        site[2] *= scale;
    }
    sites
}

fn tetrahedron_sites() -> Vec<[f64; 3]> {
    enrich_sites(vec![
        [1.0, 1.0, 1.0],
        [-1.0, -1.0, 1.0],
        [-1.0, 1.0, -1.0],
        [1.0, -1.0, -1.0],
    ])
}

fn cube_sites() -> Vec<[f64; 3]> {
    enrich_sites(
        [-1.0, 1.0]
            .into_iter()
            .flat_map(|x| {
                [-1.0, 1.0]
                    .into_iter()
                    .flat_map(move |y| [-1.0, 1.0].into_iter().map(move |z| [x, y, z]))
            })
            .collect(),
    )
}

fn octahedron_sites() -> Vec<[f64; 3]> {
    enrich_sites(vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ])
}

fn icosahedron_sites() -> Vec<[f64; 3]> {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut vertices = Vec::new();
    for s1 in [-1.0, 1.0] {
        for s2 in [-1.0, 1.0] {
            vertices.push([0.0, s1, s2 * phi]);
            vertices.push([s1, s2 * phi, 0.0]);
            vertices.push([s1 * phi, 0.0, s2]);
        }
    }
    enrich_sites(vertices)
}

fn enrich_sites(vertices: Vec<[f64; 3]>) -> Vec<[f64; 3]> {
    let mut sites = vertices.clone();
    for i in 0..vertices.len() {
        for j in (i + 1)..vertices.len() {
            sites.push(midpoint(vertices[i], vertices[j]));
        }
    }
    sites.push([0.0, 0.0, 0.0]);
    dedup_sites(sites)
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        0.5 * (left[0] + right[0]),
        0.5 * (left[1] + right[1]),
        0.5 * (left[2] + right[2]),
    ]
}

fn dedup_sites(sites: Vec<[f64; 3]>) -> Vec<[f64; 3]> {
    let mut out = Vec::<[f64; 3]>::new();
    for site in sites {
        if !out
            .iter()
            .any(|existing| distance(*existing, site) < 1.0e-8)
        {
            out.push(site);
        }
    }
    out
}

fn radius_sq(site: [f64; 3]) -> f64 {
    site[0] * site[0] + site[1] * site[1] + site[2] * site[2]
}

struct LocalDsu {
    parents: Vec<usize>,
}

impl LocalDsu {
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
        let left = self.find(left);
        let right = self.find(right);
        if left != right {
            self.parents[right] = left;
        }
    }
}
