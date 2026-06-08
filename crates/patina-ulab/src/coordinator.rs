use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AllocationTaskPhase, AllocationTerminalSummary, ContainerShard, ExternalBatchReceipt,
    SchedulerReceiptState, SiteProfile,
};

/// Coordinator-visible budget for scheduler-backed shard admission.
///
/// This keeps active-shard control explicit and site-aware instead of relying on ad hoc submit
/// loops in shell scripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardCoordinatorBudget {
    pub requested_active_shards: u32,
    pub site_scheduler_job_cap: Option<u32>,
}

impl ShardCoordinatorBudget {
    pub fn from_site_profile(
        site_profile: &SiteProfile,
        requested_active_shards: Option<u32>,
    ) -> Self {
        let site_scheduler_job_cap = match site_profile
            .submission_policy
            .max_scheduler_jobs_per_campaign
        {
            0 => None,
            value => Some(value),
        };
        let requested_active_shards =
            requested_active_shards.unwrap_or_else(|| site_scheduler_job_cap.unwrap_or(1));

        Self {
            requested_active_shards,
            site_scheduler_job_cap,
        }
    }

    pub fn effective_active_shard_limit(&self) -> u32 {
        match self.site_scheduler_job_cap {
            Some(cap) => self.requested_active_shards.min(cap),
            None => self.requested_active_shards,
        }
    }
}

/// Coordinator-level classification of one shard while deciding active admission and refill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorShardState {
    PendingSubmission,
    AwaitingReceiptRecovery,
    ActiveScheduler,
    AwaitingTerminalSummary,
    TerminalCompleted,
    TerminalFailed,
}

impl CoordinatorShardState {
    pub fn occupies_scheduler_slot(self) -> bool {
        matches!(self, Self::AwaitingReceiptRecovery | Self::ActiveScheduler)
    }
}

/// Recovery and admission inputs for one shard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoordinatorShardSnapshot {
    pub shard: ContainerShard,
    pub receipt: Option<ExternalBatchReceipt>,
    pub terminal_summary: Option<AllocationTerminalSummary>,
}

/// Recovered member-level view for one shard after coordinator restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardMembershipRecovery {
    pub shard_id: String,
    pub recovered_member_job_ids: Vec<String>,
    pub completed_member_job_ids: Vec<String>,
    pub failed_member_job_ids: Vec<String>,
    pub salvageable_member_job_ids: Vec<String>,
    pub missing_terminal_member_job_ids: Vec<String>,
}

/// Terminal shard classification after member-level recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShardTerminalDisposition {
    CompleteSuccess,
    PartialSuccess,
    CompleteFailure,
    IncompleteEvidence,
}

/// Coordinator decision for salvage, retry, and partial-success handling in one shard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardTerminalOutcome {
    pub shard_id: String,
    pub disposition: ShardTerminalDisposition,
    pub accepted_member_job_ids: Vec<String>,
    pub salvageable_member_job_ids: Vec<String>,
    pub retry_member_job_ids: Vec<String>,
    pub failed_member_job_ids: Vec<String>,
    pub missing_terminal_member_job_ids: Vec<String>,
}

impl ShardMembershipRecovery {
    pub fn terminal_outcome(&self, shard: &ContainerShard) -> ShardTerminalOutcome {
        let mut accepted_member_job_ids = self.completed_member_job_ids.clone();
        for job_id in &self.salvageable_member_job_ids {
            if !accepted_member_job_ids.contains(job_id) {
                accepted_member_job_ids.push(job_id.clone());
            }
        }

        let mut retry_member_job_ids = Vec::new();
        if shard.retry_policy.retry_failed_jobs {
            for job_id in self
                .failed_member_job_ids
                .iter()
                .chain(self.missing_terminal_member_job_ids.iter())
            {
                if !accepted_member_job_ids.contains(job_id)
                    && !retry_member_job_ids.contains(job_id)
                {
                    retry_member_job_ids.push(job_id.clone());
                }
            }
        }

        let disposition = if !self.missing_terminal_member_job_ids.is_empty() {
            ShardTerminalDisposition::IncompleteEvidence
        } else if accepted_member_job_ids.len() == self.recovered_member_job_ids.len()
            && !accepted_member_job_ids.is_empty()
        {
            ShardTerminalDisposition::CompleteSuccess
        } else if !accepted_member_job_ids.is_empty() {
            ShardTerminalDisposition::PartialSuccess
        } else {
            ShardTerminalDisposition::CompleteFailure
        };

        ShardTerminalOutcome {
            shard_id: self.shard_id.clone(),
            disposition,
            accepted_member_job_ids,
            salvageable_member_job_ids: self.salvageable_member_job_ids.clone(),
            retry_member_job_ids,
            failed_member_job_ids: self.failed_member_job_ids.clone(),
            missing_terminal_member_job_ids: self.missing_terminal_member_job_ids.clone(),
        }
    }
}

impl CoordinatorShardSnapshot {
    pub fn coordinator_state(&self) -> Result<CoordinatorShardState, ShardCoordinatorError> {
        self.validate_receipt_identity()?;
        let summary_phase = self.summary_terminal_phase()?;

        match &self.receipt {
            Some(receipt) => Ok(match receipt.state {
                SchedulerReceiptState::Submitted
                | SchedulerReceiptState::Queued
                | SchedulerReceiptState::Running => CoordinatorShardState::ActiveScheduler,
                SchedulerReceiptState::Completed => match summary_phase {
                    Some(AllocationTaskPhase::Completed) => {
                        CoordinatorShardState::TerminalCompleted
                    }
                    Some(AllocationTaskPhase::Failed) => CoordinatorShardState::TerminalFailed,
                    None => CoordinatorShardState::AwaitingTerminalSummary,
                    Some(other) => {
                        return Err(ShardCoordinatorError::NonTerminalSummaryPhase {
                            shard_id: self.shard.shard_id.clone(),
                            phase: other,
                        });
                    }
                },
                SchedulerReceiptState::Failed
                | SchedulerReceiptState::Cancelled
                | SchedulerReceiptState::Lost => CoordinatorShardState::TerminalFailed,
            }),
            None => {
                if self.shard.scheduler_receipt_id.is_some() {
                    Ok(CoordinatorShardState::AwaitingReceiptRecovery)
                } else {
                    Ok(match summary_phase {
                        Some(AllocationTaskPhase::Completed) => {
                            CoordinatorShardState::TerminalCompleted
                        }
                        Some(AllocationTaskPhase::Failed) => CoordinatorShardState::TerminalFailed,
                        None => CoordinatorShardState::PendingSubmission,
                        Some(other) => {
                            return Err(ShardCoordinatorError::NonTerminalSummaryPhase {
                                shard_id: self.shard.shard_id.clone(),
                                phase: other,
                            });
                        }
                    })
                }
            }
        }
    }

    fn validate_receipt_identity(&self) -> Result<(), ShardCoordinatorError> {
        if let (Some(attached_receipt_id), Some(receipt)) = (
            self.shard.scheduler_receipt_id.as_deref(),
            self.receipt.as_ref(),
        ) {
            if attached_receipt_id != receipt.receipt_id {
                return Err(ShardCoordinatorError::ReceiptMismatch {
                    shard_id: self.shard.shard_id.clone(),
                    attached_receipt_id: attached_receipt_id.into(),
                    observed_receipt_id: receipt.receipt_id.clone(),
                });
            }
        }

        if let Some(summary) = self.terminal_summary.as_ref() {
            if let Some(shard_id) = summary.shard_id.as_deref() {
                if shard_id != self.shard.shard_id {
                    return Err(ShardCoordinatorError::SummaryShardMismatch {
                        shard_id: self.shard.shard_id.clone(),
                        summary_shard_id: shard_id.into(),
                    });
                }
            }

            if let Some(receipt) = self.receipt.as_ref() {
                if summary.receipt_id != receipt.receipt_id {
                    return Err(ShardCoordinatorError::SummaryReceiptMismatch {
                        shard_id: self.shard.shard_id.clone(),
                        expected_receipt_id: receipt.receipt_id.clone(),
                        summary_receipt_id: summary.receipt_id.clone(),
                    });
                }
            } else if let Some(attached_receipt_id) = self.shard.scheduler_receipt_id.as_deref() {
                if summary.receipt_id != attached_receipt_id {
                    return Err(ShardCoordinatorError::SummaryReceiptMismatch {
                        shard_id: self.shard.shard_id.clone(),
                        expected_receipt_id: attached_receipt_id.into(),
                        summary_receipt_id: summary.receipt_id.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    fn summary_terminal_phase(&self) -> Result<Option<AllocationTaskPhase>, ShardCoordinatorError> {
        match self.terminal_summary.as_ref() {
            Some(summary) if !summary.final_phase.is_terminal() => {
                Err(ShardCoordinatorError::NonTerminalSummaryPhase {
                    shard_id: self.shard.shard_id.clone(),
                    phase: summary.final_phase,
                })
            }
            Some(summary) => Ok(Some(summary.final_phase)),
            None => Ok(None),
        }
    }

    pub fn recover_membership(&self) -> Result<ShardMembershipRecovery, ShardCoordinatorError> {
        self.validate_receipt_identity()?;

        let recovered_member_job_ids = self.shard.member_job_ids.clone();
        let Some(summary) = self.terminal_summary.as_ref() else {
            return Ok(ShardMembershipRecovery {
                shard_id: self.shard.shard_id.clone(),
                missing_terminal_member_job_ids: recovered_member_job_ids.clone(),
                recovered_member_job_ids,
                completed_member_job_ids: Vec::new(),
                failed_member_job_ids: Vec::new(),
                salvageable_member_job_ids: Vec::new(),
            });
        };

        let completed_member_job_ids =
            filter_known_members(&self.shard.member_job_ids, &summary.completed_job_ids);
        let failed_member_job_ids =
            filter_known_members(&self.shard.member_job_ids, &summary.failed_job_ids);
        let salvageable_member_job_ids =
            filter_known_members(&self.shard.member_job_ids, &summary.salvageable_job_ids);
        let missing_terminal_member_job_ids = self
            .shard
            .member_job_ids
            .iter()
            .filter(|job_id| {
                !completed_member_job_ids.contains(job_id)
                    && !failed_member_job_ids.contains(job_id)
                    && !salvageable_member_job_ids.contains(job_id)
            })
            .cloned()
            .collect();

        Ok(ShardMembershipRecovery {
            shard_id: self.shard.shard_id.clone(),
            recovered_member_job_ids,
            completed_member_job_ids,
            failed_member_job_ids,
            salvageable_member_job_ids,
            missing_terminal_member_job_ids,
        })
    }

    pub fn terminal_outcome(&self) -> Result<ShardTerminalOutcome, ShardCoordinatorError> {
        Ok(self.recover_membership()?.terminal_outcome(&self.shard))
    }
}

fn filter_known_members(known_members: &[String], observed: &[String]) -> Vec<String> {
    known_members
        .iter()
        .filter(|job_id| observed.iter().any(|observed_id| observed_id == *job_id))
        .cloned()
        .collect()
}

/// Admission and refill plan produced by the coordinator for one campaign snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardAdmissionPlan {
    pub budget: ShardCoordinatorBudget,
    pub active_scheduler_shard_ids: Vec<String>,
    pub recovering_receipt_shard_ids: Vec<String>,
    pub awaiting_terminal_summary_shard_ids: Vec<String>,
    pub terminal_completed_shard_ids: Vec<String>,
    pub terminal_failed_shard_ids: Vec<String>,
    pub admit_next_shard_ids: Vec<String>,
    pub deferred_pending_shard_ids: Vec<String>,
}

impl ShardAdmissionPlan {
    pub fn occupied_scheduler_slots(&self) -> u32 {
        (self.active_scheduler_shard_ids.len() + self.recovering_receipt_shard_ids.len()) as u32
    }

    pub fn free_scheduler_slots(&self) -> u32 {
        self.budget
            .effective_active_shard_limit()
            .saturating_sub(self.occupied_scheduler_slots())
    }
}

/// Coordinator-side planner that converts shard snapshots into a bounded admission/refill plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardCoordinator {
    site_profile: SiteProfile,
    budget: ShardCoordinatorBudget,
}

impl ShardCoordinator {
    pub fn new(site_profile: SiteProfile, requested_active_shards: Option<u32>) -> Self {
        let budget =
            ShardCoordinatorBudget::from_site_profile(&site_profile, requested_active_shards);
        Self {
            site_profile,
            budget,
        }
    }

    pub fn site_profile(&self) -> &SiteProfile {
        &self.site_profile
    }

    pub fn budget(&self) -> &ShardCoordinatorBudget {
        &self.budget
    }

    pub fn plan(
        &self,
        snapshots: &[CoordinatorShardSnapshot],
    ) -> Result<ShardAdmissionPlan, ShardCoordinatorError> {
        let mut active_scheduler_shard_ids = Vec::new();
        let mut recovering_receipt_shard_ids = Vec::new();
        let mut awaiting_terminal_summary_shard_ids = Vec::new();
        let mut terminal_completed_shard_ids = Vec::new();
        let mut terminal_failed_shard_ids = Vec::new();
        let mut pending_shard_ids = Vec::new();

        for snapshot in snapshots {
            match snapshot.coordinator_state()? {
                CoordinatorShardState::PendingSubmission => {
                    pending_shard_ids.push(snapshot.shard.shard_id.clone());
                }
                CoordinatorShardState::AwaitingReceiptRecovery => {
                    recovering_receipt_shard_ids.push(snapshot.shard.shard_id.clone());
                }
                CoordinatorShardState::ActiveScheduler => {
                    active_scheduler_shard_ids.push(snapshot.shard.shard_id.clone());
                }
                CoordinatorShardState::AwaitingTerminalSummary => {
                    awaiting_terminal_summary_shard_ids.push(snapshot.shard.shard_id.clone());
                }
                CoordinatorShardState::TerminalCompleted => {
                    terminal_completed_shard_ids.push(snapshot.shard.shard_id.clone());
                }
                CoordinatorShardState::TerminalFailed => {
                    terminal_failed_shard_ids.push(snapshot.shard.shard_id.clone());
                }
            }
        }

        let occupied_scheduler_slots =
            (active_scheduler_shard_ids.len() + recovering_receipt_shard_ids.len()) as u32;
        let free_scheduler_slots = self
            .budget
            .effective_active_shard_limit()
            .saturating_sub(occupied_scheduler_slots);
        let admit_count = (free_scheduler_slots as usize).min(pending_shard_ids.len());
        let admit_next_shard_ids = pending_shard_ids
            .iter()
            .take(admit_count)
            .cloned()
            .collect();
        let deferred_pending_shard_ids = pending_shard_ids
            .iter()
            .skip(admit_count)
            .cloned()
            .collect();

        Ok(ShardAdmissionPlan {
            budget: self.budget.clone(),
            active_scheduler_shard_ids,
            recovering_receipt_shard_ids,
            awaiting_terminal_summary_shard_ids,
            terminal_completed_shard_ids,
            terminal_failed_shard_ids,
            admit_next_shard_ids,
            deferred_pending_shard_ids,
        })
    }
}

#[derive(Debug, Error)]
pub enum ShardCoordinatorError {
    #[error(
        "shard `{shard_id}` attached receipt `{attached_receipt_id}` does not match observed receipt `{observed_receipt_id}`"
    )]
    ReceiptMismatch {
        shard_id: String,
        attached_receipt_id: String,
        observed_receipt_id: String,
    },
    #[error(
        "shard `{shard_id}` terminal summary claims shard `{summary_shard_id}` instead of the coordinator shard"
    )]
    SummaryShardMismatch {
        shard_id: String,
        summary_shard_id: String,
    },
    #[error(
        "shard `{shard_id}` terminal summary receipt `{summary_receipt_id}` does not match expected receipt `{expected_receipt_id}`"
    )]
    SummaryReceiptMismatch {
        shard_id: String,
        expected_receipt_id: String,
        summary_receipt_id: String,
    },
    #[error("shard `{shard_id}` reported non-terminal terminal-summary phase `{phase:?}`")]
    NonTerminalSummaryPhase {
        shard_id: String,
        phase: AllocationTaskPhase,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinatorShardSnapshot, CoordinatorShardState, ShardCoordinator, ShardCoordinatorBudget,
        ShardCoordinatorError, ShardTerminalDisposition,
    };
    use crate::{
        AllocationTaskPhase, AllocationTerminalSummary, ContainerShard, ExternalBatchReceipt,
        SchedulerFamily, SchedulerJobIdentifier, SchedulerReceiptState, ShardAssignmentPolicy,
        ShardRetryPolicy, ShardTelemetryRollup, SiteProfile,
    };

    fn shard(shard_id: &str) -> ContainerShard {
        ContainerShard {
            shard_id: shard_id.into(),
            scheduler_receipt_id: None,
            member_job_ids: vec![format!("job-{shard_id}")],
            assignment_policy: ShardAssignmentPolicy::StaticMembership,
            max_concurrency: 1,
            retry_policy: ShardRetryPolicy::default(),
            telemetry_rollup: ShardTelemetryRollup::default(),
            terminal_summary_relpath: format!("health/{shard_id}.terminal_summary.json"),
        }
    }

    fn receipt(shard_id: &str, state: SchedulerReceiptState) -> ExternalBatchReceipt {
        let receipt_id = format!("grid_engine:42:{shard_id}");
        ExternalBatchReceipt {
            receipt_id: receipt_id.clone(),
            job_id: format!("campaign-shard:{shard_id}"),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::GridEngine,
                allocation_id: "42".into(),
                step_id: None,
                array_job_id: None,
                array_index: None,
            },
            state,
            submitted_at_ms: 10,
            last_observed_at_ms: 20,
            launch_host: Some("login01".into()),
            workdir: Some(format!("/shared/{shard_id}")),
            launcher_provenance: None,
            last_telemetry: None,
        }
    }

    fn summary(
        shard_id: &str,
        receipt_id: &str,
        final_phase: AllocationTaskPhase,
    ) -> AllocationTerminalSummary {
        AllocationTerminalSummary {
            receipt_id: receipt_id.into(),
            shard_id: Some(shard_id.into()),
            observed_at_ms: 30,
            final_phase,
            completed_job_ids: vec![format!("job-{shard_id}")],
            failed_job_ids: Vec::new(),
            salvageable_job_ids: Vec::new(),
            artifact_roots: vec![format!("/shared/{shard_id}/artifacts")],
            message: None,
        }
    }

    #[test]
    fn budget_uses_site_cap_when_requested_limit_is_higher() {
        let budget = ShardCoordinatorBudget::from_site_profile(&SiteProfile::young(), Some(100));
        assert_eq!(budget.site_scheduler_job_cap, Some(32));
        assert_eq!(budget.effective_active_shard_limit(), 32);
    }

    #[test]
    fn budget_defaults_to_site_cap_for_scheduler_sites() {
        let budget = ShardCoordinatorBudget::from_site_profile(&SiteProfile::archer2(), None);
        assert_eq!(budget.requested_active_shards, 64);
        assert_eq!(budget.effective_active_shard_limit(), 64);
    }

    #[test]
    fn snapshot_without_receipt_is_pending_submission() {
        let state = CoordinatorShardSnapshot {
            shard: shard("shard-a"),
            receipt: None,
            terminal_summary: None,
        }
        .coordinator_state()
        .expect("state");

        assert_eq!(state, CoordinatorShardState::PendingSubmission);
    }

    #[test]
    fn snapshot_with_missing_attached_receipt_is_held_for_recovery() {
        let mut held_shard = shard("shard-b");
        held_shard.attach_receipt_id("grid_engine:42:shard-b");

        let state = CoordinatorShardSnapshot {
            shard: held_shard,
            receipt: None,
            terminal_summary: None,
        }
        .coordinator_state()
        .expect("state");

        assert_eq!(state, CoordinatorShardState::AwaitingReceiptRecovery);
    }

    #[test]
    fn completed_receipt_without_summary_waits_for_postrun_recovery() {
        let mut completed_shard = shard("shard-c");
        let receipt = receipt("shard-c", SchedulerReceiptState::Completed);
        completed_shard.attach_receipt_id(receipt.receipt_id.clone());

        let state = CoordinatorShardSnapshot {
            shard: completed_shard,
            receipt: Some(receipt),
            terminal_summary: None,
        }
        .coordinator_state()
        .expect("state");

        assert_eq!(state, CoordinatorShardState::AwaitingTerminalSummary);
    }

    #[test]
    fn planner_refills_only_up_to_free_budget() {
        let coordinator = ShardCoordinator::new(SiteProfile::young(), Some(2));

        let mut running = shard("shard-running");
        let running_receipt = receipt("shard-running", SchedulerReceiptState::Running);
        running.attach_receipt_id(running_receipt.receipt_id.clone());

        let mut completed = shard("shard-completed");
        let completed_receipt = receipt("shard-completed", SchedulerReceiptState::Completed);
        completed.attach_receipt_id(completed_receipt.receipt_id.clone());

        let pending_one = shard("shard-pending-1");
        let pending_two = shard("shard-pending-2");

        let plan = coordinator
            .plan(&[
                CoordinatorShardSnapshot {
                    shard: running,
                    receipt: Some(running_receipt),
                    terminal_summary: None,
                },
                CoordinatorShardSnapshot {
                    shard: completed.clone(),
                    receipt: Some(completed_receipt.clone()),
                    terminal_summary: Some(summary(
                        "shard-completed",
                        &completed_receipt.receipt_id,
                        AllocationTaskPhase::Completed,
                    )),
                },
                CoordinatorShardSnapshot {
                    shard: pending_one,
                    receipt: None,
                    terminal_summary: None,
                },
                CoordinatorShardSnapshot {
                    shard: pending_two,
                    receipt: None,
                    terminal_summary: None,
                },
            ])
            .expect("plan");

        assert_eq!(plan.active_scheduler_shard_ids, vec!["shard-running"]);
        assert_eq!(plan.terminal_completed_shard_ids, vec!["shard-completed"]);
        assert_eq!(plan.admit_next_shard_ids, vec!["shard-pending-1"]);
        assert_eq!(plan.deferred_pending_shard_ids, vec!["shard-pending-2"]);
        assert_eq!(plan.occupied_scheduler_slots(), 1);
        assert_eq!(plan.free_scheduler_slots(), 1);
    }

    #[test]
    fn recovery_hold_prevents_oversubmitting_after_restart() {
        let coordinator = ShardCoordinator::new(SiteProfile::young(), Some(2));

        let mut recovering = shard("shard-recovering");
        recovering.attach_receipt_id("grid_engine:42:shard-recovering");

        let mut running = shard("shard-running");
        let running_receipt = receipt("shard-running", SchedulerReceiptState::Running);
        running.attach_receipt_id(running_receipt.receipt_id.clone());

        let plan = coordinator
            .plan(&[
                CoordinatorShardSnapshot {
                    shard: recovering,
                    receipt: None,
                    terminal_summary: None,
                },
                CoordinatorShardSnapshot {
                    shard: running,
                    receipt: Some(running_receipt),
                    terminal_summary: None,
                },
                CoordinatorShardSnapshot {
                    shard: shard("shard-pending"),
                    receipt: None,
                    terminal_summary: None,
                },
            ])
            .expect("plan");

        assert_eq!(plan.recovering_receipt_shard_ids, vec!["shard-recovering"]);
        assert_eq!(plan.active_scheduler_shard_ids, vec!["shard-running"]);
        assert!(plan.admit_next_shard_ids.is_empty());
        assert_eq!(plan.deferred_pending_shard_ids, vec!["shard-pending"]);
    }

    #[test]
    fn snapshot_rejects_summary_receipt_mismatch() {
        let mut completed = shard("shard-mismatch");
        let receipt = receipt("shard-mismatch", SchedulerReceiptState::Completed);
        completed.attach_receipt_id(receipt.receipt_id.clone());

        let error = CoordinatorShardSnapshot {
            shard: completed,
            receipt: Some(receipt),
            terminal_summary: Some(summary(
                "shard-mismatch",
                "grid_engine:42:different",
                AllocationTaskPhase::Completed,
            )),
        }
        .coordinator_state()
        .expect_err("mismatch should be rejected");

        assert!(matches!(
            error,
            ShardCoordinatorError::SummaryReceiptMismatch { .. }
        ));
    }

    #[test]
    fn shard_membership_recovery_classifies_terminal_members() {
        let mut shard = shard("shard-recover");
        shard.member_job_ids = vec!["job-a".into(), "job-b".into(), "job-c".into()];
        let receipt = receipt("shard-recover", SchedulerReceiptState::Completed);
        shard.attach_receipt_id(receipt.receipt_id.clone());
        let mut terminal = summary(
            "shard-recover",
            &receipt.receipt_id,
            AllocationTaskPhase::Completed,
        );
        terminal.completed_job_ids = vec!["job-a".into()];
        terminal.failed_job_ids = vec!["job-b".into(), "unknown-job".into()];
        terminal.salvageable_job_ids = vec!["job-b".into()];

        let recovered = CoordinatorShardSnapshot {
            shard,
            receipt: Some(receipt),
            terminal_summary: Some(terminal),
        }
        .recover_membership()
        .expect("recover membership");

        assert_eq!(
            recovered.recovered_member_job_ids,
            vec!["job-a", "job-b", "job-c"]
        );
        assert_eq!(recovered.completed_member_job_ids, vec!["job-a"]);
        assert_eq!(recovered.failed_member_job_ids, vec!["job-b"]);
        assert_eq!(recovered.salvageable_member_job_ids, vec!["job-b"]);
        assert_eq!(recovered.missing_terminal_member_job_ids, vec!["job-c"]);
    }

    #[test]
    fn shard_membership_recovery_without_summary_marks_all_members_missing() {
        let mut shard = shard("shard-missing-summary");
        shard.member_job_ids = vec!["job-a".into(), "job-b".into()];
        let recovered = CoordinatorShardSnapshot {
            shard,
            receipt: None,
            terminal_summary: None,
        }
        .recover_membership()
        .expect("recover membership");

        assert_eq!(
            recovered.missing_terminal_member_job_ids,
            vec!["job-a", "job-b"]
        );
        assert!(recovered.completed_member_job_ids.is_empty());
        assert!(recovered.failed_member_job_ids.is_empty());
    }

    #[test]
    fn terminal_outcome_accepts_salvageable_members_as_partial_success() {
        let mut shard = shard("shard-partial");
        shard.member_job_ids = vec!["job-a".into(), "job-b".into(), "job-c".into()];
        let receipt = receipt("shard-partial", SchedulerReceiptState::Completed);
        shard.attach_receipt_id(receipt.receipt_id.clone());
        let mut terminal = summary(
            "shard-partial",
            &receipt.receipt_id,
            AllocationTaskPhase::Completed,
        );
        terminal.completed_job_ids = vec!["job-a".into()];
        terminal.failed_job_ids = vec!["job-c".into()];
        terminal.salvageable_job_ids = vec!["job-b".into()];

        let outcome = CoordinatorShardSnapshot {
            shard,
            receipt: Some(receipt),
            terminal_summary: Some(terminal),
        }
        .terminal_outcome()
        .expect("terminal outcome");

        assert_eq!(
            outcome.disposition,
            ShardTerminalDisposition::PartialSuccess
        );
        assert_eq!(outcome.accepted_member_job_ids, vec!["job-a", "job-b"]);
        assert_eq!(outcome.salvageable_member_job_ids, vec!["job-b"]);
        assert_eq!(outcome.retry_member_job_ids, vec!["job-c"]);
    }

    #[test]
    fn terminal_outcome_can_disable_failed_member_retries() {
        let mut shard = shard("shard-no-retry");
        shard.retry_policy.retry_failed_jobs = false;
        shard.member_job_ids = vec!["job-a".into(), "job-b".into()];
        let receipt = receipt("shard-no-retry", SchedulerReceiptState::Completed);
        shard.attach_receipt_id(receipt.receipt_id.clone());
        let mut terminal = summary(
            "shard-no-retry",
            &receipt.receipt_id,
            AllocationTaskPhase::Failed,
        );
        terminal.completed_job_ids = Vec::new();
        terminal.failed_job_ids = vec!["job-a".into(), "job-b".into()];

        let outcome = CoordinatorShardSnapshot {
            shard,
            receipt: Some(receipt),
            terminal_summary: Some(terminal),
        }
        .terminal_outcome()
        .expect("terminal outcome");

        assert_eq!(
            outcome.disposition,
            ShardTerminalDisposition::CompleteFailure
        );
        assert!(outcome.accepted_member_job_ids.is_empty());
        assert!(outcome.retry_member_job_ids.is_empty());
        assert_eq!(outcome.failed_member_job_ids, vec!["job-a", "job-b"]);
    }

    #[test]
    fn terminal_outcome_marks_missing_members_as_incomplete_evidence() {
        let mut shard = shard("shard-incomplete");
        shard.member_job_ids = vec!["job-a".into(), "job-b".into()];
        let receipt = receipt("shard-incomplete", SchedulerReceiptState::Completed);
        shard.attach_receipt_id(receipt.receipt_id.clone());
        let mut terminal = summary(
            "shard-incomplete",
            &receipt.receipt_id,
            AllocationTaskPhase::Completed,
        );
        terminal.completed_job_ids = vec!["job-a".into()];
        terminal.failed_job_ids = Vec::new();

        let outcome = CoordinatorShardSnapshot {
            shard,
            receipt: Some(receipt),
            terminal_summary: Some(terminal),
        }
        .terminal_outcome()
        .expect("terminal outcome");

        assert_eq!(
            outcome.disposition,
            ShardTerminalDisposition::IncompleteEvidence
        );
        assert_eq!(outcome.accepted_member_job_ids, vec!["job-a"]);
        assert_eq!(outcome.retry_member_job_ids, vec!["job-b"]);
        assert_eq!(outcome.missing_terminal_member_job_ids, vec!["job-b"]);
    }
}
