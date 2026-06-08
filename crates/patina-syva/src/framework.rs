use crate::model::PointGroupLabel;
use crate::preprocess::SyvaPreprocessedGeometry;
use crate::symmetry_elements::{PermutationRecord, SymmetrySearchResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetryEquivalenceClass {
    pub representative_active_atom_index: usize,
    pub representative_source_atom_index: usize,
    pub atomic_number: u8,
    pub species: String,
    pub active_atom_indices: Vec<usize>,
    pub source_atom_indices: Vec<usize>,
}

pub fn symmetry_equivalence_classes(
    geometry: &SyvaPreprocessedGeometry,
    permutations: &[PermutationRecord],
) -> Vec<SymmetryEquivalenceClass> {
    let atom_count = geometry.active_atoms.len();
    let mut classes = Vec::<SymmetryEquivalenceClass>::new();

    'outer: for atom_index in 1..=atom_count {
        for class in &classes {
            if class.active_atom_indices.contains(&atom_index) {
                continue 'outer;
            }
        }

        let mut active_atom_indices = Vec::<usize>::new();
        for permutation in permutations {
            let mapped = permutation
                .mapping
                .get(atom_index - 1)
                .copied()
                .unwrap_or(atom_index);
            if !active_atom_indices.contains(&mapped) {
                active_atom_indices.push(mapped);
            }
        }
        active_atom_indices.sort_unstable();

        let representative = &geometry.active_atoms[atom_index - 1];
        let source_atom_indices = active_atom_indices
            .iter()
            .map(|index| geometry.active_atoms[index - 1].source_atom_index)
            .collect::<Vec<_>>();

        classes.push(SymmetryEquivalenceClass {
            representative_active_atom_index: atom_index,
            representative_source_atom_index: representative.source_atom_index,
            atomic_number: representative.atomic_number,
            species: representative.species.clone(),
            active_atom_indices,
            source_atom_indices,
        });
    }

    classes
}

pub fn symmetry_equivalence_classes_for_search(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Vec<SymmetryEquivalenceClass> {
    symmetry_equivalence_classes(geometry, &search.permutations)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameworkSubspaceKind {
    Origin,
    ProperAxis,
    ReflectionPlane,
    Residual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisRole {
    Generic,
    Prime,
    DoublePrime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaneRole {
    Generic,
    Horizontal,
    Vertical,
    Dihedral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkAtomAssignment {
    pub active_atom_index: usize,
    pub source_atom_index: usize,
    pub species: String,
    pub subspace_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkGroupComponent {
    pub multiplicity: usize,
    pub kind: FrameworkSubspaceKind,
    pub subspace_label: String,
    pub atom_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifiedFrameworkGroup {
    pub point_group: PointGroupLabel,
    pub framework_group: String,
    pub atom_assignments: Vec<FrameworkAtomAssignment>,
    pub components: Vec<FrameworkGroupComponent>,
}

#[derive(Debug, Clone, PartialEq)]
struct CandidateSubspace {
    kind: FrameworkSubspaceKind,
    subspace_label: String,
    order: usize,
    axis_role: AxisRole,
    plane_role: PlaneRole,
    vector: [f64; 3],
    atom_indices: Vec<usize>,
}

pub fn classify_framework_group(
    point_group: &PointGroupLabel,
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> ClassifiedFrameworkGroup {
    let mut candidates = build_candidate_subspaces(point_group, geometry, search);
    let assignments = assign_atoms_to_subspaces(geometry, &mut candidates);
    let mut components = group_subspaces(geometry, &candidates);

    let residual_atoms = assignments
        .iter()
        .filter(|assignment| assignment.subspace_label == "X")
        .map(|assignment| assignment.active_atom_index)
        .collect::<Vec<_>>();
    if !residual_atoms.is_empty() {
        components.push(FrameworkGroupComponent {
            multiplicity: 1,
            kind: FrameworkSubspaceKind::Residual,
            subspace_label: "X".into(),
            atom_signature: render_count_signature(geometry, &residual_atoms),
        });
    }

    components.sort_by_key(component_sort_key);
    let rendered_components = components
        .iter()
        .map(render_framework_component)
        .collect::<Vec<_>>();

    ClassifiedFrameworkGroup {
        point_group: point_group.clone(),
        framework_group: format!(
            "{}[{}]",
            point_group.as_str(),
            rendered_components.join(",")
        ),
        atom_assignments: assignments,
        components,
    }
}

fn build_candidate_subspaces(
    point_group: &PointGroupLabel,
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
) -> Vec<CandidateSubspace> {
    let delta = geometry.settings.tolerance;
    let mut candidates = Vec::<CandidateSubspace>::new();
    let label = point_group.as_str();
    let principal_axis = principal_axis_vector(geometry, search, delta);
    let proper_axes = search
        .proper_rotation_axes
        .iter()
        .map(|axis| CandidateSubspace {
            kind: FrameworkSubspaceKind::ProperAxis,
            subspace_label: axis_display_label(axis.order, classify_axis_role(label, axis.order)),
            order: axis.order,
            axis_role: classify_axis_role(label, axis.order),
            plane_role: PlaneRole::Generic,
            vector: axis.direction,
            atom_indices: Vec::new(),
        })
        .collect::<Vec<_>>();

    if search.is_linear {
        if let Some(axis) = principal_axis {
            candidates.push(CandidateSubspace {
                kind: FrameworkSubspaceKind::ProperAxis,
                subspace_label: axis_display_label(0, AxisRole::Generic),
                order: 0,
                axis_role: AxisRole::Generic,
                plane_role: PlaneRole::Generic,
                vector: axis,
                atom_indices: Vec::new(),
            });
        }
    } else {
        candidates.extend(proper_axes);
    }

    for plane in &search.reflection_planes {
        candidates.push(CandidateSubspace {
            kind: FrameworkSubspaceKind::ReflectionPlane,
            subspace_label: plane_display_label(classify_plane_role(
                label,
                plane.normal,
                principal_axis,
                delta,
            )),
            order: 0,
            axis_role: AxisRole::Generic,
            plane_role: classify_plane_role(label, plane.normal, principal_axis, delta),
            vector: plane.normal,
            atom_indices: Vec::new(),
        });
    }

    if search.center_atom_index.is_some() {
        candidates.insert(
            0,
            CandidateSubspace {
                kind: FrameworkSubspaceKind::Origin,
                subspace_label: "O".into(),
                order: 0,
                axis_role: AxisRole::Generic,
                plane_role: PlaneRole::Generic,
                vector: [0.0, 0.0, 0.0],
                atom_indices: Vec::new(),
            },
        );
    }

    candidates
}

fn assign_atoms_to_subspaces(
    geometry: &SyvaPreprocessedGeometry,
    candidates: &mut [CandidateSubspace],
) -> Vec<FrameworkAtomAssignment> {
    let delta = geometry.settings.tolerance;
    let mut owner = vec![None::<usize>; geometry.active_atoms.len()];

    if matches!(
        candidates.first().map(|candidate| &candidate.kind),
        Some(FrameworkSubspaceKind::Origin)
    ) {
        for (atom_index, atom) in geometry.active_atoms.iter().enumerate() {
            if norm(atom.shifted_cartesian) <= delta {
                owner[atom_index] = Some(0);
                break;
            }
        }
    }

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if candidate.kind == FrameworkSubspaceKind::Origin {
            continue;
        }

        for (atom_index, atom) in geometry.active_atoms.iter().enumerate() {
            if owner[atom_index] == Some(0) {
                continue;
            }

            let lies_on = match candidate.kind {
                FrameworkSubspaceKind::ProperAxis => {
                    norm(cross(candidate.vector, atom.shifted_cartesian)) <= delta
                }
                FrameworkSubspaceKind::ReflectionPlane => {
                    dot(candidate.vector, atom.shifted_cartesian).abs() <= delta
                }
                FrameworkSubspaceKind::Origin | FrameworkSubspaceKind::Residual => false,
            };
            if !lies_on {
                continue;
            }

            match owner[atom_index] {
                Some(existing) => {
                    if candidate_priority(candidate) > candidate_priority(&candidates[existing]) {
                        owner[atom_index] = Some(candidate_index);
                    }
                }
                None => owner[atom_index] = Some(candidate_index),
            }
        }
    }

    for candidate in candidates.iter_mut() {
        candidate.atom_indices.clear();
    }

    let mut assignments =
        Vec::<FrameworkAtomAssignment>::with_capacity(geometry.active_atoms.len());
    for (atom_index, atom) in geometry.active_atoms.iter().enumerate() {
        let assigned_label = owner[atom_index]
            .map(|candidate_index| {
                candidates[candidate_index]
                    .atom_indices
                    .push(atom_index + 1);
                candidates[candidate_index].subspace_label.clone()
            })
            .unwrap_or_else(|| "X".into());
        assignments.push(FrameworkAtomAssignment {
            active_atom_index: atom_index + 1,
            source_atom_index: atom.source_atom_index,
            species: atom.species.clone(),
            subspace_label: assigned_label,
        });
    }

    assignments
}

fn group_subspaces(
    geometry: &SyvaPreprocessedGeometry,
    candidates: &[CandidateSubspace],
) -> Vec<FrameworkGroupComponent> {
    let mut grouped = BTreeMap::<(u8, String, String), FrameworkGroupComponent>::new();

    for candidate in candidates {
        if candidate.atom_indices.is_empty() {
            continue;
        }

        let atom_signature = match candidate.kind {
            FrameworkSubspaceKind::Origin | FrameworkSubspaceKind::ReflectionPlane => {
                render_count_signature(geometry, &candidate.atom_indices)
            }
            FrameworkSubspaceKind::ProperAxis => {
                render_axis_signature(geometry, &candidate.atom_indices, candidate.vector)
            }
            FrameworkSubspaceKind::Residual => continue,
        };
        let component = FrameworkGroupComponent {
            multiplicity: 1,
            kind: candidate.kind.clone(),
            subspace_label: candidate.subspace_label.clone(),
            atom_signature: atom_signature.clone(),
        };
        let key = (
            component_kind_rank(&component.kind),
            component.subspace_label.clone(),
            atom_signature,
        );
        grouped
            .entry(key)
            .and_modify(|existing| existing.multiplicity += 1)
            .or_insert(component);
    }

    grouped.into_values().collect()
}

fn candidate_priority(candidate: &CandidateSubspace) -> (u8, usize, u8) {
    match candidate.kind {
        FrameworkSubspaceKind::Origin => (3, usize::MAX, 0),
        FrameworkSubspaceKind::ProperAxis => {
            (2, candidate.order, axis_role_priority(candidate.axis_role))
        }
        FrameworkSubspaceKind::ReflectionPlane => (1, 0, plane_role_priority(candidate.plane_role)),
        FrameworkSubspaceKind::Residual => (0, 0, 0),
    }
}

fn classify_axis_role(point_group: &str, order: usize) -> AxisRole {
    if point_group == "D2d" && order == 2 {
        return AxisRole::Prime;
    }
    if matches!(point_group, "D2" | "D2h") && order == 2 {
        return AxisRole::Prime;
    }
    AxisRole::Generic
}

fn classify_plane_role(
    point_group: &str,
    normal: [f64; 3],
    principal_axis: Option<[f64; 3]>,
    delta: f64,
) -> PlaneRole {
    if point_group == "Cs" || point_group == "Th" || point_group == "Ih" {
        return PlaneRole::Horizontal;
    }
    if point_group.ends_with('d') || point_group == "Td" {
        return PlaneRole::Dihedral;
    }
    if point_group.ends_with('v') {
        return PlaneRole::Vertical;
    }
    if point_group.ends_with('h') || point_group == "Dih" {
        if let Some(axis) = principal_axis {
            if dot(axis, normal).abs() >= 1.0 - delta {
                return PlaneRole::Horizontal;
            }
        } else {
            return PlaneRole::Horizontal;
        }
    }
    PlaneRole::Generic
}

fn principal_axis_vector(
    geometry: &SyvaPreprocessedGeometry,
    search: &SymmetrySearchResult,
    delta: f64,
) -> Option<[f64; 3]> {
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

fn render_framework_component(component: &FrameworkGroupComponent) -> String {
    let prefix = if component.multiplicity > 1 {
        component.multiplicity.to_string()
    } else {
        String::new()
    };
    format!(
        "{prefix}{}({})",
        component.subspace_label, component.atom_signature
    )
}

fn render_count_signature(geometry: &SyvaPreprocessedGeometry, atom_indices: &[usize]) -> String {
    let mut counts = BTreeMap::<(u8, String), usize>::new();
    for &atom_index in atom_indices {
        let atom = &geometry.active_atoms[atom_index - 1];
        *counts
            .entry((atom.atomic_number, atom.species.clone()))
            .or_insert(0) += 1;
    }

    counts
        .into_iter()
        .rev()
        .map(|((_, species), count)| {
            if count > 1 {
                format!("{species}{count}")
            } else {
                species
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_axis_signature(
    geometry: &SyvaPreprocessedGeometry,
    atom_indices: &[usize],
    axis: [f64; 3],
) -> String {
    let mut oriented_axis = axis;
    if let Some(reference) = atom_indices
        .iter()
        .map(|index| geometry.active_atoms[index - 1].shifted_cartesian)
        .find(|coordinate| norm(*coordinate) > geometry.settings.tolerance)
    {
        if dot(reference, oriented_axis) < 0.0 {
            oriented_axis = scale(oriented_axis, -1.0);
        }
    }

    let mut positive = Vec::<(f64, String)>::new();
    let mut centered = Vec::<String>::new();
    let mut negative = Vec::<(f64, String)>::new();
    for &atom_index in atom_indices {
        let atom = &geometry.active_atoms[atom_index - 1];
        let projection = dot(oriented_axis, atom.shifted_cartesian);
        if projection > geometry.settings.tolerance {
            positive.push((projection, atom.species.clone()));
        } else if projection < -geometry.settings.tolerance {
            negative.push((projection, atom.species.clone()));
        } else {
            centered.push(atom.species.clone());
        }
    }

    positive.sort_by(|left, right| right.0.total_cmp(&left.0));
    negative.sort_by(|left, right| right.0.total_cmp(&left.0));

    let mut groups = Vec::<String>::new();
    if !positive.is_empty() {
        groups.push(
            positive
                .into_iter()
                .map(|(_, species)| species)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if !centered.is_empty() {
        groups.push(centered.join(","));
    }
    if !negative.is_empty() {
        groups.push(
            negative
                .into_iter()
                .map(|(_, species)| species)
                .collect::<Vec<_>>()
                .join(","),
        );
    }

    groups.join(".")
}

fn axis_display_label(order: usize, role: AxisRole) -> String {
    let suffix = match role {
        AxisRole::Generic => "",
        AxisRole::Prime => "'",
        AxisRole::DoublePrime => "\"",
    };
    let order = if order == 0 {
        "inf".into()
    } else {
        order.to_string()
    };
    format!("C{suffix}{order}")
}

fn plane_display_label(role: PlaneRole) -> String {
    match role {
        PlaneRole::Generic => "SG".into(),
        PlaneRole::Horizontal => "SGH".into(),
        PlaneRole::Vertical => "SGV".into(),
        PlaneRole::Dihedral => "SGD".into(),
    }
}

fn component_sort_key(component: &FrameworkGroupComponent) -> (u8, usize, String, String) {
    (
        component_kind_rank(&component.kind),
        component_order(&component.subspace_label),
        component.subspace_label.clone(),
        component.atom_signature.clone(),
    )
}

fn component_kind_rank(kind: &FrameworkSubspaceKind) -> u8 {
    match kind {
        FrameworkSubspaceKind::Origin => 0,
        FrameworkSubspaceKind::ProperAxis => 1,
        FrameworkSubspaceKind::ReflectionPlane => 2,
        FrameworkSubspaceKind::Residual => 3,
    }
}

fn component_order(label: &str) -> usize {
    if label == "X" || label == "O" {
        return 0;
    }
    label
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<usize>()
        .unwrap_or(usize::MAX / 2)
}

fn axis_role_priority(role: AxisRole) -> u8 {
    match role {
        AxisRole::Generic => 3,
        AxisRole::Prime => 2,
        AxisRole::DoublePrime => 1,
    }
}

fn plane_role_priority(role: PlaneRole) -> u8 {
    match role {
        PlaneRole::Horizontal => 4,
        PlaneRole::Vertical => 3,
        PlaneRole::Dihedral => 2,
        PlaneRole::Generic => 1,
    }
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
    use super::{classify_framework_group, symmetry_equivalence_classes_for_search};
    use crate::classify::analyze_point_group;
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input, load_fixture_output_summary};
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};
    use crate::symmetry_elements::search_symmetry_elements;

    #[test]
    fn matches_fixture_equivalence_classes_for_representative_molecules() {
        assert_fixture_classes("CO2", "CO2", false);
        assert_fixture_classes("H2O2", "H2O2", false);
        assert_fixture_classes("propyne", "propyne", false);
        assert_fixture_classes("CaTHF6", "CaTHF6_subset", true);
    }

    #[test]
    fn matches_framework_group_for_representative_molecules() {
        assert_framework_group("CO", "CO", false);
        assert_framework_group("CO2", "CO2", false);
        assert_framework_group("H2O2", "H2O2", false);
        assert_framework_group("propyne", "propyne", false);
        assert_framework_group("neopentane", "neopentane", false);
        assert_framework_group("cubane", "cubane", false);
        assert_framework_group("CaTHF6", "CaTHF6_subset", true);
        assert_framework_group("benzene", "benzene", false);
    }

    fn assert_fixture_classes(input_name: &str, output_name: &str, use_subset: bool) {
        let input_paths = bundled_fixture_paths(input_name);
        let output_paths = bundled_fixture_paths(output_name);
        let input = load_fixture_input(&input_paths.input).expect("fixture input");
        let output = load_fixture_output_summary(&output_paths.output).expect("fixture output");
        let geometry = preprocess_geometry(
            &input,
            &SyvaRunSettings {
                use_subset,
                ..SyvaRunSettings::default()
            },
        )
        .expect("preprocess");
        let search = search_symmetry_elements(&geometry);
        let actual = symmetry_equivalence_classes_for_search(&geometry, &search);

        let expected_species = output
            .symmetry_equivalence_classes
            .iter()
            .map(|class| class.species.clone())
            .collect::<Vec<_>>();
        let expected_indices = output
            .symmetry_equivalence_classes
            .iter()
            .map(|class| class.atom_indices.clone())
            .collect::<Vec<_>>();
        let actual_species = actual
            .iter()
            .map(|class| class.species.clone())
            .collect::<Vec<_>>();
        let actual_indices = actual
            .iter()
            .map(|class| class.active_atom_indices.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            actual_species, expected_species,
            "{output_name} species classes"
        );
        assert_eq!(
            actual_indices, expected_indices,
            "{output_name} atom classes"
        );
    }

    fn assert_framework_group(input_name: &str, output_name: &str, use_subset: bool) {
        let input_paths = bundled_fixture_paths(input_name);
        let output_paths = bundled_fixture_paths(output_name);
        let input = load_fixture_input(&input_paths.input).expect("fixture input");
        let output = load_fixture_output_summary(&output_paths.output).expect("fixture output");
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
        let actual = classify_framework_group(&point_group.label, &geometry, &search);

        assert_eq!(
            Some(actual.framework_group.as_str()),
            output.framework_group.as_deref(),
            "{output_name} framework group"
        );
    }
}
