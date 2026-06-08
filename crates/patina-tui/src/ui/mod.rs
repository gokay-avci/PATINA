mod assistant;
mod dashboard;
mod flow;
mod inspector;
mod launch;
mod layout;
mod ports;
mod theme;
mod workbench;

use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use patina_llm::provider::{ChatRequest, GatewayError, ModelGateway};
use patina_llm::vllm::VllmGateway;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Terminal;

use crate::app::actions::AppAction;
use crate::app::state::App;
use crate::app::state::ReloadSource;
use crate::data::datasource::RunDataSource;
use crate::domain::assistant_tools;
use crate::domain::workbench as workbench_domain;

pub fn run(app: &mut App, data_source: &dyn RunDataSource) -> Result<()> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;

    let result = run_loop(app, data_source, &mut terminal);

    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to restore cursor")?;

    result
}

fn run_loop(
    app: &mut App,
    data_source: &dyn RunDataSource,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;

        if event::poll(Duration::from_millis(200)).context("event poll failed")? {
            let Event::Key(key) = event::read().context("failed to read terminal event")? else {
                continue;
            };
            match app.handle_key(key) {
                AppAction::None => {}
                AppAction::Reload => {
                    reload(app, data_source, ReloadSource::Manual);
                }
                AppAction::ToggleFollow => {
                    app.toggle_follow_mode();
                }
                AppAction::ToggleHelp => {
                    app.toggle_help();
                }
                AppAction::ExportWorkbenchOutput => {
                    export_workbench_output(app);
                }
                AppAction::ProbeAssistantProvider => {
                    probe_assistant_provider(app);
                }
                AppAction::ExecuteAssistantTool(tool) => {
                    execute_assistant_tool(app, tool);
                }
                AppAction::SendAssistantPrompt => {
                    send_assistant_prompt(app);
                }
                AppAction::Quit => break,
            }
        } else if app.follow_due() {
            reload(app, data_source, ReloadSource::Follow);
        }
    }

    Ok(())
}

fn render(frame: &mut ratatui::Frame<'_>, app: &App) {
    let sections = layout::frame_sections(frame.area());
    theme::render_header(frame, sections.header, app);
    theme::render_footer(frame, sections.footer, app);
    theme::render_sidebar(frame, sections.sidebar, app);
    match app.screen {
        crate::app::modes::Screen::Dashboard => dashboard::render(frame, sections.main, app),
        crate::app::modes::Screen::GenerationInspector => {
            inspector::render_generation(frame, sections.main, app)
        }
        crate::app::modes::Screen::ArtifactBrowser => {
            inspector::render_artifacts(frame, sections.main, app)
        }
        crate::app::modes::Screen::Architecture => {
            inspector::render_architecture(frame, sections.main, app)
        }
        crate::app::modes::Screen::Flow => flow::render(frame, sections.main, app),
        crate::app::modes::Screen::Workbench => workbench::render(frame, sections.main, app),
        crate::app::modes::Screen::Launch => launch::render(frame, sections.main, app),
        crate::app::modes::Screen::Ports => ports::render(frame, sections.main, app),
        crate::app::modes::Screen::Assistant => assistant::render(frame, sections.main, app),
    }
    if app.show_help {
        render_help_overlay(frame, app);
    }
}

fn reload(app: &mut App, data_source: &dyn RunDataSource, source: ReloadSource) {
    let Some(run_dir) = app.run_dir.as_ref() else {
        app.status = "No run directory attached; restart with RUN_DIR to enable reload".to_string();
        return;
    };
    match data_source.load_run(run_dir) {
        Ok(snapshot) => app.replace_snapshot(snapshot, source),
        Err(error) => app.set_reload_error(source, &error),
    }
}

fn export_workbench_output(app: &mut App) {
    let registry = workbench_domain::registry();
    let Some(operation) = registry.get(app.selected_artifact) else {
        app.status = "No workbench operation selected".to_string();
        return;
    };
    let generation = app.selected_generation_number();
    match workbench_domain::export_operation(
        operation.kind,
        &app.snapshot,
        generation,
        app.selected_member,
    ) {
        Ok(path) => {
            app.status = format!("Exported workbench output to {}", path.display());
        }
        Err(error) => {
            app.status = format!("Workbench export failed: {error}");
        }
    }
}

fn render_help_overlay(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = centered_rect(frame.area(), 72, 70);
    frame.render_widget(Clear, area);
    let text = vec![
        ratatui::text::Line::from("PATINA-TUI Help"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from("1 dashboard  2 generation  3 artifacts"),
        ratatui::text::Line::from("4 architecture  5 flow  6 workbench  7 launch  8 ports  9 assistant"),
        ratatui::text::Line::from("j/k move in the current list"),
        ratatui::text::Line::from("h/l change selected member context"),
        ratatui::text::Line::from("r reload now"),
        ratatui::text::Line::from("f toggle follow mode"),
        ratatui::text::Line::from("Flow keys: u/c/d/n choose pane  m cycle pane  Enter open pane workspace"),
        ratatui::text::Line::from("7 launch: j/k workflow  h/l draft field  i or Enter edit field  Esc stop editing"),
        ratatui::text::Line::from("8 ports: inspect use-case -> port -> adapter -> artifact seams"),
        ratatui::text::Line::from("Ports keys: a relevant/all  v detail mode  x seam probe  g launch prefill  Enter open workspace"),
        ratatui::text::Line::from("9 assistant: p cycle provider  x probe provider  Shift+H/R/S/A/C/F/N run local tools  i or Enter edit prompt  Enter send  Esc exit input"),
        ratatui::text::Line::from("? or Esc close this help"),
        ratatui::text::Line::from("q quit"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from("Workbench"),
        ratatui::text::Line::from("e export the current workbench output to outputs/workbench/"),
        ratatui::text::Line::from("Topology Metrics: fast structural summary"),
        ratatui::text::Line::from("Dreadnaut Graph Export: KLMC3-compatible graph payload"),
        ratatui::text::Line::from("Canonical Hashkey: wrapper-based native identity probe"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(format!("Current screen: {}", app.screen.title())),
    ];
    let widget = Paragraph::new(text).block(theme::panel_block("Help"));
    frame.render_widget(widget, area);
}

fn send_assistant_prompt(app: &mut App) {
    let Some(prompt) = app.take_assistant_prompt() else {
        app.status = "Assistant prompt is empty".to_string();
        return;
    };
    if let Some(output) = assistant_tools::try_execute(&prompt, app) {
        app.push_assistant_message(crate::domain::assistant::MessageRole::User, prompt);
        app.assistant_session.push_tool_result(
            "direct-local-command",
            crate::domain::assistant::ToolEventStatus::Executed,
            output,
        );
        app.status = "Assistant local tool executed".to_string();
        return;
    }
    let provider = app.selected_assistant_provider().clone();
    app.push_assistant_message(crate::domain::assistant::MessageRole::User, prompt.clone());
    app.status = format!(
        "Sending assistant request through {} ({})",
        provider.label,
        provider.kind.title()
    );
    let run_context = app.assistant_run_context();
    let request = ChatRequest {
        system_prompt: Some(
            format!(
                "You are the PATINA assistant. Give concise, technically useful responses for this scientific workflow workspace. The platform is still under construction, so do not assume every pathway is stable. {tool_catalog}. {tool_protocol} Prefer those local tools when the user asks for run facts, selection facts, artifact facts, comparisons, failure summaries, or rerun suggestions. Current PATINA run context: {run_context}",
                tool_catalog = assistant_tools::tool_catalog(),
                tool_protocol = assistant_tools::tool_protocol(),
            ),
        ),
        user_prompt: prompt,
        temperature_milli: 200,
        max_output_tokens: 800,
    };
    let result = match provider.kind {
        patina_llm::config::ProviderKind::Vllm => {
            let gateway = VllmGateway::new(provider.clone());
            gateway.chat(&request)
        }
        _ => Err(GatewayError::Transport {
            provider_id: provider.id.clone(),
            message: "provider adapter not implemented yet in patina-tui".to_string(),
        }),
    };
    match result {
        Ok(response) => {
            let final_text = resolve_assistant_response(app, &provider, response.output_text);
            app.push_assistant_message(
                crate::domain::assistant::MessageRole::Assistant,
                final_text,
            );
            app.status = format!("Assistant response received from {}", provider.label);
        }
        Err(error) => {
            app.push_assistant_message(
                crate::domain::assistant::MessageRole::Assistant,
                format!("Request failed: {error}"),
            );
            app.status = format!("Assistant request failed: {error}");
        }
    }
}

fn resolve_assistant_response(
    app: &mut App,
    provider: &patina_llm::config::ProviderConfig,
    model_output: String,
) -> String {
    let Some(tool) = assistant_tools::parse_tool_request(&model_output) else {
        return assistant_tools::strip_answer_prefix(&model_output).to_string();
    };
    let tool_result = assistant_tools::execute(tool, app);
    app.assistant_session
        .push_tool_request(tool.slash_command());
    app.assistant_session.push_tool_result(
        tool.slash_command(),
        crate::domain::assistant::ToolEventStatus::Executed,
        tool_result.clone(),
    );
    let follow_up = ChatRequest {
        system_prompt: Some(
            "You are the PATINA assistant. The host has already executed one local PATINA tool for you. Produce the final answer only. Reply with `ANSWER:` followed by a concise grounded response."
                .to_string(),
        ),
        user_prompt: format!(
            "Original user request:\n{}\n\nExecuted tool:\n{}\n\nTool result:\n{}\n\nNow produce the final grounded answer.",
            app.assistant_session
                .messages
                .iter()
                .rev()
                .find(|message| matches!(message.role, crate::domain::assistant::MessageRole::User))
                .map(|message| message.body.as_str())
                .unwrap_or("n/a"),
            tool.slash_command(),
            tool_result
        ),
        temperature_milli: 100,
        max_output_tokens: 800,
    };
    let final_result = match provider.kind {
        patina_llm::config::ProviderKind::Vllm => {
            let gateway = VllmGateway::new(provider.clone());
            gateway.chat(&follow_up)
        }
        _ => Err(GatewayError::Transport {
            provider_id: provider.id.clone(),
            message: "provider adapter not implemented yet in patina-tui".to_string(),
        }),
    };
    match final_result {
        Ok(response) => format!(
            "{}\n\n[tool {} executed by host]",
            assistant_tools::strip_answer_prefix(&response.output_text),
            tool.slash_command()
        ),
        Err(error) => format!(
            "{}\n\n[tool {} executed by host]\n[final model synthesis failed: {}]",
            tool_result,
            tool.slash_command(),
            error
        ),
    }
}

fn execute_assistant_tool(app: &mut App, tool: assistant_tools::AssistantTool) {
    let output = assistant_tools::execute(tool, app);
    app.push_assistant_message(
        crate::domain::assistant::MessageRole::User,
        format!("tool {}", tool.slash_command()),
    );
    app.assistant_session.push_tool_result(
        tool.slash_command(),
        crate::domain::assistant::ToolEventStatus::Executed,
        output,
    );
    app.status = format!("Assistant local tool executed: {}", tool.title());
}

fn probe_assistant_provider(app: &mut App) {
    let provider = app.selected_assistant_provider().clone();
    let result = match provider.kind {
        patina_llm::config::ProviderKind::Vllm => {
            let gateway = VllmGateway::new(provider.clone());
            gateway.probe()
        }
        _ => Err(GatewayError::Transport {
            provider_id: provider.id.clone(),
            message: "provider probe not implemented yet in patina-tui".to_string(),
        }),
    };
    match result {
        Ok(report) => {
            let preview = report
                .model_ids
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            app.assistant_probe_status = if preview.is_empty() {
                format!("reachable; {} models listed", report.model_count)
            } else {
                format!(
                    "reachable; {} models listed; sample: {}",
                    report.model_count, preview
                )
            };
            app.status = format!("Assistant provider probe passed for {}", provider.label);
        }
        Err(error) => {
            app.assistant_probe_status = format!("probe failed: {error}");
            app.status = format!("Assistant provider probe failed: {error}");
        }
    }
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .flex(Flex::Center)
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .flex(Flex::Center)
    .split(vertical[1])[1]
}
