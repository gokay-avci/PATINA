use patina_evaluator::{ScottBackendMode, StageEngine, StagePlan, StageSelection};
use patina_types::{Candidate, EvalResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::ExternalError;

/// External program reachable from a Scott-style evaluator stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalProgram {
    Gulp,
    Cp2k,
    Crystal,
    Aims,
    Vasp,
    Nwchem,
    Dmol,
    JanusMace,
    NoEvalExport,
}

impl ExternalProgram {
    /// Stable label for run metadata and adapter dispatch.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gulp => "gulp",
            Self::Cp2k => "cp2k",
            Self::Crystal => "crystal",
            Self::Aims => "aims",
            Self::Vasp => "vasp",
            Self::Nwchem => "nwchem",
            Self::Dmol => "dmol",
            Self::JanusMace => "janus_mace",
            Self::NoEvalExport => "no_eval_export",
        }
    }

    /// Native SCOTT only retrieves VASP geometries for periodic systems.
    pub fn requires_three_d_periodic_input(self) -> bool {
        matches!(self, Self::Vasp)
    }

    /// Programs represented by native SCOTT's `DEF_ENERGY` external-DFT family.
    pub fn is_native_dft_program(self) -> bool {
        matches!(
            self,
            Self::Cp2k | Self::Crystal | Self::Aims | Self::Vasp | Self::Nwchem | Self::Dmol
        )
    }
}

impl From<StageEngine> for ExternalProgram {
    fn from(value: StageEngine) -> Self {
        match value {
            StageEngine::Gulp => Self::Gulp,
            StageEngine::Cp2k => Self::Cp2k,
            StageEngine::Crystal => Self::Crystal,
            StageEngine::Aims => Self::Aims,
            StageEngine::Vasp => Self::Vasp,
            StageEngine::Nwchem => Self::Nwchem,
            StageEngine::Dmol => Self::Dmol,
            StageEngine::NoEvalExport => Self::NoEvalExport,
        }
    }
}

impl From<ScottBackendMode> for ExternalProgram {
    fn from(value: ScottBackendMode) -> Self {
        match value {
            ScottBackendMode::Gulp => Self::Gulp,
            ScottBackendMode::JanusMace => Self::JanusMace,
        }
    }
}

/// Adapter-side evaluation mode for a single external stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalEvaluationMode {
    SinglePoint,
    Relaxation,
    ExportOnly,
}

/// How an adapter should execute a prepared external-code job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalExecutionMode {
    LocalSubprocess,
    LocalScript,
    SchedulerSubmitted,
    RemoteBatch,
}

/// Template bundle owned by an external adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExternalTemplateSet {
    pub primary_template: Option<PathBuf>,
    pub named_templates: BTreeMap<String, PathBuf>,
}

impl ExternalTemplateSet {
    pub fn with_primary(primary_template: impl Into<PathBuf>) -> Self {
        Self {
            primary_template: Some(primary_template.into()),
            named_templates: BTreeMap::new(),
        }
    }

    pub fn insert_named_template(
        &mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Option<PathBuf> {
        self.named_templates.insert(name.into(), path.into())
    }
}

/// Logical classification for one path staged or produced by an external adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalArtifactClass {
    TemplateInput,
    ControlInput,
    StructureInput,
    PrimaryOutput,
    AuxiliaryOutput,
    LogOutput,
}

/// How a concrete adapter path came to exist in the staged workdir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalArtifactMaterialization {
    Linked,
    Copied,
    Rendered,
    Generated,
    RuntimeProduced,
}

/// Intended retention semantics for one adapter-owned path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalArtifactRetention {
    RegenerableScratch,
    DurableAlways,
    DurableOnSuccess,
    DurableOnFailure,
    DurableOnTerminalState,
}

/// Replayable policy record for one staged or produced adapter path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalArtifactRecord {
    pub path: PathBuf,
    pub class: ExternalArtifactClass,
    pub materialization: ExternalArtifactMaterialization,
    pub retention: ExternalArtifactRetention,
    pub is_immutable_input: bool,
}

/// Files produced or consumed by an external adapter stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExternalArtifacts {
    pub workdir: PathBuf,
    pub input_paths: Vec<PathBuf>,
    pub output_paths: Vec<PathBuf>,
    pub stdout_path: Option<PathBuf>,
    pub stderr_path: Option<PathBuf>,
    #[serde(default)]
    pub artifact_records: Vec<ExternalArtifactRecord>,
}

impl ExternalArtifacts {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            workdir: workdir.into(),
            ..Self::default()
        }
    }

    pub fn register_input(
        &mut self,
        path: impl Into<PathBuf>,
        class: ExternalArtifactClass,
        materialization: ExternalArtifactMaterialization,
        retention: ExternalArtifactRetention,
    ) {
        let path = path.into();
        self.input_paths.push(path.clone());
        self.artifact_records.push(ExternalArtifactRecord {
            path,
            class,
            materialization,
            retention,
            is_immutable_input: true,
        });
    }

    pub fn register_output(
        &mut self,
        path: impl Into<PathBuf>,
        class: ExternalArtifactClass,
        retention: ExternalArtifactRetention,
    ) {
        let path = path.into();
        self.output_paths.push(path.clone());
        self.artifact_records.push(ExternalArtifactRecord {
            path,
            class,
            materialization: ExternalArtifactMaterialization::RuntimeProduced,
            retention,
            is_immutable_input: false,
        });
    }

    pub fn set_stdout_path(&mut self, path: impl Into<PathBuf>) {
        self.stdout_path = Some(path.into());
    }

    pub fn set_stderr_path_with_retention(
        &mut self,
        path: impl Into<PathBuf>,
        retention: ExternalArtifactRetention,
    ) {
        let path = path.into();
        self.stderr_path = Some(path.clone());
        self.artifact_records.push(ExternalArtifactRecord {
            path,
            class: ExternalArtifactClass::LogOutput,
            materialization: ExternalArtifactMaterialization::RuntimeProduced,
            retention,
            is_immutable_input: false,
        });
    }

    pub fn paths_with_retention(
        &self,
        retention: ExternalArtifactRetention,
    ) -> impl Iterator<Item = &PathBuf> {
        self.artifact_records
            .iter()
            .filter(move |record| record.retention == retention)
            .map(|record| &record.path)
    }
}

/// Fully staged request handed to an adapter edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalEvaluationRequest {
    pub request_id: String,
    pub stage: u8,
    pub program: ExternalProgram,
    pub mode: ExternalEvaluationMode,
    pub execution: ExternalExecutionMode,
    pub retrieve_relaxed_geometry: bool,
    pub candidate: Candidate,
    pub workdir: PathBuf,
    pub templates: ExternalTemplateSet,
}

impl ExternalEvaluationRequest {
    pub fn validate(&self) -> Result<(), ExternalError> {
        self.candidate
            .validate()
            .map_err(|err| ExternalError::InvalidRequest {
                reason: format!("invalid candidate: {err:?}"),
            })?;

        if self.stage == 0 {
            return Err(ExternalError::InvalidRequest {
                reason: "stage must use native Scott one-based indexing".into(),
            });
        }

        if self.program == ExternalProgram::NoEvalExport
            && self.mode != ExternalEvaluationMode::ExportOnly
        {
            return Err(ExternalError::InvalidRequest {
                reason: "no-evaluation export stages must use export-only mode".into(),
            });
        }

        if self.program.requires_three_d_periodic_input() && !self.candidate.is_three_d_periodic() {
            return Err(ExternalError::UnsupportedCandidate {
                program: self.program,
                reason: "native SCOTT VASP retrieval is periodic-only".into(),
            });
        }

        Ok(())
    }
}

/// External adapter plan derived from a typed Scott stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalStageAdapterPlan {
    pub stage: u8,
    pub program: ExternalProgram,
    pub refine_if_energy_below: Option<f64>,
    pub energy_min_threshold: Option<f64>,
    pub energy_max_threshold: Option<f64>,
    pub keep_only_if_final_stage: bool,
}

impl From<&StagePlan> for ExternalStageAdapterPlan {
    fn from(value: &StagePlan) -> Self {
        Self {
            stage: value.stage.get(),
            program: ExternalProgram::from(value.engine),
            refine_if_energy_below: value.refine_if_energy_below,
            energy_min_threshold: value.energy_min_threshold,
            energy_max_threshold: value.energy_max_threshold,
            keep_only_if_final_stage: value.keep_only_if_final_stage,
        }
    }
}

/// Reuses the Scott evaluator's native stage parsing instead of reparsing `N_DEF_ENERGY`.
pub fn external_stage_adapter_plans(selection: &StageSelection) -> Vec<ExternalStageAdapterPlan> {
    selection
        .stages
        .iter()
        .map(ExternalStageAdapterPlan::from)
        .collect()
}

/// Completed process metadata before semantic output parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedExternalRun {
    pub program: ExternalProgram,
    pub execution: ExternalExecutionMode,
    pub artifacts: ExternalArtifacts,
    pub exit_code: Option<i32>,
    pub elapsed: Duration,
}

/// Deferred launch metadata for asynchronously submitted external work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmittedExternalRun {
    pub program: ExternalProgram,
    pub execution: ExternalExecutionMode,
    pub artifacts: ExternalArtifacts,
    pub launch_handle: String,
    pub remote_workdir: Option<PathBuf>,
}

/// Result of the launcher boundary before semantic parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExternalLaunchOutcome {
    Completed { run: CompletedExternalRun },
    Submitted { run: SubmittedExternalRun },
}

/// Adapter-side semantic status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalEvaluationStatus {
    Submitted,
    Converged,
    NotConverged,
    ExportedOnly,
}

/// Parsed external-code result plus provenance artifacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalEvaluationOutcome {
    pub status: ExternalEvaluationStatus,
    pub result: Option<EvalResult>,
    pub artifacts: ExternalArtifacts,
    pub submitted_run: Option<SubmittedExternalRun>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use patina_evaluator::{StageEngine, StageIndex};

    fn cluster_candidate() -> Candidate {
        Candidate::cluster("mg", vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn periodic_candidate() -> Candidate {
        Candidate::fully_periodic(
            "mgo",
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]],
        )
    }

    fn request(program: ExternalProgram, candidate: Candidate) -> ExternalEvaluationRequest {
        ExternalEvaluationRequest {
            request_id: "req-1".into(),
            stage: 1,
            program,
            mode: ExternalEvaluationMode::Relaxation,
            execution: ExternalExecutionMode::LocalSubprocess,
            retrieve_relaxed_geometry: true,
            candidate,
            workdir: "/tmp/patina-external-test".into(),
            templates: ExternalTemplateSet::default(),
        }
    }

    #[test]
    fn maps_scott_stage_plan_to_external_adapter_plan() {
        let stage = StagePlan {
            stage: StageIndex(2),
            engine: StageEngine::Cp2k,
            refine_if_energy_below: Some(-10.0),
            energy_min_threshold: Some(-100.0),
            energy_max_threshold: Some(0.0),
            keep_only_if_final_stage: true,
        };

        let plan = ExternalStageAdapterPlan::from(&stage);

        assert_eq!(plan.stage, 2);
        assert_eq!(plan.program, ExternalProgram::Cp2k);
        assert_eq!(plan.refine_if_energy_below, Some(-10.0));
        assert_eq!(plan.energy_min_threshold, Some(-100.0));
        assert_eq!(plan.energy_max_threshold, Some(0.0));
        assert!(plan.keep_only_if_final_stage);
    }

    #[test]
    fn no_eval_export_must_use_export_only_mode() {
        let req = request(ExternalProgram::NoEvalExport, cluster_candidate());

        assert!(matches!(
            req.validate(),
            Err(ExternalError::InvalidRequest { reason }) if reason.contains("export-only")
        ));
    }

    #[test]
    fn vasp_is_rejected_for_native_cluster_request() {
        let req = request(ExternalProgram::Vasp, cluster_candidate());

        assert!(matches!(
            req.validate(),
            Err(ExternalError::UnsupportedCandidate { program, .. })
                if program == ExternalProgram::Vasp
        ));
    }

    #[test]
    fn vasp_accepts_three_d_periodic_request() {
        let req = request(ExternalProgram::Vasp, periodic_candidate());

        req.validate()
            .expect("periodic VASP request should validate");
    }

    #[test]
    fn scheduler_submitted_execution_mode_is_serializable_request_shape() {
        let mut req = request(ExternalProgram::Cp2k, periodic_candidate());
        req.execution = ExternalExecutionMode::SchedulerSubmitted;
        req.validate()
            .expect("scheduler-submitted request should still validate");
    }

    #[test]
    fn external_artifacts_track_materialization_and_retention() {
        let mut artifacts = ExternalArtifacts::new("/tmp/run");
        artifacts.register_input(
            "/tmp/run/cp2k.inp",
            ExternalArtifactClass::ControlInput,
            ExternalArtifactMaterialization::Copied,
            ExternalArtifactRetention::RegenerableScratch,
        );
        artifacts.register_output(
            "/tmp/run/cp2k.out",
            ExternalArtifactClass::PrimaryOutput,
            ExternalArtifactRetention::DurableOnTerminalState,
        );
        artifacts.set_stdout_path("/tmp/run/cp2k.out");
        artifacts.set_stderr_path_with_retention(
            "/tmp/run/cp2k.stderr.log",
            ExternalArtifactRetention::DurableOnFailure,
        );

        assert_eq!(
            artifacts.input_paths,
            vec![PathBuf::from("/tmp/run/cp2k.inp")]
        );
        assert_eq!(
            artifacts.output_paths,
            vec![PathBuf::from("/tmp/run/cp2k.out")]
        );
        assert_eq!(
            artifacts.stdout_path,
            Some(PathBuf::from("/tmp/run/cp2k.out"))
        );
        assert_eq!(artifacts.artifact_records.len(), 3);
        assert_eq!(
            artifacts
                .paths_with_retention(ExternalArtifactRetention::DurableOnFailure)
                .cloned()
                .collect::<Vec<_>>(),
            vec![PathBuf::from("/tmp/run/cp2k.stderr.log")]
        );
    }
}
