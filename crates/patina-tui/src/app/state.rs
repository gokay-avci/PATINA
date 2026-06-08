use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use patina_llm::config::{EndpointAuth, LlmStackConfig, ProviderConfig};

use crate::app::actions::AppAction;
use crate::app::modes::Screen;
use crate::domain::artifacts::RunSnapshot;
use crate::domain::assistant;
use crate::domain::assistant_tools::AssistantTool;
use crate::domain::launch;
use crate::domain::launch_draft::LaunchDraft;
use crate::domain::ports;
use crate::domain::workbench;

#[derive(Debug)]
pub struct App {
    pub run_dir: Option<PathBuf>,
    pub snapshot: RunSnapshot,
    pub screen: Screen,
    pub selected_generation: usize,
    pub selected_member: usize,
    pub selected_artifact: usize,
    pub launch_selected_field: usize,
    pub launch_draft: LaunchDraft,
    pub launch_input_mode: bool,
    pub flow_focus: FlowPane,
    pub ports_only_relevant: bool,
    pub ports_detail_mode: ports::PortsDetailMode,
    pub ports_last_probe: Option<ports::PortProbeReport>,
    pub show_help: bool,
    pub follow_mode: bool,
    pub follow_interval: Duration,
    pub last_refresh_at: Instant,
    pub status: String,
    pub llm_config_path: PathBuf,
    pub llm_stack: LlmStackConfig,
    pub assistant_provider: usize,
    pub assistant_posture: assistant::ProtocolPosture,
    pub assistant_session: assistant::AssistantSession,
    pub assistant_input: String,
    pub assistant_input_mode: bool,
    pub assistant_probe_status: String,
}

impl App {
    pub fn new(
        run_dir: Option<PathBuf>,
        snapshot: RunSnapshot,
        follow_mode: bool,
        llm_stack: LlmStackConfig,
        llm_config_path: PathBuf,
    ) -> Self {
        let selected_generation = snapshot.latest_generation_index().unwrap_or(0);
        let has_loaded_run = snapshot.has_loaded_run();
        let follow_mode = follow_mode && has_loaded_run;
        let initial_workflow = launch::registry()
            .first()
            .expect("workflow registry must not be empty");
        let launch_draft = LaunchDraft::for_workflow(initial_workflow, &snapshot);
        Self {
            run_dir,
            snapshot,
            screen: if has_loaded_run {
                Screen::Dashboard
            } else {
                Screen::Launch
            },
            selected_generation,
            selected_member: 0,
            selected_artifact: 0,
            launch_selected_field: 0,
            launch_draft,
            launch_input_mode: false,
            flow_focus: FlowPane::Upstream,
            ports_only_relevant: false,
            ports_detail_mode: ports::PortsDetailMode::Contract,
            ports_last_probe: None,
            show_help: false,
            follow_mode,
            follow_interval: Duration::from_secs(2),
            last_refresh_at: Instant::now(),
            status: if has_loaded_run {
                if follow_mode {
                    "Loaded run artifacts; follow mode enabled".to_string()
                } else {
                    "Loaded run artifacts".to_string()
                }
            } else {
                "Workspace mode: no run loaded; start from Launch or pass RUN_DIR".to_string()
            },
            llm_config_path,
            llm_stack,
            assistant_provider: 0,
            assistant_posture: assistant::ProtocolPosture::DirectAssistant,
            assistant_session: assistant::AssistantSession::bootstrap(),
            assistant_input: String::new(),
            assistant_input_mode: false,
            assistant_probe_status: "Not probed yet".to_string(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if self.show_help {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('?') => AppAction::ToggleHelp,
                KeyCode::Char('q') => AppAction::Quit,
                _ => AppAction::None,
            };
        }
        if self.assistant_input_mode && matches!(self.screen, Screen::Assistant) {
            return self.handle_assistant_input_key(key);
        }
        if self.launch_input_mode && matches!(self.screen, Screen::Launch) {
            return self.handle_launch_input_key(key);
        }
        match key.code {
            KeyCode::Char('q') => AppAction::Quit,
            KeyCode::Char('r') => AppAction::Reload,
            KeyCode::Char('f') => AppAction::ToggleFollow,
            KeyCode::Char('?') => AppAction::ToggleHelp,
            KeyCode::Char('e') if matches!(self.screen, Screen::Workbench) => {
                AppAction::ExportWorkbenchOutput
            }
            KeyCode::Tab => {
                self.screen = self.screen.next();
                AppAction::None
            }
            KeyCode::Char('1') => {
                self.screen = Screen::Dashboard;
                AppAction::None
            }
            KeyCode::Char('2') => {
                self.screen = Screen::GenerationInspector;
                AppAction::None
            }
            KeyCode::Char('3') => {
                self.screen = Screen::ArtifactBrowser;
                AppAction::None
            }
            KeyCode::Char('4') => {
                self.screen = Screen::Architecture;
                AppAction::None
            }
            KeyCode::Char('5') => {
                self.screen = Screen::Flow;
                AppAction::None
            }
            KeyCode::Char('6') => {
                self.screen = Screen::Workbench;
                AppAction::None
            }
            KeyCode::Char('7') => {
                self.screen = Screen::Launch;
                AppAction::None
            }
            KeyCode::Char('8') => {
                self.screen = Screen::Ports;
                AppAction::None
            }
            KeyCode::Char('9') => {
                self.screen = Screen::Assistant;
                AppAction::None
            }
            KeyCode::Char('m') if matches!(self.screen, Screen::Flow) => {
                self.cycle_flow_focus();
                AppAction::None
            }
            KeyCode::Char('p') if matches!(self.screen, Screen::Assistant) => {
                self.cycle_assistant_provider();
                AppAction::None
            }
            KeyCode::Char('t') if matches!(self.screen, Screen::Assistant) => {
                self.cycle_assistant_posture();
                AppAction::None
            }
            KeyCode::Char('i') if matches!(self.screen, Screen::Assistant) => {
                self.toggle_assistant_input_mode();
                AppAction::None
            }
            KeyCode::Char('i') if matches!(self.screen, Screen::Launch) => {
                self.toggle_launch_input_mode();
                AppAction::None
            }
            KeyCode::Char('H') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Help)
            }
            KeyCode::Char('R') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Run)
            }
            KeyCode::Char('S') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Selection)
            }
            KeyCode::Char('A') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Artifacts)
            }
            KeyCode::Char('C') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Compare)
            }
            KeyCode::Char('F') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Failures)
            }
            KeyCode::Char('N') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ExecuteAssistantTool(AssistantTool::Suggest)
            }
            KeyCode::Char('x') if matches!(self.screen, Screen::Assistant) => {
                AppAction::ProbeAssistantProvider
            }
            KeyCode::Char('u') if matches!(self.screen, Screen::Flow) => {
                self.set_flow_focus(FlowPane::Upstream);
                AppAction::None
            }
            KeyCode::Char('c') if matches!(self.screen, Screen::Flow) => {
                self.set_flow_focus(FlowPane::Current);
                AppAction::None
            }
            KeyCode::Char('d') if matches!(self.screen, Screen::Flow) => {
                self.set_flow_focus(FlowPane::Downstream);
                AppAction::None
            }
            KeyCode::Char('n') if matches!(self.screen, Screen::Flow) => {
                self.set_flow_focus(FlowPane::Actions);
                AppAction::None
            }
            KeyCode::Char('a') if matches!(self.screen, Screen::Ports) => {
                self.toggle_ports_relevance();
                AppAction::None
            }
            KeyCode::Char('g') if matches!(self.screen, Screen::Ports) => {
                self.open_launch_from_ports();
                AppAction::None
            }
            KeyCode::Char('v') if matches!(self.screen, Screen::Ports) => {
                self.cycle_ports_detail_mode();
                AppAction::None
            }
            KeyCode::Char('x') if matches!(self.screen, Screen::Ports) => {
                self.run_ports_probe();
                AppAction::None
            }
            KeyCode::Enter if matches!(self.screen, Screen::Flow) => {
                self.open_flow_workspace();
                AppAction::None
            }
            KeyCode::Enter if matches!(self.screen, Screen::Ports) => {
                self.open_ports_workspace();
                AppAction::None
            }
            KeyCode::Enter if matches!(self.screen, Screen::Assistant) => {
                self.toggle_assistant_input_mode();
                AppAction::None
            }
            KeyCode::Enter if matches!(self.screen, Screen::Launch) => {
                self.toggle_launch_input_mode();
                AppAction::None
            }
            KeyCode::Left | KeyCode::Char('h') if matches!(self.screen, Screen::Launch) => {
                self.move_launch_field(-1);
                AppAction::None
            }
            KeyCode::Right | KeyCode::Char('l') if matches!(self.screen, Screen::Launch) => {
                self.move_launch_field(1);
                AppAction::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.move_member_selection(-1);
                AppAction::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.move_member_selection(1);
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    pub fn replace_snapshot(&mut self, snapshot: RunSnapshot, source: ReloadSource) {
        let previous_fingerprint = self.snapshot.fingerprint();
        let next_fingerprint = snapshot.fingerprint();
        self.snapshot = snapshot;
        self.sync_launch_draft();
        if !self.snapshot.has_loaded_run() {
            self.follow_mode = false;
        }
        let max_generation = self.snapshot.latest_generation_index().unwrap_or(0);
        self.selected_generation = self.selected_generation.min(max_generation);
        let max_member = self.selected_member_max();
        self.selected_member = self.selected_member.min(max_member);
        let max_artifact = match self.screen {
            Screen::Architecture => self.snapshot.manifest.artifacts.len().saturating_sub(1),
            Screen::Flow => self.flow_stages().len().saturating_sub(1),
            Screen::Workbench => workbench::registry().len().saturating_sub(1),
            Screen::Launch => launch::registry().len().saturating_sub(1),
            Screen::Ports => self.ports_items().len().saturating_sub(1),
            Screen::Assistant => self.assistant_providers().len().saturating_sub(1),
            _ => self.snapshot.all_artifact_paths().len().saturating_sub(1),
        };
        self.selected_artifact = self.selected_artifact.min(max_artifact);
        self.last_refresh_at = Instant::now();
        self.ports_last_probe = None;
        self.status = match (source, previous_fingerprint == next_fingerprint) {
            (ReloadSource::Manual, _) => "Reloaded run artifacts".to_string(),
            (ReloadSource::Follow, true) => "Follow refresh: no artifact changes".to_string(),
            (ReloadSource::Follow, false) => {
                "Follow refresh: artifact changes detected".to_string()
            }
        };
    }

    pub fn set_reload_error(&mut self, source: ReloadSource, error: &anyhow::Error) {
        self.last_refresh_at = Instant::now();
        self.status = match source {
            ReloadSource::Manual => format!("Reload failed: {error}"),
            ReloadSource::Follow => format!("Follow refresh failed: {error}"),
        };
    }

    pub fn toggle_follow_mode(&mut self) {
        if self.run_dir.is_none() {
            self.follow_mode = false;
            self.status = "Follow mode requires a loaded run directory".to_string();
            return;
        }
        self.follow_mode = !self.follow_mode;
        self.status = if self.follow_mode {
            "Follow mode enabled".to_string()
        } else {
            "Follow mode disabled".to_string()
        };
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        self.status = if self.show_help {
            "Help overlay opened".to_string()
        } else {
            "Help overlay closed".to_string()
        };
    }

    pub fn follow_due(&self) -> bool {
        self.follow_mode && self.last_refresh_at.elapsed() >= self.follow_interval
    }

    pub fn architecture_items(&self) -> Vec<(String, String)> {
        let mut entries = self
            .snapshot
            .manifest
            .artifacts
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    pub fn flow_stages(&self) -> Vec<FlowStage> {
        let mut stages = vec![
            FlowStage {
                name: "Input Contract",
                purpose: "Run manifest, search config, and selected structure context enter the workflow here.",
                upstream: "Run directory, manifest.json, generation state, selected member",
                current: "Resolve workflow owner, system, artifact contract, and candidate context",
                downstream: "Evaluation adapter choice, topology tools, artifact browsing, launch planning",
            },
            FlowStage {
                name: "Evaluation Adapter",
                purpose: "Concrete backend/runtime binding for the scientific execution seam.",
                upstream: "Workflow owner, backend label, lane mode, parallel contract",
                current: "Interpret manifest backend metadata and runtime contract",
                downstream: "Generation metrics, controller traces, output structures, checkpoints",
            },
            FlowStage {
                name: "Analysis & Diagnostics",
                purpose: "Fast read-side summaries, topology probes, duplicate pressure, and structure inspection.",
                upstream: "Generation summaries, controller rows, selected structure",
                current: "Inspect outcomes and compute additional metrics in the workbench",
                downstream: "Troubleshooting, comparison, export, policy tuning",
            },
            FlowStage {
                name: "Persistence & Export",
                purpose: "Stable artifact seam for downstream tools and future adapters.",
                upstream: "Raw artifacts, traces, outputs, manifest contract",
                current: "Expose filesystem contract and native-facing exports",
                downstream: "Tauri UI, analysis scripts, reproducibility ledger, future cluster tooling",
            },
        ];
        if self.snapshot.manifest.parallel_contract.is_some() {
            stages.insert(
                2,
                FlowStage {
                    name: "Runtime Dispatch",
                    purpose: "Worker or generation dispatch boundary between orchestration and backend execution.",
                    upstream: "Parallel contract, backend policy, selected workflow",
                    current: "Describe how work fans out and where responses fold back",
                    downstream: "Responses, controller trace, generation summaries",
                },
            );
        }
        stages
    }

    pub fn ports_items(&self) -> Vec<ports::PortBinding> {
        let all = ports::registry().to_vec();
        if !self.ports_only_relevant {
            return all;
        }
        let filtered = all
            .iter()
            .copied()
            .filter(|binding| ports::matches_current_run(binding, &self.snapshot))
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            all
        } else {
            filtered
        }
    }

    pub fn selected_port_binding(&self) -> Option<ports::PortBinding> {
        self.ports_items().get(self.selected_artifact).copied()
    }

    pub fn selected_member_label(&self) -> String {
        self.selected_member_record()
            .map(|member| member.evaluation.structure.label.clone())
            .unwrap_or_else(|| "n/a".to_string())
    }

    pub fn selected_member_record(&self) -> Option<&crate::domain::ga::GenerationMemberFile> {
        let generation = self.selected_generation_number();
        self.snapshot
            .generation_state(generation)
            .and_then(|state| state.population.get(self.selected_member))
    }

    fn selected_member_max(&self) -> usize {
        self.snapshot
            .generation_state(self.selected_generation_number())
            .map(|state| state.population.len().saturating_sub(1))
            .unwrap_or(0)
    }

    pub fn selected_generation_number(&self) -> usize {
        self.snapshot
            .selected_metric(self.selected_generation)
            .map(|row| row.generation)
            .unwrap_or(self.selected_generation)
    }

    pub fn assistant_providers(&self) -> &[ProviderConfig] {
        &self.llm_stack.providers
    }

    pub fn selected_assistant_provider(&self) -> &ProviderConfig {
        self.assistant_providers()
            .get(self.assistant_provider)
            .unwrap_or_else(|| &self.assistant_providers()[0])
    }

    pub fn assistant_endpoint_auth_label(&self) -> String {
        match &self.selected_assistant_provider().endpoint.auth {
            EndpointAuth::None => "none".to_string(),
            EndpointAuth::BearerEnv { env_var } => format!("bearer from {env_var}"),
        }
    }

    pub fn assistant_run_context(&self) -> String {
        let workflow = match self.snapshot.workflow_kind {
            crate::domain::artifacts::WorkflowKind::Ga => "GA",
            crate::domain::artifacts::WorkflowKind::Bh => "BH",
            crate::domain::artifacts::WorkflowKind::Unknown => "Unknown",
        };
        let latest_generation = self
            .snapshot
            .latest_generation_index()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_string());
        format!(
            "run_dir={} workflow={} run_name={} backend={} owner={} selected_generation={} latest_generation={} metric_rows={} walker_rows={} current_screen={} selected_member={}",
            self.snapshot.run_dir.display(),
            workflow,
            self.snapshot.manifest.run_name.as_deref().unwrap_or("unnamed-run"),
            self.snapshot.manifest.backend.as_deref().unwrap_or("n/a"),
            self.snapshot.manifest.workflow_owner.as_deref().unwrap_or("n/a"),
            self.selected_generation_number(),
            latest_generation,
            self.snapshot.generation_metrics.len(),
            self.snapshot.walker_trace.len(),
            self.screen.title(),
            self.selected_member,
        )
    }

    fn move_selection(&mut self, delta: isize) {
        match self.screen {
            Screen::Dashboard | Screen::GenerationInspector => {
                let len = self.snapshot.generation_metrics.len();
                if len == 0 {
                    return;
                }
                let current = self.selected_generation.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_generation = next;
            }
            Screen::ArtifactBrowser => {
                let len = self.snapshot.all_artifact_paths().len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
                self.sync_launch_draft();
                if let Some(workflow) = launch::registry().get(self.selected_artifact) {
                    self.status = format!("Launch workflow: {}", workflow.name);
                }
            }
            Screen::Architecture => {
                let len = self.snapshot.manifest.artifacts.len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
            }
            Screen::Flow => {
                let len = self.flow_stages().len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
            }
            Screen::Workbench => {
                let len = workbench::registry().len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
            }
            Screen::Launch => {
                let len = launch::registry().len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
            }
            Screen::Ports => {
                let len = self.ports_items().len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
            }
            Screen::Assistant => {
                let len = self.assistant_providers().len();
                if len == 0 {
                    return;
                }
                let current = self.selected_artifact.min(len.saturating_sub(1)) as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
                self.selected_artifact = next;
                self.assistant_provider = self.selected_artifact;
                let provider = self.selected_assistant_provider();
                self.status = format!(
                    "Assistant provider: {} ({})",
                    provider.label,
                    provider.kind.title()
                );
            }
        }
    }

    fn move_member_selection(&mut self, delta: isize) {
        if !matches!(self.screen, Screen::GenerationInspector | Screen::Workbench) {
            return;
        }
        let max_member = self.selected_member_max();
        let current = self.selected_member.min(max_member) as isize;
        self.selected_member = (current + delta).clamp(0, max_member as isize) as usize;
        self.status = format!(
            "Selected member {} for generation view/workbench",
            self.selected_member
        );
    }

    fn toggle_ports_relevance(&mut self) {
        self.ports_only_relevant = !self.ports_only_relevant;
        let max_index = self.ports_items().len().saturating_sub(1);
        self.selected_artifact = self.selected_artifact.min(max_index);
        self.status = if self.ports_only_relevant {
            "Ports view: showing seams relevant to the current run".to_string()
        } else {
            "Ports view: showing all seams".to_string()
        };
    }

    fn cycle_ports_detail_mode(&mut self) {
        self.ports_detail_mode = self.ports_detail_mode.next();
        self.status = format!("Ports detail mode: {}", self.ports_detail_mode.title());
    }

    fn run_ports_probe(&mut self) {
        let Some(binding) = self.selected_port_binding() else {
            self.status = "No port seam selected".to_string();
            return;
        };
        let report = ports::probe_binding(binding, &self.snapshot);
        let (ok, fail) = report.totals();
        self.status = format!("Ports probe `{}`: {ok} passed, {fail} failed", binding.name);
        self.ports_last_probe = Some(report);
    }

    fn open_ports_workspace(&mut self) {
        let Some(binding) = self.selected_port_binding() else {
            self.status = "No port seam selected".to_string();
            return;
        };
        let workspace = ports::recommended_workspace(binding);
        let mut prefilled_route: Option<&'static str> = None;
        self.screen = match workspace {
            "launch" => {
                prefilled_route = self.prefill_launch_from_binding(binding);
                Screen::Launch
            }
            "flow" => Screen::Flow,
            "workbench" => Screen::Workbench,
            "artifacts" => Screen::ArtifactBrowser,
            _ => Screen::Architecture,
        };
        self.status = if let Some(route) = prefilled_route {
            format!(
                "Opened `{workspace}` workspace from `{}` seam (prefilled route `{route}`)",
                binding.name
            )
        } else {
            format!(
                "Opened `{workspace}` workspace from `{}` seam",
                binding.name
            )
        };
    }

    fn open_launch_from_ports(&mut self) {
        let Some(binding) = self.selected_port_binding() else {
            self.status = "No port seam selected".to_string();
            return;
        };
        let route = self.prefill_launch_from_binding(binding);
        self.screen = Screen::Launch;
        self.status = if let Some(route) = route {
            format!(
                "Opened launch workspace from `{}` seam (prefilled route `{route}`)",
                binding.name
            )
        } else {
            format!("Opened launch workspace from `{}` seam", binding.name)
        };
    }

    fn prefill_launch_from_binding(&mut self, binding: ports::PortBinding) -> Option<&'static str> {
        for route in binding.workflow_routes {
            if let Some(index) = launch::index_for_route(route) {
                self.selected_artifact = index;
                self.sync_launch_draft();
                return Some(route);
            }
        }
        self.selected_artifact = 0;
        self.sync_launch_draft();
        None
    }

    fn cycle_flow_focus(&mut self) {
        self.flow_focus = self.flow_focus.next();
        self.status = format!("Flow focus pane: {}", self.flow_focus.title());
    }

    fn set_flow_focus(&mut self, focus: FlowPane) {
        self.flow_focus = focus;
        self.status = format!("Flow focus pane: {}", self.flow_focus.title());
    }

    fn open_flow_workspace(&mut self) {
        self.screen = match self.flow_focus {
            FlowPane::Upstream => Screen::Architecture,
            FlowPane::Current => Screen::Ports,
            FlowPane::Downstream => Screen::ArtifactBrowser,
            FlowPane::Actions => Screen::Launch,
        };
        self.status = format!(
            "Opened workspace linked to flow pane `{}`",
            self.flow_focus.title()
        );
    }

    fn cycle_assistant_provider(&mut self) {
        let profiles = self.assistant_providers();
        if profiles.is_empty() {
            self.status = "No assistant provider profiles registered".to_string();
            return;
        }
        self.assistant_provider = (self.assistant_provider + 1) % profiles.len();
        self.selected_artifact = self.assistant_provider;
        let profile = self.selected_assistant_provider();
        self.status = format!(
            "Assistant provider: {} ({}) with model {}",
            profile.label,
            profile.kind.title(),
            profile.model
        );
    }

    fn cycle_assistant_posture(&mut self) {
        self.assistant_posture = self.assistant_posture.next();
        self.status = format!(
            "Assistant architecture posture: {}",
            self.assistant_posture.title()
        );
    }

    fn toggle_assistant_input_mode(&mut self) {
        self.assistant_input_mode = !self.assistant_input_mode;
        self.status = if self.assistant_input_mode {
            "Assistant input mode enabled".to_string()
        } else {
            "Assistant input mode disabled".to_string()
        };
    }

    fn handle_assistant_input_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc => {
                self.assistant_input_mode = false;
                self.status = "Assistant input mode disabled".to_string();
                AppAction::None
            }
            KeyCode::Enter => {
                if self.assistant_input.trim().is_empty() {
                    self.status = "Assistant prompt is empty".to_string();
                    AppAction::None
                } else {
                    AppAction::SendAssistantPrompt
                }
            }
            KeyCode::Backspace => {
                self.assistant_input.pop();
                AppAction::None
            }
            KeyCode::Char(ch) => {
                self.assistant_input.push(ch);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_launch_input_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.launch_input_mode = false;
                self.status = "Launch field editing disabled".to_string();
                AppAction::None
            }
            KeyCode::Backspace => {
                self.launch_draft.pop_char(self.launch_selected_field);
                AppAction::None
            }
            KeyCode::Char(ch) => {
                self.launch_draft.push_char(self.launch_selected_field, ch);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn move_launch_field(&mut self, delta: isize) {
        let len = self.launch_draft.field_count();
        if len == 0 {
            return;
        }
        let current = self.launch_selected_field.min(len.saturating_sub(1)) as isize;
        let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
        self.launch_selected_field = next;
        if let Some(field) = self.launch_draft.selected_field(self.launch_selected_field) {
            self.status = format!("Launch field: --{}", field.key);
        }
    }

    fn toggle_launch_input_mode(&mut self) {
        if self.launch_draft.field_count() == 0 {
            self.status = "No launch draft field selected".to_string();
            return;
        }
        self.launch_input_mode = !self.launch_input_mode;
        self.status = if self.launch_input_mode {
            let field = self
                .launch_draft
                .selected_field(self.launch_selected_field)
                .map(|field| field.key.as_str())
                .unwrap_or("unknown");
            format!("Editing launch field --{field}")
        } else {
            "Launch field editing disabled".to_string()
        };
    }

    fn sync_launch_draft(&mut self) {
        let workflow = launch::registry()
            .get(self.selected_artifact)
            .unwrap_or_else(|| &launch::registry()[0]);
        self.launch_draft = LaunchDraft::for_workflow(workflow, &self.snapshot);
        let max_field = self.launch_draft.field_count().saturating_sub(1);
        self.launch_selected_field = self.launch_selected_field.min(max_field);
        self.launch_input_mode = false;
    }

    pub fn take_assistant_prompt(&mut self) -> Option<String> {
        let prompt = self.assistant_input.trim().to_string();
        if prompt.is_empty() {
            return None;
        }
        self.assistant_input.clear();
        self.assistant_input_mode = false;
        Some(prompt)
    }

    pub fn push_assistant_message(
        &mut self,
        role: assistant::MessageRole,
        body: impl Into<String>,
    ) {
        self.assistant_session.push(role, body);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadSource {
    Manual,
    Follow,
}

#[derive(Debug, Clone, Copy)]
pub struct FlowStage {
    pub name: &'static str,
    pub purpose: &'static str,
    pub upstream: &'static str,
    pub current: &'static str,
    pub downstream: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowPane {
    Upstream,
    Current,
    Downstream,
    Actions,
}

impl FlowPane {
    pub fn title(self) -> &'static str {
        match self {
            Self::Upstream => "Upstream",
            Self::Current => "Current",
            Self::Downstream => "Downstream",
            Self::Actions => "Actions",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Upstream => Self::Current,
            Self::Current => Self::Downstream,
            Self::Downstream => Self::Actions,
            Self::Actions => Self::Upstream,
        }
    }
}
