use crate::io::legacy_xyz::{read_legacy_xyz, LegacyXyzError, LegacyXyzStructure};
use patina_syva::optimize_subgroup_symmetry_elements;
use patina_syva::{
    analyze_point_group, classify_framework_group, enumerate_basic_subgroups, preprocess_geometry,
    search_symmetry_elements, summarize_operations, summarize_representative_operation_classes,
    symmetrize_subgroup_geometry, symmetrize_verified_subgroup_geometry,
    symmetry_equivalence_classes_for_search, BasicSubgroupSelection, ClassifiedFrameworkGroup,
    ClassifiedPointGroup, ClusterAtom, ClusterGeometry, OptimizedSubgroupSymmetry,
    RepresentativeOperationClass, SymmetrizedGeometryResult, SymmetryEquivalenceClass,
    SymmetryOperationSummary, SyvaError, SyvaInputGeometry, SyvaPreprocessedGeometry,
    SyvaRunSettings, ToleranceScanSummary,
};
use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClusterSymmetryError {
    #[error("failed to read legacy xyz `{path}`: {source}")]
    ReadXyz {
        path: PathBuf,
        #[source]
        source: LegacyXyzError,
    },
    #[error("cluster candidate `{label}` is invalid: {reason}")]
    InvalidCandidate { label: String, reason: String },
    #[error(
        "cluster symmetry analysis only supports zero-dimensional candidates, `{label}` was {dimensionality:?}"
    )]
    NonZeroDimensionalCandidate {
        label: String,
        dimensionality: patina_types::StructureDimensionality,
    },
    #[error("failed to normalize `{path}` for syva analysis: {source}")]
    Normalize {
        path: PathBuf,
        #[source]
        source: SyvaError,
    },
    #[error("failed to preprocess `{path}` for syva analysis: {source}")]
    Preprocess {
        path: PathBuf,
        #[source]
        source: SyvaError,
    },
    #[error("failed to classify point group for `{path}`: {source}")]
    Classify {
        path: PathBuf,
        #[source]
        source: SyvaError,
    },
    #[error("failed to scan tolerance window for `{path}`: {source}")]
    ToleranceScan {
        path: PathBuf,
        #[source]
        source: SyvaError,
    },
    #[error(
        "requested subgroup `{requested}` was not available for `{path}`; available subgroups: {available:?}"
    )]
    SubgroupNotFound {
        path: PathBuf,
        requested: String,
        available: Vec<String>,
    },
    #[error("failed to optimize subgroup `{subgroup}` for `{path}`: {source}")]
    OptimizeSubgroup {
        path: PathBuf,
        subgroup: String,
        #[source]
        source: SyvaError,
    },
    #[error("failed to symmetrize subgroup `{subgroup}` for `{path}`: {source}")]
    SymmetrizeSubgroup {
        path: PathBuf,
        subgroup: String,
        #[source]
        source: SyvaError,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryConfig {
    pub input_xyz: PathBuf,
    pub tolerance: f64,
    pub tolerance_upper: f64,
    pub tolerance_lower: f64,
    pub include_tolerance_scan: bool,
    pub selected_subgroup: Option<String>,
    pub symmetrize: bool,
    pub strict_symmetrize: bool,
}

impl ClusterSymmetryConfig {
    pub fn from_xyz(input_xyz: impl Into<PathBuf>) -> Self {
        Self {
            input_xyz: input_xyz.into(),
            tolerance: 0.001,
            tolerance_upper: 5.0e-2,
            tolerance_lower: 5.0e-3,
            include_tolerance_scan: true,
            selected_subgroup: None,
            symmetrize: false,
            strict_symmetrize: false,
        }
    }

    pub fn syva_settings(&self) -> SyvaRunSettings {
        SyvaRunSettings {
            tolerance: self.tolerance,
            tolerance_upper: self.tolerance_upper,
            tolerance_lower: self.tolerance_lower,
            ..SyvaRunSettings::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryReport {
    pub source_path: PathBuf,
    pub label: String,
    pub atom_count: usize,
    pub legacy_comment: String,
    pub total_energy: Option<f64>,
    pub input_geometry: SyvaInputGeometry,
    pub preprocessed_geometry: SyvaPreprocessedGeometry,
    pub classified_point_group: Option<ClassifiedPointGroup>,
    pub classified_framework_group: Option<ClassifiedFrameworkGroup>,
    pub symmetry_equivalence_classes: Vec<SymmetryEquivalenceClass>,
    pub operations: Vec<SymmetryOperationSummary>,
    pub representative_operation_classes: Vec<RepresentativeOperationClass>,
    pub basic_subgroups: Vec<BasicSubgroupSelection>,
    pub selected_subgroup: Option<BasicSubgroupSelection>,
    pub optimized_subgroup_symmetry: Option<OptimizedSubgroupSymmetry>,
    pub symmetrized_geometry: Option<SymmetrizedGeometryResult>,
    pub symmetry: patina_syva::SymmetrySearchResult,
    pub tolerance_scan: Option<ToleranceScanSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryJsonReport {
    pub source_path: PathBuf,
    pub label: String,
    pub atom_count: usize,
    pub total_energy: Option<f64>,
    pub point_group: Option<ClassifiedPointGroup>,
    pub framework_group: Option<String>,
    pub framework_components: Vec<patina_syva::FrameworkGroupComponent>,
    pub symmetry_summary: ClusterSymmetrySearchSummary,
    pub equivalence_classes: Vec<ClusterSymmetryEquivalenceClassSummary>,
    pub operations: Vec<ClusterSymmetryOperationSummary>,
    pub representative_operation_classes: Vec<RepresentativeOperationClass>,
    pub basic_subgroups: Vec<ClusterSymmetrySubgroupSummary>,
    pub selected_subgroup: Option<ClusterSymmetrySubgroupSummary>,
    pub optimized_subgroup_symmetry: Option<ClusterSymmetryOptimizedSubgroupSummary>,
    pub symmetrized_geometry: Option<ClusterSymmetrySymmetrizationSummary>,
    pub tolerance_scan: Option<ToleranceScanSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetrySearchSummary {
    pub is_linear: bool,
    pub is_planar: bool,
    pub center_atom_index: Option<usize>,
    pub has_inversion_center: bool,
    pub reflection_plane_count: usize,
    pub proper_rotation_axis_orders: Vec<usize>,
    pub proper_rotation_count: usize,
    pub improper_rotation_count: usize,
    pub permutation_count: usize,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryEquivalenceClassSummary {
    pub species: String,
    pub atomic_number: u8,
    pub size: usize,
    pub representative_source_atom_index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryOperationSummary {
    pub index: usize,
    pub label: String,
    pub kind: patina_syva::SymmetryElementKind,
    pub fixed_atom_count: usize,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetrySubgroupSummary {
    pub label: String,
    pub operation_count: usize,
    pub operation_labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryOptimizedSubgroupSummary {
    pub point_group: String,
    pub operation_count: usize,
    pub operations: Vec<ClusterSymmetryOptimizedOperationSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetryOptimizedOperationSummary {
    pub index: usize,
    pub label: String,
    pub kind: patina_syva::SymmetryElementKind,
    pub direction: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSymmetrySymmetrizationSummary {
    pub point_group: String,
    pub orbit_count: usize,
    pub representative_class_count: usize,
    pub optimized_atom_count: usize,
    pub all_atoms_optimized: bool,
    pub success: bool,
    pub recovered_point_group: Option<String>,
    pub recovered_subgroup_labels: Vec<String>,
    pub orbit_equivalence_alignment: bool,
}

impl ClusterSymmetryReport {
    pub fn json_report(&self) -> ClusterSymmetryJsonReport {
        ClusterSymmetryJsonReport {
            source_path: self.source_path.clone(),
            label: self.label.clone(),
            atom_count: self.atom_count,
            total_energy: self.total_energy,
            point_group: self.classified_point_group.clone(),
            framework_group: self
                .classified_framework_group
                .as_ref()
                .map(|group| group.framework_group.clone()),
            framework_components: self
                .classified_framework_group
                .as_ref()
                .map(|group| group.components.clone())
                .unwrap_or_default(),
            symmetry_summary: ClusterSymmetrySearchSummary {
                is_linear: self.symmetry.is_linear,
                is_planar: self.symmetry.is_planar,
                center_atom_index: self.symmetry.center_atom_index,
                has_inversion_center: self.symmetry.inversion_center.is_some(),
                reflection_plane_count: self.symmetry.reflection_planes.len(),
                proper_rotation_axis_orders: unique_sorted_orders(
                    self.symmetry
                        .proper_rotation_axes
                        .iter()
                        .map(|axis| axis.order),
                ),
                proper_rotation_count: self.symmetry.proper_rotations.len(),
                improper_rotation_count: self.symmetry.improper_rotations.len(),
                permutation_count: self.symmetry.permutations.len(),
                max_deviation: self.symmetry.max_deviation,
            },
            equivalence_classes: self
                .symmetry_equivalence_classes
                .iter()
                .map(|class| ClusterSymmetryEquivalenceClassSummary {
                    species: class.species.clone(),
                    atomic_number: class.atomic_number,
                    size: class.active_atom_indices.len(),
                    representative_source_atom_index: class.representative_source_atom_index,
                })
                .collect(),
            operations: self
                .operations
                .iter()
                .map(|operation| ClusterSymmetryOperationSummary {
                    index: operation.index,
                    label: operation.label.clone(),
                    kind: operation.kind.clone(),
                    fixed_atom_count: operation.fixed_atom_count,
                    max_deviation: operation.max_deviation,
                })
                .collect(),
            representative_operation_classes: self.representative_operation_classes.clone(),
            basic_subgroups: self
                .basic_subgroups
                .iter()
                .map(|subgroup| ClusterSymmetrySubgroupSummary {
                    label: subgroup.label.as_str().to_string(),
                    operation_count: subgroup.operations.len(),
                    operation_labels: subgroup
                        .operations
                        .iter()
                        .map(|operation| operation.label.clone())
                        .collect(),
                })
                .collect(),
            selected_subgroup: self.selected_subgroup.as_ref().map(|subgroup| {
                ClusterSymmetrySubgroupSummary {
                    label: subgroup.label.as_str().to_string(),
                    operation_count: subgroup.operations.len(),
                    operation_labels: subgroup
                        .operations
                        .iter()
                        .map(|operation| operation.label.clone())
                        .collect(),
                }
            }),
            optimized_subgroup_symmetry: self.optimized_subgroup_symmetry.as_ref().map(
                |optimized| ClusterSymmetryOptimizedSubgroupSummary {
                    point_group: optimized.point_group.as_str().to_string(),
                    operation_count: optimized.operations.len(),
                    operations: optimized
                        .operations
                        .iter()
                        .map(|operation| ClusterSymmetryOptimizedOperationSummary {
                            index: operation.index,
                            label: operation.label.clone(),
                            kind: operation.kind.clone(),
                            direction: operation.direction,
                        })
                        .collect(),
                },
            ),
            symmetrized_geometry: self.symmetrized_geometry.as_ref().map(|result| {
                ClusterSymmetrySymmetrizationSummary {
                    point_group: result.point_group.as_str().to_string(),
                    orbit_count: result.orbits.len(),
                    representative_class_count: result.representative_classes.len(),
                    optimized_atom_count: result.status.optimized_atom_count,
                    all_atoms_optimized: result.status.all_atoms_optimized,
                    success: result.status.success,
                    recovered_point_group: result
                        .verification
                        .recovered_point_group
                        .as_ref()
                        .map(|label| label.as_str().to_string()),
                    recovered_subgroup_labels: result
                        .verification
                        .recovered_subgroup_labels
                        .iter()
                        .map(|label| label.as_str().to_string())
                        .collect(),
                    orbit_equivalence_alignment: result.verification.orbit_equivalence_alignment,
                }
            }),
            tolerance_scan: self.tolerance_scan.clone(),
        }
    }
}

pub fn run_cluster_symmetry_workflow(
    config: &ClusterSymmetryConfig,
) -> Result<ClusterSymmetryReport, ClusterSymmetryError> {
    let structure =
        read_legacy_xyz(&config.input_xyz).map_err(|source| ClusterSymmetryError::ReadXyz {
            path: config.input_xyz.clone(),
            source,
        })?;

    let cluster = legacy_xyz_to_cluster_geometry(&structure)?;
    let input_geometry = SyvaInputGeometry::from_cluster_geometry(&cluster).map_err(|source| {
        ClusterSymmetryError::Normalize {
            path: config.input_xyz.clone(),
            source,
        }
    })?;

    let settings = config.syva_settings();
    let preprocessed_geometry =
        preprocess_geometry(&input_geometry, &settings).map_err(|source| {
            ClusterSymmetryError::Preprocess {
                path: config.input_xyz.clone(),
                source,
            }
        })?;
    let symmetry = search_symmetry_elements(&preprocessed_geometry);
    let classified_point_group = analyze_point_group(&preprocessed_geometry).map_err(|source| {
        ClusterSymmetryError::Classify {
            path: config.input_xyz.clone(),
            source,
        }
    })?;
    let classified_framework_group = classified_point_group.as_ref().map(|point_group| {
        classify_framework_group(&point_group.label, &preprocessed_geometry, &symmetry)
    });
    let symmetry_equivalence_classes =
        symmetry_equivalence_classes_for_search(&preprocessed_geometry, &symmetry);
    let operations = classified_point_group
        .as_ref()
        .map(|point_group| {
            summarize_operations(&point_group.label, &preprocessed_geometry, &symmetry)
        })
        .unwrap_or_default();
    let representative_operation_classes = classified_point_group
        .as_ref()
        .map(|point_group| {
            summarize_representative_operation_classes(
                &point_group.label,
                &preprocessed_geometry,
                &symmetry,
            )
        })
        .unwrap_or_default();
    let basic_subgroups =
        enumerate_basic_subgroups(&preprocessed_geometry, &symmetry).map_err(|source| {
            ClusterSymmetryError::Classify {
                path: config.input_xyz.clone(),
                source,
            }
        })?;
    let selected_subgroup = resolve_selected_subgroup(config, &basic_subgroups, &config.input_xyz)?;
    let optimized_subgroup_symmetry = selected_subgroup
        .as_ref()
        .map(|subgroup| {
            optimize_subgroup_symmetry_elements(
                &symmetry,
                subgroup,
                preprocessed_geometry.settings.tolerance,
            )
            .map_err(|source| ClusterSymmetryError::OptimizeSubgroup {
                path: config.input_xyz.clone(),
                subgroup: subgroup.label.as_str().to_string(),
                source,
            })
        })
        .transpose()?;
    let symmetrized_geometry = if config.symmetrize {
        let subgroup =
            selected_subgroup
                .as_ref()
                .ok_or_else(|| ClusterSymmetryError::SubgroupNotFound {
                    path: config.input_xyz.clone(),
                    requested: "<missing --subgroup>".into(),
                    available: basic_subgroups
                        .iter()
                        .map(|subgroup| subgroup.label.as_str().to_string())
                        .collect(),
                })?;
        Some(
            if config.strict_symmetrize {
                symmetrize_verified_subgroup_geometry(&preprocessed_geometry, &symmetry, subgroup)
            } else {
                symmetrize_subgroup_geometry(&preprocessed_geometry, &symmetry, subgroup)
            }
            .map_err(|source| ClusterSymmetryError::SymmetrizeSubgroup {
                path: config.input_xyz.clone(),
                subgroup: subgroup.label.as_str().to_string(),
                source,
            })?,
        )
    } else {
        None
    };

    let tolerance_scan = if config.include_tolerance_scan {
        Some(
            patina_syva::scan_point_groups_over_tolerance(&input_geometry, &settings).map_err(
                |source| ClusterSymmetryError::ToleranceScan {
                    path: config.input_xyz.clone(),
                    source,
                },
            )?,
        )
    } else {
        None
    };

    let atom_count = structure.atom_count();

    Ok(ClusterSymmetryReport {
        source_path: config.input_xyz.clone(),
        label: structure.label,
        atom_count,
        legacy_comment: structure.comment,
        total_energy: structure.total_energy,
        input_geometry,
        preprocessed_geometry,
        classified_point_group,
        classified_framework_group,
        symmetry_equivalence_classes,
        operations,
        representative_operation_classes,
        basic_subgroups,
        selected_subgroup,
        optimized_subgroup_symmetry,
        symmetrized_geometry,
        symmetry,
        tolerance_scan,
    })
}

fn resolve_selected_subgroup(
    config: &ClusterSymmetryConfig,
    subgroups: &[BasicSubgroupSelection],
    input_xyz: &std::path::Path,
) -> Result<Option<BasicSubgroupSelection>, ClusterSymmetryError> {
    let Some(requested) = config.selected_subgroup.as_deref() else {
        return Ok(None);
    };
    subgroups
        .iter()
        .find(|subgroup| subgroup.label.as_str() == requested)
        .cloned()
        .map(Some)
        .ok_or_else(|| ClusterSymmetryError::SubgroupNotFound {
            path: input_xyz.to_path_buf(),
            requested: requested.to_string(),
            available: subgroups
                .iter()
                .map(|subgroup| subgroup.label.as_str().to_string())
                .collect(),
        })
}

pub fn legacy_xyz_to_cluster_geometry(
    structure: &LegacyXyzStructure,
) -> Result<ClusterGeometry, ClusterSymmetryError> {
    structure
        .candidate
        .validate()
        .map_err(|source| ClusterSymmetryError::InvalidCandidate {
            label: structure.label.clone(),
            reason: format!("{source:?}"),
        })?;

    if !structure.candidate.is_zero_d() {
        return Err(ClusterSymmetryError::NonZeroDimensionalCandidate {
            label: structure.label.clone(),
            dimensionality: structure.candidate.declared_dimensionality(),
        });
    }

    Ok(cluster_geometry_from_parts(
        &structure.label,
        &structure.candidate.species,
        &structure.candidate.fractional_coords,
    ))
}

fn cluster_geometry_from_parts(
    label: &str,
    species: &[String],
    cartesian: &[[f64; 3]],
) -> ClusterGeometry {
    let atoms = species
        .iter()
        .zip(cartesian.iter())
        .map(|(species, cartesian)| ClusterAtom {
            species: species.clone(),
            cartesian: *cartesian,
        })
        .collect();

    ClusterGeometry {
        label: label.to_string(),
        atoms,
    }
}

fn unique_sorted_orders(orders: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let mut values = orders.into_iter().collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
}

pub fn symmetrized_xyz_structure(report: &ClusterSymmetryReport) -> Option<LegacyXyzStructure> {
    let symmetrized = report.symmetrized_geometry.as_ref()?;
    let species = report
        .input_geometry
        .atoms
        .iter()
        .map(|atom| atom.species.clone())
        .collect::<Vec<_>>();
    Some(LegacyXyzStructure {
        label: format!(
            "{}_{}_symmetrized",
            report.label,
            symmetrized.point_group.as_str().to_lowercase()
        ),
        candidate: Candidate::cluster(
            format!(
                "{}_{}_symmetrized",
                report.label,
                symmetrized.point_group.as_str().to_lowercase()
            ),
            species,
            symmetrized.coordinates.clone(),
        ),
        total_energy: report.total_energy,
        charges: Vec::new(),
        cell_dims: None,
        comment: format!(
            "symmetrized subgroup={} source={}",
            symmetrized.point_group.as_str(),
            report.source_path.display()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        legacy_xyz_to_cluster_geometry, run_cluster_symmetry_workflow, symmetrized_xyz_structure,
        ClusterSymmetryConfig, ClusterSymmetryError,
    };
    use crate::io::legacy_xyz::LegacyXyzStructure;
    use patina_types::Candidate;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn classifies_linear_co2_from_xyz_workflow() {
        let temp = tempdir().expect("tempdir");
        let xyz_path = temp.path().join("co2.xyz");
        fs::write(
            &xyz_path,
            concat!(
                "3\n",
                "SCF Done             -1.0000000000e+00;\n",
                "O -1.1600000000 0.0000000000 0.0000000000\n",
                "C 0.0000000000 0.0000000000 0.0000000000\n",
                "O 1.1600000000 0.0000000000 0.0000000000\n"
            ),
        )
        .expect("write xyz");

        let report = run_cluster_symmetry_workflow(&ClusterSymmetryConfig::from_xyz(&xyz_path))
            .expect("cluster symmetry report");

        assert_eq!(report.label, "co2");
        assert_eq!(report.atom_count, 3);
        assert_eq!(
            report
                .classified_point_group
                .as_ref()
                .map(|group| group.label.as_str()),
            Some("Dih")
        );
        assert_eq!(
            report
                .classified_framework_group
                .as_ref()
                .map(|group| group.framework_group.as_str()),
            Some("Dih[O(C),Cinf(O.O)]")
        );
        assert_eq!(report.symmetry_equivalence_classes.len(), 2);
        assert_eq!(report.operations[0].label, "E");
        assert_eq!(report.operations[1].label, "i");
        assert_eq!(report.representative_operation_classes[0].label, "E");
        assert!(report
            .basic_subgroups
            .iter()
            .any(|subgroup| subgroup.label.as_str() == "Ci"));
        assert!(report
            .basic_subgroups
            .iter()
            .any(|subgroup| subgroup.label.as_str() == "Dih"));
        assert!(report.symmetry.is_linear);
        assert!(report.tolerance_scan.is_some());
    }

    #[test]
    fn rejects_periodic_candidate_conversion() {
        let structure = LegacyXyzStructure {
            label: "periodic".into(),
            candidate: Candidate::fully_periodic(
                "periodic",
                vec!["Mg".into(), "O".into()],
                vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
                [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]],
            ),
            total_energy: None,
            charges: vec![0.0, 0.0],
            cell_dims: None,
            comment: String::new(),
        };

        let error = legacy_xyz_to_cluster_geometry(&structure).expect_err("non-zero-d rejected");
        match error {
            ClusterSymmetryError::NonZeroDimensionalCandidate { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn workflow_can_skip_tolerance_scan() {
        let temp = tempdir().expect("tempdir");
        let xyz_path = temp.path().join("water.xyz");
        write_water_xyz(&xyz_path);

        let mut config = ClusterSymmetryConfig::from_xyz(&xyz_path);
        config.include_tolerance_scan = false;

        let report = run_cluster_symmetry_workflow(&config).expect("cluster symmetry report");
        assert!(report.tolerance_scan.is_none());
        assert_eq!(
            report
                .classified_point_group
                .as_ref()
                .map(|group| group.label.as_str()),
            Some("C2v")
        );
        assert_eq!(
            report
                .classified_framework_group
                .as_ref()
                .map(|group| group.framework_group.as_str()),
            Some("C2v[C2(O),SGV(H2)]")
        );
        assert!(report
            .operations
            .iter()
            .any(|operation| operation.label == "C2"));
        assert!(report
            .operations
            .iter()
            .any(|operation| operation.label == "Sigma_v"));
        assert!(report
            .representative_operation_classes
            .iter()
            .any(|operation| operation.label == "Sigma_v"));
        assert!(report
            .basic_subgroups
            .iter()
            .any(|subgroup| subgroup.label.as_str() == "C2"));
        assert!(report
            .basic_subgroups
            .iter()
            .any(|subgroup| subgroup.label.as_str() == "C2v"));
    }

    #[test]
    fn json_report_focuses_on_symmetry_summary_not_raw_atom_payloads() {
        let temp = tempdir().expect("tempdir");
        let xyz_path = temp.path().join("water.xyz");
        write_water_xyz(&xyz_path);

        let report = run_cluster_symmetry_workflow(&ClusterSymmetryConfig::from_xyz(&xyz_path))
            .expect("cluster symmetry report");
        let json_report = report.json_report();
        let value = serde_json::to_value(&json_report).expect("json");

        assert!(value.get("input_geometry").is_none());
        assert!(value.get("preprocessed_geometry").is_none());
        assert!(value.get("symmetry").is_none());
        assert_eq!(value["point_group"]["label"].as_str(), Some("C2v"));
        assert_eq!(
            value["framework_group"].as_str(),
            Some("C2v[C2(O),SGV(H2)]")
        );
        assert!(matches!(
            value["operations"].as_array(),
            Some(operations) if operations.iter().all(|entry| entry.get("permutation").is_none())
        ));
        assert!(matches!(
            value["basic_subgroups"].as_array(),
            Some(subgroups) if subgroups.iter().all(|entry| entry.get("operations").is_none())
        ));
        assert!(matches!(
            value["equivalence_classes"].as_array(),
            Some(classes) if classes.iter().all(|entry| {
                entry.get("active_atom_indices").is_none()
                    && entry.get("source_atom_indices").is_none()
            })
        ));
        assert_eq!(
            value["symmetry_summary"]["reflection_plane_count"].as_u64(),
            Some(2)
        );
    }

    #[test]
    fn workflow_can_optimize_and_symmetrize_selected_subgroup() {
        let temp = tempdir().expect("tempdir");
        let xyz_path = temp.path().join("water.xyz");
        write_water_xyz(&xyz_path);

        let mut config = ClusterSymmetryConfig::from_xyz(&xyz_path);
        config.selected_subgroup = Some("C2v".into());
        config.symmetrize = true;
        config.strict_symmetrize = true;

        let report = run_cluster_symmetry_workflow(&config).expect("cluster symmetry report");
        let optimized = report
            .optimized_subgroup_symmetry
            .as_ref()
            .expect("optimized subgroup");
        let symmetrized = report
            .symmetrized_geometry
            .as_ref()
            .expect("symmetrized geometry");

        assert_eq!(
            report
                .selected_subgroup
                .as_ref()
                .map(|subgroup| subgroup.label.as_str()),
            Some("C2v")
        );
        assert_eq!(optimized.point_group.as_str(), "C2v");
        assert!(!optimized.operations.is_empty());
        assert_eq!(symmetrized.point_group.as_str(), "C2v");
        assert!(symmetrized.status.success);
        assert!(symmetrized.verification.verified);
        assert!(symmetrized.verification.orbit_equivalence_alignment);

        let xyz = symmetrized_xyz_structure(&report).expect("symmetrized xyz structure");
        assert_eq!(xyz.atom_count(), 3);
        assert!(xyz.label.contains("c2v_symmetrized"));

        let json_report = report.json_report();
        assert_eq!(
            json_report
                .selected_subgroup
                .as_ref()
                .map(|subgroup| subgroup.label.as_str()),
            Some("C2v")
        );
        assert_eq!(
            json_report
                .optimized_subgroup_symmetry
                .as_ref()
                .map(|optimized| optimized.point_group.as_str()),
            Some("C2v")
        );
        assert_eq!(
            json_report
                .symmetrized_geometry
                .as_ref()
                .map(|symmetrized| symmetrized.point_group.as_str()),
            Some("C2v")
        );
    }

    fn write_water_xyz(path: &Path) {
        fs::write(
            path,
            concat!(
                "3\n",
                "SCF Done             -7.6000000000e+01;\n",
                "O 0.0000000000 0.0000000000 0.0000000000\n",
                "H 0.7586020000 0.0000000000 0.5042840000\n",
                "H -0.7586020000 0.0000000000 0.5042840000\n"
            ),
        )
        .expect("write water xyz");
    }
}
