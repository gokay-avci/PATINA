use serde::{Deserialize, Serialize};

/// Explicit declared dimensionality for structure semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureDimensionality {
    ZeroD,
    OneD,
    TwoD,
    ThreeD,
}

impl StructureDimensionality {
    pub fn count(self) -> usize {
        match self {
            Self::ZeroD => 0,
            Self::OneD => 1,
            Self::TwoD => 2,
            Self::ThreeD => 3,
        }
    }
}

/// Coordinate basis used by a structure payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateBasis {
    Cartesian,
    Fractional,
}

/// Periodic axes for lower-dimensional or fully periodic systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodicAxes {
    pub axes: [bool; 3],
}

impl PeriodicAxes {
    pub const NONE: Self = Self {
        axes: [false, false, false],
    };

    pub const X: Self = Self {
        axes: [true, false, false],
    };

    pub const XY: Self = Self {
        axes: [true, true, false],
    };

    pub const XYZ: Self = Self {
        axes: [true, true, true],
    };

    pub const fn new(axes: [bool; 3]) -> Self {
        Self { axes }
    }

    pub fn count(self) -> usize {
        self.axes.into_iter().filter(|enabled| *enabled).count()
    }

    pub fn dimensionality(self) -> StructureDimensionality {
        match self.count() {
            0 => StructureDimensionality::ZeroD,
            1 => StructureDimensionality::OneD,
            2 => StructureDimensionality::TwoD,
            3 => StructureDimensionality::ThreeD,
            _ => unreachable!("periodic axes always has length three"),
        }
    }

    pub fn is_non_periodic(self) -> bool {
        self == Self::NONE
    }
}

impl Default for PeriodicAxes {
    fn default() -> Self {
        Self::NONE
    }
}

/// Dense 3x3 lattice basis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lattice3 {
    pub basis: [[f64; 3]; 3],
}

impl Lattice3 {
    pub const fn new(basis: [[f64; 3]; 3]) -> Self {
        Self { basis }
    }
}

/// Atomic site with species label and coordinates in the structure's declared basis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub species: String,
    pub coords: [f64; 3],
}

/// Metadata shared by all structure payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StructureMetadata {
    pub label: String,
}

/// Broad structure family used by validated wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureType {
    Raw,
    Cluster0D,
    Wire1D,
    Slab2D,
    Framework3D,
}

/// Shared read-only structure contract for kernel types.
pub trait StructureLike {
    fn metadata(&self) -> &StructureMetadata;
    fn sites(&self) -> &[Site];
    fn coordinate_basis(&self) -> CoordinateBasis;
    fn periodic_axes(&self) -> PeriodicAxes;
    fn lattice(&self) -> Option<&Lattice3>;
    fn structure_type(&self) -> StructureType;

    fn label(&self) -> &str {
        &self.metadata().label
    }

    fn len(&self) -> usize {
        self.sites().len()
    }

    fn is_empty(&self) -> bool {
        self.sites().is_empty()
    }

    fn declared_dimensionality(&self) -> StructureDimensionality {
        self.periodic_axes().dimensionality()
    }
}

/// Unvalidated raw scientific structure decoded from a source representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawStructure {
    pub metadata: StructureMetadata,
    pub sites: Vec<Site>,
    pub coordinate_basis: CoordinateBasis,
    pub lattice: Option<Lattice3>,
    pub periodic_axes: PeriodicAxes,
}

impl RawStructure {
    pub fn validate(self) -> Result<Self, StructureError> {
        validate_sites(&self.sites)?;
        if self.lattice.is_none() && !self.periodic_axes.is_non_periodic() {
            return Err(StructureError::MissingLatticeForPeriodicAxes {
                periodic_axes: self.periodic_axes,
            });
        }
        Ok(self)
    }
}

impl StructureLike for RawStructure {
    fn metadata(&self) -> &StructureMetadata {
        &self.metadata
    }

    fn sites(&self) -> &[Site] {
        &self.sites
    }

    fn coordinate_basis(&self) -> CoordinateBasis {
        self.coordinate_basis
    }

    fn periodic_axes(&self) -> PeriodicAxes {
        self.periodic_axes
    }

    fn lattice(&self) -> Option<&Lattice3> {
        self.lattice.as_ref()
    }

    fn structure_type(&self) -> StructureType {
        StructureType::Raw
    }
}

macro_rules! define_structure_wrapper {
    ($name:ident, $kind:expr, $axes:expr, $needs_lattice:expr) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub metadata: StructureMetadata,
            pub sites: Vec<Site>,
            pub coordinate_basis: CoordinateBasis,
            pub lattice: Option<Lattice3>,
        }

        impl $name {
            pub fn new(
                metadata: StructureMetadata,
                sites: Vec<Site>,
                coordinate_basis: CoordinateBasis,
                lattice: Option<Lattice3>,
            ) -> Result<Self, StructureError> {
                validate_sites(&sites)?;
                if $needs_lattice && lattice.is_none() {
                    return Err(StructureError::MissingLatticeForPeriodicAxes {
                        periodic_axes: $axes,
                    });
                }
                if !$needs_lattice && lattice.is_some() {
                    return Err(StructureError::UnexpectedLatticeForNonPeriodicStructure);
                }
                Ok(Self {
                    metadata,
                    sites,
                    coordinate_basis,
                    lattice,
                })
            }
        }

        impl StructureLike for $name {
            fn metadata(&self) -> &StructureMetadata {
                &self.metadata
            }

            fn sites(&self) -> &[Site] {
                &self.sites
            }

            fn coordinate_basis(&self) -> CoordinateBasis {
                self.coordinate_basis
            }

            fn periodic_axes(&self) -> PeriodicAxes {
                $axes
            }

            fn lattice(&self) -> Option<&Lattice3> {
                self.lattice.as_ref()
            }

            fn structure_type(&self) -> StructureType {
                $kind
            }
        }
    };
}

define_structure_wrapper!(
    Cluster0D,
    StructureType::Cluster0D,
    PeriodicAxes::NONE,
    false
);
define_structure_wrapper!(
    Framework3D,
    StructureType::Framework3D,
    PeriodicAxes::XYZ,
    true
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wire1D {
    pub metadata: StructureMetadata,
    pub sites: Vec<Site>,
    pub coordinate_basis: CoordinateBasis,
    pub lattice: Option<Lattice3>,
    pub periodic_axes: PeriodicAxes,
}

impl Wire1D {
    pub fn new(
        metadata: StructureMetadata,
        sites: Vec<Site>,
        coordinate_basis: CoordinateBasis,
        lattice: Option<Lattice3>,
        periodic_axes: PeriodicAxes,
    ) -> Result<Self, StructureError> {
        validate_sites(&sites)?;
        if periodic_axes.count() != 1 {
            return Err(StructureError::UnexpectedPeriodicDimensionCount {
                expected: 1,
                actual: periodic_axes.count(),
                periodic_axes,
            });
        }
        if lattice.is_none() {
            return Err(StructureError::MissingLatticeForPeriodicAxes { periodic_axes });
        }
        Ok(Self {
            metadata,
            sites,
            coordinate_basis,
            lattice,
            periodic_axes,
        })
    }
}

impl StructureLike for Wire1D {
    fn metadata(&self) -> &StructureMetadata {
        &self.metadata
    }

    fn sites(&self) -> &[Site] {
        &self.sites
    }

    fn coordinate_basis(&self) -> CoordinateBasis {
        self.coordinate_basis
    }

    fn periodic_axes(&self) -> PeriodicAxes {
        self.periodic_axes
    }

    fn lattice(&self) -> Option<&Lattice3> {
        self.lattice.as_ref()
    }

    fn structure_type(&self) -> StructureType {
        StructureType::Wire1D
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slab2D {
    pub metadata: StructureMetadata,
    pub sites: Vec<Site>,
    pub coordinate_basis: CoordinateBasis,
    pub lattice: Option<Lattice3>,
    pub periodic_axes: PeriodicAxes,
}

impl Slab2D {
    pub fn new(
        metadata: StructureMetadata,
        sites: Vec<Site>,
        coordinate_basis: CoordinateBasis,
        lattice: Option<Lattice3>,
        periodic_axes: PeriodicAxes,
    ) -> Result<Self, StructureError> {
        validate_sites(&sites)?;
        if periodic_axes.count() != 2 {
            return Err(StructureError::UnexpectedPeriodicDimensionCount {
                expected: 2,
                actual: periodic_axes.count(),
                periodic_axes,
            });
        }
        if lattice.is_none() {
            return Err(StructureError::MissingLatticeForPeriodicAxes { periodic_axes });
        }
        Ok(Self {
            metadata,
            sites,
            coordinate_basis,
            lattice,
            periodic_axes,
        })
    }
}

impl StructureLike for Slab2D {
    fn metadata(&self) -> &StructureMetadata {
        &self.metadata
    }

    fn sites(&self) -> &[Site] {
        &self.sites
    }

    fn coordinate_basis(&self) -> CoordinateBasis {
        self.coordinate_basis
    }

    fn periodic_axes(&self) -> PeriodicAxes {
        self.periodic_axes
    }

    fn lattice(&self) -> Option<&Lattice3> {
        self.lattice.as_ref()
    }

    fn structure_type(&self) -> StructureType {
        StructureType::Slab2D
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureError {
    EmptySites,
    BlankSpeciesLabel,
    NonFiniteCoordinate {
        axis: usize,
    },
    UnexpectedPeriodicDimensionCount {
        expected: usize,
        actual: usize,
        periodic_axes: PeriodicAxes,
    },
    UnexpectedPeriodicAxes {
        expected: PeriodicAxes,
        actual: PeriodicAxes,
    },
    MissingLatticeForPeriodicAxes {
        periodic_axes: PeriodicAxes,
    },
    UnexpectedLatticeForNonPeriodicStructure,
}

fn validate_sites(sites: &[Site]) -> Result<(), StructureError> {
    if sites.is_empty() {
        return Err(StructureError::EmptySites);
    }
    for site in sites {
        if site.species.trim().is_empty() {
            return Err(StructureError::BlankSpeciesLabel);
        }
        for (axis, value) in site.coords.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(StructureError::NonFiniteCoordinate { axis });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Site {
        Site {
            species: "Mg".into(),
            coords: [0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn periodic_axes_reports_dimensionality() {
        assert_eq!(
            PeriodicAxes::NONE.dimensionality(),
            StructureDimensionality::ZeroD
        );
        assert_eq!(
            PeriodicAxes::X.dimensionality(),
            StructureDimensionality::OneD
        );
        assert_eq!(
            PeriodicAxes::XY.dimensionality(),
            StructureDimensionality::TwoD
        );
        assert_eq!(
            PeriodicAxes::XYZ.dimensionality(),
            StructureDimensionality::ThreeD
        );
    }

    #[test]
    fn cluster_rejects_lattice() {
        let result = Cluster0D::new(
            StructureMetadata {
                label: "cluster".into(),
            },
            vec![site()],
            CoordinateBasis::Cartesian,
            Some(Lattice3::new([
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ])),
        );
        assert_eq!(
            result,
            Err(StructureError::UnexpectedLatticeForNonPeriodicStructure)
        );
    }

    #[test]
    fn slab_requires_lattice() {
        let result = Slab2D::new(
            StructureMetadata {
                label: "slab".into(),
            },
            vec![site()],
            CoordinateBasis::Fractional,
            None,
            PeriodicAxes::XY,
        );
        assert_eq!(
            result,
            Err(StructureError::MissingLatticeForPeriodicAxes {
                periodic_axes: PeriodicAxes::XY
            })
        );
    }

    #[test]
    fn raw_structure_rejects_periodicity_without_lattice() {
        let raw = RawStructure {
            metadata: StructureMetadata::default(),
            sites: vec![site()],
            coordinate_basis: CoordinateBasis::Cartesian,
            lattice: None,
            periodic_axes: PeriodicAxes::XYZ,
        };
        assert_eq!(
            raw.validate(),
            Err(StructureError::MissingLatticeForPeriodicAxes {
                periodic_axes: PeriodicAxes::XYZ
            })
        );
    }

    #[test]
    fn slab_accepts_non_xy_two_d_periodicity() {
        let slab = Slab2D::new(
            StructureMetadata {
                label: "slab_yz".into(),
            },
            vec![site()],
            CoordinateBasis::Fractional,
            Some(Lattice3::new([
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ])),
            PeriodicAxes::new([false, true, true]),
        )
        .expect("2D slab");
        assert_eq!(slab.periodic_axes, PeriodicAxes::new([false, true, true]));
    }
}
