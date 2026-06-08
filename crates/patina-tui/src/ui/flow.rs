use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::{App, FlowPane};
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::vertical([Constraint::Length(8), Constraint::Min(10)]).split(area);
    let stage = app
        .flow_stages()
        .get(app.selected_artifact)
        .copied()
        .unwrap_or(crate::app::state::FlowStage {
            name: "n/a",
            purpose: "n/a",
            upstream: "n/a",
            current: "n/a",
            downstream: "n/a",
        });
    let guidance = vec![
        Line::from("What this screen does: expose one stage in layered upstream/current/downstream/action panes."),
        Line::from(""),
        Line::from(format!("stage: {}", stage.name)),
        Line::from(format!("focused pane: {}", app.flow_focus.title())),
        Line::from("Keys: u/c/d/n choose pane, m cycle pane, Enter open pane workspace."),
    ];
    frame.render_widget(
        Paragraph::new(guidance)
            .wrap(Wrap { trim: true })
            .block(theme::panel_block("Guidance")),
        sections[0],
    );

    let lower = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(sections[1]);
    let left =
        Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)]).split(lower[0]);
    let summary = Paragraph::new(vec![
        Line::from(format!("stage: {}", stage.name)),
        Line::from(""),
        Line::from(stage.purpose),
        Line::from(""),
        Line::from(format!(
            "workflow_owner={} backend={} parallel_contract={}",
            app.snapshot
                .manifest
                .workflow_owner
                .as_deref()
                .unwrap_or("n/a"),
            app.snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
            app.snapshot
                .manifest
                .parallel_contract
                .as_deref()
                .unwrap_or("n/a"),
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Stage Summary"));
    frame.render_widget(summary, left[0]);

    let card = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Model", Style::default().fg(theme::ACCENT_COLOR).bold()),
            Span::raw("  "),
            Span::styled(
                "Flow-Layered",
                Style::default().fg(theme::GOOD_COLOR).bold(),
            ),
        ]),
        Line::from(""),
        Line::from(format!("design_choice: {}", stage.name)),
        Line::from(format!("focused_pane: {}", app.flow_focus.title())),
        Line::from(format!(
            "gateway_workspace: {}",
            workspace_for_pane(app.flow_focus)
        )),
        Line::from(format!(
            "binding: owner={} backend={}",
            app.snapshot
                .manifest
                .workflow_owner
                .as_deref()
                .unwrap_or("n/a"),
            app.snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
        )),
        Line::from(format!(
            "lane_mode={} parallel_contract={}",
            app.snapshot.manifest.lane_mode.as_deref().unwrap_or("n/a"),
            app.snapshot
                .manifest
                .parallel_contract
                .as_deref()
                .unwrap_or("n/a"),
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Model Card"));
    frame.render_widget(card, left[1]);

    let right = Layout::vertical([Constraint::Length(4), Constraint::Min(10)]).split(lower[1]);
    let strip = Paragraph::new(vec![
        Line::from("Pane Layering"),
        Line::from(format!(
            "u upstream | c current | d downstream | n actions | m cycle | Enter open ({})",
            workspace_for_pane(app.flow_focus)
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Layer Strip"));
    frame.render_widget(strip, right[0]);

    let grid_rows =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(right[1]);
    let row1 = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(grid_rows[0]);
    let row2 = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(grid_rows[1]);

    let upstream = Paragraph::new(vec![Line::from(stage.upstream)])
        .wrap(Wrap { trim: true })
        .block(theme::panel_block_state(
            "Upstream",
            matches!(app.flow_focus, FlowPane::Upstream),
        ));
    frame.render_widget(upstream, row1[0]);

    let current = Paragraph::new(vec![
        Line::from(stage.current),
        Line::from(""),
        Line::from(format!(
            "owner={} backend={} lane_mode={}",
            app.snapshot
                .manifest
                .workflow_owner
                .as_deref()
                .unwrap_or("n/a"),
            app.snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
            app.snapshot.manifest.lane_mode.as_deref().unwrap_or("n/a"),
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block_state(
        "Current",
        matches!(app.flow_focus, FlowPane::Current),
    ));
    frame.render_widget(current, row1[1]);

    let downstream = Paragraph::new(vec![
        Line::from(stage.downstream),
        Line::from(""),
        Line::from(format!(
            "artifacts: manifest={} raw={} outputs={}",
            app.snapshot.manifest.artifacts.len(),
            app.snapshot.artifacts.raw_files.len(),
            app.snapshot.artifacts.output_files.len()
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block_state(
        "Downstream",
        matches!(app.flow_focus, FlowPane::Downstream),
    ));
    frame.render_widget(downstream, row2[0]);

    let actions = Paragraph::new(vec![
        Line::from(format!(
            "Enter opens `{}` workspace for this pane.",
            workspace_for_pane(app.flow_focus)
        )),
        Line::from(""),
        Line::from("5 flow stage navigation"),
        Line::from("8 ports seam interactivity"),
        Line::from("7 launch route planning"),
        Line::from("6 workbench local structure probes"),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block_state(
        "Actions",
        matches!(app.flow_focus, FlowPane::Actions),
    ));
    frame.render_widget(actions, row2[1]);
}

fn workspace_for_pane(pane: FlowPane) -> &'static str {
    match pane {
        FlowPane::Upstream => "architecture",
        FlowPane::Current => "ports",
        FlowPane::Downstream => "artifacts",
        FlowPane::Actions => "launch",
    }
}
