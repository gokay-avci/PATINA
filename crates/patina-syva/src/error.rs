use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum SyvaError {
    #[error("cluster must contain at least one atom")]
    EmptyCluster,
    #[error("atom {index} has non-finite cartesian coordinate {coordinate:?}")]
    NonFiniteCoordinate { index: usize, coordinate: [f64; 3] },
    #[error("symmetry tolerance must be positive and finite, received {0}")]
    InvalidTolerance(f64),
    #[error(
        "symmetry tolerance window must be positive and ordered, received lower={lower}, upper={upper}"
    )]
    InvalidToleranceWindow { lower: f64, upper: f64 },
    #[error("point-group label must not be empty")]
    EmptyPointGroup,
    #[error("cluster atom {index} has empty species label")]
    EmptySpecies { index: usize },
    #[error("cluster atom {index} uses unsupported SYVA species `{species}`")]
    UnsupportedSpecies { index: usize, species: String },
    #[error("atomic number {atomic_number} is not supported by the current SYVA tables")]
    UnsupportedAtomicNumber { atomic_number: u8 },
    #[error(
        "SYVA legacy mass table has no stable-isotope weight for atomic number {atomic_number} (`{species}`)"
    )]
    MissingAtomicWeight { atomic_number: u8, species: String },
    #[error("molecular weight must be positive after preprocessing")]
    ZeroMolecularWeight,
    #[error(
        "subset atom index {atom_index} at position {subset_position} is outside 1..={atom_count}"
    )]
    InvalidSubsetAtomIndex {
        subset_position: usize,
        atom_index: usize,
        atom_count: usize,
    },
    #[error("failed to parse SYVA fixture input: {0}")]
    InvalidFixtureInput(String),
    #[error("failed to parse SYVA fixture output: {0}")]
    InvalidFixtureOutput(String),
    #[error("missing symmetry-element geometry for operation `{label}` during symmetrization")]
    MissingSymmetryElementForOperation { label: String },
    #[error(
        "symmetrization of `{requested_point_group}` did not verify successfully (optimized_atoms={optimized_atom_count}, recovered_point_group={recovered_point_group:?})"
    )]
    SymmetrizationVerificationFailed {
        requested_point_group: String,
        optimized_atom_count: usize,
        recovered_point_group: Option<String>,
    },
}
