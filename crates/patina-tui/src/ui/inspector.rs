use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, Paragraph, Row, Table, Wrap};
use serde_json::Value;

use crate::app::state::App;
use crate::ui::theme;

pub fn render_generation(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::vertical([
        Constraint::Length(11),
        Constraint::Min(8),
        Constraint::Length(8),
    ])
    .split(area);
    let generation = app
        .snapshot
        .selected_metric(app.selected_generation)
        .map(|row| row.generation)
        .unwrap_or(app.selected_generation);
    let summary = app.snapshot.generation_summary(generation);
    let state = app.snapshot.generation_state(generation);
    let lines = vec![
        Line::from("What this screen does: inspect one generation, its members, and its controller behavior."),
        Line::from("Next: use j/k to move generations, h/l to change member context, 6 for workbench tools."),
        Line::from(format!("generation: {generation}")),
        Line::from(format!(
            "phase: {}",
            summary.map(|row| row.phase.as_str()).unwrap_or("n/a")
        )),
        Line::from(format!(
            "requests/success/failure: {}/{}/{}",
            summary.map(|row| row.request_count).unwrap_or(0),
            summary.map(|row| row.success_count).unwrap_or(0),
            summary.map(|row| row.failure_count).unwrap_or(0)
        )),
        Line::from(format!(
            "converged: {}  valid_population: {}",
            summary.map(|row| row.converged_count).unwrap_or(0),
            summary.map(|row| row.valid_population_size).unwrap_or(0)
        )),
        Line::from(format!(
            "duplicates: {}  repopulated: {}",
            summary.map(|row| row.duplicate_count).unwrap_or(0),
            summary.map(|row| row.repopulated_count).unwrap_or(0)
        )),
        Line::from(format!(
            "dup breakdown hashkey/pmoi/energy: {}/{}/{}",
            summary.map(|row| row.duplicate_hashkey_count).unwrap_or(0),
            summary.map(|row| row.duplicate_pmoi_count).unwrap_or(0),
            summary.map(|row| row.duplicate_energy_tol_count).unwrap_or(0)
        )),
        Line::from(format!(
            "elapsed_secs: {}  pop_size: {}",
            summary
                .map(|row| format!("{:.3}", row.elapsed_secs))
                .unwrap_or_else(|| "n/a".to_string()),
            summary.map(|row| row.population_size).unwrap_or(0)
        )),
        Line::from(format!(
            "state population/elites: {}/{}",
            state.map(|value| value.population.len()).unwrap_or(0),
            state.map(|value| value.elites.len()).unwrap_or(0)
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(theme::panel_block("Generation Summary")),
        sections[0],
    );

    let rows = state
        .map(|state| {
            state
                .population
                .iter()
                .map(|member| {
                    Row::new(vec![
                        member.member_id.to_string(),
                        member.origin.clone(),
                        member.occurrences.to_string(),
                        member.source.label.clone(),
                        member.evaluation.structure.label.clone(),
                        member
                            .evaluation
                            .energy
                            .map(|value| format!("{value:.3}"))
                            .unwrap_or_else(|| "n/a".to_string()),
                        if member.evaluation.converged {
                            "yes".to_string()
                        } else {
                            "no".to_string()
                        },
                    ])
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(18),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(5),
        ],
    )
    .header(
        Row::new(vec![
            "id", "origin", "occ", "source", "relaxed", "energy", "conv",
        ])
        .style(Style::default().fg(theme::GOOD_COLOR).bold()),
    )
    .block(theme::panel_block("Population Snapshot"));
    frame.render_widget(table, sections[1]);

    let trace_rows = app
        .snapshot
        .controller_rows_for_generation(generation)
        .map(|row| {
            Row::new(vec![
                row.stage.clone(),
                row.population_size.to_string(),
                row.valid_population_size.to_string(),
                row.child_count.to_string(),
                row.duplicate_count.to_string(),
                row.duplicate_hashkey_count.to_string(),
                row.duplicate_pmoi_count.to_string(),
                row.duplicate_energy_tol_count.to_string(),
                row.repopulated_count.to_string(),
                row.best_energy
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "n/a".to_string()),
                if row.selected_indices.is_empty() {
                    "-".to_string()
                } else {
                    row.selected_indices
                        .iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join("|")
                },
            ])
        })
        .collect::<Vec<_>>();
    let trace_table = Table::new(
        trace_rows,
        [
            Constraint::Length(12),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Min(10),
        ],
    )
    .header(
        Row::new(vec![
            "stage", "pop", "valid", "child", "dup", "hash", "pmoi", "e_tol", "repop", "best",
            "selected",
        ])
        .style(Style::default().fg(theme::GOOD_COLOR).bold()),
    )
    .block(theme::panel_block("Controller Trace"));
    frame.render_widget(trace_table, sections[2]);
}

pub fn render_artifacts(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let artifact_paths = app.snapshot.all_artifact_paths();
    let items = artifact_paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let marker = if index == app.selected_artifact {
                ">"
            } else {
                " "
            };
            ListItem::new(format!("{marker} {}", path.display()))
        })
        .collect::<Vec<_>>();
    let list = List::new(items).block(theme::panel_block("Artifacts"));
    frame.render_widget(list, sections[0]);

    let detail = artifact_paths
        .get(app.selected_artifact)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "no artifact selected".to_string());
    let paragraph = Paragraph::new(vec![
        Line::from("What this screen does: show the concrete files backing this run."),
        Line::from(
            "Next: move with j/k, then 4 for architecture if you want the higher-level contract.",
        ),
        Line::from(""),
        Line::from("Selected artifact"),
        Line::from(detail),
        Line::from(""),
        Line::from(format!(
            "raw files: {}  output files: {}",
            app.snapshot.artifacts.raw_files.len(),
            app.snapshot.artifacts.output_files.len()
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Artifact Detail"));
    frame.render_widget(paragraph, sections[1]);
}

pub fn render_architecture(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::vertical([Constraint::Length(12), Constraint::Min(8)]).split(area);
    let execution_path = vec![
        Line::from("What this screen does: summarize the current command, adapter, and artifact seams."),
        Line::from("Next: use j/k to inspect the artifact contract, then 5 for stage direction or 6 for tools."),
        Line::from(format!(
            "Command surface: patina-tui -> run dir `{}`",
            app.snapshot.run_dir.display()
        )),
        Line::from(format!(
            "Use-case view: workflow_owner={}  workflow_kind={:?}",
            app.snapshot
                .manifest
                .workflow_owner
                .as_deref()
                .unwrap_or("n/a"),
            app.snapshot.workflow_kind
        )),
        Line::from(format!(
            "Adapter view: backend={}  lane_mode={}  parallel_contract={}",
            app.snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
            app.snapshot.manifest.lane_mode.as_deref().unwrap_or("n/a"),
            app.snapshot
                .manifest
                .parallel_contract
                .as_deref()
                .unwrap_or("n/a"),
        )),
        Line::from(format!(
            "Artifact seam: metrics={} controller={} states={} walker={}",
            app.snapshot.generation_metrics.len(),
            app.snapshot.controller_trace.len(),
            app.snapshot.generations.states.len(),
            app.snapshot.walker_trace.len(),
        )),
        Line::from("Note: these labels are descriptive and may evolve with the architecture."),
    ];
    let summary = Paragraph::new(execution_path)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Architecture Summary"));
    frame.render_widget(summary, sections[0]);

    let lower = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);

    let contract_rows = app
        .architecture_items()
        .into_iter()
        .enumerate()
        .map(|(index, (key, value))| {
            let style = if index == app.selected_artifact {
                Style::default().fg(theme::SELECTED_COLOR).bold()
            } else {
                Style::default()
            };
            Row::new(vec![key, value]).style(style)
        })
        .collect::<Vec<_>>();
    let contract_table = Table::new(contract_rows, [Constraint::Length(32), Constraint::Min(20)])
        .header(
            Row::new(vec!["artifact key", "relative path"])
                .style(Style::default().fg(theme::GOOD_COLOR).bold()),
        )
        .block(theme::panel_block("Manifest Artifact Contract"));
    frame.render_widget(contract_table, lower[0]);

    let selected_contract = app
        .architecture_items()
        .get(app.selected_artifact)
        .cloned()
        .unwrap_or_else(|| ("n/a".to_string(), "n/a".to_string()));
    let detail_lines = vec![
        Line::from(format!("selected artifact key: {}", selected_contract.0)),
        Line::from(format!("selected path: {}", selected_contract.1)),
        Line::from(""),
        Line::from(format!(
            "search_config: {}",
            format_manifest_value(app.snapshot.manifest.search_config.as_ref())
        )),
        Line::from(""),
        Line::from(format!(
            "operator_policy: {}",
            format_manifest_value(app.snapshot.manifest.operator_policy.as_ref())
        )),
        Line::from(""),
        Line::from(format!(
            "extra: {}",
            format_manifest_value(app.snapshot.manifest.extra.as_ref())
        )),
    ];
    let detail = Paragraph::new(detail_lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Manifest Detail"));
    frame.render_widget(detail, lower[1]);
}

fn format_manifest_value(value: Option<&Value>) -> String {
    match value {
        Some(value) => {
            let compact = serde_json::to_string(value).unwrap_or_else(|_| "<unavailable>".into());
            if compact.len() > 120 {
                format!("{}...", &compact[..120])
            } else {
                compact
            }
        }
        None => "n/a".to_string(),
    }
}
