use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ContainerShard, ExternalBatchReceipt, WorkspacePlan};

/// Fine-grained execution phase observed inside a scheduler allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationTaskPhase {
    Materializing,
    LaunchingBackend,
    BackendRunning,
    Retrieving,
    Parsing,
    Completed,
    Failed,
}

impl AllocationTaskPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

/// Immutable launch metadata consumed by the runtime wrapper inside the allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationLaunchEnvelope {
    pub receipt_id: String,
    pub job_id: String,
    pub shard_id: Option<String>,
    pub stage_root: String,
    pub durable_root: String,
    pub manifest_path: String,
    pub scheduler_workdir: Option<String>,
    pub extra_env: BTreeMap<String, String>,
}

/// Compact heartbeat emitted by the runtime wrapper while the allocation is alive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationHeartbeat {
    pub phase: AllocationTaskPhase,
    pub observed_at_ms: u64,
    pub attempt: u32,
    pub active_job_id: Option<String>,
    pub backend_program: Option<String>,
    pub message: Option<String>,
}

/// Replay-safe terminal summary written by the runtime wrapper inside one allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationTerminalSummary {
    pub receipt_id: String,
    pub shard_id: Option<String>,
    pub observed_at_ms: u64,
    pub final_phase: AllocationTaskPhase,
    pub completed_job_ids: Vec<String>,
    pub failed_job_ids: Vec<String>,
    pub salvageable_job_ids: Vec<String>,
    pub artifact_roots: Vec<String>,
    pub message: Option<String>,
}

/// Hexagonal port for coordinator interaction with the runtime wrapper inside an allocation.
pub trait AllocationRuntimePort: Send + Sync {
    fn build_launch_envelope(
        &self,
        workspace: &WorkspacePlan,
        receipt: &ExternalBatchReceipt,
        shard: Option<&ContainerShard>,
    ) -> Result<AllocationLaunchEnvelope, AllocationRuntimePortError>;

    fn ingest_terminal_summary(
        &self,
        workspace: &WorkspacePlan,
        receipt: &ExternalBatchReceipt,
        shard: Option<&ContainerShard>,
    ) -> Result<Option<AllocationTerminalSummary>, AllocationRuntimePortError>;
}

/// Minimal local implementation that defines the expected launch-envelope shape.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalAllocationRuntimePort;

impl AllocationRuntimePort for LocalAllocationRuntimePort {
    fn build_launch_envelope(
        &self,
        workspace: &WorkspacePlan,
        receipt: &ExternalBatchReceipt,
        shard: Option<&ContainerShard>,
    ) -> Result<AllocationLaunchEnvelope, AllocationRuntimePortError> {
        let mut extra_env = BTreeMap::new();
        extra_env.insert("PATINA_RECEIPT_ID".into(), receipt.receipt_id.clone());
        extra_env.insert("PATINA_JOB_ID".into(), receipt.job_id.clone());
        extra_env.insert("PATINA_STAGE_ROOT".into(), workspace.stage_root.to_string());
        extra_env.insert(
            "PATINA_DURABLE_ROOT".into(),
            workspace.durable_job_root.to_string(),
        );
        extra_env.insert(
            "PATINA_MANIFEST_PATH".into(),
            workspace.manifest_path.to_string(),
        );

        if let Some(shard) = shard {
            extra_env.insert("PATINA_SHARD_ID".into(), shard.shard_id.clone());
        }

        Ok(AllocationLaunchEnvelope {
            receipt_id: receipt.receipt_id.clone(),
            job_id: receipt.job_id.clone(),
            shard_id: shard.map(|entry| entry.shard_id.clone()),
            stage_root: workspace.stage_root.to_string(),
            durable_root: workspace.durable_job_root.to_string(),
            manifest_path: workspace.manifest_path.to_string(),
            scheduler_workdir: receipt.workdir.clone(),
            extra_env,
        })
    }

    fn ingest_terminal_summary(
        &self,
        _workspace: &WorkspacePlan,
        _receipt: &ExternalBatchReceipt,
        _shard: Option<&ContainerShard>,
    ) -> Result<Option<AllocationTerminalSummary>, AllocationRuntimePortError> {
        Ok(None)
    }
}

#[derive(Debug, Error)]
pub enum AllocationRuntimePortError {
    #[error("allocation runtime bridge is not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("allocation runtime bridge contract error: {0}")]
    Contract(&'static str),
}

#[cfg(test)]
mod tests {
    use super::{AllocationRuntimePort, LocalAllocationRuntimePort};
    use crate::{
        ContainerShard, ExternalBatchReceipt, SchedulerFamily, SchedulerJobIdentifier,
        SchedulerReceiptState, ShardAssignmentPolicy, ShardRetryPolicy, ShardTelemetryRollup,
        WorkspacePlan,
    };

    fn sample_receipt() -> ExternalBatchReceipt {
        ExternalBatchReceipt {
            receipt_id: "slurm:123:job-1".into(),
            job_id: "job-1".into(),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::Slurm,
                allocation_id: "123".into(),
                step_id: None,
                array_job_id: None,
                array_index: None,
            },
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: 10,
            last_observed_at_ms: 10,
            launch_host: Some("login01".into()),
            workdir: Some("/scratch/job_job-1/stage".into()),
            launcher_provenance: None,
            last_telemetry: None,
        }
    }

    fn sample_workspace() -> WorkspacePlan {
        WorkspacePlan {
            job_id: "job-1".into(),
            durable_job_root: "/durable/job_job-1".into(),
            active_job_root: "/scratch/job_job-1".into(),
            stage_root: "/scratch/job_job-1/stage".into(),
            manifest_path: "/durable/job_job-1/manifest.json".into(),
        }
    }

    #[test]
    fn launch_envelope_exposes_known_paths_and_ids() {
        let port = LocalAllocationRuntimePort;
        let envelope = port
            .build_launch_envelope(&sample_workspace(), &sample_receipt(), None)
            .expect("envelope");

        assert_eq!(envelope.receipt_id, "slurm:123:job-1");
        assert_eq!(envelope.stage_root, "/scratch/job_job-1/stage");
        assert_eq!(
            envelope
                .extra_env
                .get("PATINA_MANIFEST_PATH")
                .map(String::as_str),
            Some("/durable/job_job-1/manifest.json")
        );
    }

    #[test]
    fn launch_envelope_includes_shard_identity_when_present() {
        let port = LocalAllocationRuntimePort;
        let shard = ContainerShard {
            shard_id: "shard-7".into(),
            scheduler_receipt_id: Some("slurm:123:job-1".into()),
            member_job_ids: vec!["job-1".into(), "job-2".into()],
            assignment_policy: ShardAssignmentPolicy::LeaseQueue,
            max_concurrency: 16,
            retry_policy: ShardRetryPolicy::default(),
            telemetry_rollup: ShardTelemetryRollup::default(),
            terminal_summary_relpath: "terminal_summary.json".into(),
        };

        let envelope = port
            .build_launch_envelope(&sample_workspace(), &sample_receipt(), Some(&shard))
            .expect("envelope");

        assert_eq!(envelope.shard_id.as_deref(), Some("shard-7"));
        assert_eq!(
            envelope
                .extra_env
                .get("PATINA_SHARD_ID")
                .map(String::as_str),
            Some("shard-7")
        );
    }
}
