use crate::domain::artifacts::{RunSnapshot, WorkflowKind};
use patina_types::{workflow_registry, WorkflowDefinition, WorkflowFileContract, WorkflowFileKind};

pub type LaunchWorkflow = WorkflowDefinition;

pub struct LaunchPanel {
    pub title: &'static str,
    pub lines: Vec<String>,
}

pub fn registry() -> &'static [LaunchWorkflow] {
    workflow_registry()
}

pub fn index_for_route(route: &str) -> Option<usize> {
    registry()
        .iter()
        .position(|workflow| workflow.route == route)
}

pub fn input_contract_panel(workflow: &LaunchWorkflow) -> LaunchPanel {
    let mut lines = Vec::new();
    lines.push(format!(
        "context: {} ({})",
        workflow.context.label(),
        workflow.context.description()
    ));
    lines.push(String::new());
    lines.push("required inputs".to_string());
    lines.extend(
        workflow
            .required_inputs
            .iter()
            .map(|item| format!("  {item}")),
    );
    lines.push(String::new());
    lines.push("representative args".to_string());
    lines.extend(workflow.example_args.iter().map(|item| format!("  {item}")));
    lines.push(String::new());
    lines.push("key switches".to_string());
    lines.extend(workflow.key_switches.iter().map(|item| format!("  {item}")));
    LaunchPanel {
        title: "Input Contract",
        lines,
    }
}

pub fn file_contract_panel(workflow: &LaunchWorkflow) -> LaunchPanel {
    let mut lines = Vec::new();
    for kind in [
        WorkflowFileKind::ScientificInput,
        WorkflowFileKind::RuntimeSupport,
        WorkflowFileKind::AdapterTemplate,
        WorkflowFileKind::GeneratedArtifact,
    ] {
        let contracts = workflow
            .file_contracts
            .iter()
            .filter(|entry| entry.kind == kind)
            .copied()
            .collect::<Vec<_>>();
        if contracts.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("{}s", kind.label()));
        for entry in contracts {
            lines.extend(render_file_contract(entry));
        }
    }
    LaunchPanel {
        title: "File Contract",
        lines,
    }
}

pub fn binding_panel(workflow: &LaunchWorkflow, snapshot: &RunSnapshot) -> LaunchPanel {
    let mut lines = vec![
        format!("route: {}", workflow.route),
        format!("family: {}", workflow.family),
        format!(
            "expected_owner={} expected_backend={}",
            workflow.expected_owner.unwrap_or("n/a"),
            workflow.expected_backend.unwrap_or("n/a")
        ),
        String::new(),
        current_binding_note(workflow, snapshot).to_string(),
        format!(
            "loaded owner={} backend={} workflow_kind={}",
            snapshot.manifest.workflow_owner.as_deref().unwrap_or("n/a"),
            snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
            workflow_kind_label(snapshot.workflow_kind),
        ),
    ];
    if snapshot.has_loaded_run() {
        lines.push(format!("current run dir={}", snapshot.run_dir.display()));
    }
    LaunchPanel {
        title: "Run Binding",
        lines,
    }
}

fn render_file_contract(entry: WorkflowFileContract) -> Vec<String> {
    let required = if entry.required {
        "required"
    } else {
        "optional"
    };
    vec![
        format!("  {}: {} [{}]", entry.label, entry.path_pattern, required),
        format!("    {}", entry.detail),
    ]
}

fn workflow_kind_label(kind: WorkflowKind) -> &'static str {
    match kind {
        WorkflowKind::Ga => "ga",
        WorkflowKind::Bh => "bh",
        WorkflowKind::Unknown => "unknown",
    }
}

pub fn current_binding_note(workflow: &LaunchWorkflow, snapshot: &RunSnapshot) -> &'static str {
    if !snapshot.has_loaded_run() {
        return "No current run loaded; showing generic launch guidance.";
    }
    if matches_current_run(workflow, snapshot) {
        "Current run matches this workflow family."
    } else {
        "Current run does not match this workflow family."
    }
}

pub fn matches_current_run(workflow: &LaunchWorkflow, snapshot: &RunSnapshot) -> bool {
    if !snapshot.has_loaded_run() {
        return false;
    }
    let owner_matches = workflow
        .expected_owner
        .is_some_and(|owner| snapshot.manifest.workflow_owner.as_deref() == Some(owner));
    let backend_matches = workflow
        .expected_backend
        .is_some_and(|backend| snapshot.manifest.backend.as_deref() == Some(backend));
    owner_matches || backend_matches
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::artifacts::{RunSnapshot, WorkflowKind};
    use crate::domain::ga::{ArtifactIndex, GenerationStore};
    use crate::domain::launch_draft::LaunchDraft;
    use crate::domain::run_manifest::RunManifest;

    use super::{index_for_route, matches_current_run, registry};

    fn minimal_snapshot() -> RunSnapshot {
        RunSnapshot {
            run_dir: PathBuf::from("runs/active/example"),
            loaded_from_run_dir: true,
            manifest: RunManifest {
                system: Some("mgo24".to_string()),
                workflow_owner: Some("scott_staged_ga".to_string()),
                backend: Some("scott_runtime".to_string()),
                requested_generations: Some(20),
                population_size: Some(24),
                ..RunManifest::default()
            },
            workflow_kind: WorkflowKind::Ga,
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
    fn preview_uses_current_run_for_follow_on_workflows() {
        let snapshot = minimal_snapshot();
        let workflow = registry()
            .iter()
            .find(|workflow| workflow.route == "run-energy-lid")
            .expect("energy lid workflow");
        let draft = LaunchDraft::for_workflow(workflow, &snapshot);
        let preview = draft.render_command_preview(workflow);
        assert!(preview.contains("--source-run-dir runs/active/example"));
        assert!(preview.contains("--top-n 8"));
    }

    #[test]
    fn matching_detects_current_staged_ga_run() {
        let snapshot = minimal_snapshot();
        let workflow = registry()
            .iter()
            .find(|workflow| workflow.route == "run-ga scott-monolithic")
            .expect("staged ga workflow");
        assert!(matches_current_run(workflow, &snapshot));
    }

    #[test]
    fn route_lookup_finds_registered_workflow() {
        assert!(index_for_route("run-basin-hopping").is_some());
        assert!(index_for_route("non-existent-route").is_none());
    }
}
