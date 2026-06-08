use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use patina_runtime::ScottRuntimeJob;

/// Durable and scratch roots owned by the orchestration layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRoots {
    pub durable_root: Utf8PathBuf,
    pub scratch_root: Option<Utf8PathBuf>,
}

impl WorkspaceRoots {
    pub fn active_root(&self) -> &Utf8PathBuf {
        self.scratch_root.as_ref().unwrap_or(&self.durable_root)
    }
}

/// Policy controlling how one Scott runtime job maps onto durable and scratch paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePolicy {
    pub job_prefix: String,
    pub collect_final_artifacts: bool,
    pub keep_scratch_on_failure: bool,
}

impl Default for WorkspacePolicy {
    fn default() -> Self {
        Self {
            job_prefix: "job".into(),
            collect_final_artifacts: true,
            keep_scratch_on_failure: true,
        }
    }
}

/// Planned workspace layout for one Scott runtime job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePlan {
    pub job_id: String,
    pub durable_job_root: Utf8PathBuf,
    pub active_job_root: Utf8PathBuf,
    pub stage_root: Utf8PathBuf,
    pub manifest_path: Utf8PathBuf,
}

impl WorkspacePolicy {
    pub fn plan_for_job(&self, roots: &WorkspaceRoots, job: &ScottRuntimeJob) -> WorkspacePlan {
        let leaf = format!("{}_{}", self.job_prefix, job.identity.job_id);
        let durable_job_root = roots.durable_root.join(&leaf);
        let active_job_root = roots.active_root().join(&leaf);

        WorkspacePlan {
            job_id: job.identity.job_id.clone(),
            durable_job_root: durable_job_root.clone(),
            active_job_root: active_job_root.clone(),
            stage_root: active_job_root.join("stage"),
            manifest_path: durable_job_root.join("manifest.json"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkspacePolicy, WorkspaceRoots};
    use indexmap::IndexMap;
    use patina_runtime::{
        ScottControllerKind, ScottJobIdentity, ScottJobKind, ScottJobRole, ScottRuntimeJob,
    };

    fn controller_job() -> ScottRuntimeJob {
        ScottRuntimeJob {
            identity: ScottJobIdentity {
                job_id: "job-42".into(),
                parent_job_id: None,
                candidate_label: None,
            },
            role: ScottJobRole::Controller,
            kind: ScottJobKind::Controller {
                kind: ScottControllerKind::ProductionRun,
                resume_from: None,
            },
            requested_cores: 1,
            requested_gpus: 0,
            required_tags: Vec::new(),
            labels: IndexMap::new(),
        }
    }

    #[test]
    fn workspace_policy_uses_scratch_when_available() {
        let roots = WorkspaceRoots {
            durable_root: "/durable".into(),
            scratch_root: Some("/scratch".into()),
        };
        let plan = WorkspacePolicy::default().plan_for_job(&roots, &controller_job());
        assert_eq!(plan.durable_job_root, "/durable/job_job-42");
        assert_eq!(plan.active_job_root, "/scratch/job_job-42");
        assert_eq!(plan.stage_root, "/scratch/job_job-42/stage");
    }

    #[test]
    fn workspace_policy_falls_back_to_durable_root() {
        let roots = WorkspaceRoots {
            durable_root: "/durable".into(),
            scratch_root: None,
        };
        let plan = WorkspacePolicy::default().plan_for_job(&roots, &controller_job());
        assert_eq!(plan.active_job_root, "/durable/job_job-42");
        assert_eq!(plan.manifest_path, "/durable/job_job-42/manifest.json");
    }
}
