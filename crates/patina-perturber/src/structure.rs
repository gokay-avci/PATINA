use crate::PerturberError;
use nalgebra::{Matrix3, SymmetricEigen, Vector3};
use patina_sci_kernel::codec::xyz::{read_xyz_frames, write_xyz_frames, AtomRecord, XyzFrame};
use patina_sci_kernel::{Cluster0D, Framework3D, Site, StructureError};
use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterAtom {
    pub species: String,
    pub cartesian: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterStructure {
    pub label: String,
    pub atoms: Vec<ClusterAtom>,
}

impl ClusterStructure {
    pub fn try_from_candidate(candidate: &Candidate) -> Result<Self, PerturberError> {
        let cluster = Cluster0D::try_from(candidate).map_err(map_cluster_structure_error)?;
        Ok(Self::from(&cluster))
    }

    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    pub fn validate(&self) -> Result<(), PerturberError> {
        if self.atoms.is_empty() {
            return Err(PerturberError::EmptyStructure);
        }
        for atom in &self.atoms {
            if atom.species.trim().is_empty() {
                return Err(PerturberError::InvalidCandidate(
                    "cluster atom has empty species label".into(),
                ));
            }
            if atom.cartesian.iter().any(|value| !value.is_finite()) {
                return Err(PerturberError::InvalidCandidate(
                    "cluster atom has non-finite coordinates".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn centroid(&self) -> Vector3<f64> {
        centroid(self)
    }

    pub fn centre_of_mass(&self) -> Vector3<f64> {
        centre_of_mass(self)
    }

    pub fn min_distance(&self) -> Option<f64> {
        min_interatomic_distance(self)
    }

    pub fn passes_min_distance(&self, dmin: f64) -> bool {
        passes_min_distance(self, dmin)
    }

    pub fn principal_axis_pca(&self) -> Result<Vector3<f64>, PerturberError> {
        principal_axis_pca(self)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

impl From<&Cluster0D> for ClusterStructure {
    fn from(value: &Cluster0D) -> Self {
        Self {
            label: value.metadata.label.clone(),
            atoms: value
                .sites
                .iter()
                .map(|site| ClusterAtom {
                    species: site.species.clone(),
                    cartesian: site.coords,
                })
                .collect(),
        }
    }
}

impl From<&ClusterStructure> for Candidate {
    fn from(value: &ClusterStructure) -> Self {
        Self::cluster(
            value.label.clone(),
            value
                .atoms
                .iter()
                .map(|atom| atom.species.clone())
                .collect(),
            value.atoms.iter().map(|atom| atom.cartesian).collect(),
        )
    }
}

impl From<&ClusterStructure> for Cluster0D {
    fn from(value: &ClusterStructure) -> Self {
        Cluster0D::new(
            patina_sci_kernel::StructureMetadata {
                label: value.label.clone(),
            },
            value
                .atoms
                .iter()
                .map(|atom| Site {
                    species: atom.species.clone(),
                    coords: atom.cartesian,
                })
                .collect(),
            patina_sci_kernel::CoordinateBasis::Cartesian,
            None,
        )
        .expect("validated cluster structure always maps to Cluster0D")
    }
}

fn map_cluster_structure_error(error: StructureError) -> PerturberError {
    match error {
        StructureError::UnexpectedPeriodicAxes { .. } => PerturberError::NonClusterCandidate,
        StructureError::UnexpectedLatticeForNonPeriodicStructure => {
            PerturberError::UnexpectedLattice
        }
        other => PerturberError::InvalidCandidate(format!("{other:?}")),
    }
}

pub fn cluster0d_from_candidate(candidate: &Candidate) -> Result<Cluster0D, PerturberError> {
    Cluster0D::try_from(candidate).map_err(map_cluster_structure_error)
}

pub fn framework_from_candidate(candidate: &Candidate) -> Result<Framework3D, PerturberError> {
    Framework3D::try_from(candidate).map_err(map_structure_error)
}

fn map_structure_error(error: StructureError) -> PerturberError {
    match error {
        StructureError::UnexpectedLatticeForNonPeriodicStructure => {
            PerturberError::UnexpectedLattice
        }
        other => PerturberError::InvalidCandidate(format!("{other:?}")),
    }
}

pub fn read_xyz_multi(path: impl AsRef<Path>) -> Result<Vec<ClusterStructure>, PerturberError> {
    let path = path.as_ref();
    let frames = read_xyz_frames(path).map_err(|err| {
        PerturberError::InvalidCandidate(format!(
            "failed to decode XYZ file `{}`: {err:?}",
            path.display()
        ))
    })?;
    let structures = frames
        .into_iter()
        .map(|frame| ClusterStructure {
            label: frame.comment.trim_end().to_string(),
            atoms: frame
                .atoms
                .into_iter()
                .map(|atom| ClusterAtom {
                    species: atom.species,
                    cartesian: atom.coords,
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    if structures.is_empty() {
        return Err(PerturberError::InvalidCandidate(format!(
            "no structures found in XYZ file `{}`",
            path.display()
        )));
    }

    Ok(structures)
}

pub fn read_xyz_single(path: impl AsRef<Path>) -> Result<ClusterStructure, PerturberError> {
    let mut structures = read_xyz_multi(path)?;
    Ok(structures.remove(0))
}

pub fn write_xyz_single(
    path: impl AsRef<Path>,
    structure: &ClusterStructure,
) -> Result<(), PerturberError> {
    let path = path.as_ref();
    structure.validate()?;
    write_xyz_frames(
        path,
        &[XyzFrame {
            atom_count: structure.atoms.len(),
            comment: structure.label.clone(),
            atoms: structure
                .atoms
                .iter()
                .map(|atom| AtomRecord {
                    species: atom.species.clone(),
                    coords: atom.cartesian,
                })
                .collect(),
        }],
    )
    .map_err(|err| {
        PerturberError::InvalidCandidate(format!(
            "failed to write XYZ file `{}`: {err:?}",
            path.display()
        ))
    })
}

pub fn write_xyz_multi(
    path: impl AsRef<Path>,
    structures: &[ClusterStructure],
) -> Result<(), PerturberError> {
    let path = path.as_ref();
    if structures.is_empty() {
        return Err(PerturberError::InvalidCandidate(
            "write_xyz_multi called with empty structures slice".into(),
        ));
    }
    let frames = structures
        .iter()
        .map(|structure| {
            structure.validate()?;
            Ok(XyzFrame {
                atom_count: structure.atoms.len(),
                comment: structure.label.clone(),
                atoms: structure
                    .atoms
                    .iter()
                    .map(|atom| AtomRecord {
                        species: atom.species.clone(),
                        coords: atom.cartesian,
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, PerturberError>>()?;
    write_xyz_frames(path, &frames).map_err(|err| {
        PerturberError::InvalidCandidate(format!(
            "failed to write XYZ file `{}`: {err:?}",
            path.display()
        ))
    })
}

pub fn write_xyz_files(
    out_dir: impl AsRef<Path>,
    prefix: &str,
    structures: &[ClusterStructure],
) -> Result<Vec<PathBuf>, PerturberError> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir).map_err(|err| {
        PerturberError::InvalidCandidate(format!(
            "failed to create output dir `{}`: {err}",
            out_dir.display()
        ))
    })?;
    let mut paths = Vec::with_capacity(structures.len());
    for (index, structure) in structures.iter().enumerate() {
        let path = out_dir.join(format!("{prefix}{index:04}.xyz"));
        write_xyz_single(&path, structure)?;
        paths.push(path);
    }
    Ok(paths)
}

pub fn passes_min_distance(structure: &ClusterStructure, dmin: f64) -> bool {
    if dmin <= 0.0 {
        return true;
    }
    let dmin_sq = dmin * dmin;
    for left in 0..structure.atoms.len() {
        let left_pos = Vector3::from(structure.atoms[left].cartesian);
        for right in (left + 1)..structure.atoms.len() {
            let right_pos = Vector3::from(structure.atoms[right].cartesian);
            if (left_pos - right_pos).norm_squared() < dmin_sq {
                return false;
            }
        }
    }
    true
}

pub fn centroid(structure: &ClusterStructure) -> Vector3<f64> {
    let count = structure.atoms.len() as f64;
    let mut sum = Vector3::new(0.0, 0.0, 0.0);
    for atom in &structure.atoms {
        sum += Vector3::from(atom.cartesian);
    }
    sum / count
}

pub fn centre_of_mass(structure: &ClusterStructure) -> Vector3<f64> {
    let mut weighted_sum = Vector3::new(0.0, 0.0, 0.0);
    let mut total_mass = 0.0;
    for atom in &structure.atoms {
        let mass = atomic_mass_approx(&atom.species);
        total_mass += mass;
        weighted_sum += Vector3::from(atom.cartesian) * mass;
    }
    if total_mass > 0.0 {
        weighted_sum / total_mass
    } else {
        centroid(structure)
    }
}

pub fn atomic_mass_approx(species: &str) -> f64 {
    match species.trim().to_ascii_uppercase().as_str() {
        "H" => 1.0079,
        "C" => 12.0107,
        "N" => 14.0067,
        "O" => 15.999,
        "F" => 18.998,
        "NA" => 22.989,
        "MG" => 24.305,
        "AL" => 26.982,
        "SI" => 28.085,
        "P" => 30.974,
        "S" => 32.06,
        "CL" => 35.45,
        "K" => 39.098,
        "CA" => 40.078,
        "TI" => 47.867,
        "V" => 50.942,
        "CR" => 51.996,
        "MN" => 54.938,
        "FE" => 55.845,
        "CO" => 58.933,
        "NI" => 58.693,
        "CU" => 63.546,
        "ZN" => 65.38,
        "ZR" => 91.224,
        "MO" => 95.95,
        "RU" => 101.07,
        "RH" => 102.91,
        "PD" => 106.42,
        "AG" => 107.87,
        "CD" => 112.41,
        "W" => 183.84,
        "IR" => 192.22,
        "PT" => 195.08,
        "AU" => 196.97,
        _ => 1.0,
    }
}

pub fn min_interatomic_distance(structure: &ClusterStructure) -> Option<f64> {
    let count = structure.atoms.len();
    if count < 2 {
        return None;
    }
    let mut best = f64::INFINITY;
    for left in 0..count {
        let left_pos = Vector3::from(structure.atoms[left].cartesian);
        for right in (left + 1)..count {
            let right_pos = Vector3::from(structure.atoms[right].cartesian);
            let distance = (left_pos - right_pos).norm();
            if distance < best {
                best = distance;
            }
        }
    }
    Some(best)
}

pub fn principal_axis_pca(structure: &ClusterStructure) -> Result<Vector3<f64>, PerturberError> {
    structure.validate()?;
    let center = centroid(structure);
    let mut covariance = Matrix3::zeros();
    for atom in &structure.atoms {
        let shifted = Vector3::from(atom.cartesian) - center;
        covariance += shifted * shifted.transpose();
    }

    let eigen = SymmetricEigen::new(covariance);
    let mut max_index = 0usize;
    let mut max_value = eigen.eigenvalues[0];
    for index in 1..3 {
        if eigen.eigenvalues[index] > max_value {
            max_value = eigen.eigenvalues[index];
            max_index = index;
        }
    }

    let axis = eigen.eigenvectors.column(max_index).into_owned();
    let norm = axis.norm();
    if !norm.is_finite() || norm == 0.0 {
        return Err(PerturberError::InvalidCandidate(
            "PCA axis is ill-defined (norm is zero or non-finite)".into(),
        ));
    }
    Ok(axis / norm)
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_mass_approx, centre_of_mass, centroid, read_xyz_multi, read_xyz_single,
        write_xyz_files, write_xyz_multi, write_xyz_single, ClusterAtom, ClusterStructure,
    };
    use std::fs;

    fn hetero_cluster() -> ClusterStructure {
        ClusterStructure {
            label: "hetero".into(),
            atoms: vec![
                ClusterAtom {
                    species: "Mg".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [10.0, 0.0, 0.0],
                },
            ],
        }
    }

    #[test]
    fn centre_of_mass_uses_species_weights() {
        let structure = hetero_cluster();
        let centroid = centroid(&structure);
        let center_of_mass = centre_of_mass(&structure);

        assert!((centroid.x - 5.0).abs() < 1.0e-12);
        assert!(center_of_mass.x < centroid.x);
        assert!((center_of_mass.x - 3.969_581_183_009_130_8).abs() < 1.0e-12);
    }

    #[test]
    fn unknown_species_mass_falls_back_to_unity() {
        assert!((atomic_mass_approx("Xx") - 1.0).abs() < 1.0e-12);
    }

    fn temp_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "patina_perturber_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn xyz_fixture_structures() -> Vec<ClusterStructure> {
        vec![
            ClusterStructure {
                label: "fixture_one".into(),
                atoms: vec![
                    ClusterAtom {
                        species: "Mg".into(),
                        cartesian: [0.0, 0.0, 0.0],
                    },
                    ClusterAtom {
                        species: "O".into(),
                        cartesian: [1.5, 0.0, 0.0],
                    },
                ],
            },
            ClusterStructure {
                label: "fixture_two".into(),
                atoms: vec![
                    ClusterAtom {
                        species: "Sr".into(),
                        cartesian: [0.1, 0.2, 0.3],
                    },
                    ClusterAtom {
                        species: "O".into(),
                        cartesian: [2.1, 2.2, 2.3],
                    },
                    ClusterAtom {
                        species: "O".into(),
                        cartesian: [-0.4, 1.4, 0.7],
                    },
                ],
            },
        ]
    }

    #[test]
    fn xyz_single_roundtrip_preserves_structure() {
        let dir = temp_test_dir("single_xyz");
        let path = dir.join("single.xyz");
        let structure = xyz_fixture_structures().remove(0);

        write_xyz_single(&path, &structure).expect("write xyz");
        let roundtrip = read_xyz_single(&path).expect("read xyz");

        assert_eq!(roundtrip, structure);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn xyz_multi_roundtrip_preserves_structure_order() {
        let dir = temp_test_dir("multi_xyz");
        let path = dir.join("multi.xyz");
        let structures = xyz_fixture_structures();

        write_xyz_multi(&path, &structures).expect("write multi xyz");
        let roundtrip = read_xyz_multi(&path).expect("read multi xyz");

        assert_eq!(roundtrip, structures);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn xyz_file_export_writes_numbered_outputs() {
        let dir = temp_test_dir("xyz_files");
        let out_dir = dir.join("outputs");
        let structures = xyz_fixture_structures();

        let paths = write_xyz_files(&out_dir, "cluster_", &structures).expect("write xyz files");

        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("cluster_0000.xyz"));
        assert!(paths[1].ends_with("cluster_0001.xyz"));
        let reread = read_xyz_single(&paths[1]).expect("read exported xyz");
        assert_eq!(reread, structures[1]);
        fs::remove_dir_all(dir).ok();
    }
}
