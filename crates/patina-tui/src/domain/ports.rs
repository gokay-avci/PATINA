use crate::domain::artifacts::{RunSnapshot, WorkflowKind};

#[derive(Debug, Clone, Copy)]
pub struct PortBinding {
    pub name: &'static str,
    pub use_case: &'static str,
    pub port_contract: &'static str,
    pub adapter_surface: &'static str,
    pub upstream: &'static str,
    pub downstream: &'static str,
    pub directionality: &'static str,
    pub workflow_routes: &'static [&'static str],
    pub artifacts: &'static [&'static str],
    pub expected_owner: Option<&'static str>,
    pub expected_backend: Option<&'static str>,
    pub expected_workflow_kind: Option<WorkflowKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortsDetailMode {
    Contract,
    Evidence,
    Actions,
}

impl PortsDetailMode {
    pub fn next(self) -> Self {
        match self {
            Self::Contract => Self::Evidence,
            Self::Evidence => Self::Actions,
            Self::Actions => Self::Contract,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Contract => "Flow Detail",
            Self::Evidence => "Run Evidence",
            Self::Actions => "Actions",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortProbeCheck {
    pub ok: bool,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct PortProbeReport {
    pub binding_name: &'static str,
    pub checks: Vec<PortProbeCheck>,
}

impl PortProbeReport {
    pub fn totals(&self) -> (usize, usize) {
        let ok = self.checks.iter().filter(|check| check.ok).count();
        let fail = self.checks.len().saturating_sub(ok);
        (ok, fail)
    }
}

pub fn registry() -> &'static [PortBinding] {
    &[
        PortBinding {
            name: "GA Evaluation Seam",
            use_case: "Execute Rust-owned GA generations through a stable evaluation boundary.",
            port_contract: "GaWorkflowService -> GaEvaluationPort",
            adapter_surface: "StagedScottGaEvaluator or PersistentPoolGaEvaluator",
            upstream: "Base candidate or checkpoint, controller policy, generation request.",
            downstream: "generation_state.json, generation_metrics.csv, controller_trace.csv, checkpoint.",
            directionality: "Controller emits candidates -> evaluation adapter executes backend -> workflow accepts/rejects -> artifacts persisted.",
            workflow_routes: &["run-ga scott-staged", "run-ga janus-persistent"],
            artifacts: &[
                "raw/generation_XXXX_state.json",
                "traces/generation_metrics.csv",
                "traces/controller_trace.csv",
                "raw/rust_ga_checkpoint_latest.json",
            ],
            expected_owner: None,
            expected_backend: None,
            expected_workflow_kind: Some(WorkflowKind::Ga),
        },
        PortBinding {
            name: "Backend Evaluation Seam",
            use_case: "Evaluate one candidate or sampled move with explicit backend selection.",
            port_contract: "BackendEvaluator-style evaluation request",
            adapter_surface: "GULP, SCOTT, or Janus/MACE-backed adapters",
            upstream: "Candidate structure, backend mode, timeout/runtime policy.",
            downstream: "Evaluation record, convergence flags, backend run directory metadata.",
            directionality: "Workflow request -> backend adapter invocation -> evaluation response -> result merged into traces.",
            workflow_routes: &[
                "evaluate-backend",
                "run-basin-hopping",
                "run-energy-lid",
                "run-simulated-annealing",
            ],
            artifacts: &["raw/generation_*.json or walker trace rows", "backend_run_dir / primary output path"],
            expected_owner: None,
            expected_backend: None,
            expected_workflow_kind: None,
        },
        PortBinding {
            name: "Sampling Workflow Seam",
            use_case: "Run Monte Carlo-like workflows over typed sampling kernels.",
            port_contract: "Sampling workflow request -> backend evaluation port",
            adapter_surface: "Basin hopping, energy-lid, simulated annealing orchestration",
            upstream: "Seed structure or source GA run, thermodynamic schedule, backend policy.",
            downstream: "walker_trace.csv or sampling summaries plus tracked manifests.",
            directionality: "Sampling controller proposes move -> backend evaluates -> accept/reject -> trace persisted -> next step.",
            workflow_routes: &[
                "run-basin-hopping",
                "run-energy-lid",
                "run-simulated-annealing",
            ],
            artifacts: &[
                "traces/walker_trace.csv",
                "sampling summary rows",
                "tracked run manifest",
            ],
            expected_owner: None,
            expected_backend: None,
            expected_workflow_kind: Some(WorkflowKind::Bh),
        },
        PortBinding {
            name: "Artifact Repository Seam",
            use_case: "Load tracked run artifacts without embedding runner logic in the UI.",
            port_contract: "RunDataSource read contract",
            adapter_surface: "FilesystemRunDataSource",
            upstream: "run_dir + manifest artifact map + raw/traces/output files.",
            downstream: "UI read model for dashboard, inspector, launch, and ports views.",
            directionality: "Filesystem artifacts -> tolerant parsers -> run snapshot -> read-side panels.",
            workflow_routes: &["patina-tui <RUN_DIR>", "patina-tui --follow <RUN_DIR>"],
            artifacts: &["manifest.json", "raw/*", "traces/*", "outputs/*"],
            expected_owner: None,
            expected_backend: None,
            expected_workflow_kind: None,
        },
        PortBinding {
            name: "Topology & Native Identity Seam",
            use_case: "Compute lightweight structure diagnostics and native-compatible graph identity outputs.",
            port_contract: "Selected structure -> topology/hashkey helper interface",
            adapter_surface: "patina-search topology helpers + hkg_dreadnaut_wrapper.py",
            upstream: "Selected generation member structure, atoms.in species table, hashkey settings.",
            downstream: "Topology summary text, dreadnaut payload, canonical hashkey output.",
            directionality: "Select structure -> derive graph/metrics -> optional wrapper execution -> export to outputs/workbench.",
            workflow_routes: &["workbench: Topology Metrics", "workbench: Dreadnaut Graph", "workbench: Canonical Hashkey"],
            artifacts: &[
                "outputs/workbench/*.txt",
                "outputs/workbench/*.dreadnaut",
                "canonical hashkey probe output",
            ],
            expected_owner: None,
            expected_backend: None,
            expected_workflow_kind: None,
        },
    ]
}

pub fn matches_current_run(binding: &PortBinding, snapshot: &RunSnapshot) -> bool {
    let owner_matches = binding
        .expected_owner
        .is_some_and(|owner| snapshot.manifest.workflow_owner.as_deref() == Some(owner));
    let backend_matches = binding
        .expected_backend
        .is_some_and(|backend| snapshot.manifest.backend.as_deref() == Some(backend));
    let kind_matches = binding
        .expected_workflow_kind
        .is_some_and(|kind| snapshot.workflow_kind == kind);

    owner_matches || backend_matches || kind_matches
}

pub fn current_binding_note(binding: &PortBinding, snapshot: &RunSnapshot) -> String {
    let owner = snapshot.manifest.workflow_owner.as_deref().unwrap_or("n/a");
    let backend = snapshot.manifest.backend.as_deref().unwrap_or("n/a");
    if matches_current_run(binding, snapshot) {
        format!("Current run likely uses this seam (workflow_owner={owner}, backend={backend}).")
    } else {
        format!(
            "Current run does not directly indicate this seam (workflow_owner={owner}, backend={backend})."
        )
    }
}

pub fn evidence_lines(binding: PortBinding, snapshot: &RunSnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "workflow_owner={} backend={} workflow_kind={:?}",
        snapshot.manifest.workflow_owner.as_deref().unwrap_or("n/a"),
        snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
        snapshot.workflow_kind
    ));
    lines.push(format!(
        "metrics_rows={} controller_rows={} walker_rows={} state_files={}",
        snapshot.generation_metrics.len(),
        snapshot.controller_trace.len(),
        snapshot.walker_trace.len(),
        snapshot.generations.states.len()
    ));
    lines.push(format!(
        "manifest_artifacts={} raw_files={} output_files={}",
        snapshot.manifest.artifacts.len(),
        snapshot.artifacts.raw_files.len(),
        snapshot.artifacts.output_files.len()
    ));
    lines.push(format!(
        "current seam match={}",
        if matches_current_run(&binding, snapshot) {
            "yes"
        } else {
            "no"
        }
    ));

    match binding.name {
        "GA Evaluation Seam" => {
            lines.push(format!(
                "ga evidence: generation_metrics={} generation_states={}",
                snapshot.generation_metrics.len(),
                snapshot.generations.states.len()
            ));
        }
        "Sampling Workflow Seam" => {
            lines.push(format!(
                "sampling evidence: walker_trace_rows={}",
                snapshot.walker_trace.len()
            ));
        }
        "Topology & Native Identity Seam" => {
            let workbench_dir = snapshot.run_dir.join("outputs").join("workbench");
            lines.push(format!(
                "workbench export dir exists={}",
                if workbench_dir.exists() { "yes" } else { "no" }
            ));
        }
        _ => {}
    }
    lines
}

pub fn recommended_workspace(binding: PortBinding) -> &'static str {
    match binding.name {
        "GA Evaluation Seam" => "launch",
        "Backend Evaluation Seam" => "flow",
        "Sampling Workflow Seam" => "flow",
        "Artifact Repository Seam" => "artifacts",
        "Topology & Native Identity Seam" => "workbench",
        _ => "architecture",
    }
}

pub fn action_lines(binding: PortBinding, snapshot: &RunSnapshot) -> Vec<String> {
    let mut lines = vec![
        "Enter: open linked workspace for this seam".to_string(),
        "g: open launch workspace with the closest matching route preselected".to_string(),
        "a: toggle all seams vs seams relevant to current run".to_string(),
        "v: cycle right-pane mode (contract/evidence/actions)".to_string(),
        "x: run seam probe checks against current run artifacts".to_string(),
    ];
    lines.push(format!(
        "Recommended workspace: {}",
        recommended_workspace(binding)
    ));
    lines.push(format!("Current run dir: {}", snapshot.run_dir.display()));
    lines.push(format!(
        "Known workflow routes: {}",
        binding.workflow_routes.join(", ")
    ));
    lines
}

pub fn probe_binding(binding: PortBinding, snapshot: &RunSnapshot) -> PortProbeReport {
    let mut checks = Vec::new();

    checks.push(PortProbeCheck {
        ok: snapshot.artifacts.manifest_path.exists(),
        label: "manifest exists".to_string(),
        detail: snapshot.artifacts.manifest_path.display().to_string(),
    });
    checks.push(PortProbeCheck {
        ok: !snapshot.manifest.artifacts.is_empty(),
        label: "manifest artifact contract available".to_string(),
        detail: format!("artifact keys={}", snapshot.manifest.artifacts.len()),
    });
    checks.push(PortProbeCheck {
        ok: !snapshot.artifacts.raw_files.is_empty() || !snapshot.artifacts.output_files.is_empty(),
        label: "filesystem artifacts discovered".to_string(),
        detail: format!(
            "raw={} outputs={}",
            snapshot.artifacts.raw_files.len(),
            snapshot.artifacts.output_files.len()
        ),
    });

    match binding.name {
        "GA Evaluation Seam" => {
            checks.push(PortProbeCheck {
                ok: !snapshot.generation_metrics.is_empty(),
                label: "GA metrics rows present".to_string(),
                detail: format!("rows={}", snapshot.generation_metrics.len()),
            });
            checks.push(PortProbeCheck {
                ok: !snapshot.generations.states.is_empty(),
                label: "GA generation state files present".to_string(),
                detail: format!("states={}", snapshot.generations.states.len()),
            });
        }
        "Backend Evaluation Seam" => {
            let has_eval_rows =
                !snapshot.generation_metrics.is_empty() || !snapshot.walker_trace.is_empty();
            checks.push(PortProbeCheck {
                ok: has_eval_rows,
                label: "evaluation traces present".to_string(),
                detail: format!(
                    "metrics={} walker={}",
                    snapshot.generation_metrics.len(),
                    snapshot.walker_trace.len()
                ),
            });
        }
        "Sampling Workflow Seam" => {
            checks.push(PortProbeCheck {
                ok: !snapshot.walker_trace.is_empty(),
                label: "sampling walker trace present".to_string(),
                detail: format!("walker rows={}", snapshot.walker_trace.len()),
            });
        }
        "Artifact Repository Seam" => {
            checks.push(PortProbeCheck {
                ok: snapshot.artifacts.manifest_path.exists(),
                label: "artifact repository root reachable".to_string(),
                detail: snapshot.run_dir.display().to_string(),
            });
        }
        "Topology & Native Identity Seam" => {
            let workbench_dir = snapshot.run_dir.join("outputs").join("workbench");
            checks.push(PortProbeCheck {
                ok: workbench_dir.exists(),
                label: "workbench output dir exists".to_string(),
                detail: workbench_dir.display().to_string(),
            });
            checks.push(PortProbeCheck {
                ok: !snapshot.generations.states.is_empty(),
                label: "structures available for topology probes".to_string(),
                detail: format!("generation states={}", snapshot.generations.states.len()),
            });
        }
        _ => {}
    }

    PortProbeReport {
        binding_name: binding.name,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::ga::{ArtifactIndex, GenerationStore};
    use crate::domain::run_manifest::RunManifest;

    use super::*;

    fn minimal_snapshot(kind: WorkflowKind) -> RunSnapshot {
        RunSnapshot {
            run_dir: PathBuf::from("runs/active/example"),
            loaded_from_run_dir: true,
            manifest: RunManifest {
                workflow_owner: Some("scott_staged_ga".to_string()),
                backend: Some("scott_runtime".to_string()),
                ..RunManifest::default()
            },
            workflow_kind: kind,
            generation_metrics: Vec::new(),
            controller_trace: Vec::new(),
            walker_trace: Vec::new(),
            generations: GenerationStore::default(),
            artifacts: ArtifactIndex {
                manifest_path: PathBuf::from("runs/active/example/manifest.json"),
                raw_files: Vec::new(),
                output_files: Vec::new(),
            },
        }
    }

    #[test]
    fn ga_seam_matches_ga_runs_by_workflow_kind() {
        let snapshot = minimal_snapshot(WorkflowKind::Ga);
        let seam = registry()
            .iter()
            .find(|entry| entry.name == "GA Evaluation Seam")
            .expect("ga seam present");
        assert!(matches_current_run(seam, &snapshot));
    }

    #[test]
    fn sampling_seam_does_not_match_ga_run() {
        let snapshot = minimal_snapshot(WorkflowKind::Ga);
        let seam = registry()
            .iter()
            .find(|entry| entry.name == "Sampling Workflow Seam")
            .expect("sampling seam present");
        assert!(!matches_current_run(seam, &snapshot));
    }

    #[test]
    fn probe_has_checks_for_ga_seam() {
        let snapshot = minimal_snapshot(WorkflowKind::Ga);
        let seam = registry()
            .iter()
            .find(|entry| entry.name == "GA Evaluation Seam")
            .copied()
            .expect("ga seam present");
        let report = probe_binding(seam, &snapshot);
        assert!(!report.checks.is_empty());
    }
}
