use super::constrained_random::ConstrainedRandomGraphGenerator;
use crate::domain::{Composition, MotifCandidate, TopologyResult};
use crate::fingerprints::{fingerprint_candidate, jaccard_similarity};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyMonteCarloReport {
    pub proposals: usize,
    pub accepted: usize,
    pub rejected_duplicate_wl: usize,
    pub rejected_similarity: usize,
    pub min_jaccard_distance: f64,
}

#[derive(Debug, Clone)]
pub struct TopologyMonteCarloGenerator {
    pub bond_length: f64,
    pub constrained: ConstrainedRandomGraphGenerator,
    pub proposals: usize,
    pub max_accept: usize,
    pub min_jaccard_distance: f64,
}

impl TopologyMonteCarloGenerator {
    pub fn new(bond_length: f64) -> Self {
        Self {
            bond_length,
            constrained: ConstrainedRandomGraphGenerator::new(bond_length),
            proposals: 32,
            max_accept: 8,
            min_jaccard_distance: 0.20,
        }
    }

    pub fn generate_family(
        self,
        start_index: usize,
        composition: Composition,
        seed: Option<u64>,
    ) -> TopologyResult<Vec<MotifCandidate>> {
        let mut accepted = Vec::<MotifCandidate>::new();
        let mut wl_seen = BTreeSet::<String>::new();
        let mut report = TopologyMonteCarloReport {
            proposals: self.proposals,
            accepted: 0,
            rejected_duplicate_wl: 0,
            rejected_similarity: 0,
            min_jaccard_distance: self.min_jaccard_distance,
        };

        for proposal_index in 0..self.proposals {
            if accepted.len() >= self.max_accept {
                break;
            }
            let mut generator = self.constrained.clone();
            generator.target_extra_edges = proposal_index % composition.total_atoms.max(1);
            let mut candidate = generator.generate(
                start_index + accepted.len(),
                composition.clone(),
                seed.map(|value| value ^ ((proposal_index as u64) << 24)),
            )?;
            candidate.generator.name = "topology_mc".to_string();
            candidate
                .parameters
                .insert("mc_proposal_index".to_string(), json!(proposal_index));
            let signature = fingerprint_candidate(&candidate);
            if !wl_seen.insert(signature.hashes.wl_hash.clone()) {
                report.rejected_duplicate_wl += 1;
                continue;
            }
            let min_distance = accepted
                .iter()
                .filter_map(|existing| existing.topology_signature.as_ref())
                .map(|existing| {
                    1.0 - jaccard_similarity(
                        &existing.jaccard_features,
                        &signature.jaccard_features,
                    )
                })
                .fold(1.0_f64, f64::min);
            if !accepted.is_empty() && min_distance < self.min_jaccard_distance {
                report.rejected_similarity += 1;
                continue;
            }
            candidate.topology_signature = Some(signature);
            accepted.push(candidate);
        }
        report.accepted = accepted.len();
        for candidate in &mut accepted {
            candidate.parameters.insert(
                "topology_mc_report".to_string(),
                serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
            );
        }
        Ok(accepted)
    }
}
