use super::common::{atom, base_parameters, bond, decorate_sites, motif};
use crate::domain::{Composition, MotifCandidate, TopologyResult};
use serde_json::json;

#[derive(Debug, Clone, Copy)]
pub struct WireGenerator {
    pub bond_length: f64,
    pub zigzag_amplitude: f64,
}

impl WireGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self {
            bond_length,
            zigzag_amplitude: 0.25 * bond_length,
        }
    }

    pub fn generate(
        self,
        index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<MotifCandidate> {
        let n = composition.total_atoms;
        let labels = decorate_sites(&composition);
        let mut positions = Vec::with_capacity(n);
        for i in 0..n {
            let y = if i % 2 == 0 {
                self.zigzag_amplitude
            } else {
                -self.zigzag_amplitude
            };
            positions.push([i as f64 * self.bond_length, y, 0.0]);
        }
        let atoms = labels
            .into_iter()
            .enumerate()
            .map(|(i, element)| atom(i, element, positions[i], "wire_site"))
            .collect::<Vec<_>>();
        let bonds = if n >= 2 {
            (0..n - 1)
                .map(|i| bond(i, i + 1, &positions, "wire"))
                .collect()
        } else {
            Vec::new()
        };
        let mut parameters = base_parameters(self.bond_length);
        parameters.insert("length".to_string(), json!(n));
        parameters.insert("wire_mode".to_string(), json!("zigzag"));
        Ok(motif(
            index,
            "wire",
            composition,
            atoms,
            bonds,
            seed,
            parameters,
        ))
    }
}
