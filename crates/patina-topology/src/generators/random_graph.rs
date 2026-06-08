use super::common::{atom, base_parameters, bond, decorate_sites, motif};
use crate::domain::{Composition, MotifCandidate, TopologyResult};
use serde_json::json;
use std::collections::BTreeSet;
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy)]
pub struct RandomGraphGenerator {
    pub bond_length: f64,
    pub max_degree: usize,
}

impl RandomGraphGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self {
            bond_length,
            max_degree: 4,
        }
    }

    pub fn generate(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let n = composition.total_atoms;
        let mut rng = TinyRng::new(seed.unwrap_or(0x5eed) ^ n as u64);
        let mut edges = BTreeSet::<(usize, usize)>::new();
        let mut degrees = vec![0usize; n];
        for i in 0..n.saturating_sub(1) {
            insert_edge(i, i + 1, &mut edges, &mut degrees);
        }
        let target_extra = (n / 2).max(1);
        let mut attempts = 0usize;
        while edges.len() < n.saturating_sub(1) + target_extra && attempts < n * n * 8 {
            attempts += 1;
            let i = rng.next_usize(n);
            let mut j = rng.next_usize(n);
            if i == j {
                j = (j + 1) % n;
            }
            let (left, right) = (i.min(j), i.max(j));
            if degrees[left] >= self.max_degree || degrees[right] >= self.max_degree {
                continue;
            }
            edges.insert((left, right));
            degrees[left] += 1;
            degrees[right] += 1;
        }

        let labels = decorate_sites(&composition);
        let positions = helix_positions(n, self.bond_length);
        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| atom(i, element, positions[i], "random_graph_site"))
            .collect::<Vec<_>>();
        let bonds = edges
            .into_iter()
            .map(|(i, j)| bond(i, j, &positions, "random_constrained"))
            .collect::<Vec<_>>();
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("max_degree".to_string(), json!(self.max_degree));
        parameters.insert("embedding".to_string(), json!("deterministic_helix"));
        parameters.insert("seed".to_string(), json!(seed));
        Ok(motif(
            index,
            "random_graph",
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        ))
    }
}

fn insert_edge(i: usize, j: usize, edges: &mut BTreeSet<(usize, usize)>, degrees: &mut [usize]) {
    let (left, right) = (i.min(j), i.max(j));
    if edges.insert((left, right)) {
        degrees[left] += 1;
        degrees[right] += 1;
    }
}

fn helix_positions(n: usize, bond_length: f64) -> Vec<[f64; 3]> {
    let radius = bond_length;
    let pitch = 0.6 * bond_length;
    (0..n)
        .map(|i| {
            let theta = TAU * i as f64 / 5.0;
            [
                radius * theta.cos(),
                radius * theta.sin(),
                (i as f64 - n as f64 / 2.0) * pitch,
            ]
        })
        .collect()
}

struct TinyRng {
    state: u64,
}

impl TinyRng {
    fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}
