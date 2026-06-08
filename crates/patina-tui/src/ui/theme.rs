use ratatui::prelude::*;
use ratatui::widgets::block::BorderType;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::state::App;
use crate::domain::artifacts::WorkflowKind;
use crate::domain::launch;
use crate::domain::ports;

pub const ACCENT_COLOR: Color = Color::Cyan;
pub const MUTE_COLOR: Color = Color::Gray;
pub const BORDER_COLOR: Color = Color::DarkGray;
pub const SELECTED_COLOR: Color = Color::Yellow;
pub const GOOD_COLOR: Color = Color::Green;

pub fn panel_block<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_COLOR))
        .title(title.into().style(Style::default().fg(ACCENT_COLOR).bold()))
}

pub fn panel_block_state<'a>(title: impl Into<Line<'a>>, active: bool) -> Block<'a> {
    let border_color = if active { SELECTED_COLOR } else { BORDER_COLOR };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(title.into().style(Style::default().fg(ACCENT_COLOR).bold()))
}

pub fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let workflow = match app.snapshot.workflow_kind {
        WorkflowKind::Ga => "GA",
        WorkflowKind::Bh => "BH",
        WorkflowKind::Unknown => "Unknown",
    };
    let context_label = if app.snapshot.has_loaded_run() {
        app.snapshot
            .manifest
            .run_name
            .as_deref()
            .unwrap_or("unnamed-run")
    } else {
        "workspace"
    };
    let title = format!("PATINA-TUI  {}  {}", context_label, app.screen.title());
    let scope_label = if app.snapshot.has_loaded_run() {
        "run dir="
    } else {
        "workspace="
    };
    let scope_value = app.snapshot.run_dir.display().to_string();
    let details = Line::from(vec![
        Span::styled(scope_label, Style::default().fg(MUTE_COLOR)),
        Span::raw(scope_value),
        Span::raw("  "),
        Span::styled("workflow=", Style::default().fg(MUTE_COLOR)),
        Span::styled(workflow, Style::default().fg(ACCENT_COLOR).bold()),
        Span::raw("  "),
        Span::styled("system=", Style::default().fg(MUTE_COLOR)),
        Span::raw(app.snapshot.manifest.system.as_deref().unwrap_or("n/a")),
        Span::raw("  "),
        Span::styled("backend=", Style::default().fg(MUTE_COLOR)),
        Span::raw(app.snapshot.manifest.backend.as_deref().unwrap_or("n/a")),
        Span::raw("  "),
        Span::styled("owner=", Style::default().fg(MUTE_COLOR)),
        Span::raw(
            app.snapshot
                .manifest
                .workflow_owner
                .as_deref()
                .unwrap_or("n/a"),
        ),
    ]);
    let scope = Line::from(vec![
        Span::styled("scope: ", Style::default().fg(MUTE_COLOR)),
        Span::raw(if app.snapshot.has_loaded_run() {
            app.snapshot
                .manifest
                .workflow_scope
                .as_deref()
                .unwrap_or("no workflow scope recorded")
        } else {
            "launch-first workspace mode"
        }),
    ]);
    let text = Paragraph::new(vec![details, scope]).block(panel_block(title));
    frame.render_widget(text, area);
}

pub fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let widget = Paragraph::new(vec![
        Line::from(screen_help(app)),
        Line::from(format!(
            "1 dashboard 2 generation 3 artifacts 4 architecture 5 flow 6 workbench 7 launch 8 ports 9 assistant  h/l member or launch field  e export  r reload  f follow={}  launch: j/k workflow  h/l field  i Enter edit  flow: u/c/d/n m Enter  ports: a v x g Enter  assistant: p t x i Enter Esc j/k  roadmap: phase 1-4 in panel  ? help  q quit  status: {}",
            if app.follow_mode { "on" } else { "off" },
            app.status,
        )),
    ])
    .block(panel_block("Controls"));
    frame.render_widget(widget, area);
}

pub fn render_sidebar(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let (title, items) = if matches!(app.screen, crate::app::modes::Screen::Architecture) {
        let items = app
            .architecture_items()
            .iter()
            .enumerate()
            .map(|(index, (key, _))| {
                let marker = if index == app.selected_artifact {
                    ">"
                } else {
                    " "
                };
                row_item(marker, key, false)
            })
            .collect::<Vec<_>>();
        ("Artifact Contract", items)
    } else if matches!(app.screen, crate::app::modes::Screen::Flow) {
        let items = app
            .flow_stages()
            .iter()
            .enumerate()
            .map(|(index, stage)| {
                let marker = if index == app.selected_artifact {
                    ">"
                } else {
                    " "
                };
                row_item(marker, stage.name, false)
            })
            .collect::<Vec<_>>();
        ("Flow Stages", items)
    } else if matches!(app.screen, crate::app::modes::Screen::Workbench) {
        let items = crate::domain::workbench::registry()
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                let marker = if index == app.selected_artifact {
                    ">"
                } else {
                    " "
                };
                row_item(marker, operation.name, false)
            })
            .collect::<Vec<_>>();
        ("Workbench Ops", items)
    } else if matches!(app.screen, crate::app::modes::Screen::Launch) {
        let items = launch::registry()
            .iter()
            .enumerate()
            .map(|(index, workflow)| {
                let marker = if index == app.selected_artifact {
                    ">"
                } else {
                    " "
                };
                let active = launch::matches_current_run(workflow, &app.snapshot);
                row_item(marker, workflow.name, active)
            })
            .collect::<Vec<_>>();
        ("Workflow Catalog", items)
    } else if matches!(app.screen, crate::app::modes::Screen::Ports) {
        let items = app
            .ports_items()
            .iter()
            .enumerate()
            .map(|(index, binding)| {
                let marker = if index == app.selected_artifact {
                    ">"
                } else {
                    " "
                };
                let active = ports::matches_current_run(binding, &app.snapshot);
                row_item(marker, binding.name, active)
            })
            .collect::<Vec<_>>();
        ("Ports & Adapters", items)
    } else if matches!(app.screen, crate::app::modes::Screen::Assistant) {
        let items = app
            .assistant_providers()
            .iter()
            .enumerate()
            .map(|(index, provider)| {
                let marker = if index == app.selected_artifact {
                    ">"
                } else {
                    " "
                };
                row_item(marker, &provider.label, provider.enabled)
            })
            .collect::<Vec<_>>();
        ("Assistant Providers", items)
    } else if !app.snapshot.generation_metrics.is_empty() {
        let items = app
            .snapshot
            .generation_metrics
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let marker = if index == app.selected_generation {
                    ">"
                } else {
                    " "
                };
                let energy = row
                    .best_energy
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "n/a".to_string());
                row_item(
                    marker,
                    &format!("g{:04}  best {energy}", row.generation),
                    false,
                )
            })
            .collect::<Vec<_>>();
        ("Generations", items)
    } else {
        let items = app
            .snapshot
            .walker_trace
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let marker = if index == app.selected_generation {
                    ">"
                } else {
                    " "
                };
                let energy = row
                    .energy
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "n/a".to_string());
                row_item(
                    marker,
                    &format!("step {:04}  {}  {}", row.step, row.accepted, energy),
                    false,
                )
            })
            .collect::<Vec<_>>();
        ("Walker Trace", items)
    };
    let list = List::new(items).block(panel_block(title));
    frame.render_widget(list, area);
}

fn screen_help(app: &App) -> &'static str {
    match app.screen {
        crate::app::modes::Screen::Dashboard => {
            if app.snapshot.has_loaded_run() {
                "Dashboard: read the run at a glance, then move to 2 for details or 5 for flow."
            } else {
                "Dashboard: start from workflow choice first; use 7 Launch as the primary entrypoint."
            }
        }
        crate::app::modes::Screen::GenerationInspector => {
            "Generation: inspect one generation, then use h/l to shift member context for deeper checks."
        }
        crate::app::modes::Screen::ArtifactBrowser => {
            "Artifacts: verify the filesystem contract and inspect what the workflow actually wrote."
        }
        crate::app::modes::Screen::Architecture => {
            "Architecture: see the current command, adapter, and artifact seams without assuming they are final."
        }
        crate::app::modes::Screen::Flow => {
            "Flow: u/c/d/n focus pane, m cycle panes, Enter open linked workspace for focused pane."
        }
        crate::app::modes::Screen::Workbench => {
            "Workbench: run small focused tools on the selected structure and inspect the result immediately."
        }
        crate::app::modes::Screen::Launch => {
            "Launch: edit a shared workflow draft, inspect file contracts, and derive the command shape from that draft."
        }
        crate::app::modes::Screen::Ports => {
            "Ports: a filter all/relevant, v cycle detail, x probe seam checks, g open launch prefilled, Enter open linked workspace."
        }
        crate::app::modes::Screen::Assistant => {
            "Assistant: phase 1 co-pilot, grounded on current run context; x probes provider health."
        }
    }
}

fn row_item(marker: &str, label: &str, active: bool) -> ListItem<'static> {
    let mut spans = vec![
        Span::styled(format!("{marker} "), Style::default().fg(SELECTED_COLOR)),
        Span::raw(label.to_string()),
    ];
    if active {
        spans.push(Span::raw(" "));
        spans.push(Span::styled("*", Style::default().fg(GOOD_COLOR).bold()));
    }
    ListItem::new(Line::from(spans))
}
