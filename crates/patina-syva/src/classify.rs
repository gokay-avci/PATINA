use crate::model::PointGroupLabel;
use crate::preprocess::{preprocess_geometry, SyvaPreprocessedGeometry, SyvaRunSettings};
use crate::symmetry_elements::{search_symmetry_elements, SymmetrySearchResult};
use crate::{SyvaError, SyvaInputGeometry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointGroupSignature {
    pub order: isize,
    pub has_inversion: bool,
    pub reflection_plane_count: usize,
    pub proper_rotation_count: usize,
    pub improper_rotation_count: usize,
    pub principal_order: isize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedPointGroup {
    pub label: PointGroupLabel,
    pub signature: PointGroupSignature,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceWindowHit {
    pub point_group: PointGroupLabel,
    pub tolerance_upper: f64,
    pub tolerance_lower: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceScanSummary {
    pub windows: Vec<ToleranceWindowHit>,
    pub largest_point_group: Option<PointGroupLabel>,
    pub optimize_tolerance_lower: Option<f64>,
    pub optimize_tolerance_upper: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PointGroupPattern {
    label: &'static str,
    order: isize,
    inversion: bool,
    planes: usize,
    proper: usize,
    improper: usize,
    principal: isize,
}

const POINT_GROUP_PATTERNS: &[PointGroupPattern] = &[
    PointGroupPattern {
        label: "C1",
        order: 1,
        inversion: false,
        planes: 0,
        proper: 0,
        improper: 0,
        principal: 0,
    },
    PointGroupPattern {
        label: "Cs",
        order: 2,
        inversion: false,
        planes: 1,
        proper: 0,
        improper: 0,
        principal: 0,
    },
    PointGroupPattern {
        label: "Ci",
        order: 2,
        inversion: true,
        planes: 0,
        proper: 0,
        improper: 0,
        principal: 0,
    },
    PointGroupPattern {
        label: "C2",
        order: 2,
        inversion: false,
        planes: 0,
        proper: 1,
        improper: 0,
        principal: 2,
    },
    PointGroupPattern {
        label: "C3",
        order: 3,
        inversion: false,
        planes: 0,
        proper: 2,
        improper: 0,
        principal: 3,
    },
    PointGroupPattern {
        label: "C4",
        order: 4,
        inversion: false,
        planes: 0,
        proper: 3,
        improper: 0,
        principal: 4,
    },
    PointGroupPattern {
        label: "C5",
        order: 5,
        inversion: false,
        planes: 0,
        proper: 4,
        improper: 0,
        principal: 5,
    },
    PointGroupPattern {
        label: "C6",
        order: 6,
        inversion: false,
        planes: 0,
        proper: 5,
        improper: 0,
        principal: 6,
    },
    PointGroupPattern {
        label: "C7",
        order: 7,
        inversion: false,
        planes: 0,
        proper: 6,
        improper: 0,
        principal: 7,
    },
    PointGroupPattern {
        label: "C8",
        order: 8,
        inversion: false,
        planes: 0,
        proper: 7,
        improper: 0,
        principal: 8,
    },
    PointGroupPattern {
        label: "D2",
        order: 4,
        inversion: false,
        planes: 0,
        proper: 3,
        improper: 0,
        principal: 2,
    },
    PointGroupPattern {
        label: "D3",
        order: 6,
        inversion: false,
        planes: 0,
        proper: 5,
        improper: 0,
        principal: 3,
    },
    PointGroupPattern {
        label: "D4",
        order: 8,
        inversion: false,
        planes: 0,
        proper: 7,
        improper: 0,
        principal: 4,
    },
    PointGroupPattern {
        label: "D5",
        order: 10,
        inversion: false,
        planes: 0,
        proper: 9,
        improper: 0,
        principal: 5,
    },
    PointGroupPattern {
        label: "D6",
        order: 12,
        inversion: false,
        planes: 0,
        proper: 11,
        improper: 0,
        principal: 6,
    },
    PointGroupPattern {
        label: "D7",
        order: 14,
        inversion: false,
        planes: 0,
        proper: 13,
        improper: 0,
        principal: 7,
    },
    PointGroupPattern {
        label: "D8",
        order: 16,
        inversion: false,
        planes: 0,
        proper: 15,
        improper: 0,
        principal: 8,
    },
    PointGroupPattern {
        label: "C2v",
        order: 4,
        inversion: false,
        planes: 2,
        proper: 1,
        improper: 0,
        principal: 2,
    },
    PointGroupPattern {
        label: "C3v",
        order: 6,
        inversion: false,
        planes: 3,
        proper: 2,
        improper: 0,
        principal: 3,
    },
    PointGroupPattern {
        label: "C4v",
        order: 8,
        inversion: false,
        planes: 4,
        proper: 3,
        improper: 0,
        principal: 4,
    },
    PointGroupPattern {
        label: "C5v",
        order: 10,
        inversion: false,
        planes: 5,
        proper: 4,
        improper: 0,
        principal: 5,
    },
    PointGroupPattern {
        label: "C6v",
        order: 12,
        inversion: false,
        planes: 6,
        proper: 5,
        improper: 0,
        principal: 6,
    },
    PointGroupPattern {
        label: "C7v",
        order: 14,
        inversion: false,
        planes: 7,
        proper: 6,
        improper: 0,
        principal: 7,
    },
    PointGroupPattern {
        label: "C8v",
        order: 16,
        inversion: false,
        planes: 8,
        proper: 7,
        improper: 0,
        principal: 8,
    },
    PointGroupPattern {
        label: "C2h",
        order: 4,
        inversion: true,
        planes: 1,
        proper: 1,
        improper: 0,
        principal: 2,
    },
    PointGroupPattern {
        label: "C3h",
        order: 6,
        inversion: false,
        planes: 1,
        proper: 2,
        improper: 2,
        principal: 3,
    },
    PointGroupPattern {
        label: "C4h",
        order: 8,
        inversion: true,
        planes: 1,
        proper: 3,
        improper: 2,
        principal: 4,
    },
    PointGroupPattern {
        label: "C5h",
        order: 10,
        inversion: false,
        planes: 1,
        proper: 4,
        improper: 4,
        principal: 5,
    },
    PointGroupPattern {
        label: "C6h",
        order: 12,
        inversion: true,
        planes: 1,
        proper: 5,
        improper: 4,
        principal: 6,
    },
    PointGroupPattern {
        label: "C7h",
        order: 14,
        inversion: false,
        planes: 1,
        proper: 6,
        improper: 6,
        principal: 7,
    },
    PointGroupPattern {
        label: "C8h",
        order: 16,
        inversion: true,
        planes: 1,
        proper: 7,
        improper: 6,
        principal: 8,
    },
    PointGroupPattern {
        label: "D2h",
        order: 8,
        inversion: true,
        planes: 3,
        proper: 3,
        improper: 0,
        principal: 2,
    },
    PointGroupPattern {
        label: "D3h",
        order: 12,
        inversion: false,
        planes: 4,
        proper: 5,
        improper: 2,
        principal: 3,
    },
    PointGroupPattern {
        label: "D4h",
        order: 16,
        inversion: true,
        planes: 5,
        proper: 7,
        improper: 2,
        principal: 4,
    },
    PointGroupPattern {
        label: "D5h",
        order: 20,
        inversion: false,
        planes: 6,
        proper: 9,
        improper: 4,
        principal: 5,
    },
    PointGroupPattern {
        label: "D6h",
        order: 24,
        inversion: true,
        planes: 7,
        proper: 11,
        improper: 4,
        principal: 6,
    },
    PointGroupPattern {
        label: "D7h",
        order: 28,
        inversion: false,
        planes: 8,
        proper: 13,
        improper: 6,
        principal: 7,
    },
    PointGroupPattern {
        label: "D8h",
        order: 32,
        inversion: true,
        planes: 9,
        proper: 15,
        improper: 6,
        principal: 8,
    },
    PointGroupPattern {
        label: "D2d",
        order: 8,
        inversion: false,
        planes: 2,
        proper: 3,
        improper: 2,
        principal: 2,
    },
    PointGroupPattern {
        label: "D3d",
        order: 12,
        inversion: true,
        planes: 3,
        proper: 5,
        improper: 2,
        principal: 3,
    },
    PointGroupPattern {
        label: "D4d",
        order: 16,
        inversion: false,
        planes: 4,
        proper: 7,
        improper: 4,
        principal: 4,
    },
    PointGroupPattern {
        label: "D5d",
        order: 20,
        inversion: true,
        planes: 5,
        proper: 9,
        improper: 4,
        principal: 5,
    },
    PointGroupPattern {
        label: "D6d",
        order: 24,
        inversion: false,
        planes: 6,
        proper: 11,
        improper: 6,
        principal: 6,
    },
    PointGroupPattern {
        label: "D7d",
        order: 28,
        inversion: true,
        planes: 7,
        proper: 13,
        improper: 6,
        principal: 7,
    },
    PointGroupPattern {
        label: "D8d",
        order: 32,
        inversion: false,
        planes: 8,
        proper: 15,
        improper: 8,
        principal: 8,
    },
    PointGroupPattern {
        label: "S4",
        order: 4,
        inversion: false,
        planes: 0,
        proper: 1,
        improper: 2,
        principal: 2,
    },
    PointGroupPattern {
        label: "S6",
        order: 6,
        inversion: true,
        planes: 0,
        proper: 2,
        improper: 2,
        principal: 3,
    },
    PointGroupPattern {
        label: "S8",
        order: 8,
        inversion: false,
        planes: 0,
        proper: 3,
        improper: 4,
        principal: 4,
    },
    PointGroupPattern {
        label: "T",
        order: 12,
        inversion: false,
        planes: 0,
        proper: 11,
        improper: 0,
        principal: 3,
    },
    PointGroupPattern {
        label: "Th",
        order: 24,
        inversion: true,
        planes: 3,
        proper: 11,
        improper: 8,
        principal: 3,
    },
    PointGroupPattern {
        label: "Td",
        order: 24,
        inversion: false,
        planes: 6,
        proper: 11,
        improper: 6,
        principal: 3,
    },
    PointGroupPattern {
        label: "O",
        order: 24,
        inversion: false,
        planes: 0,
        proper: 23,
        improper: 0,
        principal: 4,
    },
    PointGroupPattern {
        label: "Oh",
        order: 48,
        inversion: true,
        planes: 9,
        proper: 23,
        improper: 14,
        principal: 4,
    },
    PointGroupPattern {
        label: "I",
        order: 60,
        inversion: false,
        planes: 0,
        proper: 59,
        improper: 0,
        principal: 5,
    },
    PointGroupPattern {
        label: "Ih",
        order: 120,
        inversion: true,
        planes: 15,
        proper: 59,
        improper: 44,
        principal: 5,
    },
    PointGroupPattern {
        label: "Civ",
        order: -1,
        inversion: false,
        planes: 1,
        proper: 1,
        improper: 0,
        principal: -1,
    },
    PointGroupPattern {
        label: "Dih",
        order: -1,
        inversion: true,
        planes: 1,
        proper: 2,
        improper: 1,
        principal: -1,
    },
];

pub fn point_group_signature(search: &SymmetrySearchResult) -> PointGroupSignature {
    if search.is_linear {
        return PointGroupSignature {
            order: -1,
            has_inversion: search.inversion_center.is_some(),
            reflection_plane_count: 1,
            proper_rotation_count: if search.inversion_center.is_some() {
                2
            } else {
                1
            },
            improper_rotation_count: usize::from(search.inversion_center.is_some()),
            principal_order: -1,
        };
    }

    let order = (1
        + usize::from(search.inversion_center.is_some())
        + search.reflection_planes.len()
        + search.proper_rotations.len()
        + search.improper_rotations.len()) as isize;
    let principal_order = search
        .proper_rotation_axes
        .iter()
        .map(|axis| axis.order)
        .max()
        .unwrap_or(0) as isize;

    PointGroupSignature {
        order,
        has_inversion: search.inversion_center.is_some(),
        reflection_plane_count: search.reflection_planes.len(),
        proper_rotation_count: search.proper_rotations.len(),
        improper_rotation_count: search.improper_rotations.len(),
        principal_order,
    }
}

pub fn classify_point_group(
    search: &SymmetrySearchResult,
) -> Result<Option<ClassifiedPointGroup>, SyvaError> {
    let signature = point_group_signature(search);
    let mut matches = POINT_GROUP_PATTERNS
        .iter()
        .filter(|pattern| {
            pattern.order == signature.order
                && pattern.inversion == signature.has_inversion
                && pattern.planes == signature.reflection_plane_count
                && pattern.proper == signature.proper_rotation_count
                && pattern.improper == signature.improper_rotation_count
                && pattern.principal == signature.principal_order
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        if signature.order == 1 {
            return Ok(Some(ClassifiedPointGroup {
                label: PointGroupLabel::new("C1")?,
                signature,
            }));
        }
        return Ok(None);
    }

    if matches.len() > 1 {
        return Ok(None);
    }

    let pattern = matches.remove(0);
    Ok(Some(ClassifiedPointGroup {
        label: PointGroupLabel::new(pattern.label)?,
        signature,
    }))
}

pub fn analyze_point_group(
    geometry: &SyvaPreprocessedGeometry,
) -> Result<Option<ClassifiedPointGroup>, SyvaError> {
    let search = search_symmetry_elements(geometry);
    classify_point_group(&search)
}

pub fn scan_point_groups_over_tolerance(
    input: &SyvaInputGeometry,
    settings: &SyvaRunSettings,
) -> Result<ToleranceScanSummary, SyvaError> {
    settings.validate()?;

    let mut current = settings.tolerance_upper;
    let mut windows = Vec::<ToleranceWindowHit>::new();
    let mut previous_label: Option<String> = None;
    let mut largest_group: Option<(isize, PointGroupLabel)> = None;

    while current >= settings.tolerance_lower {
        let geometry = preprocess_geometry(
            input,
            &SyvaRunSettings {
                tolerance: current,
                ..settings.clone()
            },
        )?;
        let search = search_symmetry_elements(&geometry);
        let Some(classified) = classify_point_group(&search)? else {
            current *= 0.97;
            previous_label = None;
            continue;
        };

        let next_lower = search.max_deviation;
        let label_string = classified.label.as_str().to_string();
        if previous_label.as_deref() != Some(classified.label.as_str()) {
            windows.push(ToleranceWindowHit {
                point_group: classified.label.clone(),
                tolerance_upper: current,
                tolerance_lower: next_lower,
            });
            previous_label = Some(label_string);
        } else if let Some(last) = windows.last_mut() {
            last.tolerance_lower = next_lower;
        }

        let magnitude = classified.signature.order.abs();
        match &largest_group {
            Some((existing, _)) if *existing >= magnitude => {}
            _ => largest_group = Some((magnitude, classified.label.clone())),
        }

        current = next_lower * 0.98;
    }

    let largest_point_group = largest_group.map(|(_, label)| label);
    let optimize_window = largest_point_group.as_ref().and_then(|label| {
        windows
            .iter()
            .find(|window| window.point_group.as_str() == label.as_str())
            .map(|window| (window.tolerance_lower, window.tolerance_upper))
    });

    Ok(ToleranceScanSummary {
        windows,
        largest_point_group,
        optimize_tolerance_lower: optimize_window.map(|window| window.0),
        optimize_tolerance_upper: optimize_window.map(|window| window.1),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_point_group, classify_point_group, point_group_signature,
        scan_point_groups_over_tolerance,
    };
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input};
    use crate::preprocess::{preprocess_geometry, SyvaRunSettings};
    use crate::symmetry_elements::search_symmetry_elements;

    #[test]
    fn classifies_representative_fixture_groups() {
        assert_fixture_group("CO", false, "Civ");
        assert_fixture_group("CO2", false, "Dih");
        assert_fixture_group("H2O2", false, "C2");
        assert_fixture_group("propyne", false, "C3v");
        assert_fixture_group("neopentane", false, "Td");
        assert_fixture_group("cubane", false, "Oh");
        assert_fixture_group("CaTHF6", true, "Oh");
    }

    #[test]
    fn computes_expected_signature_for_propyne() {
        let geometry = preprocess_fixture("propyne", false, 0.001);
        let search = search_symmetry_elements(&geometry);
        let signature = point_group_signature(&search);
        assert_eq!(signature.order, 6);
        assert!(!signature.has_inversion);
        assert_eq!(signature.reflection_plane_count, 3);
        assert_eq!(signature.proper_rotation_count, 2);
        assert_eq!(signature.improper_rotation_count, 0);
        assert_eq!(signature.principal_order, 3);
    }

    #[test]
    fn scans_linear_co2_tolerance_window() {
        let input = load_fixture("CO2");
        let summary = scan_point_groups_over_tolerance(
            &input,
            &SyvaRunSettings {
                tolerance_upper: 5.0e-2,
                tolerance_lower: 5.0e-3,
                ..SyvaRunSettings::default()
            },
        )
        .expect("scan");
        assert_eq!(
            summary
                .largest_point_group
                .as_ref()
                .map(|label| label.as_str()),
            Some("Dih")
        );
        assert!(!summary.windows.is_empty());
        assert_eq!(summary.windows[0].point_group.as_str(), "Dih");
    }

    fn assert_fixture_group(name: &str, use_subset: bool, expected: &str) {
        let geometry = preprocess_fixture(name, use_subset, 0.001);
        let classified = analyze_point_group(&geometry)
            .expect("classify")
            .expect("point group");
        assert_eq!(classified.label.as_str(), expected);

        let search = search_symmetry_elements(&geometry);
        let direct = classify_point_group(&search)
            .expect("classify")
            .expect("point group");
        assert_eq!(direct.label.as_str(), expected);
    }

    fn preprocess_fixture(
        name: &str,
        use_subset: bool,
        tolerance: f64,
    ) -> crate::preprocess::SyvaPreprocessedGeometry {
        let input = load_fixture(name);
        preprocess_geometry(
            &input,
            &SyvaRunSettings {
                tolerance,
                use_subset,
                ..SyvaRunSettings::default()
            },
        )
        .expect("preprocess")
    }

    fn load_fixture(name: &str) -> crate::SyvaInputGeometry {
        let input_path = bundled_fixture_paths(name).input;
        load_fixture_input(&input_path).expect("fixture input")
    }
}
