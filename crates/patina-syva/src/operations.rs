use crate::model::PointGroupLabel;
use crate::preprocess::SyvaPreprocessedGeometry;
use crate::symmetry_elements::{
    ImproperRotationRecord, ProperRotationRecord, SymmetryElementKind, SymmetrySearchResult,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetryOperationSummary {
    pub index: usize,
    pub label: String,
    pub kind: SymmetryElementKind,
    pub permutation: Vec<usize>,
    pub fixed_atom_count: usize,
    pub max_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepresentativeOperationClass {
    pub label: String,
    pub kind: SymmetryElementKind,
    pub multiplicity: usize,
    pub order: Option<usize>,
    pub power: Option<usize>,
    pub fixed_atom_count: usize,
    pub contribution: f64,
    pub reducible_character: f64,
}

pub fn summarize_operations(
    point_group: &PointGroupLabel,
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Vec<SymmetryOperationSummary> {
    let mut operations = Vec::<SymmetryOperationSummary>::new();
    operations.push(SymmetryOperationSummary {
        index: 1,
        label: "E".into(),
        kind: SymmetryElementKind::Identity,
        permutation: identity_permutation(geometry.active_atoms.len()),
        fixed_atom_count: geometry.active_atoms.len(),
        max_deviation: 0.0,
    });

    if let Some(inversion) = &search.inversion_center {
        operations.push(SymmetryOperationSummary {
            index: operations.len() + 1,
            label: "i".into(),
            kind: SymmetryElementKind::InversionCenter,
            permutation: inversion.permutation.clone(),
            fixed_atom_count: usize::from(search.center_atom_index.is_some()),
            max_deviation: inversion.max_deviation,
        });
    }

    let principal_axis = principal_axis_vector(geometry, search);
    let mut planes = search.reflection_planes.clone();
    planes.sort_by(|left, right| {
        plane_rank(point_group.as_str(), left.normal, principal_axis)
            .cmp(&plane_rank(
                point_group.as_str(),
                right.normal,
                principal_axis,
            ))
            .then_with(|| right.fixed_atom_count.cmp(&left.fixed_atom_count))
            .then_with(|| left.max_deviation.total_cmp(&right.max_deviation))
            .then_with(|| vector_sort_key(left.normal).cmp(&vector_sort_key(right.normal)))
    });
    for plane in planes {
        operations.push(SymmetryOperationSummary {
            index: operations.len() + 1,
            label: plane_label(point_group.as_str(), plane.normal, principal_axis),
            kind: SymmetryElementKind::ReflectionPlane,
            permutation: plane.permutation,
            fixed_atom_count: plane.fixed_atom_count,
            max_deviation: plane.max_deviation,
        });
    }

    let mut proper = search.proper_rotations.clone();
    proper.sort_by(proper_rotation_sort_key);
    for rotation in proper {
        operations.push(SymmetryOperationSummary {
            index: operations.len() + 1,
            label: proper_rotation_label(point_group.as_str(), &rotation),
            kind: SymmetryElementKind::ProperRotation,
            permutation: rotation.permutation,
            fixed_atom_count: rotation.fixed_atom_count,
            max_deviation: rotation.max_deviation,
        });
    }

    let mut improper = search.improper_rotations.clone();
    improper.sort_by(improper_rotation_sort_key);
    for rotation in improper {
        operations.push(SymmetryOperationSummary {
            index: operations.len() + 1,
            label: improper_rotation_label(&rotation),
            kind: SymmetryElementKind::ImproperRotation,
            permutation: rotation.permutation,
            fixed_atom_count: rotation.fixed_atom_count,
            max_deviation: rotation.max_deviation,
        });
    }

    operations
}

pub fn summarize_representative_operation_classes(
    point_group: &PointGroupLabel,
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Vec<RepresentativeOperationClass> {
    let mut representatives = Vec::<RepresentativeOperationClass>::new();
    representatives.push(RepresentativeOperationClass {
        label: "E".into(),
        kind: SymmetryElementKind::Identity,
        multiplicity: 1,
        order: None,
        power: None,
        fixed_atom_count: geometry.active_atoms.len(),
        contribution: 3.0,
        reducible_character: 3.0 * geometry.active_atoms.len() as f64,
    });

    if search.inversion_center.is_some() {
        let fixed_atom_count = usize::from(search.center_atom_index.is_some());
        representatives.push(RepresentativeOperationClass {
            label: "i".into(),
            kind: SymmetryElementKind::InversionCenter,
            multiplicity: 1,
            order: None,
            power: None,
            fixed_atom_count,
            contribution: -3.0,
            reducible_character: -3.0 * fixed_atom_count as f64,
        });
    }

    for plane in &search.reflection_planes {
        let label = plane_label(
            point_group.as_str(),
            plane.normal,
            principal_axis_vector(geometry, search),
        );
        if let Some(existing) = representatives.iter_mut().find(|candidate| {
            candidate.kind == SymmetryElementKind::ReflectionPlane
                && candidate.label == label
                && candidate.fixed_atom_count == plane.fixed_atom_count
        }) {
            existing.multiplicity += 1;
            continue;
        }

        representatives.push(RepresentativeOperationClass {
            label,
            kind: SymmetryElementKind::ReflectionPlane,
            multiplicity: 1,
            order: None,
            power: None,
            fixed_atom_count: plane.fixed_atom_count,
            contribution: 1.0,
            reducible_character: plane.fixed_atom_count as f64,
        });
    }

    for rotation in &search.proper_rotations {
        let order = rotation.order;
        let power = normalize_proper_power(order, rotation.power);
        let label = representative_proper_label(point_group.as_str(), order, power);
        if let Some(existing) = representatives.iter_mut().find(|candidate| {
            candidate.kind == SymmetryElementKind::ProperRotation
                && candidate.order == Some(order)
                && candidate.power == Some(power)
                && candidate.fixed_atom_count == rotation.fixed_atom_count
                && candidate.label == label
        }) {
            existing.multiplicity += 1;
            continue;
        }

        let contribution = 1.0 + 2.0 * angle_cos(order, power);
        representatives.push(RepresentativeOperationClass {
            label,
            kind: SymmetryElementKind::ProperRotation,
            multiplicity: 1,
            order: Some(order),
            power: Some(power),
            fixed_atom_count: rotation.fixed_atom_count,
            contribution,
            reducible_character: contribution * rotation.fixed_atom_count as f64,
        });
    }

    for rotation in &search.improper_rotations {
        let order = rotation.order;
        let power = normalize_improper_power(order, rotation.power);
        let label = representative_improper_label(order, power);
        if let Some(existing) = representatives.iter_mut().find(|candidate| {
            candidate.kind == SymmetryElementKind::ImproperRotation
                && candidate.order == Some(order)
                && candidate.power == Some(power)
                && candidate.fixed_atom_count == rotation.fixed_atom_count
                && candidate.label == label
        }) {
            existing.multiplicity += 1;
            continue;
        }

        let contribution = -1.0 + 2.0 * angle_cos(order, power);
        representatives.push(RepresentativeOperationClass {
            label,
            kind: SymmetryElementKind::ImproperRotation,
            multiplicity: 1,
            order: Some(order),
            power: Some(power),
            fixed_atom_count: rotation.fixed_atom_count,
            contribution,
            reducible_character: contribution * rotation.fixed_atom_count as f64,
        });
    }

    representatives.sort_by(representative_sort_key);
    representatives
}

fn identity_permutation(atom_count: usize) -> Vec<usize> {
    (1..=atom_count).collect()
}

fn principal_axis_vector(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Option<[f64; 3]> {
    let delta = geometry.settings.tolerance;
    if search.is_linear {
        return geometry
            .active_atoms
            .iter()
            .map(|atom| atom.shifted_cartesian)
            .find_map(|coordinate| normalize(coordinate, delta));
    }

    search
        .proper_rotation_axes
        .iter()
        .max_by_key(|axis| (axis.order, axis.fixed_atom_count))
        .map(|axis| axis.direction)
}

fn plane_label(point_group: &str, normal: [f64; 3], principal_axis: Option<[f64; 3]>) -> String {
    match plane_rank(point_group, normal, principal_axis) {
        0 => "Sigma_h".into(),
        1 => "Sigma_v".into(),
        2 => "Sigma_d".into(),
        _ => "Sigma_".into(),
    }
}

fn plane_rank(point_group: &str, normal: [f64; 3], principal_axis: Option<[f64; 3]>) -> u8 {
    let delta = 1.0e-6;
    if point_group == "Cs" || point_group == "Th" || point_group == "Ih" {
        return 0;
    }
    if point_group.ends_with('d') || point_group == "Td" {
        return 2;
    }
    if point_group.ends_with('v') {
        return 1;
    }
    if point_group.ends_with('h') || point_group == "Dih" {
        if let Some(axis) = principal_axis {
            if dot(axis, normal).abs() >= 1.0 - delta {
                return 0;
            }
        } else {
            return 0;
        }
    }
    3
}

fn proper_rotation_label(point_group: &str, rotation: &ProperRotationRecord) -> String {
    let mut label = String::from("C");
    label.push_str(&rotation.order.to_string());
    if rotation.power > 1 {
        label.push('^');
        label.push_str(&rotation.power.to_string());
    }
    if rotation.order == 2 {
        match axis_role(point_group, rotation.order) {
            1 => label.push('\''),
            2 => label.push('"'),
            _ => {}
        }
    }
    label
}

fn axis_role(point_group: &str, order: usize) -> u8 {
    if point_group == "D2d" && order == 2 {
        return 1;
    }
    if matches!(point_group, "D2" | "D2h") && order == 2 {
        return 1;
    }
    0
}

fn improper_rotation_label(rotation: &ImproperRotationRecord) -> String {
    let mut label = String::from("S");
    label.push_str(&rotation.order.to_string());
    if rotation.power > 1 {
        label.push('^');
        label.push_str(&rotation.power.to_string());
    }
    label
}

fn representative_proper_label(point_group: &str, order: usize, power: usize) -> String {
    let mut label = String::from("C");
    label.push_str(&order.to_string());
    if power > 1 {
        label.push('^');
        label.push_str(&power.to_string());
    }
    if order == 2 {
        match axis_role(point_group, order) {
            1 => label.push('\''),
            2 => label.push('"'),
            _ => {}
        }
    }
    label
}

fn representative_improper_label(order: usize, power: usize) -> String {
    let mut label = String::from("S");
    label.push_str(&order.to_string());
    if power > 1 {
        label.push('^');
        label.push_str(&power.to_string());
    }
    label
}

fn proper_rotation_sort_key(
    rotation: &ProperRotationRecord,
    other: &ProperRotationRecord,
) -> std::cmp::Ordering {
    other
        .order
        .cmp(&rotation.order)
        .then_with(|| rotation.power.cmp(&other.power))
        .then_with(|| other.fixed_atom_count.cmp(&rotation.fixed_atom_count))
        .then_with(|| rotation.max_deviation.total_cmp(&other.max_deviation))
        .then_with(|| vector_sort_key(rotation.direction).cmp(&vector_sort_key(other.direction)))
}

fn improper_rotation_sort_key(
    rotation: &ImproperRotationRecord,
    other: &ImproperRotationRecord,
) -> std::cmp::Ordering {
    other
        .order
        .cmp(&rotation.order)
        .then_with(|| rotation.power.cmp(&other.power))
        .then_with(|| other.fixed_atom_count.cmp(&rotation.fixed_atom_count))
        .then_with(|| rotation.max_deviation.total_cmp(&other.max_deviation))
        .then_with(|| vector_sort_key(rotation.direction).cmp(&vector_sort_key(other.direction)))
}

fn vector_sort_key(vector: [f64; 3]) -> [i64; 3] {
    const SCALE: f64 = 1.0e6;
    [
        (vector[0] * SCALE).round() as i64,
        (vector[1] * SCALE).round() as i64,
        (vector[2] * SCALE).round() as i64,
    ]
}

fn normalize_proper_power(order: usize, power: usize) -> usize {
    power.min(order.saturating_sub(power))
}

fn normalize_improper_power(order: usize, power: usize) -> usize {
    if order.is_multiple_of(2) {
        power.min(order.saturating_sub(power))
    } else {
        power.min(2 * order - power)
    }
}

fn angle_cos(order: usize, power: usize) -> f64 {
    let angle = 2.0 * std::f64::consts::PI * power as f64 / order as f64;
    angle.cos()
}

fn representative_sort_key(
    left: &RepresentativeOperationClass,
    right: &RepresentativeOperationClass,
) -> std::cmp::Ordering {
    representative_kind_rank(&left.kind)
        .cmp(&representative_kind_rank(&right.kind))
        .then_with(|| right.order.cmp(&left.order))
        .then_with(|| left.power.cmp(&right.power))
        .then_with(|| right.multiplicity.cmp(&left.multiplicity))
        .then_with(|| right.fixed_atom_count.cmp(&left.fixed_atom_count))
        .then_with(|| left.label.cmp(&right.label))
}

fn representative_kind_rank(kind: &SymmetryElementKind) -> u8 {
    match kind {
        SymmetryElementKind::Identity => 0,
        SymmetryElementKind::InversionCenter => 1,
        SymmetryElementKind::ReflectionPlane => 2,
        SymmetryElementKind::ProperRotationAxis => 3,
        SymmetryElementKind::ProperRotation => 4,
        SymmetryElementKind::ImproperRotation => 5,
    }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn scale(vector: [f64; 3], factor: f64) -> [f64; 3] {
    [vector[0] * factor, vector[1] * factor, vector[2] * factor]
}

fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn normalize(vector: [f64; 3], delta: f64) -> Option<[f64; 3]> {
    let magnitude = norm(vector);
    if magnitude <= delta {
        None
    } else {
        Some(scale(vector, 1.0 / magnitude))
    }
}

#[cfg(test)]
mod tests {
    use super::{summarize_operations, summarize_representative_operation_classes};
    use crate::classify::analyze_point_group;
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input};
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};
    use crate::symmetry_elements::search_symmetry_elements;

    #[test]
    fn summarizes_representative_operation_labels() {
        assert_labels("H2O2", false, &["E", "C2"]);
        assert_labels("propyne", false, &["E", "Sigma_v", "C3", "C3^2"]);
        assert_labels("cubane", false, &["E", "i", "C4", "C4^3", "S4"]);
    }

    #[test]
    fn summarizes_representative_operation_classes() {
        assert_representatives("propyne", false, &["E", "C3", "Sigma_v"]);
        assert_representatives("cubane", false, &["E", "i", "C4", "C3", "C2", "S6", "S4"]);
    }

    fn assert_labels(name: &str, use_subset: bool, expected_subset: &[&str]) {
        let paths = bundled_fixture_paths(name);
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry = preprocess_geometry(
            &input,
            &SyvaRunSettings {
                use_subset,
                ..SyvaRunSettings::default()
            },
        )
        .expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let point_group = analyze_point_group(&geometry)
            .expect("point group analysis")
            .expect("classified point group");
        let operations = summarize_operations(&point_group.label, &geometry, &search);
        let labels = operations
            .iter()
            .map(|operation| operation.label.as_str())
            .collect::<Vec<_>>();

        for expected in expected_subset {
            assert!(
                labels.contains(expected),
                "{name} missing operation label {expected:?} in {labels:?}"
            );
        }
    }

    fn assert_representatives(name: &str, use_subset: bool, expected_subset: &[&str]) {
        let paths = bundled_fixture_paths(name);
        let input = load_fixture_input(&paths.input).expect("fixture input");
        let geometry = preprocess_geometry(
            &input,
            &SyvaRunSettings {
                use_subset,
                ..SyvaRunSettings::default()
            },
        )
        .expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let point_group = analyze_point_group(&geometry)
            .expect("point group analysis")
            .expect("classified point group");
        let representatives =
            summarize_representative_operation_classes(&point_group.label, &geometry, &search);
        let labels = representatives
            .iter()
            .map(|representative| representative.label.as_str())
            .collect::<Vec<_>>();

        for expected in expected_subset {
            assert!(
                labels.contains(expected),
                "{name} missing representative label {expected:?} in {labels:?}"
            );
        }
    }
}
