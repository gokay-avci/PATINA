use serde::{Deserialize, Serialize};

/// Explicit declared dimensionality for cluster or periodic structure pathways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureDimensionality {
    ZeroD,
    OneD,
    TwoD,
    ThreeD,
}

impl StructureDimensionality {
    /// Returns the number of active dimensions.
    pub fn count(self) -> usize {
        match self {
            Self::ZeroD => 0,
            Self::OneD => 1,
            Self::TwoD => 2,
            Self::ThreeD => 3,
        }
    }

    /// Returns `true` when the dimensionality is partial periodicity.
    pub fn is_partial_periodic(self) -> bool {
        matches!(self, Self::OneD | Self::TwoD)
    }
}

/// A crystal or cluster candidate passed to an evaluation backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Atomic species labels in structure order.
    pub species: Vec<String>,
    /// Fractional coordinates for each site.
    pub fractional_coords: Vec<[f64; 3]>,
    /// Optional lattice vectors. `None` indicates a non-periodic structure.
    pub lattice: Option<[[f64; 3]; 3]>,
    /// Periodic axes for lower-dimensional or fully periodic systems.
    pub periodic_axes: [bool; 3],
    /// Human-readable identifier for logging and result reporting.
    pub label: String,
}

impl Candidate {
    /// Builds a candidate from explicit structure parts.
    pub fn from_parts(
        label: impl Into<String>,
        species: Vec<String>,
        fractional_coords: Vec<[f64; 3]>,
        lattice: Option<[[f64; 3]; 3]>,
        periodic_axes: [bool; 3],
    ) -> Self {
        Self {
            species,
            fractional_coords,
            lattice,
            periodic_axes,
            label: label.into(),
        }
    }

    /// Builds a non-periodic cluster candidate.
    pub fn cluster(
        label: impl Into<String>,
        species: Vec<String>,
        fractional_coords: Vec<[f64; 3]>,
    ) -> Self {
        Self::from_parts(
            label,
            species,
            fractional_coords,
            None,
            [false, false, false],
        )
    }

    /// Builds a periodic candidate with explicit periodic axes.
    pub fn periodic(
        label: impl Into<String>,
        species: Vec<String>,
        fractional_coords: Vec<[f64; 3]>,
        lattice: [[f64; 3]; 3],
        periodic_axes: [bool; 3],
    ) -> Self {
        Self::from_parts(
            label,
            species,
            fractional_coords,
            Some(lattice),
            periodic_axes,
        )
    }

    /// Builds a fully periodic three-dimensional candidate.
    pub fn fully_periodic(
        label: impl Into<String>,
        species: Vec<String>,
        fractional_coords: Vec<[f64; 3]>,
        lattice: [[f64; 3]; 3],
    ) -> Self {
        Self::periodic(
            label,
            species,
            fractional_coords,
            lattice,
            [true, true, true],
        )
    }

    /// Returns the number of sites in the candidate.
    pub fn len(&self) -> usize {
        self.species.len()
    }

    /// Returns `true` when the candidate has no sites.
    pub fn is_empty(&self) -> bool {
        self.species.is_empty()
    }

    /// Returns the explicit dimensionality declared by periodic axes.
    pub fn declared_dimensionality(&self) -> StructureDimensionality {
        dimensionality_from_periodic_axes(self.periodic_axes)
    }

    /// Returns `true` when the candidate is non-periodic.
    pub fn is_zero_d(&self) -> bool {
        self.declared_dimensionality() == StructureDimensionality::ZeroD
    }

    /// Returns `true` when the candidate is fully periodic in all three axes.
    pub fn is_three_d_periodic(&self) -> bool {
        self.declared_dimensionality() == StructureDimensionality::ThreeD
    }

    /// Returns `true` when the candidate is lower-dimensional periodic.
    pub fn has_partial_periodicity(&self) -> bool {
        self.declared_dimensionality().is_partial_periodic()
    }

    /// Returns `true` when the candidate fits native Scott GA/BH search support.
    pub fn supports_native_scott_search(&self) -> bool {
        self.is_zero_d() || self.is_three_d_periodic()
    }

    /// Validates internal shape consistency.
    pub fn validate(&self) -> Result<(), CandidateError> {
        if self.species.len() != self.fractional_coords.len() {
            return Err(CandidateError::MismatchedSiteCounts {
                species: self.species.len(),
                coords: self.fractional_coords.len(),
            });
        }

        if self.species.is_empty() {
            return Err(CandidateError::Empty);
        }

        if self.lattice.is_none() && self.periodic_axes != [false, false, false] {
            return Err(CandidateError::PeriodicAxesWithoutLattice {
                periodic_axes: self.periodic_axes,
            });
        }

        Ok(())
    }
}

/// Compact structure payload used for continuity checkpoints and workflow boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureRecord {
    /// Human-readable identifier for logs, checkpoints, or UI.
    pub label: String,
    /// Atomic species labels in structure order.
    pub species: Vec<String>,
    /// Fractional coordinates for each site.
    pub fractional_coords: Vec<[f64; 3]>,
    /// Optional lattice vectors. `None` indicates a non-periodic structure.
    pub lattice: Option<[[f64; 3]; 3]>,
    /// Periodic axes for lower-dimensional or fully periodic systems.
    pub periodic_axes: [bool; 3],
}

impl StructureRecord {
    /// Returns the explicit dimensionality declared by periodic axes.
    pub fn declared_dimensionality(&self) -> StructureDimensionality {
        dimensionality_from_periodic_axes(self.periodic_axes)
    }
}

impl From<&Candidate> for StructureRecord {
    fn from(candidate: &Candidate) -> Self {
        Self {
            label: candidate.label.clone(),
            species: candidate.species.clone(),
            fractional_coords: candidate.fractional_coords.clone(),
            lattice: candidate.lattice,
            periodic_axes: candidate.periodic_axes,
        }
    }
}

impl From<&StructureRecord> for Candidate {
    fn from(record: &StructureRecord) -> Self {
        Self::from_parts(
            record.label.clone(),
            record.species.clone(),
            record.fractional_coords.clone(),
            record.lattice,
            record.periodic_axes,
        )
    }
}

fn dimensionality_from_periodic_axes(periodic_axes: [bool; 3]) -> StructureDimensionality {
    match periodic_axes.into_iter().filter(|enabled| *enabled).count() {
        0 => StructureDimensionality::ZeroD,
        1 => StructureDimensionality::OneD,
        2 => StructureDimensionality::TwoD,
        3 => StructureDimensionality::ThreeD,
        _ => unreachable!("periodic_axes always has length three"),
    }
}

/// Errors returned by [`Candidate::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateError {
    /// The candidate contains no atomic sites.
    Empty,
    /// The species and coordinate vectors differ in length.
    MismatchedSiteCounts { species: usize, coords: usize },
    /// Periodic axes were declared without a lattice.
    PeriodicAxesWithoutLattice { periodic_axes: [bool; 3] },
}
