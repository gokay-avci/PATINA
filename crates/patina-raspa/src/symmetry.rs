use moyo::{base::Operation, MoyoDataset};
use serde::{Deserialize, Serialize};

use crate::framework::{
    decode_species, matrix3_to_array_f64, species_catalog, vector3_to_array_f64, FrameworkAtom,
    PeriodicFramework,
};
use crate::RaspaInterfaceError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SymmetryTolerance {
    pub position_tolerance: f64,
    pub cell_tolerance: f64,
}

impl Default for SymmetryTolerance {
    fn default() -> Self {
        Self {
            position_tolerance: 1.0e-5,
            cell_tolerance: 1.0e-5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetryOperation {
    pub rotation: [[i32; 3]; 3],
    pub translation: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetryAnalysis {
    pub hall_number: Option<usize>,
    pub international_number: Option<usize>,
    pub hm_symbol: Option<String>,
    pub primitive_lattice: Option<[[f64; 3]; 3]>,
    pub reduced_lattice: Option<[[f64; 3]; 3]>,
    pub symmetrized_atoms: Vec<FrameworkAtom>,
    pub operations: Vec<SymmetryOperation>,
    pub orbits: Vec<usize>,
    pub wyckoff_letters: Vec<String>,
    pub standardized_framework: Option<PeriodicFramework>,
    pub primitive_standardized_framework: Option<PeriodicFramework>,
}

pub trait SymmetryAnalyzer {
    fn analyze(
        &self,
        framework: &PeriodicFramework,
        tolerance: SymmetryTolerance,
    ) -> Result<SymmetryAnalysis, RaspaInterfaceError>;
}

#[derive(Debug, Clone, Default)]
pub struct MoyoSymmetryAnalyzer;

impl SymmetryAnalyzer for MoyoSymmetryAnalyzer {
    fn analyze(
        &self,
        framework: &PeriodicFramework,
        tolerance: SymmetryTolerance,
    ) -> Result<SymmetryAnalysis, RaspaInterfaceError> {
        let cell = framework.to_moyo_cell()?;
        let dataset = MoyoDataset::with_default(&cell, tolerance.position_tolerance)
            .map_err(|err| RaspaInterfaceError::SymmetryBackend(format!("{err:?}")))?;
        let species_labels = species_catalog(framework);

        let symmetrized_atoms = dataset
            .std_cell
            .positions
            .iter()
            .zip(dataset.std_cell.numbers.iter().copied())
            .map(|(position, number)| {
                let species = decode_species(number, &species_labels)?;
                Ok(FrameworkAtom {
                    species,
                    fractional: vector3_to_array_f64(position),
                })
            })
            .collect::<Result<Vec<_>, RaspaInterfaceError>>()?;

        let standardized_framework = PeriodicFramework::from_moyo_cell(
            format!("{}__standardized", framework.label),
            framework.periodic_axes,
            &dataset.std_cell,
            &species_labels,
        )?;
        let primitive_standardized_framework = PeriodicFramework::from_moyo_cell(
            format!("{}__primitive_standardized", framework.label),
            framework.periodic_axes,
            &dataset.prim_std_cell,
            &species_labels,
        )?;

        Ok(SymmetryAnalysis {
            hall_number: Some(dataset.hall_number as usize),
            international_number: Some(dataset.number as usize),
            hm_symbol: Some(dataset.hm_symbol.clone()),
            primitive_lattice: Some(matrix3_to_array_f64(&dataset.prim_std_cell.lattice.basis)),
            reduced_lattice: Some(matrix3_to_array_f64(&dataset.std_cell.lattice.basis)),
            symmetrized_atoms,
            operations: dataset
                .operations
                .iter()
                .map(symmetry_operation_from_moyo)
                .collect(),
            orbits: dataset.orbits.clone(),
            wyckoff_letters: dataset.wyckoffs.iter().map(char::to_string).collect(),
            standardized_framework: Some(standardized_framework),
            primitive_standardized_framework: Some(primitive_standardized_framework),
        })
    }
}

fn symmetry_operation_from_moyo(operation: &Operation) -> SymmetryOperation {
    let rotation = [
        [
            operation.rotation[(0, 0)],
            operation.rotation[(0, 1)],
            operation.rotation[(0, 2)],
        ],
        [
            operation.rotation[(1, 0)],
            operation.rotation[(1, 1)],
            operation.rotation[(1, 2)],
        ],
        [
            operation.rotation[(2, 0)],
            operation.rotation[(2, 1)],
            operation.rotation[(2, 2)],
        ],
    ];
    let translation = [
        operation.translation[0],
        operation.translation[1],
        operation.translation[2],
    ];
    SymmetryOperation {
        rotation,
        translation,
    }
}
