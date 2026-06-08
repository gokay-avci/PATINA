use super::common::{atom, base_parameters, bond, decorate_sites, motif};
use crate::domain::{Composition, MotifCandidate, TopologyError, TopologyResult};
use serde_json::json;
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy)]
pub struct RingGenerator {
    pub bond_length: f64,
    pub buckling_amplitude: f64,
}

impl RingGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self {
            bond_length,
            buckling_amplitude: 0.0,
        }
    }

    pub fn generate(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let n = composition.total_atoms;
        if n < 3 {
            return Err(TopologyError::TooFewAtoms {
                minimum: 3,
                actual: n,
            });
        }
        let radius = self.bond_length / (2.0 * (std::f64::consts::PI / n as f64).sin());
        let labels = decorate_sites(&composition);
        let mut positions = Vec::with_capacity(n);
        for i in 0..n {
            let theta = TAU * i as f64 / n as f64;
            let z = if self.buckling_amplitude == 0.0 {
                0.0
            } else {
                self.buckling_amplitude * if i % 2 == 0 { 1.0 } else { -1.0 }
            };
            positions.push([radius * theta.cos(), radius * theta.sin(), z]);
        }
        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| atom(i, element, positions[i], "ring_site"))
            .collect::<Vec<_>>();
        let bonds = (0..n)
            .map(|i| bond(i, (i + 1) % n, &positions, "ring"))
            .collect::<Vec<_>>();
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("ring_size".to_string(), json!(n));
        parameters.insert("decoration".to_string(), json!("round_robin_formula_order"));
        Ok(motif(
            index,
            "ring",
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        ))
    }
}
