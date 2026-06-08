#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ArvoError {
    #[error("sphere cloud must contain at least one sphere")]
    EmptySphereCloud,
    #[error("sphere {index} has invalid radius {radius}")]
    InvalidRadius { index: usize, radius: f64 },
    #[error("sphere {index} has non-finite center {center:?}")]
    NonFiniteCenter { index: usize, center: [f64; 3] },
    #[error("surface area must be positive and finite, received {0}")]
    InvalidArea(f64),
    #[error("atom count must be greater than zero")]
    EmptySystem,
    #[error("energy inputs must be finite")]
    NonFiniteEnergy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sphere {
    pub center: [f64; 3],
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SphereCloud {
    pub spheres: Vec<Sphere>,
}

impl SphereCloud {
    pub fn validate(&self) -> Result<(), ArvoError> {
        if self.spheres.is_empty() {
            return Err(ArvoError::EmptySphereCloud);
        }

        for (index, sphere) in self.spheres.iter().enumerate() {
            if !sphere.radius.is_finite() || sphere.radius <= 0.0 {
                return Err(ArvoError::InvalidRadius {
                    index,
                    radius: sphere.radius,
                });
            }
            if sphere.center.iter().any(|value| !value.is_finite()) {
                return Err(ArvoError::NonFiniteCenter {
                    index,
                    center: sphere.center,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGeometrySummary {
    pub volume: f64,
    pub area: f64,
    pub sphere_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceEnergyInput {
    pub total_energy: f64,
    pub atom_count: usize,
    pub bulk_energy_per_atom: f64,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceEnergySummary {
    pub area: f64,
    pub excess_energy: f64,
    pub surface_energy: f64,
}

pub fn compute_surface_energy(
    input: &SurfaceEnergyInput,
) -> Result<SurfaceEnergySummary, ArvoError> {
    if input.atom_count == 0 {
        return Err(ArvoError::EmptySystem);
    }
    if !input.total_energy.is_finite() || !input.bulk_energy_per_atom.is_finite() {
        return Err(ArvoError::NonFiniteEnergy);
    }
    if !input.area.is_finite() || input.area <= 0.0 {
        return Err(ArvoError::InvalidArea(input.area));
    }

    let excess_energy = input.total_energy - input.atom_count as f64 * input.bulk_energy_per_atom;

    Ok(SurfaceEnergySummary {
        area: input.area,
        excess_energy,
        surface_energy: excess_energy / input.area,
    })
}

#[cfg(test)]
mod tests {
    use super::{compute_surface_energy, ArvoError, Sphere, SphereCloud, SurfaceEnergyInput};

    #[test]
    fn validates_sphere_cloud() {
        let cloud = SphereCloud {
            spheres: vec![Sphere {
                center: [0.0, 0.0, 0.0],
                radius: 1.5,
            }],
        };

        cloud.validate().expect("sphere cloud valid");
    }

    #[test]
    fn computes_surface_energy_summary() {
        let summary = compute_surface_energy(&SurfaceEnergyInput {
            total_energy: -100.0,
            atom_count: 10,
            bulk_energy_per_atom: -10.2,
            area: 20.0,
        })
        .expect("surface energy");

        assert_eq!(summary.excess_energy, 2.0);
        assert_eq!(summary.surface_energy, 0.1);
    }

    #[test]
    fn rejects_non_positive_area() {
        let error = compute_surface_energy(&SurfaceEnergyInput {
            total_energy: -1.0,
            atom_count: 1,
            bulk_energy_per_atom: -1.0,
            area: 0.0,
        })
        .expect_err("invalid area");

        assert_eq!(error, ArvoError::InvalidArea(0.0));
    }
}
