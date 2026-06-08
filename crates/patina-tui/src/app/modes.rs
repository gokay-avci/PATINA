#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    GenerationInspector,
    ArtifactBrowser,
    Architecture,
    Flow,
    Workbench,
    Launch,
    Ports,
    Assistant,
}

impl Screen {
    pub fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::GenerationInspector,
            Self::GenerationInspector => Self::ArtifactBrowser,
            Self::ArtifactBrowser => Self::Architecture,
            Self::Architecture => Self::Flow,
            Self::Flow => Self::Workbench,
            Self::Workbench => Self::Launch,
            Self::Launch => Self::Ports,
            Self::Ports => Self::Assistant,
            Self::Assistant => Self::Dashboard,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::GenerationInspector => "Generation Inspector",
            Self::ArtifactBrowser => "Artifact Browser",
            Self::Architecture => "Architecture",
            Self::Flow => "Flow",
            Self::Workbench => "Workbench",
            Self::Launch => "Launch",
            Self::Ports => "Ports",
            Self::Assistant => "Assistant",
        }
    }
}
