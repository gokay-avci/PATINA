use crate::assignment_distance::AssignmentFingerprintDistanceEngine;
use crate::fingerprint::{FingerprintVector, OverlapMatrixFingerprintEngine};
use crate::structure::ClusterStructure;
use crate::{
    DuplicateScreeningEngine, EnvironmentOverlapFingerprintConfig, PerturberError,
    StructureFingerprintEngine,
};
use patina_sci_kernel::Cluster0D;
use serde::{Deserialize, Serialize};

pub type DistanceMatrix = Vec<Vec<f64>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FingerprintDistance {
    pub left_label: String,
    pub right_label: String,
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateDecision {
    pub left_label: String,
    pub right_label: String,
    pub distance: f64,
    pub threshold: f64,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FingerprintDuplicateScreeningEngine {
    pub include_p_orbitals: bool,
}

impl DuplicateScreeningEngine for FingerprintDuplicateScreeningEngine {
    fn distance(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
    ) -> Result<FingerprintDistance, PerturberError> {
        let fingerprint_engine = OverlapMatrixFingerprintEngine {
            include_p_orbitals: self.include_p_orbitals,
        };
        let left_fp = fingerprint_engine.fingerprint(left)?;
        let right_fp = fingerprint_engine.fingerprint(right)?;
        let distance = euclidean(&left_fp.values, &right_fp.values)?;
        Ok(FingerprintDistance {
            left_label: left.label.clone(),
            right_label: right.label.clone(),
            distance,
        })
    }
}

#[derive(Debug, Clone)]
pub struct EnvironmentAssignmentDuplicateScreeningEngine {
    pub config: EnvironmentOverlapFingerprintConfig,
}

impl DuplicateScreeningEngine for EnvironmentAssignmentDuplicateScreeningEngine {
    fn distance(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
    ) -> Result<FingerprintDistance, PerturberError> {
        left.validate()?;
        right.validate()?;
        let assignment_engine = AssignmentFingerprintDistanceEngine {
            config: self.config.clone(),
        };
        let distance =
            assignment_engine.distance_clusters(&Cluster0D::from(left), &Cluster0D::from(right))?;
        Ok(FingerprintDistance {
            left_label: distance.left_label,
            right_label: distance.right_label,
            distance: distance.distance,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateScreeningMode {
    #[default]
    GlobalOverlap,
    EnvironmentAssignment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateScreeningConfig {
    pub mode: DuplicateScreeningMode,
    pub include_p_orbitals: bool,
    pub environment: EnvironmentOverlapFingerprintConfig,
}

impl Default for DuplicateScreeningConfig {
    fn default() -> Self {
        Self {
            mode: DuplicateScreeningMode::GlobalOverlap,
            include_p_orbitals: false,
            environment: EnvironmentOverlapFingerprintConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfiguredDuplicateScreeningEngine {
    pub config: DuplicateScreeningConfig,
}

impl DuplicateScreeningEngine for ConfiguredDuplicateScreeningEngine {
    fn distance(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
    ) -> Result<FingerprintDistance, PerturberError> {
        match self.config.mode {
            DuplicateScreeningMode::GlobalOverlap => FingerprintDuplicateScreeningEngine {
                include_p_orbitals: self.config.include_p_orbitals,
            }
            .distance(left, right),
            DuplicateScreeningMode::EnvironmentAssignment => {
                EnvironmentAssignmentDuplicateScreeningEngine {
                    config: self.config.environment.clone(),
                }
                .distance(left, right)
            }
        }
    }
}

pub fn pairwise_distances(
    fingerprints: &[FingerprintVector],
) -> Result<DistanceMatrix, PerturberError> {
    validate_fingerprints(fingerprints)?;
    let mut matrix = vec![vec![0.0; fingerprints.len()]; fingerprints.len()];
    for (i, left_fingerprint) in fingerprints.iter().enumerate() {
        let (head, tail) = matrix.split_at_mut(i + 1);
        let row = &mut head[i];
        for (offset, right_fingerprint) in fingerprints.iter().skip(i + 1).enumerate() {
            let distance = euclidean(&left_fingerprint.values, &right_fingerprint.values)?;
            let j = i + 1 + offset;
            row[j] = distance;
            tail[offset][i] = distance;
        }
    }
    Ok(matrix)
}

pub fn distances_to_csv(matrix: &DistanceMatrix) -> Result<String, PerturberError> {
    validate_distance_matrix(matrix)?;
    let mut output = String::from("i,j,distance\n");
    for (i, row) in matrix.iter().enumerate() {
        for (j, distance) in row.iter().enumerate().skip(i + 1) {
            output.push_str(&format!("{i},{j},{distance:.16e}\n"));
        }
    }
    Ok(output)
}

pub(crate) fn euclidean(left: &[f64], right: &[f64]) -> Result<f64, PerturberError> {
    if left.len() != right.len() {
        return Err(PerturberError::FingerprintSizeMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    Ok(left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| {
            let delta = a - b;
            delta * delta
        })
        .sum::<f64>()
        .sqrt())
}

fn validate_distance_matrix(matrix: &DistanceMatrix) -> Result<(), PerturberError> {
    if matrix.is_empty() {
        return Err(PerturberError::InvalidCandidate(
            "distance matrix is empty".into(),
        ));
    }
    let expected = matrix.len();
    for (row_index, row) in matrix.iter().enumerate() {
        if row.len() != expected {
            return Err(PerturberError::InvalidCandidate(format!(
                "distance matrix is not square: row {row_index} has length {}, expected {expected}",
                row.len()
            )));
        }
        for (col_index, value) in row.iter().enumerate() {
            if !value.is_finite() {
                return Err(PerturberError::InvalidCandidate(format!(
                    "distance matrix has non-finite value at ({row_index},{col_index})"
                )));
            }
        }
    }
    Ok(())
}

fn validate_fingerprints(fingerprints: &[FingerprintVector]) -> Result<(), PerturberError> {
    if fingerprints.is_empty() {
        return Err(PerturberError::InvalidCandidate(
            "no fingerprints provided".into(),
        ));
    }
    let expected_dimension = fingerprints[0].values.len();
    if expected_dimension == 0 {
        return Err(PerturberError::InvalidCandidate(
            "fingerprints have zero length".into(),
        ));
    }
    for (fingerprint_index, fingerprint) in fingerprints.iter().enumerate() {
        if fingerprint.values.len() != expected_dimension {
            return Err(PerturberError::FingerprintSizeMismatch {
                left: expected_dimension,
                right: fingerprint.values.len(),
            });
        }
        for (value_index, value) in fingerprint.values.iter().enumerate() {
            if !value.is_finite() {
                return Err(PerturberError::InvalidCandidate(format!(
                    "fingerprint {fingerprint_index} has non-finite value at index {value_index}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ConfiguredDuplicateScreeningEngine, DistanceMatrix, DuplicateScreeningConfig,
        DuplicateScreeningMode, EnvironmentAssignmentDuplicateScreeningEngine,
        EnvironmentOverlapFingerprintConfig,
    };
    use crate::{
        distances_to_csv, pairwise_distances, ClusterAtom, ClusterStructure,
        DuplicateScreeningEngine, FingerprintVector,
    };

    fn simple_cluster(label: &str, second_x: f64) -> ClusterStructure {
        ClusterStructure {
            label: label.into(),
            atoms: vec![
                ClusterAtom {
                    species: "Mg".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [second_x, 0.0, 0.0],
                },
            ],
        }
    }

    #[test]
    fn pairwise_distances_are_symmetric() {
        let fingerprints = vec![
            FingerprintVector {
                values: vec![0.0, 1.0],
            },
            FingerprintVector {
                values: vec![3.0, 5.0],
            },
        ];
        let distances = pairwise_distances(&fingerprints).expect("distances");
        assert_eq!(distances[0][0], 0.0);
        assert_eq!(distances[1][1], 0.0);
        assert_eq!(distances[0][1], distances[1][0]);
    }

    #[test]
    fn distances_to_csv_writes_upper_triangle_rows() {
        let matrix: DistanceMatrix = vec![
            vec![0.0, 1.5, 2.5],
            vec![1.5, 0.0, 3.5],
            vec![2.5, 3.5, 0.0],
        ];
        let csv = distances_to_csv(&matrix).expect("csv");
        assert_eq!(
            csv,
            "i,j,distance\n0,1,1.5000000000000000e0\n0,2,2.5000000000000000e0\n1,2,3.5000000000000000e0\n"
        );
    }

    #[test]
    fn environment_assignment_duplicate_screening_reports_zero_for_identical_cluster() {
        let structure = simple_cluster("cluster", 1.5);
        let engine = EnvironmentAssignmentDuplicateScreeningEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 8,
                s_orbital_count: 1,
                p_orbital_count: 0,
            },
        };
        let distance = engine.distance(&structure, &structure).expect("distance");
        assert!(distance.distance.abs() < 1.0e-10);
    }

    #[test]
    fn configured_duplicate_screening_dispatches_to_environment_assignment_mode() {
        let left = simple_cluster("left", 1.5);
        let right = simple_cluster("right", 1.8);
        let engine = ConfiguredDuplicateScreeningEngine {
            config: DuplicateScreeningConfig {
                mode: DuplicateScreeningMode::EnvironmentAssignment,
                include_p_orbitals: false,
                environment: EnvironmentOverlapFingerprintConfig {
                    width_cutoff: 1.0,
                    max_atoms_in_sphere: 8,
                    s_orbital_count: 1,
                    p_orbital_count: 0,
                },
            },
        };
        let distance = engine.distance(&left, &right).expect("distance");
        assert!(distance.distance > 0.0);
    }
}
