use super::common::{atom, base_parameters, bond, motif};
use crate::domain::{
    validate_geometry, Composition, ElementSymbol, GeometryValidationConfig, MotifCandidate,
    TopologyResult, ValidationReport,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegreeBounds {
    pub min: usize,
    pub max: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChemicalEdgePolicy {
    Any,
    PreferHetero,
    RequireHetero,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstrainedRandomReport {
    pub attempts: usize,
    pub accepted_edges: usize,
    pub rejected_degree: usize,
    pub rejected_chemical_policy: usize,
    pub validation: ValidationReport,
}

#[derive(Debug, Clone)]
pub struct ConstrainedRandomGraphGenerator {
    pub bond_length: f64,
    pub degree_bounds: BTreeMap<ElementSymbol, DegreeBounds>,
    pub fallback_bounds: DegreeBounds,
    pub edge_policy: ChemicalEdgePolicy,
    pub target_extra_edges: usize,
    pub max_attempts_factor: usize,
    pub validation_config: GeometryValidationConfig,
}

impl ConstrainedRandomGraphGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self {
            bond_length,
            degree_bounds: BTreeMap::new(),
            fallback_bounds: DegreeBounds { min: 1, max: 4 },
            edge_policy: ChemicalEdgePolicy::PreferHetero,
            target_extra_edges: 0,
            max_attempts_factor: 12,
            validation_config: GeometryValidationConfig::for_bond_length(bond_length),
        }
    }

    pub fn generate(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let labels = interleaved_labels(&composition);
        let n = labels.len();
        let mut rng = TinyRng::new(seed.unwrap_or(0x9e3779b97f4a7c15) ^ (index as u64) << 32);
        let positions = sphere_positions(n, self.bond_length);
        let mut edges = BTreeSet::<(usize, usize)>::new();
        let mut degree = vec![0usize; n];
        let validation_config = self.validation_config.clone();
        let pair_order = sorted_pairs(&positions);
        let mut report = ConstrainedRandomReport {
            attempts: 0,
            accepted_edges: 0,
            rejected_degree: 0,
            rejected_chemical_policy: 0,
            validation: ValidationReport {
                passed: false,
                connected_components: n,
                min_bond_distance: None,
                max_bond_distance: None,
                min_nonbonded_distance: None,
                max_edge_strain: 0.0,
                issues: Vec::new(),
            },
        };

        connect_by_nearest_pairs(&labels, &pair_order, &mut edges, &mut degree, &self);

        let target_edges = (n.saturating_sub(1) + self.target_extra_edges.max(n / 2))
            .min(n.saturating_mul(self.fallback_bounds.max) / 2);
        let max_attempts = self.max_attempts_factor * n.max(1) * n.max(1);
        while edges.len() < target_edges && report.attempts < max_attempts && !pair_order.is_empty()
        {
            report.attempts += 1;
            let (_, left, right) = pair_order[rng.next_usize(pair_order.len())];
            if edges.contains(&(left, right)) {
                continue;
            }
            if distance(positions[left], positions[right]) > validation_config.bond_max() {
                continue;
            }
            if !self.degree_allows(&labels, &degree, left, right) {
                report.rejected_degree += 1;
                continue;
            }
            if !self.edge_policy_allows(&labels[left], &labels[right], false) {
                report.rejected_chemical_policy += 1;
                continue;
            }
            add_edge(left, right, &mut edges, &mut degree);
        }
        report.accepted_edges = edges.len();

        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| atom(i, element, positions[i], "constrained_random_site"))
            .collect::<Vec<_>>();
        let bonds = edges
            .into_iter()
            .map(|(i, j)| bond(i, j, &positions, "constrained_random"))
            .collect::<Vec<_>>();
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("edge_policy".to_string(), json!(self.edge_policy));
        parameters.insert(
            "target_extra_edges".to_string(),
            json!(self.target_extra_edges),
        );
        parameters.insert(
            "degree_bounds".to_string(),
            serde_json::to_value(&self.degree_bounds).unwrap_or_else(|_| json!({})),
        );

        let mut candidate = motif(
            index,
            "constrained_random_graph",
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        );
        report.validation = validate_geometry(&candidate, &validation_config);
        candidate.parameters.insert(
            "validation_report".to_string(),
            serde_json::to_value(&report.validation).unwrap_or_else(|_| json!({})),
        );
        candidate.parameters.insert(
            "constrained_random_report".to_string(),
            serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
        );
        Ok(candidate)
    }

    fn bounds_for(&self, element: &ElementSymbol) -> &DegreeBounds {
        self.degree_bounds
            .get(element)
            .unwrap_or(&self.fallback_bounds)
    }

    fn degree_allows(
        &self,
        labels: &[ElementSymbol],
        degree: &[usize],
        left: usize,
        right: usize,
    ) -> bool {
        degree[left] < self.bounds_for(&labels[left]).max
            && degree[right] < self.bounds_for(&labels[right]).max
    }

    fn edge_policy_allows(
        &self,
        left: &ElementSymbol,
        right: &ElementSymbol,
        connectivity_edge: bool,
    ) -> bool {
        match self.edge_policy {
            ChemicalEdgePolicy::Any => true,
            ChemicalEdgePolicy::PreferHetero => connectivity_edge || left != right,
            ChemicalEdgePolicy::RequireHetero => left != right,
        }
    }
}

fn connect_by_nearest_pairs(
    labels: &[ElementSymbol],
    pair_order: &[(f64, usize, usize)],
    edges: &mut BTreeSet<(usize, usize)>,
    degree: &mut [usize],
    generator: &ConstrainedRandomGraphGenerator,
) {
    let mut dsu = LocalDsu::new(labels.len());
    for &(_, i, j) in pair_order {
        if dsu.find(i) == dsu.find(j) {
            continue;
        }
        if generator.degree_allows(labels, degree, i, j)
            && generator.edge_policy_allows(&labels[i], &labels[j], true)
        {
            add_edge(i, j, edges, degree);
            dsu.union(i, j);
        }
        if dsu.component_count == 1 {
            break;
        }
    }
}

fn add_edge(i: usize, j: usize, edges: &mut BTreeSet<(usize, usize)>, degree: &mut [usize]) {
    let edge = (i.min(j), i.max(j));
    if edges.insert(edge) {
        degree[edge.0] += 1;
        degree[edge.1] += 1;
    }
}

fn sphere_positions(n: usize, bond_length: f64) -> Vec<[f64; 3]> {
    if n == 0 {
        return Vec::new();
    }
    let radius = (bond_length * (n.max(4) as f64).sqrt() / std::f64::consts::PI).max(bond_length);
    let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
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

fn interleaved_labels(composition: &Composition) -> Vec<ElementSymbol> {
    let mut remaining = composition
        .total_counts
        .iter()
        .map(|(element, count)| (element.clone(), *count))
        .collect::<Vec<_>>();
    let mut labels = Vec::with_capacity(composition.total_atoms);
    while labels.len() < composition.total_atoms {
        remaining.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        let active_count = remaining.iter().filter(|(_, count)| *count > 0).count();
        let mut inserted = false;
        for (element, count) in &mut remaining {
            if *count == 0 {
                continue;
            }
            if labels.last() == Some(element) && active_count > 1 {
                continue;
            }
            labels.push(element.clone());
            *count -= 1;
            inserted = true;
            break;
        }
        if !inserted {
            if let Some((element, count)) = remaining.iter_mut().find(|(_, count)| *count > 0) {
                labels.push(element.clone());
                *count -= 1;
            }
        }
    }
    labels
}

fn sorted_pairs(positions: &[[f64; 3]]) -> Vec<(f64, usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            pairs.push((distance(positions[i], positions[j]), i, j));
        }
    }
    pairs.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pairs
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

struct LocalDsu {
    parents: Vec<usize>,
    component_count: usize,
}

impl LocalDsu {
    fn new(n: usize) -> Self {
        Self {
            parents: (0..n).collect(),
            component_count: n,
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
            self.component_count -= 1;
        }
    }
}

pub(crate) struct TinyRng {
    state: u64,
}

impl TinyRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    pub(crate) fn next_usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}
