use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolPosture {
    DirectAssistant,
    ToolAwareHost,
    McpAugmentedHost,
}

impl ProtocolPosture {
    pub fn title(self) -> &'static str {
        match self {
            Self::DirectAssistant => "Direct Assistant",
            Self::ToolAwareHost => "Tool-Aware Host",
            Self::McpAugmentedHost => "MCP-Augmented Host",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::DirectAssistant => {
                "Shortest path: patina-tui owns the transcript and calls one model gateway directly."
            }
            Self::ToolAwareHost => {
                "Recommended next step: patina-tui hosts typed PATINA tools beside the model gateway."
            }
            Self::McpAugmentedHost => {
                "Future interoperability layer: bridge selected tools/context through MCP after the native host is stable."
            }
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::DirectAssistant => Self::ToolAwareHost,
            Self::ToolAwareHost => Self::McpAugmentedHost,
            Self::McpAugmentedHost => Self::DirectAssistant,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantPhase {
    pub name: &'static str,
    pub title: &'static str,
    pub focus: &'static str,
    pub scope: &'static str,
}

pub fn roadmap() -> &'static [AssistantPhase] {
    &[
        AssistantPhase {
            name: "Phase 1",
            title: "Run Co-Pilot",
            focus:
                "Read-heavy support for current runs, artifacts, failures, and next-step planning.",
            scope: "Current practical target while PATINA itself is still under construction.",
        },
        AssistantPhase {
            name: "Phase 2",
            title: "Tool-Using Analyst",
            focus:
                "Typed PATINA tools for summaries, comparisons, report drafts, and guarded actions.",
            scope: "Add host-owned tools before trusting free-form model behavior.",
        },
        AssistantPhase {
            name: "Phase 3",
            title: "Experiment Partner",
            focus:
                "Cross-run memory, campaign suggestions, monitoring, and ranked follow-up options.",
            scope: "Useful once more PATINA workflows and artifact seams are stable.",
        },
        AssistantPhase {
            name: "Phase 4",
            title: "Controlled Autonomy",
            focus:
                "Bounded loops with confirmation gates, health watching, and prepared rerun plans.",
            scope: "Only after tooling, provenance, and workflow reliability are mature enough.",
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantSession {
    pub title: String,
    pub messages: Vec<AssistantMessage>,
}

impl AssistantSession {
    pub fn bootstrap() -> Self {
        Self {
            title: "PATINA Assistant Planning Session".to_string(),
            messages: vec![
                AssistantMessage {
                    role: MessageRole::System,
                    body: "This assistant window is the control plane for model routing, future tool permissions, and PATINA-specific workflows.".to_string(),
                    event: None,
                },
                AssistantMessage {
                    role: MessageRole::Assistant,
                    body: "First milestone: choose a provider profile, choose a model preset, and keep request execution behind a typed gateway seam.".to_string(),
                    event: None,
                },
                AssistantMessage {
                    role: MessageRole::Assistant,
                    body: "Recommended architecture: direct model gateway now, tool-aware host next, MCP bridge later.".to_string(),
                    event: None,
                },
            ],
        }
    }

    pub fn push(&mut self, role: MessageRole, body: impl Into<String>) {
        self.messages.push(AssistantMessage {
            role,
            body: body.into(),
            event: None,
        });
    }

    pub fn push_tool_request(&mut self, tool_name: impl Into<String>) {
        self.messages.push(AssistantMessage {
            role: MessageRole::Assistant,
            body: String::new(),
            event: Some(AssistantEvent::ToolRequest {
                tool_name: tool_name.into(),
            }),
        });
    }

    pub fn push_tool_result(
        &mut self,
        tool_name: impl Into<String>,
        status: ToolEventStatus,
        output: impl Into<String>,
    ) {
        self.messages.push(AssistantMessage {
            role: MessageRole::Tool,
            body: String::new(),
            event: Some(AssistantEvent::ToolResult {
                tool_name: tool_name.into(),
                status,
                output: output.into(),
            }),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: MessageRole,
    pub body: String,
    pub event: Option<AssistantEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistantEvent {
    ToolRequest {
        tool_name: String,
    },
    ToolResult {
        tool_name: String,
        status: ToolEventStatus,
        output: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolEventStatus {
    Executed,
    Failed,
}

impl ToolEventStatus {
    pub fn title(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn title(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}
