// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/analysis/surface_bonds.rs`

use crate::domain::{
    SurfaceBondDiagnosticsSummary, SurfaceDanglingBondCandidate, SurfaceFace,
    SurfaceInterfaceError, SurfaceSlab,
};
use crate::graph::bonding::{build_bond_graph, BondingConfig};
use nalgebra::Vector3;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SurfaceBondReport {
    pub(crate) n_atoms: usize,
    pub(crate) z_skin: f64,
    pub(crate) z_min: f64,
    pub(crate) z_max: f64,
    pub(crate) bottom_indices: Vec<usize>,
    pub(crate) top_indices: Vec<usize>,
    pub(crate) dangling_candidates: Vec<DanglingAtom>,
    pub(crate) surface_stats: BTreeMap<String, (usize, f64)>,
    pub(crate) bonding: BondingConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DanglingAtom {
    pub(crate) index: usize,
    pub(crate) element: String,
    pub(crate) degree: usize,
    pub(crate) expected_min: usize,
    pub(crate) region: SurfaceRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceRegion {
    Bottom,
    Top,
}

impl SurfaceBondReport {
    pub(crate) fn to_summary(&self) -> SurfaceBondDiagnosticsSummary {
        SurfaceBondDiagnosticsSummary {
            n_atoms: self.n_atoms,
            z_skin_angstrom: self.z_skin,
            z_min: self.z_min,
            z_max: self.z_max,
            bottom_indices: self.bottom_indices.clone(),
            top_indices: self.top_indices.clone(),
            dangling_candidates: self
                .dangling_candidates
                .iter()
                .map(|candidate| SurfaceDanglingBondCandidate {
                    atom_index: candidate.index,
                    species: candidate.element.clone(),
                    degree: candidate.degree,
                    expected_min: candidate.expected_min,
                    region: match candidate.region {
                        SurfaceRegion::Bottom => SurfaceFace::Bottom,
                        SurfaceRegion::Top => SurfaceFace::Top,
                    },
                })
                .collect(),
            surface_stats: self.surface_stats.clone().into_iter().collect(),
        }
    }
}

pub(crate) fn analyse_surface_bonds_with_config(
    slab: &SurfaceSlab,
    bonding: BondingConfig,
    z_skin: f64,
) -> Result<SurfaceBondReport, SurfaceInterfaceError> {
    if !(z_skin.is_finite() && z_skin > 0.0) {
        return Err(SurfaceInterfaceError::GraphKernel(format!(
            "z_skin must be > 0 and finite, got {z_skin}"
        )));
    }

    let atom_count = slab.atoms.len();
    if atom_count == 0 {
        return Ok(SurfaceBondReport {
            n_atoms: 0,
            z_skin,
            z_min: 0.0,
            z_max: 0.0,
            bottom_indices: Vec::new(),
            top_indices: Vec::new(),
            dangling_candidates: Vec::new(),
            surface_stats: BTreeMap::new(),
            bonding,
        });
    }

    let c = Vector3::new(slab.lattice[0][2], slab.lattice[1][2], slab.lattice[2][2]);
    let c_hat = c
        .try_normalize(1.0e-15)
        .unwrap_or_else(|| Vector3::new(0.0, 0.0, 1.0));
    let z_values = slab
        .atoms
        .iter()
        .map(|atom| {
            Vector3::new(atom.cartesian[0], atom.cartesian[1], atom.cartesian[2]).dot(&c_hat)
        })
        .collect::<Vec<_>>();

    let z_min = z_values.iter().copied().fold(f64::INFINITY, f64::min);
    let z_max = z_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let bottom_threshold = z_min + z_skin;
    let top_threshold = z_max - z_skin;

    let mut bottom_indices = Vec::new();
    let mut top_indices = Vec::new();
    for (i, z) in z_values.iter().copied().enumerate() {
        if z <= bottom_threshold {
            bottom_indices.push(i);
        }
        if z >= top_threshold {
            top_indices.push(i);
        }
    }

    let graph = build_bond_graph(slab, &bonding)?;
    let mut surface_stats = BTreeMap::<String, (usize, f64)>::new();
    let mut dangling_candidates = Vec::<DanglingAtom>::new();
    let mut surface_mask = vec![false; atom_count];
    for &i in &bottom_indices {
        surface_mask[i] = true;
    }
    for &i in &top_indices {
        surface_mask[i] = true;
    }

    for (i, atom) in slab.atoms.iter().enumerate() {
        if !surface_mask[i] {
            continue;
        }
        let degree = graph.degree(i);
        let entry = surface_stats
            .entry(atom.species.clone())
            .or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += degree as f64;

        let (expected_min, _) = expected_surface_coordination_range(&atom.species);
        if degree < expected_min {
            let region = if z_values[i] <= bottom_threshold {
                SurfaceRegion::Bottom
            } else {
                SurfaceRegion::Top
            };
            dangling_candidates.push(DanglingAtom {
                index: i,
                element: atom.species.clone(),
                degree,
                expected_min,
                region,
            });
        }
    }

    for (count, avg) in surface_stats.values_mut() {
        if *count > 0 {
            *avg /= *count as f64;
        }
    }

    Ok(SurfaceBondReport {
        n_atoms: atom_count,
        z_skin,
        z_min,
        z_max,
        bottom_indices,
        top_indices,
        dangling_candidates,
        surface_stats,
        bonding,
    })
}

fn expected_surface_coordination_range(element: &str) -> (usize, usize) {
    match element {
        "H" => (1, 1),
        "C" => (1, 4),
        "N" => (1, 4),
        "O" => (1, 4),
        "Si" => (2, 4),
        "Al" => (2, 6),
        "P" => (1, 6),
        "S" => (1, 6),
        _ => (0, 12),
    }
}
