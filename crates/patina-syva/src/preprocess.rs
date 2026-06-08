use crate::error::SyvaError;
use crate::model::{SyvaInputAtom, SyvaInputGeometry};
use crate::periodic_table::legacy_atomic_mass;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyvaRunSettings {
    pub tolerance: f64,
    pub output_level: usize,
    pub maxsym: bool,
    pub optimize_all_subgroups: bool,
    pub permutations: bool,
    pub gaussian: bool,
    pub use_subset: bool,
    pub tolerance_upper: f64,
    pub tolerance_lower: f64,
}

impl Default for SyvaRunSettings {
    fn default() -> Self {
        Self {
            tolerance: 0.001,
            output_level: 2,
            maxsym: false,
            optimize_all_subgroups: false,
            permutations: false,
            gaussian: false,
            use_subset: false,
            tolerance_upper: 5.0e-2,
            tolerance_lower: 5.0e-3,
        }
    }
}

impl SyvaRunSettings {
    pub fn validate(&self) -> Result<(), SyvaError> {
        if !self.tolerance.is_finite() || self.tolerance <= 0.0 {
            return Err(SyvaError::InvalidTolerance(self.tolerance));
        }
        if !self.tolerance_lower.is_finite()
            || !self.tolerance_upper.is_finite()
            || self.tolerance_lower <= 0.0
            || self.tolerance_upper <= 0.0
            || self.tolerance_lower > self.tolerance_upper
        {
            return Err(SyvaError::InvalidToleranceWindow {
                lower: self.tolerance_lower,
                upper: self.tolerance_upper,
            });
        }
        Ok(())
    }

    pub fn effective_maxsym(&self) -> bool {
        self.maxsym || self.optimize_all_subgroups
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyvaPreprocessedAtom {
    pub source_atom_index: usize,
    pub atomic_number: u8,
    pub species: String,
    pub original_cartesian: [f64; 3],
    pub shifted_cartesian: [f64; 3],
    pub atomic_mass: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyvaAtomClass {
    pub atomic_number: u8,
    pub species: String,
    pub atom_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyvaPreprocessedGeometry {
    pub title: String,
    pub molecular_weight: f64,
    pub centre_of_mass: [f64; 3],
    pub full_atom_count: usize,
    pub used_subset: bool,
    pub active_atoms: Vec<SyvaPreprocessedAtom>,
    pub atom_classes: Vec<SyvaAtomClass>,
    pub settings: SyvaRunSettings,
}

pub fn preprocess_geometry(
    input: &SyvaInputGeometry,
    settings: &SyvaRunSettings,
) -> Result<SyvaPreprocessedGeometry, SyvaError> {
    settings.validate()?;

    let molecular_weight = molecular_weight(&input.atoms)?;
    if molecular_weight <= 0.0 {
        return Err(SyvaError::ZeroMolecularWeight);
    }

    let centre_of_mass = centre_of_mass(&input.atoms, molecular_weight)?;
    let used_subset = settings.use_subset && input.subset.is_some();

    let active_atoms = if used_subset {
        let subset = input.subset.as_ref().expect("subset already checked");
        subset
            .atom_indices
            .iter()
            .enumerate()
            .map(|(subset_position, &atom_index)| {
                let selected = input.atoms.get(atom_index.saturating_sub(1)).ok_or(
                    SyvaError::InvalidSubsetAtomIndex {
                        subset_position,
                        atom_index,
                        atom_count: input.atoms.len(),
                    },
                )?;
                build_preprocessed_atom(selected, atom_index, centre_of_mass)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        input
            .atoms
            .iter()
            .enumerate()
            .map(|(index, atom)| build_preprocessed_atom(atom, index + 1, centre_of_mass))
            .collect::<Result<Vec<_>, _>>()?
    };

    let atom_classes = build_atom_classes(&active_atoms);

    Ok(SyvaPreprocessedGeometry {
        title: input.title.clone(),
        molecular_weight,
        centre_of_mass,
        full_atom_count: input.atoms.len(),
        used_subset,
        active_atoms,
        atom_classes,
        settings: settings.clone(),
    })
}

fn molecular_weight(atoms: &[SyvaInputAtom]) -> Result<f64, SyvaError> {
    atoms.iter().try_fold(
        0.0,
        |accumulator, atom| Ok(accumulator + atomic_mass(atom)?),
    )
}

fn centre_of_mass(atoms: &[SyvaInputAtom], molecular_weight: f64) -> Result<[f64; 3], SyvaError> {
    let mut weighted_sum = [0.0; 3];
    for atom in atoms {
        let mass = atomic_mass(atom)?;
        for (axis, accumulator) in weighted_sum.iter_mut().enumerate() {
            *accumulator += mass * atom.cartesian[axis];
        }
    }

    if molecular_weight <= 0.0 {
        return Err(SyvaError::ZeroMolecularWeight);
    }

    Ok(weighted_sum.map(|value| value / molecular_weight))
}

fn build_preprocessed_atom(
    atom: &SyvaInputAtom,
    source_atom_index: usize,
    centre_of_mass: [f64; 3],
) -> Result<SyvaPreprocessedAtom, SyvaError> {
    let atomic_mass = atomic_mass(atom)?;
    Ok(SyvaPreprocessedAtom {
        source_atom_index,
        atomic_number: atom.atomic_number,
        species: atom.species.clone(),
        original_cartesian: atom.cartesian,
        shifted_cartesian: [
            atom.cartesian[0] - centre_of_mass[0],
            atom.cartesian[1] - centre_of_mass[1],
            atom.cartesian[2] - centre_of_mass[2],
        ],
        atomic_mass,
    })
}

fn atomic_mass(atom: &SyvaInputAtom) -> Result<f64, SyvaError> {
    legacy_atomic_mass(atom.atomic_number).ok_or_else(|| SyvaError::MissingAtomicWeight {
        atomic_number: atom.atomic_number,
        species: atom.species.clone(),
    })
}

fn build_atom_classes(atoms: &[SyvaPreprocessedAtom]) -> Vec<SyvaAtomClass> {
    let mut classes = Vec::<SyvaAtomClass>::new();
    for (active_index, atom) in atoms.iter().enumerate() {
        if let Some(existing) = classes
            .iter_mut()
            .find(|class| class.atomic_number == atom.atomic_number)
        {
            existing.atom_indices.push(active_index + 1);
            continue;
        }

        classes.push(SyvaAtomClass {
            atomic_number: atom.atomic_number,
            species: atom.species.clone(),
            atom_indices: vec![active_index + 1],
        });
    }
    classes
}

#[cfg(test)]
mod tests {
    use super::{preprocess_geometry, SyvaRunSettings};
    use crate::fixtures::{bundled_fixture_paths, load_fixture_input, load_fixture_output_summary};
    use crate::model::{SyvaInputAtom, SyvaInputGeometry};
    use crate::SyvaError;

    #[test]
    fn benzene_preprocessing_matches_fixture_output() {
        let paths = bundled_fixture_paths("benzene");
        let input = load_fixture_input(&paths.input).expect("benzene input");
        let expected = load_fixture_output_summary(&paths.output).expect("benzene output");

        let actual = preprocess_geometry(&input, &SyvaRunSettings::default()).expect("preprocess");

        assert!(!actual.used_subset);
        assert_close(
            actual.molecular_weight,
            expected.molecular_weight.expect("weight"),
            1.0e-6,
        );
        assert_vector_close(
            actual.centre_of_mass,
            expected.centre_of_mass.expect("centre of mass"),
            1.0e-6,
        );
        assert_eq!(actual.active_atoms.len(), expected.shifted_atoms.len());
        for (actual_atom, expected_atom) in actual.active_atoms.iter().zip(&expected.shifted_atoms)
        {
            assert_eq!(actual_atom.atomic_number, expected_atom.atomic_number);
            assert_vector_close(
                actual_atom.shifted_cartesian,
                expected_atom.cartesian,
                1.0e-6,
            );
        }
        assert_eq!(actual.atom_classes.len(), 2);
        assert_eq!(actual.atom_classes[0].species, "C");
        assert_eq!(actual.atom_classes[0].atom_indices, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(actual.atom_classes[1].species, "H");
        assert_eq!(
            actual.atom_classes[1].atom_indices,
            vec![7, 8, 9, 10, 11, 12]
        );
    }

    #[test]
    fn subset_preprocessing_matches_fixture_output() {
        let paths = bundled_fixture_paths("CaTHF6");
        let input = load_fixture_input(&paths.input).expect("CaTHF6 input");
        let expected = load_fixture_output_summary(&paths.output.with_file_name("CaTHF6_subset"))
            .expect("CaTHF6 subset output");
        let settings = SyvaRunSettings {
            use_subset: true,
            ..SyvaRunSettings::default()
        };

        let actual = preprocess_geometry(&input, &settings).expect("subset preprocess");

        assert!(actual.used_subset);
        assert_eq!(actual.active_atoms.len(), 7);
        assert_close(
            actual.molecular_weight,
            expected.molecular_weight.expect("weight"),
            1.0e-6,
        );
        assert_vector_close(
            actual.centre_of_mass,
            expected.centre_of_mass.expect("centre of mass"),
            1.0e-6,
        );
        assert_eq!(actual.active_atoms.len(), expected.shifted_atoms.len());
        for (actual_atom, expected_atom) in actual.active_atoms.iter().zip(&expected.shifted_atoms)
        {
            assert_eq!(actual_atom.atomic_number, expected_atom.atomic_number);
            assert_vector_close(
                actual_atom.shifted_cartesian,
                expected_atom.cartesian,
                1.0e-6,
            );
        }
        assert_eq!(actual.atom_classes.len(), 2);
        assert_eq!(actual.atom_classes[0].species, "Ca");
        assert_eq!(actual.atom_classes[0].atom_indices, vec![1]);
        assert_eq!(actual.atom_classes[1].species, "O");
        assert_eq!(actual.atom_classes[1].atom_indices, vec![2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn optimize_all_subgroups_implies_effective_maxsym() {
        let settings = SyvaRunSettings {
            optimize_all_subgroups: true,
            ..SyvaRunSettings::default()
        };

        settings.validate().expect("valid settings");
        assert!(settings.effective_maxsym());
    }

    #[test]
    fn preprocessing_rejects_subset_index_out_of_bounds() {
        let input = SyvaInputGeometry {
            title: "bad subset".into(),
            atoms: vec![SyvaInputAtom {
                atomic_number: 8,
                species: "O".into(),
                cartesian: [0.0, 0.0, 0.0],
            }],
            subset: Some(crate::model::SyvaSubsetSpec {
                atom_count: 1,
                atom_indices: vec![2],
            }),
        };

        let error = preprocess_geometry(
            &input,
            &SyvaRunSettings {
                use_subset: true,
                ..SyvaRunSettings::default()
            },
        )
        .expect_err("invalid subset");

        assert_eq!(
            error,
            SyvaError::InvalidSubsetAtomIndex {
                subset_position: 0,
                atom_index: 2,
                atom_count: 1,
            }
        );
    }

    #[test]
    fn preprocessing_preserves_missing_legacy_masses_as_errors() {
        let input = SyvaInputGeometry {
            title: "thorium".into(),
            atoms: vec![SyvaInputAtom {
                atomic_number: 90,
                species: "Th".into(),
                cartesian: [0.0, 0.0, 0.0],
            }],
            subset: None,
        };

        let error =
            preprocess_geometry(&input, &SyvaRunSettings::default()).expect_err("missing weight");
        assert_eq!(
            error,
            SyvaError::MissingAtomicWeight {
                atomic_number: 90,
                species: "Th".into(),
            }
        );
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}, tolerance {tolerance}"
        );
    }

    fn assert_vector_close(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
        for axis in 0..3 {
            assert_close(actual[axis], expected[axis], tolerance);
        }
    }
}
