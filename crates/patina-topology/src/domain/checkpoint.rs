use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    Planned,
    Running,
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub description: String,
    pub status: CheckpointStatus,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointChecklist {
    pub name: String,
    pub items: Vec<ChecklistItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub path: String,
    pub kind: String,
    pub role: String,
    pub record_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunManifest {
    pub schema_version: String,
    pub campaign_id: String,
    pub command: String,
    pub cwd: String,
    pub created_unix_seconds: u64,
    pub inputs: BTreeMap<String, String>,
    pub artifacts: Vec<ArtifactRecord>,
    pub checklist: CheckpointChecklist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCheckpoint {
    pub timestamp_unix_seconds: u64,
    pub stage: String,
    pub status: CheckpointStatus,
    pub message: String,
    pub artifacts: Vec<ArtifactRecord>,
}

impl CheckpointChecklist {
    pub fn topology_generation() -> Self {
        Self {
            name: "topology_generation".to_string(),
            items: vec![
                item("formula_parsed", "Formula parsed and scaled exactly"),
                item("generators_selected", "Requested generators resolved"),
                item(
                    "graph_validated",
                    "Generated bonds reference valid atom indices",
                ),
                item("jsonl_written", "Candidate JSONL artifact written"),
                item(
                    "no_energy_dependency",
                    "No relaxation or energy backend invoked",
                ),
            ],
        }
    }

    pub fn fingerprinting() -> Self {
        Self {
            name: "fingerprinting".to_string(),
            items: vec![
                item("input_loaded", "Candidate JSONL loaded"),
                item(
                    "graph_signature",
                    "Graph/ring/coordination signatures computed",
                ),
                item(
                    "hash_cascade",
                    "Fast, WL, geometry, and full hashes recorded",
                ),
                item(
                    "symmetry_backend",
                    "Optional point-symmetry backend recorded",
                ),
                item("jsonl_written", "Signature JSONL artifact written"),
                item(
                    "uniqueness_guardrail",
                    "Approximate hashes not labelled proof-grade",
                ),
            ],
        }
    }

    pub fn grouping() -> Self {
        Self {
            name: "grouping".to_string(),
            items: vec![
                item("input_loaded", "Signature JSONL loaded"),
                item("similarity_defined", "Jaccard threshold recorded"),
                item("components_written", "Motif-family groups written"),
            ],
        }
    }

    pub fn preview() -> Self {
        Self {
            name: "preview".to_string(),
            items: vec![
                item("candidate_projection", "2D motif gallery written"),
                item("signature_summary", "Signature summary figure written"),
                item("symmetry_summary", "Point-symmetry summary figure written"),
                item("validation_summary", "Validation/rejection figure written"),
                item(
                    "generator_morphology_matrix",
                    "Generator-to-morphology matrix written",
                ),
                item("topology_metrics", "Topology metric map written"),
                item("similarity_graph", "Jaccard graph DOT written"),
                item("atlas_written", "Browsable topology atlas HTML written"),
            ],
        }
    }

    pub fn mark(mut self, id: &str, status: CheckpointStatus, evidence: impl Into<String>) -> Self {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.status = status;
            item.evidence.push(evidence.into());
        }
        self
    }
}

fn item(id: &str, description: &str) -> ChecklistItem {
    ChecklistItem {
        id: id.to_string(),
        description: description.to_string(),
        status: CheckpointStatus::Planned,
        evidence: Vec::new(),
    }
}
