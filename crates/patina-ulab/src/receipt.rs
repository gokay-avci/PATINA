use serde::{Deserialize, Serialize};

use crate::{LauncherProvenance, SchedulerFamily};

/// Scheduler-native identifier for an external batch job or step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerJobIdentifier {
    pub scheduler_family: SchedulerFamily,
    pub allocation_id: String,
    pub step_id: Option<String>,
    pub array_job_id: Option<String>,
    pub array_index: Option<u32>,
}

/// Live state of a scheduler receipt as understood by the orchestration layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerReceiptState {
    Submitted,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Lost,
}

/// Lightweight telemetry record sampled while a batch job is running.
///
/// Keep this compact. This is control-plane data, not a full profiling dump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TelemetrySample {
    pub sampled_at_ms: u64,
    pub rss_mb: Option<u64>,
    pub vm_mb: Option<u64>,
    pub cpu_time_secs: Option<f64>,
    pub gpu_util_percent: Option<f64>,
    pub node_list: Vec<String>,
}

/// One process observed inside a scheduler allocation for a specific job wrapper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobProcessTelemetry {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub rss_mb: Option<u64>,
    pub vm_mb: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub command: Option<String>,
}

/// Job-scoped process telemetry sampled from an allocation-local runtime wrapper.
///
/// This is the first compact shape for the Young shard metric upgrade: unlike
/// `TelemetrySample`, it describes only the tracked process tree for one logical job or shard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct JobScopedTelemetrySample {
    pub sampled_at_ms: u64,
    pub root_pid: Option<u32>,
    pub process_count: u32,
    pub rss_total_mb: Option<u64>,
    pub rss_peak_mb: Option<u64>,
    pub vm_total_mb: Option<u64>,
    pub cpu_percent_total: Option<f64>,
    pub gpu_util_percent: Option<f64>,
    pub node_list: Vec<String>,
    pub processes: Vec<JobProcessTelemetry>,
}

impl JobScopedTelemetrySample {
    pub fn from_processes(
        sampled_at_ms: u64,
        root_pid: Option<u32>,
        gpu_util_percent: Option<f64>,
        node_list: Vec<String>,
        processes: Vec<JobProcessTelemetry>,
    ) -> Self {
        let rss_values = processes.iter().filter_map(|process| process.rss_mb);
        let rss_total_mb = sum_u64(rss_values.clone());
        let rss_peak_mb = rss_values.max();
        let vm_total_mb = sum_u64(processes.iter().filter_map(|process| process.vm_mb));
        let cpu_percent_total = sum_f64(processes.iter().filter_map(|process| process.cpu_percent));

        Self {
            sampled_at_ms,
            root_pid,
            process_count: processes.len() as u32,
            rss_total_mb,
            rss_peak_mb,
            vm_total_mb,
            cpu_percent_total,
            gpu_util_percent,
            node_list,
            processes,
        }
    }
}

fn sum_u64(values: impl Iterator<Item = u64>) -> Option<u64> {
    let mut saw_any = false;
    let mut total = 0;
    for value in values {
        saw_any = true;
        total += value;
    }
    saw_any.then_some(total)
}

fn sum_f64(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut saw_any = false;
    let mut total = 0.0;
    for value in values {
        saw_any = true;
        total += value;
    }
    saw_any.then_some(total)
}

/// External batch execution receipt tracked by `patina-ulab`.
///
/// This is distinct from `ScottDispatchReceipt` because one Scott runtime job may be implemented
/// by a scheduler-visible batch job or step whose lifecycle must be polled and recovered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalBatchReceipt {
    pub receipt_id: String,
    pub job_id: String,
    pub scheduler_job: SchedulerJobIdentifier,
    pub state: SchedulerReceiptState,
    pub submitted_at_ms: u64,
    pub last_observed_at_ms: u64,
    pub launch_host: Option<String>,
    pub workdir: Option<String>,
    pub launcher_provenance: Option<LauncherProvenance>,
    pub last_telemetry: Option<TelemetrySample>,
}

impl ExternalBatchReceipt {
    pub fn mark_observed(&mut self, now_ms: u64, state: SchedulerReceiptState) {
        self.last_observed_at_ms = now_ms;
        self.state = state;
    }

    pub fn attach_telemetry(&mut self, sample: TelemetrySample) {
        self.last_observed_at_ms = sample.sampled_at_ms;
        self.last_telemetry = Some(sample);
    }

    pub fn attach_launcher_provenance(&mut self, provenance: LauncherProvenance) {
        self.launcher_provenance = Some(provenance);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExternalBatchReceipt, JobProcessTelemetry, JobScopedTelemetrySample,
        SchedulerJobIdentifier, SchedulerReceiptState, TelemetrySample,
    };
    use crate::SchedulerFamily;

    #[test]
    fn receipt_updates_state_and_timestamp() {
        let mut receipt = ExternalBatchReceipt {
            receipt_id: "r-1".into(),
            job_id: "job-1".into(),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::Slurm,
                allocation_id: "12345".into(),
                step_id: Some("0".into()),
                array_job_id: None,
                array_index: None,
            },
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: 10,
            last_observed_at_ms: 10,
            launch_host: None,
            workdir: None,
            launcher_provenance: None,
            last_telemetry: None,
        };

        receipt.mark_observed(22, SchedulerReceiptState::Running);
        assert_eq!(receipt.last_observed_at_ms, 22);
        assert_eq!(receipt.state, SchedulerReceiptState::Running);
    }

    #[test]
    fn receipt_attaches_telemetry() {
        let mut receipt = ExternalBatchReceipt {
            receipt_id: "r-2".into(),
            job_id: "job-2".into(),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::Pbs,
                allocation_id: "777".into(),
                step_id: None,
                array_job_id: None,
                array_index: None,
            },
            state: SchedulerReceiptState::Running,
            submitted_at_ms: 10,
            last_observed_at_ms: 10,
            launch_host: None,
            workdir: None,
            launcher_provenance: None,
            last_telemetry: None,
        };

        receipt.attach_telemetry(TelemetrySample {
            sampled_at_ms: 33,
            rss_mb: Some(1024),
            vm_mb: None,
            cpu_time_secs: Some(7.5),
            gpu_util_percent: None,
            node_list: vec!["n01".into()],
        });

        assert_eq!(receipt.last_observed_at_ms, 33);
        assert_eq!(
            receipt.last_telemetry.as_ref().and_then(|t| t.rss_mb),
            Some(1024)
        );
    }

    #[test]
    fn job_scoped_telemetry_rolls_up_process_tree() {
        let sample = JobScopedTelemetrySample::from_processes(
            99,
            Some(10),
            Some(42.0),
            vec!["node-a".into()],
            vec![
                JobProcessTelemetry {
                    pid: 10,
                    ppid: Some(1),
                    rss_mb: Some(100),
                    vm_mb: Some(500),
                    cpu_percent: Some(25.5),
                    command: Some("patina-driver".into()),
                },
                JobProcessTelemetry {
                    pid: 11,
                    ppid: Some(10),
                    rss_mb: Some(50),
                    vm_mb: Some(250),
                    cpu_percent: Some(10.0),
                    command: Some("gulp".into()),
                },
            ],
        );

        assert_eq!(sample.process_count, 2);
        assert_eq!(sample.rss_total_mb, Some(150));
        assert_eq!(sample.rss_peak_mb, Some(100));
        assert_eq!(sample.vm_total_mb, Some(750));
        assert_eq!(sample.cpu_percent_total, Some(35.5));
        assert_eq!(sample.node_list, vec!["node-a"]);
    }

    #[test]
    fn receipt_attaches_launcher_provenance() {
        let mut receipt = ExternalBatchReceipt {
            receipt_id: "r-3".into(),
            job_id: "job-3".into(),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::Slurm,
                allocation_id: "999".into(),
                step_id: None,
                array_job_id: None,
                array_index: None,
            },
            state: SchedulerReceiptState::Submitted,
            submitted_at_ms: 10,
            last_observed_at_ms: 10,
            launch_host: Some("login".into()),
            workdir: Some("/scratch/job-3".into()),
            launcher_provenance: None,
            last_telemetry: None,
        };

        receipt.attach_launcher_provenance(crate::SiteProfile::archer2().launcher_provenance());
        assert_eq!(
            receipt
                .launcher_provenance
                .as_ref()
                .map(|p| p.launch_command.as_str()),
            Some("srun")
        );
    }
}
