use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::App;
use crate::domain::launch;
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let workflows = launch::registry();
    let workflow = workflows
        .get(app.selected_artifact)
        .copied()
        .unwrap_or(workflows[0]);

    let sections = Layout::vertical([Constraint::Length(8), Constraint::Min(10)]).split(area);
    let guidance = Paragraph::new(vec![
        Line::from("What this screen does: expose the shared workflow contract, not just a route name."),
        Line::from(""),
        Line::from(format!("selected workflow: {}", workflow.name)),
        Line::from(format!("family: {}", workflow.family)),
        Line::from(launch::current_binding_note(&workflow, &app.snapshot)),
        Line::from("Next: j/k changes workflow."),
        Line::from("Then read the input contract, file contract, and command shape before leaving the TUI."),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Guidance"));
    frame.render_widget(guidance, sections[0]);

    let lower = Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(sections[1]);
    let left =
        Layout::vertical([Constraint::Percentage(54), Constraint::Percentage(46)]).split(lower[0]);
    let right =
        Layout::vertical([Constraint::Percentage(42), Constraint::Percentage(58)]).split(lower[1]);

    let summary = Paragraph::new(vec![
        Line::from(format!("route: {}", workflow.route)),
        Line::from(format!("family: {}", workflow.family)),
        Line::from(""),
        Line::from(workflow.summary),
        Line::from(""),
        Line::from(format!("when to use: {}", workflow.when_to_use)),
        Line::from(""),
        Line::from(format!("inbound port: {}", workflow.inbound_port)),
        Line::from(format!("adapter: {}", workflow.adapter)),
        Line::from(""),
        Line::from(format!("upstream: {}", workflow.upstream)),
        Line::from(""),
        Line::from(format!("downstream: {}", workflow.downstream)),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Workflow Summary"));
    frame.render_widget(summary, left[0]);

    let draft_lines = app
        .launch_draft
        .render_field_lines(app.launch_selected_field, app.launch_input_mode);
    frame.render_widget(
        Paragraph::new(render_panel_lines(draft_lines))
            .wrap(Wrap { trim: true })
            .block(theme::panel_block(if app.launch_input_mode {
                "Launch Draft [editing]"
            } else {
                "Launch Draft"
            })),
        left[1],
    );

    let right_top = Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(right[0]);
    let binding = launch::binding_panel(&workflow, &app.snapshot);
    frame.render_widget(
        Paragraph::new(render_panel_lines(binding.lines))
            .wrap(Wrap { trim: true })
            .block(theme::panel_block(binding.title)),
        right_top[0],
    );

    let command = Paragraph::new(app.launch_draft.render_command_preview(&workflow))
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Command Preview"));
    frame.render_widget(command, right_top[1]);

    let bottom = Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(right[1]);
    let input_contract = launch::input_contract_panel(&workflow);
    frame.render_widget(
        Paragraph::new(render_panel_lines(input_contract.lines))
            .wrap(Wrap { trim: true })
            .block(theme::panel_block(input_contract.title)),
        bottom[0],
    );
    let file_contract = launch::file_contract_panel(&workflow);
    frame.render_widget(
        Paragraph::new(render_panel_lines(file_contract.lines))
            .wrap(Wrap { trim: true })
            .block(theme::panel_block(file_contract.title)),
        bottom[1],
    );
}

fn render_panel_lines(lines: Vec<String>) -> Vec<Line<'static>> {
    lines.into_iter().map(Line::from).collect()
}
