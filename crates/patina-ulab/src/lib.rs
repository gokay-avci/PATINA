#![forbid(unsafe_code)]

/*!
What this crate implements: the optional Scott-to-HPC orchestration adapter layer.
Design basis: Scott scientific/runtime code must not depend directly on any site-specific
scheduler or the archived `unified_lab` runtime, but we still want a native path for
projecting Scott jobs into monitorable, schedulable workflow substrates.
Current scope:

- allocation-local runtime wrapper ports and launch-envelope types
- scheduler-neutral projection helpers
- first-class shard / container records for many-task allocations
- site-profile and launcher-policy metadata
- worker capability and lease vocabulary
- workspace planning that separates durable storage from scratch execution
- scheduler-facing traits that preserve Scott runtime ownership

This crate is intentionally adapter-shaped. It must not become the owner of Scott procedure
semantics, DFT deck semantics, or generic workflow-engine logic.
*/

mod allocation_runtime;
mod bridge;
mod capabilities;
mod coordinator;
mod lease;
mod projection_store;
mod receipt;
mod reconciliation;
mod report_bridge;
mod scheduler;
mod shard;
mod site_profile;
mod state;
mod workspace;

pub use allocation_runtime::{
    AllocationHeartbeat, AllocationLaunchEnvelope, AllocationRuntimePort,
    AllocationRuntimePortError, AllocationTaskPhase, AllocationTerminalSummary,
    LocalAllocationRuntimePort,
};
pub use bridge::{
    UnifiedLabBridgeConfig, UnifiedLabProjectionError, UnifiedLabRuntimeBridge,
    UnifiedLabWorkerEnvelope, UnifiedLabWorkflowEnvelope,
};
pub use capabilities::{CapabilityMatch, ResourceShape, WorkerCapabilities};
pub use coordinator::{
    CoordinatorShardSnapshot, CoordinatorShardState, ShardAdmissionPlan, ShardCoordinator,
    ShardCoordinatorBudget, ShardCoordinatorError, ShardMembershipRecovery,
    ShardTerminalDisposition, ShardTerminalOutcome,
};
pub use lease::{LeaseState, WorkLease};
pub use projection_store::{DurableProjectionStore, ProjectionStoreError};
pub use receipt::{
    ExternalBatchReceipt, JobProcessTelemetry, JobScopedTelemetrySample, SchedulerJobIdentifier,
    SchedulerReceiptState, TelemetrySample,
};
pub use reconciliation::{
    reconcile_job_truth, reconstruct_reconciliation_inputs, ArtifactEvidence, EvidenceSource,
    ParserEvidence, ReconciliationInputs, ReconciliationOutcome, RecoveryGap, RecoverySnapshot,
    ReplayOperationKey, ReplayOperationKind, TerminalParserEvidence,
};
pub use report_bridge::{
    reconciled_stage_completion_to_runtime_report, ReconciledStageCompletionContext,
    ReconciledStageCompletionError,
};
pub use scheduler::{
    GridEngineCommandRunner, GridEngineSchedulerAdapter, PbsCommandRunner, PbsSchedulerAdapter,
    RuntimeBackedScheduler, SchedulerAdapterError, SchedulerCommand, SchedulerCommandOutput,
    SchedulerPlacement, SlurmCommandRunner, SlurmSchedulerAdapter, UlabScheduler,
};
pub use shard::{ContainerShard, ShardAssignmentPolicy, ShardRetryPolicy, ShardTelemetryRollup};
pub use site_profile::{
    LaunchStrategy, LauncherPolicy, LauncherProvenance, ModuleEnvironment, SchedulerFamily,
    ScratchMode, ScratchPolicy, SiteProfile, SubmissionMode, SubmissionPolicy,
};
pub use state::{
    DftFailureClass, JobFailureKind, JobProfilingSnapshot, OrchestratedJobState,
    OrchestratedJobStatus,
};
pub use workspace::{WorkspacePlan, WorkspacePolicy, WorkspaceRoots};
