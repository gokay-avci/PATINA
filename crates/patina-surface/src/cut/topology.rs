// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/analysis/topology.rs`
// - existing heuristic cut-selection logic previously embedded in `engine.rs`

use crate::domain::SurfaceParentStructure;
use crate::generation::LatticeOps;
use nalgebra::Vector3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SafeCut {
    pub(crate) offset_angstrom: f64,
    pub(crate) gap_size: f64,
    pub(crate) quality_score: f64,
}

pub(crate) fn find_safe_offsets(
    parent: &SurfaceParentStructure,
    lattice: &LatticeOps,
    surface_normal: Vector3<f64>,
) -> Vec<SafeCut> {
    let normal = match surface_normal.try_normalize(1.0e-15) {
        Some(normal) => normal,
        None => return Vec::new(),
    };

    let p_a = lattice.matrix.column(0).dot(&normal).abs();
    let p_b = lattice.matrix.column(1).dot(&normal).abs();
    let p_c = lattice.matrix.column(2).dot(&normal).abs();
    let periodicity = p_a.max(p_b).max(p_c);
    if periodicity <= 1.0e-12 {
        return Vec::new();
    }

    let mut intervals = Vec::<(f64, f64)>::new();
    for atom in &parent.atoms {
        let cart = lattice.to_cartesian(atom.fractional);
        let z = cart.dot(&normal).rem_euclid(periodicity);
        let r = vdw_radius(&atom.species);
        for shifted in [z - periodicity, z, z + periodicity] {
            intervals.push((shifted - r, shifted + r));
        }
    }
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    if intervals.is_empty() {
        return vec![SafeCut {
            offset_angstrom: 0.0,
            gap_size: 10.0,
            quality_score: 1.0,
        }];
    }

    let mut merged = Vec::<(f64, f64)>::new();
    let (mut current_start, mut current_end) = intervals[0];
    for &(next_start, next_end) in intervals.iter().skip(1) {
        if next_start < current_end {
            current_end = current_end.max(next_end);
        } else {
            merged.push((current_start, current_end));
            current_start = next_start;
            current_end = next_end;
        }
    }
    merged.push((current_start, current_end));

    let mut cuts = Vec::new();
    for window in merged.windows(2) {
        let gap_size = window[1].0 - window[0].1;
        if gap_size > 0.01 {
            let midpoint = window[0].1 + gap_size / 2.0;
            if midpoint >= 0.0 && midpoint <= periodicity {
                cuts.push(SafeCut {
                    offset_angstrom: midpoint,
                    gap_size,
                    quality_score: (gap_size / 3.0).min(1.0),
                });
            }
        }
    }
    cuts.sort_by(|left, right| {
        right
            .gap_size
            .total_cmp(&left.gap_size)
            .then_with(|| right.quality_score.total_cmp(&left.quality_score))
    });
    cuts
}

fn vdw_radius(element: &str) -> f64 {
    match element {
        "H" => 1.20,
        "He" => 1.40,
        "Li" => 1.82,
        "Be" => 1.53,
        "B" => 1.92,
        "C" => 1.70,
        "N" => 1.55,
        "O" => 1.52,
        "F" => 1.47,
        "Ne" => 1.54,
        "Na" => 2.27,
        "Mg" => 1.73,
        "Al" => 1.84,
        "Si" => 2.10,
        "P" => 1.80,
        "S" => 1.80,
        "Cl" => 1.75,
        "Ar" => 1.88,
        "K" => 2.75,
        "Ca" => 2.31,
        "Sc" => 2.11,
        "Ti" => 2.00,
        "V" => 2.00,
        "Cr" => 2.00,
        "Mn" => 2.00,
        "Fe" => 2.00,
        "Co" => 2.00,
        "Ni" => 1.63,
        "Cu" => 1.40,
        "Zn" => 1.39,
        "Ga" => 1.87,
        "Ge" => 2.11,
        "As" => 1.85,
        "Se" => 1.90,
        "Br" => 1.85,
        "Kr" => 2.02,
        "Rb" => 3.03,
        "Sr" => 2.49,
        "Pd" => 1.63,
        "Ag" => 1.72,
        "Cd" => 1.58,
        "In" => 1.93,
        "Sn" => 2.17,
        "Sb" => 2.06,
        "Te" => 2.06,
        "I" => 1.98,
        "Xe" => 2.16,
        "Cs" => 3.43,
        "Ba" => 2.68,
        "Pt" => 1.75,
        "Au" => 1.66,
        "Hg" => 1.55,
        "Tl" => 1.96,
        "Pb" => 2.02,
        "Bi" => 2.07,
        "Po" => 1.97,
        "At" => 2.02,
        "Rn" => 2.20,
        _ => 1.40,
    }
}
