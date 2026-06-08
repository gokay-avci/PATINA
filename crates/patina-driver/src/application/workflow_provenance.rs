use anyhow::{anyhow, ensure, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TemplateBundleProvenance<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_job: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_gin: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atoms_in: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<&'a Path>,
}

#[derive(Debug, Clone)]
pub struct WorkflowProvenanceSpec<'a> {
    pub workflow_id: &'a str,
    pub workflow_owner: &'a str,
    pub backend_id: Option<&'a str>,
    pub backend_mode: Option<&'a str>,
    pub lane_mode: Option<&'a str>,
    pub duplicate_policy_mode: Option<&'a str>,
    pub parallel_contract: Option<&'a str>,
    pub run_dir: Option<&'a Path>,
    pub workdir: Option<&'a Path>,
    pub source_run_dir: Option<&'a Path>,
    pub source_path: Option<&'a Path>,
    pub candidate_json: Option<&'a Path>,
    pub template_bundle: Option<TemplateBundleProvenance<'a>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowProvenanceRecord<'a> {
    pub workflow_id: &'a str,
    pub workflow_route: &'a str,
    pub workflow_family: &'a str,
    pub workflow_context: &'static str,
    pub workflow_owner: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane_mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_policy_mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_contract: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_dir: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workdir: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_run_dir: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_json: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_bundle: Option<TemplateBundleProvenance<'a>>,
}

pub fn build_workflow_provenance(
    spec: WorkflowProvenanceSpec<'_>,
) -> Result<WorkflowProvenanceRecord<'_>> {
    let definition = patina_types::workflow_by_id(spec.workflow_id)
        .ok_or_else(|| anyhow!("unknown workflow provenance id `{}`", spec.workflow_id))?;
    if let Some(expected_owner) = definition.expected_owner {
        ensure!(
            expected_owner == spec.workflow_owner,
            "workflow provenance owner mismatch for `{}`: expected `{expected_owner}`, found `{}`",
            spec.workflow_id,
            spec.workflow_owner
        );
    }
    if let (Some(expected_backend), Some(actual_backend)) =
        (definition.expected_backend, spec.backend_id)
    {
        ensure!(
            expected_backend == actual_backend,
            "workflow provenance backend mismatch for `{}`: expected `{expected_backend}`, found `{actual_backend}`",
            spec.workflow_id
        );
    }
    Ok(WorkflowProvenanceRecord {
        workflow_id: spec.workflow_id,
        workflow_route: definition.route,
        workflow_family: definition.family,
        workflow_context: definition.context.label(),
        workflow_owner: spec.workflow_owner,
        backend_id: spec.backend_id,
        backend_mode: spec.backend_mode,
        lane_mode: spec.lane_mode,
        duplicate_policy_mode: spec.duplicate_policy_mode,
        parallel_contract: spec.parallel_contract,
        run_dir: spec.run_dir,
        workdir: spec.workdir,
        source_run_dir: spec.source_run_dir,
        source_path: spec.source_path,
        candidate_json: spec.candidate_json,
        template_bundle: spec.template_bundle,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_workflow_provenance, WorkflowProvenanceSpec};
    use std::path::Path;

    #[test]
    fn builds_registry_backed_provenance_for_supported_workflow() {
        let record = build_workflow_provenance(WorkflowProvenanceSpec {
            workflow_id: "ga.scott-monolithic",
            workflow_owner: "scott_monolithic_ga",
            backend_id: Some("scott_runtime"),
            backend_mode: Some("gulp"),
            lane_mode: Some("standalone_capable"),
            duplicate_policy_mode: Some("external-native-hashkey"),
            parallel_contract: Some("monolithic_scott_runtime_generation_dispatch"),
            run_dir: Some(Path::new("runs/active/example")),
            workdir: Some(Path::new("scratch/example")),
            source_run_dir: None,
            source_path: None,
            candidate_json: Some(Path::new("inputs/base_candidate.json")),
            template_bundle: None,
        })
        .expect("provenance");

        assert_eq!(record.workflow_route, "run-ga scott-monolithic");
        assert_eq!(record.workflow_context, "new-run");
        assert_eq!(record.backend_id, Some("scott_runtime"));
    }
}
