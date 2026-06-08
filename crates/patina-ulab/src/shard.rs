use serde::{Deserialize, Serialize};

/// Assignment strategy for tasks owned by one scheduler-visible container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShardAssignmentPolicy {
    /// Membership is fixed when the shard is created.
    StaticMembership,
    /// Tasks are drained from a lease queue inside the allocation.
    LeaseQueue,
    /// Membership is selected by scheduler array index.
    SchedulerArrayIndex,
}

/// Retry and reclaim budget for one container shard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardRetryPolicy {
    pub max_attempts_per_job: u32,
    pub max_reclaims_per_job: u32,
    pub retry_failed_jobs: bool,
}

impl Default for ShardRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts_per_job: 2,
            max_reclaims_per_job: 1,
            retry_failed_jobs: true,
        }
    }
}

/// Compact rollup attached to a shard rather than to individual tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShardTelemetryRollup {
    pub running_jobs: u32,
    pub completed_jobs: u32,
    pub failed_jobs: u32,
    pub peak_rss_mb: Option<u64>,
    pub nodes: Vec<String>,
}

/// First-class container / shard record for running many Scott jobs in one allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerShard {
    pub shard_id: String,
    pub scheduler_receipt_id: Option<String>,
    pub member_job_ids: Vec<String>,
    pub assignment_policy: ShardAssignmentPolicy,
    pub max_concurrency: u32,
    pub retry_policy: ShardRetryPolicy,
    pub telemetry_rollup: ShardTelemetryRollup,
    pub terminal_summary_relpath: String,
}

impl ContainerShard {
    pub fn attach_receipt_id(&mut self, receipt_id: impl Into<String>) {
        self.scheduler_receipt_id = Some(receipt_id.into());
    }
}

#[cfg(test)]
mod tests {
    use super::{ContainerShard, ShardAssignmentPolicy, ShardRetryPolicy, ShardTelemetryRollup};

    #[test]
    fn shard_can_attach_scheduler_receipt() {
        let mut shard = ContainerShard {
            shard_id: "shard-a".into(),
            scheduler_receipt_id: None,
            member_job_ids: vec!["job-1".into(), "job-2".into()],
            assignment_policy: ShardAssignmentPolicy::LeaseQueue,
            max_concurrency: 32,
            retry_policy: ShardRetryPolicy::default(),
            telemetry_rollup: ShardTelemetryRollup::default(),
            terminal_summary_relpath: "terminal_summary.json".into(),
        };

        shard.attach_receipt_id("slurm:123:job-shard-a");
        assert_eq!(
            shard.scheduler_receipt_id.as_deref(),
            Some("slurm:123:job-shard-a")
        );
    }
}
