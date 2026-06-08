use crate::app::state::App;
use crate::domain::artifacts::WorkflowKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantTool {
    Help,
    Run,
    Selection,
    Artifacts,
    Compare,
    Failures,
    Suggest,
}

impl AssistantTool {
    pub fn shortcut(self) -> char {
        match self {
            Self::Help => 'H',
            Self::Run => 'R',
            Self::Selection => 'S',
            Self::Artifacts => 'A',
            Self::Compare => 'C',
            Self::Failures => 'F',
            Self::Suggest => 'N',
        }
    }

    pub fn slash_command(self) -> &'static str {
        match self {
            Self::Help => "/help",
            Self::Run => "/run",
            Self::Selection => "/selection",
            Self::Artifacts => "/artifacts",
            Self::Compare => "/compare",
            Self::Failures => "/failures",
            Self::Suggest => "/suggest",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Help => "Tool Help",
            Self::Run => "Run Summary",
            Self::Selection => "Selection Summary",
            Self::Artifacts => "Artifact Summary",
            Self::Compare => "Generation Compare",
            Self::Failures => "Failure Summary",
            Self::Suggest => "Next-Step Suggestions",
        }
    }
}

pub fn tool_list() -> &'static [AssistantTool] {
    &[
        AssistantTool::Help,
        AssistantTool::Run,
        AssistantTool::Selection,
        AssistantTool::Artifacts,
        AssistantTool::Compare,
        AssistantTool::Failures,
        AssistantTool::Suggest,
    ]
}

pub fn try_execute(prompt: &str, app: &App) -> Option<String> {
    let trimmed = prompt.trim();
    match trimmed {
        "/help" => Some(execute(AssistantTool::Help, app)),
        "/run" => Some(execute(AssistantTool::Run, app)),
        "/selection" => Some(execute(AssistantTool::Selection, app)),
        "/artifacts" => Some(execute(AssistantTool::Artifacts, app)),
        "/compare" => Some(execute(AssistantTool::Compare, app)),
        "/failures" => Some(execute(AssistantTool::Failures, app)),
        "/suggest" => Some(execute(AssistantTool::Suggest, app)),
        _ => None,
    }
}

pub fn tool_catalog() -> &'static str {
    "Available built-in PATINA tools: /help, /run, /selection, /artifacts, /compare, /failures, /suggest"
}

pub fn tool_protocol() -> &'static str {
    "Tool protocol: reply with either `ANSWER:` followed by the final response, or `TOOL: /command` using exactly one of the supported slash commands."
}

pub fn execute(tool: AssistantTool, app: &App) -> String {
    match tool {
        AssistantTool::Help => help_text(),
        AssistantTool::Run => run_summary(app),
        AssistantTool::Selection => selection_summary(app),
        AssistantTool::Artifacts => artifact_summary(app),
        AssistantTool::Compare => compare_summary(app),
        AssistantTool::Failures => failure_summary(app),
        AssistantTool::Suggest => rerun_suggestions(app),
    }
}

pub fn parse_tool_request(text: &str) -> Option<AssistantTool> {
    let line = text.lines().next()?.trim();
    let command = line.strip_prefix("TOOL:")?.trim();
    match command {
        "/help" => Some(AssistantTool::Help),
        "/run" => Some(AssistantTool::Run),
        "/selection" => Some(AssistantTool::Selection),
        "/artifacts" => Some(AssistantTool::Artifacts),
        "/compare" => Some(AssistantTool::Compare),
        "/failures" => Some(AssistantTool::Failures),
        "/suggest" => Some(AssistantTool::Suggest),
        _ => None,
    }
}

pub fn strip_answer_prefix(text: &str) -> &str {
    text.trim()
        .strip_prefix("ANSWER:")
        .map(str::trim)
        .unwrap_or_else(|| text.trim())
}

fn help_text() -> String {
    [
        "Built-in PATINA assistant tools",
        "/help - show the available local assistant tools",
        "/run - summarize the loaded run and current workflow state",
        "/selection - summarize the current selection context",
        "/artifacts - summarize the artifact contract and a few example paths",
        "/compare - compare the selected generation against the latest generation",
        "/failures - summarize failure and duplicate pressure in the loaded run",
        "/suggest - draft next-step rerun suggestions from current run evidence",
    ]
    .join("\n")
}

fn run_summary(app: &App) -> String {
    let workflow = match app.snapshot.workflow_kind {
        WorkflowKind::Ga => "GA",
        WorkflowKind::Bh => "BH",
        WorkflowKind::Unknown => "Unknown",
    };
    let latest_generation = app
        .snapshot
        .latest_generation_index()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    let best_energy = app
        .snapshot
        .generation_metrics
        .last()
        .and_then(|row| row.best_energy)
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "n/a".to_string());
    [
        "Run Summary".to_string(),
        format!("run dir: {}", app.snapshot.run_dir.display()),
        format!(
            "run name: {}",
            app.snapshot
                .manifest
                .run_name
                .as_deref()
                .unwrap_or("unnamed-run")
        ),
        format!("workflow kind: {workflow}"),
        format!(
            "backend: {}",
            app.snapshot.manifest.backend.as_deref().unwrap_or("n/a")
        ),
        format!(
            "workflow owner: {}",
            app.snapshot
                .manifest
                .workflow_owner
                .as_deref()
                .unwrap_or("n/a")
        ),
        format!("latest generation: {latest_generation}"),
        format!(
            "generation metrics rows: {}",
            app.snapshot.generation_metrics.len()
        ),
        format!("walker trace rows: {}", app.snapshot.walker_trace.len()),
        format!("best energy in latest metric row: {best_energy}"),
        format!("current screen: {}", app.screen.title()),
    ]
    .join("\n")
}

fn selection_summary(app: &App) -> String {
    let selected_generation = app.selected_generation_number();
    let selected_metric = app.snapshot.selected_metric(app.selected_generation);
    let selected_member = app.selected_member_record();
    [
        "Selection Summary".to_string(),
        format!("selected generation: {selected_generation}"),
        format!(
            "selected generation phase: {}",
            selected_metric
                .map(|row| row.phase.as_str())
                .unwrap_or("n/a")
        ),
        format!(
            "selected generation requests/success/failure: {}/{}/{}",
            selected_metric.map(|row| row.request_count).unwrap_or(0),
            selected_metric.map(|row| row.success_count).unwrap_or(0),
            selected_metric.map(|row| row.failure_count).unwrap_or(0)
        ),
        format!("selected member index: {}", app.selected_member),
        format!(
            "selected member label: {}",
            selected_member
                .map(|member| member.evaluation.structure.label.as_str())
                .unwrap_or("n/a")
        ),
        format!(
            "selected member energy: {}",
            selected_member
                .and_then(|member| member.evaluation.energy)
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "selected member converged: {}",
            selected_member
                .map(|member| member.evaluation.converged.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
    ]
    .join("\n")
}

fn artifact_summary(app: &App) -> String {
    let artifact_count = app.snapshot.manifest.artifacts.len();
    let sample = app
        .snapshot
        .all_artifact_paths()
        .into_iter()
        .take(5)
        .map(|path| format!("- {}", path.display()))
        .collect::<Vec<_>>();
    let mut lines = vec![
        "Artifact Summary".to_string(),
        format!("manifest artifacts: {artifact_count}"),
        format!("raw files: {}", app.snapshot.artifacts.raw_files.len()),
        format!(
            "output files: {}",
            app.snapshot.artifacts.output_files.len()
        ),
        "sample paths:".to_string(),
    ];
    if sample.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(sample);
    }
    lines.join("\n")
}

fn compare_summary(app: &App) -> String {
    let selected_generation = app.selected_generation_number();
    let latest_generation = app.snapshot.latest_generation_index();
    let selected = app.snapshot.selected_metric(app.selected_generation);
    let latest = latest_generation.and_then(|generation| {
        app.snapshot
            .generation_metrics
            .iter()
            .find(|row| row.generation == generation)
    });
    let best_delta = match (
        selected.and_then(|row| row.best_energy),
        latest.and_then(|row| row.best_energy),
    ) {
        (Some(selected_best), Some(latest_best)) => format!("{:.6}", latest_best - selected_best),
        _ => "n/a".to_string(),
    };
    [
        "Generation Comparison".to_string(),
        format!("selected generation: {selected_generation}"),
        format!(
            "latest generation: {}",
            latest_generation
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "selected best energy: {}",
            selected
                .and_then(|row| row.best_energy)
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "latest best energy: {}",
            latest
                .and_then(|row| row.best_energy)
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("latest - selected best energy delta: {best_delta}"),
        format!(
            "selected duplicate count: {}",
            selected
                .and_then(|row| row.duplicate_count)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "latest duplicate count: {}",
            latest
                .and_then(|row| row.duplicate_count)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
    ]
    .join("\n")
}

fn failure_summary(app: &App) -> String {
    if app.snapshot.workflow_kind == WorkflowKind::Bh {
        let rejected = app
            .snapshot
            .walker_trace
            .iter()
            .filter(|row| row.accepted.eq_ignore_ascii_case("false"))
            .count();
        return [
            "Failure Pressure Summary".to_string(),
            format!("walker rows: {}", app.snapshot.walker_trace.len()),
            format!("rejected steps: {rejected}"),
            "Interpretation: use BH traces and move classes before assuming backend failure."
                .to_string(),
        ]
        .join("\n");
    }

    let total_requests: usize = app
        .snapshot
        .generation_metrics
        .iter()
        .map(|row| row.request_count)
        .sum();
    let total_failures: usize = app
        .snapshot
        .generation_metrics
        .iter()
        .map(|row| row.failure_count)
        .sum();
    let total_duplicates: usize = app
        .snapshot
        .generation_metrics
        .iter()
        .map(|row| row.duplicate_count.unwrap_or(0))
        .sum();
    let total_repopulated: usize = app
        .snapshot
        .generation_metrics
        .iter()
        .map(|row| row.repopulated_count.unwrap_or(0))
        .sum();
    let hottest = app
        .snapshot
        .generation_metrics
        .iter()
        .max_by_key(|row| row.failure_count + row.duplicate_count.unwrap_or(0));
    [
        "Failure Pressure Summary".to_string(),
        format!("total requests: {total_requests}"),
        format!("total failures: {total_failures}"),
        format!("total duplicates: {total_duplicates}"),
        format!("total repopulated: {total_repopulated}"),
        format!(
            "highest-pressure generation: {}",
            hottest
                .map(|row| row.generation.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "highest-pressure generation detail: failures={} duplicates={}",
            hottest.map(|row| row.failure_count).unwrap_or(0),
            hottest.and_then(|row| row.duplicate_count).unwrap_or(0)
        ),
    ]
    .join("\n")
}

fn rerun_suggestions(app: &App) -> String {
    let latest = app.snapshot.generation_metrics.last();
    let workflow = match app.snapshot.workflow_kind {
        WorkflowKind::Ga => "GA",
        WorkflowKind::Bh => "BH",
        WorkflowKind::Unknown => "Unknown",
    };
    let mut suggestions = vec![
        "Next-Step Suggestions".to_string(),
        format!("workflow kind: {workflow}"),
    ];
    if latest.is_none() && app.snapshot.walker_trace.is_empty() {
        suggestions.push(
            "No metrics or walker trace loaded. First verify the run directory and artifact contract."
                .to_string(),
        );
        return suggestions.join("\n");
    }
    if let Some(latest) = latest {
        if latest.failure_count > 0 {
            suggestions.push(format!(
                "Failure pressure exists in generation {}. Inspect backend outputs before rerunning unchanged.",
                latest.generation
            ));
        }
        if latest.duplicate_count.unwrap_or(0) > 0 {
            suggestions.push(
                "Duplicate pressure is non-zero. Review diversity settings or duplicate policy before scaling up."
                    .to_string(),
            );
        }
        if latest.repopulated_count.unwrap_or(0) > 0 {
            suggestions.push(
                "Repopulation occurred. Check whether selection pressure or evaluation failures are starving the population."
                    .to_string(),
            );
        }
        if latest.failure_count == 0
            && latest.duplicate_count.unwrap_or(0) == 0
            && latest.repopulated_count.unwrap_or(0) == 0
        {
            suggestions.push(
                "Latest generation looks operationally clean. A controlled rerun or parameter sweep is reasonable."
                    .to_string(),
            );
        }
    }
    if app.snapshot.workflow_kind == WorkflowKind::Bh {
        suggestions.push(
            "For BH runs, compare accepted vs rejected moves and temperature/step-size trends before changing backend settings."
                .to_string(),
        );
    }
    suggestions.push(
        "Because PATINA is still under construction, prefer small guarded reruns over large autonomous campaign changes."
            .to_string(),
    );
    suggestions.join("\n")
}
