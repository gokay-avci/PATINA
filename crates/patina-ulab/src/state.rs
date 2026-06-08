use serde::{Deserialize, Serialize};

/// Detailed orchestration status for one Scott runtime job.
///
/// The intent is to keep scheduler state, lease state, staging state, and scientific completion
/// state distinguishable so the runtime can recover and retry safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratedJobStatus {
    Pending,
    Admitted,
    Leased,
    Materializing,
    Staged,
    SubmittedScheduler,
    QueuedScheduler,
    RunningExternal,
    Retrieving,
    Parsing,
    Completed,
    Failed,
    Cancelled,
    Reclaimed,
}

impl OrchestratedJobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Reclaimed
        )
    }
}

/// Reason for terminal or semi-terminal failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobFailureKind {
    Preflight,
    Launch,
    QueueTimeout,
    Runtime,
    Retrieval,
    Parse,
    Timeout,
    LostWorker,
    LostReceipt,
    CancelledByOperator,
}

/// DFT/external-engine failure class used by the HPC reconciliation layer.
///
/// This is more specific than `JobFailureKind`: it preserves where the failure happened without
/// forcing scheduler, retrieval, and parser errors into one generic runtime bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DftFailureClass {
    QueuedTooLong,
    TimedOut,
    Cancelled,
    RetrievalFailed,
    ParseFailed,
    MissingTerminalOutput,
    BackendRuntimeFailed,
    LostSchedulerReceipt,
}

impl DftFailureClass {
    pub fn job_failure_kind(self) -> JobFailureKind {
        match self {
            Self::QueuedTooLong => JobFailureKind::QueueTimeout,
            Self::TimedOut => JobFailureKind::Timeout,
            Self::Cancelled => JobFailureKind::CancelledByOperator,
            Self::RetrievalFailed => JobFailureKind::Retrieval,
            Self::ParseFailed => JobFailureKind::Parse,
            Self::MissingTerminalOutput => JobFailureKind::LostReceipt,
            Self::BackendRuntimeFailed => JobFailureKind::Runtime,
            Self::LostSchedulerReceipt => JobFailureKind::LostReceipt,
        }
    }
}

/// Compact profiling summary attached to one orchestrated job state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct JobProfilingSnapshot {
    pub peak_rss_mb: Option<u64>,
    pub peak_vm_mb: Option<u64>,
    pub cpu_time_secs: Option<f64>,
    pub wallclock_secs: Option<f64>,
    pub sampled_nodes: Vec<String>,
}

/// Durable projection of orchestration state for one Scott runtime job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestratedJobState {
    pub job_id: String,
    pub status: OrchestratedJobStatus,
    pub attempt: u32,
    pub lease_id: Option<String>,
    pub batch_receipt_id: Option<String>,
    pub worker_id: Option<String>,
    pub last_transition_ms: u64,
    pub failure_kind: Option<JobFailureKind>,
    pub profiling: JobProfilingSnapshot,
}

impl OrchestratedJobState {
    pub fn new(job_id: impl Into<String>, now_ms: u64) -> Self {
        Self {
            job_id: job_id.into(),
            status: OrchestratedJobStatus::Pending,
            attempt: 0,
            lease_id: None,
            batch_receipt_id: None,
            worker_id: None,
            last_transition_ms: now_ms,
            failure_kind: None,
            profiling: JobProfilingSnapshot::default(),
        }
    }

    pub fn transition(
        &mut self,
        next: OrchestratedJobStatus,
        now_ms: u64,
    ) -> Result<(), InvalidTransition> {
        if !self.status.can_transition_to(next) {
            return Err(InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        self.status = next;
        self.last_transition_ms = now_ms;
        Ok(())
    }

    pub fn fail(&mut self, kind: JobFailureKind, now_ms: u64) {
        self.failure_kind = Some(kind);
        self.status = OrchestratedJobStatus::Failed;
        self.last_transition_ms = now_ms;
    }
}

impl OrchestratedJobStatus {
    pub fn can_transition_to(self, next: OrchestratedJobStatus) -> bool {
        use OrchestratedJobStatus as S;

        matches!(
            (self, next),
            (S::Pending, S::Admitted)
                | (S::Admitted, S::Leased)
                | (S::Leased, S::Materializing)
                | (S::Materializing, S::Staged)
                | (S::Staged, S::SubmittedScheduler)
                | (S::SubmittedScheduler, S::QueuedScheduler)
                | (S::SubmittedScheduler, S::RunningExternal)
                | (S::QueuedScheduler, S::RunningExternal)
                | (S::RunningExternal, S::Retrieving)
                | (S::Retrieving, S::Parsing)
                | (S::Parsing, S::Completed)
                | (S::Leased, S::Reclaimed)
                | (S::Materializing, S::Reclaimed)
                | (S::SubmittedScheduler, S::Cancelled)
                | (S::QueuedScheduler, S::Cancelled)
                | (S::RunningExternal, S::Cancelled)
                | (S::Pending, S::Cancelled)
                | (S::Admitted, S::Cancelled)
                | (S::Pending, S::Failed)
                | (S::Admitted, S::Failed)
                | (S::Leased, S::Failed)
                | (S::Materializing, S::Failed)
                | (S::Staged, S::Failed)
                | (S::SubmittedScheduler, S::Failed)
                | (S::QueuedScheduler, S::Failed)
                | (S::RunningExternal, S::Failed)
                | (S::Retrieving, S::Failed)
                | (S::Parsing, S::Failed)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: OrchestratedJobStatus,
    pub to: OrchestratedJobStatus,
}

#[cfg(test)]
mod tests {
    use super::{DftFailureClass, JobFailureKind, OrchestratedJobState, OrchestratedJobStatus};

    #[test]
    fn orchestrated_state_accepts_valid_progression() {
        let mut state = OrchestratedJobState::new("job-1", 0);
        state
            .transition(OrchestratedJobStatus::Admitted, 1)
            .unwrap();
        state.transition(OrchestratedJobStatus::Leased, 2).unwrap();
        state
            .transition(OrchestratedJobStatus::Materializing, 3)
            .unwrap();
        state.transition(OrchestratedJobStatus::Staged, 4).unwrap();
        state
            .transition(OrchestratedJobStatus::SubmittedScheduler, 5)
            .unwrap();
        state
            .transition(OrchestratedJobStatus::QueuedScheduler, 6)
            .unwrap();
        state
            .transition(OrchestratedJobStatus::RunningExternal, 7)
            .unwrap();
        state
            .transition(OrchestratedJobStatus::Retrieving, 8)
            .unwrap();
        state.transition(OrchestratedJobStatus::Parsing, 9).unwrap();
        state
            .transition(OrchestratedJobStatus::Completed, 10)
            .unwrap();

        assert!(state.status.is_terminal());
    }

    #[test]
    fn orchestrated_state_rejects_invalid_jump() {
        let mut state = OrchestratedJobState::new("job-2", 0);
        let error = state
            .transition(OrchestratedJobStatus::RunningExternal, 1)
            .expect_err("should reject jump");
        assert_eq!(error.from, OrchestratedJobStatus::Pending);
        assert_eq!(error.to, OrchestratedJobStatus::RunningExternal);
    }

    #[test]
    fn orchestrated_state_records_failure_kind() {
        let mut state = OrchestratedJobState::new("job-3", 0);
        state
            .transition(OrchestratedJobStatus::Admitted, 1)
            .unwrap();
        state.fail(JobFailureKind::Timeout, 2);
        assert_eq!(state.failure_kind, Some(JobFailureKind::Timeout));
        assert_eq!(state.status, OrchestratedJobStatus::Failed);
    }

    #[test]
    fn dft_failure_class_maps_to_generic_failure_kind() {
        assert_eq!(
            DftFailureClass::RetrievalFailed.job_failure_kind(),
            JobFailureKind::Retrieval
        );
        assert_eq!(
            DftFailureClass::QueuedTooLong.job_failure_kind(),
            JobFailureKind::QueueTimeout
        );
    }
}
