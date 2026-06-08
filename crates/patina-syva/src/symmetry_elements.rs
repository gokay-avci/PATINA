use crate::preprocess::SyvaPreprocessedGeometry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymmetryElementKind {
    Identity,
    InversionCenter,
    ReflectionPlane,
    ProperRotationAxis,
    ProperRotation,
    ImproperRotation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermutationRecord {
    pub mapping: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionPlaneRecord {
    pub normal: [f64; 3],
    pub fixed_atom_count: usize,
    pub permutation: Vec<usize>,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InversionCenterRecord {
    pub permutation: Vec<usize>,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProperRotationAxisRecord {
    pub direction: [f64; 3],
    pub order: usize,
    pub angle_degrees: f64,
    pub fixed_atom_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProperRotationRecord {
    pub direction: [f64; 3],
    pub order: usize,
    pub power: usize,
    pub fixed_atom_count: usize,
    pub permutation: Vec<usize>,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImproperRotationRecord {
    pub direction: [f64; 3],
    pub order: usize,
    pub power: usize,
    pub fixed_atom_count: usize,
    pub permutation: Vec<usize>,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetrySearchResult {
    pub center_atom_index: Option<usize>,
    pub is_linear: bool,
    pub is_planar: bool,
    pub inversion_center: Option<InversionCenterRecord>,
    pub reflection_planes: Vec<ReflectionPlaneRecord>,
    pub proper_rotation_axes: Vec<ProperRotationAxisRecord>,
    pub proper_rotations: Vec<ProperRotationRecord>,
    pub improper_rotations: Vec<ImproperRotationRecord>,
    pub permutations: Vec<PermutationRecord>,
    pub max_deviation: f64,
}

pub fn search_symmetry_elements(geometry: &SyvaPreprocessedGeometry) -> SymmetrySearchResult {
    let delta = geometry.settings.tolerance;
    let coordinates = geometry
        .active_atoms
        .iter()
        .map(|atom| atom.shifted_cartesian)
        .collect::<Vec<_>>();
    let atomic_numbers = geometry
        .active_atoms
        .iter()
        .map(|atom| atom.atomic_number)
        .collect::<Vec<_>>();

    let mut result = SymmetrySearchResult {
        center_atom_index: None,
        is_linear: false,
        is_planar: false,
        inversion_center: None,
        reflection_planes: Vec::new(),
        proper_rotation_axes: Vec::new(),
        proper_rotations: Vec::new(),
        improper_rotations: Vec::new(),
        permutations: vec![PermutationRecord {
            mapping: (1..=coordinates.len()).collect(),
        }],
        max_deviation: 0.0,
    };

    if let Some(operation) = check_operation(
        &atomic_numbers,
        &coordinates,
        invert_coordinates(&coordinates),
        delta,
    ) {
        result.max_deviation = result.max_deviation.max(operation.max_deviation);
        push_permutation(&mut result.permutations, operation.permutation.clone());
        result.inversion_center = Some(InversionCenterRecord {
            permutation: operation.permutation,
            max_deviation: operation.max_deviation,
        });
    }

    for (index, coordinate) in coordinates.iter().enumerate() {
        let distance = norm(*coordinate);
        if distance <= delta {
            result.center_atom_index = Some(index + 1);
            result.max_deviation = result.max_deviation.max(distance);
            break;
        }
    }

    let non_collinear_normal = find_non_collinear_normal(
        &coordinates,
        result.center_atom_index,
        delta,
        &mut result.max_deviation,
    );
    result.is_linear = non_collinear_normal.is_none();

    if let Some(planar_normal) = non_collinear_normal {
        let plane_deviation = plane_max_deviation(planar_normal, &coordinates);
        if plane_deviation <= delta {
            result.is_planar = true;
            let identity = (1..=coordinates.len()).collect::<Vec<_>>();
            add_reflection_plane(
                &mut result.reflection_planes,
                planar_normal,
                plane_fixed_atom_count(planar_normal, &coordinates, delta),
                identity,
                plane_deviation,
                delta,
            );
            result.max_deviation = result.max_deviation.max(plane_deviation);
        }
    }

    for class in &geometry.atom_classes {
        if class.atom_indices.len() < 2 {
            continue;
        }

        for left in 0..class.atom_indices.len() - 1 {
            let left_index = class.atom_indices[left] - 1;
            let p1 = coordinates[left_index];
            for right in left + 1..class.atom_indices.len() {
                let right_index = class.atom_indices[right] - 1;
                let p2 = coordinates[right_index];
                let midpoint = scale(add(p1, p2), 0.5);

                for normal in candidate_plane_normals(p1, p2, delta) {
                    if dot(normal, negate(midpoint)).abs() >= delta {
                        continue;
                    }
                    let transformed = reflect_coordinates(&coordinates, normal, midpoint);
                    let Some(operation) =
                        check_operation(&atomic_numbers, &coordinates, transformed, delta)
                    else {
                        continue;
                    };

                    result.max_deviation = result.max_deviation.max(operation.max_deviation);
                    push_permutation(&mut result.permutations, operation.permutation.clone());
                    add_reflection_plane(
                        &mut result.reflection_planes,
                        normal,
                        plane_fixed_atom_count(normal, &coordinates, delta),
                        operation.permutation,
                        operation.max_deviation,
                        delta,
                    );
                }
            }
        }
    }

    if !result.is_linear {
        search_proper_rotations(geometry, &coordinates, &atomic_numbers, delta, &mut result);
        search_improper_rotations(&coordinates, &atomic_numbers, delta, &mut result);
    }

    result
}

#[derive(Debug, Clone)]
struct OperationMatch {
    permutation: Vec<usize>,
    max_deviation: f64,
}

fn check_operation(
    atomic_numbers: &[u8],
    original: &[[f64; 3]],
    transformed: Vec<[f64; 3]>,
    delta: f64,
) -> Option<OperationMatch> {
    let mut permutation = Vec::with_capacity(original.len());
    let mut max_deviation: f64 = 0.0;
    let mut used_targets = vec![false; transformed.len()];

    'outer: for (source_index, original_coord) in original.iter().enumerate() {
        for (target_index, transformed_coord) in transformed.iter().enumerate() {
            if used_targets[target_index] {
                continue;
            }
            if atomic_numbers[source_index] != atomic_numbers[target_index] {
                continue;
            }

            let separation = norm(sub(*original_coord, *transformed_coord));
            if separation <= delta {
                permutation.push(target_index + 1);
                used_targets[target_index] = true;
                max_deviation = max_deviation.max(separation);
                continue 'outer;
            }
        }
        return None;
    }

    Some(OperationMatch {
        permutation,
        max_deviation,
    })
}

fn invert_coordinates(coordinates: &[[f64; 3]]) -> Vec<[f64; 3]> {
    coordinates
        .iter()
        .map(|coordinate| negate(*coordinate))
        .collect()
}

fn reflect_coordinates(
    coordinates: &[[f64; 3]],
    normal: [f64; 3],
    point_on_plane: [f64; 3],
) -> Vec<[f64; 3]> {
    coordinates
        .iter()
        .map(|coordinate| {
            let offset = sub(*coordinate, point_on_plane);
            let scale_factor = -dot(normal, offset);
            add(*coordinate, scale(normal, 2.0 * scale_factor))
        })
        .collect()
}

fn rotate_coordinates(
    coordinates: &[[f64; 3]],
    axis: [f64; 3],
    sine: f64,
    cosine: f64,
) -> Vec<[f64; 3]> {
    coordinates
        .iter()
        .map(|coordinate| {
            let cross_term = cross(axis, *coordinate);
            let cross_cross_term = cross(axis, cross_term);
            add(
                *coordinate,
                add(
                    scale(cross_term, sine),
                    scale(cross_cross_term, 1.0 - cosine),
                ),
            )
        })
        .collect()
}

fn improper_rotate_coordinates(
    coordinates: &[[f64; 3]],
    axis: [f64; 3],
    sine: f64,
    cosine: f64,
) -> Vec<[f64; 3]> {
    rotate_coordinates(coordinates, axis, sine, cosine)
        .into_iter()
        .map(|coordinate| {
            let scale_factor = -dot(axis, coordinate);
            add(coordinate, scale(axis, 2.0 * scale_factor))
        })
        .collect()
}

fn candidate_plane_normals(p1: [f64; 3], p2: [f64; 3], delta: f64) -> Vec<[f64; 3]> {
    let midpoint = scale(add(p1, p2), 0.5);
    let mut candidates = Vec::new();

    let pair_axis = sub(p2, midpoint);
    if let Some(normal) = normalize(pair_axis, delta) {
        candidates.push(normal);
    }

    if let Some(normal) = normalize(cross(p1, p2), delta) {
        candidates.push(normal);
    }

    if candidates.len() >= 2 {
        if let Some(normal) = normalize(cross(candidates[0], candidates[1]), delta) {
            let _ = midpoint;
            candidates.push(normal);
        }
    }

    candidates
}

fn find_non_collinear_normal(
    coordinates: &[[f64; 3]],
    center_atom_index: Option<usize>,
    delta: f64,
    max_deviation: &mut f64,
) -> Option<[f64; 3]> {
    for left in 0..coordinates.len().saturating_sub(1) {
        if center_atom_index == Some(left + 1) {
            continue;
        }
        for right in left + 1..coordinates.len() {
            if center_atom_index == Some(right + 1) {
                continue;
            }

            let candidate = cross(coordinates[left], coordinates[right]);
            let magnitude = norm(candidate);
            if magnitude > delta {
                return Some(scale(candidate, 1.0 / magnitude));
            }
            *max_deviation = (*max_deviation).max(magnitude);
        }
    }

    None
}

fn search_proper_rotations(
    geometry: &SyvaPreprocessedGeometry,
    coordinates: &[[f64; 3]],
    atomic_numbers: &[u8],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    let reflection_planes = result.reflection_planes.clone();
    collect_axes_from_planes(coordinates, &reflection_planes, delta, result);
    collect_axes_from_atoms(geometry, coordinates, atomic_numbers, delta, result);
    collect_c2_axes_from_pairs(geometry, coordinates, atomic_numbers, delta, result);
    collect_higher_order_axes_from_triples(geometry, coordinates, atomic_numbers, delta, result);
    collect_proper_rotations(coordinates, atomic_numbers, delta, result);
}

fn collect_axes_from_planes(
    coordinates: &[[f64; 3]],
    planes: &[ReflectionPlaneRecord],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    for left in 0..planes.len().saturating_sub(1) {
        for right in left + 1..planes.len() {
            let axis = cross(planes[left].normal, planes[right].normal);
            let Some(axis) = normalize(axis, delta) else {
                continue;
            };
            let mut angle = dot(planes[left].normal, planes[right].normal)
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            if angle > 90.0 {
                angle = 180.0 - angle;
            }
            angle *= 2.0;
            if angle <= 0.0 {
                continue;
            }

            add_rotation_axis(
                result,
                axis,
                angle,
                count_axis_atoms(axis, coordinates, delta),
                delta,
            );
        }
    }
}

fn collect_axes_from_atoms(
    geometry: &SyvaPreprocessedGeometry,
    coordinates: &[[f64; 3]],
    atomic_numbers: &[u8],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    for class in &geometry.atom_classes {
        for &atom_index in &class.atom_indices {
            if result.center_atom_index == Some(atom_index) {
                continue;
            }
            let coordinate = coordinates[atom_index - 1];
            let Some(axis) = normalize(coordinate, delta) else {
                continue;
            };
            for order in (2..=12).rev() {
                let angle = std::f64::consts::TAU / order as f64;
                let transformed = rotate_coordinates(coordinates, axis, angle.sin(), angle.cos());
                let Some(operation) =
                    check_operation(atomic_numbers, coordinates, transformed, delta)
                else {
                    continue;
                };
                result.max_deviation = result.max_deviation.max(operation.max_deviation);
                push_permutation(&mut result.permutations, operation.permutation.clone());
                add_rotation_axis(
                    result,
                    axis,
                    angle.to_degrees(),
                    count_axis_atoms(axis, coordinates, delta),
                    delta,
                );
                break;
            }
        }
    }
}

fn collect_c2_axes_from_pairs(
    geometry: &SyvaPreprocessedGeometry,
    coordinates: &[[f64; 3]],
    atomic_numbers: &[u8],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    for class in &geometry.atom_classes {
        if class.atom_indices.len() < 2 {
            continue;
        }

        for left in 0..class.atom_indices.len() - 1 {
            let p1 = coordinates[class.atom_indices[left] - 1];
            for right in left + 1..class.atom_indices.len() {
                let p2 = coordinates[class.atom_indices[right] - 1];
                let midpoint = scale(add(p1, p2), 0.5);
                let Some(axis) = normalize(midpoint, delta) else {
                    continue;
                };
                let angle = std::f64::consts::PI;
                let transformed = rotate_coordinates(coordinates, axis, angle.sin(), angle.cos());
                let Some(operation) =
                    check_operation(atomic_numbers, coordinates, transformed, delta)
                else {
                    continue;
                };
                result.max_deviation = result.max_deviation.max(operation.max_deviation);
                push_permutation(&mut result.permutations, operation.permutation.clone());
                add_rotation_axis(
                    result,
                    axis,
                    180.0,
                    count_axis_atoms(axis, coordinates, delta),
                    delta,
                );
            }
        }
    }
}

fn collect_higher_order_axes_from_triples(
    geometry: &SyvaPreprocessedGeometry,
    coordinates: &[[f64; 3]],
    atomic_numbers: &[u8],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    for class in &geometry.atom_classes {
        let class_len = class.atom_indices.len();
        if class_len < 3 {
            continue;
        }

        for first in 0..class_len - 2 {
            let a = coordinates[class.atom_indices[first] - 1];
            for second in first + 1..class_len - 1 {
                let b = coordinates[class.atom_indices[second] - 1];
                let midpoint_ab = scale(add(a, b), 0.5);
                let Some(v1) = normalize(sub(b, midpoint_ab), delta) else {
                    continue;
                };
                for third in second + 1..class_len {
                    let c = coordinates[class.atom_indices[third] - 1];
                    let midpoint_bc = scale(add(b, c), 0.5);
                    let Some(v2) = normalize(sub(c, midpoint_bc), delta) else {
                        continue;
                    };
                    let Some(axis) = normalize(cross(v1, v2), delta) else {
                        continue;
                    };

                    let angle = dot(v1, v2).clamp(-1.0, 1.0).acos();
                    if angle.abs() < delta {
                        continue;
                    }
                    let order = (std::f64::consts::TAU / angle + delta).floor() as usize;
                    if order < 3 || order > class_len {
                        continue;
                    }
                    if (order as f64 * angle) < (std::f64::consts::TAU - delta) {
                        continue;
                    }

                    let transformed =
                        rotate_coordinates(coordinates, axis, angle.sin(), angle.cos());
                    let Some(operation) =
                        check_operation(atomic_numbers, coordinates, transformed, delta)
                    else {
                        continue;
                    };
                    result.max_deviation = result.max_deviation.max(operation.max_deviation);
                    push_permutation(&mut result.permutations, operation.permutation.clone());
                    add_rotation_axis(
                        result,
                        axis,
                        angle.to_degrees(),
                        count_axis_atoms(axis, coordinates, delta),
                        delta,
                    );
                }
            }
        }
    }
}

fn collect_proper_rotations(
    coordinates: &[[f64; 3]],
    atomic_numbers: &[u8],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    let axes = result.proper_rotation_axes.clone();
    for axis in axes {
        for order in (2..=12).rev() {
            let angle = std::f64::consts::TAU / order as f64;
            let transformed =
                rotate_coordinates(coordinates, axis.direction, angle.sin(), angle.cos());
            let Some(operation) = check_operation(atomic_numbers, coordinates, transformed, delta)
            else {
                continue;
            };

            result.max_deviation = result.max_deviation.max(operation.max_deviation);
            push_permutation(&mut result.permutations, operation.permutation.clone());
            push_proper_rotation(
                &mut result.proper_rotations,
                ProperRotationRecord {
                    direction: axis.direction,
                    order,
                    power: 1,
                    fixed_atom_count: axis.fixed_atom_count,
                    permutation: operation.permutation,
                    max_deviation: operation.max_deviation,
                },
                delta,
            );

            if order > 2 {
                for power in 2..order {
                    let phase = power as f64 * angle;
                    let transformed =
                        rotate_coordinates(coordinates, axis.direction, phase.sin(), phase.cos());
                    let Some(operation) =
                        check_operation(atomic_numbers, coordinates, transformed, delta)
                    else {
                        continue;
                    };
                    result.max_deviation = result.max_deviation.max(operation.max_deviation);
                    push_permutation(&mut result.permutations, operation.permutation.clone());
                    let gcd = gcd(order, power);
                    push_proper_rotation(
                        &mut result.proper_rotations,
                        ProperRotationRecord {
                            direction: axis.direction,
                            order: order / gcd,
                            power: power / gcd,
                            fixed_atom_count: axis.fixed_atom_count,
                            permutation: operation.permutation,
                            max_deviation: operation.max_deviation,
                        },
                        delta,
                    );
                }
            }
        }
    }
}

fn search_improper_rotations(
    coordinates: &[[f64; 3]],
    atomic_numbers: &[u8],
    delta: f64,
    result: &mut SymmetrySearchResult,
) {
    let axes = result.proper_rotation_axes.clone();
    let fixed_atom_count = usize::from(result.center_atom_index.is_some());

    for axis in axes {
        for order in (3..=24).rev() {
            let angle = std::f64::consts::TAU / order as f64;
            let transformed =
                improper_rotate_coordinates(coordinates, axis.direction, angle.sin(), angle.cos());
            let Some(operation) = check_operation(atomic_numbers, coordinates, transformed, delta)
            else {
                continue;
            };

            result.max_deviation = result.max_deviation.max(operation.max_deviation);
            push_permutation(&mut result.permutations, operation.permutation.clone());
            push_improper_rotation(
                &mut result.improper_rotations,
                ImproperRotationRecord {
                    direction: axis.direction,
                    order,
                    power: 1,
                    fixed_atom_count,
                    permutation: operation.permutation,
                    max_deviation: operation.max_deviation,
                },
                delta,
            );

            let max_power = if order % 2 == 0 {
                order - 1
            } else {
                2 * order - 1
            };
            for power in 2..=max_power {
                if power % 2 == 0 || power == order || gcd(order, power) != 1 {
                    continue;
                }

                let phase = power as f64 * angle;
                let transformed = improper_rotate_coordinates(
                    coordinates,
                    axis.direction,
                    phase.sin(),
                    phase.cos(),
                );
                let Some(operation) =
                    check_operation(atomic_numbers, coordinates, transformed, delta)
                else {
                    continue;
                };

                result.max_deviation = result.max_deviation.max(operation.max_deviation);
                push_permutation(&mut result.permutations, operation.permutation.clone());
                push_improper_rotation(
                    &mut result.improper_rotations,
                    ImproperRotationRecord {
                        direction: axis.direction,
                        order,
                        power,
                        fixed_atom_count,
                        permutation: operation.permutation,
                        max_deviation: operation.max_deviation,
                    },
                    delta,
                );
            }
        }
    }
}

fn add_rotation_axis(
    result: &mut SymmetrySearchResult,
    direction: [f64; 3],
    angle_degrees: f64,
    fixed_atom_count: usize,
    delta: f64,
) {
    let order = order_from_angle(angle_degrees);
    let Some(order) = order else {
        return;
    };

    for existing in &mut result.proper_rotation_axes {
        let overlap = dot(existing.direction, direction).abs();
        if overlap > (1.0 - delta) && overlap < (1.0 + delta) {
            if angle_degrees + delta < existing.angle_degrees {
                existing.direction = direction;
                existing.angle_degrees = angle_degrees;
                existing.order = order;
                existing.fixed_atom_count = fixed_atom_count;
            } else if fixed_atom_count > existing.fixed_atom_count {
                existing.fixed_atom_count = fixed_atom_count;
            }
            return;
        }
    }

    result.proper_rotation_axes.push(ProperRotationAxisRecord {
        direction,
        order,
        angle_degrees,
        fixed_atom_count,
    });
}

fn push_proper_rotation(
    rotations: &mut Vec<ProperRotationRecord>,
    candidate: ProperRotationRecord,
    delta: f64,
) {
    for existing in rotations.iter_mut() {
        let same_axis = dot(existing.direction, candidate.direction).abs() > (1.0 - delta);
        if same_axis && existing.order == candidate.order && existing.power == candidate.power {
            if candidate.max_deviation < existing.max_deviation {
                *existing = candidate;
            }
            return;
        }
    }
    rotations.push(candidate);
}

fn push_improper_rotation(
    rotations: &mut Vec<ImproperRotationRecord>,
    candidate: ImproperRotationRecord,
    delta: f64,
) {
    for existing in rotations.iter_mut() {
        let same_axis = dot(existing.direction, candidate.direction).abs() > (1.0 - delta);
        if same_axis && existing.order == candidate.order && existing.power == candidate.power {
            if candidate.max_deviation < existing.max_deviation {
                *existing = candidate;
            }
            return;
        }
    }
    rotations.push(candidate);
}

fn count_axis_atoms(axis: [f64; 3], coordinates: &[[f64; 3]], delta: f64) -> usize {
    coordinates
        .iter()
        .filter(|coordinate| norm(cross(axis, **coordinate)) <= delta)
        .count()
}

fn order_from_angle(angle_degrees: f64) -> Option<usize> {
    if !angle_degrees.is_finite() || angle_degrees <= 0.0 {
        return None;
    }
    let order = (360.0 / angle_degrees).round() as usize;
    (order >= 2).then_some(order)
}

fn gcd(left: usize, right: usize) -> usize {
    let mut a = left;
    let mut b = right;
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

fn add_reflection_plane(
    planes: &mut Vec<ReflectionPlaneRecord>,
    normal: [f64; 3],
    fixed_atom_count: usize,
    permutation: Vec<usize>,
    max_deviation: f64,
    delta: f64,
) {
    for existing in planes.iter_mut() {
        let overlap = dot(existing.normal, normal).abs();
        if overlap > (1.0 - delta) && overlap < (1.0 + delta) {
            if existing.max_deviation > max_deviation {
                existing.max_deviation = max_deviation;
                existing.permutation = permutation;
                existing.fixed_atom_count = fixed_atom_count;
                existing.normal = normal;
            }
            return;
        }
    }

    planes.push(ReflectionPlaneRecord {
        normal,
        fixed_atom_count,
        permutation,
        max_deviation,
    });
}

fn push_permutation(permutations: &mut Vec<PermutationRecord>, mapping: Vec<usize>) {
    if permutations
        .iter()
        .any(|existing| existing.mapping == mapping)
    {
        return;
    }
    permutations.push(PermutationRecord { mapping });
}

fn plane_fixed_atom_count(normal: [f64; 3], coordinates: &[[f64; 3]], delta: f64) -> usize {
    coordinates
        .iter()
        .filter(|coordinate| dot(normal, **coordinate).abs() <= delta)
        .count()
}

fn plane_max_deviation(normal: [f64; 3], coordinates: &[[f64; 3]]) -> f64 {
    coordinates
        .iter()
        .map(|coordinate| dot(normal, *coordinate).abs())
        .fold(0.0, f64::max)
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        -left[0] * right[2] + left[2] * right[0],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn normalize(vector: [f64; 3], delta: f64) -> Option<[f64; 3]> {
    let magnitude = norm(vector);
    (magnitude > delta).then(|| scale(vector, 1.0 / magnitude))
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(vector: [f64; 3], factor: f64) -> [f64; 3] {
    [vector[0] * factor, vector[1] * factor, vector[2] * factor]
}

fn negate(vector: [f64; 3]) -> [f64; 3] {
    scale(vector, -1.0)
}

#[cfg(test)]
mod tests {
    use super::{dot, search_symmetry_elements};
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input};
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};

    #[test]
    fn detects_linear_branches_for_co_and_co2() {
        let co2 = preprocess_fixture("CO2", false);
        let co2_result = search_symmetry_elements(&co2);
        assert!(co2_result.is_linear);
        assert_eq!(co2_result.center_atom_index, Some(2));
        assert!(co2_result.inversion_center.is_some());

        let co = preprocess_fixture("CO", false);
        let co_result = search_symmetry_elements(&co);
        assert!(co_result.is_linear);
        assert_eq!(co_result.center_atom_index, None);
        assert!(co_result.inversion_center.is_none());
    }

    #[test]
    fn recovers_reflection_planes_for_cathf6_subset() {
        let geometry = preprocess_fixture("CaTHF6", true);
        let result = search_symmetry_elements(&geometry);

        assert!(!result.is_linear);
        assert!(result.inversion_center.is_some());
        assert_eq!(result.center_atom_index, Some(1));
        assert_eq!(result.reflection_planes.len(), 9);
        assert!(contains_plane_close_to(
            result.reflection_planes.iter().map(|plane| plane.normal),
            [1.0, 0.0, 0.0],
            5.0e-3
        ));
        assert!(contains_plane_close_to(
            result.reflection_planes.iter().map(|plane| plane.normal),
            [0.0, 1.0, 0.0],
            5.0e-3
        ));
        assert!(contains_plane_close_to(
            result.reflection_planes.iter().map(|plane| plane.normal),
            [0.0, 0.0, 1.0],
            5.0e-3
        ));
    }

    #[test]
    fn keeps_unique_permutations_only() {
        let geometry = preprocess_fixture("CaTHF6", true);
        let result = search_symmetry_elements(&geometry);
        let unique_count = result
            .permutations
            .iter()
            .map(|permutation| permutation.mapping.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert_eq!(unique_count, result.permutations.len());
    }

    #[test]
    fn detects_c2_axis_for_h2o2() {
        let geometry = preprocess_fixture("H2O2", false);
        let result = search_symmetry_elements(&geometry);
        assert!(contains_axis_close_to(
            result
                .proper_rotation_axes
                .iter()
                .map(|axis| (axis.direction, axis.order)),
            [0.0, 0.0, 1.0],
            2,
            5.0e-3,
        ));
        assert!(result
            .proper_rotations
            .iter()
            .any(|rotation| rotation.order == 2 && rotation.power == 1));
    }

    #[test]
    fn detects_cubane_rotation_families() {
        let geometry = preprocess_fixture("cubane", false);
        let result = search_symmetry_elements(&geometry);
        assert!(result.proper_rotation_axes.len() >= 13);
        assert!(result
            .proper_rotation_axes
            .iter()
            .any(|axis| axis.order == 4));
        assert!(result
            .proper_rotation_axes
            .iter()
            .any(|axis| axis.order == 3));
        assert!(result
            .proper_rotation_axes
            .iter()
            .any(|axis| axis.order == 2));
        assert!(contains_axis_close_to(
            result
                .proper_rotation_axes
                .iter()
                .map(|axis| (axis.direction, axis.order)),
            [1.0, 0.0, 0.0],
            4,
            5.0e-3,
        ));
        assert!(contains_axis_close_to(
            result
                .proper_rotation_axes
                .iter()
                .map(|axis| (axis.direction, axis.order)),
            [1.0, 1.0, 1.0],
            3,
            5.0e-3,
        ));
        assert!(result
            .improper_rotations
            .iter()
            .any(|rotation| rotation.order == 4));
        assert!(result
            .improper_rotations
            .iter()
            .any(|rotation| rotation.order == 6));
    }

    #[test]
    fn detects_neopentane_improper_rotations() {
        let geometry = preprocess_fixture("neopentane", false);
        let result = search_symmetry_elements(&geometry);

        assert_eq!(result.improper_rotations.len(), 6);
        assert!(result
            .improper_rotations
            .iter()
            .all(|rotation| rotation.order == 4));
        assert!(result
            .improper_rotations
            .iter()
            .any(|rotation| rotation.power == 1));
        assert!(result
            .improper_rotations
            .iter()
            .any(|rotation| rotation.power == 3));
    }

    #[test]
    fn detects_octahedral_subset_improper_rotations() {
        let geometry = preprocess_fixture("CaTHF6", true);
        let result = search_symmetry_elements(&geometry);

        assert!(result
            .improper_rotations
            .iter()
            .any(|rotation| rotation.order == 4));
        assert!(result
            .improper_rotations
            .iter()
            .any(|rotation| rotation.order == 6));
    }

    fn preprocess_fixture(
        name: &str,
        use_subset: bool,
    ) -> crate::preprocess::SyvaPreprocessedGeometry {
        let input_path = bundled_fixture_paths(name).input;
        let input = load_fixture_input(&input_path).expect("fixture input");
        preprocess_geometry(
            &input,
            &SyvaRunSettings {
                use_subset,
                ..SyvaRunSettings::default()
            },
        )
        .expect("preprocess")
    }

    fn contains_plane_close_to(
        mut normals: impl Iterator<Item = [f64; 3]>,
        target: [f64; 3],
        tolerance: f64,
    ) -> bool {
        normals.any(|normal| dot(normal, target).abs() >= 1.0 - tolerance)
    }

    fn contains_axis_close_to(
        mut axes: impl Iterator<Item = ([f64; 3], usize)>,
        target: [f64; 3],
        order: usize,
        tolerance: f64,
    ) -> bool {
        axes.any(|(axis, axis_order)| {
            axis_order == order && dot(axis, normalize_test(target)).abs() >= 1.0 - tolerance
        })
    }

    fn normalize_test(vector: [f64; 3]) -> [f64; 3] {
        let norm = dot(vector, vector).sqrt();
        [vector[0] / norm, vector[1] / norm, vector[2] / norm]
    }
}
