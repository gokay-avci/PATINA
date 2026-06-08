use crate::classify::classify_point_group;
use crate::model::PointGroupLabel;
use crate::operations::{summarize_operations, SymmetryOperationSummary};
use crate::preprocess::SyvaPreprocessedGeometry;
use crate::symmetry_elements::{SymmetryElementKind, SymmetrySearchResult};
use crate::SyvaError;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasicSubgroupSelection {
    pub label: PointGroupLabel,
    pub operations: Vec<SymmetryOperationSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PrincipalAxisSelection {
    order: usize,
    direction: [f64; 3],
}

#[derive(Debug, Clone, PartialEq)]
struct SelectedOperationKey {
    kind: SymmetryElementKind,
    permutation: Vec<usize>,
}

pub fn enumerate_basic_subgroups(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Result<Vec<BasicSubgroupSelection>, SyvaError> {
    let mut subgroups = Vec::<BasicSubgroupSelection>::new();

    subgroups.push(BasicSubgroupSelection {
        label: PointGroupLabel::new("C1")?,
        operations: vec![identity_operation(geometry.active_atoms.len())],
    });

    if let Some(inversion) = &search.inversion_center {
        subgroups.push(BasicSubgroupSelection {
            label: PointGroupLabel::new("Ci")?,
            operations: vec![
                identity_operation(geometry.active_atoms.len()),
                SymmetryOperationSummary {
                    index: 2,
                    label: "i".into(),
                    kind: crate::symmetry_elements::SymmetryElementKind::InversionCenter,
                    permutation: inversion.permutation.clone(),
                    fixed_atom_count: usize::from(search.center_atom_index.is_some()),
                    max_deviation: inversion.max_deviation,
                },
            ],
        });
    }

    if let Some(best_plane) = best_plane(search) {
        subgroups.push(BasicSubgroupSelection {
            label: PointGroupLabel::new("Cs")?,
            operations: vec![
                identity_operation(geometry.active_atoms.len()),
                SymmetryOperationSummary {
                    index: 2,
                    label: "Sigma".into(),
                    kind: crate::symmetry_elements::SymmetryElementKind::ReflectionPlane,
                    permutation: best_plane.permutation.clone(),
                    fixed_atom_count: best_plane.fixed_atom_count,
                    max_deviation: best_plane.max_deviation,
                },
            ],
        });
    }

    if !search.is_linear {
        if let Some(principal_axis) = principal_axis(search, geometry.settings.tolerance) {
            push_cyclic_family_subgroups(geometry, search, principal_axis, &mut subgroups)?;
            push_dihedral_family_subgroups(geometry, search, principal_axis, &mut subgroups)?;
        }
        push_improper_cyclic_subgroups(geometry, search, &mut subgroups)?;
    }
    push_special_case_subgroups(geometry, search, &mut subgroups)?;

    dedup_subgroups(&mut subgroups);
    Ok(subgroups)
}

fn identity_operation(atom_count: usize) -> SymmetryOperationSummary {
    SymmetryOperationSummary {
        index: 1,
        label: "E".into(),
        kind: crate::symmetry_elements::SymmetryElementKind::Identity,
        permutation: (1..=atom_count).collect(),
        fixed_atom_count: atom_count,
        max_deviation: 0.0,
    }
}

fn best_plane(
    search: &SymmetrySearchResult,
) -> Option<&crate::symmetry_elements::ReflectionPlaneRecord> {
    search.reflection_planes.iter().max_by(|left, right| {
        left.fixed_atom_count
            .cmp(&right.fixed_atom_count)
            .then_with(|| right.max_deviation.total_cmp(&left.max_deviation))
    })
}

fn principal_axis(search: &SymmetrySearchResult, delta: f64) -> Option<PrincipalAxisSelection> {
    let highest_improper = highest_improper_axis(search, delta);
    search
        .proper_rotation_axes
        .iter()
        .max_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| {
                    let left_alignment = axis_alignment_score(
                        left.direction,
                        highest_improper.as_ref().map(|axis| axis.direction),
                        delta,
                    );
                    let right_alignment = axis_alignment_score(
                        right.direction,
                        highest_improper.as_ref().map(|axis| axis.direction),
                        delta,
                    );
                    left_alignment.cmp(&right_alignment)
                })
                .then_with(|| left.fixed_atom_count.cmp(&right.fixed_atom_count))
        })
        .and_then(|axis| {
            normalize(axis.direction, delta).map(|direction| PrincipalAxisSelection {
                order: axis.order,
                direction,
            })
        })
}

fn axis_alignment_score(direction: [f64; 3], reference: Option<[f64; 3]>, delta: f64) -> u8 {
    let Some(reference) = reference else {
        return 0;
    };
    let Some(direction) = normalize(direction, delta) else {
        return 0;
    };
    u8::from(dot(direction, reference).abs() >= 1.0 - delta)
}

fn push_cyclic_family_subgroups(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    principal_axis: PrincipalAxisSelection,
    subgroups: &mut Vec<BasicSubgroupSelection>,
) -> Result<(), SyvaError> {
    if principal_axis.order <= 1 {
        return Ok(());
    }

    let principal_rotations = principal_rotation_permutations(
        search,
        principal_axis.order,
        principal_axis.direction,
        geometry.settings.tolerance,
    );

    if let Some(subgroup) = build_filtered_subgroup(
        geometry,
        search,
        &format!("C{}", principal_axis.order),
        principal_axis.order,
        &principal_rotations,
    )? {
        subgroups.push(subgroup);
    }

    if let Some(vertical_plane_indices) = select_plane_orbit(
        &search.reflection_planes,
        principal_axis.direction,
        principal_axis.order,
        geometry.settings.tolerance,
    ) {
        let mut permutations = principal_rotations.clone();
        for plane_index in vertical_plane_indices {
            push_unique_operation(
                &mut permutations,
                SymmetryElementKind::ReflectionPlane,
                search.reflection_planes[plane_index].permutation.clone(),
            );
        }

        if let Some(subgroup) = build_filtered_subgroup(
            geometry,
            search,
            &format!("C{}v", principal_axis.order),
            2 * principal_axis.order,
            &permutations,
        )? {
            subgroups.push(subgroup);
        }
    }

    if let Some(horizontal_plane_index) = horizontal_plane_index(
        search,
        principal_axis.direction,
        geometry.settings.tolerance,
    ) {
        let mut operations = principal_rotations;
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ReflectionPlane,
            search.reflection_planes[horizontal_plane_index]
                .permutation
                .clone(),
        );
        if principal_axis.order.is_multiple_of(2) {
            if let Some(inversion) = &search.inversion_center {
                push_unique_operation(
                    &mut operations,
                    SymmetryElementKind::InversionCenter,
                    inversion.permutation.clone(),
                );
            }
        }
        for operation in principal_improper_permutations(
            search,
            principal_axis.order,
            principal_axis.direction,
            geometry.settings.tolerance,
        ) {
            push_unique_operation(&mut operations, operation.kind, operation.permutation);
        }

        if let Some(subgroup) = build_filtered_subgroup(
            geometry,
            search,
            &format!("C{}h", principal_axis.order),
            2 * principal_axis.order,
            &operations,
        )? {
            subgroups.push(subgroup);
        }
    }

    Ok(())
}

fn push_dihedral_family_subgroups(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    principal_axis: PrincipalAxisSelection,
    subgroups: &mut Vec<BasicSubgroupSelection>,
) -> Result<(), SyvaError> {
    if principal_axis.order <= 1 {
        return Ok(());
    }

    let principal_rotations = principal_rotation_permutations(
        search,
        principal_axis.order,
        principal_axis.direction,
        geometry.settings.tolerance,
    );
    let Some(perpendicular_c2_indices) = select_perpendicular_c2_orbit(
        &search.proper_rotations,
        principal_axis.direction,
        principal_axis.order,
        geometry.settings.tolerance,
    ) else {
        return Ok(());
    };

    let mut dihedral_permutations = principal_rotations.clone();
    for rotation_index in &perpendicular_c2_indices {
        push_unique_operation(
            &mut dihedral_permutations,
            SymmetryElementKind::ProperRotation,
            search.proper_rotations[*rotation_index].permutation.clone(),
        );
    }

    if let Some(subgroup) = build_filtered_subgroup(
        geometry,
        search,
        &format!("D{}", principal_axis.order),
        2 * principal_axis.order,
        &dihedral_permutations,
    )? {
        subgroups.push(subgroup);
    }

    if let Some(dihedral_plane_indices) = select_plane_orbit(
        &search.reflection_planes,
        principal_axis.direction,
        principal_axis.order,
        geometry.settings.tolerance,
    ) {
        let mut dnd_permutations = dihedral_permutations.clone();
        for plane_index in dihedral_plane_indices {
            push_unique_operation(
                &mut dnd_permutations,
                SymmetryElementKind::ReflectionPlane,
                search.reflection_planes[plane_index].permutation.clone(),
            );
        }
        if principal_axis.order % 2 == 1 {
            if let Some(inversion) = &search.inversion_center {
                push_unique_operation(
                    &mut dnd_permutations,
                    SymmetryElementKind::InversionCenter,
                    inversion.permutation.clone(),
                );
            }
        }
        for operation in principal_improper_permutations(
            search,
            2 * principal_axis.order,
            principal_axis.direction,
            geometry.settings.tolerance,
        ) {
            push_unique_operation(&mut dnd_permutations, operation.kind, operation.permutation);
        }

        if let Some(subgroup) = build_filtered_subgroup(
            geometry,
            search,
            &format!("D{}d", principal_axis.order),
            4 * principal_axis.order,
            &dnd_permutations,
        )? {
            subgroups.push(subgroup);
        }
    }

    let Some(horizontal_plane_index) = horizontal_plane_index(
        search,
        principal_axis.direction,
        geometry.settings.tolerance,
    ) else {
        return Ok(());
    };
    let Some(vertical_plane_indices) = select_plane_orbit(
        &search.reflection_planes,
        principal_axis.direction,
        principal_axis.order,
        geometry.settings.tolerance,
    ) else {
        return Ok(());
    };

    let mut dnh_permutations = dihedral_permutations;
    push_unique_operation(
        &mut dnh_permutations,
        SymmetryElementKind::ReflectionPlane,
        search.reflection_planes[horizontal_plane_index]
            .permutation
            .clone(),
    );
    for plane_index in vertical_plane_indices {
        push_unique_operation(
            &mut dnh_permutations,
            SymmetryElementKind::ReflectionPlane,
            search.reflection_planes[plane_index].permutation.clone(),
        );
    }
    if principal_axis.order.is_multiple_of(2) {
        if let Some(inversion) = &search.inversion_center {
            push_unique_operation(
                &mut dnh_permutations,
                SymmetryElementKind::InversionCenter,
                inversion.permutation.clone(),
            );
        }
    }
    for operation in principal_improper_permutations(
        search,
        principal_axis.order,
        principal_axis.direction,
        geometry.settings.tolerance,
    ) {
        push_unique_operation(&mut dnh_permutations, operation.kind, operation.permutation);
    }

    if let Some(subgroup) = build_filtered_subgroup(
        geometry,
        search,
        &format!("D{}h", principal_axis.order),
        4 * principal_axis.order,
        &dnh_permutations,
    )? {
        subgroups.push(subgroup);
    }

    Ok(())
}

fn push_improper_cyclic_subgroups(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    subgroups: &mut Vec<BasicSubgroupSelection>,
) -> Result<(), SyvaError> {
    let Some(improper_axis) = highest_improper_axis(search, geometry.settings.tolerance) else {
        return Ok(());
    };
    if improper_axis.order <= 2 || improper_axis.order % 2 != 0 {
        return Ok(());
    }

    let mut operations = principal_rotation_permutations(
        search,
        improper_axis.order / 2,
        improper_axis.direction,
        geometry.settings.tolerance,
    );
    if improper_axis.order % 4 == 2 {
        if let Some(inversion) = &search.inversion_center {
            push_unique_operation(
                &mut operations,
                SymmetryElementKind::InversionCenter,
                inversion.permutation.clone(),
            );
        }
    }
    for operation in principal_improper_permutations(
        search,
        improper_axis.order,
        improper_axis.direction,
        geometry.settings.tolerance,
    ) {
        push_unique_operation(&mut operations, operation.kind, operation.permutation);
    }

    if let Some(subgroup) = build_filtered_subgroup(
        geometry,
        search,
        &format!("S{}", improper_axis.order),
        improper_axis.order,
        &operations,
    )? {
        subgroups.push(subgroup);
    }

    Ok(())
}

fn push_special_case_subgroups(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    subgroups: &mut Vec<BasicSubgroupSelection>,
) -> Result<(), SyvaError> {
    let Some(classified) = classify_point_group(search)? else {
        return Ok(());
    };

    match classified.label.as_str() {
        "Civ" | "Dih" | "Td" | "T" | "Th" | "O" | "Oh" | "I" | "Ih" => {
            subgroups.push(full_group_subgroup(
                geometry,
                search,
                classified.label.as_str(),
            )?);
        }
        _ => {}
    }

    match classified.label.as_str() {
        "Td" | "T" | "Th" | "O" | "Oh" => {
            if let Some(subgroup) = build_tetrahedral_proper_subgroup(geometry, search)? {
                subgroups.push(subgroup);
            }
        }
        _ => {}
    }

    match classified.label.as_str() {
        "Oh" => {
            if let Some(subgroup) = build_polyhedral_proper_subgroup(geometry, search, "O")? {
                subgroups.push(subgroup);
            }
            if let Some(subgroup) = build_th_subgroup(geometry, search)? {
                subgroups.push(subgroup);
            }
            if let Some(subgroup) = build_td_subgroup(geometry, search)? {
                subgroups.push(subgroup);
            }
        }
        "Ih" => {
            if let Some(subgroup) = build_polyhedral_proper_subgroup(geometry, search, "I")? {
                subgroups.push(subgroup);
            }
        }
        _ => {}
    }

    Ok(())
}

fn full_group_subgroup(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    label: &str,
) -> Result<BasicSubgroupSelection, SyvaError> {
    let point_group = PointGroupLabel::new(label)?;
    Ok(BasicSubgroupSelection {
        label: point_group.clone(),
        operations: summarize_operations(&point_group, geometry, search),
    })
}

fn build_polyhedral_proper_subgroup(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    label: &str,
) -> Result<Option<BasicSubgroupSelection>, SyvaError> {
    let mut operations = Vec::<SelectedOperationKey>::new();
    for rotation in &search.proper_rotations {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ProperRotation,
            rotation.permutation.clone(),
        );
    }

    let expected_count = 1 + operations.len();
    build_filtered_subgroup(geometry, search, label, expected_count, &operations)
}

fn build_tetrahedral_proper_subgroup(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Result<Option<BasicSubgroupSelection>, SyvaError> {
    let mut operations = Vec::<SelectedOperationKey>::new();
    for rotation in &search.proper_rotations {
        if rotation.order == 3 {
            push_unique_operation(
                &mut operations,
                SymmetryElementKind::ProperRotation,
                rotation.permutation.clone(),
            );
        }
    }

    let c2_indices = tetrahedral_c2_indices(search, geometry.settings.tolerance);
    for rotation_index in c2_indices {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ProperRotation,
            search.proper_rotations[rotation_index].permutation.clone(),
        );
    }

    build_filtered_subgroup(geometry, search, "T", 12, &operations)
}

fn build_td_subgroup(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Result<Option<BasicSubgroupSelection>, SyvaError> {
    let mut operations = Vec::<SelectedOperationKey>::new();
    for plane_index in sigma_d_plane_indices(search, geometry.settings.tolerance) {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ReflectionPlane,
            search.reflection_planes[plane_index].permutation.clone(),
        );
    }
    for rotation in &search.proper_rotations {
        if rotation.order == 3 {
            push_unique_operation(
                &mut operations,
                SymmetryElementKind::ProperRotation,
                rotation.permutation.clone(),
            );
        }
    }
    for rotation_index in tetrahedral_c2_indices(search, geometry.settings.tolerance) {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ProperRotation,
            search.proper_rotations[rotation_index].permutation.clone(),
        );
    }
    for improper in &search.improper_rotations {
        if improper.order == 4 {
            push_unique_operation(
                &mut operations,
                SymmetryElementKind::ImproperRotation,
                improper.permutation.clone(),
            );
        }
    }

    build_filtered_subgroup(geometry, search, "Td", 24, &operations)
}

fn build_th_subgroup(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Result<Option<BasicSubgroupSelection>, SyvaError> {
    let mut operations = Vec::<SelectedOperationKey>::new();

    let c3_directions = search
        .proper_rotations
        .iter()
        .filter(|rotation| rotation.order == 3)
        .filter_map(|rotation| normalize(rotation.direction, geometry.settings.tolerance))
        .collect::<Vec<_>>();
    for rotation in &search.proper_rotations {
        if rotation.order == 3 {
            push_unique_operation(
                &mut operations,
                SymmetryElementKind::ProperRotation,
                rotation.permutation.clone(),
            );
        }
    }

    let c2_indices = tetrahedral_c2_indices(search, geometry.settings.tolerance);
    for rotation_index in &c2_indices {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ProperRotation,
            search.proper_rotations[*rotation_index].permutation.clone(),
        );
    }

    if let Some(inversion) = &search.inversion_center {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::InversionCenter,
            inversion.permutation.clone(),
        );
    }

    for improper in &search.improper_rotations {
        if improper.order != 6 {
            continue;
        }
        let Some(direction) = normalize(improper.direction, geometry.settings.tolerance) else {
            continue;
        };
        if c3_directions
            .iter()
            .any(|candidate| dot(*candidate, direction).abs() >= 1.0 - geometry.settings.tolerance)
        {
            push_unique_operation(
                &mut operations,
                SymmetryElementKind::ImproperRotation,
                improper.permutation.clone(),
            );
        }
    }

    for plane_index in c2_aligned_plane_indices(search, &c2_indices, geometry.settings.tolerance) {
        push_unique_operation(
            &mut operations,
            SymmetryElementKind::ReflectionPlane,
            search.reflection_planes[plane_index].permutation.clone(),
        );
    }

    build_filtered_subgroup(geometry, search, "Th", 24, &operations)
}

fn build_filtered_subgroup(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    label: &str,
    expected_operation_count: usize,
    selected_operations: &[SelectedOperationKey],
) -> Result<Option<BasicSubgroupSelection>, SyvaError> {
    let target = PointGroupLabel::new(label)?;
    let mut operations = summarize_operations(&target, geometry, search)
        .into_iter()
        .filter(|operation| {
            operation.kind == SymmetryElementKind::Identity
                || contains_operation(selected_operations, &operation.kind, &operation.permutation)
        })
        .collect::<Vec<_>>();
    reindex_operations(&mut operations);

    if operations.len() != expected_operation_count {
        return Ok(None);
    }

    Ok(Some(BasicSubgroupSelection {
        label: target,
        operations,
    }))
}

fn principal_rotation_permutations(
    search: &SymmetrySearchResult,
    target_order: usize,
    principal_axis: [f64; 3],
    delta: f64,
) -> Vec<SelectedOperationKey> {
    let mut permutations = Vec::<SelectedOperationKey>::new();
    for rotation in &search.proper_rotations {
        let Some(direction) = normalize(rotation.direction, delta) else {
            continue;
        };
        if dot(direction, principal_axis).abs() < 1.0 - delta
            || !target_order.is_multiple_of(rotation.order)
        {
            continue;
        }
        push_unique_operation(
            &mut permutations,
            SymmetryElementKind::ProperRotation,
            rotation.permutation.clone(),
        );
    }
    permutations
}

fn principal_improper_permutations(
    search: &SymmetrySearchResult,
    target_order: usize,
    principal_axis: [f64; 3],
    delta: f64,
) -> Vec<SelectedOperationKey> {
    let mut permutations = Vec::<SelectedOperationKey>::new();
    for rotation in &search.improper_rotations {
        let Some(direction) = normalize(rotation.direction, delta) else {
            continue;
        };
        if dot(direction, principal_axis).abs() < 1.0 - delta
            || !target_order.is_multiple_of(rotation.order)
        {
            continue;
        }
        push_unique_operation(
            &mut permutations,
            SymmetryElementKind::ImproperRotation,
            rotation.permutation.clone(),
        );
    }
    permutations
}

fn horizontal_plane_index(
    search: &SymmetrySearchResult,
    principal_axis: [f64; 3],
    delta: f64,
) -> Option<usize> {
    search
        .reflection_planes
        .iter()
        .enumerate()
        .filter_map(|(index, plane)| {
            normalize(plane.normal, delta)
                .filter(|normal| dot(*normal, principal_axis).abs() >= 1.0 - delta)
                .map(|_| index)
        })
        .max_by(|left, right| {
            plane_ranked_key(&search.reflection_planes[*left])
                .cmp(&plane_ranked_key(&search.reflection_planes[*right]))
        })
}

fn highest_improper_axis(
    search: &SymmetrySearchResult,
    delta: f64,
) -> Option<PrincipalAxisSelection> {
    search
        .improper_rotations
        .iter()
        .max_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.fixed_atom_count.cmp(&right.fixed_atom_count))
        })
        .and_then(|rotation| {
            normalize(rotation.direction, delta).map(|direction| PrincipalAxisSelection {
                order: rotation.order,
                direction,
            })
        })
}

fn tetrahedral_c2_indices(search: &SymmetrySearchResult, delta: f64) -> Vec<usize> {
    let improper_axes = unique_improper_axes(search, 4, delta);

    let mut indices = search
        .proper_rotations
        .iter()
        .enumerate()
        .filter(|(_, rotation)| rotation.order == 2)
        .filter_map(|(index, rotation)| {
            let direction = normalize(rotation.direction, delta)?;
            if improper_axes.is_empty()
                || improper_axes
                    .iter()
                    .any(|axis| dot(*axis, direction).abs() >= 1.0 - delta)
            {
                Some(index)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    indices.sort_by(|left, right| {
        proper_rotation_ranked_key(&search.proper_rotations[*left]).cmp(
            &proper_rotation_ranked_key(&search.proper_rotations[*right]),
        )
    });
    indices.reverse();
    indices
}

fn sigma_d_plane_indices(search: &SymmetrySearchResult, delta: f64) -> Vec<usize> {
    let improper_axes = unique_improper_axes(search, 4, delta);

    let mut indices = search
        .reflection_planes
        .iter()
        .enumerate()
        .filter_map(|(index, plane)| {
            let normal = normalize(plane.normal, delta)?;
            let orthogonal_axis_count = improper_axes
                .iter()
                .filter(|axis| dot(**axis, normal).abs() <= delta)
                .count();
            if improper_axes.is_empty() || orthogonal_axis_count == 1 {
                Some(index)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        plane_ranked_key(&search.reflection_planes[*left])
            .cmp(&plane_ranked_key(&search.reflection_planes[*right]))
    });
    indices.reverse();
    indices
}

fn c2_aligned_plane_indices(
    search: &SymmetrySearchResult,
    c2_indices: &[usize],
    delta: f64,
) -> Vec<usize> {
    let c2_axes = c2_indices
        .iter()
        .filter_map(|index| normalize(search.proper_rotations[*index].direction, delta))
        .collect::<Vec<_>>();

    let mut indices = search
        .reflection_planes
        .iter()
        .enumerate()
        .filter_map(|(index, plane)| {
            let normal = normalize(plane.normal, delta)?;
            c2_axes
                .iter()
                .any(|axis| dot(*axis, normal).abs() >= 1.0 - delta)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        plane_ranked_key(&search.reflection_planes[*left])
            .cmp(&plane_ranked_key(&search.reflection_planes[*right]))
    });
    indices.reverse();
    indices
}

fn unique_improper_axes(search: &SymmetrySearchResult, order: usize, delta: f64) -> Vec<[f64; 3]> {
    let mut axes = Vec::<[f64; 3]>::new();
    for rotation in &search.improper_rotations {
        if rotation.order != order {
            continue;
        }
        let Some(direction) = normalize(rotation.direction, delta) else {
            continue;
        };
        if axes
            .iter()
            .any(|existing| dot(*existing, direction).abs() >= 1.0 - delta)
        {
            continue;
        }
        axes.push(direction);
    }
    axes
}

fn select_plane_orbit(
    planes: &[crate::symmetry_elements::ReflectionPlaneRecord],
    principal_axis: [f64; 3],
    order: usize,
    delta: f64,
) -> Option<Vec<usize>> {
    let candidate_indices = ranked_plane_indices(planes, principal_axis, delta);
    if candidate_indices.len() < order {
        return None;
    }

    let step = PI / order as f64;
    for seed_index in candidate_indices.iter().copied() {
        let seed_direction = normalize(planes[seed_index].normal, delta)?;
        let mut orbit = Vec::<usize>::with_capacity(order);
        for orbit_index in 0..order {
            let rotated =
                rotate_about_axis(seed_direction, principal_axis, orbit_index as f64 * step);
            let match_index = candidate_indices
                .iter()
                .copied()
                .filter(|candidate_index| !orbit.contains(candidate_index))
                .filter_map(|candidate_index| {
                    let candidate = normalize(planes[candidate_index].normal, delta)?;
                    let alignment = dot(candidate, rotated).abs();
                    (alignment >= 1.0 - delta).then_some((candidate_index, alignment))
                })
                .max_by(
                    |(left_index, left_alignment), (right_index, right_alignment)| {
                        left_alignment.total_cmp(right_alignment).then_with(|| {
                            plane_ranked_key(&planes[*left_index])
                                .cmp(&plane_ranked_key(&planes[*right_index]))
                        })
                    },
                )
                .map(|(index, _)| index);
            let Some(match_index) = match_index else {
                orbit.clear();
                break;
            };
            orbit.push(match_index);
        }
        if orbit.len() == order {
            return Some(orbit);
        }
    }

    None
}

fn select_perpendicular_c2_orbit(
    rotations: &[crate::symmetry_elements::ProperRotationRecord],
    principal_axis: [f64; 3],
    order: usize,
    delta: f64,
) -> Option<Vec<usize>> {
    let candidate_indices = ranked_perpendicular_c2_indices(rotations, principal_axis, delta);
    if candidate_indices.len() < order {
        return None;
    }

    let step = PI / order as f64;
    for seed_index in candidate_indices.iter().copied() {
        let seed_direction = normalize(rotations[seed_index].direction, delta)?;
        let mut orbit = Vec::<usize>::with_capacity(order);
        for orbit_index in 0..order {
            let rotated =
                rotate_about_axis(seed_direction, principal_axis, orbit_index as f64 * step);
            let match_index = candidate_indices
                .iter()
                .copied()
                .filter(|candidate_index| !orbit.contains(candidate_index))
                .filter_map(|candidate_index| {
                    let candidate = normalize(rotations[candidate_index].direction, delta)?;
                    let alignment = dot(candidate, rotated).abs();
                    (alignment >= 1.0 - delta).then_some((candidate_index, alignment))
                })
                .max_by(
                    |(left_index, left_alignment), (right_index, right_alignment)| {
                        left_alignment.total_cmp(right_alignment).then_with(|| {
                            proper_rotation_ranked_key(&rotations[*left_index])
                                .cmp(&proper_rotation_ranked_key(&rotations[*right_index]))
                        })
                    },
                )
                .map(|(index, _)| index);
            let Some(match_index) = match_index else {
                orbit.clear();
                break;
            };
            orbit.push(match_index);
        }
        if orbit.len() == order {
            return Some(orbit);
        }
    }

    None
}

fn ranked_plane_indices(
    planes: &[crate::symmetry_elements::ReflectionPlaneRecord],
    principal_axis: [f64; 3],
    delta: f64,
) -> Vec<usize> {
    let mut indices = planes
        .iter()
        .enumerate()
        .filter_map(|(index, plane)| {
            normalize(plane.normal, delta)
                .filter(|normal| dot(*normal, principal_axis).abs() <= delta)
                .map(|_| index)
        })
        .collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        plane_ranked_key(&planes[*left]).cmp(&plane_ranked_key(&planes[*right]))
    });
    indices.reverse();
    indices
}

fn ranked_perpendicular_c2_indices(
    rotations: &[crate::symmetry_elements::ProperRotationRecord],
    principal_axis: [f64; 3],
    delta: f64,
) -> Vec<usize> {
    let mut indices = rotations
        .iter()
        .enumerate()
        .filter_map(|(index, rotation)| {
            (rotation.order == 2)
                .then_some(rotation)
                .and_then(|rotation| {
                    normalize(rotation.direction, delta).map(|direction| (index, direction))
                })
                .filter(|(_, direction)| dot(*direction, principal_axis).abs() <= delta)
                .map(|(index, _)| index)
        })
        .collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        proper_rotation_ranked_key(&rotations[*left])
            .cmp(&proper_rotation_ranked_key(&rotations[*right]))
    });
    indices.reverse();
    indices
}

fn plane_ranked_key(
    plane: &crate::symmetry_elements::ReflectionPlaneRecord,
) -> (usize, std::cmp::Reverse<i64>, [i64; 3]) {
    (
        plane.fixed_atom_count,
        std::cmp::Reverse((plane.max_deviation * 1.0e9).round() as i64),
        vector_sort_key(plane.normal),
    )
}

fn proper_rotation_ranked_key(
    rotation: &crate::symmetry_elements::ProperRotationRecord,
) -> (usize, std::cmp::Reverse<i64>, [i64; 3]) {
    (
        rotation.fixed_atom_count,
        std::cmp::Reverse((rotation.max_deviation * 1.0e9).round() as i64),
        vector_sort_key(rotation.direction),
    )
}

fn rotate_about_axis(vector: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let sine = angle.sin();
    let cosine = angle.cos();
    let cross_term = cross(axis, vector);
    let axis_projection = dot(axis, vector);
    [
        vector[0] * cosine + cross_term[0] * sine + axis[0] * axis_projection * (1.0 - cosine),
        vector[1] * cosine + cross_term[1] * sine + axis[1] * axis_projection * (1.0 - cosine),
        vector[2] * cosine + cross_term[2] * sine + axis[2] * axis_projection * (1.0 - cosine),
    ]
}

fn push_unique_operation(
    operations: &mut Vec<SelectedOperationKey>,
    kind: SymmetryElementKind,
    permutation: Vec<usize>,
) {
    if !contains_operation(operations, &kind, &permutation) {
        operations.push(SelectedOperationKey { kind, permutation });
    }
}

fn contains_operation(
    operations: &[SelectedOperationKey],
    kind: &SymmetryElementKind,
    permutation: &[usize],
) -> bool {
    operations
        .iter()
        .any(|candidate| &candidate.kind == kind && candidate.permutation == permutation)
}

fn reindex_operations(operations: &mut [SymmetryOperationSummary]) {
    for (index, operation) in operations.iter_mut().enumerate() {
        operation.index = index + 1;
    }
}

fn dedup_subgroups(subgroups: &mut Vec<BasicSubgroupSelection>) {
    let mut deduped = Vec::<BasicSubgroupSelection>::new();
    for subgroup in subgroups.drain(..) {
        if deduped
            .iter()
            .any(|existing| existing.label.as_str() == subgroup.label.as_str())
        {
            continue;
        }
        deduped.push(subgroup);
    }
    *subgroups = deduped;
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn normalize(vector: [f64; 3], delta: f64) -> Option<[f64; 3]> {
    let magnitude = norm(vector);
    if magnitude <= delta {
        None
    } else {
        Some([
            vector[0] / magnitude,
            vector[1] / magnitude,
            vector[2] / magnitude,
        ])
    }
}

fn vector_sort_key(vector: [f64; 3]) -> [i64; 3] {
    const SCALE: f64 = 1.0e6;
    [
        (vector[0] * SCALE).round() as i64,
        (vector[1] * SCALE).round() as i64,
        (vector[2] * SCALE).round() as i64,
    ]
}

#[cfg(test)]
mod tests {
    use super::enumerate_basic_subgroups;
    use crate::analyze_point_group;
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input};
    use crate::model::{ClusterAtom, ClusterGeometry, SyvaInputGeometry};
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};
    use crate::symmetry_elements::search_symmetry_elements;

    #[test]
    fn enumerates_basic_subgroups_for_propyne() {
        let paths = bundled_fixture_paths("propyne");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"C1"));
        assert!(labels.contains(&"Cs"));
        assert!(labels.contains(&"C3"));
        assert!(labels.contains(&"C3v"));
    }

    #[test]
    fn enumerates_basic_subgroups_for_h2o2() {
        let paths = bundled_fixture_paths("H2O2");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"C1"));
        assert!(labels.contains(&"C2"));
        assert!(!labels.contains(&"Ci"));
    }

    #[test]
    fn enumerates_linear_full_group_for_co2() {
        let paths = bundled_fixture_paths("CO2");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Dih"));
        assert!(labels.contains(&"Ci"));
    }

    #[test]
    fn enumerates_dihedral_subgroups_for_n4s4() {
        let paths = bundled_fixture_paths("N4S4");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"D2"));
        assert!(labels.contains(&"D2d"));
        assert!(labels.contains(&"S4"));
    }

    #[test]
    fn enumerates_d4h_family_for_square_planar_xef4() {
        let input = SyvaInputGeometry::from_cluster_geometry(&ClusterGeometry {
            label: "xef4".into(),
            atoms: vec![
                ClusterAtom {
                    species: "Xe".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "F".into(),
                    cartesian: [1.8, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "F".into(),
                    cartesian: [-1.8, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "F".into(),
                    cartesian: [0.0, 1.8, 0.0],
                },
                ClusterAtom {
                    species: "F".into(),
                    cartesian: [0.0, -1.8, 0.0],
                },
            ],
        })
        .expect("input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let classified = analyze_point_group(&geometry)
            .expect("classification")
            .expect("point group");
        assert_eq!(classified.label.as_str(), "D4h");

        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"C4"));
        assert!(labels.contains(&"C4h"));
        assert!(labels.contains(&"C4v"));
        assert!(labels.contains(&"D4"));
        assert!(labels.contains(&"D4h"));
    }

    #[test]
    fn enumerates_tetrahedral_special_case_subgroups_for_neopentane() {
        let paths = bundled_fixture_paths("neopentane");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"T"));
        assert!(labels.contains(&"Td"));
    }

    #[test]
    fn enumerates_octahedral_special_case_subgroups_for_cubane() {
        let paths = bundled_fixture_paths("cubane");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroups = enumerate_basic_subgroups(&geometry, &search).expect("subgroups");
        let labels = subgroups
            .iter()
            .map(|subgroup| subgroup.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"T"));
        assert!(labels.contains(&"Th"));
        assert!(labels.contains(&"Td"));
        assert!(labels.contains(&"O"));
        assert!(labels.contains(&"Oh"));
    }
}
