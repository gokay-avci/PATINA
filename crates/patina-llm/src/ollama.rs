use crate::config::{EndpointAuth, EndpointConfig, InstallLayoutRef, ProviderConfig, ProviderKind};

pub const INSTALL_ROOT: &str = "crates/patina-llm/install/ollama";
pub const NOTES_PATH: &str = "crates/patina-llm/install/ollama/README.md";

pub fn future_provider_config() -> ProviderConfig {
    ProviderConfig {
        id: "ollama-future".to_string(),
        label: "Ollama (future slot)".to_string(),
        kind: ProviderKind::Ollama,
        enabled: false,
        model: "gemma3:4b".to_string(),
        endpoint: EndpointConfig {
            base_url: "http://127.0.0.1:11434".to_string(),
            auth: EndpointAuth::None,
            request_timeout_secs: 120,
        },
        install: InstallLayoutRef {
            root: INSTALL_ROOT.to_string(),
            launch_script: String::new(),
            env_file: String::new(),
            config_example: String::new(),
            notes: NOTES_PATH.to_string(),
        },
    }
}
