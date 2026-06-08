use crate::domain::assistant_tools::AssistantTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    None,
    Reload,
    ToggleFollow,
    ToggleHelp,
    ExportWorkbenchOutput,
    ProbeAssistantProvider,
    ExecuteAssistantTool(AssistantTool),
    SendAssistantPrompt,
    Quit,
}
