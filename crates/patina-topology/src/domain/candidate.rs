use crate::domain::{Composition, ElementSymbol, TopologyError, TopologyResult, TopologySignature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CandidateId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorProvenance {
    pub name: String,
    pub version: String,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotifCandidate {
    pub id: CandidateId,
    pub generator: GeneratorProvenance,
    #[serde(default)]
    pub provenance: CandidateProvenance,
    pub composition: Composition,
    pub atoms: Vec<AtomSite>,
    pub bonds: Vec<Bond>,
    pub parameters: BTreeMap<String, serde_json::Value>,
    pub topology_signature: Option<TopologySignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateProvenance {
    pub campaign_id: String,
    pub creation_stage: String,
    pub parent_candidate_ids: Vec<CandidateId>,
    pub checklist_evidence: Vec<String>,
}

impl Default for CandidateProvenance {
    fn default() -> Self {
        Self {
            campaign_id: "patina-topology-library".to_string(),
            creation_stage: "generate".to_string(),
            parent_candidate_ids: Vec::new(),
            checklist_evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomSite {
    pub index: usize,
    pub element: ElementSymbol,
    pub position: [f64; 3],
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    pub i: usize,
    pub j: usize,
    pub distance: f64,
    pub bond_class: Option<String>,
}

impl MotifCandidate {
    pub fn validate(&self) -> TopologyResult<()> {
        for (expected, atom) in self.atoms.iter().enumerate() {
            if atom.index != expected {
                return Err(TopologyError::InvalidBondIndex {
                    i: atom.index,
                    j: expected,
                    n_atoms: self.atoms.len(),
                });
            }
            if atom.position.iter().any(|value| !value.is_finite()) {
                return Err(TopologyError::Io {
                    message: format!("candidate `{}` contains non-finite coordinate", self.id.0),
                });
            }
        }
        for bond in &self.bonds {
            if bond.i >= self.atoms.len() || bond.j >= self.atoms.len() || bond.i == bond.j {
                return Err(TopologyError::InvalidBondIndex {
                    i: bond.i,
                    j: bond.j,
                    n_atoms: self.atoms.len(),
                });
            }
        }
        Ok(())
    }
}
