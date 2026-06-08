use patina_arvo::{compute_surface_energy, ArvoError, SurfaceEnergyInput, SurfaceEnergySummary};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSurfaceEnergyRequest {
    pub total_energy: f64,
    pub atom_count: usize,
    pub bulk_energy_per_atom: f64,
    pub area: f64,
}

impl ClusterSurfaceEnergyRequest {
    pub fn to_arvo_input(&self) -> SurfaceEnergyInput {
        SurfaceEnergyInput {
            total_energy: self.total_energy,
            atom_count: self.atom_count,
            bulk_energy_per_atom: self.bulk_energy_per_atom,
            area: self.area,
        }
    }
}

pub fn compute_cluster_surface_energy(
    request: &ClusterSurfaceEnergyRequest,
) -> Result<SurfaceEnergySummary, ArvoError> {
    compute_surface_energy(&request.to_arvo_input())
}

#[cfg(test)]
mod tests {
    use super::{compute_cluster_surface_energy, ClusterSurfaceEnergyRequest};

    #[test]
    fn computes_surface_energy_from_scalar_geometry_inputs() {
        let summary = compute_cluster_surface_energy(&ClusterSurfaceEnergyRequest {
            total_energy: -100.0,
            atom_count: 10,
            bulk_energy_per_atom: -10.5,
            area: 25.0,
        })
        .expect("surface energy");

        assert_eq!(summary.excess_energy, 5.0);
        assert_eq!(summary.surface_energy, 0.2);
    }
}
