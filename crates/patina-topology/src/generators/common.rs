use crate::domain::{
    AtomSite, Bond, CandidateId, CandidateProvenance, Composition, ElementSymbol,
    GeneratorProvenance, MotifCandidate,
};
use serde_json::json;
use std::collections::BTreeMap;

pub fn decorate_sites(composition: &Composition) -> Vec<ElementSymbol> {
    composition.ordered_atom_labels()
}

pub fn candidate_id(index: usize) -> CandidateId {
    CandidateId(format!("candidate_{index:09}"))
}

pub fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

pub fn atom(index: usize, element: ElementSymbol, position: [f64; 3], role: &str) -> AtomSite {
    AtomSite {
        index,
        element,
        position,
        role: Some(role.to_string()),
    }
}

pub fn bond(i: usize, j: usize, positions: &[[f64; 3]], bond_class: &str) -> Bond {
    Bond {
        i,
        j,
        distance: distance(positions[i], positions[j]),
        bond_class: Some(bond_class.to_string()),
    }
}

pub fn motif(
    index: usize,
    generator_name: &str,
    composition: Composition,
    atoms: Vec<AtomSite>,
    bonds: Vec<Bond>,
    seed: Option<u64>,
    parameters: BTreeMap<String, serde_json::Value>,
) -> MotifCandidate {
    MotifCandidate {
        id: candidate_id(index),
        generator: GeneratorProvenance {
            name: generator_name.to_string(),
            version: "phase1".to_string(),
            seed,
        },
        provenance: CandidateProvenance {
            campaign_id: "patina-topology-library".to_string(),
            creation_stage: "generate".to_string(),
            parent_candidate_ids: Vec::new(),
            checklist_evidence: vec![
                format!("formula={}", composition.total_formula()),
                format!("generator={generator_name}"),
                "graph_bonds_constructed_explicitly".to_string(),
                "no_energy_backend_invoked".to_string(),
            ],
        },
        composition,
        atoms,
        bonds,
        parameters,
        topology_signature: None,
    }
}

pub fn base_parameters(bond_length: f64) -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([("bond_length".to_string(), json!(bond_length))])
}
