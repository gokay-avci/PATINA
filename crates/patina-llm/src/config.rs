use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmStackConfig {
    pub default_provider_id: String,
    pub providers: Vec<ProviderConfig>,
}

impl LlmStackConfig {
    pub fn vllm_first() -> Self {
        let provider = crate::vllm::default_provider_config();
        Self {
            default_provider_id: provider.id.clone(),
            providers: vec![
                provider,
                crate::hosted::openai_future_provider_config(),
                crate::hosted::anthropic_future_provider_config(),
                crate::ollama::future_provider_config(),
            ],
        }
    }

    pub fn default_provider(&self) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.id == self.default_provider_id)
    }

    pub fn load_toml(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.display().to_string(),
            source,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    pub enabled: bool,
    pub model: String,
    pub endpoint: EndpointConfig,
    pub install: InstallLayoutRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    Vllm,
    Ollama,
    OpenAi,
    Anthropic,
}

impl ProviderKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Vllm => "vLLM",
            Self::Ollama => "Ollama",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointConfig {
    pub base_url: String,
    pub auth: EndpointAuth,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointAuth {
    None,
    BearerEnv { env_var: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallLayoutRef {
    pub root: String,
    pub launch_script: String,
    pub env_file: String,
    pub config_example: String,
    pub notes: String,
}

impl InstallLayoutRef {
    pub fn not_applicable(notes: impl Into<String>) -> Self {
        Self {
            root: String::new(),
            launch_script: String::new(),
            env_file: String::new(),
            config_example: String::new(),
            notes: notes.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config `{path}`: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config `{path}`: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::LlmStackConfig;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn vllm_first_config_marks_vllm_as_default() {
        let config = LlmStackConfig::vllm_first();
        let provider = config.default_provider().expect("default provider");
        assert_eq!(provider.kind.title(), "vLLM");
        assert!(provider.enabled);
    }

    #[test]
    fn loads_config_from_toml_file() {
        let mut file = NamedTempFile::new().expect("tempfile");
        writeln!(
            file,
            r#"
default_provider_id = "vllm-local"

[[providers]]
id = "vllm-local"
label = "Local vLLM"
kind = "Vllm"
enabled = true
model = "google/gemma-3-4b-it"

[providers.endpoint]
base_url = "http://127.0.0.1:8000"
request_timeout_secs = 120

[providers.endpoint.auth]
BearerEnv = {{ env_var = "PATINA_LLM_VLLM_API_KEY" }}

[providers.install]
root = "crates/patina-llm/install/vllm"
launch_script = "crates/patina-llm/install/vllm/launch_vllm.sh"
env_file = "crates/patina-llm/install/vllm/vllm.env.example"
config_example = "crates/patina-llm/install/vllm/patina-llm.vllm.toml"
notes = "crates/patina-llm/install/vllm/README.md"
"#
        )
        .expect("write config");
        let config = LlmStackConfig::load_toml(file.path()).expect("load config");
        assert_eq!(config.default_provider_id, "vllm-local");
        assert_eq!(config.providers.len(), 1);
    }
}
