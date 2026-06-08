//! Provenance:
//! - This module is part of the faithful readaptation of the `Fingerprint2` environment
//!   overlap-matrix fingerprint method into Rust.
//! - Reference sources audited for this module:
//!   - `Fingerprint2/src/fingerprint.f90`
//! - This module now performs the explicit phase-1 environment fingerprint implementation:
//!   sphere construction, overlap-matrix assembly, amplitude weighting, and eigenvalue extraction
//!   for clusters and 3D periodic frameworks.
//! - Any intentional deviations should be documented in
//!   `docs/FINGERPRINT2_PERTURBER_CAMPAIGN_2026-04-16.md`.

use crate::{EnvironmentStructureFingerprintEngine, PerturberError};
use nalgebra::{DMatrix, Matrix3, SymmetricEigen, Vector3};
use patina_sci_kernel::{
    fractional_to_cartesian, Cluster0D, CoordinateBasis, Framework3D, StructureLike,
};
use serde::{Deserialize, Serialize};

const ANGSTROM_TO_BOHR: f64 = 1.889_726_125;
const NEX_CUTOFF: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentOverlapFingerprintConfig {
    pub width_cutoff: f64,
    pub max_atoms_in_sphere: usize,
    pub s_orbital_count: usize,
    pub p_orbital_count: usize,
}

impl Default for EnvironmentOverlapFingerprintConfig {
    fn default() -> Self {
        Self {
            width_cutoff: 5.0,
            max_atoms_in_sphere: 100,
            s_orbital_count: 1,
            p_orbital_count: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentFingerprintVector {
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentFingerprintSet {
    pub structure_label: String,
    pub fingerprint_length: usize,
    pub environment_count: usize,
    pub environments: Vec<EnvironmentFingerprintVector>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentSphereAtom {
    pub source_atom_index: usize,
    pub image_offset: [i32; 3],
    pub species: String,
    pub cartesian: [f64; 3],
    pub distance_sq: f64,
    pub amplitude: f64,
    pub derivative_amplitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentSphere {
    pub structure_label: String,
    pub center_atom_index: usize,
    pub central_sphere_index: usize,
    pub atoms: Vec<EnvironmentSphereAtom>,
}

#[derive(Debug, Clone)]
pub struct EnvironmentOverlapFingerprintEngine {
    pub config: EnvironmentOverlapFingerprintConfig,
}

impl EnvironmentOverlapFingerprintEngine {
    pub fn validate_config(&self) -> Result<(), PerturberError> {
        if !self.config.width_cutoff.is_finite() || self.config.width_cutoff <= 0.0 {
            return Err(PerturberError::InvalidCutoff(self.config.width_cutoff));
        }
        if self.config.max_atoms_in_sphere == 0 {
            return Err(PerturberError::InvalidSphereCapacity(
                self.config.max_atoms_in_sphere,
            ));
        }
        if self.config.s_orbital_count != 1 || self.config.p_orbital_count > 1 {
            return Err(PerturberError::UnsupportedEnvironmentBasis {
                s_orbitals: self.config.s_orbital_count,
                p_orbitals: self.config.p_orbital_count,
            });
        }
        Ok(())
    }

    pub fn build_cluster_spheres(
        &self,
        structure: &Cluster0D,
    ) -> Result<Vec<EnvironmentSphere>, PerturberError> {
        self.validate_config()?;
        let positions = cartesian_positions(structure)?;
        let species = structure
            .sites()
            .iter()
            .map(|site| site.species.as_str())
            .collect::<Vec<_>>();
        let mut spheres = Vec::with_capacity(positions.len());
        for center_atom_index in 0..positions.len() {
            spheres.push(self.build_sphere(
                structure.label(),
                &positions,
                None,
                &species,
                center_atom_index,
                0,
            )?);
        }
        Ok(spheres)
    }

    pub fn fingerprint_cluster(
        &self,
        structure: &Cluster0D,
    ) -> Result<EnvironmentFingerprintSet, PerturberError> {
        let spheres = self.build_cluster_spheres(structure)?;
        self.fingerprint_from_spheres(structure.label(), spheres)
    }

    pub fn build_framework_spheres(
        &self,
        structure: &Framework3D,
    ) -> Result<Vec<EnvironmentSphere>, PerturberError> {
        self.validate_config()?;
        let positions = cartesian_positions(structure)?;
        let lattice = structure
            .lattice()
            .ok_or_else(|| {
                PerturberError::UnsupportedStructureForEnvironmentFingerprint(
                    "framework structure is missing lattice".into(),
                )
            })?
            .basis;
        let species = structure
            .sites()
            .iter()
            .map(|site| site.species.as_str())
            .collect::<Vec<_>>();
        let ixyzmax = periodic_image_radius(lattice, self.radius_cutoff())?;
        let mut spheres = Vec::with_capacity(positions.len());
        for center_atom_index in 0..positions.len() {
            spheres.push(self.build_sphere(
                structure.label(),
                &positions,
                Some(lattice),
                &species,
                center_atom_index,
                ixyzmax,
            )?);
        }
        Ok(spheres)
    }

    pub fn fingerprint_framework(
        &self,
        structure: &Framework3D,
    ) -> Result<EnvironmentFingerprintSet, PerturberError> {
        let spheres = self.build_framework_spheres(structure)?;
        self.fingerprint_from_spheres(structure.label(), spheres)
    }

    fn build_sphere(
        &self,
        structure_label: &str,
        positions: &[[f64; 3]],
        lattice: Option<[[f64; 3]; 3]>,
        species: &[&str],
        center_atom_index: usize,
        ixyzmax: i32,
    ) -> Result<EnvironmentSphere, PerturberError> {
        let mut atoms = Vec::new();
        let radius_cutoff_sq = self.radius_cutoff().powi(2);
        let factor_cutoff =
            1.0 / (2.0 * NEX_CUTOFF as f64 * self.config.width_cutoff * self.config.width_cutoff);
        let center = vector_from_array(positions[center_atom_index]);
        let a = lattice.map(|basis| vector_from_array(basis[0]));
        let b = lattice.map(|basis| vector_from_array(basis[1]));
        let c = lattice.map(|basis| vector_from_array(basis[2]));
        let mut central_sphere_index = None;

        for source_atom_index in 0..positions.len() {
            let base = vector_from_array(positions[source_atom_index]);
            for ix in -ixyzmax..=ixyzmax {
                for iy in -ixyzmax..=ixyzmax {
                    for iz in -ixyzmax..=ixyzmax {
                        let translated = base
                            + a.unwrap_or_else(Vector3::zeros) * ix as f64
                            + b.unwrap_or_else(Vector3::zeros) * iy as f64
                            + c.unwrap_or_else(Vector3::zeros) * iz as f64;
                        let delta = translated - center;
                        let distance_sq = delta.dot(&delta);
                        if distance_sq > radius_cutoff_sq {
                            continue;
                        }
                        if atoms.len() >= self.config.max_atoms_in_sphere {
                            return Err(PerturberError::EnvironmentSphereOverflow {
                                capacity: self.config.max_atoms_in_sphere,
                            });
                        }
                        let attenuation = 1.0 - distance_sq * factor_cutoff;
                        let temp = attenuation.powi((NEX_CUTOFF - 1) as i32);
                        let amplitude = temp * attenuation;
                        let derivative_amplitude = -2.0 * factor_cutoff * NEX_CUTOFF as f64 * temp;
                        atoms.push(EnvironmentSphereAtom {
                            source_atom_index,
                            image_offset: [ix, iy, iz],
                            species: species[source_atom_index].to_string(),
                            cartesian: [translated.x, translated.y, translated.z],
                            distance_sq,
                            amplitude,
                            derivative_amplitude,
                        });
                        if source_atom_index == center_atom_index && ix == 0 && iy == 0 && iz == 0 {
                            central_sphere_index = Some(atoms.len() - 1);
                        }
                    }
                }
            }
        }

        Ok(EnvironmentSphere {
            structure_label: structure_label.to_string(),
            center_atom_index,
            central_sphere_index: central_sphere_index.ok_or_else(|| {
                PerturberError::InvalidCandidate(format!(
                    "failed to locate central atom image for environment {center_atom_index}"
                ))
            })?,
            atoms,
        })
    }

    fn radius_cutoff(&self) -> f64 {
        (2.0 * NEX_CUTOFF as f64).sqrt() * self.config.width_cutoff
    }

    fn fingerprint_from_spheres(
        &self,
        structure_label: &str,
        spheres: Vec<EnvironmentSphere>,
    ) -> Result<EnvironmentFingerprintSet, PerturberError> {
        let fingerprint_length = self.config.max_atoms_in_sphere
            * (self.config.s_orbital_count + 3 * self.config.p_orbital_count);
        let environments = spheres
            .iter()
            .map(|sphere| {
                Ok(EnvironmentFingerprintVector {
                    values: self.compute_environment_fingerprint(sphere, fingerprint_length)?,
                })
            })
            .collect::<Result<Vec<_>, PerturberError>>()?;
        Ok(EnvironmentFingerprintSet {
            structure_label: structure_label.to_string(),
            fingerprint_length,
            environment_count: environments.len(),
            environments,
        })
    }

    fn compute_environment_fingerprint(
        &self,
        sphere: &EnvironmentSphere,
        fingerprint_length: usize,
    ) -> Result<Vec<f64>, PerturberError> {
        let nat = sphere.atoms.len();
        let orbitals_per_atom = self.config.s_orbital_count + 3 * self.config.p_orbital_count;
        let norb = nat * orbitals_per_atom;
        let mut overlap = DMatrix::<f64>::zeros(norb, norb);
        let atoms = sphere
            .atoms
            .iter()
            .map(|atom| EnvironmentAtomData {
                pos: vector_from_array(atom.cartesian) * ANGSTROM_TO_BOHR,
                rcov: fingerprint2_covalent_radius_bohr(&atom.species).unwrap_or(1.0),
                amplitude: atom.amplitude,
            })
            .collect::<Vec<_>>();

        fill_env_ss_block(&atoms, orbitals_per_atom, &mut overlap);
        if self.config.p_orbital_count == 1 {
            fill_env_sp_and_ps_blocks(&atoms, orbitals_per_atom, &mut overlap);
            fill_env_pp_block(&atoms, orbitals_per_atom, &mut overlap);
        }
        apply_amplitudes(&atoms, orbitals_per_atom, &mut overlap);
        symmetrize_from_lower_triangle(&mut overlap);

        let eig = SymmetricEigen::new(overlap);
        let mut desc = eig.eigenvalues.as_slice().to_vec();
        desc.sort_by(|left, right| right.partial_cmp(left).unwrap_or(std::cmp::Ordering::Equal));
        let mut values = vec![0.0; fingerprint_length];
        values[..norb].copy_from_slice(&desc[..norb]);
        Ok(values)
    }
}

impl EnvironmentStructureFingerprintEngine<Cluster0D> for EnvironmentOverlapFingerprintEngine {
    fn fingerprint_environments(
        &self,
        structure: &Cluster0D,
    ) -> Result<EnvironmentFingerprintSet, PerturberError> {
        self.fingerprint_cluster(structure)
    }
}

impl EnvironmentStructureFingerprintEngine<Framework3D> for EnvironmentOverlapFingerprintEngine {
    fn fingerprint_environments(
        &self,
        structure: &Framework3D,
    ) -> Result<EnvironmentFingerprintSet, PerturberError> {
        self.fingerprint_framework(structure)
    }
}

#[derive(Debug, Clone, Copy)]
struct EnvironmentAtomData {
    pos: Vector3<f64>,
    rcov: f64,
    amplitude: f64,
}

fn cartesian_positions<S: StructureLike>(structure: &S) -> Result<Vec<[f64; 3]>, PerturberError> {
    let lattice = structure.lattice().map(|lattice| lattice.basis);
    structure
        .sites()
        .iter()
        .map(|site| {
            if site.species.trim().is_empty() {
                return Err(PerturberError::InvalidCandidate(
                    "environment fingerprint structure has blank species".into(),
                ));
            }
            if site.coords.iter().any(|value| !value.is_finite()) {
                return Err(PerturberError::InvalidCandidate(
                    "environment fingerprint structure has non-finite coordinates".into(),
                ));
            }
            let cartesian = match structure.coordinate_basis() {
                CoordinateBasis::Cartesian => site.coords,
                CoordinateBasis::Fractional => {
                    let lattice = lattice.ok_or_else(|| {
                        PerturberError::UnsupportedStructureForEnvironmentFingerprint(
                            "fractional coordinates require lattice".into(),
                        )
                    })?;
                    fractional_to_cartesian(lattice, site.coords)
                }
            };
            Ok(cartesian)
        })
        .collect()
}

fn periodic_image_radius(
    lattice: [[f64; 3]; 3],
    radius_cutoff: f64,
) -> Result<i32, PerturberError> {
    let gram = Matrix3::new(
        dot(lattice[0], lattice[0]),
        dot(lattice[0], lattice[1]),
        dot(lattice[0], lattice[2]),
        dot(lattice[1], lattice[0]),
        dot(lattice[1], lattice[1]),
        dot(lattice[1], lattice[2]),
        dot(lattice[2], lattice[0]),
        dot(lattice[2], lattice[1]),
        dot(lattice[2], lattice[2]),
    );
    let eigen = SymmetricEigen::new(gram);
    let smallest = eigen
        .eigenvalues
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    if !smallest.is_finite() || smallest <= 0.0 {
        return Err(
            PerturberError::UnsupportedStructureForEnvironmentFingerprint(
                "periodic lattice produced non-positive metric eigenvalue".into(),
            ),
        );
    }
    Ok((radius_cutoff / smallest.sqrt()).floor() as i32 + 1)
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn vector_from_array(value: [f64; 3]) -> Vector3<f64> {
    Vector3::new(value[0], value[1], value[2])
}

fn env_delta(left: &EnvironmentAtomData, right: &EnvironmentAtomData) -> (Vector3<f64>, f64) {
    let delta = left.pos - right.pos;
    let distance_sq = delta.dot(&delta);
    (delta, distance_sq)
}

fn gaussian_overlap_terms(
    left: &EnvironmentAtomData,
    right: &EnvironmentAtomData,
    distance_sq: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    let ai = 0.5 / (left.rcov * left.rcov);
    let aj = 0.5 / (right.rcov * right.rcov);
    let t1 = ai * aj;
    let t2 = ai + aj;
    let sij = (2.0 * t1.sqrt() / t2).sqrt().powi(3) * (-(t1 / t2) * distance_sq).exp();
    (ai, aj, t1, t2, sij, t1 / t2)
}

fn fill_env_ss_block(
    atoms: &[EnvironmentAtomData],
    orbitals_per_atom: usize,
    overlap: &mut DMatrix<f64>,
) {
    for j in 0..atoms.len() {
        for i in 0..atoms.len() {
            let (_, distance_sq) = env_delta(&atoms[i], &atoms[j]);
            let (_, _, _, _, sij, _) = gaussian_overlap_terms(&atoms[i], &atoms[j], distance_sq);
            overlap[(i * orbitals_per_atom, j * orbitals_per_atom)] = sij;
        }
    }
}

fn fill_env_sp_and_ps_blocks(
    atoms: &[EnvironmentAtomData],
    orbitals_per_atom: usize,
    overlap: &mut DMatrix<f64>,
) {
    let nat = atoms.len();
    for j in 0..nat {
        for i in 0..nat {
            let (delta, distance_sq) = env_delta(&atoms[i], &atoms[j]);
            let (ai, aj, _, t2, sij, _) = gaussian_overlap_terms(&atoms[i], &atoms[j], distance_sq);

            let i_s = i * orbitals_per_atom;
            let i_px = i_s + 1;
            let i_py = i_s + 2;
            let i_pz = i_s + 3;
            let j_s = j * orbitals_per_atom;
            let j_px = j_s + 1;
            let j_py = j_s + 2;
            let j_pz = j_s + 3;

            let t_pi_sj = -2.0 * ai.sqrt() * aj / t2;
            overlap[(i_px, j_s)] = t_pi_sj * delta.x * sij;
            overlap[(i_py, j_s)] = t_pi_sj * delta.y * sij;
            overlap[(i_pz, j_s)] = t_pi_sj * delta.z * sij;

            let t_si_pj = 2.0 * aj.sqrt() * ai / t2;
            overlap[(i_s, j_px)] = t_si_pj * delta.x * sij;
            overlap[(i_s, j_py)] = t_si_pj * delta.y * sij;
            overlap[(i_s, j_pz)] = t_si_pj * delta.z * sij;
        }
    }
}

fn fill_env_pp_block(
    atoms: &[EnvironmentAtomData],
    orbitals_per_atom: usize,
    overlap: &mut DMatrix<f64>,
) {
    let nat = atoms.len();
    for j in 0..nat {
        for i in 0..nat {
            let (delta, distance_sq) = env_delta(&atoms[i], &atoms[j]);
            let (_, _, t1, t2, sij, ratio) =
                gaussian_overlap_terms(&atoms[i], &atoms[j], distance_sq);
            let t4 = 2.0 * t1.sqrt() / t2;
            let t5 = -2.0 * ratio;

            let i_s = i * orbitals_per_atom;
            let i_px = i_s + 1;
            let i_py = i_s + 2;
            let i_pz = i_s + 3;
            let j_s = j * orbitals_per_atom;
            let j_px = j_s + 1;
            let j_py = j_s + 2;
            let j_pz = j_s + 3;

            overlap[(i_px, j_px)] = t4 * (1.0 + t5 * delta.x * delta.x) * sij;
            overlap[(i_py, j_px)] = t4 * (t5 * delta.y * delta.x) * sij;
            overlap[(i_pz, j_px)] = t4 * (t5 * delta.z * delta.x) * sij;
            overlap[(i_px, j_py)] = t4 * (t5 * delta.x * delta.y) * sij;
            overlap[(i_py, j_py)] = t4 * (1.0 + t5 * delta.y * delta.y) * sij;
            overlap[(i_pz, j_py)] = t4 * (t5 * delta.z * delta.y) * sij;
            overlap[(i_px, j_pz)] = t4 * (t5 * delta.x * delta.z) * sij;
            overlap[(i_py, j_pz)] = t4 * (t5 * delta.y * delta.z) * sij;
            overlap[(i_pz, j_pz)] = t4 * (1.0 + t5 * delta.z * delta.z) * sij;
        }
    }
}

fn apply_amplitudes(
    atoms: &[EnvironmentAtomData],
    orbitals_per_atom: usize,
    overlap: &mut DMatrix<f64>,
) {
    for jat in 0..atoms.len() {
        for j in 0..orbitals_per_atom {
            let jorb = jat * orbitals_per_atom + j;
            for iat in 0..atoms.len() {
                for i in 0..orbitals_per_atom {
                    let iorb = iat * orbitals_per_atom + i;
                    overlap[(iorb, jorb)] *= atoms[iat].amplitude * atoms[jat].amplitude;
                }
            }
        }
    }
}

fn symmetrize_from_lower_triangle(overlap: &mut DMatrix<f64>) {
    for row in 0..overlap.nrows() {
        for col in (row + 1)..overlap.ncols() {
            overlap[(row, col)] = overlap[(col, row)];
        }
    }
}

fn fingerprint2_covalent_radius_bohr(symbol: &str) -> Option<f64> {
    let angstrom = match symbol.trim() {
        "H" => 0.37,
        "He" => 0.32,
        "Li" => 1.34,
        "Be" => 0.90,
        "B" => 0.82,
        "C" => 0.77,
        "N" => 0.75,
        "O" => 0.73,
        "F" => 0.71,
        "Ne" => 0.69,
        "Na" => 1.54,
        "Mg" => 1.30,
        "Al" => 1.18,
        "Si" => 1.11,
        "P" => 1.06,
        "S" => 1.02,
        "Cl" => 0.99,
        "Ar" => 0.97,
        "K" => 1.96,
        "Ca" => 1.74,
        "Sc" => 1.44,
        "Ti" => 1.36,
        "V" => 1.25,
        "Cr" => 1.27,
        "Mn" => 1.39,
        "Fe" => 1.25,
        "Co" => 1.26,
        "Ni" => 1.21,
        "Cu" => 1.38,
        "Zn" => 1.31,
        "Ga" => 1.26,
        "Ge" => 1.22,
        "As" => 1.19,
        "Se" => 1.16,
        "Br" => 1.14,
        "Kr" => 1.10,
        "Rb" => 2.11,
        "Sr" => 1.92,
        "Y" => 1.62,
        "Zr" => 1.48,
        "Nb" => 1.37,
        "Mo" => 1.45,
        "Tc" => 1.56,
        "Ru" => 1.26,
        "Rh" => 1.35,
        "Pd" => 1.31,
        "Ag" => 1.53,
        "Cd" => 1.48,
        "In" => 1.44,
        "Sn" => 1.41,
        "Sb" => 1.38,
        "Te" => 1.35,
        "I" => 1.33,
        "Xe" => 1.30,
        "Cs" => 2.25,
        "Ba" => 1.98,
        "La" => 1.69,
        "Lu" => 1.60,
        "Hf" => 1.50,
        "Ta" => 1.38,
        "W" => 1.46,
        "Re" => 1.59,
        "Os" => 1.28,
        "Ir" => 1.37,
        "Pt" => 1.28,
        "Au" => 1.44,
        "Hg" => 1.49,
        "Tl" => 1.48,
        "Pb" => 1.47,
        "Bi" => 1.46,
        "Rn" => 1.45,
        "LA" => 1.122462048309373,
        "LB" => 0.9877666025122482,
        _ => return None,
    };
    Some(angstrom / 0.529_177_208_59)
}

#[cfg(test)]
mod tests {
    use super::*;
    use patina_sci_kernel::{Framework3D, Site, StructureMetadata};

    fn cluster() -> Cluster0D {
        Cluster0D::new(
            StructureMetadata {
                label: "cluster".into(),
            },
            vec![
                Site {
                    species: "Mg".into(),
                    coords: [0.0, 0.0, 0.0],
                },
                Site {
                    species: "O".into(),
                    coords: [1.5, 0.0, 0.0],
                },
            ],
            CoordinateBasis::Cartesian,
            None,
        )
        .expect("cluster")
    }

    fn framework() -> Framework3D {
        Framework3D::new(
            StructureMetadata {
                label: "framework".into(),
            },
            vec![
                Site {
                    species: "Na".into(),
                    coords: [0.0, 0.0, 0.0],
                },
                Site {
                    species: "Cl".into(),
                    coords: [0.9, 0.0, 0.0],
                },
            ],
            CoordinateBasis::Fractional,
            Some(patina_sci_kernel::Lattice3::new([
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ])),
        )
        .expect("framework")
    }

    #[test]
    fn cluster_sphere_contains_all_atoms_within_cutoff() {
        let engine = EnvironmentOverlapFingerprintEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 4,
                s_orbital_count: 1,
                p_orbital_count: 1,
            },
        };

        let spheres = engine.build_cluster_spheres(&cluster()).expect("spheres");
        assert_eq!(spheres.len(), 2);
        assert_eq!(spheres[0].atoms.len(), 2);
        assert_eq!(spheres[0].central_sphere_index, 0);
        assert_eq!(spheres[0].atoms[0].species, "Mg");
        assert_eq!(spheres[0].atoms[1].species, "O");
        assert!((spheres[0].atoms[0].amplitude - 1.0).abs() < 1.0e-12);
        assert!((spheres[0].atoms[1].distance_sq - 2.25).abs() < 1.0e-12);
        assert!((spheres[0].atoms[1].amplitude - 0.19140625).abs() < 1.0e-12);
    }

    #[test]
    fn framework_sphere_includes_nearest_periodic_image() {
        let engine = EnvironmentOverlapFingerprintEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 0.3,
                max_atoms_in_sphere: 8,
                s_orbital_count: 1,
                p_orbital_count: 1,
            },
        };

        let spheres = engine
            .build_framework_spheres(&framework())
            .expect("spheres");
        assert_eq!(spheres.len(), 2);
        assert_eq!(spheres[0].atoms.len(), 2);
        assert_eq!(spheres[0].atoms[1].source_atom_index, 1);
        assert_eq!(spheres[0].atoms[1].image_offset, [-1, 0, 0]);
        assert!((spheres[0].atoms[1].cartesian[0] + 0.1).abs() < 1.0e-12);
        assert!((spheres[0].atoms[1].distance_sq - 0.01).abs() < 1.0e-12);
    }

    #[test]
    fn sphere_builder_rejects_overflow() {
        let engine = EnvironmentOverlapFingerprintEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 1,
                s_orbital_count: 1,
                p_orbital_count: 1,
            },
        };

        let error = engine
            .build_cluster_spheres(&cluster())
            .expect_err("must reject overflow");
        assert!(matches!(
            error,
            PerturberError::EnvironmentSphereOverflow { capacity: 1 }
        ));
    }

    #[test]
    fn sphere_builder_rejects_unsupported_basis_request() {
        let engine = EnvironmentOverlapFingerprintEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 4,
                s_orbital_count: 2,
                p_orbital_count: 1,
            },
        };

        let error = engine
            .build_cluster_spheres(&cluster())
            .expect_err("must reject unsupported basis");
        assert!(matches!(
            error,
            PerturberError::UnsupportedEnvironmentBasis {
                s_orbitals: 2,
                p_orbitals: 1
            }
        ));
    }

    #[test]
    fn cluster_environment_fingerprint_has_fixed_padded_length() {
        let engine = EnvironmentOverlapFingerprintEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 4,
                s_orbital_count: 1,
                p_orbital_count: 0,
            },
        };

        let fps = engine.fingerprint_cluster(&cluster()).expect("fps");
        assert_eq!(fps.environment_count, 2);
        assert_eq!(fps.fingerprint_length, 4);
        assert_eq!(fps.environments[0].values.len(), 4);
        assert!(fps.environments[0].values[0] >= fps.environments[0].values[1]);
        assert_eq!(fps.environments[0].values[2], 0.0);
        assert_eq!(fps.environments[0].values[3], 0.0);
    }

    #[test]
    fn framework_environment_fingerprint_returns_periodic_result() {
        let engine = EnvironmentOverlapFingerprintEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 0.3,
                max_atoms_in_sphere: 8,
                s_orbital_count: 1,
                p_orbital_count: 1,
            },
        };

        let fps = engine.fingerprint_framework(&framework()).expect("fps");
        assert_eq!(fps.environment_count, 2);
        assert_eq!(fps.fingerprint_length, 32);
        assert_eq!(fps.environments[0].values.len(), 32);
        assert!(fps.environments[0].values[0].is_finite());
        assert!(fps.environments[0].values[1].is_finite());
    }
}
