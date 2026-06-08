use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomSpec {
    pub species: String,
    pub covalent_radius: f64,
    pub ionic_radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyAtom {
    pub species: String,
    pub coords: [f64; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashkeyRadiusMode {
    Ionic,
    Covalent,
}

impl HashkeyRadiusMode {
    pub fn from_label(label: &str) -> Self {
        if label.eq_ignore_ascii_case("CR") {
            Self::Covalent
        } else {
            Self::Ionic
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum TopologyError {
    #[error("no non-placeholder species were found")]
    NoNonPlaceholderSpecies,
    #[error("missing atom specification for species `{species}`")]
    MissingAtomSpec { species: String },
}

pub fn compute_structure_hashkey_radius(
    species_counts: &BTreeMap<String, usize>,
    atom_specs: &[AtomSpec],
    radius_mode: HashkeyRadiusMode,
    radius_const: f64,
) -> Result<f64, TopologyError> {
    let present = species_counts
        .keys()
        .filter(|species| !species.eq_ignore_ascii_case("X"))
        .cloned()
        .collect::<Vec<_>>();
    if present.is_empty() {
        return Err(TopologyError::NoNonPlaceholderSpecies);
    }

    let radii = present
        .iter()
        .map(|species| {
            atom_specs
                .iter()
                .find(|record| record.species == *species)
                .map(|record| match radius_mode {
                    HashkeyRadiusMode::Covalent => record.covalent_radius,
                    HashkeyRadiusMode::Ionic => record.ionic_radius,
                })
                .ok_or_else(|| TopologyError::MissingAtomSpec {
                    species: species.clone(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut max_dist = 0.0_f64;
    for i in 0..radii.len() {
        for j in 0..radii.len() {
            if i != j || radii.len() == 1 {
                max_dist = max_dist.max(radii[i] + radii[j] + radius_const);
            }
        }
    }
    Ok(max_dist)
}

pub fn build_structure_edges(atoms: &[TopologyAtom], radius: f64) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    let radius_sq = radius * radius;
    for i in 0..atoms.len() {
        for j in (i + 1)..atoms.len() {
            let dx = atoms[i].coords[0] - atoms[j].coords[0];
            let dy = atoms[i].coords[1] - atoms[j].coords[1];
            let dz = atoms[i].coords[2] - atoms[j].coords[2];
            let dist_sq = dx * dx + dy * dy + dz * dz;
            if dist_sq <= radius_sq + 1e-12 {
                edges.push((i, j));
            }
        }
    }
    edges
}

pub fn summarize_edge_pairs(
    atoms: &[TopologyAtom],
    edges: &[(usize, usize)],
) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for &(left, right) in edges {
        let key = canonical_species_pair(&atoms[left].species, &atoms[right].species);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

pub fn summarize_coordination_histograms(
    atoms: &[TopologyAtom],
    edges: &[(usize, usize)],
) -> BTreeMap<String, BTreeMap<u32, u32>> {
    let mut degrees = vec![0_u32; atoms.len()];
    for &(left, right) in edges {
        degrees[left] += 1;
        degrees[right] += 1;
    }

    let mut histograms = BTreeMap::new();
    for (atom, degree) in atoms.iter().zip(degrees) {
        let entry = histograms
            .entry(atom.species.clone())
            .or_insert_with(BTreeMap::new);
        *entry.entry(degree).or_insert(0) += 1;
    }
    histograms
}

pub fn classify_topology_verdict(
    exact_hashkey_match: Option<bool>,
    edge_diff_total: u32,
    coordination_delta_total: u32,
    near_edge_threshold: u32,
    near_coordination_threshold: u32,
) -> &'static str {
    if exact_hashkey_match == Some(true) {
        "identical"
    } else if edge_diff_total <= near_edge_threshold
        && coordination_delta_total <= near_coordination_threshold
    {
        "near"
    } else {
        "different"
    }
}

fn canonical_species_pair(left: &str, right: &str) -> String {
    if left <= right {
        format!("{left}-{right}")
    } else {
        format!("{right}-{left}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashkey_radius_uses_requested_mode() {
        let species_counts =
            BTreeMap::from([(String::from("Mg"), 1usize), (String::from("O"), 1usize)]);
        let atom_specs = vec![
            AtomSpec {
                species: "Mg".into(),
                covalent_radius: 1.41,
                ionic_radius: 0.86,
            },
            AtomSpec {
                species: "O".into(),
                covalent_radius: 0.66,
                ionic_radius: 1.26,
            },
        ];

        let ionic = compute_structure_hashkey_radius(
            &species_counts,
            &atom_specs,
            HashkeyRadiusMode::Ionic,
            0.2,
        )
        .unwrap();
        let covalent = compute_structure_hashkey_radius(
            &species_counts,
            &atom_specs,
            HashkeyRadiusMode::Covalent,
            0.2,
        )
        .unwrap();

        assert!((ionic - (0.86 + 1.26 + 0.2)).abs() < 1.0e-12);
        assert!((covalent - (1.41 + 0.66 + 0.2)).abs() < 1.0e-12);
    }

    #[test]
    fn edge_summaries_are_species_order_invariant() {
        let atoms = vec![
            TopologyAtom {
                species: "Mg".into(),
                coords: [0.0, 0.0, 0.0],
            },
            TopologyAtom {
                species: "O".into(),
                coords: [1.0, 0.0, 0.0],
            },
            TopologyAtom {
                species: "O".into(),
                coords: [0.0, 1.0, 0.0],
            },
        ];
        let edges = build_structure_edges(&atoms, 1.1);
        let pairs = summarize_edge_pairs(&atoms, &edges);
        let coord = summarize_coordination_histograms(&atoms, &edges);
        assert_eq!(pairs.get("Mg-O"), Some(&2));
        assert_eq!(coord.get("Mg").and_then(|h| h.get(&2)), Some(&1));
    }
}
