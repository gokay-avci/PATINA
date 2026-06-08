use thiserror::Error;

#[derive(Debug, Error)]
pub enum RaspaInterfaceError {
    #[error("periodic framework conversion requires a lattice")]
    MissingLattice,
    #[error("periodic framework conversion requires at least one periodic axis")]
    NonPeriodicCandidate,
    #[error(
        "partial periodicity is not yet enabled for this RASPA-owned path; received periodic_axes={periodic_axes:?}"
    )]
    PartialPeriodicityUnsupported { periodic_axes: [bool; 3] },
    #[error("framework candidate validation failed: {0}")]
    InvalidCandidate(String),
    #[error("unsupported empty or blank species label in framework")]
    InvalidSpeciesLabel,
    #[error("density grid dimensions must all be > 0")]
    InvalidDensityGridDimensions,
    #[error("energy histogram requires at least one bin")]
    InvalidEnergyHistogramBins,
    #[error("energy histogram range must be finite and increasing")]
    InvalidEnergyHistogramRange,
    #[error("number histogram upper limit must be >= lower limit")]
    InvalidNumberHistogramRange,
    #[error("GCMC requires a guest template with at least one atom")]
    InvalidGuestTemplate,
    #[error("GCMC requires a finite positive temperature")]
    InvalidTemperature,
    #[error("GCMC requires either pressure/fugacity or chemical potential")]
    MissingGrandCanonicalControl,
    #[error("GCMC requires a positive pressure or fugacity when chemical potential is omitted")]
    InvalidPressure,
    #[error("GCMC maximum guest count must be > 0")]
    InvalidMaximumGuestCount,
    #[error("GCMC energy evaluation failed: {0}")]
    EnergyEvaluation(String),
    #[error("moyo symmetry analysis failed: {0}")]
    SymmetryBackend(String),
}
