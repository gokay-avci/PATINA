use crate::structure::ClusterStructure;
use crate::{PerturberError, StructureFingerprintEngine};
use nalgebra::{DMatrix, SymmetricEigen, Vector3};
use serde::{Deserialize, Serialize};

const ANGSTROM_TO_BOHR: f64 = 1.889_726_125;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FingerprintVector {
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlapMatrixFingerprintEngine {
    pub include_p_orbitals: bool,
}

impl StructureFingerprintEngine for OverlapMatrixFingerprintEngine {
    fn fingerprint(
        &self,
        structure: &ClusterStructure,
    ) -> Result<FingerprintVector, PerturberError> {
        structure.validate()?;
        let values = compute_overlap_fingerprint(structure, self.include_p_orbitals)?;
        Ok(FingerprintVector { values })
    }
}

pub(crate) fn compute_overlap_fingerprint(
    structure: &ClusterStructure,
    include_p_orbitals: bool,
) -> Result<Vec<f64>, PerturberError> {
    if structure.is_empty() {
        return Err(PerturberError::EmptyStructure);
    }
    let atom_count = structure.atom_count();
    let dimension = if include_p_orbitals {
        atom_count * 4
    } else {
        atom_count
    };
    let mut overlap = DMatrix::<f64>::zeros(dimension, dimension);
    let atoms = structure
        .atoms
        .iter()
        .map(|atom| AtomData {
            pos: Vector3::from(atom.cartesian) * ANGSTROM_TO_BOHR,
            rcov: covalent_radius_bohr(&atom.species).unwrap_or(1.0),
        })
        .collect::<Vec<_>>();

    fill_ss_block(&atoms, atom_count, &mut overlap);

    if include_p_orbitals {
        fill_sp_and_ps_blocks(&atoms, atom_count, &mut overlap);
        fill_pp_block(&atoms, atom_count, &mut overlap);
    }

    for row in 0..overlap.nrows() {
        for col in (row + 1)..overlap.ncols() {
            let value = 0.5 * (overlap[(row, col)] + overlap[(col, row)]);
            overlap[(row, col)] = value;
            overlap[(col, row)] = value;
        }
    }

    let eig = SymmetricEigen::new(overlap);
    let mut values = eig.eigenvalues.as_slice().to_vec();
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    Ok(values)
}

#[derive(Debug, Clone, Copy)]
struct AtomData {
    pos: Vector3<f64>,
    rcov: f64,
}

fn delta(left: &AtomData, right: &AtomData) -> (Vector3<f64>, f64) {
    let delta = right.pos - left.pos;
    let distance_sq = delta.dot(&delta);
    (delta, distance_sq)
}

fn s_overlap_base(left: &AtomData, right: &AtomData, distance_sq: f64) -> (f64, f64) {
    let denom = left.rcov * left.rcov + right.rcov * right.rcov;
    let r = 0.5 / denom;
    let factor = (4.0 * r * (left.rcov * right.rcov)).sqrt();
    let s = factor.powi(3) * (-distance_sq * r).exp();
    (r, s)
}

fn fill_ss_block(atoms: &[AtomData], atom_count: usize, overlap: &mut DMatrix<f64>) {
    for i in 0..atom_count {
        for j in i..atom_count {
            let (_, distance_sq) = delta(&atoms[i], &atoms[j]);
            let (_, s) = s_overlap_base(&atoms[i], &atoms[j], distance_sq);
            add_sym(overlap, i, j, s);
        }
    }
}

fn fill_sp_and_ps_blocks(atoms: &[AtomData], atom_count: usize, overlap: &mut DMatrix<f64>) {
    let sqrt8 = 8.0_f64.sqrt();
    for i in 0..atom_count {
        for j in 0..atom_count {
            if i == j {
                continue;
            }

            let (delta_vec, distance_sq) = delta(&atoms[i], &atoms[j]);
            let (r, sji) = s_overlap_base(&atoms[i], &atoms[j], distance_sq);

            let tj = sqrt8 * atoms[j].rcov * r * sji;
            let ti = -sqrt8 * atoms[i].rcov * r * sji;

            let source_s = i;
            let target_s = j;

            let j_px = atom_count + 3 * j;
            let j_py = atom_count + 3 * j + 1;
            let j_pz = atom_count + 3 * j + 2;

            let i_px = atom_count + 3 * i;
            let i_py = atom_count + 3 * i + 1;
            let i_pz = atom_count + 3 * i + 2;

            add_sym(overlap, j_px, source_s, tj * delta_vec.x);
            add_sym(overlap, j_py, source_s, tj * delta_vec.y);
            add_sym(overlap, j_pz, source_s, tj * delta_vec.z);

            add_sym(overlap, i_px, target_s, ti * delta_vec.x);
            add_sym(overlap, i_py, target_s, ti * delta_vec.y);
            add_sym(overlap, i_pz, target_s, ti * delta_vec.z);
        }
    }
}

fn fill_pp_block(atoms: &[AtomData], atom_count: usize, overlap: &mut DMatrix<f64>) {
    for i in 0..atom_count {
        for j in i..atom_count {
            let (delta_vec, distance_sq) = delta(&atoms[i], &atoms[j]);
            let (r, sji) = s_overlap_base(&atoms[i], &atoms[j], distance_sq);
            let tt = -8.0 * atoms[i].rcov * atoms[j].rcov * r * r * sji;
            let half_over_r = 0.5 / r;

            let dx = delta_vec.x;
            let dy = delta_vec.y;
            let dz = delta_vec.z;

            let ix = atom_count + 3 * i;
            let iy = atom_count + 3 * i + 1;
            let iz = atom_count + 3 * i + 2;
            let jx = atom_count + 3 * j;
            let jy = atom_count + 3 * j + 1;
            let jz = atom_count + 3 * j + 2;

            add_sym(overlap, jx, ix, tt * (dx * dx - half_over_r));
            add_sym(overlap, jx, iy, tt * (dx * dy));
            add_sym(overlap, jx, iz, tt * (dx * dz));

            add_sym(overlap, jy, ix, tt * (dx * dy));
            add_sym(overlap, jy, iy, tt * (dy * dy - half_over_r));
            add_sym(overlap, jy, iz, tt * (dy * dz));

            add_sym(overlap, jz, ix, tt * (dx * dz));
            add_sym(overlap, jz, iy, tt * (dy * dz));
            add_sym(overlap, jz, iz, tt * (dz * dz - half_over_r));
        }
    }
}

fn add_sym(overlap: &mut DMatrix<f64>, row: usize, col: usize, value: f64) {
    overlap[(row, col)] += value;
    if row != col {
        overlap[(col, row)] += value;
    }
}

pub(crate) fn covalent_radius_bohr(symbol: &str) -> Option<f64> {
    match symbol.trim() {
        "LJ" => Some(1.00),
        "H" => Some(0.75),
        "He" => Some(0.75),
        "Li" => Some(3.40),
        "Be" => Some(2.30),
        "B" => Some(1.55),
        "C" => Some(1.45),
        "N" => Some(1.42),
        "O" => Some(1.38),
        "F" => Some(1.35),
        "Ne" => Some(1.35),
        "Na" => Some(3.40),
        "Mg" => Some(2.65),
        "Al" => Some(2.23),
        "Si" => Some(2.09),
        "P" => Some(2.00),
        "S" => Some(1.92),
        "Cl" => Some(1.87),
        "Ar" => Some(1.80),
        "K" => Some(4.00),
        "Ca" => Some(3.80),
        "Sc" => Some(2.70),
        "Ti" => Some(2.70),
        "V" => Some(2.60),
        "Cr" => Some(2.60),
        "Mn" => Some(2.50),
        "Fe" => Some(2.50),
        "Co" => Some(2.40),
        "Ni" => Some(2.30),
        "Cu" => Some(2.80),
        "Zn" => Some(2.70),
        "Ga" => Some(2.40),
        "Ge" => Some(2.40),
        "As" => Some(2.30),
        "Se" => Some(2.30),
        "Br" => Some(2.20),
        "Kr" => Some(2.20),
        "Rb" => Some(4.50),
        "Sr" => Some(4.00),
        "Y" => Some(3.50),
        "Zr" => Some(3.00),
        "Nb" => Some(2.92),
        "Mo" => Some(2.83),
        "Tc" => Some(2.75),
        "Ru" => Some(2.67),
        "Rh" => Some(2.58),
        "Pd" => Some(2.50),
        "Ag" => Some(2.90),
        "Cd" => Some(2.80),
        "In" => Some(2.70),
        "Sn" => Some(2.66),
        "Sb" => Some(2.66),
        "Te" => Some(2.53),
        "I" => Some(2.50),
        "Xe" => Some(2.50),
        "Cs" => Some(4.50),
        "Ba" => Some(4.00),
        "La" => Some(3.50),
        "Ce" => Some(3.50),
        "Pr" => Some(3.44),
        "Nd" => Some(3.38),
        "Pm" => Some(3.33),
        "Sm" => Some(3.27),
        "Eu" => Some(3.21),
        "Gd" => Some(3.15),
        "Tb" => Some(3.09),
        "Dy" => Some(3.03),
        "Ho" => Some(2.97),
        "Er" => Some(2.92),
        "Tm" => Some(2.92),
        "Yb" => Some(2.80),
        "Lu" => Some(2.80),
        "Hf" => Some(2.90),
        "Ta" => Some(3.10),
        "W" => Some(2.60),
        "Re" => Some(2.60),
        "Os" => Some(2.50),
        "Ir" => Some(2.60),
        "Pt" => Some(2.60),
        "Au" => Some(4.00),
        "Hg" => Some(3.20),
        "Tl" => Some(3.20),
        "Pb" => Some(3.30),
        "Bi" => Some(2.90),
        "Po" => Some(2.80),
        "At" => Some(2.60),
        "Rn" => Some(2.60),
        _ => None,
    }
}
