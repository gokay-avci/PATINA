use crate::domain::MotifCandidate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryValidationConfig {
    pub target_bond_length: f64,
    pub bond_min_factor: f64,
    pub bond_max_factor: f64,
    pub min_nonbonded_factor: f64,
    pub require_connected: bool,
}

impl GeometryValidationConfig {
    pub fn for_bond_length(target_bond_length: f64) -> Self {
        Self {
            target_bond_length,
            bond_min_factor: 0.75,
            bond_max_factor: 1.25,
            min_nonbonded_factor: 0.60,
            require_connected: true,
        }
    }

    pub fn bond_min(&self) -> f64 {
        self.target_bond_length * self.bond_min_factor
    }

    pub fn bond_max(&self) -> f64 {
        self.target_bond_length * self.bond_max_factor
    }

    pub fn min_nonbonded_distance(&self) -> f64 {
        self.target_bond_length * self.min_nonbonded_factor
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Warning,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub passed: bool,
    pub connected_components: usize,
    pub min_bond_distance: Option<f64>,
    pub max_bond_distance: Option<f64>,
    pub min_nonbonded_distance: Option<f64>,
    pub max_edge_strain: f64,
    pub issues: Vec<ValidationIssue>,
}

pub fn validate_geometry(
    candidate: &MotifCandidate,
    config: &GeometryValidationConfig,
) -> ValidationReport {
    let mut issues = Vec::new();
    let edge_set = candidate
        .bonds
        .iter()
        .map(|bond| (bond.i.min(bond.j), bond.i.max(bond.j)))
        .collect::<BTreeSet<_>>();
    let connected_components = connected_components(candidate.atoms.len(), &edge_set);
    if config.require_connected && connected_components != 1 {
        issues.push(reject(
            "disconnected_graph",
            format!("expected one connected component, found {connected_components}"),
        ));
    }

    let mut min_bond_distance = None::<f64>;
    let mut max_bond_distance = None::<f64>;
    let mut max_edge_strain = 0.0_f64;
    for bond in &candidate.bonds {
        let d = distance(
            candidate.atoms[bond.i].position,
            candidate.atoms[bond.j].position,
        );
        min_bond_distance = Some(min_bond_distance.map_or(d, |value| value.min(d)));
        max_bond_distance = Some(max_bond_distance.map_or(d, |value| value.max(d)));
        if config.target_bond_length > 0.0 {
            max_edge_strain = max_edge_strain
                .max((d - config.target_bond_length).abs() / config.target_bond_length);
        }
        if d < config.bond_min() {
            issues.push(reject(
                "bond_too_short",
                format!(
                    "bond {}-{} distance {d:.4} below {:.4}",
                    bond.i,
                    bond.j,
                    config.bond_min()
                ),
            ));
        }
        if d > config.bond_max() {
            issues.push(reject(
                "bond_too_long",
                format!(
                    "bond {}-{} distance {d:.4} above {:.4}",
                    bond.i,
                    bond.j,
                    config.bond_max()
                ),
            ));
        }
    }

    let mut min_nonbonded_distance = None::<f64>;
    for i in 0..candidate.atoms.len() {
        for j in (i + 1)..candidate.atoms.len() {
            if edge_set.contains(&(i, j)) {
                continue;
            }
            let d = distance(candidate.atoms[i].position, candidate.atoms[j].position);
            min_nonbonded_distance = Some(min_nonbonded_distance.map_or(d, |value| value.min(d)));
            if d < config.min_nonbonded_distance() {
                issues.push(reject(
                    "nonbonded_collision",
                    format!(
                        "nonbonded pair {i}-{j} distance {d:.4} below {:.4}",
                        config.min_nonbonded_distance()
                    ),
                ));
            }
        }
    }

    ValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Reject),
        connected_components,
        min_bond_distance,
        max_bond_distance,
        min_nonbonded_distance,
        max_edge_strain,
        issues,
    }
}

fn reject(code: &str, message: String) -> ValidationIssue {
    ValidationIssue {
        code: code.to_string(),
        severity: ValidationSeverity::Reject,
        message,
    }
}

fn connected_components(n: usize, edges: &BTreeSet<(usize, usize)>) -> usize {
    if n == 0 {
        return 0;
    }
    let mut adjacency = vec![Vec::new(); n];
    for &(i, j) in edges {
        adjacency[i].push(j);
        adjacency[j].push(i);
    }
    let mut seen = vec![false; n];
    let mut count = 0usize;
    for start in 0..n {
        if seen[start] {
            continue;
        }
        count += 1;
        seen[start] = true;
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for &neighbor in &adjacency[node] {
                if !seen[neighbor] {
                    seen[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
    }
    count
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
