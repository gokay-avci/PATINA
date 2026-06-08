use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::App;
use crate::domain::assistant;
use crate::domain::assistant_tools;
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(9),
    ])
    .split(area);
    render_control_plane(frame, rows[0], app);
    render_transcript(frame, rows[1], app);
    render_tool_history(frame, rows[2], app);
    render_prompt(frame, rows[3], app);
    render_architecture(frame, rows[4], app);
}

fn render_control_plane(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let cols = Layout::horizontal([Constraint::Length(46), Constraint::Min(24)]).split(area);
    let profile = app.selected_assistant_provider();
    let control = Paragraph::new(vec![
        Line::from("This screen owns model routing and the future assistant request seam."),
        Line::from(
            "Status: assistant and broader PATINA functionality are still under construction.",
        ),
        Line::from(format!("config: {}", app.llm_config_path.display())),
        Line::from(format!("provider profile: {}", profile.label)),
        Line::from(format!("provider kind: {}", profile.kind.title())),
        Line::from(format!("endpoint: {}", profile.endpoint.base_url)),
        Line::from(format!("auth: {}", app.assistant_endpoint_auth_label())),
        Line::from(format!("model: {}", profile.model)),
        Line::from(format!("enabled: {}", profile.enabled)),
        Line::from(format!("probe: {}", app.assistant_probe_status)),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Assistant Control Plane"));
    frame.render_widget(control, cols[0]);

    let detail = Paragraph::new(vec![
        Line::from(format!("provider id: {}", profile.id)),
        Line::from(format!(
            "timeout: {}s",
            profile.endpoint.request_timeout_secs
        )),
        Line::from(""),
        Line::from("Current runtime path: patina-tui -> patina-llm -> provider gateway"),
        Line::from("vLLM is live; hosted and Ollama routes remain selectable architecture lanes."),
        Line::from(
            "Use `x` to probe provider reachability and listed models before sending prompts.",
        ),
        Line::from(
            "Built-in tools: /help  /run  /selection  /artifacts  /compare  /failures  /suggest",
        ),
        Line::from(
            "Key tools: H help  R run  S selection  A artifacts  C compare  F failures  N suggest",
        ),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Selected Route"));
    frame.render_widget(detail, cols[1]);
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = app
        .assistant_session
        .messages
        .iter()
        .flat_map(|message| {
            let color = match message.role {
                crate::domain::assistant::MessageRole::System => theme::MUTE_COLOR,
                crate::domain::assistant::MessageRole::User => theme::SELECTED_COLOR,
                crate::domain::assistant::MessageRole::Assistant => theme::ACCENT_COLOR,
                crate::domain::assistant::MessageRole::Tool => theme::GOOD_COLOR,
            };
            let body = if let Some(event) = &message.event {
                match event {
                    crate::domain::assistant::AssistantEvent::ToolRequest { tool_name } => {
                        format!("requested {tool_name}")
                    }
                    crate::domain::assistant::AssistantEvent::ToolResult {
                        tool_name,
                        status,
                        output,
                    } => {
                        format!("{tool_name} {}\n{output}", status.title())
                    }
                }
            } else {
                message.body.clone()
            };
            [
                Line::from(vec![
                    Span::styled(
                        format!("{} ", message.role.title()),
                        Style::default().fg(color).bold(),
                    ),
                    Span::raw(body),
                ]),
                Line::from(""),
            ]
        })
        .collect::<Vec<_>>();
    let transcript = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block(app.assistant_session.title.as_str()));
    frame.render_widget(transcript, area);
}

fn render_prompt(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = if app.assistant_input_mode {
        "Prompt Input (editing)"
    } else {
        "Prompt Input"
    };
    let input = Paragraph::new(vec![
        Line::from(app.assistant_input.as_str()),
        Line::from(""),
        Line::from(
            "Press `i` or `Enter` to edit. Press `Enter` again to send. Press `Esc` to stop editing.",
        ),
        Line::from("Local tools: /help  /run  /selection  /artifacts  /compare  /failures  /suggest"),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block_state(title, app.assistant_input_mode));
    frame.render_widget(input, area);
}

fn render_tool_history(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![Line::from("Recent tool activity")];
    let mut count = 0usize;
    for message in app.assistant_session.messages.iter().rev() {
        let Some(event) = &message.event else {
            continue;
        };
        match event {
            crate::domain::assistant::AssistantEvent::ToolRequest { tool_name } => {
                lines.push(Line::from(format!("assistant requested {tool_name}")));
                count += 1;
            }
            crate::domain::assistant::AssistantEvent::ToolResult {
                tool_name, status, ..
            } => {
                lines.push(Line::from(format!("tool {tool_name} {}", status.title())));
                count += 1;
            }
        }
        if count >= 4 {
            break;
        }
    }
    if count == 0 {
        lines.push(Line::from("No tool events yet."));
    }
    let panel = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Tool History"));
    frame.render_widget(panel, area);
}

fn render_architecture(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let cols = Layout::horizontal([
        Constraint::Length(42),
        Constraint::Length(54),
        Constraint::Min(24),
    ])
    .split(area);
    let posture = Paragraph::new(vec![
        Line::from(format!(
            "current posture: {}",
            app.assistant_posture.title()
        )),
        Line::from(app.assistant_posture.summary()),
        Line::from(""),
        Line::from("controls: p provider  t posture  x probe  j/k provider list  i edit prompt"),
        Line::from(
            "request path: current provider config -> patina-llm gateway -> assistant transcript",
        ),
    ])
    .wrap(Wrap { trim: true })
    .block(theme::panel_block("Architecture Posture"));
    frame.render_widget(posture, cols[0]);

    let roadmap_lines = assistant::roadmap()
        .iter()
        .flat_map(|phase| {
            [
                Line::from(vec![
                    Span::styled(
                        format!("{} ", phase.name),
                        Style::default().fg(theme::ACCENT_COLOR).bold(),
                    ),
                    Span::raw(phase.title),
                ]),
                Line::from(phase.focus),
                Line::from(format!("scope: {}", phase.scope)),
                Line::from(""),
            ]
        })
        .collect::<Vec<_>>();
    let roadmap = Paragraph::new(roadmap_lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Assistant Roadmap"));
    frame.render_widget(roadmap, cols[1]);

    let mut tool_lines = vec![Line::from("Local Tool Actions")];
    tool_lines.extend(
        assistant_tools::tool_list()
            .iter()
            .map(|tool| Line::from(format!("{}  {}", tool.shortcut(), tool.slash_command()))),
    );
    let next = Paragraph::new(tool_lines)
        .wrap(Wrap { trim: true })
        .block(theme::panel_block("Next Slice"));
    frame.render_widget(next, cols[2]);
}
