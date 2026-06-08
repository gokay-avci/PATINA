use crate::framework::{symmetry_equivalence_classes_for_search, SymmetryEquivalenceClass};
use crate::model::{ClusterAtom, ClusterGeometry, PointGroupLabel, SyvaInputGeometry};
use crate::optimize::{optimize_subgroup_symmetry_elements, OptimizedSubgroupSymmetry};
use crate::preprocess::SyvaPreprocessedGeometry;
use crate::subgroups::BasicSubgroupSelection;
use crate::symmetry_elements::{SymmetryElementKind, SymmetrySearchResult};
use crate::SyvaError;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::f64::consts::TAU;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SymmetryAnchorKind {
    CenterOfMass,
    ProperRotationAxis { order: usize, direction: [f64; 3] },
    ReflectionPlane { normal: [f64; 3] },
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetryAnchorAssignment {
    pub atom_index: usize,
    pub kind: SymmetryAnchorKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetrizedGeometryResult {
    pub point_group: PointGroupLabel,
    pub coordinates: Vec<[f64; 3]>,
    pub orbits: Vec<SymmetryOrbitSummary>,
    pub representative_classes: Vec<SymmetryRepresentativeClassSummary>,
    pub anchor_assignments: Vec<SymmetryAnchorAssignment>,
    pub status: SymmetrizedGeometryStatus,
    pub verification: SymmetrizedGeometryVerification,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetrizedGeometryVerification {
    pub verified: bool,
    pub recovered_point_group: Option<PointGroupLabel>,
    pub recovered_subgroup_labels: Vec<PointGroupLabel>,
    pub recovered_equivalence_classes: Vec<SymmetryEquivalenceClass>,
    pub orbit_equivalence_alignment: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetrizedGeometryStatus {
    pub optimized_atom_count: usize,
    pub all_atoms_optimized: bool,
    pub fallback_atom_indices: Vec<usize>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetryOrbitSummary {
    pub representative_atom_index: usize,
    pub member_atom_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetryRepresentativeClassSummary {
    pub representative_atom_index: usize,
    pub representative_source_atom_index: usize,
    pub atomic_number: u8,
    pub species: String,
    pub member_atom_indices: Vec<usize>,
    pub member_source_atom_indices: Vec<usize>,
    pub anchor: SymmetryAnchorKind,
    pub members: Vec<SymmetryOrbitMemberSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetryOrbitMemberSummary {
    pub atom_index: usize,
    pub source_atom_index: usize,
    pub operation_path: Vec<SymmetryOperationPathStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetryOperationPathStep {
    pub operation_index: usize,
    pub label: String,
    pub kind: SymmetryElementKind,
}

pub fn symmetrize_subgroup_geometry(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    subgroup: &BasicSubgroupSelection,
) -> Result<SymmetrizedGeometryResult, SyvaError> {
    let optimized_symmetry =
        optimize_subgroup_symmetry_elements(search, subgroup, geometry.settings.tolerance)?;
    let atom_count = geometry.active_atoms.len();
    let mut coordinates = vec![[0.0; 3]; atom_count];

    let orbit_plans = build_subgroup_orbit_plans(geometry, search, subgroup, &optimized_symmetry)?;
    let mut assignments = Vec::<SymmetryAnchorAssignment>::with_capacity(orbit_plans.len());
    let mut orbit_summaries = Vec::<SymmetryOrbitSummary>::with_capacity(orbit_plans.len());
    let mut representative_classes =
        Vec::<SymmetryRepresentativeClassSummary>::with_capacity(orbit_plans.len());

    for orbit in &orbit_plans {
        let representative_index = orbit.summary.representative_atom_index;
        let anchored = project_anchor_coordinate(
            geometry.active_atoms[representative_index - 1].shifted_cartesian,
            &orbit.assignment.kind,
        );
        let representative_coordinate = stabilize_coordinate(
            representative_index,
            anchored,
            subgroup,
            search,
            &optimized_symmetry,
        )?;

        for member in &orbit.members {
            coordinates[member.atom_index - 1] = propagate_orbit_coordinate(
                representative_coordinate,
                member,
                subgroup,
                search,
                &optimized_symmetry,
            )?;
        }

        assignments.push(orbit.assignment.clone());
        orbit_summaries.push(orbit.summary.clone());
        representative_classes.push(summarize_representative_class(geometry, subgroup, orbit));
    }

    assignments.sort_by_key(|assignment| assignment.atom_index);
    orbit_summaries.sort_by_key(|orbit| orbit.representative_atom_index);
    representative_classes.sort_by_key(|class| class.representative_atom_index);
    let verification = verify_symmetrized_geometry(geometry, &coordinates, subgroup)?;
    let optimized_atom_count = atom_count;
    let status = SymmetrizedGeometryStatus {
        optimized_atom_count,
        all_atoms_optimized: true,
        fallback_atom_indices: Vec::new(),
        success: verification.verified && verification.orbit_equivalence_alignment,
    };

    Ok(SymmetrizedGeometryResult {
        point_group: subgroup.label.clone(),
        coordinates,
        orbits: orbit_summaries,
        representative_classes,
        anchor_assignments: assignments,
        status,
        verification,
    })
}

pub fn symmetrize_verified_subgroup_geometry(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    subgroup: &BasicSubgroupSelection,
) -> Result<SymmetrizedGeometryResult, SyvaError> {
    let result = symmetrize_subgroup_geometry(geometry, search, subgroup)?;
    if result.status.success {
        Ok(result)
    } else {
        Err(SyvaError::SymmetrizationVerificationFailed {
            requested_point_group: subgroup.label.as_str().to_string(),
            optimized_atom_count: result.status.optimized_atom_count,
            recovered_point_group: result
                .verification
                .recovered_point_group
                .as_ref()
                .map(|label| label.as_str().to_string()),
        })
    }
}

pub fn verify_symmetrized_geometry(
    geometry: &SyvaPreprocessedGeometry,
    coordinates: &[[f64; 3]],
    expected_subgroup: &BasicSubgroupSelection,
) -> Result<SymmetrizedGeometryVerification, SyvaError> {
    let verification_input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
        label: format!("{}-symmetrized", geometry.title),
        atoms: geometry
            .active_atoms
            .iter()
            .zip(coordinates.iter())
            .map(|(atom, coordinate)| ClusterAtom {
                species: atom.species.clone(),
                cartesian: *coordinate,
            })
            .collect(),
    })?;
    let verification_geometry =
        crate::preprocess::preprocess_geometry(&verification_input, &geometry.settings)?;
    let verification_search =
        crate::symmetry_elements::search_symmetry_elements(&verification_geometry);
    let recovered_point_group =
        crate::classify::classify_point_group(&verification_search)?.map(|group| group.label);
    let recovered_subgroup_labels =
        crate::subgroups::enumerate_basic_subgroups(&verification_geometry, &verification_search)?
            .into_iter()
            .map(|subgroup| subgroup.label)
            .collect::<Vec<_>>();
    let recovered_equivalence_classes =
        symmetry_equivalence_classes_for_search(&verification_geometry, &verification_search);
    let planned_orbit_classes = build_sorted_orbit_classes_from_subgroup(expected_subgroup);
    let recovered_orbit_classes = build_sorted_equivalence_classes(&recovered_equivalence_classes);
    let verified = recovered_subgroup_labels
        .iter()
        .any(|label| label.as_str() == expected_subgroup.label.as_str());

    Ok(SymmetrizedGeometryVerification {
        verified,
        recovered_point_group,
        recovered_subgroup_labels,
        recovered_equivalence_classes,
        orbit_equivalence_alignment: planned_orbit_classes == recovered_orbit_classes,
    })
}

fn assign_atom_anchor(
    atom_index: usize,
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    subgroup: &BasicSubgroupSelection,
    optimized: &OptimizedSubgroupSymmetry,
) -> Result<SymmetryAnchorAssignment, SyvaError> {
    let tolerance = geometry.settings.tolerance;
    let coordinate = geometry.active_atoms[atom_index - 1].shifted_cartesian;

    if norm(coordinate) <= tolerance || search.center_atom_index == Some(atom_index) {
        return Ok(SymmetryAnchorAssignment {
            atom_index,
            kind: SymmetryAnchorKind::CenterOfMass,
        });
    }

    let stabilizer = collect_fixed_stabilizer(atom_index, search, subgroup, optimized, tolerance)?;
    let kind = derive_anchor_kind_from_stabilizer(&stabilizer, tolerance);

    Ok(SymmetryAnchorAssignment { atom_index, kind })
}

fn project_anchor_coordinate(coordinate: [f64; 3], kind: &SymmetryAnchorKind) -> [f64; 3] {
    match kind {
        SymmetryAnchorKind::CenterOfMass => [0.0, 0.0, 0.0],
        SymmetryAnchorKind::ProperRotationAxis { direction, .. } => {
            let projection = dot(coordinate, *direction);
            scale(*direction, projection)
        }
        SymmetryAnchorKind::ReflectionPlane { normal } => {
            let distance = dot(coordinate, *normal);
            sub(coordinate, scale(*normal, distance))
        }
        SymmetryAnchorKind::None => coordinate,
    }
}

fn stabilize_coordinate(
    atom_index: usize,
    coordinate: [f64; 3],
    subgroup: &BasicSubgroupSelection,
    search: &SymmetrySearchResult,
    optimized: &OptimizedSubgroupSymmetry,
) -> Result<[f64; 3], SyvaError> {
    let mut accumulated = [0.0; 3];
    let mut count = 0usize;

    for operation in subgroup
        .operations
        .iter()
        .filter(|operation| operation_participates_in_symmetrization(subgroup, &operation.kind))
        .filter(|operation| operation.permutation.get(atom_index - 1) == Some(&atom_index))
    {
        let transformed = apply_operation(
            coordinate,
            operation.index,
            operation.kind.clone(),
            &operation.permutation,
            search,
            optimized,
        )?;
        accumulated = add(accumulated, transformed);
        count += 1;
    }

    if count == 0 {
        return Ok(coordinate);
    }

    Ok(scale(accumulated, 1.0 / count as f64))
}

fn summarize_representative_class(
    geometry: &SyvaPreprocessedGeometry,
    subgroup: &BasicSubgroupSelection,
    orbit: &SubgroupOrbitPlan,
) -> SymmetryRepresentativeClassSummary {
    let representative = &geometry.active_atoms[orbit.summary.representative_atom_index - 1];

    SymmetryRepresentativeClassSummary {
        representative_atom_index: orbit.summary.representative_atom_index,
        representative_source_atom_index: representative.source_atom_index,
        atomic_number: representative.atomic_number,
        species: representative.species.clone(),
        member_atom_indices: orbit.summary.member_atom_indices.clone(),
        member_source_atom_indices: orbit
            .summary
            .member_atom_indices
            .iter()
            .map(|&atom_index| geometry.active_atoms[atom_index - 1].source_atom_index)
            .collect(),
        anchor: orbit.assignment.kind.clone(),
        members: orbit
            .members
            .iter()
            .map(|member| SymmetryOrbitMemberSummary {
                atom_index: member.atom_index,
                source_atom_index: geometry.active_atoms[member.atom_index - 1].source_atom_index,
                operation_path: member
                    .operation_positions
                    .iter()
                    .map(|&operation_position| {
                        let operation = &subgroup.operations[operation_position];
                        SymmetryOperationPathStep {
                            operation_index: operation.index,
                            label: operation.label.clone(),
                            kind: operation.kind.clone(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn build_sorted_orbit_classes_from_subgroup(subgroup: &BasicSubgroupSelection) -> Vec<Vec<usize>> {
    let atom_count = subgroup
        .operations
        .first()
        .map(|operation| operation.permutation.len())
        .unwrap_or(0);
    let mut globally_visited = vec![false; atom_count];
    let mut classes = Vec::<Vec<usize>>::new();

    for representative_atom_index in 1..=atom_count {
        if globally_visited[representative_atom_index - 1] {
            continue;
        }

        let mut members = discover_orbit_members(
            representative_atom_index,
            atom_count,
            subgroup,
            &mut globally_visited,
        )
        .into_iter()
        .map(|member| member.atom_index)
        .collect::<Vec<_>>();
        members.sort_unstable();
        classes.push(members);
    }

    classes.sort();
    classes
}

fn build_sorted_equivalence_classes(
    equivalence_classes: &[SymmetryEquivalenceClass],
) -> Vec<Vec<usize>> {
    let mut classes = equivalence_classes
        .iter()
        .map(|class| {
            let mut members = class.active_atom_indices.clone();
            members.sort_unstable();
            members
        })
        .collect::<Vec<_>>();
    classes.sort();
    classes
}

#[derive(Debug, Clone, PartialEq)]
struct SubgroupOrbitMemberPlan {
    atom_index: usize,
    operation_positions: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct SubgroupOrbitPlan {
    summary: SymmetryOrbitSummary,
    assignment: SymmetryAnchorAssignment,
    members: Vec<SubgroupOrbitMemberPlan>,
}

fn build_subgroup_orbit_plans(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    subgroup: &BasicSubgroupSelection,
    optimized: &OptimizedSubgroupSymmetry,
) -> Result<Vec<SubgroupOrbitPlan>, SyvaError> {
    let atom_count = geometry.active_atoms.len();
    let mut plans = Vec::<SubgroupOrbitPlan>::new();
    let mut globally_visited = vec![false; atom_count];

    for representative_atom_index in 1..=atom_count {
        if globally_visited[representative_atom_index - 1] {
            continue;
        }

        let mut members = discover_orbit_members(
            representative_atom_index,
            atom_count,
            subgroup,
            &mut globally_visited,
        );
        members.sort_by_key(|member| member.atom_index);
        let member_atom_indices = members
            .iter()
            .map(|member| member.atom_index)
            .collect::<Vec<_>>();
        let assignment = assign_atom_anchor(
            representative_atom_index,
            geometry,
            search,
            subgroup,
            optimized,
        )?;

        plans.push(SubgroupOrbitPlan {
            summary: SymmetryOrbitSummary {
                representative_atom_index,
                member_atom_indices,
            },
            assignment,
            members,
        });
    }

    Ok(plans)
}

fn discover_orbit_members(
    representative_atom_index: usize,
    atom_count: usize,
    subgroup: &BasicSubgroupSelection,
    globally_visited: &mut [bool],
) -> Vec<SubgroupOrbitMemberPlan> {
    let mut members = vec![SubgroupOrbitMemberPlan {
        atom_index: representative_atom_index,
        operation_positions: Vec::new(),
    }];
    let mut locally_seen = vec![false; atom_count];
    locally_seen[representative_atom_index - 1] = true;
    globally_visited[representative_atom_index - 1] = true;

    let mut frontier =
        VecDeque::<(usize, Vec<usize>)>::from([(representative_atom_index, Vec::new())]);

    while let Some((current_atom_index, current_path)) = frontier.pop_front() {
        for (operation_position, operation) in subgroup.operations.iter().enumerate() {
            if !operation_participates_in_symmetrization(subgroup, &operation.kind) {
                continue;
            }
            let Some(mapped_atom_index) =
                operation.permutation.get(current_atom_index - 1).copied()
            else {
                continue;
            };
            if mapped_atom_index == 0 || mapped_atom_index > atom_count {
                continue;
            }
            if locally_seen[mapped_atom_index - 1] {
                continue;
            }

            let mut mapped_path = current_path.clone();
            mapped_path.push(operation_position);
            locally_seen[mapped_atom_index - 1] = true;
            globally_visited[mapped_atom_index - 1] = true;
            members.push(SubgroupOrbitMemberPlan {
                atom_index: mapped_atom_index,
                operation_positions: mapped_path.clone(),
            });
            frontier.push_back((mapped_atom_index, mapped_path));
        }
    }

    members
}

fn propagate_orbit_coordinate(
    representative_coordinate: [f64; 3],
    member: &SubgroupOrbitMemberPlan,
    subgroup: &BasicSubgroupSelection,
    search: &SymmetrySearchResult,
    optimized: &OptimizedSubgroupSymmetry,
) -> Result<[f64; 3], SyvaError> {
    let mut coordinate = representative_coordinate;
    for &operation_position in &member.operation_positions {
        let operation = &subgroup.operations[operation_position];
        if !operation_participates_in_symmetrization(subgroup, &operation.kind) {
            continue;
        }
        coordinate = apply_operation(
            coordinate,
            operation.index,
            operation.kind.clone(),
            &operation.permutation,
            search,
            optimized,
        )?;
    }
    Ok(coordinate)
}

fn operation_participates_in_symmetrization(
    subgroup: &BasicSubgroupSelection,
    kind: &SymmetryElementKind,
) -> bool {
    if matches!(subgroup.label.as_str(), "Civ" | "Dih") {
        return matches!(
            kind,
            SymmetryElementKind::Identity | SymmetryElementKind::InversionCenter
        );
    }
    true
}

#[derive(Debug, Clone, PartialEq)]
struct FixedStabilizer {
    has_inversion: bool,
    proper_axes: Vec<(usize, [f64; 3])>,
    reflection_planes: Vec<[f64; 3]>,
}

fn collect_fixed_stabilizer(
    atom_index: usize,
    search: &SymmetrySearchResult,
    subgroup: &BasicSubgroupSelection,
    optimized: &OptimizedSubgroupSymmetry,
    tolerance: f64,
) -> Result<FixedStabilizer, SyvaError> {
    let mut stabilizer = FixedStabilizer {
        has_inversion: false,
        proper_axes: Vec::new(),
        reflection_planes: Vec::new(),
    };

    for operation in subgroup
        .operations
        .iter()
        .filter(|operation| operation.permutation.get(atom_index - 1) == Some(&atom_index))
    {
        match operation.kind {
            SymmetryElementKind::InversionCenter => stabilizer.has_inversion = true,
            SymmetryElementKind::ProperRotation => {
                let Some(rotation) = search
                    .proper_rotations
                    .iter()
                    .find(|candidate| candidate.permutation == operation.permutation)
                else {
                    return Err(SyvaError::MissingSymmetryElementForOperation {
                        label: operation.label.clone(),
                    });
                };
                let Some(direction) = optimized
                    .direction_for(operation.index, &operation.kind, &operation.permutation)
                    .or_else(|| normalize(rotation.direction, tolerance))
                else {
                    continue;
                };
                push_unique_axis(
                    &mut stabilizer.proper_axes,
                    rotation.order,
                    direction,
                    tolerance,
                );
            }
            SymmetryElementKind::ReflectionPlane => {
                let Some(plane) = search
                    .reflection_planes
                    .iter()
                    .find(|candidate| candidate.permutation == operation.permutation)
                else {
                    return Err(SyvaError::MissingSymmetryElementForOperation {
                        label: operation.label.clone(),
                    });
                };
                let Some(normal) = optimized
                    .direction_for(operation.index, &operation.kind, &operation.permutation)
                    .or_else(|| normalize(plane.normal, tolerance))
                else {
                    continue;
                };
                push_unique_direction(&mut stabilizer.reflection_planes, normal, tolerance);
            }
            _ => {}
        }
    }

    Ok(stabilizer)
}

fn derive_anchor_kind_from_stabilizer(
    stabilizer: &FixedStabilizer,
    tolerance: f64,
) -> SymmetryAnchorKind {
    if stabilizer.has_inversion
        || has_nonparallel_axis_pair(&stabilizer.proper_axes, tolerance)
        || axis_plane_intersects_at_origin(stabilizer, tolerance)
        || plane_span_rank(&stabilizer.reflection_planes, tolerance) >= 3
    {
        return SymmetryAnchorKind::CenterOfMass;
    }

    if let Some((order, direction)) = highest_order_axis(&stabilizer.proper_axes) {
        return SymmetryAnchorKind::ProperRotationAxis { order, direction };
    }

    if let Some(direction) = intersection_axis_from_planes(&stabilizer.reflection_planes, tolerance)
    {
        return SymmetryAnchorKind::ProperRotationAxis {
            order: 2,
            direction,
        };
    }

    if let Some(&normal) = stabilizer.reflection_planes.first() {
        return SymmetryAnchorKind::ReflectionPlane { normal };
    }

    SymmetryAnchorKind::None
}

fn push_unique_axis(
    accumulator: &mut Vec<(usize, [f64; 3])>,
    order: usize,
    direction: [f64; 3],
    tolerance: f64,
) {
    if accumulator
        .iter()
        .any(|(_, existing)| dot(*existing, direction).abs() >= 1.0 - tolerance)
    {
        return;
    }
    accumulator.push((order, direction));
}

fn push_unique_direction(accumulator: &mut Vec<[f64; 3]>, direction: [f64; 3], tolerance: f64) {
    if accumulator
        .iter()
        .any(|existing| dot(*existing, direction).abs() >= 1.0 - tolerance)
    {
        return;
    }
    accumulator.push(direction);
}

fn highest_order_axis(axes: &[(usize, [f64; 3])]) -> Option<(usize, [f64; 3])> {
    axes.iter()
        .copied()
        .max_by(|(left_order, _), (right_order, _)| left_order.cmp(right_order))
}

fn has_nonparallel_axis_pair(axes: &[(usize, [f64; 3])], tolerance: f64) -> bool {
    axes.iter().enumerate().any(|(index, (_, left))| {
        axes.iter()
            .skip(index + 1)
            .any(|(_, right)| dot(*left, *right).abs() < 1.0 - tolerance)
    })
}

fn axis_plane_intersects_at_origin(stabilizer: &FixedStabilizer, tolerance: f64) -> bool {
    stabilizer.proper_axes.iter().any(|(_, axis)| {
        stabilizer
            .reflection_planes
            .iter()
            .any(|normal| dot(*axis, *normal).abs() > tolerance)
    })
}

fn plane_span_rank(normals: &[[f64; 3]], tolerance: f64) -> usize {
    let mut basis = Vec::<[f64; 3]>::new();
    for normal in normals {
        let mut candidate = *normal;
        for existing in &basis {
            candidate = sub(candidate, scale(*existing, dot(candidate, *existing)));
        }
        let Some(candidate) = normalize(candidate, tolerance) else {
            continue;
        };
        basis.push(candidate);
    }
    basis.len()
}

fn intersection_axis_from_planes(normals: &[[f64; 3]], tolerance: f64) -> Option<[f64; 3]> {
    for (index, left) in normals.iter().enumerate() {
        for right in normals.iter().skip(index + 1) {
            let Some(direction) = normalize(cross(*left, *right), tolerance) else {
                continue;
            };
            return Some(direction);
        }
    }
    None
}

fn apply_operation(
    coordinate: [f64; 3],
    operation_index: usize,
    kind: SymmetryElementKind,
    permutation: &[usize],
    search: &SymmetrySearchResult,
    optimized: &OptimizedSubgroupSymmetry,
) -> Result<[f64; 3], SyvaError> {
    match kind {
        SymmetryElementKind::Identity => Ok(coordinate),
        SymmetryElementKind::InversionCenter => Ok(scale(coordinate, -1.0)),
        SymmetryElementKind::ReflectionPlane => {
            let Some(plane) = search
                .reflection_planes
                .iter()
                .find(|candidate| candidate.permutation == permutation)
            else {
                return Err(SyvaError::MissingSymmetryElementForOperation {
                    label: "reflection".into(),
                });
            };
            let normal = optimized
                .direction_for(operation_index, &kind, permutation)
                .or_else(|| normalize(plane.normal, 1.0e-12))
                .ok_or_else(|| SyvaError::MissingSymmetryElementForOperation {
                    label: "reflection".into(),
                })?;
            Ok(sub(
                coordinate,
                scale(normal, 2.0 * dot(coordinate, normal)),
            ))
        }
        SymmetryElementKind::ProperRotation => {
            let Some(rotation) = search
                .proper_rotations
                .iter()
                .find(|candidate| candidate.permutation == permutation)
            else {
                return Err(SyvaError::MissingSymmetryElementForOperation {
                    label: "proper rotation".into(),
                });
            };
            let axis = optimized
                .direction_for(operation_index, &kind, permutation)
                .or_else(|| normalize(rotation.direction, 1.0e-12))
                .ok_or_else(|| SyvaError::MissingSymmetryElementForOperation {
                    label: "proper rotation".into(),
                })?;
            Ok(rotate_about_axis(
                coordinate,
                axis,
                TAU * rotation.power as f64 / rotation.order as f64,
            ))
        }
        SymmetryElementKind::ImproperRotation => {
            let Some(rotation) = search
                .improper_rotations
                .iter()
                .find(|candidate| candidate.permutation == permutation)
            else {
                return Err(SyvaError::MissingSymmetryElementForOperation {
                    label: "improper rotation".into(),
                });
            };
            let axis = optimized
                .direction_for(operation_index, &kind, permutation)
                .or_else(|| normalize(rotation.direction, 1.0e-12))
                .ok_or_else(|| SyvaError::MissingSymmetryElementForOperation {
                    label: "improper rotation".into(),
                })?;
            let rotated = rotate_about_axis(
                coordinate,
                axis,
                TAU * rotation.power as f64 / rotation.order as f64,
            );
            Ok(sub(rotated, scale(axis, 2.0 * dot(rotated, axis))))
        }
        SymmetryElementKind::ProperRotationAxis => {
            Err(SyvaError::MissingSymmetryElementForOperation {
                label: "proper rotation axis".into(),
            })
        }
    }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn scale(vector: [f64; 3], factor: f64) -> [f64; 3] {
    [vector[0] * factor, vector[1] * factor, vector[2] * factor]
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize(vector: [f64; 3], delta: f64) -> Option<[f64; 3]> {
    let magnitude = norm(vector);
    if magnitude <= delta {
        None
    } else {
        Some(scale(vector, 1.0 / magnitude))
    }
}

fn rotate_about_axis(vector: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let sine = angle.sin();
    let cosine = angle.cos();
    let cross_term = cross(axis, vector);
    let projection = dot(axis, vector);
    [
        vector[0] * cosine + cross_term[0] * sine + axis[0] * projection * (1.0 - cosine),
        vector[1] * cosine + cross_term[1] * sine + axis[1] * projection * (1.0 - cosine),
        vector[2] * cosine + cross_term[2] * sine + axis[2] * projection * (1.0 - cosine),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        add, apply_operation, assign_atom_anchor, discover_orbit_members, norm,
        stabilize_coordinate, sub, symmetrize_subgroup_geometry,
        symmetrize_verified_subgroup_geometry, verify_symmetrized_geometry, SymmetryAnchorKind,
    };
    use crate::model::{ClusterAtom, ClusterGeometry, PointGroupLabel, SyvaInputGeometry};
    use crate::optimize::{
        optimize_subgroup_symmetry_elements, OptimizedOperationGeometry, OptimizedSubgroupSymmetry,
    };
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};
    use crate::subgroups::{enumerate_basic_subgroups, BasicSubgroupSelection};
    use crate::symmetry_elements::{
        search_symmetry_elements, ProperRotationRecord, ReflectionPlaneRecord, SymmetryElementKind,
        SymmetrySearchResult,
    };

    #[test]
    fn symmetrizes_perturbed_water_into_c2v_geometry() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "water".into(),
            atoms: vec![
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [0.758602, 0.0002, 0.504284],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [-0.758500, 0.0001, 0.504100],
                },
            ],
        })
        .expect("input");
        let settings = SyvaRunSettings {
            tolerance: 0.001,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "C2v")
            .expect("c2v subgroup");
        let optimized =
            optimize_subgroup_symmetry_elements(&search, &subgroup, geometry.settings.tolerance)
                .expect("optimize");

        let result =
            symmetrize_subgroup_geometry(&geometry, &search, &subgroup).expect("symmetrize");

        assert!(matches!(
            result.anchor_assignments[0].kind,
            SymmetryAnchorKind::CenterOfMass | SymmetryAnchorKind::ProperRotationAxis { .. }
        ));
        assert!(matches!(
            result.anchor_assignments[1].kind,
            SymmetryAnchorKind::ReflectionPlane { .. }
        ));
        assert_eq!(result.representative_classes.len(), 2);
        assert_eq!(
            result.representative_classes[1].member_source_atom_indices,
            vec![2, 3]
        );
        assert_eq!(
            result.representative_classes[1].members[1]
                .operation_path
                .iter()
                .map(|step| step.operation_index)
                .collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(result.orbits.len(), 2);
        assert_eq!(result.orbits[0].member_atom_indices, vec![1]);
        assert_eq!(result.orbits[1].member_atom_indices, vec![2, 3]);
        let h1 = result.coordinates[1];
        let h2 = result.coordinates[2];
        assert!((norm(h1) - norm(h2)).abs() < 1.0e-6);
        assert!(subgroup.operations.iter().any(|operation| {
            operation.permutation.get(1) == Some(&3)
                && apply_operation(
                    h1,
                    operation.index,
                    operation.kind.clone(),
                    &operation.permutation,
                    &search,
                    &optimized,
                )
                .map(|mapped| norm(sub(mapped, h2)) < 1.0e-6)
                .unwrap_or(false)
        }));
        assert!(result.verification.verified);
        assert!(result.status.success);
        assert_eq!(result.status.optimized_atom_count, 3);
        assert!(result.status.all_atoms_optimized);
        assert!(result.status.fallback_atom_indices.is_empty());
        assert!(result.verification.orbit_equivalence_alignment);
        assert_eq!(result.verification.recovered_equivalence_classes.len(), 2);
        assert_eq!(
            result
                .verification
                .recovered_point_group
                .as_ref()
                .map(crate::model::PointGroupLabel::as_str),
            Some("C2v")
        );
    }

    #[test]
    fn symmetrizes_linear_co2_center_atom_to_origin() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "co2".into(),
            atoms: vec![
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [-1.16, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "C".into(),
                    cartesian: [0.0001, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [1.1598, 0.0, 0.0],
                },
            ],
        })
        .expect("input");
        let settings = SyvaRunSettings {
            tolerance: 0.001,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "Dih")
            .expect("dih subgroup");

        let result =
            symmetrize_subgroup_geometry(&geometry, &search, &subgroup).expect("symmetrize");
        let carbon = result.coordinates[1];
        assert!(carbon[0].abs() < 1.0e-8);
        assert!(carbon[1].abs() < 1.0e-8);
        assert!(carbon[2].abs() < 1.0e-8);
        assert_eq!(result.orbits.len(), 2);
        assert_eq!(result.orbits[0].member_atom_indices, vec![1, 3]);
        assert_eq!(result.orbits[1].member_atom_indices, vec![2]);
        assert_eq!(
            result.representative_classes[0].members[1]
                .operation_path
                .iter()
                .map(|step| step.label.as_str())
                .collect::<Vec<_>>(),
            vec!["i"]
        );
        assert!(result.verification.verified);
        assert!(result.status.success);
        assert_eq!(result.status.optimized_atom_count, 3);
        assert!(result.status.all_atoms_optimized);
        assert!(result.status.fallback_atom_indices.is_empty());
        assert!(result.verification.orbit_equivalence_alignment);
        assert_eq!(
            result
                .verification
                .recovered_point_group
                .as_ref()
                .map(crate::model::PointGroupLabel::as_str),
            Some("Dih")
        );
    }

    #[test]
    fn verification_detects_when_expected_subgroup_is_not_recovered() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "water".into(),
            atoms: vec![
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [0.8, 0.2, 0.5],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [-0.6, -0.1, 0.4],
                },
            ],
        })
        .expect("input");
        let settings = SyvaRunSettings {
            tolerance: 0.001,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let expected_subgroup = crate::subgroups::BasicSubgroupSelection {
            label: crate::model::PointGroupLabel::new("C2v").expect("label"),
            operations: enumerate_basic_subgroups(&geometry, &search)
                .expect("subgroups")
                .into_iter()
                .find(|subgroup| subgroup.label.as_str() == "C1")
                .expect("c1 subgroup")
                .operations,
        };

        let verification = verify_symmetrized_geometry(
            &geometry,
            &[[0.1, 0.0, 0.0], [0.8, 0.2, 0.5], [-0.6, -0.1, 0.4]],
            &expected_subgroup,
        )
        .expect("verify");

        assert!(!verification.verified);
        assert!(verification.orbit_equivalence_alignment);
    }

    #[test]
    fn verification_detects_when_recovered_equivalence_classes_break_expected_orbits() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "water".into(),
            atoms: vec![
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [0.7586, 0.0, 0.5043],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [-0.7586, 0.0, 0.5043],
                },
            ],
        })
        .expect("input");
        let settings = SyvaRunSettings {
            tolerance: 0.001,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let expected_subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "C2v")
            .expect("c2v subgroup");

        let verification = verify_symmetrized_geometry(
            &geometry,
            &[[0.0, 0.0, 0.0], [0.9, 0.2, 0.4], [-0.6, -0.3, 0.7]],
            &expected_subgroup,
        )
        .expect("verify");

        assert!(!verification.verified);
        assert!(!verification.orbit_equivalence_alignment);
    }

    #[test]
    fn strict_symmetrization_rejects_unverified_geometry() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "water".into(),
            atoms: vec![
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [0.8, 0.2, 0.5],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [-0.6, -0.1, 0.4],
                },
            ],
        })
        .expect("input");
        let settings = SyvaRunSettings {
            tolerance: 0.001,
            ..SyvaRunSettings::default()
        };
        let geometry = preprocess_geometry(&input, &settings).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = crate::subgroups::BasicSubgroupSelection {
            label: crate::model::PointGroupLabel::new("C2v").expect("label"),
            operations: enumerate_basic_subgroups(&geometry, &search)
                .expect("subgroups")
                .into_iter()
                .find(|subgroup| subgroup.label.as_str() == "C1")
                .expect("c1 subgroup")
                .operations,
        };

        let error = symmetrize_verified_subgroup_geometry(&geometry, &search, &subgroup)
            .expect_err("strict failure");
        assert!(matches!(
            error,
            crate::SyvaError::SymmetrizationVerificationFailed {
                requested_point_group,
                optimized_atom_count: 3,
                ..
            } if requested_point_group == "C2v"
        ));
    }

    #[test]
    fn stabilizer_average_projects_onto_intersection_of_fixed_elements() {
        let subgroup = BasicSubgroupSelection {
            label: PointGroupLabel::new("C2v").expect("label"),
            operations: vec![
                crate::operations::SymmetryOperationSummary {
                    index: 1,
                    label: "E".into(),
                    kind: SymmetryElementKind::Identity,
                    permutation: vec![1],
                    fixed_atom_count: 1,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 2,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1],
                    fixed_atom_count: 1,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 3,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1],
                    fixed_atom_count: 1,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 4,
                    label: "C2".into(),
                    kind: SymmetryElementKind::ProperRotation,
                    permutation: vec![1],
                    fixed_atom_count: 1,
                    max_deviation: 0.0,
                },
            ],
        };
        let search = SymmetrySearchResult {
            center_atom_index: None,
            is_linear: false,
            is_planar: false,
            inversion_center: None,
            reflection_planes: vec![
                ReflectionPlaneRecord {
                    normal: [1.0, 0.0, 0.0],
                    fixed_atom_count: 1,
                    permutation: vec![1],
                    max_deviation: 0.0,
                },
                ReflectionPlaneRecord {
                    normal: [0.0, 1.0, 0.0],
                    fixed_atom_count: 1,
                    permutation: vec![1],
                    max_deviation: 0.0,
                },
            ],
            proper_rotation_axes: Vec::new(),
            proper_rotations: vec![ProperRotationRecord {
                direction: [0.0, 0.0, 1.0],
                order: 2,
                power: 1,
                fixed_atom_count: 1,
                permutation: vec![1],
                max_deviation: 0.0,
            }],
            improper_rotations: Vec::new(),
            permutations: vec![crate::symmetry_elements::PermutationRecord { mapping: vec![1] }],
            max_deviation: 0.0,
        };
        let optimized = OptimizedSubgroupSymmetry {
            point_group: PointGroupLabel::new("C2v").expect("label"),
            operations: vec![
                OptimizedOperationGeometry {
                    index: 2,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1],
                    direction: [1.0, 0.0, 0.0],
                },
                OptimizedOperationGeometry {
                    index: 3,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1],
                    direction: [0.0, 1.0, 0.0],
                },
                OptimizedOperationGeometry {
                    index: 4,
                    label: "C2".into(),
                    kind: SymmetryElementKind::ProperRotation,
                    permutation: vec![1],
                    direction: [0.0, 0.0, 1.0],
                },
            ],
        };

        let stabilized = stabilize_coordinate(1, [1.0, 2.0, 3.0], &subgroup, &search, &optimized)
            .expect("stabilize");

        assert!(norm(sub(stabilized, [0.0, 0.0, 3.0])) < 1.0e-10);
        assert_eq!(add(stabilized, [0.0, 0.0, 0.0]), stabilized);
    }

    #[test]
    fn anchor_assignment_uses_plane_intersection_as_axis() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "line-anchor".into(),
            atoms: vec![
                ClusterAtom {
                    species: "C".into(),
                    cartesian: [0.0, 0.0, 3.0],
                },
                ClusterAtom {
                    species: "H".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
            ],
        })
        .expect("input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let subgroup = BasicSubgroupSelection {
            label: PointGroupLabel::new("C2v").expect("label"),
            operations: vec![
                crate::operations::SymmetryOperationSummary {
                    index: 1,
                    label: "E".into(),
                    kind: SymmetryElementKind::Identity,
                    permutation: vec![1, 2],
                    fixed_atom_count: 2,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 2,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1, 2],
                    fixed_atom_count: 2,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 3,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1, 2],
                    fixed_atom_count: 2,
                    max_deviation: 0.0,
                },
            ],
        };
        let search = SymmetrySearchResult {
            center_atom_index: None,
            is_linear: false,
            is_planar: false,
            inversion_center: None,
            reflection_planes: vec![
                ReflectionPlaneRecord {
                    normal: [1.0, 0.0, 0.0],
                    fixed_atom_count: 2,
                    permutation: vec![1, 2],
                    max_deviation: 0.0,
                },
                ReflectionPlaneRecord {
                    normal: [0.0, 1.0, 0.0],
                    fixed_atom_count: 2,
                    permutation: vec![1, 2],
                    max_deviation: 0.0,
                },
            ],
            proper_rotation_axes: Vec::new(),
            proper_rotations: Vec::new(),
            improper_rotations: Vec::new(),
            permutations: vec![crate::symmetry_elements::PermutationRecord {
                mapping: vec![1, 2],
            }],
            max_deviation: 0.0,
        };
        let optimized = OptimizedSubgroupSymmetry {
            point_group: PointGroupLabel::new("C2v").expect("label"),
            operations: vec![
                OptimizedOperationGeometry {
                    index: 2,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1, 2],
                    direction: [1.0, 0.0, 0.0],
                },
                OptimizedOperationGeometry {
                    index: 3,
                    label: "Sigma_v".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1, 2],
                    direction: [0.0, 1.0, 0.0],
                },
            ],
        };

        let assignment =
            assign_atom_anchor(1, &geometry, &search, &subgroup, &optimized).expect("assignment");

        assert!(matches!(
            assignment.kind,
            SymmetryAnchorKind::ProperRotationAxis { order: 2, direction }
                if norm(sub(direction, [0.0, 0.0, 1.0])) < 1.0e-10
                    || norm(sub(direction, [0.0, 0.0, -1.0])) < 1.0e-10
        ));
    }

    #[test]
    fn orbit_discovery_tracks_composite_member_paths() {
        let subgroup = BasicSubgroupSelection {
            label: PointGroupLabel::new("C1").expect("label"),
            operations: vec![
                crate::operations::SymmetryOperationSummary {
                    index: 1,
                    label: "E".into(),
                    kind: SymmetryElementKind::Identity,
                    permutation: vec![1, 2, 3],
                    fixed_atom_count: 3,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 2,
                    label: "swap12".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![2, 1, 3],
                    fixed_atom_count: 1,
                    max_deviation: 0.0,
                },
                crate::operations::SymmetryOperationSummary {
                    index: 3,
                    label: "swap23".into(),
                    kind: SymmetryElementKind::ReflectionPlane,
                    permutation: vec![1, 3, 2],
                    fixed_atom_count: 1,
                    max_deviation: 0.0,
                },
            ],
        };
        let mut visited = vec![false; 3];
        let members = discover_orbit_members(1, 3, &subgroup, &mut visited);

        assert_eq!(members.len(), 3);
        assert_eq!(members[0].atom_index, 1);
        assert!(members[0].operation_positions.is_empty());
        assert_eq!(members[1].atom_index, 2);
        assert_eq!(members[1].operation_positions, vec![1]);
        assert_eq!(members[2].atom_index, 3);
        assert_eq!(members[2].operation_positions, vec![1, 2]);
    }
}
