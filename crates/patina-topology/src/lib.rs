pub mod cli;
pub mod domain;
pub mod fingerprints;
pub mod generators;
pub mod io;
pub mod ports;

pub use domain::{
    estimate_bond_length_from_formula, validate_geometry, ArtifactRecord, AtomSite, Bond,
    BondLengthEstimate, BondLengthMode, CandidateId, CandidateProvenance, ChecklistItem,
    CheckpointChecklist, CheckpointStatus, Composition, ElementSymbol, FormulaUnit,
    GeneratorProvenance, GeometryValidationConfig, MotifCandidate, RunCheckpoint, RunManifest,
    TopologyError, TopologyResult, TopologySignature, ValidationIssue, ValidationReport,
    ValidationSeverity,
};
pub use fingerprints::{
    fingerprint_candidate, fingerprint_candidate_with_symmetry, group_by_jaccard,
    jaccard_similarity, SignatureRecord,
};
pub use generators::{
    generate_candidates, BarrelGenerator, ChemicalEdgePolicy, ConstrainedRandomGraphGenerator,
    ConstrainedRandomReport, DegreeBounds, GenerationConstraints, PlatonicGenerator,
    RandomGraphGenerator, RingGenerator, ShellGenerator, TopologyMonteCarloGenerator,
    TopologyMonteCarloReport, WireGenerator,
};
pub use ports::{PointSymmetryBackend, SyvaPointSymmetryBackend, SyvaSymmetrizationRecord};
