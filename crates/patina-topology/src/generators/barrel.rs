use super::common::{atom, base_parameters, bond, decorate_sites, motif};
use crate::domain::{Composition, MotifCandidate, TopologyResult};
use serde_json::json;
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy)]
pub struct BarrelGenerator {
    pub bond_length: f64,
    pub interlayer_spacing: f64,
    pub twist_angle: f64,
}

impl BarrelGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self {
            bond_length,
            interlayer_spacing: bond_length,
            twist_angle: 0.0,
        }
    }

    pub fn generate_family(
        self,
        start_index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<Vec<MotifCandidate>> {
        let n = composition.total_atoms;
        let mut out = Vec::new();
        for ring_size in 3..=n {
            if n.is_multiple_of(ring_size) {
                let layers = n / ring_size;
                if layers >= 2 {
                    out.push(self.generate(
                        start_index + out.len(),
                        composition.clone(),
                        seed,
                        ring_size,
                        layers,
                    )?);
                }
            }
        }
        Ok(out)
    }

    pub fn generate(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
        ring_size: usize,
        layers: usize,
    ) -> TopologyResult<MotifCandidate> {
        let labels = decorate_sites(&composition);
        let radius = self.bond_length / (2.0 * (std::f64::consts::PI / ring_size as f64).sin());
        let mut positions = Vec::with_capacity(labels.len());
        for layer in 0..layers {
            let z = (layer as f64 - (layers - 1) as f64 / 2.0) * self.interlayer_spacing;
            let twist = self.twist_angle * layer as f64;
            for site in 0..ring_size {
                let theta = TAU * site as f64 / ring_size as f64 + twist;
                positions.push([radius * theta.cos(), radius * theta.sin(), z]);
            }
        }
        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| {
                let layer = i / ring_size;
                let role = if layer == 0 || layer + 1 == layers {
                    "barrel_rim_site"
                } else {
                    "barrel_wall_site"
                };
                atom(i, element, positions[i], role)
            })
            .collect::<Vec<_>>();
        let mut bonds = Vec::new();
        for layer in 0..layers {
            let offset = layer * ring_size;
            for site in 0..ring_size {
                bonds.push(bond(
                    offset + site,
                    offset + (site + 1) % ring_size,
                    &positions,
                    "barrel_ring",
                ));
            }
        }
        for layer in 0..layers - 1 {
            let lower = layer * ring_size;
            let upper = (layer + 1) * ring_size;
            for site in 0..ring_size {
                bonds.push(bond(lower + site, upper + site, &positions, "barrel_axial"));
            }
        }
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("ring_size".to_string(), json!(ring_size));
        parameters.insert("layers".to_string(), json!(layers));
        parameters.insert("twist_angle".to_string(), json!(self.twist_angle));
        parameters.insert("cap_mode".to_string(), json!("none"));
        Ok(motif(
            index,
            "barrel",
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        ))
    }
}
