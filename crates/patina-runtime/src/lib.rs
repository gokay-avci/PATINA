#![forbid(unsafe_code)]

/*!
What this crate implements: the Scott-owned orchestration contract that sits between
scientific procedure semantics and optional runtime substrates such as local Rayon execution
or future `unified_lab` scheduling.
Design basis: Scott must remain the owner of scientific workflow meaning, while launch,
polling, checkpointing, and monitoring remain pluggable.
Assumption: the first pass defines stable runtime envelopes and traits without forcing a
specific transport or async runtime.
*/

mod backend_bridge;
mod completion_bridge;
mod local_runner;
mod protocol;

pub use backend_bridge::{ghost_result_to_attempt_report, GhostAttemptContext};
pub use completion_bridge::{
    external_stage_outcome_to_runtime_report,
    external_stage_outcome_to_runtime_report_with_context, PriorAcceptedStageState,
    StageJobCompletionContext, StageJobCompletionError,
};
pub use local_runner::{
    run_local_procedure, BackendStageExecutor, FixedBackendStageExecutor, JanusRetryPolicy,
    LocalProcedureError, RoutedBackendStageExecutor, ScottBackendRoutingPolicy,
};
pub use protocol::{
    RuntimeCheckpoint, RuntimeCheckpointError, RuntimeDiagnostics, RuntimeLaunchProvenance,
    ScottControllerKind, ScottDispatchReceipt, ScottJobIdentity, ScottJobKind, ScottJobRole,
    ScottRuntime, ScottRuntimeError, ScottRuntimeJob, ScottRuntimeReport, ScottRuntimeStatus,
    ScottStageDispatch, ScottStageResult,
};
