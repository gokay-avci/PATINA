use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::App;
use crate::domain::ports;
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let bindings = app.ports_items();
    if bindings.is_empty() {
        let empty = Paragraph::new("No port seams available.")
            .wrap(Wrap { trim: true })
            .block(theme::panel_block("Ports"));
        frame.render_widget(empty, area);
        return;
    }
    let binding = bindings
        .get(app.selected_artifact)
        .cloned()
        .unwrap_or(bindings[0]);

    let sections = Layout::vertical([Constraint::Length(10), Constraint::Min(10)]).split(area);
    let guidance = Paragraph::new(vec![
        Line::from(
            "What this screen does: expose use-case -> port -> adapter -> artifact directionality.",
        ),
        Line::from(""),
        Line::from(format!("selected seam: {}", binding.name)),
        Line::from(ports::current_binding_note(&binding, &app.snapshot)),
        Line::from(format!(
            "mode={} filter={}",
            app.ports_detail_mode.title(),
            if app.ports_only_relevant {
                "relevant-only"
            } else {
                "all"
            }
        )),
        Line::from("Keys: a filter  v mode  x probe  g launch prefill  Enter open workspace."),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Guidance"));
    frame.render_widget(guidance, sections[0]);

    let lower = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(sections[1]);
    let left =
        Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)]).split(lower[0]);
    let right =
        Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)]).split(lower[1]);

    let summary = Paragraph::new(vec![
        Line::from(format!("use-case: {}", binding.use_case)),
        Line::from(""),
        Line::from(format!("port contract: {}", binding.port_contract)),
        Line::from(format!("adapter surface: {}", binding.adapter_surface)),
        Line::from(""),
        Line::from(format!("upstream: {}", binding.upstream)),
        Line::from(""),
        Line::from(format!("downstream: {}", binding.downstream)),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Seam Summary"));
    frame.render_widget(summary, left[0]);

    let card = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Model", Style::default().fg(theme::ACCENT_COLOR).bold()),
            Span::raw("  "),
            Span::styled(
                "Port-Adapter",
                Style::default().fg(theme::GOOD_COLOR).bold(),
            ),
        ]),
        Line::from(""),
        Line::from(format!("seam: {}", binding.name)),
        Line::from(format!(
            "current_run_match: {}",
            if ports::matches_current_run(&binding, &app.snapshot) {
                "yes"
            } else {
                "no"
            }
        )),
        Line::from(format!("detail_mode: {}", app.ports_detail_mode.title())),
        Line::from(format!("routes: {}", binding.workflow_routes.join(", "))),
        Line::from(format!(
            "artifact_contract_items: {}",
            binding.artifacts.len()
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Model Card"));
    frame.render_widget(card, left[1]);

    let direction_lines = match app.ports_detail_mode {
        ports::PortsDetailMode::Contract => vec![
            Line::from("Directionality"),
            Line::from(""),
            Line::from("Upstream"),
            Line::from(binding.upstream),
            Line::from(""),
            Line::from("Port"),
            Line::from(binding.port_contract),
            Line::from(""),
            Line::from("Adapter"),
            Line::from(binding.adapter_surface),
            Line::from(""),
            Line::from("Downstream"),
            Line::from(binding.downstream),
            Line::from(""),
            Line::from(binding.directionality),
        ],
        ports::PortsDetailMode::Evidence => {
            let mut lines = vec![Line::from("Current run evidence"), Line::from("")];
            for line in ports::evidence_lines(binding, &app.snapshot) {
                lines.push(Line::from(format!("- {line}")));
            }
            lines
        }
        ports::PortsDetailMode::Actions => {
            let mut lines = vec![Line::from("Seam actions"), Line::from("")];
            for line in ports::action_lines(binding, &app.snapshot) {
                lines.push(Line::from(format!("- {line}")));
            }
            lines
        }
    };
    let direction = Paragraph::new(direction_lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block(app.ports_detail_mode.title()));
    frame.render_widget(direction, right[0]);

    let mut detail_lines = vec![Line::from("Workflow routes")];
    push_lines(&mut detail_lines, binding.workflow_routes);
    detail_lines.push(Line::from(""));
    detail_lines.push(Line::from("Artifacts"));
    push_lines(&mut detail_lines, binding.artifacts);
    detail_lines.push(Line::from(""));
    detail_lines.push(Line::from("Probe"));
    if let Some(report) = app
        .ports_last_probe
        .as_ref()
        .filter(|report| report.binding_name == binding.name)
    {
        let (ok, fail) = report.totals();
        detail_lines.push(Line::from(format!(
            "- last run: {ok} passed, {fail} failed"
        )));
        for check in &report.checks {
            let status = if check.ok { "ok" } else { "fail" };
            detail_lines.push(Line::from(format!(
                "- [{status}] {} ({})",
                check.label, check.detail
            )));
        }
    } else {
        detail_lines.push(Line::from("- no probe results yet; press x"));
    }
    let detail = Paragraph::new(detail_lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Routes, Artifacts & Probe"));
    frame.render_widget(detail, right[1]);
}

fn push_lines(lines: &mut Vec<Line<'static>>, items: &[&str]) {
    for item in items {
        lines.push(Line::from(format!("- {item}")));
    }
}
