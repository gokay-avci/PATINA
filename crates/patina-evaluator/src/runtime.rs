use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::plan::ScottBackendMode;
use crate::stages::StageIndex;

/// Stable identity for one submitted Scott stage execution.
///
/// This is the seam between Scott procedure semantics and a future scheduler:
/// the scientific kernel decides *what* should run next, while a runner or
/// scheduler decides *when* and *where* to execute the ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageTicket {
    pub request_id: String,
    pub backend_mode: ScottBackendMode,
    pub stage: StageIndex,
    pub attempt: usize,
    pub workdir: Utf8PathBuf,
}

/// Runtime status of a single candidate as it moves through the Scott procedure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureRuntimeState {
    ReadyToSubmit,
    Submitted,
    Running,
    Retrieving,
    StageAccepted,
    Completed,
    Rejected,
    Failed,
}

/// What the pure Scott procedure kernel wants the execution layer to do next.
///
/// This intentionally does not prescribe sync vs async execution. A simple local
/// runner can handle these immediately, while a durable scheduler can persist and
/// route the same actions across workers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureAction {
    Submit(StageTicket),
    Poll(StageTicket),
    Retrieve(StageTicket),
    Retry(StageTicket),
    AdvanceTo(StageIndex),
    FinishAccepted,
    FinishRejected,
    FinishFailed,
}

/// Minimal scheduler-facing cursor for a candidate undergoing staged evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureCursor {
    pub ticket: StageTicket,
    pub state: ProcedureRuntimeState,
}

impl ProcedureCursor {
    pub fn new(ticket: StageTicket) -> Self {
        Self {
            ticket,
            state: ProcedureRuntimeState::ReadyToSubmit,
        }
    }

    pub fn submit_action(&self) -> ProcedureAction {
        ProcedureAction::Submit(self.ticket.clone())
    }

    pub fn poll_action(&self) -> ProcedureAction {
        ProcedureAction::Poll(self.ticket.clone())
    }

    pub fn retrieve_action(&self) -> ProcedureAction {
        ProcedureAction::Retrieve(self.ticket.clone())
    }

    pub fn retry_action(&self) -> ProcedureAction {
        let mut ticket = self.ticket.clone();
        ticket.attempt += 1;
        ProcedureAction::Retry(ticket)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcedureAction, ProcedureCursor, ProcedureRuntimeState, StageTicket};
    use crate::{ScottBackendMode, StageIndex};

    fn make_ticket() -> StageTicket {
        StageTicket {
            request_id: "req-1".into(),
            backend_mode: ScottBackendMode::Gulp,
            stage: StageIndex(1),
            attempt: 1,
            workdir: "/tmp/test-stage".into(),
        }
    }

    #[test]
    fn procedure_cursor_starts_ready_to_submit() {
        let cursor = ProcedureCursor::new(make_ticket());
        assert_eq!(cursor.state, ProcedureRuntimeState::ReadyToSubmit);
    }

    #[test]
    fn retry_action_increments_attempt_without_mutating_cursor() {
        let cursor = ProcedureCursor::new(make_ticket());
        let action = cursor.retry_action();

        match action {
            ProcedureAction::Retry(ticket) => {
                assert_eq!(ticket.attempt, 2);
            }
            other => panic!("expected retry action, got {other:?}"),
        }

        assert_eq!(cursor.ticket.attempt, 1);
    }
}
