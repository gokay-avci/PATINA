use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use patina_runtime::{
    RuntimeDiagnostics, ScottDispatchReceipt, ScottRuntime, ScottRuntimeError, ScottRuntimeJob,
    ScottRuntimeReport,
};

use crate::{
    ExternalBatchReceipt, LeaseState, SchedulerFamily, SchedulerJobIdentifier,
    SchedulerReceiptState, SiteProfile, TelemetrySample, WorkLease, WorkspacePlan,
};

/// Optional placement hints owned by the HPC adapter layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchedulerPlacement {
    pub queue: Option<String>,
    pub account: Option<String>,
    pub reservation: Option<String>,
    pub node_feature: Option<String>,
    pub launcher_argv: Vec<String>,
    pub extra_submit_args: Vec<String>,
}

/// Scheduler-facing trait implemented by `patina-ulab` adapters.
///
/// The important rule is that this trait consumes Scott runtime jobs and returns Scott runtime
/// receipts/reports. It must not redefine Scott scientific meaning.
pub trait UlabScheduler: Send + Sync {
    fn site_profile(&self) -> &SiteProfile;

    fn submit(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ScottDispatchReceipt, SchedulerAdapterError>;

    fn poll(
        &self,
        receipt: &ScottDispatchReceipt,
    ) -> Result<Option<ScottRuntimeReport>, SchedulerAdapterError>;

    fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), SchedulerAdapterError>;
}

/// Thin adapter that lets `patina-ulab` drive any scheduler-neutral `ScottRuntime`.
///
/// This is the first local execution seam for end-to-end testing. It preserves Scott runtime
/// ownership while enforcing `ulab`-side lease and workspace consistency before submission.
#[derive(Clone)]
pub struct RuntimeBackedScheduler {
    site_profile: SiteProfile,
    runtime: Arc<dyn ScottRuntime>,
}

impl RuntimeBackedScheduler {
    pub fn local(runtime: Arc<dyn ScottRuntime>) -> Self {
        Self::new(SiteProfile::local(), runtime)
    }

    pub fn new(site_profile: SiteProfile, runtime: Arc<dyn ScottRuntime>) -> Self {
        Self {
            site_profile,
            runtime,
        }
    }

    fn validate_submission(
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
    ) -> Result<(), SchedulerAdapterError> {
        if lease.job_id != job.identity.job_id {
            return Err(SchedulerAdapterError::LeaseJobMismatch {
                lease_job_id: lease.job_id.clone(),
                runtime_job_id: job.identity.job_id.clone(),
            });
        }

        if workspace.job_id != job.identity.job_id {
            return Err(SchedulerAdapterError::WorkspaceJobMismatch {
                workspace_job_id: workspace.job_id.clone(),
                runtime_job_id: job.identity.job_id.clone(),
            });
        }

        if !matches!(
            lease.state,
            LeaseState::Offered | LeaseState::Accepted | LeaseState::Active
        ) {
            return Err(SchedulerAdapterError::InactiveLease {
                lease_id: lease.lease_id.clone(),
                job_id: job.identity.job_id.clone(),
                state: lease.state,
            });
        }

        Ok(())
    }
}

impl std::fmt::Debug for RuntimeBackedScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeBackedScheduler")
            .field("site_profile", &self.site_profile)
            .finish()
    }
}

impl UlabScheduler for RuntimeBackedScheduler {
    fn site_profile(&self) -> &SiteProfile {
        &self.site_profile
    }

    fn submit(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        _placement: &SchedulerPlacement,
    ) -> Result<ScottDispatchReceipt, SchedulerAdapterError> {
        Self::validate_submission(lease, job, workspace)?;
        self.runtime.submit(job).map_err(Into::into)
    }

    fn poll(
        &self,
        receipt: &ScottDispatchReceipt,
    ) -> Result<Option<ScottRuntimeReport>, SchedulerAdapterError> {
        self.runtime.poll(receipt).map_err(Into::into)
    }

    fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), SchedulerAdapterError> {
        self.runtime.cancel(receipt).map_err(Into::into)
    }
}

/// Minimal scheduler command representation used by typed adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// Captured stdout/stderr from a scheduler command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status_code: i32,
}

/// Narrow runner interface that can be replaced by a real process-backed implementation later.
pub trait SlurmCommandRunner: Send + Sync {
    fn run(
        &self,
        command: &SchedulerCommand,
    ) -> Result<SchedulerCommandOutput, SchedulerAdapterError>;
}

/// Narrow runner interface for Grid Engine style command execution.
pub trait GridEngineCommandRunner: Send + Sync {
    fn run(
        &self,
        command: &SchedulerCommand,
    ) -> Result<SchedulerCommandOutput, SchedulerAdapterError>;
}

/// Narrow runner interface for PBS-family command execution.
pub trait PbsCommandRunner: Send + Sync {
    fn run(
        &self,
        command: &SchedulerCommand,
    ) -> Result<SchedulerCommandOutput, SchedulerAdapterError>;
}

#[derive(Debug, Clone, PartialEq)]
struct SlurmReceiptObservation {
    scheduler_job: SchedulerJobIdentifier,
    state: SchedulerReceiptState,
    telemetry: Option<TelemetrySample>,
}

#[derive(Debug, Clone, PartialEq)]
struct GridEngineReceiptObservation {
    scheduler_job: SchedulerJobIdentifier,
    state: SchedulerReceiptState,
}

#[derive(Debug, Clone, PartialEq)]
struct PbsReceiptObservation {
    scheduler_job: SchedulerJobIdentifier,
    state: SchedulerReceiptState,
}

/// Scheduler adapter that owns Slurm command construction, receipt parsing, and receipt tracking.
///
/// It intentionally does not fabricate Scott scientific reports. `poll()` only updates scheduler
/// receipt state and returns `None` until a later external-runtime integration turns terminal
/// scheduler state into parsed Scott results.
pub struct SlurmSchedulerAdapter {
    site_profile: SiteProfile,
    runner: Arc<dyn SlurmCommandRunner>,
    receipts: Mutex<HashMap<String, ExternalBatchReceipt>>,
}

/// Scheduler adapter that owns Grid Engine command construction, receipt parsing, and receipt tracking.
///
/// It intentionally mirrors the Slurm adapter discipline:
/// scheduler-facing state is tracked here, while Scott scientific completion remains upstream.
pub struct GridEngineSchedulerAdapter {
    site_profile: SiteProfile,
    runner: Arc<dyn GridEngineCommandRunner>,
    receipts: Mutex<HashMap<String, ExternalBatchReceipt>>,
}

/// Scheduler adapter for PBS-family sites.
///
/// PBS shares the same `UlabScheduler` and `ExternalBatchReceipt` boundary as Slurm and Grid
/// Engine. The scheduler-specific part is only command construction and receipt parsing.
pub struct PbsSchedulerAdapter {
    site_profile: SiteProfile,
    runner: Arc<dyn PbsCommandRunner>,
    receipts: Mutex<HashMap<String, ExternalBatchReceipt>>,
}

impl SlurmSchedulerAdapter {
    pub fn new(
        site_profile: SiteProfile,
        runner: Arc<dyn SlurmCommandRunner>,
    ) -> Result<Self, SchedulerAdapterError> {
        if site_profile.scheduler_family != SchedulerFamily::Slurm {
            return Err(SchedulerAdapterError::UnsupportedSchedulerFamily {
                site_name: site_profile.site_name.clone(),
                scheduler_family: site_profile.scheduler_family,
            });
        }

        Ok(Self {
            site_profile,
            runner,
            receipts: Mutex::new(HashMap::new()),
        })
    }

    pub fn build_submit_command(
        &self,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let submit_program = self
            .site_profile
            .launcher
            .submit_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "submit",
            })?;

        if placement.launcher_argv.is_empty() {
            return Err(SchedulerAdapterError::MissingLaunchPayload {
                job_id: job.identity.job_id.clone(),
            });
        }

        let mut args = vec![
            "--parsable".into(),
            "--job-name".into(),
            job.identity.job_id.clone(),
            "--chdir".into(),
            workspace.stage_root.to_string(),
            "--cpus-per-task".into(),
            job.requested_cores.to_string(),
        ];

        if job.requested_gpus > 0 {
            args.push("--gpus-per-task".into());
            args.push(job.requested_gpus.to_string());
        }
        if let Some(queue) = &placement.queue {
            args.push("--partition".into());
            args.push(queue.clone());
        }
        if let Some(account) = &placement.account {
            args.push("--account".into());
            args.push(account.clone());
        }
        if let Some(reservation) = &placement.reservation {
            args.push("--reservation".into());
            args.push(reservation.clone());
        }
        if let Some(constraint) = &placement.node_feature {
            args.push("--constraint".into());
            args.push(constraint.clone());
        }
        args.extend(placement.extra_submit_args.iter().cloned());
        args.push("--wrap".into());
        args.push(self.wrap_command(placement));

        Ok(SchedulerCommand {
            program: submit_program,
            args,
        })
    }

    pub fn submit_batch_receipt(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        RuntimeBackedScheduler::validate_submission(lease, job, workspace)?;
        let command = self.build_submit_command(job, workspace, placement)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;

        let allocation_id = parse_sbatch_allocation_id(&output.stdout)?;
        let scheduler_job = parse_slurm_job_identifier(&allocation_id);
        let now_ms = now_ms();
        let receipt = ExternalBatchReceipt {
            receipt_id: format!("slurm:{}:{}", allocation_id, job.identity.job_id),
            job_id: job.identity.job_id.clone(),
            scheduler_job,
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: now_ms,
            last_observed_at_ms: now_ms,
            launch_host: launch_host(),
            workdir: Some(workspace.stage_root.to_string()),
            launcher_provenance: Some(self.site_profile.launcher_provenance()),
            last_telemetry: None,
        };
        self.store_receipt(receipt.clone())?;
        Ok(receipt)
    }

    pub fn poll_batch_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let status_command = self.build_status_command(receipt)?;
        let status_output = self.runner.run(&status_command)?;
        ensure_success(&status_command, &status_output)?;

        let observation = if let Some(active_line) = first_nonempty_line(&status_output.stdout) {
            let mut observation = parse_squeue_line(active_line)?;
            observation.telemetry = self.poll_telemetry(receipt)?;
            observation
        } else {
            let accounting_command = self.build_accounting_command(receipt)?;
            let accounting_output = self.runner.run(&accounting_command)?;
            ensure_success(&accounting_command, &accounting_output)?;
            let accounting_line =
                first_nonempty_line(&accounting_output.stdout).ok_or_else(|| {
                    SchedulerAdapterError::MissingReceiptObservation {
                        receipt_id: receipt.receipt_id.clone(),
                        scheduler_job_id: receipt.scheduler_job.allocation_id.clone(),
                    }
                })?;
            parse_sacct_line(accounting_line)?
        };

        let mut updated = receipt.clone();
        updated.scheduler_job = observation.scheduler_job;
        updated.mark_observed(now_ms(), observation.state);
        if let Some(telemetry) = observation.telemetry {
            updated.attach_telemetry(telemetry);
        }
        self.store_receipt(updated.clone())?;
        Ok(updated)
    }

    pub fn cancel_batch_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let command = self.build_cancel_command(receipt)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;

        let mut updated = receipt.clone();
        updated.mark_observed(now_ms(), SchedulerReceiptState::Cancelled);
        self.store_receipt(updated.clone())?;
        Ok(updated)
    }

    pub fn receipt_for_handle(
        &self,
        runtime_handle: &str,
    ) -> Result<Option<ExternalBatchReceipt>, SchedulerAdapterError> {
        let receipts = self.lock_receipts()?;
        Ok(receipts.get(runtime_handle).cloned())
    }

    pub fn runtime_diagnostics_for_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> RuntimeDiagnostics {
        let mut diagnostics = RuntimeDiagnostics::default()
            .with_launch_provenance(self.site_profile.runtime_launch_provenance());
        diagnostics.workdir = receipt.workdir.as_ref().map(Into::into);
        diagnostics.message = Some(format!(
            "scheduler receipt `{}` observed in state `{}`",
            receipt.receipt_id,
            scheduler_state_label(receipt.state)
        ));
        diagnostics.metrics.insert(
            "scheduler_family".into(),
            match receipt.scheduler_job.scheduler_family {
                SchedulerFamily::Local => "local",
                SchedulerFamily::Slurm => "slurm",
                SchedulerFamily::Pbs => "pbs",
                SchedulerFamily::GridEngine => "grid_engine",
                SchedulerFamily::Unknown => "unknown",
            }
            .into(),
        );
        diagnostics.metrics.insert(
            "scheduler_allocation_id".into(),
            receipt.scheduler_job.allocation_id.clone(),
        );
        diagnostics.metrics.insert(
            "scheduler_state".into(),
            scheduler_state_label(receipt.state).into(),
        );
        if let Some(launch_host) = &receipt.launch_host {
            diagnostics
                .metrics
                .insert("launch_host".into(), launch_host.clone());
        }
        if let Some(step_id) = &receipt.scheduler_job.step_id {
            diagnostics
                .metrics
                .insert("scheduler_step_id".into(), step_id.clone());
        }
        if let Some(array_job_id) = &receipt.scheduler_job.array_job_id {
            diagnostics
                .metrics
                .insert("scheduler_array_job_id".into(), array_job_id.clone());
        }
        if let Some(array_index) = receipt.scheduler_job.array_index {
            diagnostics
                .metrics
                .insert("scheduler_array_index".into(), array_index.to_string());
        }
        if let Some(telemetry) = &receipt.last_telemetry {
            if let Some(rss_mb) = telemetry.rss_mb {
                diagnostics
                    .metrics
                    .insert("telemetry_rss_mb".into(), rss_mb.to_string());
            }
            if let Some(vm_mb) = telemetry.vm_mb {
                diagnostics
                    .metrics
                    .insert("telemetry_vm_mb".into(), vm_mb.to_string());
            }
            if let Some(cpu_time_secs) = telemetry.cpu_time_secs {
                diagnostics.metrics.insert(
                    "telemetry_cpu_time_secs".into(),
                    format!("{cpu_time_secs:.3}"),
                );
            }
            if let Some(gpu_util_percent) = telemetry.gpu_util_percent {
                diagnostics.metrics.insert(
                    "telemetry_gpu_util_percent".into(),
                    format!("{gpu_util_percent:.3}"),
                );
            }
            if !telemetry.node_list.is_empty() {
                diagnostics
                    .metrics
                    .insert("telemetry_nodes".into(), telemetry.node_list.join(","));
            }
        }
        diagnostics
    }

    fn store_receipt(&self, receipt: ExternalBatchReceipt) -> Result<(), SchedulerAdapterError> {
        let mut receipts = self.lock_receipts()?;
        receipts.insert(receipt.receipt_id.clone(), receipt);
        Ok(())
    }

    fn load_receipt(
        &self,
        runtime_handle: &str,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let receipts = self.lock_receipts()?;
        receipts.get(runtime_handle).cloned().ok_or_else(|| {
            SchedulerAdapterError::UnknownReceiptHandle {
                runtime_handle: runtime_handle.into(),
            }
        })
    }

    fn lock_receipts(
        &self,
    ) -> Result<
        std::sync::MutexGuard<'_, HashMap<String, ExternalBatchReceipt>>,
        SchedulerAdapterError,
    > {
        self.receipts
            .lock()
            .map_err(|_| SchedulerAdapterError::Internal("slurm receipt registry lock poisoned"))
    }

    fn wrap_command(&self, placement: &SchedulerPlacement) -> String {
        let mut argv = vec![self.site_profile.launcher.launch_command.clone()];
        argv.extend(placement.launcher_argv.iter().cloned());
        argv.into_iter()
            .map(|arg| shell_quote(&arg))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn build_status_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .status_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "status",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec![
                "--noheader".into(),
                "--format=%T|%i".into(),
                "--jobs".into(),
                receipt.scheduler_job.allocation_id.clone(),
            ],
        })
    }

    fn build_accounting_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .accounting_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "accounting",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec![
                "--noheader".into(),
                "--parsable2".into(),
                "--format=State,JobID".into(),
                "--jobs".into(),
                receipt.scheduler_job.allocation_id.clone(),
            ],
        })
    }

    fn build_cancel_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .cancel_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "cancel",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec![receipt.scheduler_job.allocation_id.clone()],
        })
    }

    fn poll_telemetry(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<Option<TelemetrySample>, SchedulerAdapterError> {
        let Some(program) = self.site_profile.launcher.telemetry_command.clone() else {
            return Ok(None);
        };

        let command = SchedulerCommand {
            program,
            args: vec![
                "--noheader".into(),
                "--parsable2".into(),
                "--format=MaxRSS,AveVMSize,AveCPU,NodeList".into(),
                "--jobs".into(),
                receipt.scheduler_job.allocation_id.clone(),
            ],
        };

        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;
        Ok(parse_sstat_output(&output.stdout))
    }
}

impl GridEngineSchedulerAdapter {
    pub fn new(
        site_profile: SiteProfile,
        runner: Arc<dyn GridEngineCommandRunner>,
    ) -> Result<Self, SchedulerAdapterError> {
        if site_profile.scheduler_family != SchedulerFamily::GridEngine {
            return Err(SchedulerAdapterError::UnsupportedSchedulerFamily {
                site_name: site_profile.site_name.clone(),
                scheduler_family: site_profile.scheduler_family,
            });
        }

        Ok(Self {
            site_profile,
            runner,
            receipts: Mutex::new(HashMap::new()),
        })
    }

    pub fn build_submit_command(
        &self,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let submit_program = self
            .site_profile
            .launcher
            .submit_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "submit",
            })?;

        if placement.launcher_argv.is_empty() {
            return Err(SchedulerAdapterError::MissingLaunchPayload {
                job_id: job.identity.job_id.clone(),
            });
        }

        let mut args = vec![
            "-terse".into(),
            "-N".into(),
            job.identity.job_id.clone(),
            "-wd".into(),
            workspace.stage_root.to_string(),
        ];

        if job.requested_cores > 1 {
            args.push("-pe".into());
            args.push("mpi".into());
            args.push(job.requested_cores.to_string());
        }

        if let Some(queue) = &placement.queue {
            args.push("-q".into());
            args.push(queue.clone());
        }
        if let Some(account) = &placement.account {
            args.push("-A".into());
            args.push(account.clone());
        }
        if let Some(project) = &placement.reservation {
            args.push("-P".into());
            args.push(project.clone());
        }
        if let Some(feature) = &placement.node_feature {
            args.push("-l".into());
            args.push(feature.clone());
        }

        args.extend(placement.extra_submit_args.iter().cloned());
        args.push("-b".into());
        args.push("y".into());
        args.extend(placement.launcher_argv.iter().cloned());

        Ok(SchedulerCommand {
            program: submit_program,
            args,
        })
    }

    pub fn submit_batch_receipt(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        RuntimeBackedScheduler::validate_submission(lease, job, workspace)?;
        let command = self.build_submit_command(job, workspace, placement)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;

        let allocation_id = parse_qsub_allocation_id(&output.stdout)?;
        let scheduler_job = parse_grid_engine_job_identifier(&allocation_id);
        let now_ms = now_ms();
        let receipt = ExternalBatchReceipt {
            receipt_id: format!("grid_engine:{}:{}", allocation_id, job.identity.job_id),
            job_id: job.identity.job_id.clone(),
            scheduler_job,
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: now_ms,
            last_observed_at_ms: now_ms,
            launch_host: launch_host(),
            workdir: Some(workspace.stage_root.to_string()),
            launcher_provenance: Some(self.site_profile.launcher_provenance()),
            last_telemetry: None,
        };
        self.store_receipt(receipt.clone())?;
        Ok(receipt)
    }

    pub fn poll_batch_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let status_command = self.build_status_command(receipt)?;
        let status_output = self.runner.run(&status_command)?;
        ensure_success(&status_command, &status_output)?;

        let observation = if let Some(active_line) = first_nonempty_line(&status_output.stdout) {
            parse_qstat_line(active_line)?
        } else {
            let accounting_command = self.build_accounting_command(receipt)?;
            let accounting_output = self.runner.run(&accounting_command)?;
            ensure_success(&accounting_command, &accounting_output)?;
            parse_qacct_output(receipt, &accounting_output.stdout)?
        };

        let mut updated = receipt.clone();
        updated.scheduler_job = observation.scheduler_job;
        updated.mark_observed(now_ms(), observation.state);
        self.store_receipt(updated.clone())?;
        Ok(updated)
    }

    pub fn cancel_batch_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let command = self.build_cancel_command(receipt)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;

        let mut updated = receipt.clone();
        updated.mark_observed(now_ms(), SchedulerReceiptState::Cancelled);
        self.store_receipt(updated.clone())?;
        Ok(updated)
    }

    pub fn receipt_for_handle(
        &self,
        runtime_handle: &str,
    ) -> Result<Option<ExternalBatchReceipt>, SchedulerAdapterError> {
        let receipts = self.lock_receipts()?;
        Ok(receipts.get(runtime_handle).cloned())
    }

    pub fn runtime_diagnostics_for_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> RuntimeDiagnostics {
        let mut diagnostics = RuntimeDiagnostics::default()
            .with_launch_provenance(self.site_profile.runtime_launch_provenance());
        diagnostics.workdir = receipt.workdir.as_ref().map(Into::into);
        diagnostics.message = Some(format!(
            "scheduler receipt `{}` observed in state `{}`",
            receipt.receipt_id,
            scheduler_state_label(receipt.state)
        ));
        diagnostics
            .metrics
            .insert("scheduler_family".into(), "grid_engine".into());
        diagnostics.metrics.insert(
            "scheduler_allocation_id".into(),
            receipt.scheduler_job.allocation_id.clone(),
        );
        diagnostics.metrics.insert(
            "scheduler_state".into(),
            scheduler_state_label(receipt.state).into(),
        );
        if let Some(launch_host) = &receipt.launch_host {
            diagnostics
                .metrics
                .insert("launch_host".into(), launch_host.clone());
        }
        diagnostics
    }

    fn store_receipt(&self, receipt: ExternalBatchReceipt) -> Result<(), SchedulerAdapterError> {
        let mut receipts = self.lock_receipts()?;
        receipts.insert(receipt.receipt_id.clone(), receipt);
        Ok(())
    }

    fn load_receipt(
        &self,
        runtime_handle: &str,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let receipts = self.lock_receipts()?;
        receipts.get(runtime_handle).cloned().ok_or_else(|| {
            SchedulerAdapterError::UnknownReceiptHandle {
                runtime_handle: runtime_handle.into(),
            }
        })
    }

    fn lock_receipts(
        &self,
    ) -> Result<
        std::sync::MutexGuard<'_, HashMap<String, ExternalBatchReceipt>>,
        SchedulerAdapterError,
    > {
        self.receipts.lock().map_err(|_| {
            SchedulerAdapterError::Internal("grid engine receipt registry lock poisoned")
        })
    }

    fn build_status_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .status_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "status",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec!["-j".into(), receipt.scheduler_job.allocation_id.clone()],
        })
    }

    fn build_accounting_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .accounting_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "accounting",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec!["-j".into(), receipt.scheduler_job.allocation_id.clone()],
        })
    }

    fn build_cancel_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .cancel_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "cancel",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec![receipt.scheduler_job.allocation_id.clone()],
        })
    }
}

impl PbsSchedulerAdapter {
    pub fn new(
        site_profile: SiteProfile,
        runner: Arc<dyn PbsCommandRunner>,
    ) -> Result<Self, SchedulerAdapterError> {
        if site_profile.scheduler_family != SchedulerFamily::Pbs {
            return Err(SchedulerAdapterError::UnsupportedSchedulerFamily {
                site_name: site_profile.site_name.clone(),
                scheduler_family: site_profile.scheduler_family,
            });
        }

        Ok(Self {
            site_profile,
            runner,
            receipts: Mutex::new(HashMap::new()),
        })
    }

    pub fn build_submit_command(
        &self,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let submit_program = self
            .site_profile
            .launcher
            .submit_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "submit",
            })?;

        if placement.launcher_argv.is_empty() {
            return Err(SchedulerAdapterError::MissingLaunchPayload {
                job_id: job.identity.job_id.clone(),
            });
        }

        let mut args = vec![
            "-N".into(),
            job.identity.job_id.clone(),
            "-d".into(),
            workspace.stage_root.to_string(),
            "-l".into(),
            format!("select=1:ncpus={}", job.requested_cores.max(1)),
        ];

        if let Some(queue) = &placement.queue {
            args.push("-q".into());
            args.push(queue.clone());
        }
        if let Some(account) = &placement.account {
            args.push("-A".into());
            args.push(account.clone());
        }
        if let Some(feature) = &placement.node_feature {
            args.push("-l".into());
            args.push(feature.clone());
        }
        args.extend(placement.extra_submit_args.iter().cloned());
        args.push("--".into());
        args.extend(placement.launcher_argv.iter().cloned());

        Ok(SchedulerCommand {
            program: submit_program,
            args,
        })
    }

    pub fn submit_batch_receipt(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        RuntimeBackedScheduler::validate_submission(lease, job, workspace)?;
        let command = self.build_submit_command(job, workspace, placement)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;

        let allocation_id = parse_pbs_qsub_allocation_id(&output.stdout)?;
        let scheduler_job = parse_pbs_job_identifier(&allocation_id);
        let now_ms = now_ms();
        let receipt = ExternalBatchReceipt {
            receipt_id: format!("pbs:{}:{}", allocation_id, job.identity.job_id),
            job_id: job.identity.job_id.clone(),
            scheduler_job,
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: now_ms,
            last_observed_at_ms: now_ms,
            launch_host: launch_host(),
            workdir: Some(workspace.stage_root.to_string()),
            launcher_provenance: Some(self.site_profile.launcher_provenance()),
            last_telemetry: None,
        };
        self.store_receipt(receipt.clone())?;
        Ok(receipt)
    }

    pub fn poll_batch_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let command = self.build_status_command(receipt)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;
        let observation = parse_pbs_qstat_output(receipt, &output.stdout)?;

        let mut updated = receipt.clone();
        updated.scheduler_job = observation.scheduler_job;
        updated.mark_observed(now_ms(), observation.state);
        self.store_receipt(updated.clone())?;
        Ok(updated)
    }

    pub fn cancel_batch_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let command = self.build_cancel_command(receipt)?;
        let output = self.runner.run(&command)?;
        ensure_success(&command, &output)?;

        let mut updated = receipt.clone();
        updated.mark_observed(now_ms(), SchedulerReceiptState::Cancelled);
        self.store_receipt(updated.clone())?;
        Ok(updated)
    }

    pub fn receipt_for_handle(
        &self,
        runtime_handle: &str,
    ) -> Result<Option<ExternalBatchReceipt>, SchedulerAdapterError> {
        let receipts = self.lock_receipts()?;
        Ok(receipts.get(runtime_handle).cloned())
    }

    fn store_receipt(&self, receipt: ExternalBatchReceipt) -> Result<(), SchedulerAdapterError> {
        let mut receipts = self.lock_receipts()?;
        receipts.insert(receipt.receipt_id.clone(), receipt);
        Ok(())
    }

    fn load_receipt(
        &self,
        runtime_handle: &str,
    ) -> Result<ExternalBatchReceipt, SchedulerAdapterError> {
        let receipts = self.lock_receipts()?;
        receipts.get(runtime_handle).cloned().ok_or_else(|| {
            SchedulerAdapterError::UnknownReceiptHandle {
                runtime_handle: runtime_handle.into(),
            }
        })
    }

    fn lock_receipts(
        &self,
    ) -> Result<
        std::sync::MutexGuard<'_, HashMap<String, ExternalBatchReceipt>>,
        SchedulerAdapterError,
    > {
        self.receipts
            .lock()
            .map_err(|_| SchedulerAdapterError::Internal("pbs receipt registry lock poisoned"))
    }

    fn build_status_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .status_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "status",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec!["-f".into(), receipt.scheduler_job.allocation_id.clone()],
        })
    }

    fn build_cancel_command(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<SchedulerCommand, SchedulerAdapterError> {
        let program = self
            .site_profile
            .launcher
            .cancel_command
            .clone()
            .ok_or_else(|| SchedulerAdapterError::MissingSchedulerCommand {
                site_name: self.site_profile.site_name.clone(),
                command_kind: "cancel",
            })?;

        Ok(SchedulerCommand {
            program,
            args: vec![receipt.scheduler_job.allocation_id.clone()],
        })
    }
}

impl std::fmt::Debug for SlurmSchedulerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlurmSchedulerAdapter")
            .field("site_profile", &self.site_profile)
            .finish()
    }
}

impl UlabScheduler for SlurmSchedulerAdapter {
    fn site_profile(&self) -> &SiteProfile {
        &self.site_profile
    }

    fn submit(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ScottDispatchReceipt, SchedulerAdapterError> {
        let receipt = self.submit_batch_receipt(lease, job, workspace, placement)?;
        Ok(ScottDispatchReceipt {
            job_id: job.identity.job_id.clone(),
            runtime_handle: receipt.receipt_id,
        })
    }

    fn poll(
        &self,
        receipt: &ScottDispatchReceipt,
    ) -> Result<Option<ScottRuntimeReport>, SchedulerAdapterError> {
        let batch_receipt = self.load_receipt(&receipt.runtime_handle)?;
        let _updated = self.poll_batch_receipt(&batch_receipt)?;
        Ok(None)
    }

    fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), SchedulerAdapterError> {
        let batch_receipt = self.load_receipt(&receipt.runtime_handle)?;
        let _updated = self.cancel_batch_receipt(&batch_receipt)?;
        Ok(())
    }
}

impl std::fmt::Debug for GridEngineSchedulerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridEngineSchedulerAdapter")
            .field("site_profile", &self.site_profile)
            .finish()
    }
}

impl std::fmt::Debug for PbsSchedulerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PbsSchedulerAdapter")
            .field("site_profile", &self.site_profile)
            .finish()
    }
}

impl UlabScheduler for GridEngineSchedulerAdapter {
    fn site_profile(&self) -> &SiteProfile {
        &self.site_profile
    }

    fn submit(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ScottDispatchReceipt, SchedulerAdapterError> {
        let receipt = self.submit_batch_receipt(lease, job, workspace, placement)?;
        Ok(ScottDispatchReceipt {
            job_id: job.identity.job_id.clone(),
            runtime_handle: receipt.receipt_id,
        })
    }

    fn poll(
        &self,
        receipt: &ScottDispatchReceipt,
    ) -> Result<Option<ScottRuntimeReport>, SchedulerAdapterError> {
        let batch_receipt = self.load_receipt(&receipt.runtime_handle)?;
        let _updated = self.poll_batch_receipt(&batch_receipt)?;
        Ok(None)
    }

    fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), SchedulerAdapterError> {
        let batch_receipt = self.load_receipt(&receipt.runtime_handle)?;
        let _updated = self.cancel_batch_receipt(&batch_receipt)?;
        Ok(())
    }
}

impl UlabScheduler for PbsSchedulerAdapter {
    fn site_profile(&self) -> &SiteProfile {
        &self.site_profile
    }

    fn submit(
        &self,
        lease: &WorkLease,
        job: &ScottRuntimeJob,
        workspace: &WorkspacePlan,
        placement: &SchedulerPlacement,
    ) -> Result<ScottDispatchReceipt, SchedulerAdapterError> {
        let receipt = self.submit_batch_receipt(lease, job, workspace, placement)?;
        Ok(ScottDispatchReceipt {
            job_id: job.identity.job_id.clone(),
            runtime_handle: receipt.receipt_id,
        })
    }

    fn poll(
        &self,
        receipt: &ScottDispatchReceipt,
    ) -> Result<Option<ScottRuntimeReport>, SchedulerAdapterError> {
        let batch_receipt = self.load_receipt(&receipt.runtime_handle)?;
        let _updated = self.poll_batch_receipt(&batch_receipt)?;
        Ok(None)
    }

    fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), SchedulerAdapterError> {
        let batch_receipt = self.load_receipt(&receipt.runtime_handle)?;
        let _updated = self.cancel_batch_receipt(&batch_receipt)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SchedulerAdapterError {
    #[error("scheduler operation is not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("internal scheduler adapter error: {0}")]
    Internal(&'static str),
    #[error("site `{site_name}` uses unsupported scheduler family `{scheduler_family:?}`")]
    UnsupportedSchedulerFamily {
        site_name: String,
        scheduler_family: SchedulerFamily,
    },
    #[error("site `{site_name}` does not define a `{command_kind}` scheduler command")]
    MissingSchedulerCommand {
        site_name: String,
        command_kind: &'static str,
    },
    #[error("scheduler submission for `{job_id}` did not include a launch payload command")]
    MissingLaunchPayload { job_id: String },
    #[error("runtime handle `{runtime_handle}` is unknown to the scheduler adapter")]
    UnknownReceiptHandle { runtime_handle: String },
    #[error("lease job `{lease_job_id}` does not match Scott runtime job `{runtime_job_id}`")]
    LeaseJobMismatch {
        lease_job_id: String,
        runtime_job_id: String,
    },
    #[error(
        "workspace job `{workspace_job_id}` does not match Scott runtime job `{runtime_job_id}`"
    )]
    WorkspaceJobMismatch {
        workspace_job_id: String,
        runtime_job_id: String,
    },
    #[error(
        "lease `{lease_id}` for job `{job_id}` is not active enough for submission: {state:?}"
    )]
    InactiveLease {
        lease_id: String,
        job_id: String,
        state: LeaseState,
    },
    #[error("scheduler command `{program}` failed with exit code {status_code}: {stderr}")]
    SchedulerCommandFailed {
        program: String,
        status_code: i32,
        stderr: String,
    },
    #[error(
        "scheduler output from `{program}` could not be parsed: {message}; output was `{output}`"
    )]
    SchedulerOutputParse {
        program: String,
        message: String,
        output: String,
    },
    #[error(
        "scheduler receipt `{receipt_id}` for job allocation `{scheduler_job_id}` returned no observation"
    )]
    MissingReceiptObservation {
        receipt_id: String,
        scheduler_job_id: String,
    },
    #[error("scheduler rejected job `{job_id}`: {message}")]
    SubmitRejected { job_id: String, message: String },
    #[error("scheduler returned an invalid receipt for `{job_id}`: {message}")]
    InvalidReceipt { job_id: String, message: String },
    #[error("runtime bridge failed: {0}")]
    Runtime(#[from] ScottRuntimeError),
}

fn ensure_success(
    command: &SchedulerCommand,
    output: &SchedulerCommandOutput,
) -> Result<(), SchedulerAdapterError> {
    if output.status_code == 0 {
        return Ok(());
    }

    Err(SchedulerAdapterError::SchedulerCommandFailed {
        program: command.program.clone(),
        status_code: output.status_code,
        stderr: output.stderr.trim().into(),
    })
}

fn parse_sbatch_allocation_id(stdout: &str) -> Result<String, SchedulerAdapterError> {
    let raw =
        first_nonempty_line(stdout).ok_or_else(|| SchedulerAdapterError::SchedulerOutputParse {
            program: "sbatch".into(),
            message: "expected parsable allocation id".into(),
            output: stdout.trim().into(),
        })?;
    let allocation_id = raw.split(';').next().unwrap_or_default().trim().to_string();
    if allocation_id.is_empty() {
        return Err(SchedulerAdapterError::SchedulerOutputParse {
            program: "sbatch".into(),
            message: "allocation id was empty".into(),
            output: stdout.trim().into(),
        });
    }
    Ok(allocation_id)
}

fn parse_qsub_allocation_id(stdout: &str) -> Result<String, SchedulerAdapterError> {
    let raw =
        first_nonempty_line(stdout).ok_or_else(|| SchedulerAdapterError::SchedulerOutputParse {
            program: "qsub".into(),
            message: "expected allocation id".into(),
            output: stdout.trim().into(),
        })?;
    if let Some(token) = raw
        .split_whitespace()
        .find(|token| token.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Ok(token.to_string());
    }
    Err(SchedulerAdapterError::SchedulerOutputParse {
        program: "qsub".into(),
        message: "could not extract job id".into(),
        output: stdout.trim().into(),
    })
}

fn parse_pbs_qsub_allocation_id(stdout: &str) -> Result<String, SchedulerAdapterError> {
    let raw =
        first_nonempty_line(stdout).ok_or_else(|| SchedulerAdapterError::SchedulerOutputParse {
            program: "qsub".into(),
            message: "expected PBS job id".into(),
            output: stdout.trim().into(),
        })?;
    let id = raw.trim();
    if id.is_empty() {
        return Err(SchedulerAdapterError::SchedulerOutputParse {
            program: "qsub".into(),
            message: "PBS job id was empty".into(),
            output: stdout.trim().into(),
        });
    }
    Ok(id.to_string())
}

fn parse_squeue_line(line: &str) -> Result<SlurmReceiptObservation, SchedulerAdapterError> {
    let mut parts = line.splitn(2, '|');
    let state = parts.next().unwrap_or_default().trim();
    let job_id = parts.next().unwrap_or_default().trim();
    if state.is_empty() || job_id.is_empty() {
        return Err(SchedulerAdapterError::SchedulerOutputParse {
            program: "squeue".into(),
            message: "expected `%T|%i` output".into(),
            output: line.into(),
        });
    }
    Ok(SlurmReceiptObservation {
        scheduler_job: parse_slurm_job_identifier(job_id),
        state: map_slurm_state(state),
        telemetry: None,
    })
}

fn parse_sacct_line(line: &str) -> Result<SlurmReceiptObservation, SchedulerAdapterError> {
    let mut parts = line.splitn(2, '|');
    let state = parts.next().unwrap_or_default().trim();
    let job_id = parts.next().unwrap_or_default().trim();
    if state.is_empty() || job_id.is_empty() {
        return Err(SchedulerAdapterError::SchedulerOutputParse {
            program: "sacct".into(),
            message: "expected `State|JobID` output".into(),
            output: line.into(),
        });
    }
    Ok(SlurmReceiptObservation {
        scheduler_job: parse_slurm_job_identifier(job_id),
        state: map_slurm_state(state),
        telemetry: None,
    })
}

fn parse_qstat_line(line: &str) -> Result<GridEngineReceiptObservation, SchedulerAdapterError> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 5 {
        return Err(SchedulerAdapterError::SchedulerOutputParse {
            program: "qstat".into(),
            message: "expected qstat job line with id and state".into(),
            output: line.into(),
        });
    }

    let job_id = fields[0].trim();
    let state = fields[4].trim();
    if !job_id.chars().all(|ch| ch.is_ascii_digit()) || state.is_empty() {
        return Err(SchedulerAdapterError::SchedulerOutputParse {
            program: "qstat".into(),
            message: "could not read job id and state from qstat line".into(),
            output: line.into(),
        });
    }

    Ok(GridEngineReceiptObservation {
        scheduler_job: parse_grid_engine_job_identifier(job_id),
        state: map_grid_engine_state(state),
    })
}

fn parse_qacct_output(
    receipt: &ExternalBatchReceipt,
    stdout: &str,
) -> Result<GridEngineReceiptObservation, SchedulerAdapterError> {
    let mut failed = None;
    let mut exit_status = None;

    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        match key {
            "failed" => failed = value.parse::<i32>().ok(),
            "exit_status" => exit_status = value.parse::<i32>().ok(),
            _ => {}
        }
    }

    let state = match (failed, exit_status) {
        (Some(failed), _) if failed != 0 => SchedulerReceiptState::Failed,
        (_, Some(code)) if code != 0 => SchedulerReceiptState::Failed,
        (Some(0), Some(0)) | (Some(0), None) | (None, Some(0)) => SchedulerReceiptState::Completed,
        _ => {
            return Err(SchedulerAdapterError::MissingReceiptObservation {
                receipt_id: receipt.receipt_id.clone(),
                scheduler_job_id: receipt.scheduler_job.allocation_id.clone(),
            })
        }
    };

    Ok(GridEngineReceiptObservation {
        scheduler_job: parse_grid_engine_job_identifier(&receipt.scheduler_job.allocation_id),
        state,
    })
}

fn parse_pbs_qstat_output(
    receipt: &ExternalBatchReceipt,
    stdout: &str,
) -> Result<PbsReceiptObservation, SchedulerAdapterError> {
    let mut job_id = None;
    let mut state = None;

    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(value) = line.strip_prefix("Job Id:") {
            job_id = Some(value.trim().to_string());
        } else if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "job_state" {
                state = Some(map_pbs_state(value.trim()));
            }
        }
    }

    let scheduler_job = parse_pbs_job_identifier(
        job_id
            .as_deref()
            .unwrap_or(&receipt.scheduler_job.allocation_id),
    );
    let state = state.ok_or_else(|| SchedulerAdapterError::MissingReceiptObservation {
        receipt_id: receipt.receipt_id.clone(),
        scheduler_job_id: receipt.scheduler_job.allocation_id.clone(),
    })?;

    Ok(PbsReceiptObservation {
        scheduler_job,
        state,
    })
}

fn parse_sstat_output(stdout: &str) -> Option<TelemetrySample> {
    let mut sample = TelemetrySample {
        sampled_at_ms: now_ms(),
        ..TelemetrySample::default()
    };
    let mut saw_any = false;

    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let rss = parts.next().unwrap_or_default().trim();
        let vm = parts.next().unwrap_or_default().trim();
        let cpu = parts.next().unwrap_or_default().trim();
        let nodes = parts.next().unwrap_or_default().trim();

        sample.rss_mb = max_optional_mb(sample.rss_mb, parse_slurm_memory_mb(rss));
        sample.vm_mb = max_optional_mb(sample.vm_mb, parse_slurm_memory_mb(vm));
        sample.cpu_time_secs =
            max_optional_f64(sample.cpu_time_secs, parse_slurm_duration_secs(cpu));
        if !nodes.is_empty() {
            for node in nodes
                .split(',')
                .map(str::trim)
                .filter(|node| !node.is_empty())
            {
                if !sample.node_list.iter().any(|existing| existing == node) {
                    sample.node_list.push(node.into());
                }
            }
        }
        saw_any = true;
    }

    saw_any.then_some(sample)
}

fn parse_slurm_job_identifier(raw: &str) -> SchedulerJobIdentifier {
    let trimmed = raw.trim();
    let (job_token, step_id) = if let Some((job, step)) = trimmed.split_once('.') {
        (job, Some(step.to_string()))
    } else {
        (trimmed, None)
    };

    let (allocation_id, array_job_id, array_index) =
        if let Some((job, index)) = job_token.split_once('_') {
            (job.to_string(), Some(job.to_string()), index.parse().ok())
        } else {
            (job_token.to_string(), None, None)
        };

    SchedulerJobIdentifier {
        scheduler_family: SchedulerFamily::Slurm,
        allocation_id,
        step_id,
        array_job_id,
        array_index,
    }
}

fn parse_grid_engine_job_identifier(raw: &str) -> SchedulerJobIdentifier {
    SchedulerJobIdentifier {
        scheduler_family: SchedulerFamily::GridEngine,
        allocation_id: raw.trim().to_string(),
        step_id: None,
        array_job_id: None,
        array_index: None,
    }
}

fn parse_pbs_job_identifier(raw: &str) -> SchedulerJobIdentifier {
    SchedulerJobIdentifier {
        scheduler_family: SchedulerFamily::Pbs,
        allocation_id: raw.trim().to_string(),
        step_id: None,
        array_job_id: None,
        array_index: None,
    }
}

fn map_slurm_state(raw_state: &str) -> SchedulerReceiptState {
    let state = raw_state.trim().to_ascii_uppercase();
    match state.as_str() {
        "PENDING" | "CONFIGURING" | "REQUEUED" | "REQUEUE_HOLD" | "RESIZING" | "SUSPENDED" => {
            SchedulerReceiptState::Queued
        }
        "RUNNING" | "COMPLETING" | "STAGE_OUT" | "STOPPED" | "SIGNALING" => {
            SchedulerReceiptState::Running
        }
        "COMPLETED" => SchedulerReceiptState::Completed,
        "CANCELLED" | "PREEMPTED" => SchedulerReceiptState::Cancelled,
        "BOOT_FAIL" | "DEADLINE" | "FAILED" | "NODE_FAIL" | "OUT_OF_MEMORY" | "TIMEOUT"
        | "REVOKED" => SchedulerReceiptState::Failed,
        _ => SchedulerReceiptState::Lost,
    }
}

fn map_grid_engine_state(raw_state: &str) -> SchedulerReceiptState {
    let state = raw_state.trim().to_ascii_lowercase();
    if state.contains('r') || state.contains('t') {
        SchedulerReceiptState::Running
    } else if state.contains("qw")
        || state.contains('q')
        || state.contains('h')
        || state.contains('w')
    {
        SchedulerReceiptState::Queued
    } else if state.contains("eqw") || state.contains('e') {
        SchedulerReceiptState::Failed
    } else {
        SchedulerReceiptState::Lost
    }
}

fn map_pbs_state(raw_state: &str) -> SchedulerReceiptState {
    match raw_state.trim().to_ascii_uppercase().as_str() {
        "Q" | "H" | "W" => SchedulerReceiptState::Queued,
        "R" | "B" | "E" | "S" => SchedulerReceiptState::Running,
        "F" => SchedulerReceiptState::Completed,
        "C" => SchedulerReceiptState::Cancelled,
        _ => SchedulerReceiptState::Lost,
    }
}

fn scheduler_state_label(state: SchedulerReceiptState) -> &'static str {
    match state {
        SchedulerReceiptState::Submitted => "submitted",
        SchedulerReceiptState::Queued => "queued",
        SchedulerReceiptState::Running => "running",
        SchedulerReceiptState::Completed => "completed",
        SchedulerReceiptState::Failed => "failed",
        SchedulerReceiptState::Cancelled => "cancelled",
        SchedulerReceiptState::Lost => "lost",
    }
}

fn parse_slurm_memory_mb(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") {
        return None;
    }

    let numeric_end = trimmed
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .unwrap_or(trimmed.len());
    let numeric = trimmed[..numeric_end].parse::<f64>().ok()?;
    let unit = trimmed[numeric_end..]
        .chars()
        .find(|ch| ch.is_ascii_alphabetic())
        .map(|ch| ch.to_ascii_uppercase());

    let mb = match unit {
        Some('K') => numeric / 1024.0,
        Some('M') => numeric,
        Some('G') => numeric * 1024.0,
        Some('T') => numeric * 1024.0 * 1024.0,
        Some('P') => numeric * 1024.0 * 1024.0 * 1024.0,
        _ => numeric,
    };

    Some(mb.round() as u64)
}

fn parse_slurm_duration_secs(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") {
        return None;
    }

    let (days, rest) = if let Some((days, rest)) = trimmed.split_once('-') {
        (days.parse::<f64>().ok()?, rest)
    } else {
        (0.0, trimmed)
    };

    let components = rest
        .split(':')
        .map(str::trim)
        .map(|part| part.parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;

    let seconds = match components.as_slice() {
        [minutes, seconds] => minutes * 60.0 + seconds,
        [hours, minutes, seconds] => hours * 3600.0 + minutes * 60.0 + seconds,
        [seconds] => *seconds,
        _ => return None,
    };

    Some(days * 86_400.0 + seconds)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'"'"'"#))
}

fn max_optional_mb(current: Option<u64>, next: Option<u64>) -> Option<u64> {
    match (current, next) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn max_optional_f64(current: Option<f64>, next: Option<f64>) -> Option<f64> {
    match (current, next) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn first_nonempty_line(input: &str) -> Option<&str> {
    input.lines().map(str::trim).find(|line| !line.is_empty())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn launch_host() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("HOST").ok())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::collections::VecDeque;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use indexmap::IndexMap;
    use patina_evaluator::{
        FinalStageFailurePolicy, ProcedureAction, ProcedureCursor, ScottBackendMode,
        ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode, ScottProcedureIntent,
        ScottProcedurePlan, ScottProcedureRequest, StageEngine, StageIndex, StagePlan,
        StageSelection, StageTicket,
    };
    use patina_external::{
        ExternalEvaluationMode, ExternalEvaluationRequest, ExternalEvaluator,
        ExternalExecutionMode, ExternalProgram, ExternalTemplateSet, GulpExternalAdapter,
        GulpExternalAdapterConfig,
    };
    use patina_runtime::{
        external_stage_outcome_to_runtime_report, RuntimeDiagnostics, ScottControllerKind,
        ScottDispatchReceipt, ScottJobIdentity, ScottJobKind, ScottJobRole, ScottRuntime,
        ScottRuntimeError, ScottRuntimeJob, ScottRuntimeReport, ScottRuntimeStatus,
        ScottStageDispatch,
    };
    use patina_types::Candidate;
    use tempfile::tempdir;

    use super::{
        parse_qacct_output, parse_qstat_line, parse_qsub_allocation_id, parse_sacct_line,
        parse_sbatch_allocation_id, parse_slurm_duration_secs, parse_slurm_job_identifier,
        parse_slurm_memory_mb, parse_squeue_line, parse_sstat_output, GridEngineCommandRunner,
        GridEngineSchedulerAdapter, PbsCommandRunner, PbsSchedulerAdapter, RuntimeBackedScheduler,
        SchedulerAdapterError, SchedulerCommand, SchedulerCommandOutput, SchedulerPlacement,
        SlurmCommandRunner, SlurmSchedulerAdapter, UlabScheduler,
    };
    use crate::{
        ExternalBatchReceipt, LeaseState, SchedulerFamily, SchedulerReceiptState, SiteProfile,
        WorkLease, WorkspacePlan,
    };

    #[derive(Default)]
    struct FakeRuntime {
        submitted_jobs: Mutex<Vec<String>>,
        polled_handles: Mutex<Vec<String>>,
        cancelled_handles: Mutex<Vec<String>>,
        next_report: Mutex<Option<ScottRuntimeReport>>,
    }

    impl ScottRuntime for FakeRuntime {
        fn submit(
            &self,
            job: &ScottRuntimeJob,
        ) -> Result<ScottDispatchReceipt, patina_runtime::ScottRuntimeError> {
            self.submitted_jobs
                .lock()
                .unwrap()
                .push(job.identity.job_id.clone());
            Ok(ScottDispatchReceipt {
                job_id: job.identity.job_id.clone(),
                runtime_handle: format!("handle-{}", job.identity.job_id),
            })
        }

        fn poll(
            &self,
            receipt: &ScottDispatchReceipt,
        ) -> Result<Option<ScottRuntimeReport>, patina_runtime::ScottRuntimeError> {
            self.polled_handles
                .lock()
                .unwrap()
                .push(receipt.runtime_handle.clone());
            Ok(self.next_report.lock().unwrap().clone())
        }

        fn cancel(
            &self,
            receipt: &ScottDispatchReceipt,
        ) -> Result<(), patina_runtime::ScottRuntimeError> {
            self.cancelled_handles
                .lock()
                .unwrap()
                .push(receipt.runtime_handle.clone());
            Ok(())
        }
    }

    struct FakeSlurmRunner {
        commands: Mutex<Vec<SchedulerCommand>>,
        outputs: Mutex<VecDeque<SchedulerCommandOutput>>,
    }

    impl FakeSlurmRunner {
        fn new(outputs: Vec<SchedulerCommandOutput>) -> Self {
            Self {
                commands: Mutex::new(Vec::new()),
                outputs: Mutex::new(outputs.into()),
            }
        }
    }

    struct LocalGulpRuntime {
        adapter: GulpExternalAdapter,
        jobs: Mutex<HashMap<String, ScottRuntimeJob>>,
        stage_roots: Mutex<HashMap<String, PathBuf>>,
    }

    impl LocalGulpRuntime {
        fn new(adapter: GulpExternalAdapter) -> Self {
            Self {
                adapter,
                jobs: Mutex::new(HashMap::new()),
                stage_roots: Mutex::new(HashMap::new()),
            }
        }

        fn stage_request(
            &self,
            job: &ScottRuntimeJob,
            runtime_handle: &str,
        ) -> Result<ExternalEvaluationRequest, ScottRuntimeError> {
            let dispatch = match &job.kind {
                ScottJobKind::EvaluateStage { dispatch } => dispatch,
                _ => {
                    return Err(ScottRuntimeError::SubmitRejected {
                        job_id: job.identity.job_id.clone(),
                        message: "LocalGulpRuntime only supports stage-worker jobs".into(),
                    });
                }
            };
            let stage_root = self
                .stage_roots
                .lock()
                .unwrap()
                .get(runtime_handle)
                .cloned()
                .ok_or_else(|| ScottRuntimeError::UnknownHandle {
                    handle: runtime_handle.into(),
                })?;
            let ticket = &dispatch.cursor.ticket;
            let stage_workdir = stage_root.join(format!(
                "stage_{:02}_attempt_{:02}",
                ticket.stage.get(),
                ticket.attempt
            ));

            Ok(ExternalEvaluationRequest {
                request_id: dispatch.request.request_id.clone(),
                stage: ticket.stage.get(),
                program: ExternalProgram::Gulp,
                mode: ExternalEvaluationMode::Relaxation,
                execution: ExternalExecutionMode::LocalScript,
                retrieve_relaxed_geometry: dispatch
                    .plan
                    .evaluator
                    .settings
                    .retrieve_relaxed_geometry,
                candidate: dispatch.request.candidate.clone(),
                workdir: stage_workdir,
                templates: ExternalTemplateSet::default(),
            })
        }
    }

    impl ScottRuntime for LocalGulpRuntime {
        fn submit(&self, job: &ScottRuntimeJob) -> Result<ScottDispatchReceipt, ScottRuntimeError> {
            let handle = format!("local-gulp-handle-{}", job.identity.job_id);
            self.jobs
                .lock()
                .unwrap()
                .insert(handle.clone(), job.clone());
            Ok(ScottDispatchReceipt {
                job_id: job.identity.job_id.clone(),
                runtime_handle: handle,
            })
        }

        fn poll(
            &self,
            receipt: &ScottDispatchReceipt,
        ) -> Result<Option<ScottRuntimeReport>, ScottRuntimeError> {
            let job = self
                .jobs
                .lock()
                .unwrap()
                .remove(&receipt.runtime_handle)
                .ok_or_else(|| ScottRuntimeError::UnknownHandle {
                    handle: receipt.runtime_handle.clone(),
                })?;
            let request = self.stage_request(&job, &receipt.runtime_handle)?;
            let outcome = self.adapter.evaluate(&request).map_err(|error| {
                ScottRuntimeError::SubmitRejected {
                    job_id: job.identity.job_id.clone(),
                    message: format!("GULP adapter failed: {error}"),
                }
            })?;
            let report = external_stage_outcome_to_runtime_report(
                &job,
                outcome,
                RuntimeDiagnostics::default(),
            )
            .map_err(|error| ScottRuntimeError::SubmitRejected {
                job_id: job.identity.job_id.clone(),
                message: format!("stage completion bridge failed: {error}"),
            })?;
            Ok(Some(report))
        }

        fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), ScottRuntimeError> {
            let removed = self.jobs.lock().unwrap().remove(&receipt.runtime_handle);
            if removed.is_none() {
                return Err(ScottRuntimeError::UnknownHandle {
                    handle: receipt.runtime_handle.clone(),
                });
            }
            Ok(())
        }
    }

    impl SlurmCommandRunner for FakeSlurmRunner {
        fn run(
            &self,
            command: &SchedulerCommand,
        ) -> Result<SchedulerCommandOutput, SchedulerAdapterError> {
            self.commands.lock().unwrap().push(command.clone());
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(SchedulerAdapterError::Internal(
                    "fake runner had no queued output",
                ))
        }
    }

    struct FakeGridEngineRunner {
        commands: Mutex<Vec<SchedulerCommand>>,
        outputs: Mutex<VecDeque<SchedulerCommandOutput>>,
    }

    impl FakeGridEngineRunner {
        fn new(outputs: Vec<SchedulerCommandOutput>) -> Self {
            Self {
                commands: Mutex::new(Vec::new()),
                outputs: Mutex::new(outputs.into()),
            }
        }
    }

    impl GridEngineCommandRunner for FakeGridEngineRunner {
        fn run(
            &self,
            command: &SchedulerCommand,
        ) -> Result<SchedulerCommandOutput, SchedulerAdapterError> {
            self.commands.lock().unwrap().push(command.clone());
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(SchedulerAdapterError::Internal(
                    "fake grid engine runner had no queued output",
                ))
        }
    }

    struct FakePbsRunner {
        commands: Mutex<Vec<SchedulerCommand>>,
        outputs: Mutex<VecDeque<SchedulerCommandOutput>>,
    }

    impl FakePbsRunner {
        fn new(outputs: Vec<SchedulerCommandOutput>) -> Self {
            Self {
                commands: Mutex::new(Vec::new()),
                outputs: Mutex::new(outputs.into()),
            }
        }
    }

    impl PbsCommandRunner for FakePbsRunner {
        fn run(
            &self,
            command: &SchedulerCommand,
        ) -> Result<SchedulerCommandOutput, SchedulerAdapterError> {
            self.commands.lock().unwrap().push(command.clone());
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(SchedulerAdapterError::Internal(
                    "fake pbs runner had no queued output",
                ))
        }
    }

    fn controller_job(job_id: &str) -> ScottRuntimeJob {
        ScottRuntimeJob {
            identity: ScottJobIdentity {
                job_id: job_id.into(),
                parent_job_id: None,
                candidate_label: None,
            },
            role: ScottJobRole::Controller,
            kind: ScottJobKind::Controller {
                kind: ScottControllerKind::ProductionRun,
                resume_from: None,
            },
            requested_cores: 4,
            requested_gpus: 1,
            required_tags: vec!["cpu".into()],
            labels: IndexMap::new(),
        }
    }

    fn periodic_candidate(label: &str) -> Candidate {
        Candidate::fully_periodic(
            label,
            vec!["Ce".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]],
        )
    }

    fn gulp_stage_job(job_id: &str, workdir: &str) -> ScottRuntimeJob {
        let ticket = StageTicket {
            request_id: format!("req-{job_id}"),
            backend_mode: ScottBackendMode::Gulp,
            stage: StageIndex(1),
            attempt: 1,
            workdir: workdir.into(),
        };
        let dispatch = ScottStageDispatch {
            plan: ScottProcedurePlan {
                evaluator: ScottEvaluatorPlan {
                    master_template: "/tmp/Master.gin".into(),
                    atoms_in: None,
                    work_root: workdir.into(),
                    settings: ScottEvaluatorSettings {
                        backend_mode: ScottBackendMode::Gulp,
                        procedure_intent: ScottProcedureIntent::ProductionRun,
                        lattice_mode: ScottLatticeMode::Cluster,
                        max_relaxation_attempts: 2,
                        gnorm_tolerance: 1.0e-4,
                        final_stage_failure_policy: FinalStageFailurePolicy::RejectCandidate,
                        retrieve_relaxed_geometry: true,
                        keep_stage_artifacts: true,
                    },
                },
                stages: StageSelection {
                    stages: vec![StagePlan {
                        stage: StageIndex(1),
                        engine: StageEngine::Gulp,
                        refine_if_energy_below: None,
                        energy_min_threshold: None,
                        energy_max_threshold: None,
                        keep_only_if_final_stage: false,
                    }],
                },
            },
            request: ScottProcedureRequest {
                candidate: periodic_candidate("ceria"),
                request_id: format!("req-{job_id}"),
                workdir: workdir.into(),
            },
            cursor: ProcedureCursor::new(ticket.clone()),
            action: ProcedureAction::Submit(ticket),
        };
        let mut job = ScottRuntimeJob::stage_worker(job_id, Some("controller-1".into()), dispatch);
        job.requested_cores = 1;
        job.requested_gpus = 0;
        job.required_tags = vec!["cpu".into(), "gulp".into()];
        job
    }

    fn lease(job_id: &str, state: LeaseState) -> WorkLease {
        WorkLease {
            lease_id: format!("lease-{job_id}"),
            job_id: job_id.into(),
            worker_id: "worker-local".into(),
            issued_at_ms: 100,
            expires_at_ms: 200,
            state,
        }
    }

    fn workspace(job_id: &str) -> WorkspacePlan {
        WorkspacePlan {
            job_id: job_id.into(),
            durable_job_root: format!("/durable/{job_id}").into(),
            active_job_root: format!("/scratch/{job_id}").into(),
            stage_root: format!("/scratch/{job_id}/stage").into(),
            manifest_path: format!("/durable/{job_id}/manifest.json").into(),
        }
    }

    #[cfg(unix)]
    fn write_fake_gulp_script(script_path: &Path) {
        fs::write(
            script_path,
            "#!/bin/sh\nprintf 'Final energy = -1.23 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Ce 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n' > candidate.got\n",
        )
        .expect("write fake gulp script");
        let mut perms = fs::metadata(script_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(script_path, perms).expect("chmod");
    }

    #[test]
    fn runtime_backed_scheduler_delegates_submit_poll_and_cancel() {
        let runtime = Arc::new(FakeRuntime {
            next_report: Mutex::new(Some(ScottRuntimeReport {
                identity: controller_job("job-1").identity,
                status: ScottRuntimeStatus::Completed,
                stage_result: None,
                generated_jobs: Vec::new(),
                diagnostics: RuntimeDiagnostics::default(),
            })),
            ..Default::default()
        });
        let scheduler = RuntimeBackedScheduler::local(runtime.clone());
        let job = controller_job("job-1");

        let receipt = scheduler
            .submit(
                &lease("job-1", LeaseState::Active),
                &job,
                &workspace("job-1"),
                &SchedulerPlacement::default(),
            )
            .unwrap();
        assert_eq!(receipt.runtime_handle, "handle-job-1");
        assert_eq!(
            runtime.submitted_jobs.lock().unwrap().as_slice(),
            &["job-1".to_string()]
        );

        let report = scheduler.poll(&receipt).unwrap().unwrap();
        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert_eq!(
            runtime.polled_handles.lock().unwrap().as_slice(),
            &["handle-job-1".to_string()]
        );

        scheduler.cancel(&receipt).unwrap();
        assert_eq!(
            runtime.cancelled_handles.lock().unwrap().as_slice(),
            &["handle-job-1".to_string()]
        );
    }

    #[test]
    #[cfg(unix)]
    fn runtime_backed_scheduler_runs_real_gulp_stage_locally() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.gin");
        fs::write(
            &template_path,
            "\
opti conp
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
        )
        .expect("write template");
        let script_path = temp.path().join("fake_gulp.sh");
        write_fake_gulp_script(&script_path);

        let adapter =
            GulpExternalAdapter::new(GulpExternalAdapterConfig::new(&template_path, &script_path))
                .expect("adapter");
        let runtime = Arc::new(LocalGulpRuntime::new(adapter));
        let job = gulp_stage_job("job-gulp-local", temp.path().to_str().expect("utf8 path"));
        let handle_stage_root = temp.path().join("runtime-job-gulp-local");
        runtime.stage_roots.lock().unwrap().insert(
            "local-gulp-handle-job-gulp-local".into(),
            handle_stage_root.clone(),
        );

        let scheduler = RuntimeBackedScheduler::local(runtime);
        let receipt = scheduler
            .submit(
                &lease("job-gulp-local", LeaseState::Active),
                &job,
                &workspace("job-gulp-local"),
                &SchedulerPlacement::default(),
            )
            .expect("submit");
        let report = scheduler.poll(&receipt).expect("poll").expect("report");

        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert!(report.generated_jobs.is_empty());
        let stage_result = report.stage_result.expect("stage result");
        assert_eq!(stage_result.ticket.stage, StageIndex(1));
        assert_eq!(stage_result.outcome.final_stage, Some(StageIndex(1)));
        assert_eq!(
            stage_result
                .outcome
                .final_result
                .as_ref()
                .map(|result| result.energy),
            Some(-1.23)
        );
        assert!(handle_stage_root
            .join("stage_01_attempt_01")
            .join("candidate.gin")
            .exists());
        assert!(handle_stage_root
            .join("stage_01_attempt_01")
            .join("candidate.got")
            .exists());
    }

    #[test]
    fn runtime_backed_scheduler_rejects_mismatched_lease_job() {
        let scheduler = RuntimeBackedScheduler::local(Arc::new(FakeRuntime::default()));
        let error = scheduler
            .submit(
                &lease("job-x", LeaseState::Accepted),
                &controller_job("job-y"),
                &workspace("job-y"),
                &SchedulerPlacement::default(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            SchedulerAdapterError::LeaseJobMismatch { .. }
        ));
    }

    #[test]
    fn runtime_backed_scheduler_rejects_inactive_lease() {
        let scheduler = RuntimeBackedScheduler::local(Arc::new(FakeRuntime::default()));
        let error = scheduler
            .submit(
                &lease("job-z", LeaseState::Reclaimed),
                &controller_job("job-z"),
                &workspace("job-z"),
                &SchedulerPlacement::default(),
            )
            .unwrap_err();

        assert!(matches!(error, SchedulerAdapterError::InactiveLease { .. }));
    }

    #[test]
    fn slurm_scheduler_builds_typed_submit_command() {
        let runner = Arc::new(FakeSlurmRunner::new(Vec::new()));
        let scheduler = SlurmSchedulerAdapter::new(SiteProfile::archer2(), runner).unwrap();
        let command = scheduler
            .build_submit_command(
                &controller_job("job-slurm"),
                &workspace("job-slurm"),
                &SchedulerPlacement {
                    queue: Some("standard".into()),
                    account: Some("e05".into()),
                    reservation: None,
                    node_feature: Some("gpu".into()),
                    launcher_argv: vec![
                        "ulab-dispatch".into(),
                        "--job-id".into(),
                        "job-slurm".into(),
                    ],
                    extra_submit_args: vec!["--exclusive".into()],
                },
            )
            .unwrap();

        assert_eq!(command.program, "sbatch");
        assert!(command.args.contains(&"--partition".to_string()));
        assert!(command.args.contains(&"standard".to_string()));
        assert!(command.args.contains(&"--gpus-per-task".to_string()));
        let wrap = command.args.last().unwrap();
        assert!(wrap.contains("'srun'"));
        assert!(wrap.contains("'ulab-dispatch'"));
    }

    #[test]
    fn slurm_scheduler_submit_poll_and_cancel_track_receipts() {
        let runner = Arc::new(FakeSlurmRunner::new(vec![
            SchedulerCommandOutput {
                stdout: "12345;archer2\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: "RUNNING|12345\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: "2048M|4096M|01:02:03|node01,node02\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status_code: 0,
            },
        ]));
        let scheduler = SlurmSchedulerAdapter::new(SiteProfile::archer2(), runner.clone()).unwrap();

        let receipt = scheduler
            .submit(
                &lease("job-77", LeaseState::Active),
                &controller_job("job-77"),
                &workspace("job-77"),
                &SchedulerPlacement {
                    queue: Some("standard".into()),
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["ulab-dispatch".into(), "job-77".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(receipt.runtime_handle, "slurm:12345:job-77");

        let polled = scheduler.poll(&receipt).unwrap();
        assert!(polled.is_none());
        let tracked = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(tracked.state, SchedulerReceiptState::Running);
        assert_eq!(
            tracked.last_telemetry.as_ref().and_then(|t| t.rss_mb),
            Some(2048)
        );
        assert_eq!(
            tracked.last_telemetry.as_ref().map(|t| t.node_list.clone()),
            Some(vec!["node01".into(), "node02".into()])
        );
        assert_eq!(
            tracked
                .launcher_provenance
                .as_ref()
                .map(|p| p.launch_command.as_str()),
            Some("srun")
        );

        scheduler.cancel(&receipt).unwrap();
        let cancelled = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.state, SchedulerReceiptState::Cancelled);

        let commands = runner.commands.lock().unwrap();
        assert_eq!(commands[0].program, "sbatch");
        assert_eq!(commands[1].program, "squeue");
        assert_eq!(commands[2].program, "sstat");
        assert_eq!(commands[3].program, "scancel");
    }

    #[test]
    fn slurm_scheduler_falls_back_to_accounting_for_terminal_state() {
        let runner = Arc::new(FakeSlurmRunner::new(vec![
            SchedulerCommandOutput {
                stdout: "22222\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: "COMPLETED|22222\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
        ]));
        let scheduler = SlurmSchedulerAdapter::new(SiteProfile::archer2(), runner).unwrap();

        let receipt = scheduler
            .submit(
                &lease("job-88", LeaseState::Active),
                &controller_job("job-88"),
                &workspace("job-88"),
                &SchedulerPlacement {
                    queue: None,
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["ulab-dispatch".into(), "job-88".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();
        scheduler.poll(&receipt).unwrap();

        let tracked = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(tracked.state, SchedulerReceiptState::Completed);
    }

    #[test]
    fn slurm_scheduler_projects_runtime_diagnostics_from_receipt() {
        let runner = Arc::new(FakeSlurmRunner::new(vec![SchedulerCommandOutput {
            stdout: "33333\n".into(),
            stderr: String::new(),
            status_code: 0,
        }]));
        let scheduler = SlurmSchedulerAdapter::new(SiteProfile::archer2(), runner).unwrap();
        let receipt = scheduler
            .submit_batch_receipt(
                &lease("job-99", LeaseState::Active),
                &controller_job("job-99"),
                &workspace("job-99"),
                &SchedulerPlacement {
                    queue: None,
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["ulab-dispatch".into(), "job-99".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();

        let diagnostics = scheduler.runtime_diagnostics_for_receipt(&receipt);
        assert_eq!(
            diagnostics
                .launch_provenance
                .as_ref()
                .and_then(|p| p.site_name.as_deref()),
            Some("archer2")
        );
        assert_eq!(
            diagnostics
                .metrics
                .get("scheduler_allocation_id")
                .map(String::as_str),
            Some("33333")
        );
        assert_eq!(
            diagnostics
                .metrics
                .get("scheduler_state")
                .map(String::as_str),
            Some("submitted")
        );
    }

    #[test]
    fn grid_engine_scheduler_builds_typed_submit_command() {
        let runner = Arc::new(FakeGridEngineRunner::new(Vec::new()));
        let scheduler = GridEngineSchedulerAdapter::new(SiteProfile::young(), runner).unwrap();
        let command = scheduler
            .build_submit_command(
                &controller_job("job-young"),
                &workspace("job-young"),
                &SchedulerPlacement {
                    queue: Some("all.q".into()),
                    account: Some("UCL_chemM_Woodley".into()),
                    reservation: Some("Gold".into()),
                    node_feature: Some("mem=1G".into()),
                    launcher_argv: vec![
                        "./target/release/patina-driver".into(),
                        "evaluate-backend-batch".into(),
                    ],
                    extra_submit_args: vec!["-l".into(), "h_rt=01:00:00".into()],
                },
            )
            .unwrap();

        assert_eq!(command.program, "qsub");
        assert!(command.args.contains(&"-terse".to_string()));
        assert!(command.args.contains(&"-wd".to_string()));
        assert!(command
            .args
            .contains(&"/scratch/job-young/stage".to_string()));
        assert!(command.args.contains(&"-P".to_string()));
        assert!(command.args.contains(&"Gold".to_string()));
        assert!(command.args.contains(&"-A".to_string()));
        assert!(command.args.contains(&"UCL_chemM_Woodley".to_string()));
        assert!(command.args.contains(&"-pe".to_string()));
        assert!(command.args.contains(&"mpi".to_string()));
    }

    #[test]
    fn grid_engine_scheduler_submit_poll_and_cancel_track_receipts() {
        let runner = Arc::new(FakeGridEngineRunner::new(vec![
            SchedulerCommandOutput {
                stdout: "67890\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: "67890 0.55500 job-young user r 04/20/2026 11:00:00 all.q@node01 80\n"
                    .into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status_code: 0,
            },
        ]));
        let scheduler =
            GridEngineSchedulerAdapter::new(SiteProfile::young(), runner.clone()).unwrap();

        let receipt = scheduler
            .submit(
                &lease("job-young", LeaseState::Active),
                &controller_job("job-young"),
                &workspace("job-young"),
                &SchedulerPlacement {
                    queue: None,
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["./target/release/unifiedlab".into(), "worker".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(receipt.runtime_handle, "grid_engine:67890:job-young");

        let polled = scheduler.poll(&receipt).unwrap();
        assert!(polled.is_none());
        let tracked = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(tracked.state, SchedulerReceiptState::Running);
        assert_eq!(
            tracked
                .launcher_provenance
                .as_ref()
                .and_then(|p| p.submit_command.as_deref()),
            Some("qsub")
        );

        scheduler.cancel(&receipt).unwrap();
        let cancelled = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.state, SchedulerReceiptState::Cancelled);

        let commands = runner.commands.lock().unwrap();
        assert_eq!(commands[0].program, "qsub");
        assert_eq!(commands[1].program, "qstat");
        assert_eq!(commands[2].program, "qdel");
    }

    #[test]
    fn grid_engine_scheduler_falls_back_to_accounting_for_terminal_state() {
        let runner = Arc::new(FakeGridEngineRunner::new(vec![
            SchedulerCommandOutput {
                stdout: "67891\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: "failed 0\nexit_status 0\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
        ]));
        let scheduler = GridEngineSchedulerAdapter::new(SiteProfile::young(), runner).unwrap();

        let receipt = scheduler
            .submit(
                &lease("job-young-terminal", LeaseState::Active),
                &controller_job("job-young-terminal"),
                &workspace("job-young-terminal"),
                &SchedulerPlacement {
                    queue: None,
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["./target/release/unifiedlab".into(), "worker".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();
        scheduler.poll(&receipt).unwrap();

        let tracked = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(tracked.state, SchedulerReceiptState::Completed);
    }

    #[test]
    fn grid_engine_scheduler_projects_runtime_diagnostics_from_receipt() {
        let runner = Arc::new(FakeGridEngineRunner::new(vec![SchedulerCommandOutput {
            stdout: "67892\n".into(),
            stderr: String::new(),
            status_code: 0,
        }]));
        let scheduler = GridEngineSchedulerAdapter::new(SiteProfile::young(), runner).unwrap();
        let receipt = scheduler
            .submit_batch_receipt(
                &lease("job-young-diagnostics", LeaseState::Active),
                &controller_job("job-young-diagnostics"),
                &workspace("job-young-diagnostics"),
                &SchedulerPlacement {
                    queue: None,
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["./target/release/unifiedlab".into(), "worker".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();

        let diagnostics = scheduler.runtime_diagnostics_for_receipt(&receipt);
        assert_eq!(
            diagnostics
                .launch_provenance
                .as_ref()
                .and_then(|p| p.site_name.as_deref()),
            Some("young")
        );
        assert_eq!(
            diagnostics
                .metrics
                .get("scheduler_family")
                .map(String::as_str),
            Some("grid_engine")
        );
        assert_eq!(
            diagnostics
                .metrics
                .get("scheduler_allocation_id")
                .map(String::as_str),
            Some("67892")
        );
    }

    #[test]
    fn pbs_scheduler_builds_typed_submit_command() {
        let runner = Arc::new(FakePbsRunner::new(Vec::new()));
        let scheduler = PbsSchedulerAdapter::new(SiteProfile::generic_pbs("pbs-site"), runner)
            .expect("pbs scheduler");
        let command = scheduler
            .build_submit_command(
                &controller_job("job-pbs"),
                &workspace("job-pbs"),
                &SchedulerPlacement {
                    queue: Some("workq".into()),
                    account: Some("chem".into()),
                    reservation: None,
                    node_feature: Some("walltime=01:00:00".into()),
                    launcher_argv: vec!["ulab-dispatch".into(), "job-pbs".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();

        assert_eq!(command.program, "qsub");
        assert!(command.args.contains(&"-N".to_string()));
        assert!(command.args.contains(&"job-pbs".to_string()));
        assert!(command.args.contains(&"-d".to_string()));
        assert!(command.args.contains(&"/scratch/job-pbs/stage".to_string()));
        assert!(command.args.contains(&"-q".to_string()));
        assert!(command.args.contains(&"workq".to_string()));
        assert!(command.args.contains(&"-A".to_string()));
        assert!(command.args.contains(&"chem".to_string()));
        assert!(command.args.contains(&"--".to_string()));
    }

    #[test]
    fn pbs_scheduler_submit_poll_and_cancel_track_receipts() {
        let runner = Arc::new(FakePbsRunner::new(vec![
            SchedulerCommandOutput {
                stdout: "12345.server\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: "Job Id: 12345.server\n    job_state = R\n".into(),
                stderr: String::new(),
                status_code: 0,
            },
            SchedulerCommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status_code: 0,
            },
        ]));
        let scheduler =
            PbsSchedulerAdapter::new(SiteProfile::generic_pbs("pbs-site"), runner.clone())
                .expect("pbs scheduler");

        let receipt = scheduler
            .submit(
                &lease("job-pbs", LeaseState::Active),
                &controller_job("job-pbs"),
                &workspace("job-pbs"),
                &SchedulerPlacement {
                    queue: None,
                    account: None,
                    reservation: None,
                    node_feature: None,
                    launcher_argv: vec!["ulab-dispatch".into(), "job-pbs".into()],
                    extra_submit_args: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(receipt.runtime_handle, "pbs:12345.server:job-pbs");

        assert!(scheduler.poll(&receipt).unwrap().is_none());
        let tracked = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(tracked.state, SchedulerReceiptState::Running);
        assert_eq!(tracked.scheduler_job.scheduler_family, SchedulerFamily::Pbs);

        scheduler.cancel(&receipt).unwrap();
        let cancelled = scheduler
            .receipt_for_handle(&receipt.runtime_handle)
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.state, SchedulerReceiptState::Cancelled);

        let commands = runner.commands.lock().unwrap();
        assert_eq!(commands[0].program, "qsub");
        assert_eq!(commands[1].program, "qstat");
        assert_eq!(commands[2].program, "qdel");
    }

    #[test]
    fn parses_slurm_receipt_and_telemetry_helpers() {
        assert_eq!(
            parse_sbatch_allocation_id("12345;cluster\n").unwrap(),
            "12345"
        );
        assert_eq!(
            parse_slurm_job_identifier("12345_7.batch").allocation_id,
            "12345"
        );
        assert_eq!(
            parse_slurm_job_identifier("12345_7.batch").array_index,
            Some(7)
        );
        assert_eq!(
            parse_squeue_line("RUNNING|12345_7\n").unwrap().state,
            SchedulerReceiptState::Running
        );
        assert_eq!(
            parse_sacct_line("TIMEOUT|12345.batch\n").unwrap().state,
            SchedulerReceiptState::Failed
        );
        assert_eq!(parse_slurm_memory_mb("1.5G"), Some(1536));
        assert_eq!(parse_slurm_duration_secs("1-01:02:03"), Some(90_123.0));

        let telemetry =
            parse_sstat_output("1024M|2048M|01:00:00|nid001\n512M|4096M|00:30:00|nid002\n")
                .unwrap();
        assert_eq!(telemetry.rss_mb, Some(1024));
        assert_eq!(telemetry.vm_mb, Some(4096));
        assert_eq!(telemetry.cpu_time_secs, Some(3600.0));
        assert_eq!(telemetry.node_list, vec!["nid001", "nid002"]);
    }

    #[test]
    fn parses_grid_engine_receipt_helpers() {
        assert_eq!(parse_qsub_allocation_id("67890\n").unwrap(), "67890");
        assert_eq!(
            parse_qsub_allocation_id("Your job 67890 (\"job-young\") has been submitted\n")
                .unwrap(),
            "67890"
        );

        let active =
            parse_qstat_line("67890 0.55500 job-young user r 04/20/2026 11:00:00 all.q@node01 80")
                .unwrap();
        assert_eq!(active.state, SchedulerReceiptState::Running);
        assert_eq!(
            active.scheduler_job.scheduler_family,
            SchedulerFamily::GridEngine
        );

        let queued =
            parse_qstat_line("67891 0.55500 job-young user qw 04/20/2026 11:00:00 1").unwrap();
        assert_eq!(queued.state, SchedulerReceiptState::Queued);

        let receipt = ExternalBatchReceipt {
            receipt_id: "grid_engine:67890:job-young".into(),
            job_id: "job-young".into(),
            scheduler_job: super::parse_grid_engine_job_identifier("67890"),
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: 0,
            last_observed_at_ms: 0,
            launch_host: None,
            workdir: None,
            launcher_provenance: None,
            last_telemetry: None,
        };
        assert_eq!(
            parse_qacct_output(&receipt, "failed 0\nexit_status 0\n")
                .unwrap()
                .state,
            SchedulerReceiptState::Completed
        );
        assert_eq!(
            parse_qacct_output(&receipt, "failed 1\nexit_status 1\n")
                .unwrap()
                .state,
            SchedulerReceiptState::Failed
        );
    }

    #[test]
    fn parses_pbs_receipt_helpers() {
        assert_eq!(
            super::parse_pbs_qsub_allocation_id("12345.server\n").unwrap(),
            "12345.server"
        );
        let receipt = ExternalBatchReceipt {
            receipt_id: "pbs:12345.server:job-pbs".into(),
            job_id: "job-pbs".into(),
            scheduler_job: super::parse_pbs_job_identifier("12345.server"),
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: 0,
            last_observed_at_ms: 0,
            launch_host: None,
            workdir: None,
            launcher_provenance: None,
            last_telemetry: None,
        };
        let running =
            super::parse_pbs_qstat_output(&receipt, "Job Id: 12345.server\n    job_state = R\n")
                .unwrap();
        assert_eq!(running.state, SchedulerReceiptState::Running);
        assert_eq!(running.scheduler_job.scheduler_family, SchedulerFamily::Pbs);
        let completed =
            super::parse_pbs_qstat_output(&receipt, "Job Id: 12345.server\n    job_state = F\n")
                .unwrap();
        assert_eq!(completed.state, SchedulerReceiptState::Completed);
    }
}
