use serde::Serialize;

use super::policy::{ForkReadiness, ScientificScope, SearchFamily, SearchForkPolicy, WorkflowLane};

/// Explicit policy object for one workflow lane and workflow family.
///
/// This keeps path-specific rules visible and prevents similarly named codepaths
/// from silently sharing semantics when they should remain distinct.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct WorkflowExecutionPolicy {
    pub lane: WorkflowLane,
    pub family: SearchFamily,
    pub readiness: ForkReadiness,
    pub lane_scope: ScientificScope,
    pub uses_scott_runtime: bool,
    pub emits_janus_guardrails: bool,
}

impl WorkflowExecutionPolicy {
    fn from_family_lane(
        family: SearchFamily,
        lane: WorkflowLane,
        uses_scott_runtime: bool,
        emits_janus_guardrails: bool,
    ) -> Self {
        let base = SearchForkPolicy::for_family(family);
        let lane_scope = match lane {
            WorkflowLane::NativeScott => base.native_lane_scope,
            WorkflowLane::RustScottParity => base.rust_lane_scope,
            WorkflowLane::RustOwnedJanus => base.rust_lane_scope,
        };
        Self {
            lane,
            family,
            readiness: base.readiness,
            lane_scope,
            uses_scott_runtime,
            emits_janus_guardrails,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct GaWorkflowPolicy {
    pub lane: WorkflowLane,
    pub family: SearchFamily,
    pub readiness: ForkReadiness,
    pub lane_scope: ScientificScope,
    pub uses_scott_runtime: bool,
    pub emits_janus_guardrails: bool,
}

impl From<WorkflowExecutionPolicy> for GaWorkflowPolicy {
    fn from(policy: WorkflowExecutionPolicy) -> Self {
        Self {
            lane: policy.lane,
            family: policy.family,
            readiness: policy.readiness,
            lane_scope: policy.lane_scope,
            uses_scott_runtime: policy.uses_scott_runtime,
            emits_janus_guardrails: policy.emits_janus_guardrails,
        }
    }
}

impl GaWorkflowPolicy {
    pub fn rust_janus() -> Self {
        WorkflowExecutionPolicy::from_family_lane(
            SearchFamily::GeneticAlgorithm,
            WorkflowLane::RustOwnedJanus,
            false,
            true,
        )
        .into()
    }

    pub fn scott_staged_runtime() -> Self {
        WorkflowExecutionPolicy::from_family_lane(
            SearchFamily::GeneticAlgorithm,
            WorkflowLane::RustScottParity,
            true,
            false,
        )
        .into()
    }
}
