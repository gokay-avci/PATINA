use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::App;
use crate::domain::workbench;
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::vertical([Constraint::Length(10), Constraint::Min(10)]).split(area);
    let operation = workbench::registry()
        .get(app.selected_artifact)
        .cloned()
        .unwrap_or_else(|| workbench::registry()[0].clone());

    let guidance = Paragraph::new(vec![
        Line::from("What this screen does: run small focused tools on the selected structure."),
        Line::from(""),
        Line::from(format!("generation: {}", app.selected_generation_number())),
        Line::from(format!("member: {}", app.selected_member)),
        Line::from(format!("structure: {}", app.selected_member_label())),
        Line::from(
            "Next: j/k changes the tool, h/l changes the structure context, e exports the output.",
        ),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Guidance"));
    frame.render_widget(guidance, sections[0]);

    let lower = Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(sections[1]);
    let summary = Paragraph::new(vec![
        Line::from(format!("operation: {}", operation.name)),
        Line::from(""),
        Line::from(operation.summary),
        Line::from(""),
        Line::from(format!("upstream: {}", operation.upstream)),
        Line::from(""),
        Line::from(format!("downstream: {}", operation.downstream)),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Operation"));
    frame.render_widget(summary, lower[0]);

    let generation = app
        .snapshot
        .selected_metric(app.selected_generation)
        .map(|row| row.generation)
        .unwrap_or(app.selected_generation);
    let output = workbench::run_operation(
        operation.kind,
        &app.snapshot,
        generation,
        app.selected_member,
    )
    .unwrap_or_else(|error| format!("operation unavailable\n\n{error}"));
    let output = Paragraph::new(output)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Workbench Output"));
    frame.render_widget(output, lower[1]);
}
