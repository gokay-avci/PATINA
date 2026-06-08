use crate::domain::{MotifCandidate, SymmetrySignature, TopologyError, TopologyResult};
use crate::ports::PointSymmetryBackend;
use patina_syva::{
    analyze_point_group, enumerate_basic_subgroups, preprocess_geometry, search_symmetry_elements,
    summarize_representative_operation_classes, symmetrize_subgroup_geometry,
    symmetrize_verified_subgroup_geometry, symmetry_equivalence_classes_for_search, ClusterAtom,
    ClusterGeometry, SyvaInputGeometry, SyvaRunSettings,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct SyvaPointSymmetryBackend {
    pub tolerance: f64,
}

impl SyvaPointSymmetryBackend {
    pub fn new(tolerance: f64) -> Self {
        Self { tolerance }
    }

    pub fn symmetrize_candidate(
        &self,
        candidate: &MotifCandidate,
        strict: bool,
    ) -> SyvaSymmetrizationRecord {
        match self.try_symmetrize_candidate(candidate, strict) {
            Ok(record) => record,
            Err(error) => SyvaSymmetrizationRecord {
                candidate_id: candidate.id.0.clone(),
                point_group: None,
                recovered_point_group: None,
                status: "failed".to_string(),
                verified: false,
                orbit_equivalence_alignment: false,
                optimized_atom_count: 0,
                all_atoms_optimized: false,
                coordinates: None,
                xyz_path: None,
                message: Some(error.to_string()),
            },
        }
    }

    fn try_symmetrize_candidate(
        &self,
        candidate: &MotifCandidate,
        strict: bool,
    ) -> TopologyResult<SyvaSymmetrizationRecord> {
        let input = SyvaInputGeometry::from_cluster_geometry(&candidate_to_cluster(candidate))
            .map_err(syva_error)?;
        let settings = SyvaRunSettings {
            tolerance: self.tolerance,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).map_err(syva_error)?;
        let search = search_symmetry_elements(&geometry);
        let classified = analyze_point_group(&geometry)
            .map_err(syva_error)?
            .ok_or_else(|| TopologyError::Backend {
                backend: "syva".to_string(),
                message: "point group classification returned no result".to_string(),
            })?;
        let subgroups = enumerate_basic_subgroups(&geometry, &search).map_err(syva_error)?;
        let subgroup = subgroups
            .iter()
            .find(|subgroup| subgroup.label.as_str() == classified.label.as_str())
            .or_else(|| {
                subgroups
                    .iter()
                    .find(|subgroup| subgroup.label.as_str() == "C1")
            })
            .ok_or_else(|| TopologyError::Backend {
                backend: "syva".to_string(),
                message: format!(
                    "no selectable subgroup for point group {}",
                    classified.label.as_str()
                ),
            })?;
        let result = if strict {
            symmetrize_verified_subgroup_geometry(&geometry, &search, subgroup)
        } else {
            symmetrize_subgroup_geometry(&geometry, &search, subgroup)
        }
        .map_err(syva_error)?;
        Ok(SyvaSymmetrizationRecord {
            candidate_id: candidate.id.0.clone(),
            point_group: Some(result.point_group.as_str().to_string()),
            recovered_point_group: result
                .verification
                .recovered_point_group
                .as_ref()
                .map(|label| label.as_str().to_string()),
            status: if result.status.success {
                "ok".to_string()
            } else {
                "warning".to_string()
            },
            verified: result.verification.verified,
            orbit_equivalence_alignment: result.verification.orbit_equivalence_alignment,
            optimized_atom_count: result.status.optimized_atom_count,
            all_atoms_optimized: result.status.all_atoms_optimized,
            coordinates: Some(result.coordinates),
            xyz_path: None,
            message: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyvaSymmetrizationRecord {
    pub candidate_id: String,
    pub point_group: Option<String>,
    pub recovered_point_group: Option<String>,
    pub status: String,
    pub verified: bool,
    pub orbit_equivalence_alignment: bool,
    pub optimized_atom_count: usize,
    pub all_atoms_optimized: bool,
    pub coordinates: Option<Vec<[f64; 3]>>,
    pub xyz_path: Option<PathBuf>,
    pub message: Option<String>,
}

impl PointSymmetryBackend for SyvaPointSymmetryBackend {
    fn analyze(&self, candidate: &MotifCandidate) -> TopologyResult<SymmetrySignature> {
        let cluster = candidate_to_cluster(candidate);
        let input = SyvaInputGeometry::from_cluster_geometry(&cluster).map_err(syva_error)?;
        let settings = SyvaRunSettings {
            tolerance: self.tolerance,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).map_err(syva_error)?;
        let search = search_symmetry_elements(&geometry);
        let point_group = analyze_point_group(&geometry).map_err(syva_error)?;
        let equivalence_classes = symmetry_equivalence_classes_for_search(&geometry, &search)
            .into_iter()
            .map(|class| {
                class
                    .source_atom_indices
                    .into_iter()
                    .map(|index| index.saturating_sub(1))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let representative_operation_count = point_group
            .as_ref()
            .map(|classified| {
                summarize_representative_operation_classes(&classified.label, &geometry, &search)
                    .len()
            })
            .unwrap_or(0);

        Ok(SymmetrySignature {
            point_group: point_group.map(|classified| classified.label.as_str().to_string()),
            backend: Some("syva".to_string()),
            tolerance: Some(self.tolerance),
            max_deviation: Some(search.max_deviation),
            operation_count: Some(representative_operation_count),
            permutation_count: Some(search.permutations.len()),
            equivalence_classes,
            is_linear: Some(search.is_linear),
            is_planar: Some(search.is_planar),
            status: "ok".to_string(),
            message: None,
        })
    }
}

fn syva_error(error: patina_syva::SyvaError) -> TopologyError {
    TopologyError::Backend {
        backend: "syva".to_string(),
        message: error.to_string(),
    }
}

fn candidate_to_cluster(candidate: &MotifCandidate) -> ClusterGeometry {
    ClusterGeometry {
        label: candidate.id.0.clone(),
        atoms: candidate
            .atoms
            .iter()
            .map(|atom| ClusterAtom {
                species: atom.element.to_string(),
                cartesian: atom.position,
            })
            .collect(),
    }
}
