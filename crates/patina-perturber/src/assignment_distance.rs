//! Provenance:
//! - This module is part of the faithful readaptation of the `Fingerprint2` environment
//!   overlap-matrix fingerprint method into Rust.
//! - Reference sources audited for this module:
//!   - `Fingerprint2/src/fp_distance.f90`
//!   - `Fingerprint2/src/hung.f90`
//! - This phase-1 slice implements assignment-based distance over environment fingerprint sets as
//!   a separate lane from the existing whole-structure Euclidean distance path.
//! - Any intentional deviations should be documented in
//!   `docs/FINGERPRINT2_PERTURBER_CAMPAIGN_2026-04-16.md`.

use crate::distance::euclidean;
use crate::{
    EnvironmentFingerprintSet, EnvironmentOverlapFingerprintConfig,
    EnvironmentOverlapFingerprintEngine, EnvironmentStructureFingerprintEngine, PerturberError,
};
use patina_sci_kernel::{Cluster0D, Framework3D};
use serde::{Deserialize, Serialize};

pub type AssignmentCostMatrix = Vec<Vec<f64>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentFingerprintDistance {
    pub left_label: String,
    pub right_label: String,
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentSolution {
    pub assignment: Vec<usize>,
    pub cost: f64,
}

#[derive(Debug, Clone)]
pub struct AssignmentFingerprintDistanceEngine {
    pub config: EnvironmentOverlapFingerprintConfig,
}

impl AssignmentFingerprintDistanceEngine {
    pub fn distance_clusters(
        &self,
        left: &Cluster0D,
        right: &Cluster0D,
    ) -> Result<AssignmentFingerprintDistance, PerturberError> {
        let fp_engine = EnvironmentOverlapFingerprintEngine {
            config: self.config.clone(),
        };
        let left_fp = fp_engine.fingerprint_environments(left)?;
        let right_fp = fp_engine.fingerprint_environments(right)?;
        self.distance_sets(&left_fp, &right_fp)
    }

    pub fn distance_frameworks(
        &self,
        left: &Framework3D,
        right: &Framework3D,
    ) -> Result<AssignmentFingerprintDistance, PerturberError> {
        let fp_engine = EnvironmentOverlapFingerprintEngine {
            config: self.config.clone(),
        };
        let left_fp = fp_engine.fingerprint_environments(left)?;
        let right_fp = fp_engine.fingerprint_environments(right)?;
        self.distance_sets(&left_fp, &right_fp)
    }

    pub fn distance_sets(
        &self,
        left: &EnvironmentFingerprintSet,
        right: &EnvironmentFingerprintSet,
    ) -> Result<AssignmentFingerprintDistance, PerturberError> {
        let cost = build_assignment_cost_matrix(left, right)?;
        let solution = solve_assignment(&cost)?;
        Ok(AssignmentFingerprintDistance {
            left_label: left.structure_label.clone(),
            right_label: right.structure_label.clone(),
            distance: solution.cost,
        })
    }
}

pub trait EnvironmentAssignmentDistanceEngine<S> {
    fn distance_environments(
        &self,
        left: &S,
        right: &S,
    ) -> Result<AssignmentFingerprintDistance, PerturberError>;
}

impl EnvironmentAssignmentDistanceEngine<Cluster0D> for AssignmentFingerprintDistanceEngine {
    fn distance_environments(
        &self,
        left: &Cluster0D,
        right: &Cluster0D,
    ) -> Result<AssignmentFingerprintDistance, PerturberError> {
        self.distance_clusters(left, right)
    }
}

impl EnvironmentAssignmentDistanceEngine<Framework3D> for AssignmentFingerprintDistanceEngine {
    fn distance_environments(
        &self,
        left: &Framework3D,
        right: &Framework3D,
    ) -> Result<AssignmentFingerprintDistance, PerturberError> {
        self.distance_frameworks(left, right)
    }
}

pub fn build_assignment_cost_matrix(
    left: &EnvironmentFingerprintSet,
    right: &EnvironmentFingerprintSet,
) -> Result<AssignmentCostMatrix, PerturberError> {
    validate_environment_fingerprint_set(left)?;
    validate_environment_fingerprint_set(right)?;
    if left.environment_count != right.environment_count {
        return Err(PerturberError::AssignmentDimensionMismatch {
            left: left.environment_count,
            right: right.environment_count,
        });
    }

    let mut cost = vec![vec![0.0; right.environment_count]; left.environment_count];
    for (i, left_environment) in left.environments.iter().enumerate() {
        for (j, right_environment) in right.environments.iter().enumerate() {
            cost[i][j] = euclidean(&left_environment.values, &right_environment.values)?;
        }
    }
    Ok(cost)
}

pub fn solve_assignment(cost: &AssignmentCostMatrix) -> Result<AssignmentSolution, PerturberError> {
    validate_cost_matrix(cost)?;
    let n = cost.len();
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; n + 1];
    let mut p = vec![0usize; n + 1];
    let mut way = vec![0usize; n + 1];

    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![f64::INFINITY; n + 1];
        let mut used = vec![false; n + 1];

        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0usize;
            for j in 1..=n {
                if used[j] {
                    continue;
                }
                let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                if cur < minv[j] {
                    minv[j] = cur;
                    way[j] = j0;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    j1 = j;
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }

        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }

    let mut assignment = vec![0usize; n];
    for j in 1..=n {
        if p[j] == 0 {
            return Err(PerturberError::InvalidCandidate(
                "assignment solver returned incomplete matching".into(),
            ));
        }
        assignment[p[j] - 1] = j - 1;
    }

    let cost_sum = assignment
        .iter()
        .enumerate()
        .map(|(i, j)| cost[i][*j])
        .sum::<f64>();

    Ok(AssignmentSolution {
        assignment,
        cost: cost_sum,
    })
}

fn validate_environment_fingerprint_set(
    set: &EnvironmentFingerprintSet,
) -> Result<(), PerturberError> {
    if set.environment_count == 0 || set.environments.is_empty() {
        return Err(PerturberError::InvalidCandidate(
            "environment fingerprint set is empty".into(),
        ));
    }
    if set.environment_count != set.environments.len() {
        return Err(PerturberError::InvalidCandidate(format!(
            "environment fingerprint count mismatch: metadata={}, actual={}",
            set.environment_count,
            set.environments.len()
        )));
    }
    for (env_index, env) in set.environments.iter().enumerate() {
        if env.values.len() != set.fingerprint_length {
            return Err(PerturberError::FingerprintSizeMismatch {
                left: set.fingerprint_length,
                right: env.values.len(),
            });
        }
        for (value_index, value) in env.values.iter().enumerate() {
            if !value.is_finite() {
                return Err(PerturberError::InvalidCandidate(format!(
                    "environment fingerprint {env_index} has non-finite value at index {value_index}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_cost_matrix(cost: &AssignmentCostMatrix) -> Result<(), PerturberError> {
    if cost.is_empty() {
        return Err(PerturberError::InvalidCandidate(
            "assignment cost matrix is empty".into(),
        ));
    }
    let n = cost.len();
    for (row_index, row) in cost.iter().enumerate() {
        if row.len() != n {
            return Err(PerturberError::AssignmentDimensionMismatch {
                left: n,
                right: row.len(),
            });
        }
        for (col_index, value) in row.iter().enumerate() {
            if !value.is_finite() || *value < 0.0 {
                return Err(PerturberError::InvalidCandidate(format!(
                    "assignment cost matrix has invalid value at ({row_index},{col_index})"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use patina_sci_kernel::{CoordinateBasis, Site, StructureMetadata};

    fn env_set(label: &str, environments: &[&[f64]]) -> EnvironmentFingerprintSet {
        EnvironmentFingerprintSet {
            structure_label: label.into(),
            fingerprint_length: environments[0].len(),
            environment_count: environments.len(),
            environments: environments
                .iter()
                .map(|values| crate::EnvironmentFingerprintVector {
                    values: values.to_vec(),
                })
                .collect(),
        }
    }

    fn cluster(label: &str, second_x: f64) -> Cluster0D {
        Cluster0D::new(
            StructureMetadata {
                label: label.into(),
            },
            vec![
                Site {
                    species: "Mg".into(),
                    coords: [0.0, 0.0, 0.0],
                },
                Site {
                    species: "O".into(),
                    coords: [second_x, 0.0, 0.0],
                },
            ],
            CoordinateBasis::Cartesian,
            None,
        )
        .expect("cluster")
    }

    #[test]
    fn assignment_solver_finds_swapped_zero_cost_matching() {
        let left = env_set("left", &[&[0.0, 1.0], &[10.0, 11.0]]);
        let right = env_set("right", &[&[10.0, 11.0], &[0.0, 1.0]]);
        let cost = build_assignment_cost_matrix(&left, &right).expect("cost");
        let solution = solve_assignment(&cost).expect("solution");
        assert_eq!(solution.assignment, vec![1, 0]);
        assert!(solution.cost.abs() < 1.0e-12);
    }

    #[test]
    fn assignment_cost_matrix_rejects_mismatched_environment_counts() {
        let left = env_set("left", &[&[0.0], &[1.0]]);
        let right = env_set("right", &[&[0.0]]);
        let error = build_assignment_cost_matrix(&left, &right).expect_err("must reject mismatch");
        assert!(matches!(
            error,
            PerturberError::AssignmentDimensionMismatch { left: 2, right: 1 }
        ));
    }

    #[test]
    fn cluster_assignment_distance_is_zero_for_identical_structure() {
        let engine = AssignmentFingerprintDistanceEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 4,
                s_orbital_count: 1,
                p_orbital_count: 0,
            },
        };
        let left = cluster("left", 1.5);
        let right = cluster("right", 1.5);
        let distance = engine.distance_clusters(&left, &right).expect("distance");
        assert!(distance.distance.abs() < 1.0e-10);
    }

    #[test]
    fn cluster_assignment_distance_is_positive_for_distorted_structure() {
        let engine = AssignmentFingerprintDistanceEngine {
            config: EnvironmentOverlapFingerprintConfig {
                width_cutoff: 1.0,
                max_atoms_in_sphere: 4,
                s_orbital_count: 1,
                p_orbital_count: 0,
            },
        };
        let left = cluster("left", 1.5);
        let right = cluster("right", 1.8);
        let distance = engine.distance_clusters(&left, &right).expect("distance");
        assert!(distance.distance > 0.0);
    }
}
