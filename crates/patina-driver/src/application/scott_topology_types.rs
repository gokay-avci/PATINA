use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct TopologyNearnessReport {
    pub metadata: TopologyNearnessMetadata,
    pub summary: TopologyNearnessSummary,
    pub comparisons: Vec<TopologyNearnessComparison>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyNearnessMetadata {
    pub radius_mode: String,
    pub radius_const: f64,
    pub species_order: Vec<String>,
    pub near_edge_threshold: u32,
    pub near_coordination_threshold: u32,
    pub sources: TopologyNearnessSources,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyNearnessSources {
    pub audit_events: Option<String>,
    pub top_structures_hashkeys: Option<String>,
    pub ga_statistics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TopologyNearnessSummary {
    pub comparisons_total: usize,
    pub verdict_counts: BTreeMap<String, usize>,
    pub source_counts: BTreeMap<String, usize>,
    pub detail_counts: BTreeMap<String, usize>,
    pub unresolved_pairs: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyNearnessComparison {
    pub source: String,
    pub detail: String,
    pub left_id: String,
    pub right_id: String,
    pub left_hashkey: Option<String>,
    pub right_hashkey: Option<String>,
    pub exact_hashkey_match: Option<bool>,
    pub same_species_counts: Option<bool>,
    pub same_dimensionality: Option<bool>,
    pub left_structure: Option<String>,
    pub right_structure: Option<String>,
    pub left_dimensionality: Option<String>,
    pub right_dimensionality: Option<String>,
    pub left_radius: Option<f64>,
    pub right_radius: Option<f64>,
    pub edge_diff_total: Option<u32>,
    pub edge_diff_by_pair: BTreeMap<String, u32>,
    pub coordination_delta_total: Option<u32>,
    pub coordination_delta_by_species: BTreeMap<String, BTreeMap<u32, i32>>,
    pub verdict: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AtomSpecRecord {
    pub species: String,
    pub covalent_radius: f64,
    pub ionic_radius: f64,
}

#[derive(Debug, Clone)]
pub struct StructureAtom {
    pub species: String,
    pub coords: [f64; 3],
}

#[derive(Debug, Clone)]
pub struct StructureSummary {
    pub dimensionality: patina_types::StructureDimensionality,
    pub species_counts: BTreeMap<String, usize>,
    pub edge_counts_by_pair: BTreeMap<String, u32>,
    pub coordination_histograms: BTreeMap<String, BTreeMap<u32, u32>>,
    pub radius: f64,
}

#[derive(Debug, Clone)]
pub struct TopologyPairRequest {
    pub source: String,
    pub detail: String,
    pub left_id: String,
    pub right_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct TopologyEventRow {
    pub ga_iter: usize,
    pub event_kind: String,
    pub detail: String,
    pub left_id: String,
    pub right_id: String,
}
