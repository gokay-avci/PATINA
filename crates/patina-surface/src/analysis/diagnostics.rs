// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/analysis/diagnostics.rs`

use crate::domain::{
    SurfaceDiagnosticsDataset, SurfaceInterfaceError, SurfaceSlab, SurfaceSuspiciousCoordination,
};
use crate::graph::bonding::{build_bond_graph, BondingConfig};
use std::collections::BTreeMap;

fn expected_coordination_range(element: &str) -> (usize, usize) {
    match element {
        "H" => (1, 1),
        "C" => (2, 4),
        "N" => (1, 4),
        "O" => (1, 3),
        "F" | "Cl" | "Br" | "I" => (1, 1),
        "Si" => (3, 4),
        "Al" => (3, 6),
        "P" => (2, 6),
        "S" => (1, 6),
        _ => (0, 12),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DiagnosticsReport {
    pub(crate) n_atoms: usize,
    pub(crate) n_bonds: usize,
    pub(crate) n_components: usize,
    pub(crate) largest_component: usize,
    pub(crate) element_counts: BTreeMap<String, usize>,
    pub(crate) element_degrees: BTreeMap<String, (usize, usize, f64)>,
    pub(crate) isolated_atoms: Vec<usize>,
    pub(crate) suspicious_atoms: Vec<(usize, String, usize, (usize, usize))>,
    pub(crate) bonding: BondingConfig,
}

impl DiagnosticsReport {
    pub(crate) fn to_surface_dataset(&self) -> SurfaceDiagnosticsDataset {
        SurfaceDiagnosticsDataset {
            n_atoms: self.n_atoms,
            n_bonds: self.n_bonds,
            n_components: self.n_components,
            largest_component: self.largest_component,
            element_counts: self.element_counts.clone().into_iter().collect(),
            element_degrees: self.element_degrees.clone().into_iter().collect(),
            isolated_atoms: self.isolated_atoms.clone(),
            suspicious_atoms: self
                .suspicious_atoms
                .iter()
                .map(|(atom_index, species, degree, expected_range)| {
                    SurfaceSuspiciousCoordination {
                        atom_index: *atom_index,
                        species: species.clone(),
                        degree: *degree,
                        expected_range: *expected_range,
                    }
                })
                .collect(),
        }
    }
}

pub(crate) fn run_diagnostics_with_config(
    slab: &SurfaceSlab,
    bonding: BondingConfig,
) -> Result<DiagnosticsReport, SurfaceInterfaceError> {
    let graph = build_bond_graph(slab, &bonding)?;
    let components = graph.connected_components();

    let mut element_counts = BTreeMap::<String, usize>::new();
    let mut element_degree_lists = BTreeMap::<String, Vec<usize>>::new();
    let mut isolated_atoms = Vec::new();
    let mut suspicious_atoms = Vec::new();

    for (i, atom) in slab.atoms.iter().enumerate() {
        let element = atom.species.clone();
        *element_counts.entry(element.clone()).or_default() += 1;

        let degree = graph.degree(i);
        element_degree_lists
            .entry(element.clone())
            .or_default()
            .push(degree);

        if degree == 0 {
            isolated_atoms.push(i);
        }

        let (lo, hi) = expected_coordination_range(&element);
        if (degree < lo || degree > hi) && !(lo == 0 && hi >= 12) {
            suspicious_atoms.push((i, element, degree, (lo, hi)));
        }
    }

    let mut element_degrees = BTreeMap::<String, (usize, usize, f64)>::new();
    for (element, degrees) in element_degree_lists {
        let Some((&first, rest)) = degrees.split_first() else {
            continue;
        };
        let mut min_degree = first;
        let mut max_degree = first;
        for &degree in rest {
            min_degree = min_degree.min(degree);
            max_degree = max_degree.max(degree);
        }
        let avg_degree = degrees.iter().map(|&v| v as f64).sum::<f64>() / degrees.len() as f64;
        element_degrees.insert(element, (min_degree, max_degree, avg_degree));
    }

    Ok(DiagnosticsReport {
        n_atoms: slab.atoms.len(),
        n_bonds: graph.edges.len(),
        n_components: components.len(),
        largest_component: components.iter().map(|c| c.len()).max().unwrap_or(0),
        element_counts,
        element_degrees,
        isolated_atoms,
        suspicious_atoms,
        bonding,
    })
}
