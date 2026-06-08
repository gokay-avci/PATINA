#![forbid(unsafe_code)]

mod analysis;
mod cut;
mod domain;
mod engine;
mod generation;
mod graph;
mod math;
mod reduction;

pub use domain::{
    CanonicalEnsembleProtocol, DedupConfig, DedupReport, DipoleCancellationPolicy,
    ExternalPointChargeCompensation, GrandCanonicalEnsembleProtocol, IonicMoveKind,
    IonicReconstructionCandidate, IonicReconstructionConfig, IonicReconstructionEngine,
    IonicReconstructionRequest, IonicReconstructionResult, IonicSiteModification, MillerIndex,
    PaperLikeSurfaceProtocol, SlabReductionConfig, StochasticSamplingProtocol, SurfaceAtom,
    SurfaceBondDiagnosticsSummary, SurfaceCutStrategy, SurfaceDanglingBondCandidate,
    SurfaceDiagnosticsDataset, SurfaceEvaluationBackendKind, SurfaceFace, SurfaceFrameworkAtom,
    SurfaceGenerationConfig, SurfaceGenerationEngine, SurfaceGenerationRequest,
    SurfaceGenerationResult, SurfaceInterfaceError, SurfaceOptimizerKind, SurfaceParentStructure,
    SurfacePolarityAnalyzer, SurfacePolarityClass, SurfacePolarityReport,
    SurfaceReconstructionMode, SurfaceRelaxationProtocol, SurfaceSiteSamplingRules, SurfaceSlab,
    SurfaceSupercellConfig, SurfaceTerminationBias, SurfaceTopologyDiagnostics,
    TwoRegionSurfaceModel,
};
pub use engine::{DefaultSurfaceGenerationEngine, HeuristicSurfacePolarityAnalyzer};

#[cfg(test)]
mod tests;
