use patina_runtime::RuntimeLaunchProvenance;
use serde::{Deserialize, Serialize};

/// Scheduler family seen by `patina-ulab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerFamily {
    Local,
    Slurm,
    Pbs,
    GridEngine,
    Unknown,
}

/// High-level launcher policy for site-specific command wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStrategy {
    Direct,
    Srun,
    Mpirun,
    PbsMpi,
}

/// Module environment expected by the site launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleEnvironment {
    None,
    EnvironmentModules,
    Lmod,
}

/// Default scratch behavior for a site profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchMode {
    DurableOnly,
    SharedFilesystem,
    NodeLocal,
}

/// Preferred scheduling mode for Scott jobs on a given site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionMode {
    DirectRuntime,
    WithinAllocation,
    JobArray,
    NestedBatch,
}

/// How jobs are launched and observed on a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherPolicy {
    pub strategy: LaunchStrategy,
    pub launch_command: String,
    pub submit_command: Option<String>,
    pub status_command: Option<String>,
    pub accounting_command: Option<String>,
    pub cancel_command: Option<String>,
    pub telemetry_command: Option<String>,
    pub scheduler_job_id_env: Option<String>,
    pub module_environment: ModuleEnvironment,
}

/// Scratch and staging behavior for a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScratchPolicy {
    pub default_mode: ScratchMode,
    pub env_vars: Vec<String>,
    pub optional_node_local: bool,
    pub requires_explicit_request: bool,
}

/// Coordinator-side submission budget and scheduler pressure controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionPolicy {
    pub preferred_mode: SubmissionMode,
    pub prefer_allocation_reuse: bool,
    pub prefer_job_arrays: bool,
    pub allow_nested_scheduler_submit: bool,
    pub max_scheduler_jobs_per_campaign: u32,
    pub max_nested_scheduler_jobs: u32,
    pub scheduler_poll_interval_secs: u64,
}

/// Serializable launcher/site snapshot that can be attached to runtime diagnostics and receipts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherProvenance {
    pub site_name: String,
    pub scheduler_family: SchedulerFamily,
    pub launch_strategy: LaunchStrategy,
    pub launch_command: String,
    pub submit_command: Option<String>,
    pub status_command: Option<String>,
    pub accounting_command: Option<String>,
    pub cancel_command: Option<String>,
    pub telemetry_command: Option<String>,
    pub scheduler_job_id_env: Option<String>,
    pub module_environment: ModuleEnvironment,
    pub scratch_mode: ScratchMode,
    pub scratch_env_vars: Vec<String>,
    pub preferred_submission_mode: SubmissionMode,
    pub allow_nested_scheduler_submit: bool,
    pub max_nested_scheduler_jobs: u32,
    pub poll_interval_secs: u64,
}

/// Site-level execution profile used by the orchestration adapter layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteProfile {
    pub site_name: String,
    pub scheduler_family: SchedulerFamily,
    pub launcher: LauncherPolicy,
    pub scratch_policy: ScratchPolicy,
    pub submission_policy: SubmissionPolicy,
    pub default_worker_tags: Vec<String>,
}

impl SiteProfile {
    pub fn local() -> Self {
        Self {
            site_name: "local".into(),
            scheduler_family: SchedulerFamily::Local,
            launcher: LauncherPolicy {
                strategy: LaunchStrategy::Direct,
                launch_command: "direct".into(),
                submit_command: None,
                status_command: None,
                accounting_command: None,
                cancel_command: None,
                telemetry_command: None,
                scheduler_job_id_env: None,
                module_environment: ModuleEnvironment::None,
            },
            scratch_policy: ScratchPolicy {
                default_mode: ScratchMode::DurableOnly,
                env_vars: Vec::new(),
                optional_node_local: false,
                requires_explicit_request: false,
            },
            submission_policy: SubmissionPolicy {
                preferred_mode: SubmissionMode::DirectRuntime,
                prefer_allocation_reuse: false,
                prefer_job_arrays: false,
                allow_nested_scheduler_submit: false,
                max_scheduler_jobs_per_campaign: 0,
                max_nested_scheduler_jobs: 0,
                scheduler_poll_interval_secs: 1,
            },
            default_worker_tags: vec!["brain".into(), "muscle".into()],
        }
    }

    pub fn archer2() -> Self {
        Self {
            site_name: "archer2".into(),
            scheduler_family: SchedulerFamily::Slurm,
            launcher: LauncherPolicy {
                strategy: LaunchStrategy::Srun,
                launch_command: "srun".into(),
                submit_command: Some("sbatch".into()),
                status_command: Some("squeue".into()),
                accounting_command: Some("sacct".into()),
                cancel_command: Some("scancel".into()),
                telemetry_command: Some("sstat".into()),
                scheduler_job_id_env: Some("SLURM_JOB_ID".into()),
                module_environment: ModuleEnvironment::EnvironmentModules,
            },
            scratch_policy: ScratchPolicy {
                default_mode: ScratchMode::SharedFilesystem,
                env_vars: vec!["TMPDIR".into(), "SCRATCHDIR".into()],
                optional_node_local: true,
                requires_explicit_request: true,
            },
            submission_policy: SubmissionPolicy {
                preferred_mode: SubmissionMode::WithinAllocation,
                prefer_allocation_reuse: true,
                prefer_job_arrays: true,
                allow_nested_scheduler_submit: false,
                max_scheduler_jobs_per_campaign: 64,
                max_nested_scheduler_jobs: 0,
                scheduler_poll_interval_secs: 20,
            },
            default_worker_tags: vec![
                "brain".into(),
                "muscle".into(),
                "slurm".into(),
                "archer2".into(),
            ],
        }
    }

    pub fn young() -> Self {
        Self {
            site_name: "young".into(),
            scheduler_family: SchedulerFamily::GridEngine,
            launcher: LauncherPolicy {
                strategy: LaunchStrategy::Direct,
                launch_command: "direct".into(),
                submit_command: Some("qsub".into()),
                status_command: Some("qstat".into()),
                accounting_command: Some("qacct".into()),
                cancel_command: Some("qdel".into()),
                telemetry_command: None,
                scheduler_job_id_env: Some("JOB_ID".into()),
                module_environment: ModuleEnvironment::EnvironmentModules,
            },
            scratch_policy: ScratchPolicy {
                default_mode: ScratchMode::SharedFilesystem,
                env_vars: Vec::new(),
                optional_node_local: false,
                requires_explicit_request: false,
            },
            submission_policy: SubmissionPolicy {
                preferred_mode: SubmissionMode::WithinAllocation,
                prefer_allocation_reuse: true,
                prefer_job_arrays: false,
                allow_nested_scheduler_submit: false,
                max_scheduler_jobs_per_campaign: 32,
                max_nested_scheduler_jobs: 0,
                scheduler_poll_interval_secs: 30,
            },
            default_worker_tags: vec![
                "brain".into(),
                "muscle".into(),
                "grid_engine".into(),
                "young".into(),
            ],
        }
    }

    pub fn generic_slurm(site_name: impl Into<String>) -> Self {
        Self {
            site_name: site_name.into(),
            scheduler_family: SchedulerFamily::Slurm,
            launcher: LauncherPolicy {
                strategy: LaunchStrategy::Srun,
                launch_command: "srun".into(),
                submit_command: Some("sbatch".into()),
                status_command: Some("squeue".into()),
                accounting_command: Some("sacct".into()),
                cancel_command: Some("scancel".into()),
                telemetry_command: Some("sstat".into()),
                scheduler_job_id_env: Some("SLURM_JOB_ID".into()),
                module_environment: ModuleEnvironment::EnvironmentModules,
            },
            scratch_policy: ScratchPolicy {
                default_mode: ScratchMode::SharedFilesystem,
                env_vars: vec!["TMPDIR".into(), "SCRATCH".into()],
                optional_node_local: true,
                requires_explicit_request: false,
            },
            submission_policy: SubmissionPolicy {
                preferred_mode: SubmissionMode::WithinAllocation,
                prefer_allocation_reuse: true,
                prefer_job_arrays: true,
                allow_nested_scheduler_submit: false,
                max_scheduler_jobs_per_campaign: 64,
                max_nested_scheduler_jobs: 0,
                scheduler_poll_interval_secs: 15,
            },
            default_worker_tags: vec!["brain".into(), "muscle".into(), "slurm".into()],
        }
    }

    pub fn generic_pbs(site_name: impl Into<String>) -> Self {
        Self {
            site_name: site_name.into(),
            scheduler_family: SchedulerFamily::Pbs,
            launcher: LauncherPolicy {
                strategy: LaunchStrategy::PbsMpi,
                launch_command: "mpirun".into(),
                submit_command: Some("qsub".into()),
                status_command: Some("qstat".into()),
                accounting_command: None,
                cancel_command: Some("qdel".into()),
                telemetry_command: None,
                scheduler_job_id_env: Some("PBS_JOBID".into()),
                module_environment: ModuleEnvironment::EnvironmentModules,
            },
            scratch_policy: ScratchPolicy {
                default_mode: ScratchMode::SharedFilesystem,
                env_vars: vec!["TMPDIR".into()],
                optional_node_local: false,
                requires_explicit_request: false,
            },
            submission_policy: SubmissionPolicy {
                preferred_mode: SubmissionMode::WithinAllocation,
                prefer_allocation_reuse: true,
                prefer_job_arrays: false,
                allow_nested_scheduler_submit: false,
                max_scheduler_jobs_per_campaign: 32,
                max_nested_scheduler_jobs: 0,
                scheduler_poll_interval_secs: 30,
            },
            default_worker_tags: vec!["brain".into(), "muscle".into(), "pbs".into()],
        }
    }

    pub fn generic_grid_engine(site_name: impl Into<String>) -> Self {
        Self {
            site_name: site_name.into(),
            scheduler_family: SchedulerFamily::GridEngine,
            launcher: LauncherPolicy {
                strategy: LaunchStrategy::Direct,
                launch_command: "direct".into(),
                submit_command: Some("qsub".into()),
                status_command: Some("qstat".into()),
                accounting_command: Some("qacct".into()),
                cancel_command: Some("qdel".into()),
                telemetry_command: None,
                scheduler_job_id_env: Some("JOB_ID".into()),
                module_environment: ModuleEnvironment::EnvironmentModules,
            },
            scratch_policy: ScratchPolicy {
                default_mode: ScratchMode::SharedFilesystem,
                env_vars: Vec::new(),
                optional_node_local: false,
                requires_explicit_request: false,
            },
            submission_policy: SubmissionPolicy {
                preferred_mode: SubmissionMode::WithinAllocation,
                prefer_allocation_reuse: true,
                prefer_job_arrays: false,
                allow_nested_scheduler_submit: false,
                max_scheduler_jobs_per_campaign: 32,
                max_nested_scheduler_jobs: 0,
                scheduler_poll_interval_secs: 30,
            },
            default_worker_tags: vec!["brain".into(), "muscle".into(), "grid_engine".into()],
        }
    }

    pub fn launch_strategy(&self) -> LaunchStrategy {
        self.launcher.strategy
    }

    pub fn scheduler_job_id_env(&self) -> Option<&str> {
        self.launcher.scheduler_job_id_env.as_deref()
    }

    pub fn scratch_env_vars(&self) -> &[String] {
        &self.scratch_policy.env_vars
    }

    pub fn launcher_provenance(&self) -> LauncherProvenance {
        LauncherProvenance {
            site_name: self.site_name.clone(),
            scheduler_family: self.scheduler_family,
            launch_strategy: self.launcher.strategy,
            launch_command: self.launcher.launch_command.clone(),
            submit_command: self.launcher.submit_command.clone(),
            status_command: self.launcher.status_command.clone(),
            accounting_command: self.launcher.accounting_command.clone(),
            cancel_command: self.launcher.cancel_command.clone(),
            telemetry_command: self.launcher.telemetry_command.clone(),
            scheduler_job_id_env: self.launcher.scheduler_job_id_env.clone(),
            module_environment: self.launcher.module_environment,
            scratch_mode: self.scratch_policy.default_mode,
            scratch_env_vars: self.scratch_policy.env_vars.clone(),
            preferred_submission_mode: self.submission_policy.preferred_mode,
            allow_nested_scheduler_submit: self.submission_policy.allow_nested_scheduler_submit,
            max_nested_scheduler_jobs: self.submission_policy.max_nested_scheduler_jobs,
            poll_interval_secs: self.submission_policy.scheduler_poll_interval_secs,
        }
    }

    pub fn runtime_launch_provenance(&self) -> RuntimeLaunchProvenance {
        RuntimeLaunchProvenance {
            site_name: Some(self.site_name.clone()),
            scheduler_family: Some(
                match self.scheduler_family {
                    SchedulerFamily::Local => "local",
                    SchedulerFamily::Slurm => "slurm",
                    SchedulerFamily::Pbs => "pbs",
                    SchedulerFamily::GridEngine => "grid_engine",
                    SchedulerFamily::Unknown => "unknown",
                }
                .into(),
            ),
            launch_strategy: Some(
                match self.launcher.strategy {
                    LaunchStrategy::Direct => "direct",
                    LaunchStrategy::Srun => "srun",
                    LaunchStrategy::Mpirun => "mpirun",
                    LaunchStrategy::PbsMpi => "pbs_mpi",
                }
                .into(),
            ),
            launch_command: Some(self.launcher.launch_command.clone()),
            submit_command: self.launcher.submit_command.clone(),
            status_command: self.launcher.status_command.clone(),
            accounting_command: self.launcher.accounting_command.clone(),
            cancel_command: self.launcher.cancel_command.clone(),
            telemetry_command: self.launcher.telemetry_command.clone(),
            scheduler_job_id_env: self.launcher.scheduler_job_id_env.clone(),
            module_environment: Some(
                match self.launcher.module_environment {
                    ModuleEnvironment::None => "none",
                    ModuleEnvironment::EnvironmentModules => "environment_modules",
                    ModuleEnvironment::Lmod => "lmod",
                }
                .into(),
            ),
            scratch_mode: Some(
                match self.scratch_policy.default_mode {
                    ScratchMode::DurableOnly => "durable_only",
                    ScratchMode::SharedFilesystem => "shared_filesystem",
                    ScratchMode::NodeLocal => "node_local",
                }
                .into(),
            ),
            preferred_submission_mode: Some(
                match self.submission_policy.preferred_mode {
                    SubmissionMode::DirectRuntime => "direct_runtime",
                    SubmissionMode::WithinAllocation => "within_allocation",
                    SubmissionMode::JobArray => "job_array",
                    SubmissionMode::NestedBatch => "nested_batch",
                }
                .into(),
            ),
            poll_interval_secs: Some(self.submission_policy.scheduler_poll_interval_secs),
        }
    }

    pub fn detect_from_host_env(
        hostname: &str,
        has_slurm: bool,
        has_pbs: bool,
        has_grid_engine: bool,
    ) -> Self {
        let lowered = hostname.to_ascii_lowercase();
        if lowered.contains("archer2") || lowered.starts_with("nid") {
            return Self::archer2();
        }
        if lowered.contains("young") {
            return Self::young();
        }
        if has_slurm {
            return Self::generic_slurm(hostname);
        }
        if has_grid_engine {
            return Self::generic_grid_engine(hostname);
        }
        if has_pbs {
            return Self::generic_pbs(hostname);
        }
        Self::local()
    }

    pub fn detect_current() -> Self {
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "local".into());
        let has_slurm = std::env::var("SLURM_JOB_ID").is_ok();
        let has_pbs = std::env::var("PBS_JOBID").is_ok();
        let has_grid_engine =
            std::env::var("JOB_ID").is_ok() || std::env::var("SGE_JOB_ID").is_ok();
        Self::detect_from_host_env(&hostname, has_slurm, has_pbs, has_grid_engine)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LaunchStrategy, ModuleEnvironment, SchedulerFamily, ScratchMode, SiteProfile,
        SubmissionMode,
    };

    #[test]
    fn detects_archer2_from_hostname() {
        let profile = SiteProfile::detect_from_host_env("nid001234", true, false, false);
        assert_eq!(profile.site_name, "archer2");
        assert_eq!(profile.scheduler_family, SchedulerFamily::Slurm);
        assert_eq!(profile.launch_strategy(), LaunchStrategy::Srun);
        assert_eq!(
            profile.submission_policy.preferred_mode,
            SubmissionMode::WithinAllocation
        );
        assert_eq!(profile.launcher.telemetry_command.as_deref(), Some("sstat"));
    }

    #[test]
    fn detects_young_from_hostname() {
        let profile = SiteProfile::detect_from_host_env("young-login01", false, false, true);
        assert_eq!(profile.site_name, "young");
        assert_eq!(profile.scheduler_family, SchedulerFamily::GridEngine);
        assert_eq!(profile.launch_strategy(), LaunchStrategy::Direct);
        assert_eq!(profile.launcher.submit_command.as_deref(), Some("qsub"));
        assert_eq!(profile.launcher.status_command.as_deref(), Some("qstat"));
        assert_eq!(
            profile.scratch_policy.default_mode,
            ScratchMode::SharedFilesystem
        );
        assert!(profile.scratch_policy.env_vars.is_empty());
        assert!(!profile.scratch_policy.optional_node_local);
        assert!(!profile.scratch_policy.requires_explicit_request);
    }

    #[test]
    fn falls_back_to_local_without_scheduler_signals() {
        let profile = SiteProfile::detect_from_host_env("my-macbook", false, false, false);
        assert_eq!(profile.scheduler_family, SchedulerFamily::Local);
        assert_eq!(profile.launch_strategy(), LaunchStrategy::Direct);
        assert_eq!(
            profile.submission_policy.preferred_mode,
            SubmissionMode::DirectRuntime
        );
    }

    #[test]
    fn falls_back_to_generic_grid_engine_when_only_job_id_is_known() {
        let profile = SiteProfile::detect_from_host_env("cluster-login", false, false, true);
        assert_eq!(profile.scheduler_family, SchedulerFamily::GridEngine);
        assert_eq!(profile.launcher.submit_command.as_deref(), Some("qsub"));
        assert_eq!(profile.launcher.cancel_command.as_deref(), Some("qdel"));
    }

    #[test]
    fn launcher_provenance_captures_site_execution_policy() {
        let provenance = SiteProfile::archer2().launcher_provenance();
        assert_eq!(provenance.site_name, "archer2");
        assert_eq!(provenance.launch_command, "srun");
        assert_eq!(
            provenance.module_environment,
            ModuleEnvironment::EnvironmentModules
        );
        assert_eq!(
            provenance.preferred_submission_mode,
            SubmissionMode::WithinAllocation
        );
        assert!(!provenance.allow_nested_scheduler_submit);
    }

    #[test]
    fn runtime_launch_provenance_projects_site_policy_without_ulab_types() {
        let provenance = SiteProfile::young().runtime_launch_provenance();
        assert_eq!(provenance.site_name.as_deref(), Some("young"));
        assert_eq!(provenance.scheduler_family.as_deref(), Some("grid_engine"));
        assert_eq!(provenance.launch_strategy.as_deref(), Some("direct"));
        assert_eq!(provenance.submit_command.as_deref(), Some("qsub"));
        assert_eq!(provenance.accounting_command.as_deref(), Some("qacct"));
        assert_eq!(
            provenance.preferred_submission_mode.as_deref(),
            Some("within_allocation")
        );
    }
}
