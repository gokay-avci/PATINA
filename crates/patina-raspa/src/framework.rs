use std::collections::BTreeMap;

use moyo::base::{AtomicSpecie, Cell, Lattice, Position};
use patina_sci_kernel::{Framework3D, Site, StructureError};
use patina_types::Candidate;
use serde::{Deserialize, Serialize};

use crate::RaspaInterfaceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaspaPeriodicity {
    OneD,
    TwoD,
    ThreeD,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameworkAtom {
    pub species: String,
    pub fractional: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeriodicFramework {
    pub label: String,
    pub lattice: [[f64; 3]; 3],
    pub periodic_axes: [bool; 3],
    pub periodicity: RaspaPeriodicity,
    pub atoms: Vec<FrameworkAtom>,
}

impl PeriodicFramework {
    pub fn try_from_candidate(candidate: &Candidate) -> Result<Self, RaspaInterfaceError> {
        let framework = Framework3D::try_from(candidate).map_err(map_framework_structure_error)?;
        Ok(Self::from(&framework))
    }

    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    pub fn to_candidate(&self) -> Candidate {
        Candidate::periodic(
            self.label.clone(),
            self.atoms.iter().map(|atom| atom.species.clone()).collect(),
            self.atoms.iter().map(|atom| atom.fractional).collect(),
            self.lattice,
            self.periodic_axes,
        )
    }

    pub(crate) fn to_moyo_cell(&self) -> Result<Cell, RaspaInterfaceError> {
        let species_labels = species_catalog_from_atoms(&self.atoms);
        let numbers = encode_species(&self.atoms, &species_labels)?;
        let positions = self
            .atoms
            .iter()
            .map(|atom| Position::new(atom.fractional[0], atom.fractional[1], atom.fractional[2]))
            .collect();
        Ok(Cell::new(
            Lattice::from_basis(self.lattice),
            positions,
            numbers,
        ))
    }

    pub(crate) fn from_moyo_cell(
        label: impl Into<String>,
        periodic_axes: [bool; 3],
        cell: &Cell,
        species_labels: &[String],
    ) -> Result<Self, RaspaInterfaceError> {
        let lattice = matrix3_to_array_f64(&cell.lattice.basis);
        let atoms = cell
            .positions
            .iter()
            .zip(cell.numbers.iter().copied())
            .map(|(position, number)| {
                let species = decode_species(number, species_labels)?;
                Ok(FrameworkAtom {
                    species,
                    fractional: vector3_to_array_f64(position),
                })
            })
            .collect::<Result<Vec<_>, RaspaInterfaceError>>()?;
        Ok(Self {
            label: label.into(),
            lattice,
            periodic_axes,
            periodicity: RaspaPeriodicity::ThreeD,
            atoms,
        })
    }
}

impl From<&Framework3D> for PeriodicFramework {
    fn from(value: &Framework3D) -> Self {
        Self {
            label: value.metadata.label.clone(),
            lattice: value
                .lattice
                .expect("validated framework must have a lattice")
                .basis,
            periodic_axes: [true, true, true],
            periodicity: RaspaPeriodicity::ThreeD,
            atoms: value
                .sites
                .iter()
                .map(|site| FrameworkAtom {
                    species: site.species.clone(),
                    fractional: site.coords,
                })
                .collect(),
        }
    }
}

impl From<&PeriodicFramework> for Framework3D {
    fn from(value: &PeriodicFramework) -> Self {
        Framework3D::new(
            patina_sci_kernel::StructureMetadata {
                label: value.label.clone(),
            },
            value
                .atoms
                .iter()
                .map(|atom| Site {
                    species: atom.species.clone(),
                    coords: atom.fractional,
                })
                .collect(),
            patina_sci_kernel::CoordinateBasis::Fractional,
            Some(patina_sci_kernel::Lattice3::new(value.lattice)),
        )
        .expect("validated periodic framework always maps to Framework3D")
    }
}

fn map_framework_structure_error(error: StructureError) -> RaspaInterfaceError {
    match error {
        StructureError::UnexpectedPeriodicAxes { actual, .. }
            if actual == patina_sci_kernel::PeriodicAxes::NONE =>
        {
            RaspaInterfaceError::NonPeriodicCandidate
        }
        StructureError::UnexpectedPeriodicAxes { actual, .. } => {
            RaspaInterfaceError::PartialPeriodicityUnsupported {
                periodic_axes: actual.axes,
            }
        }
        StructureError::MissingLatticeForPeriodicAxes { .. } => RaspaInterfaceError::MissingLattice,
        other => RaspaInterfaceError::InvalidCandidate(format!("{other:?}")),
    }
}

pub(crate) fn encode_species(
    atoms: &[FrameworkAtom],
    species_labels: &[String],
) -> Result<Vec<AtomicSpecie>, RaspaInterfaceError> {
    let registry = species_labels
        .iter()
        .enumerate()
        .map(|(index, label)| (label.as_str(), (index + 1) as AtomicSpecie))
        .collect::<BTreeMap<_, _>>();
    atoms
        .iter()
        .map(|atom| {
            let symbol = atom.species.trim();
            if symbol.is_empty() {
                return Err(RaspaInterfaceError::InvalidSpeciesLabel);
            }
            registry
                .get(symbol)
                .copied()
                .ok_or(RaspaInterfaceError::InvalidSpeciesLabel)
        })
        .collect()
}

pub(crate) fn species_catalog(framework: &PeriodicFramework) -> Vec<String> {
    species_catalog_from_atoms(&framework.atoms)
}

pub(crate) fn species_catalog_from_atoms(atoms: &[FrameworkAtom]) -> Vec<String> {
    let mut labels = atoms
        .iter()
        .map(|atom| atom.species.trim().to_string())
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    labels
}

pub(crate) fn decode_species(
    code: AtomicSpecie,
    species_labels: &[String],
) -> Result<String, RaspaInterfaceError> {
    let index = usize::try_from(code)
        .ok()
        .and_then(|value| value.checked_sub(1))
        .ok_or(RaspaInterfaceError::InvalidSpeciesLabel)?;
    species_labels
        .get(index)
        .cloned()
        .ok_or(RaspaInterfaceError::InvalidSpeciesLabel)
}

pub(crate) fn vector3_to_array_f64(vector: &Position) -> [f64; 3] {
    [vector[0], vector[1], vector[2]]
}

pub(crate) fn matrix3_to_array_f64(
    matrix: &impl std::ops::Index<(usize, usize), Output = f64>,
) -> [[f64; 3]; 3] {
    [
        [matrix[(0, 0)], matrix[(0, 1)], matrix[(0, 2)]],
        [matrix[(1, 0)], matrix[(1, 1)], matrix[(1, 2)]],
        [matrix[(2, 0)], matrix[(2, 1)], matrix[(2, 2)]],
    ]
}
