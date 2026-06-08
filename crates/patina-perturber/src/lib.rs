#![forbid(unsafe_code)]

mod assignment_distance;
mod distance;
mod eliminate;
mod environment_fingerprint;
mod fingerprint;
mod perturb;
mod structure;
mod topology;

use thiserror::Error;

pub use assignment_distance::{
    build_assignment_cost_matrix, solve_assignment, AssignmentCostMatrix,
    AssignmentFingerprintDistance, AssignmentFingerprintDistanceEngine, AssignmentSolution,
    EnvironmentAssignmentDistanceEngine,
};
pub use distance::{
    distances_to_csv, pairwise_distances, ConfiguredDuplicateScreeningEngine, DistanceMatrix,
    DuplicateDecision, DuplicateScreeningConfig, DuplicateScreeningMode,
    EnvironmentAssignmentDuplicateScreeningEngine, FingerprintDistance,
    FingerprintDuplicateScreeningEngine,
};
pub use eliminate::eliminate_by_threshold;
pub use environment_fingerprint::{
    EnvironmentFingerprintSet, EnvironmentFingerprintVector, EnvironmentOverlapFingerprintConfig,
    EnvironmentOverlapFingerprintEngine, EnvironmentSphere, EnvironmentSphereAtom,
};
pub use fingerprint::{FingerprintVector, OverlapMatrixFingerprintEngine};
pub use perturb::{
    DefaultClusterPerturbationEngine, PerturbAxis, PerturbCentre, PerturbDistribution, PerturbMode,
    PerturbationBatch, PerturbationConfig,
};
pub use structure::{
    atomic_mass_approx, centre_of_mass, centroid, cluster0d_from_candidate,
    framework_from_candidate, min_interatomic_distance,
    passes_min_distance as cluster_passes_min_distance, principal_axis_pca, read_xyz_multi,
    read_xyz_single, write_xyz_files, write_xyz_multi, write_xyz_single, ClusterAtom,
    ClusterStructure,
};
pub use topology::{
    perturbation_topology_sweep, topology_sweep_csv, CutoffMarginSummary,
    PerturbationTopologyObservation, PerturbationTopologySweep, PerturbationTopologySweepRequest,
    TopologySweepBase,
};

#[derive(Debug, Error)]
pub enum PerturberError {
    #[error("candidate validation failed: {0}")]
    InvalidCandidate(String),
    #[error("patina-perturber currently supports only zero-dimensional clusters")]
    NonClusterCandidate,
    #[error("cluster candidate must not carry a lattice")]
    UnexpectedLattice,
    #[error("invalid perturbation sigma `{0}`")]
    InvalidSigma(f64),
    #[error("invalid perturbation count `{0}`")]
    InvalidCount(usize),
    #[error("min-distance validation failed after {attempts} attempts")]
    MinDistanceViolation { attempts: usize },
    #[error("fingerprint requires at least one atom")]
    EmptyStructure,
    #[error("fingerprint dimensionality mismatch: left={left}, right={right}")]
    FingerprintSizeMismatch { left: usize, right: usize },
    #[error("invalid environment cutoff `{0}`")]
    InvalidCutoff(f64),
    #[error("invalid environment sphere capacity `{0}`")]
    InvalidSphereCapacity(usize),
    #[error("unsupported environment basis request: s={s_orbitals}, p={p_orbitals}")]
    UnsupportedEnvironmentBasis {
        s_orbitals: usize,
        p_orbitals: usize,
    },
    #[error("environment sphere exceeded capacity {capacity}")]
    EnvironmentSphereOverflow { capacity: usize },
    #[error("unsupported structure for environment fingerprint: {0}")]
    UnsupportedStructureForEnvironmentFingerprint(String),
    #[error("assignment dimension mismatch: left={left}, right={right}")]
    AssignmentDimensionMismatch { left: usize, right: usize },
}

pub trait ClusterPerturbationEngine {
    fn generate(
        &self,
        source: &ClusterStructure,
        config: PerturbationConfig,
        count: usize,
    ) -> Result<PerturbationBatch, PerturberError>;
}

pub trait StructureFingerprintEngine {
    fn fingerprint(
        &self,
        structure: &ClusterStructure,
    ) -> Result<FingerprintVector, PerturberError>;
}

pub trait EnvironmentStructureFingerprintEngine<S> {
    fn fingerprint_environments(
        &self,
        structure: &S,
    ) -> Result<EnvironmentFingerprintSet, PerturberError>;
}

pub trait DuplicateScreeningEngine {
    fn distance(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
    ) -> Result<FingerprintDistance, PerturberError>;

    fn classify_duplicate(
        &self,
        left: &ClusterStructure,
        right: &ClusterStructure,
        threshold: f64,
    ) -> Result<DuplicateDecision, PerturberError> {
        let distance = self.distance(left, right)?;
        Ok(DuplicateDecision {
            left_label: distance.left_label.clone(),
            right_label: distance.right_label.clone(),
            distance: distance.distance,
            threshold,
            duplicate: distance.distance < threshold,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        centroid, distances_to_csv, eliminate_by_threshold, pairwise_distances,
        ClusterPerturbationEngine, ClusterStructure, DefaultClusterPerturbationEngine,
        DuplicateScreeningEngine, FingerprintDuplicateScreeningEngine,
        OverlapMatrixFingerprintEngine, PerturbAxis, PerturbMode, PerturbationConfig,
        PerturberError, StructureFingerprintEngine,
    };
    use patina_types::Candidate;

    fn cluster_candidate() -> Candidate {
        Candidate::cluster(
            "cluster",
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [1.5, 0.0, 0.0]],
        )
    }

    #[test]
    fn cluster_structure_accepts_zero_d_candidate() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        assert_eq!(cluster.atom_count(), 2);
    }

    #[test]
    fn cluster_structure_rejects_periodic_candidate() {
        let mut candidate = cluster_candidate();
        candidate.lattice = Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]]);
        candidate.periodic_axes = [true, true, true];
        let error = ClusterStructure::try_from_candidate(&candidate).expect_err("must reject");
        assert!(matches!(error, PerturberError::NonClusterCandidate));
    }

    #[test]
    fn cluster_structure_roundtrips_back_to_candidate() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let candidate = Candidate::from(&cluster);
        assert_eq!(candidate, cluster_candidate());
    }

    #[test]
    fn perturbation_engine_generates_requested_variant_count() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let batch = DefaultClusterPerturbationEngine
            .generate(
                &cluster,
                PerturbationConfig {
                    sigma: 0.05,
                    max_displacement: Some(0.1),
                    validate_min_distance: Some(0.5),
                    seed: Some(7),
                    ..PerturbationConfig::default()
                },
                3,
            )
            .expect("batch");
        assert_eq!(batch.variants.len(), 3);
        assert_ne!(batch.variants[0], cluster);
    }

    #[test]
    fn fingerprint_engine_returns_sorted_vector() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let fingerprint = OverlapMatrixFingerprintEngine::default()
            .fingerprint(&cluster)
            .expect("fingerprint");
        assert_eq!(fingerprint.values.len(), 2);
        assert!(fingerprint.values[0] <= fingerprint.values[1]);
    }

    #[test]
    fn duplicate_screening_engine_reports_zero_distance_for_identical_structure() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let distance = FingerprintDuplicateScreeningEngine::default()
            .distance(&cluster, &cluster)
            .expect("distance");
        assert!(distance.distance.abs() < 1.0e-12);
    }

    #[test]
    fn elimination_keeps_diverse_structures() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let mut shifted = cluster.clone();
        shifted.label = "shifted".into();
        shifted.atoms[1].cartesian[0] = 2.2;
        let fp_engine = OverlapMatrixFingerprintEngine::default();
        let fps = vec![
            fp_engine.fingerprint(&cluster).expect("fp0"),
            fp_engine.fingerprint(&cluster).expect("fp1"),
            fp_engine.fingerprint(&shifted).expect("fp2"),
        ];
        let distances = pairwise_distances(&fps).expect("distances");
        let survivors = eliminate_by_threshold(
            &[cluster.clone(), cluster.clone(), shifted],
            &distances,
            1.0e-6,
        )
        .expect("survivors");
        assert_eq!(survivors.len(), 2);
    }

    #[test]
    fn pairwise_distances_rejects_empty_fingerprint_list() {
        let error = pairwise_distances(&[]).expect_err("must reject empty fingerprint list");
        assert!(matches!(error, PerturberError::InvalidCandidate(_)));
    }

    #[test]
    fn elimination_rejects_non_square_distance_matrix() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let error = eliminate_by_threshold(
            &[cluster.clone(), cluster],
            &[vec![0.0, 0.1], vec![0.1]],
            1.0e-6,
        )
        .expect_err("must reject malformed matrix");
        assert!(matches!(error, PerturberError::InvalidCandidate(_)));
    }

    #[test]
    fn centroid_reports_arithmetic_mean() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let center = centroid(&cluster);
        assert!((center.x - 0.75).abs() < 1.0e-12);
        assert!(center.y.abs() < 1.0e-12);
        assert!(center.z.abs() < 1.0e-12);
    }

    #[test]
    fn sp_mode_perturbation_is_supported() {
        let cluster = ClusterStructure::try_from_candidate(&cluster_candidate()).expect("cluster");
        let batch = DefaultClusterPerturbationEngine
            .generate(
                &cluster,
                PerturbationConfig {
                    mode: PerturbMode::Sp,
                    anisotropy: 3.0,
                    axis: PerturbAxis::X,
                    seed: Some(17),
                    ..PerturbationConfig::default()
                },
                1,
            )
            .expect("sp batch");
        assert_eq!(batch.variants.len(), 1);
    }

    #[test]
    fn distances_to_csv_writes_upper_triangle_rows() {
        let csv = distances_to_csv(&vec![
            vec![0.0, 1.25, 2.5],
            vec![1.25, 0.0, 3.75],
            vec![2.5, 3.75, 0.0],
        ])
        .expect("csv");
        assert!(csv.starts_with("i,j,distance\n"));
        assert!(csv.contains("0,1,1.2500000000000000e0"));
        assert!(csv.contains("1,2,3.7500000000000000e0"));
    }
}
