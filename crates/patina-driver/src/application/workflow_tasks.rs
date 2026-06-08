use anyhow::{anyhow, Result};
use patina_search::{MonteCarloKernelState, WorkflowEvaluationTask, WorkflowLineage};
use patina_types::Candidate;

pub fn queue_single_sampling_task(
    state: &mut MonteCarloKernelState,
    candidate: Candidate,
    lineage: WorkflowLineage,
    context: &str,
) -> Result<WorkflowEvaluationTask> {
    state
        .queue_candidate(candidate, lineage)
        .map_err(|error| anyhow!("{context}: failed to queue workflow task: {error:?}"))?;
    take_queued_task(state.take_task(), context)
}

pub fn take_queued_task(
    task: Option<WorkflowEvaluationTask>,
    context: &str,
) -> Result<WorkflowEvaluationTask> {
    task.ok_or_else(|| anyhow!("{context}: queued workflow task was not available"))
}
