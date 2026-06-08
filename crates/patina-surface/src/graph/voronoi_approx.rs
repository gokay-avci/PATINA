// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/core/voronoi_approx.rs`
//
// This remains an approximation/scaffold layer rather than a true periodic
// Voronoi tessellation. The port keeps that status explicit.

use crate::domain::SurfaceSlab;
use crate::graph::slab_cartesian;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WeightConfig {
    pub(crate) r0: f64,
    pub(crate) occlusion: bool,
    pub(crate) occlusion_radius: f64,
    pub(crate) occlusion_strength: f64,
    pub(crate) covalent_soft: bool,
    pub(crate) covalent_sigma: f64,
    pub(crate) covalent_tol: f64,
    pub(crate) surface_boost: bool,
    pub(crate) surface_boost_factor: f64,
}

impl Default for WeightConfig {
    fn default() -> Self {
        Self {
            r0: 4.0,
            occlusion: true,
            occlusion_radius: 1.2,
            occlusion_strength: 0.35,
            covalent_soft: true,
            covalent_sigma: 0.35,
            covalent_tol: 0.30,
            surface_boost: true,
            surface_boost_factor: 1.15,
        }
    }
}

pub(crate) fn compute_weights_for_atom(
    slab: &SurfaceSlab,
    atom_index: usize,
    candidates: &[(usize, f64, Vector3<f64>)],
    surface_flag: bool,
    config: &WeightConfig,
) -> Vec<f64> {
    let mut weights = Vec::with_capacity(candidates.len());
    let atom_cart = slab_cartesian(slab, slab.atoms[atom_index].fractional);
    let r0 = config.r0.max(1.0e-6);

    for &(neighbor_index, distance, _) in candidates {
        let mut weight = (-(distance / r0).powi(2)).exp();

        if config.covalent_soft {
            let left = covalent_radius(&slab.atoms[atom_index].species).unwrap_or(0.77);
            let right = covalent_radius(&slab.atoms[neighbor_index].species).unwrap_or(0.77);
            let cutoff = left + right + config.covalent_tol;
            if distance > cutoff {
                let sigma = config.covalent_sigma.max(1.0e-6);
                let x = (distance - cutoff) / sigma;
                weight *= (-x * x).exp();
            }
        }

        if config.occlusion {
            let neighbor_cart = slab_cartesian(slab, slab.atoms[neighbor_index].fractional);
            let midpoint = 0.5 * (atom_cart + neighbor_cart);
            let threshold_sq = config.occlusion_radius.max(1.0e-6).powi(2);
            let mut occluders = 0usize;

            for (k, atom) in slab.atoms.iter().enumerate() {
                if k == atom_index || k == neighbor_index {
                    continue;
                }
                let other_cart = slab_cartesian(slab, atom.fractional);
                if (other_cart - midpoint).norm_squared() < threshold_sq {
                    occluders += 1;
                    if occluders >= 6 {
                        break;
                    }
                }
            }

            if occluders > 0 {
                let strength = config.occlusion_strength.clamp(0.0, 1.0);
                weight *= (1.0 - strength).powi(occluders as i32);
            }
        }

        if config.surface_boost && surface_flag {
            weight *= config.surface_boost_factor.max(1.0);
        }

        weights.push(weight);
    }

    weights
}

pub(crate) fn covalent_radius(element: &str) -> Option<f64> {
    Some(match element {
        "H" => 0.31,
        "B" => 0.84,
        "C" => 0.76,
        "N" => 0.71,
        "O" => 0.66,
        "F" => 0.57,
        "Si" => 1.11,
        "P" => 1.07,
        "S" => 1.05,
        "Cl" => 1.02,
        "Br" => 1.20,
        "I" => 1.39,
        "Al" => 1.21,
        "Na" => 1.66,
        "K" => 2.03,
        "Mg" => 1.41,
        "Ca" => 1.76,
        "Ti" => 1.60,
        "V" => 1.53,
        "Cr" => 1.39,
        "Mn" => 1.39,
        "Fe" => 1.32,
        "Co" => 1.26,
        "Ni" => 1.24,
        "Cu" => 1.32,
        "Zn" => 1.22,
        "Zr" => 1.75,
        "Mo" => 1.54,
        "Ag" => 1.45,
        "Cd" => 1.44,
        _ => return None,
    })
}
