mod syva;

use crate::domain::{MotifCandidate, SymmetrySignature, TopologyResult};
use serde::{Deserialize, Serialize};

pub use syva::{SyvaPointSymmetryBackend, SyvaSymmetrizationRecord};

pub trait TopologyGenerator {
    fn generate(&self) -> TopologyResult<Vec<MotifCandidate>>;
}

pub trait TopologyFingerprinter {
    type Signature;

    fn fingerprint(&self, candidate: &MotifCandidate) -> TopologyResult<Self::Signature>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalGraphLabel {
    pub label: String,
    pub backend: String,
}

pub trait CanonicalGraphBackend {
    fn canonical_label(&self, graph: &LabelledGraphView<'_>)
        -> TopologyResult<CanonicalGraphLabel>;
    fn automorphism_group_order(
        &self,
        graph: &LabelledGraphView<'_>,
    ) -> TopologyResult<Option<String>>;
    fn orbits(&self, graph: &LabelledGraphView<'_>) -> TopologyResult<Option<Vec<Vec<usize>>>>;
}

pub trait PointSymmetryBackend {
    fn analyze(&self, candidate: &MotifCandidate) -> TopologyResult<SymmetrySignature>;
}

pub trait EmbeddingBackend {
    fn embed(&self, labels: &[String], edges: &[(usize, usize)]) -> TopologyResult<Vec<[f64; 3]>>;
}

pub trait CandidateStore {
    fn write_candidate(&mut self, candidate: &MotifCandidate) -> TopologyResult<()>;
}

#[derive(Debug, Clone, Copy)]
pub struct LabelledGraphView<'a> {
    pub labels: &'a [String],
    pub edges: &'a [(usize, usize)],
}
