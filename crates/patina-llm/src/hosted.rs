use crate::config::{EndpointAuth, EndpointConfig, InstallLayoutRef, ProviderConfig, ProviderKind};

pub const NOTES_PATH: &str = "crates/patina-llm/install/hosted/README.md";

pub fn openai_future_provider_config() -> ProviderConfig {
    ProviderConfig {
        id: "openai-future".to_string(),
        label: "OpenAI (future slot)".to_string(),
        kind: ProviderKind::OpenAi,
        enabled: false,
        model: "gpt-5".to_string(),
        endpoint: EndpointConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            auth: EndpointAuth::BearerEnv {
                env_var: "OPENAI_API_KEY".to_string(),
            },
            request_timeout_secs: 120,
        },
        install: InstallLayoutRef::not_applicable(NOTES_PATH),
    }
}

pub fn anthropic_future_provider_config() -> ProviderConfig {
    ProviderConfig {
        id: "anthropic-future".to_string(),
        label: "Anthropic Claude (future slot)".to_string(),
        kind: ProviderKind::Anthropic,
        enabled: false,
        model: "claude-sonnet-4-5".to_string(),
        endpoint: EndpointConfig {
            base_url: "https://api.anthropic.com".to_string(),
            auth: EndpointAuth::BearerEnv {
                env_var: "ANTHROPIC_API_KEY".to_string(),
            },
            request_timeout_secs: 120,
        },
        install: InstallLayoutRef::not_applicable(NOTES_PATH),
    }
}
