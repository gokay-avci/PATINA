#![forbid(unsafe_code)]

/*!
What this crate implements: the Scott-native evaluator procedure boundary that sits above concrete
backend adapters such as GULP and JANUS_MACE.
Design basis: the native Fortran code couples template handling, stage progression, retry rules,
status decoding, retrieval timing, and rollback semantics into one scientific procedure. This crate
freezes that procedure contract in Rust before the full implementation is migrated.
Assumption: the first pass establishes compile-time boundaries and typed procedure vocabulary
without yet replacing the working application-layer orchestration.
*/

mod plan;
mod procedure;
mod runtime;
mod stages;
mod status;
mod template;

pub use plan::{
    NativeScottProcedureConfig, NativeScottProcedureParseError, ScottBackendMode,
    ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode, ScottProcedureIntent,
    ScottProcedurePlan,
};
pub use procedure::{
    ProcedureState, ScottEvaluator, ScottEvaluatorError, ScottEvaluatorProcedure,
    ScottProcedureRequest,
};
pub use runtime::{ProcedureAction, ProcedureCursor, ProcedureRuntimeState, StageTicket};
pub use stages::{
    FinalStageFailurePolicy, StageEngine, StageIndex, StagePlan, StageSelection,
    StageSelectionError, StageTransition,
};
pub use status::{
    normalize_backend_attempt, BackendAttemptReport, NormalizedConvergence, ProcedureDecision,
    ScottDuplicateAction, ScottDuplicateProvenance, ScottDuplicateReason, ScottEvalOutcome,
    ScottEvalOutcomeConsistencyError, ScottEvalProvenance, ScottEvalRollback, ScottEvaluationState,
    ScottProcedureGateEvidence, ScottProcedureGateKind, ScottProcedureGateOutcome,
    ScottScienceEvidence, ScottStageEvaluation, ScottStageFailure, ScottStageStatus,
    ScottStateDigest, ScottTopologyProvenance, ScottValidityCheck, ScottValidityCheckKind,
    StageBackendStatus,
};
pub use template::{
    summarize_master_template, MasterTemplateLayout, MasterTemplateParseError,
    MasterTemplateSection, MasterTemplateSummary,
};
