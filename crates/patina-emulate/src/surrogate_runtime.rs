use crate::{
    AcquisitionRecord, BranchFamily, BranchId, CampaignId, FeatureRepresentation, FidelityClass,
    LearningObjective, PredictionTarget, TaskDirection,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SURROGATE_REQUEST_SCHEMA_VERSION: &str = "patina.emulate.surrogate_request.v1";
pub const SURROGATE_RESPONSE_SCHEMA_VERSION: &str = "patina.emulate.surrogate_response.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurrogateTask {
    ScoreCandidates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurrogateModelVariant {
    MeanFieldSvgp,
    UnwhitenedSvgp,
    WhitenedSvgp,
    MultitaskMeanFieldSvgp,
    MultitaskUnwhitenedSvgp,
    MultitaskWhitenedSvgp,
}

impl SurrogateModelVariant {
    fn supports_target_count(self, target_count: usize) -> bool {
        let is_multitask = matches!(
            self,
            Self::MultitaskMeanFieldSvgp
                | Self::MultitaskUnwhitenedSvgp
                | Self::MultitaskWhitenedSvgp
        );
        match target_count {
            0 => false,
            1 => !is_multitask,
            _ => is_multitask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurrogateObjective {
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateTrainingRow {
    pub candidate_id: String,
    pub features: Vec<f64>,
    pub targets: Vec<f64>,
    pub fidelity: FidelityClass,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateCandidateRow {
    pub candidate_id: String,
    pub features: Vec<f64>,
    pub requested_fidelity: Option<FidelityClass>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurrogateRequestContext {
    pub direction: TaskDirection,
    pub objective: LearningObjective,
    pub feature_representation: FeatureRepresentation,
    pub targets: Vec<PredictionTarget>,
    pub provenance_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateConfig {
    pub model_variant: SurrogateModelVariant,
    pub epochs: usize,
    pub num_inducing: usize,
    pub batch_size: usize,
    pub lr: f64,
    pub n_bootstraps: usize,
    pub random_seed: u64,
    pub log_level: String,
    pub objective: SurrogateObjective,
    pub primary_target_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<SurrogateCheckpointConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurrogateCheckpointConfig {
    pub checkpoint_dir: String,
    pub resume: bool,
    pub save: bool,
}

impl Default for SurrogateConfig {
    fn default() -> Self {
        Self {
            model_variant: SurrogateModelVariant::WhitenedSvgp,
            epochs: 100,
            num_inducing: 24,
            batch_size: 32,
            lr: 0.05,
            n_bootstraps: 1,
            random_seed: 42,
            log_level: "warning".into(),
            objective: SurrogateObjective::Minimize,
            primary_target_index: 0,
            checkpoint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateScoringRequest {
    pub schema_version: String,
    pub campaign_id: CampaignId,
    pub branch_id: BranchId,
    pub workflow: BranchFamily,
    pub task: SurrogateTask,
    pub context: SurrogateRequestContext,
    pub feature_names: Vec<String>,
    pub target_names: Vec<String>,
    pub training_rows: Vec<SurrogateTrainingRow>,
    pub candidate_rows: Vec<SurrogateCandidateRow>,
    pub surrogate: SurrogateConfig,
}

impl SurrogateScoringRequest {
    pub fn validate_shape(&self) -> Result<()> {
        if self.schema_version != SURROGATE_REQUEST_SCHEMA_VERSION {
            bail!(
                "unsupported surrogate request schema version `{}`",
                self.schema_version
            );
        }
        if self.workflow != BranchFamily::GeneticAlgorithm {
            bail!(
                "current surrogate runtime only supports `genetic_algorithm`, got `{}`",
                serde_json::to_string(&self.workflow).unwrap_or_else(|_| "\"unknown\"".into())
            );
        }
        if self.context.direction != TaskDirection::Downstream {
            bail!("current surrogate runtime only supports downstream scoring requests");
        }
        if self.context.objective != LearningObjective::ScoreCandidates
            && self.context.objective != LearningObjective::ReduceUncertainty
        {
            bail!(
                "current surrogate runtime supports only `score_candidates` or `reduce_uncertainty` objectives"
            );
        }
        if self.feature_names.is_empty() {
            bail!("surrogate request must declare at least one feature name");
        }
        if self.target_names.is_empty() {
            bail!("surrogate request must declare at least one target name");
        }
        if self.training_rows.len() < 3 {
            bail!("surrogate request must contain at least three training rows");
        }
        if self.candidate_rows.is_empty() {
            bail!("surrogate request must contain at least one candidate row");
        }
        if self.context.feature_representation.feature_names != self.feature_names {
            bail!("context feature representation names do not match feature_names");
        }
        if self.context.targets.len() != self.target_names.len() {
            bail!("context targets length does not match target_names");
        }
        for (target, target_name) in self.context.targets.iter().zip(&self.target_names) {
            if &target.name != target_name {
                bail!(
                    "context target `{}` does not match target_names entry `{}`",
                    target.name,
                    target_name
                );
            }
        }
        if !self
            .surrogate
            .model_variant
            .supports_target_count(self.target_names.len())
        {
            bail!(
                "model variant `{:?}` is incompatible with target count {}",
                self.surrogate.model_variant,
                self.target_names.len()
            );
        }
        if self.surrogate.epochs == 0 {
            bail!("surrogate config `epochs` must be positive");
        }
        if self.surrogate.num_inducing < 2 {
            bail!("surrogate config `num_inducing` must be at least 2");
        }
        if self.surrogate.batch_size == 0 {
            bail!("surrogate config `batch_size` must be positive");
        }
        if !(self.surrogate.lr.is_finite() && self.surrogate.lr > 0.0) {
            bail!("surrogate config `lr` must be positive and finite");
        }
        if self.surrogate.n_bootstraps == 0 {
            bail!("surrogate config `n_bootstraps` must be positive");
        }
        if self.surrogate.primary_target_index >= self.target_names.len() {
            bail!("surrogate config `primary_target_index` is out of bounds");
        }
        if let Some(checkpoint) = &self.surrogate.checkpoint {
            if checkpoint.checkpoint_dir.trim().is_empty() {
                bail!(
                    "surrogate checkpoint_dir must be non-empty when checkpointing is configured"
                );
            }
            if !checkpoint.resume && !checkpoint.save {
                bail!("surrogate checkpoint config must enable resume or save");
            }
        }

        let feature_count = self.feature_names.len();
        let target_count = self.target_names.len();
        let mut seen_training = BTreeSet::new();
        for row in &self.training_rows {
            validate_candidate_id(&row.candidate_id, "training row")?;
            if !seen_training.insert(row.candidate_id.clone()) {
                bail!("duplicate training candidate_id `{}`", row.candidate_id);
            }
            validate_vector(&row.features, feature_count, "training.features")?;
            validate_vector(&row.targets, target_count, "training.targets")?;
        }

        let mut seen_candidates = BTreeSet::new();
        for row in &self.candidate_rows {
            validate_candidate_id(&row.candidate_id, "candidate row")?;
            if !seen_candidates.insert(row.candidate_id.clone()) {
                bail!("duplicate candidate candidate_id `{}`", row.candidate_id);
            }
            validate_vector(&row.features, feature_count, "candidate.features")?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateCandidatePrediction {
    pub candidate_id: String,
    pub means: Vec<f64>,
    pub variances: Vec<f64>,
    pub acquisition_score: f64,
    pub uncertainty_score: f64,
    pub rank: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateScoringResponse {
    pub schema_version: String,
    pub campaign_id: CampaignId,
    pub branch_id: BranchId,
    pub workflow: BranchFamily,
    pub task: SurrogateTask,
    #[serde(default)]
    pub direction: Option<TaskDirection>,
    #[serde(default)]
    pub objective: Option<LearningObjective>,
    pub feature_names: Vec<String>,
    pub target_names: Vec<String>,
    pub model_variant: SurrogateModelVariant,
    pub selected_model_name: String,
    pub incumbent_target: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_path: Option<String>,
    pub predictions: Vec<SurrogateCandidatePrediction>,
}

impl SurrogateScoringResponse {
    pub fn validate_against(&self, request: &SurrogateScoringRequest) -> Result<()> {
        if self.schema_version != SURROGATE_RESPONSE_SCHEMA_VERSION {
            bail!(
                "unsupported surrogate response schema version `{}`",
                self.schema_version
            );
        }
        if self.campaign_id != request.campaign_id {
            bail!("surrogate response campaign_id does not match request");
        }
        if self.branch_id != request.branch_id {
            bail!("surrogate response branch_id does not match request");
        }
        if self.workflow != request.workflow {
            bail!("surrogate response workflow does not match request");
        }
        if self.task != request.task {
            bail!("surrogate response task does not match request");
        }
        if let Some(direction) = self.direction {
            if direction != request.context.direction {
                bail!("surrogate response direction does not match request");
            }
        }
        if let Some(objective) = self.objective {
            if objective != request.context.objective {
                bail!("surrogate response objective does not match request");
            }
        }
        if self.feature_names != request.feature_names {
            bail!("surrogate response feature_names do not match request");
        }
        if self.target_names != request.target_names {
            bail!("surrogate response target_names do not match request");
        }
        if self.model_variant != request.surrogate.model_variant {
            bail!("surrogate response model_variant does not match request");
        }
        if self.selected_model_name.trim().is_empty() {
            bail!("surrogate response selected_model_name must be non-empty");
        }
        if !self.incumbent_target.is_finite() {
            bail!("surrogate response incumbent_target must be finite");
        }
        if self
            .checkpoint_path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            bail!("surrogate response checkpoint_path must be non-empty when present");
        }
        if self.predictions.len() != request.candidate_rows.len() {
            bail!("surrogate response prediction count does not match candidate count");
        }

        let target_count = self.target_names.len();
        let expected_ids = request
            .candidate_rows
            .iter()
            .map(|row| row.candidate_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut seen_ids = BTreeSet::new();
        let mut seen_ranks = BTreeSet::new();
        for prediction in &self.predictions {
            validate_candidate_id(&prediction.candidate_id, "response prediction")?;
            if !expected_ids.contains(prediction.candidate_id.as_str()) {
                bail!(
                    "surrogate response prediction references unknown candidate_id `{}`",
                    prediction.candidate_id
                );
            }
            if !seen_ids.insert(prediction.candidate_id.clone()) {
                bail!(
                    "surrogate response contains duplicate candidate_id `{}`",
                    prediction.candidate_id
                );
            }
            if prediction.rank == 0 {
                bail!("surrogate response ranks must be positive");
            }
            if !seen_ranks.insert(prediction.rank) {
                bail!(
                    "surrogate response contains duplicate rank {}",
                    prediction.rank
                );
            }
            validate_vector(&prediction.means, target_count, "response.means")?;
            validate_vector(&prediction.variances, target_count, "response.variances")?;
            if !prediction.acquisition_score.is_finite() {
                bail!("surrogate response acquisition scores must be finite");
            }
            if !prediction.uncertainty_score.is_finite() {
                bail!("surrogate response uncertainty scores must be finite");
            }
        }

        for expected_rank in 1..=self.predictions.len() {
            if !seen_ranks.contains(&expected_rank) {
                bail!("surrogate response is missing rank {}", expected_rank);
            }
        }

        Ok(())
    }

    pub fn to_acquisition_records(&self) -> Vec<AcquisitionRecord> {
        let mut predictions = self.predictions.clone();
        predictions.sort_by_key(|prediction| prediction.rank);
        predictions
            .into_iter()
            .map(|prediction| AcquisitionRecord {
                branch_id: self.branch_id.clone(),
                candidate_id: prediction.candidate_id,
                rank: prediction.rank,
                direction: self.direction.unwrap_or(TaskDirection::Downstream),
                objective: self.objective.unwrap_or(LearningObjective::ScoreCandidates),
                acquisition_score: prediction.acquisition_score,
                novelty_score: 0.0,
                uncertainty_score: prediction.uncertainty_score,
                expected_improvement: Some(prediction.acquisition_score),
                recommended_family: Some(self.workflow),
            })
            .collect()
    }
}

pub trait SurrogateScoringPort {
    fn score_candidates(
        &self,
        request: &SurrogateScoringRequest,
    ) -> Result<SurrogateScoringResponse>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UvSurrogateRuntimeConfig {
    pub uv_bin: PathBuf,
    pub project_dir: PathBuf,
    pub work_root: PathBuf,
    pub environment_name: String,
    pub extras: Vec<String>,
    pub native_tls: bool,
    pub timeout: Option<Duration>,
    pub checkpointing: RuntimeCheckpointConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCheckpointConfig {
    pub root: Option<PathBuf>,
    pub resume: bool,
    pub save: bool,
}

impl RuntimeCheckpointConfig {
    pub fn enabled() -> Self {
        Self {
            root: None,
            resume: true,
            save: true,
        }
    }

    pub fn disabled() -> Self {
        Self {
            root: None,
            resume: false,
            save: false,
        }
    }
}

impl UvSurrogateRuntimeConfig {
    pub fn workspace_default() -> Self {
        Self {
            uv_bin: PathBuf::from("uv"),
            project_dir: PathBuf::from("crates/patina-emulate/python"),
            work_root: PathBuf::from("runs/patina-emulate/surrogate"),
            environment_name: "autoemulate".into(),
            extras: vec!["autoemulate".into()],
            native_tls: true,
            timeout: Some(Duration::from_secs(300)),
            checkpointing: RuntimeCheckpointConfig::enabled(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UvSurrogateRuntimeAdapter {
    config: UvSurrogateRuntimeConfig,
}

impl UvSurrogateRuntimeAdapter {
    pub fn new(config: UvSurrogateRuntimeConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &UvSurrogateRuntimeConfig {
        &self.config
    }

    fn runtime_checkpoint_config(
        &self,
        request: &SurrogateScoringRequest,
    ) -> Option<SurrogateCheckpointConfig> {
        if !self.config.checkpointing.resume && !self.config.checkpointing.save {
            return None;
        }
        let root = self
            .config
            .checkpointing
            .root
            .clone()
            .unwrap_or_else(|| self.config.work_root.join("checkpoints"));
        let model_key = serde_json::to_string(&request.surrogate.model_variant)
            .unwrap_or_else(|_| format!("{:?}", request.surrogate.model_variant))
            .trim_matches('"')
            .to_string();
        let descriptor_key = format!(
            "{}-{}-{}-{}f",
            model_key,
            request.context.feature_representation.family,
            request.context.feature_representation.version,
            request.feature_names.len()
        );
        let checkpoint_dir = root
            .join(sanitize_fragment(&request.campaign_id.0))
            .join(sanitize_fragment(&request.branch_id.0))
            .join(sanitize_fragment(&descriptor_key));
        Some(SurrogateCheckpointConfig {
            checkpoint_dir: checkpoint_dir.display().to_string(),
            resume: self.config.checkpointing.resume,
            save: self.config.checkpointing.save,
        })
    }
}

impl SurrogateScoringPort for UvSurrogateRuntimeAdapter {
    fn score_candidates(
        &self,
        request: &SurrogateScoringRequest,
    ) -> Result<SurrogateScoringResponse> {
        let mut runtime_request = request.clone();
        if runtime_request.surrogate.checkpoint.is_none() {
            runtime_request.surrogate.checkpoint = self.runtime_checkpoint_config(&runtime_request);
        }
        if let Some(checkpoint) = &runtime_request.surrogate.checkpoint {
            fs::create_dir_all(&checkpoint.checkpoint_dir).with_context(|| {
                format!(
                    "failed to create surrogate checkpoint dir `{}`",
                    checkpoint.checkpoint_dir
                )
            })?;
        }
        runtime_request.validate_shape()?;

        fs::create_dir_all(&self.config.work_root).with_context(|| {
            format!(
                "failed to create surrogate work root `{}`",
                self.config.work_root.display()
            )
        })?;
        let batch_dir = surrogate_batch_dir(
            &self.config.work_root,
            &runtime_request.campaign_id.0,
            &runtime_request.branch_id.0,
        );
        fs::create_dir_all(&batch_dir).with_context(|| {
            format!(
                "failed to create surrogate batch directory `{}`",
                batch_dir.display()
            )
        })?;

        let request_path = batch_dir.join("request.json");
        let response_path = batch_dir.join("response.json");
        let stdout_path = batch_dir.join("runtime.stdout.log");
        let stderr_path = batch_dir.join("runtime.stderr.log");

        let raw_request = serde_json::to_vec_pretty(&runtime_request)
            .context("failed to serialize surrogate request")?;
        fs::write(&request_path, raw_request).with_context(|| {
            format!(
                "failed to write surrogate request payload `{}`",
                request_path.display()
            )
        })?;

        let mpl_config_dir = self.config.work_root.join(".mplcache");
        fs::create_dir_all(&mpl_config_dir).with_context(|| {
            format!(
                "failed to create matplotlib cache dir `{}`",
                mpl_config_dir.display()
            )
        })?;
        let uv_cache_dir = self.config.work_root.join(".uvcache");
        fs::create_dir_all(&uv_cache_dir).with_context(|| {
            format!("failed to create uv cache dir `{}`", uv_cache_dir.display())
        })?;

        let explicit_environment_path =
            explicit_environment_path(&self.config.project_dir, &self.config.environment_name);
        let mut child = if let Some(environment_path) = explicit_environment_path.as_ref() {
            let python_bin = python_bin_for_environment(environment_path).ok_or_else(|| {
                anyhow::anyhow!(
                    "explicit emulate environment `{}` does not contain a runnable python executable",
                    environment_path.display()
                )
            })?;
            Command::new(&python_bin)
                .current_dir(&self.config.project_dir)
                .env_remove("CONDA_PREFIX")
                .env("PYTHONPATH", self.config.project_dir.join("src"))
                .env("MPLCONFIGDIR", &mpl_config_dir)
                .arg("-m")
                .arg("patina_emulate_runtime.cli")
                .arg("score-candidates")
                .arg("--request")
                .arg(&request_path)
                .arg("--response")
                .arg(&response_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .with_context(|| {
                    format!(
                        "failed to spawn python surrogate runtime with environment `{}`",
                        environment_path.display()
                    )
                })?
        } else {
            let mut command = Command::new(&self.config.uv_bin);
            command
                .current_dir(&self.config.project_dir)
                .env_remove("CONDA_PREFIX")
                .env(
                    "UV_PROJECT_ENVIRONMENT",
                    uv_project_environment_value(
                        &self.config.project_dir,
                        &self.config.environment_name,
                    ),
                )
                .env("UV_CACHE_DIR", &uv_cache_dir)
                .env("MPLCONFIGDIR", &mpl_config_dir)
                .arg("run")
                .arg("--no-dev");
            if self.config.native_tls {
                command.arg("--native-tls");
            }
            for extra in &self.config.extras {
                command.arg("--extra").arg(extra);
            }
            command
                .arg("--project")
                .arg(&self.config.project_dir)
                .arg("patina-emulate-runtime")
                .arg("score-candidates")
                .arg("--request")
                .arg(&request_path)
                .arg("--response")
                .arg(&response_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .with_context(|| {
                    format!(
                        "failed to spawn uv surrogate runtime with project `{}`",
                        self.config.project_dir.display()
                    )
                })?
        };

        let started = Instant::now();
        let output = if let Some(timeout) = self.config.timeout {
            loop {
                if let Some(status) = child
                    .try_wait()
                    .context("failed to poll surrogate runtime")?
                {
                    let output = child
                        .wait_with_output()
                        .context("failed to collect surrogate runtime output")?;
                    break CompletedProcess {
                        status,
                        stdout: output.stdout,
                        stderr: output.stderr,
                    };
                }

                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "surrogate runtime timed out after {:?}; preserved artifacts in `{}`",
                        started.elapsed(),
                        batch_dir.display()
                    );
                }

                std::thread::sleep(Duration::from_millis(50));
            }
        } else {
            let output = child
                .wait_with_output()
                .context("failed to collect surrogate runtime output")?;
            CompletedProcess {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            }
        };

        fs::write(&stdout_path, &output.stdout)
            .with_context(|| format!("failed to persist `{}`", stdout_path.display()))?;
        fs::write(&stderr_path, &output.stderr)
            .with_context(|| format!("failed to persist `{}`", stderr_path.display()))?;

        if !output.status.success() {
            bail!(
                "surrogate runtime exited with code {:?}; see `{}` and `{}`",
                output.status.code(),
                stdout_path.display(),
                stderr_path.display()
            );
        }
        if !response_path.exists() {
            bail!(
                "surrogate runtime exited successfully but did not create `{}`",
                response_path.display()
            );
        }

        let raw_response = fs::read(&response_path).with_context(|| {
            format!(
                "failed to read surrogate response payload `{}`",
                response_path.display()
            )
        })?;
        let response: SurrogateScoringResponse = serde_json::from_slice(&raw_response)
            .with_context(|| {
                format!(
                    "failed to parse surrogate response payload `{}`",
                    response_path.display()
                )
            })?;
        response
            .validate_against(&runtime_request)
            .with_context(|| {
                format!(
                    "surrogate response validation failed for artifacts in `{}`",
                    batch_dir.display()
                )
            })?;

        Ok(response)
    }
}

struct CompletedProcess {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn surrogate_batch_dir(work_root: &Path, campaign_id: &str, branch_id: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    work_root.join(format!(
        "{}-{}-{}",
        sanitize_fragment(campaign_id),
        sanitize_fragment(branch_id),
        stamp
    ))
}

pub(crate) fn explicit_environment_path(
    project_dir: &Path,
    environment_name: &str,
) -> Option<PathBuf> {
    let trimmed = environment_name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = Path::new(trimmed);
    if candidate.is_absolute() || trimmed.starts_with('.') || candidate.components().count() > 1 {
        let resolved = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| project_dir.to_path_buf())
                .join(candidate)
        };
        return Some(resolved);
    }

    None
}

pub(crate) fn uv_project_environment_value(project_dir: &Path, environment_name: &str) -> String {
    let trimmed = environment_name.trim();
    if trimmed.is_empty() {
        return default_workspace_environment_path(project_dir, "autoemulate");
    }

    if let Some(resolved) = explicit_environment_path(project_dir, environment_name) {
        return resolved.display().to_string();
    }

    default_workspace_environment_path(project_dir, trimmed)
}

fn default_workspace_environment_path(project_dir: &Path, environment_name: &str) -> String {
    if let Some(workspace_root) = workspace_root_for(project_dir) {
        return workspace_root
            .join("venvs")
            .join(environment_name)
            .display()
            .to_string();
    }
    format!("venvs/{environment_name}")
}

fn workspace_root_for(project_dir: &Path) -> Option<PathBuf> {
    let anchored = if project_dir.is_absolute() {
        project_dir.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(project_dir)
    };
    anchored
        .ancestors()
        .filter(|candidate| candidate.join("Cargo.toml").is_file())
        .last()
        .map(Path::to_path_buf)
}

pub(crate) fn python_bin_for_environment(environment_path: &Path) -> Option<PathBuf> {
    let unix = environment_path.join("bin").join("python");
    if unix.is_file() {
        return Some(unix);
    }
    let unix3 = environment_path.join("bin").join("python3");
    if unix3.is_file() {
        return Some(unix3);
    }
    let windows = environment_path.join("Scripts").join("python.exe");
    if windows.is_file() {
        return Some(windows);
    }
    None
}

fn sanitize_fragment(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unnamed".into();
    }
    trimmed
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

fn validate_candidate_id(candidate_id: &str, label: &str) -> Result<()> {
    if candidate_id.trim().is_empty() {
        bail!("{label} candidate_id must be non-empty");
    }
    Ok(())
}

fn validate_vector(values: &[f64], expected_len: usize, label: &str) -> Result<()> {
    if values.len() != expected_len {
        bail!(
            "{label} length {} does not match expected length {}",
            values.len(),
            expected_len
        );
    }
    if values.iter().any(|value| !value.is_finite()) {
        bail!("{label} must contain only finite values");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn sample_request() -> SurrogateScoringRequest {
        SurrogateScoringRequest {
            schema_version: SURROGATE_REQUEST_SCHEMA_VERSION.into(),
            campaign_id: CampaignId("campaign-demo".into()),
            branch_id: BranchId("branch-ga-1".into()),
            workflow: BranchFamily::GeneticAlgorithm,
            task: SurrogateTask::ScoreCandidates,
            context: SurrogateRequestContext {
                direction: TaskDirection::Downstream,
                objective: LearningObjective::ScoreCandidates,
                feature_representation: FeatureRepresentation {
                    family: "global_soap_pca".into(),
                    version: "v1".into(),
                    feature_names: vec!["soap_pc1".into(), "soap_pc2".into()],
                    provenance_label: "demo_descriptor".into(),
                },
                targets: vec![PredictionTarget {
                    name: "energy".into(),
                    head_kind: crate::PredictionHeadKind::Scalar,
                    fidelity: FidelityClass::JanusMaceLow,
                    unit: Some("ev".into()),
                }],
                provenance_label: Some("ga_generation_3".into()),
            },
            feature_names: vec!["soap_pc1".into(), "soap_pc2".into()],
            target_names: vec!["energy".into()],
            training_rows: vec![
                SurrogateTrainingRow {
                    candidate_id: "obs-1".into(),
                    features: vec![-1.0, 0.8],
                    targets: vec![-1.8],
                    fidelity: FidelityClass::JanusMaceLow,
                    metadata: BTreeMap::new(),
                },
                SurrogateTrainingRow {
                    candidate_id: "obs-2".into(),
                    features: vec![0.0, 0.1],
                    targets: vec![-1.4],
                    fidelity: FidelityClass::JanusMaceLow,
                    metadata: BTreeMap::new(),
                },
                SurrogateTrainingRow {
                    candidate_id: "obs-3".into(),
                    features: vec![1.1, -0.6],
                    targets: vec![-1.9],
                    fidelity: FidelityClass::JanusMaceHigh,
                    metadata: BTreeMap::new(),
                },
            ],
            candidate_rows: vec![
                SurrogateCandidateRow {
                    candidate_id: "cand-1".into(),
                    features: vec![0.2, 0.0],
                    requested_fidelity: Some(FidelityClass::JanusMaceHigh),
                    metadata: BTreeMap::new(),
                },
                SurrogateCandidateRow {
                    candidate_id: "cand-2".into(),
                    features: vec![0.8, -0.2],
                    requested_fidelity: Some(FidelityClass::JanusMaceHigh),
                    metadata: BTreeMap::new(),
                },
            ],
            surrogate: SurrogateConfig::default(),
        }
    }

    #[test]
    fn request_validation_rejects_feature_mismatch() {
        let mut request = sample_request();
        request.candidate_rows[0].features = vec![0.2];

        let error = request
            .validate_shape()
            .expect_err("feature mismatch should be rejected");

        assert!(error
            .to_string()
            .contains("candidate.features length 1 does not match expected length 2"));
    }

    #[test]
    fn response_converts_to_ranked_acquisition_records() {
        let request = sample_request();
        let response = SurrogateScoringResponse {
            schema_version: SURROGATE_RESPONSE_SCHEMA_VERSION.into(),
            campaign_id: request.campaign_id.clone(),
            branch_id: request.branch_id.clone(),
            workflow: request.workflow,
            task: request.task,
            direction: Some(TaskDirection::Downstream),
            objective: Some(LearningObjective::ReduceUncertainty),
            feature_names: request.feature_names.clone(),
            target_names: request.target_names.clone(),
            model_variant: request.surrogate.model_variant,
            selected_model_name: "Whitened Full-Covariance SVGP".into(),
            incumbent_target: -1.9,
            checkpoint_path: None,
            predictions: vec![
                SurrogateCandidatePrediction {
                    candidate_id: "cand-2".into(),
                    means: vec![-1.7],
                    variances: vec![0.02],
                    acquisition_score: 0.4,
                    uncertainty_score: 0.14,
                    rank: 1,
                },
                SurrogateCandidatePrediction {
                    candidate_id: "cand-1".into(),
                    means: vec![-1.6],
                    variances: vec![0.03],
                    acquisition_score: 0.2,
                    uncertainty_score: 0.17,
                    rank: 2,
                },
            ],
        };

        let records = response.to_acquisition_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].candidate_id, "cand-2");
        assert_eq!(records[0].rank, 1);
        assert_eq!(records[0].direction, TaskDirection::Downstream);
        assert_eq!(records[0].objective, LearningObjective::ReduceUncertainty);
        assert_eq!(
            records[0].recommended_family,
            Some(BranchFamily::GeneticAlgorithm)
        );
    }

    #[cfg(unix)]
    #[test]
    fn uv_adapter_writes_request_and_parses_response() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        let work_root = temp.path().join("runs");
        fs::create_dir_all(&project_dir).expect("project dir");
        fs::create_dir_all(&work_root).expect("work root");

        let uv_bin = temp.path().join("fake_uv.sh");
        let mut script = fs::File::create(&uv_bin).expect("fake uv");
        writeln!(
            script,
            "#!/bin/sh\nREQUEST=\"\"\nRESPONSE=\"\"\nARGS=\"$*\"\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    --request) REQUEST=\"$2\"; shift 2 ;;\n    --response) RESPONSE=\"$2\"; shift 2 ;;\n    *) shift ;;\n  esac\ndone\ncat > \"$RESPONSE\" <<'JSON'\n{{\n  \"schema_version\": \"{response_schema}\",\n  \"campaign_id\": \"campaign-demo\",\n  \"branch_id\": \"branch-ga-1\",\n  \"workflow\": \"genetic_algorithm\",\n  \"task\": \"score_candidates\",\n  \"direction\": \"downstream\",\n  \"objective\": \"score_candidates\",\n  \"feature_names\": [\"soap_pc1\", \"soap_pc2\"],\n  \"target_names\": [\"energy\"],\n  \"model_variant\": \"whitened_svgp\",\n  \"selected_model_name\": \"Whitened Full-Covariance SVGP\",\n  \"incumbent_target\": -1.9,\n  \"predictions\": [\n    {{\n      \"candidate_id\": \"cand-2\",\n      \"means\": [-1.7],\n      \"variances\": [0.02],\n      \"acquisition_score\": 0.4,\n      \"uncertainty_score\": 0.14,\n      \"rank\": 1\n    }},\n    {{\n      \"candidate_id\": \"cand-1\",\n      \"means\": [-1.6],\n      \"variances\": [0.03],\n      \"acquisition_score\": 0.2,\n      \"uncertainty_score\": 0.17,\n      \"rank\": 2\n    }}\n  ]\n}}\nJSON\nprintf '%s' \"$UV_PROJECT_ENVIRONMENT\" > \"{env_path}\"\nprintf '%s' \"$MPLCONFIGDIR\" > \"{mpl_env_path}\"\nprintf '%s' \"$ARGS\" > \"{args_path}\"\n",
            response_schema = SURROGATE_RESPONSE_SCHEMA_VERSION,
            env_path = project_dir.join("uv_env.txt").display(),
            mpl_env_path = project_dir.join("mpl_env.txt").display(),
            args_path = project_dir.join("uv_args.txt").display(),
        )
        .expect("write fake uv");
        script.flush().expect("flush fake uv");
        drop(script);
        let mut perms = fs::metadata(&uv_bin).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&uv_bin, perms).expect("chmod");

        let adapter = UvSurrogateRuntimeAdapter::new(UvSurrogateRuntimeConfig {
            uv_bin,
            project_dir: project_dir.clone(),
            work_root: work_root.clone(),
            environment_name: "autoemulate".into(),
            extras: vec!["autoemulate".into()],
            native_tls: true,
            timeout: Some(Duration::from_secs(5)),
            checkpointing: RuntimeCheckpointConfig::disabled(),
        });

        let response = adapter
            .score_candidates(&sample_request())
            .expect("response should parse");

        assert_eq!(response.predictions[0].candidate_id, "cand-2");
        assert_eq!(
            fs::read_to_string(project_dir.join("uv_env.txt")).expect("env log"),
            "venvs/autoemulate"
        );
        assert!(fs::read_to_string(project_dir.join("uv_args.txt"))
            .expect("arg log")
            .contains("--extra autoemulate"));
        assert!(fs::read_to_string(project_dir.join("uv_args.txt"))
            .expect("arg log")
            .contains("--native-tls"));
        assert!(fs::read_to_string(project_dir.join("uv_args.txt"))
            .expect("arg log")
            .contains("--no-dev"));
        assert!(fs::read_to_string(project_dir.join("mpl_env.txt"))
            .expect("mpl env log")
            .contains(".mplcache"));

        let batch_dirs = fs::read_dir(&work_root)
            .expect("read work root")
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| {
                !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(".mplcache" | ".uvcache")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(batch_dirs.len(), 1);
        assert!(batch_dirs[0].join("request.json").exists());
        assert!(batch_dirs[0].join("response.json").exists());
    }

    #[test]
    fn absolute_environment_path_is_preserved() {
        let project_dir = PathBuf::from("/tmp/project");
        let env_path = "/tmp/external-autoemulate/.venv";
        assert_eq!(
            uv_project_environment_value(&project_dir, env_path),
            env_path
        );
    }

    #[test]
    fn workspace_projects_resolve_bare_environment_names_under_repo_root_venvs() {
        let project_dir = PathBuf::from("crates/patina-emulate/python");
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .filter(|candidate| candidate.join("Cargo.toml").is_file())
            .last()
            .expect("workspace root")
            .to_path_buf();
        assert_eq!(
            uv_project_environment_value(&project_dir, "autoemulate"),
            repo_root
                .join("venvs")
                .join("autoemulate")
                .display()
                .to_string()
        );
    }
}
