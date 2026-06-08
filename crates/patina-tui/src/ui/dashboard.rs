use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::symbols;
use ratatui::widgets::{Axis, Chart, Dataset, Paragraph, Row, Table, Wrap};

use crate::app::state::App;
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if !app.snapshot.has_loaded_run() {
        render_workspace_start(frame, area, app);
        return;
    }
    let chunks = Layout::vertical([Constraint::Length(11), Constraint::Min(8)]).split(area);
    if app.snapshot.generation_metrics.is_empty() && !app.snapshot.walker_trace.is_empty() {
        render_bh_summary(frame, chunks[0], app);
        render_bh_table(frame, chunks[1], app);
    } else {
        render_summary(frame, chunks[0], app);
        render_metrics_table(frame, chunks[1], app);
    }
}

fn render_workspace_start(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::vertical([Constraint::Length(12), Constraint::Min(10)]).split(area);
    let workspace_root = app.snapshot.run_dir.display().to_string();
    let summary = Paragraph::new(vec![
        Line::from("What this screen does: start from the core workflows instead of assuming an existing GA artifact tree."),
        Line::from(""),
        Line::from(format!("workspace root: {workspace_root}")),
        Line::from("loaded run: none"),
        Line::from("primary action: go to 7 for Launch and choose a workflow family"),
        Line::from("secondary actions: 8 for ports and 5 for architecture flow"),
        Line::from("reload/follow: unavailable until a run directory is attached at startup"),
        Line::from("design note: keep the first screen action-oriented, then reveal run diagnostics only when a run exists."),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Workspace Start"));
    frame.render_widget(summary, sections[0]);

    let lower = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);
    let launch = Paragraph::new(vec![
        Line::from("Launch-first workflow"),
        Line::from(""),
        Line::from("1. Choose a workflow in 7 Launch."),
        Line::from("2. Read the required inputs and command preview."),
        Line::from("3. Run the command outside the TUI."),
        Line::from("4. Restart `patina-tui <run-dir>` to inspect outputs."),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Recommended Flow"));
    frame.render_widget(launch, lower[0]);

    let rationale = Paragraph::new(vec![
        Line::from("Why this is simpler"),
        Line::from(""),
        Line::from("Recognition over recall: visible workflow choices beat memorized routes."),
        Line::from("Progressive disclosure: launch first, diagnostics later."),
        Line::from("Minimalism: no empty generation tables pretending a run already exists."),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Design Rationale"));
    frame.render_widget(rationale, lower[1]);
}

fn render_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::horizontal([Constraint::Length(38), Constraint::Min(20)]).split(area);
    let selected = app.snapshot.selected_metric(app.selected_generation);
    let latest = app
        .snapshot
        .generation_metrics
        .last()
        .map(|row| row.generation.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    let summary = Paragraph::new(vec![
        Line::from("What this screen does: show the run trend first, then let you drill into one generation."),
        Line::from("Next: use j/k to choose a generation, 2 to inspect it, 5 to see workflow direction."),
        Line::from(format!(
            "run dir: {}",
            app.snapshot.run_dir.display()
        )),
        Line::from(format!(
            "requested generations: {}",
            app.snapshot
                .manifest
                .requested_generations
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        )),
        Line::from(format!(
            "population size: {}",
            app.snapshot
                .manifest
                .population_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        )),
        Line::from(format!("latest generation: {latest}")),
        Line::from(format!(
            "selected phase: {}",
            selected
                .map(|row| row.phase.as_str())
                .unwrap_or("n/a")
        )),
        Line::from(format!(
            "selected duplicate pressure: {}",
            selected
                .and_then(|row| row.duplicate_count)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        )),
        Line::from(format!(
            "requests/success/failure: {}/{}/{}",
            selected.map(|row| row.request_count).unwrap_or(0),
            selected.map(|row| row.success_count).unwrap_or(0),
            selected.map(|row| row.failure_count).unwrap_or(0)
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Run Summary"));
    frame.render_widget(summary, sections[0]);

    let mut best_points = Vec::new();
    for row in &app.snapshot.generation_metrics {
        if let Some(best_energy) = row.best_energy {
            best_points.push((row.generation as f64, best_energy));
        }
    }
    let dataset = Dataset::default()
        .name("best energy")
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(theme::ACCENT_COLOR))
        .data(&best_points);
    let x_max = app
        .snapshot
        .generation_metrics
        .last()
        .map(|row| row.generation as f64)
        .unwrap_or(1.0)
        .max(1.0);
    let (mut y_min, mut y_max) = (-1.0, 1.0);
    if let Some((min, max)) = energy_bounds(&best_points) {
        y_min = min;
        y_max = max;
    }
    let chart = Chart::new(vec![dataset])
        .block(theme::panel_block("Best Energy Trend"))
        .x_axis(Axis::default().title("generation").bounds([0.0, x_max]))
        .y_axis(Axis::default().title("energy").bounds([y_min, y_max]));
    frame.render_widget(chart, sections[1]);
}

fn render_bh_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::horizontal([Constraint::Length(38), Constraint::Min(20)]).split(area);
    let latest = app.snapshot.walker_trace.last();
    let summary = Paragraph::new(vec![
        Line::from(
            "What this screen does: show the BH trajectory, acceptance behavior, and latest state.",
        ),
        Line::from(
            "Next: use j/k to move through steps, then 3 for artifacts or 4 for architecture.",
        ),
        Line::from(format!("run dir: {}", app.snapshot.run_dir.display())),
        Line::from(format!("steps loaded: {}", app.snapshot.walker_trace.len())),
        Line::from(format!(
            "latest walker: {}",
            latest.map(|row| row.walker_id.as_str()).unwrap_or("n/a")
        )),
        Line::from(format!(
            "latest move: {}",
            latest.map(|row| row.move_class.as_str()).unwrap_or("n/a")
        )),
        Line::from(format!(
            "latest accepted: {}",
            latest.map(|row| row.accepted.as_str()).unwrap_or("n/a")
        )),
        Line::from(format!(
            "latest best energy: {}",
            latest
                .and_then(|row| row.best_energy)
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "n/a".to_string())
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("BH Summary"));
    frame.render_widget(summary, sections[0]);

    let points = app
        .snapshot
        .walker_trace
        .iter()
        .filter_map(|row| row.energy.map(|energy| (row.step as f64, energy)))
        .collect::<Vec<_>>();
    let dataset = Dataset::default()
        .name("walker energy")
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::LightMagenta))
        .data(&points);
    let x_max = app
        .snapshot
        .walker_trace
        .last()
        .map(|row| row.step as f64)
        .unwrap_or(1.0)
        .max(1.0);
    let (mut y_min, mut y_max) = (-1.0, 1.0);
    if let Some((min, max)) = energy_bounds(&points) {
        y_min = min;
        y_max = max;
    }
    let chart = Chart::new(vec![dataset])
        .block(theme::panel_block("Walker Energy Trend"))
        .x_axis(Axis::default().title("step").bounds([0.0, x_max]))
        .y_axis(Axis::default().title("energy").bounds([y_min, y_max]));
    frame.render_widget(chart, sections[1]);
}

fn render_bh_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app
        .snapshot
        .walker_trace
        .iter()
        .map(|row| {
            Row::new(vec![
                row.step.to_string(),
                row.walker_id.clone(),
                row.accepted.clone(),
                format_optional_f64(row.energy),
                format_optional_f64(row.best_energy),
                format_optional_f64(row.temperature),
                format_optional_f64(row.step_size),
                row.move_class.clone(),
                row.label.clone(),
            ])
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Min(12),
        ],
    )
    .header(
        Row::new(vec![
            "step", "walker", "accepted", "energy", "best", "temp", "step", "move", "label",
        ])
        .style(Style::default().fg(theme::GOOD_COLOR).bold()),
    )
    .block(theme::panel_block("Walker Trace"));
    frame.render_widget(table, area);
}

fn render_metrics_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app
        .snapshot
        .generation_metrics
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let selected_style = if index == app.selected_generation {
                Style::default().fg(theme::SELECTED_COLOR).bold()
            } else {
                Style::default()
            };
            Row::new(vec![
                row.generation.to_string(),
                row.phase.clone(),
                format_optional_f64(row.best_energy),
                format_optional_f64(row.mean_energy),
                format_optional_f64(row.worst_energy),
                row.converged_count.to_string(),
                row.failure_count.to_string(),
                row.duplicate_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                row.repopulated_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
            ])
            .style(selected_style)
        })
        .collect::<Vec<_>>();
    let widths = [
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(11),
        Constraint::Length(11),
    ];
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec![
                "gen",
                "phase",
                "best",
                "mean",
                "worst",
                "conv",
                "fail",
                "duplicates",
                "repop",
            ])
            .style(Style::default().fg(theme::GOOD_COLOR).bold()),
        )
        .block(theme::panel_block("Generation Metrics"));
    frame.render_widget(table, area);
}

fn energy_bounds(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let mut min = points[0].1;
    let mut max = points[0].1;
    for (_, value) in points.iter().copied() {
        min = min.min(value);
        max = max.max(value);
    }
    if (max - min).abs() < f64::EPSILON {
        Some((min - 1.0, max + 1.0))
    } else {
        Some((min, max))
    }
}

fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}
