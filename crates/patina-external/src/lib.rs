#![forbid(unsafe_code)]

/*!
Adapter-side contracts for external DFT and force-evaluation programs.

This crate is intentionally outside the Scott/domain kernels. It owns the vocabulary for input
decks, local or batch launches, output parsing, and adapter status, while the application layer
continues to own workflow orchestration and stage progression.
*/

mod backend_adapters;
mod composition;
mod cp2k_adapter;
mod cp2k_contract;
mod crystal_adapter;
mod crystal_contract;
mod defaults;
mod error;
mod gulp_adapter;
mod ports;
mod types;

pub use backend_adapters::*;
pub use composition::{BackendEvaluatorExternalAdapter, ComposedExternalEvaluator};
pub use cp2k_adapter::{Cp2kExternalAdapter, Cp2kExternalAdapterConfig};
pub use cp2k_contract::{
    parse_cp2k_project_name, Cp2kArtifactCandidate, Cp2kArtifactKind, Cp2kStdoutAssessment,
    Cp2kStdoutStatus, Cp2kTerminalOutputContract,
};
pub use crystal_adapter::{CrystalExternalAdapter, CrystalExternalAdapterConfig};
pub use crystal_contract::{
    CrystalArtifactCandidate, CrystalArtifactKind, CrystalParsedObservables,
    CrystalResultMappingDecision, CrystalResultMappingStatus, CrystalRunIntent,
    CrystalStdoutAssessment, CrystalStdoutStatus, CrystalTerminalOutputContract,
};
pub use defaults::{
    default_janus_adapter_script, default_janus_python_bin, DEFAULT_JANUS_ADAPTER_SCRIPT,
    DEFAULT_JANUS_PYTHON_BIN,
};
pub use error::ExternalError;
pub use gulp_adapter::{GulpExternalAdapter, GulpExternalAdapterConfig};
pub use ports::{ExternalEvaluator, ExternalInputWriter, ExternalLauncher, ExternalOutputParser};
pub use types::{
    external_stage_adapter_plans, CompletedExternalRun, ExternalArtifactClass,
    ExternalArtifactMaterialization, ExternalArtifactRecord, ExternalArtifactRetention,
    ExternalArtifacts, ExternalEvaluationMode, ExternalEvaluationOutcome,
    ExternalEvaluationRequest, ExternalEvaluationStatus, ExternalExecutionMode,
    ExternalLaunchOutcome, ExternalProgram, ExternalStageAdapterPlan, ExternalTemplateSet,
    SubmittedExternalRun,
};
