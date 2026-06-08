use crate::model::PointGroupLabel;
use crate::subgroups::BasicSubgroupSelection;
use crate::symmetry_elements::{SymmetryElementKind, SymmetrySearchResult};
use crate::SyvaError;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizedOperationGeometry {
    pub index: usize,
    pub label: String,
    pub kind: SymmetryElementKind,
    pub permutation: Vec<usize>,
    pub direction: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizedSubgroupSymmetry {
    pub point_group: PointGroupLabel,
    pub operations: Vec<OptimizedOperationGeometry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptimizationMode {
    None,
    Cubic,
    Icosahedral,
    PrincipalAxis {
        order: usize,
        axis_candidates: &'static [AxisCandidate],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AxisCandidate {
    kind: SymmetryElementKind,
    order: AxisOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisOrder {
    Group,
    Double,
    Half,
}

const C_AXIS: [AxisCandidate; 1] = [AxisCandidate {
    kind: SymmetryElementKind::ProperRotation,
    order: AxisOrder::Group,
}];

const C_H_AXIS: [AxisCandidate; 2] = [
    AxisCandidate {
        kind: SymmetryElementKind::ProperRotation,
        order: AxisOrder::Group,
    },
    AxisCandidate {
        kind: SymmetryElementKind::ImproperRotation,
        order: AxisOrder::Group,
    },
];

const D_AXIS: [AxisCandidate; 2] = [
    AxisCandidate {
        kind: SymmetryElementKind::ProperRotation,
        order: AxisOrder::Group,
    },
    AxisCandidate {
        kind: SymmetryElementKind::ImproperRotation,
        order: AxisOrder::Group,
    },
];

const D_D_AXIS: [AxisCandidate; 2] = [
    AxisCandidate {
        kind: SymmetryElementKind::ImproperRotation,
        order: AxisOrder::Double,
    },
    AxisCandidate {
        kind: SymmetryElementKind::ProperRotation,
        order: AxisOrder::Group,
    },
];

const S_AXIS: [AxisCandidate; 2] = [
    AxisCandidate {
        kind: SymmetryElementKind::ImproperRotation,
        order: AxisOrder::Group,
    },
    AxisCandidate {
        kind: SymmetryElementKind::ProperRotation,
        order: AxisOrder::Half,
    },
];

impl OptimizedSubgroupSymmetry {
    pub fn direction_for(
        &self,
        index: usize,
        kind: &SymmetryElementKind,
        permutation: &[usize],
    ) -> Option<[f64; 3]> {
        self.operations
            .iter()
            .find(|operation| {
                operation.index == index
                    && &operation.kind == kind
                    && operation.permutation == permutation
            })
            .or_else(|| {
                self.operations.iter().find(|operation| {
                    &operation.kind == kind && operation.permutation == permutation
                })
            })
            .map(|operation| operation.direction)
    }
}

pub fn optimize_subgroup_symmetry_elements(
    search: &SymmetrySearchResult,
    subgroup: &BasicSubgroupSelection,
    tolerance: f64,
) -> Result<OptimizedSubgroupSymmetry, SyvaError> {
    let mut operations = subgroup
        .operations
        .iter()
        .filter_map(|operation| {
            raw_direction_for_operation(search, operation.kind.clone(), &operation.permutation).map(
                |direction| OptimizedOperationGeometry {
                    index: operation.index,
                    label: operation.label.clone(),
                    kind: operation.kind.clone(),
                    permutation: operation.permutation.clone(),
                    direction,
                },
            )
        })
        .collect::<Vec<_>>();

    match optimization_mode(subgroup.label.as_str()) {
        OptimizationMode::Cubic => optimize_cubic_directions(&mut operations, tolerance),
        OptimizationMode::Icosahedral => {
            optimize_icosahedral_directions(&mut operations, tolerance)
        }
        OptimizationMode::PrincipalAxis {
            order,
            axis_candidates,
        } => optimize_principal_axis_directions(&mut operations, order, axis_candidates, tolerance),
        OptimizationMode::None => normalize_all(&mut operations, tolerance),
    }

    Ok(OptimizedSubgroupSymmetry {
        point_group: subgroup.label.clone(),
        operations,
    })
}

fn optimization_mode(label: &str) -> OptimizationMode {
    if is_cubic_group(label) {
        return OptimizationMode::Cubic;
    }
    if matches!(label, "I" | "Ih") {
        return OptimizationMode::Icosahedral;
    }

    if let Some(order) = parse_ordered_label(label, 'S') {
        if order > 1 {
            return OptimizationMode::PrincipalAxis {
                order,
                axis_candidates: &S_AXIS,
            };
        }
    }

    if let Some(order) = parse_ordered_label(label, 'D') {
        if order > 1 {
            return OptimizationMode::PrincipalAxis {
                order,
                axis_candidates: if label.ends_with('d') {
                    &D_D_AXIS
                } else {
                    &D_AXIS
                },
            };
        }
    }

    if let Some(order) = parse_ordered_label(label, 'C') {
        if order > 1 {
            return OptimizationMode::PrincipalAxis {
                order,
                axis_candidates: if label.ends_with('h') {
                    &C_H_AXIS
                } else {
                    &C_AXIS
                },
            };
        }
    }

    OptimizationMode::None
}

fn is_cubic_group(label: &str) -> bool {
    matches!(label, "T" | "Th" | "Td" | "O" | "Oh")
}

fn parse_ordered_label(label: &str, prefix: char) -> Option<usize> {
    let mut characters = label.chars();
    if characters.next()? != prefix {
        return None;
    }
    let digits = characters
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn optimize_principal_axis_directions(
    operations: &mut [OptimizedOperationGeometry],
    order: usize,
    axis_candidates: &[AxisCandidate],
    tolerance: f64,
) {
    let direction_tolerance = tolerance.max(5.0e-2);
    let Some(principal_axis) =
        select_principal_axis(operations, order, axis_candidates, direction_tolerance)
    else {
        normalize_all(operations, tolerance);
        return;
    };

    let perpendicular_orbit =
        build_perpendicular_orbit(operations, principal_axis, order, direction_tolerance);

    for operation in operations {
        let Some(direction) = normalize(operation.direction, direction_tolerance) else {
            continue;
        };
        let parallel_alignment = dot(direction, principal_axis).abs();
        if parallel_alignment >= 1.0 - direction_tolerance {
            operation.direction = orient_like(principal_axis, direction);
            continue;
        }

        if let Some(candidate_orbit) = &perpendicular_orbit {
            let perpendicular =
                perpendicular_component(direction, principal_axis, direction_tolerance)
                    .unwrap_or(direction);
            if let Some(snapped) = nearest_direction(
                perpendicular,
                candidate_orbit.as_slice(),
                direction_tolerance,
            ) {
                operation.direction = snapped;
                continue;
            }
        }

        operation.direction = direction;
    }
}

fn optimize_icosahedral_directions(operations: &mut [OptimizedOperationGeometry], tolerance: f64) {
    let direction_tolerance = tolerance.max(4.0e-2);
    let Some((primary_c5, secondary_c5)) = select_distinct_c5_axes(operations, direction_tolerance)
    else {
        normalize_all(operations, tolerance);
        return;
    };

    let Some(perpendicular_axis) = normalize(cross(primary_c5, secondary_c5), direction_tolerance)
    else {
        normalize_all(operations, tolerance);
        return;
    };

    let s =
        (54.0_f64.to_radians().tan() / 2.0) * (2.0 * (1.0 - (PI - (2.0_f64).atan()).cos())).sqrt();
    let ri = 0.5 * (5.0 / 2.0 + 11.0 * 5.0_f64.sqrt() / 10.0).sqrt();
    let alpha = (1.0 - s.powi(2) / (2.0 * ri.powi(2))).acos();

    let secondary_alignment_sign = if dot(primary_c5, secondary_c5) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let rotated_c5 = normalize(
        rotate_about_axis(
            primary_c5,
            perpendicular_axis,
            secondary_alignment_sign * alpha,
        ),
        direction_tolerance,
    )
    .unwrap_or(secondary_c5);

    let c5_axes = icosahedral_c5_axes(primary_c5, rotated_c5, direction_tolerance);
    let c3_axes = icosahedral_c3_axes(primary_c5, rotated_c5, direction_tolerance);
    let c2_axes = icosahedral_c2_axes(primary_c5, rotated_c5, direction_tolerance);

    for operation in operations {
        let candidates = match operation.kind {
            SymmetryElementKind::ProperRotation => {
                if operation.label.starts_with("C5") {
                    c5_axes.as_slice()
                } else if operation.label.starts_with("C3") {
                    c3_axes.as_slice()
                } else if operation.label.starts_with("C2") {
                    c2_axes.as_slice()
                } else {
                    &[]
                }
            }
            SymmetryElementKind::ImproperRotation => {
                if operation.label.starts_with("S10") {
                    c5_axes.as_slice()
                } else if operation.label.starts_with("S6") {
                    c3_axes.as_slice()
                } else {
                    &[]
                }
            }
            SymmetryElementKind::ReflectionPlane => c2_axes.as_slice(),
            _ => &[],
        };

        if let Some(direction) =
            nearest_direction(operation.direction, candidates, direction_tolerance)
        {
            operation.direction = direction;
        } else if let Some(direction) = normalize(operation.direction, tolerance) {
            operation.direction = direction;
        }
    }
}

fn select_distinct_c5_axes(
    operations: &[OptimizedOperationGeometry],
    tolerance: f64,
) -> Option<([f64; 3], [f64; 3])> {
    let c5_axes = operations
        .iter()
        .filter(|operation| {
            operation.kind == SymmetryElementKind::ProperRotation
                && operation.label.starts_with("C5")
        })
        .filter_map(|operation| normalize(operation.direction, tolerance))
        .fold(Vec::<[f64; 3]>::new(), |mut accumulator, direction| {
            if !accumulator
                .iter()
                .any(|existing| dot(*existing, direction).abs() >= 1.0 - tolerance)
            {
                accumulator.push(direction);
            }
            accumulator
        });

    for (index, primary) in c5_axes.iter().copied().enumerate() {
        for secondary in c5_axes.iter().copied().skip(index + 1) {
            if dot(primary, secondary).abs() < 1.0 - tolerance {
                return Some((primary, secondary));
            }
        }
    }

    None
}

fn icosahedral_c5_axes(
    primary_c5: [f64; 3],
    rotated_c5: [f64; 3],
    tolerance: f64,
) -> Vec<[f64; 3]> {
    let mut axes = vec![primary_c5];
    let angle = 2.0 * PI / 5.0;
    for index in 0..5 {
        push_unique_direction(
            &mut axes,
            rotate_about_axis(rotated_c5, primary_c5, index as f64 * angle),
            tolerance,
        );
    }
    axes
}

fn icosahedral_c3_axes(
    primary_c5: [f64; 3],
    rotated_c5: [f64; 3],
    tolerance: f64,
) -> Vec<[f64; 3]> {
    let angle = 2.0 * PI / 5.0;
    let mut axes = Vec::<[f64; 3]>::new();

    let rotated_once =
        normalize(rotate_about_axis(rotated_c5, primary_c5, angle), tolerance).expect("rotated c5");
    let seed_one =
        normalize(add(primary_c5, add(rotated_c5, rotated_once)), tolerance).expect("c3 seed one");
    for index in 0..5 {
        push_unique_direction(
            &mut axes,
            rotate_about_axis(seed_one, primary_c5, index as f64 * angle),
            tolerance,
        );
    }

    let rotated_twice = normalize(
        rotate_about_axis(rotated_c5, primary_c5, 2.0 * angle),
        tolerance,
    )
    .expect("rotated c5 twice");
    let rotated_thrice = normalize(
        rotate_about_axis(rotated_c5, primary_c5, 3.0 * angle),
        tolerance,
    )
    .expect("rotated c5 thrice");
    let seed_two = normalize(
        sub(sub(rotated_c5, rotated_twice), rotated_thrice),
        tolerance,
    )
    .expect("c3 seed two");
    for index in 0..5 {
        push_unique_direction(
            &mut axes,
            rotate_about_axis(seed_two, primary_c5, index as f64 * angle),
            tolerance,
        );
    }

    axes
}

fn icosahedral_c2_axes(
    primary_c5: [f64; 3],
    rotated_c5: [f64; 3],
    tolerance: f64,
) -> Vec<[f64; 3]> {
    let angle = 2.0 * PI / 5.0;
    let mut axes = Vec::<[f64; 3]>::new();

    let seed_one = normalize(add(primary_c5, rotated_c5), tolerance).expect("c2 seed one");
    for index in 0..5 {
        push_unique_direction(
            &mut axes,
            rotate_about_axis(seed_one, primary_c5, index as f64 * angle),
            tolerance,
        );
    }

    let rotated_once =
        normalize(rotate_about_axis(rotated_c5, primary_c5, angle), tolerance).expect("rotated c5");
    let seed_two = normalize(add(rotated_c5, rotated_once), tolerance).expect("c2 seed two");
    for index in 0..5 {
        push_unique_direction(
            &mut axes,
            rotate_about_axis(seed_two, primary_c5, index as f64 * angle),
            tolerance,
        );
    }

    let rotated_twice = normalize(
        rotate_about_axis(rotated_c5, primary_c5, 2.0 * angle),
        tolerance,
    )
    .expect("rotated c5 twice");
    let seed_three = normalize(sub(rotated_c5, rotated_twice), tolerance).expect("c2 seed three");
    for index in 0..5 {
        push_unique_direction(
            &mut axes,
            rotate_about_axis(seed_three, primary_c5, index as f64 * angle),
            tolerance,
        );
    }

    axes
}

fn push_unique_direction(accumulator: &mut Vec<[f64; 3]>, direction: [f64; 3], tolerance: f64) {
    let Some(direction) = normalize(direction, tolerance) else {
        return;
    };
    if accumulator
        .iter()
        .any(|existing| dot(*existing, direction).abs() >= 1.0 - tolerance)
    {
        return;
    }
    accumulator.push(direction);
}

fn select_principal_axis(
    operations: &[OptimizedOperationGeometry],
    order: usize,
    axis_candidates: &[AxisCandidate],
    tolerance: f64,
) -> Option<[f64; 3]> {
    for candidate in axis_candidates {
        let Some(resolved_order) = resolve_axis_order(order, candidate.order) else {
            continue;
        };
        if let Some(direction) = select_axis_by_kind_and_order(
            operations,
            candidate.kind.clone(),
            resolved_order,
            tolerance,
        ) {
            return Some(direction);
        }
    }
    None
}

fn resolve_axis_order(group_order: usize, axis_order: AxisOrder) -> Option<usize> {
    match axis_order {
        AxisOrder::Group => Some(group_order),
        AxisOrder::Double => Some(2 * group_order),
        AxisOrder::Half => group_order.is_multiple_of(2).then_some(group_order / 2),
    }
}

fn select_axis_by_kind_and_order(
    operations: &[OptimizedOperationGeometry],
    kind: SymmetryElementKind,
    order: usize,
    tolerance: f64,
) -> Option<[f64; 3]> {
    operations.iter().find_map(|operation| {
        (operation.kind == kind
            && rotation_label_order(operation.label.as_str(), &kind) == Some(order))
        .then(|| normalize(operation.direction, tolerance))
        .flatten()
    })
}

fn rotation_label_order(label: &str, kind: &SymmetryElementKind) -> Option<usize> {
    let prefix = match kind {
        SymmetryElementKind::ProperRotation => 'C',
        SymmetryElementKind::ImproperRotation => 'S',
        _ => return None,
    };
    parse_ordered_label(label, prefix)
}

fn build_perpendicular_orbit(
    operations: &[OptimizedOperationGeometry],
    principal_axis: [f64; 3],
    order: usize,
    tolerance: f64,
) -> Option<Vec<[f64; 3]>> {
    let seed = operations
        .iter()
        .filter_map(|operation| normalize(operation.direction, tolerance))
        .find_map(|direction| perpendicular_component(direction, principal_axis, tolerance))?;

    let step = PI / (2.0 * order as f64);
    let mut orbit = Vec::<[f64; 3]>::new();
    for index in 0..(2 * order) {
        let rotated = normalize(
            rotate_about_axis(seed, principal_axis, index as f64 * step),
            tolerance,
        )?;
        if orbit
            .iter()
            .any(|existing| dot(*existing, rotated).abs() >= 1.0 - tolerance)
        {
            continue;
        }
        orbit.push(rotated);
    }
    Some(orbit)
}

fn perpendicular_component(
    direction: [f64; 3],
    axis: [f64; 3],
    tolerance: f64,
) -> Option<[f64; 3]> {
    let perpendicular = sub(direction, scale(axis, dot(direction, axis)));
    normalize(perpendicular, tolerance)
}

fn orient_like(reference: [f64; 3], direction: [f64; 3]) -> [f64; 3] {
    if dot(reference, direction) < 0.0 {
        scale(reference, -1.0)
    } else {
        reference
    }
}

fn optimize_cubic_directions(operations: &mut [OptimizedOperationGeometry], tolerance: f64) {
    let Some(triad) = select_perpendicular_triad(operations, tolerance) else {
        normalize_all(operations, tolerance);
        return;
    };
    let c3_axes = cubic_c3_axes(&triad);
    let diagonal_axes = cubic_diagonal_axes(&triad, tolerance);

    for operation in operations {
        let candidates = match operation.kind {
            SymmetryElementKind::ProperRotation => {
                if operation.label.starts_with("C3") {
                    c3_axes.as_slice()
                } else if operation.label.starts_with("C4") || operation.label == "C2" {
                    if let Some(direction) =
                        nearest_direction(operation.direction, &triad, tolerance)
                    {
                        operation.direction = direction;
                        continue;
                    }
                    diagonal_axes.as_slice()
                } else {
                    diagonal_axes.as_slice()
                }
            }
            SymmetryElementKind::ImproperRotation => {
                if operation.label.starts_with("S6") {
                    c3_axes.as_slice()
                } else {
                    triad.as_slice()
                }
            }
            SymmetryElementKind::ReflectionPlane => {
                if operation.label == "Sigma_h" || operation.label == "Sigma" {
                    triad.as_slice()
                } else {
                    diagonal_axes.as_slice()
                }
            }
            _ => continue,
        };

        if let Some(direction) = nearest_direction(operation.direction, candidates, tolerance) {
            operation.direction = direction;
        } else if let Some(direction) = normalize(operation.direction, tolerance) {
            operation.direction = direction;
        }
    }
}

fn select_perpendicular_triad(
    operations: &[OptimizedOperationGeometry],
    tolerance: f64,
) -> Option<[[f64; 3]; 3]> {
    let candidates = operations
        .iter()
        .filter(|operation| {
            matches!(
                operation.kind,
                SymmetryElementKind::ProperRotation | SymmetryElementKind::ImproperRotation
            ) && (operation.label.starts_with("C4")
                || operation.label.starts_with("S4")
                || operation.label == "C2")
        })
        .filter_map(|operation| normalize(operation.direction, tolerance))
        .fold(Vec::<[f64; 3]>::new(), |mut accumulator, direction| {
            if !accumulator
                .iter()
                .any(|existing| dot(*existing, direction).abs() >= 1.0 - tolerance)
            {
                accumulator.push(direction);
            }
            accumulator
        });

    for i in 0..candidates.len() {
        for j in i + 1..candidates.len() {
            if dot(candidates[i], candidates[j]).abs() > tolerance {
                continue;
            }
            for k in j + 1..candidates.len() {
                if dot(candidates[i], candidates[k]).abs() > tolerance
                    || dot(candidates[j], candidates[k]).abs() > tolerance
                {
                    continue;
                }
                return Some([candidates[i], candidates[j], candidates[k]]);
            }
        }
    }

    None
}

fn cubic_c3_axes(triad: &[[f64; 3]; 3]) -> Vec<[f64; 3]> {
    let mut axes = Vec::<[f64; 3]>::new();
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            let direction = normalize(
                add(scale(triad[0], sx), add(scale(triad[1], sy), triad[2])),
                1.0e-12,
            )
            .expect("c3 axis");
            if !axes
                .iter()
                .any(|existing| dot(*existing, direction).abs() >= 1.0 - 1.0e-12)
            {
                axes.push(direction);
            }
        }
    }
    axes
}

fn cubic_diagonal_axes(triad: &[[f64; 3]; 3], tolerance: f64) -> Vec<[f64; 3]> {
    let mut axes = Vec::<[f64; 3]>::new();
    let candidates = [
        add(triad[0], triad[1]),
        sub(triad[0], triad[1]),
        add(triad[0], triad[2]),
        sub(triad[0], triad[2]),
        add(triad[1], triad[2]),
        sub(triad[1], triad[2]),
    ];
    for candidate in candidates {
        let Some(direction) = normalize(candidate, tolerance) else {
            continue;
        };
        if axes
            .iter()
            .any(|existing| dot(*existing, direction).abs() >= 1.0 - tolerance)
        {
            continue;
        }
        axes.push(direction);
    }
    axes
}

fn nearest_direction(
    current: [f64; 3],
    candidates: &[[f64; 3]],
    tolerance: f64,
) -> Option<[f64; 3]> {
    let current = normalize(current, tolerance)?;
    candidates
        .iter()
        .map(|candidate| {
            let alignment = dot(current, *candidate);
            let direction = if alignment < 0.0 {
                scale(*candidate, -1.0)
            } else {
                *candidate
            };
            (alignment.abs(), direction)
        })
        .max_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, direction)| direction)
}

fn normalize_all(operations: &mut [OptimizedOperationGeometry], tolerance: f64) {
    for operation in operations {
        if let Some(direction) = normalize(operation.direction, tolerance) {
            operation.direction = direction;
        }
    }
}

fn raw_direction_for_operation(
    search: &SymmetrySearchResult,
    kind: SymmetryElementKind,
    permutation: &[usize],
) -> Option<[f64; 3]> {
    match kind {
        SymmetryElementKind::ReflectionPlane => search
            .reflection_planes
            .iter()
            .find(|plane| plane.permutation == permutation)
            .map(|plane| plane.normal),
        SymmetryElementKind::ProperRotation => search
            .proper_rotations
            .iter()
            .find(|rotation| rotation.permutation == permutation)
            .map(|rotation| rotation.direction),
        SymmetryElementKind::ImproperRotation => search
            .improper_rotations
            .iter()
            .find(|rotation| rotation.permutation == permutation)
            .map(|rotation| rotation.direction),
        _ => None,
    }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
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

fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn normalize(vector: [f64; 3], tolerance: f64) -> Option<[f64; 3]> {
    let magnitude = norm(vector);
    if magnitude <= tolerance {
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
    use super::{dot, optimize_subgroup_symmetry_elements};
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input};
    use crate::model::{ClusterAtom, ClusterGeometry, SyvaInputGeometry};
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};
    use crate::subgroups::enumerate_basic_subgroups;
    use crate::symmetry_elements::search_symmetry_elements;

    #[test]
    fn optimizes_cubic_subgroup_directions_for_cubane() {
        let paths = bundled_fixture_paths("cubane");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "Td")
            .expect("td subgroup");

        let optimized =
            optimize_subgroup_symmetry_elements(&search, &subgroup, geometry.settings.tolerance)
                .expect("optimize");

        assert!(optimized.operations.iter().any(|operation| {
            operation.label.starts_with("C3")
                && (operation.direction[0].abs() - operation.direction[1].abs()).abs() < 1.0e-8
                && (operation.direction[1].abs() - operation.direction[2].abs()).abs() < 1.0e-8
        }));
        assert!(optimized.operations.iter().any(|operation| {
            operation.label.starts_with("S4")
                && operation
                    .direction
                    .iter()
                    .filter(|value| value.abs() > 1.0e-8)
                    .count()
                    == 1
        }));
    }

    #[test]
    fn optimizes_principal_axis_family_for_propyne_c3v() {
        let paths = bundled_fixture_paths("propyne");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "C3v")
            .expect("c3v subgroup");

        let optimized =
            optimize_subgroup_symmetry_elements(&search, &subgroup, geometry.settings.tolerance)
                .expect("optimize");

        let principal_axis = optimized
            .operations
            .iter()
            .find(|operation| {
                operation.kind == crate::symmetry_elements::SymmetryElementKind::ProperRotation
                    && operation.label == "C3"
            })
            .map(|operation| operation.direction)
            .expect("principal axis");
        let plane_normals = optimized
            .operations
            .iter()
            .filter(|operation| {
                operation.kind == crate::symmetry_elements::SymmetryElementKind::ReflectionPlane
            })
            .map(|operation| operation.direction)
            .collect::<Vec<_>>();

        assert!(optimized.operations.iter().all(|operation| {
            operation.kind != crate::symmetry_elements::SymmetryElementKind::ProperRotation
                || !operation.label.starts_with("C3")
                || dot(operation.direction, principal_axis).abs() >= 1.0 - 1.0e-10
        }));
        assert_eq!(plane_normals.len(), 3);
        assert!(plane_normals
            .iter()
            .all(|normal| dot(*normal, principal_axis).abs() <= 1.0e-10));
        let first = plane_normals[0];
        assert!(plane_normals.iter().skip(1).any(|candidate| {
            let alignment = dot(first, *candidate).abs();
            (alignment - 0.5).abs() < 1.0e-8
        }));
    }

    #[test]
    fn optimizes_principal_axis_family_for_n4s4_d2d() {
        let paths = bundled_fixture_paths("N4S4");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "D2d")
            .expect("d2d subgroup");

        let optimized =
            optimize_subgroup_symmetry_elements(&search, &subgroup, geometry.settings.tolerance)
                .expect("optimize");

        let principal_axis = optimized
            .operations
            .iter()
            .find(|operation| {
                operation.kind == crate::symmetry_elements::SymmetryElementKind::ImproperRotation
                    && operation.label == "S4"
            })
            .map(|operation| operation.direction)
            .expect("s4 axis");

        let c2_alignments = optimized
            .operations
            .iter()
            .filter(|operation| {
                operation.kind == crate::symmetry_elements::SymmetryElementKind::ProperRotation
                    && operation.label.starts_with("C2")
            })
            .map(|operation| dot(operation.direction, principal_axis).abs())
            .collect::<Vec<_>>();

        assert!(c2_alignments
            .iter()
            .any(|alignment| *alignment >= 1.0 - 1.0e-10));
        assert!(c2_alignments.iter().any(|alignment| *alignment <= 1.0e-10));
        assert!(optimized
            .operations
            .iter()
            .filter(|operation| operation.kind
                == crate::symmetry_elements::SymmetryElementKind::ReflectionPlane)
            .all(|operation| dot(operation.direction, principal_axis).abs() <= 1.0e-10));
    }

    #[test]
    fn optimizes_principal_axis_family_for_square_planar_xef4_d4h() {
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
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "D4h")
            .expect("d4h subgroup");

        let optimized =
            optimize_subgroup_symmetry_elements(&search, &subgroup, geometry.settings.tolerance)
                .expect("optimize");

        let principal_axis = optimized
            .operations
            .iter()
            .find(|operation| {
                operation.kind == crate::symmetry_elements::SymmetryElementKind::ProperRotation
                    && operation.label == "C4"
            })
            .map(|operation| operation.direction)
            .expect("c4 axis");
        let perpendicular_planes = optimized
            .operations
            .iter()
            .filter(|operation| operation.label == "Sigma_")
            .map(|operation| operation.direction)
            .collect::<Vec<_>>();

        assert!(optimized.operations.iter().all(|operation| {
            !(operation.label == "Sigma_h"
                || operation.label.starts_with("C4")
                || operation.label.starts_with("S4"))
                || dot(operation.direction, principal_axis).abs() >= 1.0 - 1.0e-10
        }));
        assert_eq!(perpendicular_planes.len(), 4);
        assert!(perpendicular_planes
            .iter()
            .all(|normal| dot(*normal, principal_axis).abs() <= 1.0e-10));
        assert!(perpendicular_planes
            .iter()
            .enumerate()
            .any(|(index, left)| {
                perpendicular_planes.iter().skip(index + 1).any(|right| {
                    (dot(*left, *right).abs() - std::f64::consts::FRAC_1_SQRT_2).abs() < 1.0e-8
                })
            }));
    }

    #[test]
    fn optimizes_icosahedral_family_for_c60_ih() {
        let paths = bundled_fixture_paths("C60");
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let subgroup = enumerate_basic_subgroups(&geometry, &search)
            .expect("subgroups")
            .into_iter()
            .find(|subgroup| subgroup.label.as_str() == "Ih")
            .expect("ih subgroup");

        let optimized =
            optimize_subgroup_symmetry_elements(&search, &subgroup, geometry.settings.tolerance)
                .expect("optimize");

        let c5_axes = unique_family_axes(&optimized, |operation| {
            operation.kind == crate::symmetry_elements::SymmetryElementKind::ProperRotation
                && operation.label.starts_with("C5")
        });
        let c3_axes = unique_family_axes(&optimized, |operation| {
            operation.kind == crate::symmetry_elements::SymmetryElementKind::ProperRotation
                && operation.label.starts_with("C3")
        });
        let c2_axes = unique_family_axes(&optimized, |operation| {
            operation.kind == crate::symmetry_elements::SymmetryElementKind::ProperRotation
                && operation.label.starts_with("C2")
        });

        assert_eq!(c5_axes.len(), 6);
        assert_eq!(c3_axes.len(), 10);
        assert_eq!(c2_axes.len(), 15);
        assert!(optimized.operations.iter().all(|operation| {
            if operation.kind == crate::symmetry_elements::SymmetryElementKind::ImproperRotation
                && operation.label.starts_with("S10")
            {
                c5_axes
                    .iter()
                    .any(|axis| dot(*axis, operation.direction).abs() >= 1.0 - 1.0e-8)
            } else if operation.kind
                == crate::symmetry_elements::SymmetryElementKind::ImproperRotation
                && operation.label.starts_with("S6")
            {
                c3_axes
                    .iter()
                    .any(|axis| dot(*axis, operation.direction).abs() >= 1.0 - 1.0e-8)
            } else if operation.kind
                == crate::symmetry_elements::SymmetryElementKind::ReflectionPlane
            {
                c2_axes
                    .iter()
                    .any(|axis| dot(*axis, operation.direction).abs() >= 1.0 - 1.0e-8)
            } else {
                true
            }
        }));
    }

    fn unique_family_axes(
        optimized: &crate::optimize::OptimizedSubgroupSymmetry,
        predicate: impl Fn(&crate::optimize::OptimizedOperationGeometry) -> bool,
    ) -> Vec<[f64; 3]> {
        optimized
            .operations
            .iter()
            .filter(|operation| predicate(operation))
            .fold(Vec::<[f64; 3]>::new(), |mut accumulator, operation| {
                if !accumulator
                    .iter()
                    .any(|existing| dot(*existing, operation.direction).abs() >= 1.0 - 1.0e-8)
                {
                    accumulator.push(operation.direction);
                }
                accumulator
            })
    }
}
