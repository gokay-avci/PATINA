use crate::conversion::{ConversionKind, ConversionOutcome};
use crate::structure::{
    Cluster0D, CoordinateBasis, Framework3D, Lattice3, PeriodicAxes, RawStructure, Site, Slab2D,
    StructureError, StructureMetadata, Wire1D,
};
use patina_types::Candidate;

fn metadata_from_candidate(candidate: &Candidate) -> StructureMetadata {
    StructureMetadata {
        label: candidate.label.clone(),
    }
}

fn sites_from_candidate(candidate: &Candidate) -> Vec<Site> {
    candidate
        .species
        .iter()
        .cloned()
        .zip(candidate.fractional_coords.iter().copied())
        .map(|(species, coords)| Site { species, coords })
        .collect()
}

fn lattice_from_candidate(candidate: &Candidate) -> Option<Lattice3> {
    candidate.lattice.map(Lattice3::new)
}

impl TryFrom<&Candidate> for RawStructure {
    type Error = StructureError;

    fn try_from(candidate: &Candidate) -> Result<Self, Self::Error> {
        RawStructure {
            metadata: metadata_from_candidate(candidate),
            sites: sites_from_candidate(candidate),
            coordinate_basis: CoordinateBasis::Fractional,
            lattice: lattice_from_candidate(candidate),
            periodic_axes: PeriodicAxes::new(candidate.periodic_axes),
        }
        .validate()
    }
}

impl TryFrom<&Candidate> for Cluster0D {
    type Error = StructureError;

    fn try_from(candidate: &Candidate) -> Result<Self, Self::Error> {
        let actual_axes = PeriodicAxes::new(candidate.periodic_axes);
        if actual_axes != PeriodicAxes::NONE {
            return Err(StructureError::UnexpectedPeriodicAxes {
                expected: PeriodicAxes::NONE,
                actual: actual_axes,
            });
        }
        Cluster0D::new(
            metadata_from_candidate(candidate),
            sites_from_candidate(candidate),
            CoordinateBasis::Cartesian,
            candidate.lattice.map(Lattice3::new),
        )
    }
}

impl TryFrom<&Candidate> for Wire1D {
    type Error = StructureError;

    fn try_from(candidate: &Candidate) -> Result<Self, Self::Error> {
        let actual_axes = PeriodicAxes::new(candidate.periodic_axes);
        if actual_axes.count() != 1 {
            return Err(StructureError::UnexpectedPeriodicDimensionCount {
                expected: 1,
                actual: actual_axes.count(),
                periodic_axes: actual_axes,
            });
        }
        Wire1D::new(
            metadata_from_candidate(candidate),
            sites_from_candidate(candidate),
            CoordinateBasis::Fractional,
            lattice_from_candidate(candidate),
            actual_axes,
        )
    }
}

impl TryFrom<&Candidate> for Slab2D {
    type Error = StructureError;

    fn try_from(candidate: &Candidate) -> Result<Self, Self::Error> {
        let actual_axes = PeriodicAxes::new(candidate.periodic_axes);
        if actual_axes.count() != 2 {
            return Err(StructureError::UnexpectedPeriodicDimensionCount {
                expected: 2,
                actual: actual_axes.count(),
                periodic_axes: actual_axes,
            });
        }
        Slab2D::new(
            metadata_from_candidate(candidate),
            sites_from_candidate(candidate),
            CoordinateBasis::Fractional,
            lattice_from_candidate(candidate),
            actual_axes,
        )
    }
}

impl TryFrom<&Candidate> for Framework3D {
    type Error = StructureError;

    fn try_from(candidate: &Candidate) -> Result<Self, Self::Error> {
        let actual_axes = PeriodicAxes::new(candidate.periodic_axes);
        if actual_axes != PeriodicAxes::XYZ {
            return Err(StructureError::UnexpectedPeriodicAxes {
                expected: PeriodicAxes::XYZ,
                actual: actual_axes,
            });
        }
        Framework3D::new(
            metadata_from_candidate(candidate),
            sites_from_candidate(candidate),
            CoordinateBasis::Fractional,
            lattice_from_candidate(candidate),
        )
    }
}

impl From<&Cluster0D> for Candidate {
    fn from(value: &Cluster0D) -> Self {
        Self::cluster(
            value.metadata.label.clone(),
            value
                .sites
                .iter()
                .map(|site| site.species.clone())
                .collect(),
            value.sites.iter().map(|site| site.coords).collect(),
        )
    }
}

impl From<&Slab2D> for Candidate {
    fn from(value: &Slab2D) -> Self {
        Self::periodic(
            value.metadata.label.clone(),
            value
                .sites
                .iter()
                .map(|site| site.species.clone())
                .collect(),
            value.sites.iter().map(|site| site.coords).collect(),
            value
                .lattice
                .expect("validated slab must have a lattice")
                .basis,
            value.periodic_axes.axes,
        )
    }
}

impl From<&Framework3D> for Candidate {
    fn from(value: &Framework3D) -> Self {
        Self::fully_periodic(
            value.metadata.label.clone(),
            value
                .sites
                .iter()
                .map(|site| site.species.clone())
                .collect(),
            value.sites.iter().map(|site| site.coords).collect(),
            value
                .lattice
                .expect("validated framework must have a lattice")
                .basis,
        )
    }
}

pub fn raw_structure_from_candidate(
    candidate: &Candidate,
) -> Result<ConversionOutcome<RawStructure>, StructureError> {
    Ok(ConversionOutcome::new(
        ConversionKind::Bridge,
        RawStructure::try_from(candidate)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cluster_candidate() -> Candidate {
        Candidate::cluster("cluster", vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    #[test]
    fn cluster_bridge_rejects_periodic_axes() {
        let mut candidate = cluster_candidate();
        candidate.lattice = Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]]);
        candidate.periodic_axes = [true, true, true];
        let error = Cluster0D::try_from(&candidate).expect_err("must reject periodic candidate");
        assert_eq!(
            error,
            StructureError::UnexpectedPeriodicAxes {
                expected: PeriodicAxes::NONE,
                actual: PeriodicAxes::XYZ,
            }
        );
    }
}
