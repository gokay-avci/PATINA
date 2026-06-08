use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologySignature {
    pub schema_version: String,
    pub formula: String,
    pub n_atoms: usize,
    pub graph_basic: GraphBasicSignature,
    pub rings: RingSignature,
    pub spectra: Option<SpectralSignature>,
    pub hashes: HashSignature,
    pub geometry: GeometrySignature,
    pub coordination: CoordinationSignature,
    pub symmetry: Option<SymmetrySignature>,
    pub jaccard_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphBasicSignature {
    pub n_nodes: usize,
    pub n_edges: usize,
    pub connected_components: usize,
    pub cycle_rank: isize,
    pub degree_histogram: BTreeMap<usize, usize>,
    pub degree_histogram_by_element: BTreeMap<String, BTreeMap<usize, usize>>,
    pub edge_type_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RingSignature {
    pub cycle_basis_lengths: Vec<usize>,
    pub ring_size_distribution: BTreeMap<usize, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralSignature {
    pub adjacency_spectrum: Vec<f64>,
    pub laplacian_spectrum: Vec<f64>,
    pub adjacency_spectrum_hash: String,
    pub laplacian_spectrum_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashSignature {
    pub fast_hash: String,
    pub wl_hash: String,
    pub canonical_graph_hash: Option<String>,
    pub geometry_distance_hash: String,
    pub full_signature_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometrySignature {
    pub centroid: [f64; 3],
    pub radius_of_gyration: f64,
    pub principal_moments: [f64; 3],
    pub asphericity: f64,
    pub acylindricity: f64,
    pub pair_distance_histogram: BTreeMap<String, usize>,
    pub morphology_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationSignature {
    pub by_element: BTreeMap<String, BTreeMap<usize, usize>>,
    pub sequence_by_node: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymmetrySignature {
    pub point_group: Option<String>,
    pub backend: Option<String>,
    pub tolerance: Option<f64>,
    pub max_deviation: Option<f64>,
    pub operation_count: Option<usize>,
    pub permutation_count: Option<usize>,
    pub equivalence_classes: Vec<Vec<usize>>,
    pub is_linear: Option<bool>,
    pub is_planar: Option<bool>,
    pub status: String,
    pub message: Option<String>,
}
