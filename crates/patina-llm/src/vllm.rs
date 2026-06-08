use crate::config::{EndpointAuth, EndpointConfig, InstallLayoutRef, ProviderConfig, ProviderKind};
use crate::provider::{
    validate_provider_config, ChatRequest, ChatResponse, GatewayError, ModelGateway, ProbeResult,
    ProviderMetadata,
};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

pub const INSTALL_ROOT: &str = "crates/patina-llm/install/vllm";
pub const LAUNCH_SCRIPT: &str = "crates/patina-llm/install/vllm/launch_vllm.sh";
pub const ENV_FILE: &str = "crates/patina-llm/install/vllm/vllm.env.example";
pub const CONFIG_EXAMPLE: &str = "crates/patina-llm/install/vllm/patina-llm.vllm.toml";
pub const NOTES_PATH: &str = "crates/patina-llm/install/vllm/README.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VllmGateway {
    config: ProviderConfig,
}

impl VllmGateway {
    pub fn new(config: ProviderConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn client(&self) -> Result<Client, GatewayError> {
        Client::builder()
            .timeout(Duration::from_secs(
                self.config.endpoint.request_timeout_secs,
            ))
            .build()
            .map_err(|error| GatewayError::Transport {
                provider_id: self.config.id.clone(),
                message: error.to_string(),
            })
    }

    fn bearer_token(&self) -> Result<Option<String>, GatewayError> {
        match &self.config.endpoint.auth {
            EndpointAuth::None => Ok(None),
            EndpointAuth::BearerEnv { env_var } => match env::var(env_var) {
                Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
                _ => Err(GatewayError::MissingBearerEnv {
                    provider_id: self.config.id.clone(),
                    env_var: env_var.clone(),
                }),
            },
        }
    }

    fn chat_url(&self) -> String {
        format!(
            "{}/v1/chat/completions",
            self.config.endpoint.base_url.trim_end_matches('/')
        )
    }

    fn models_url(&self) -> String {
        format!(
            "{}/v1/models",
            self.config.endpoint.base_url.trim_end_matches('/')
        )
    }
}

impl ModelGateway for VllmGateway {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.config.id.clone(),
            label: self.config.label.clone(),
            kind: self.config.kind,
            default_model: self.config.model.clone(),
            install_root: self.config.install.root.clone(),
            hosted: false,
        }
    }

    fn validate(&self) -> Result<(), crate::provider::GatewayError> {
        validate_provider_config(&self.config)
    }

    fn probe(&self) -> Result<ProbeResult, GatewayError> {
        self.validate()?;
        let client = self.client()?;
        let token = self.bearer_token()?;
        let mut builder = client.get(self.models_url());
        if let Some(token) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = builder.send().map_err(|error| GatewayError::Transport {
            provider_id: self.config.id.clone(),
            message: error.to_string(),
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let message = response.text().unwrap_or_else(|_| status.to_string());
            return Err(GatewayError::Http {
                provider_id: self.config.id.clone(),
                message,
            });
        }
        let payload: VllmModelsResponse =
            response
                .json()
                .map_err(|error| GatewayError::InvalidResponse {
                    provider_id: format!("{} ({error})", self.config.id),
                })?;
        Ok(ProbeResult {
            provider_label: self.config.label.clone(),
            model_count: payload.data.len(),
            model_ids: payload.data.into_iter().map(|model| model.id).collect(),
        })
    }

    fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, GatewayError> {
        self.validate()?;
        let client = self.client()?;
        let token = self.bearer_token()?;
        let body = VllmChatRequest::from_request(&self.config.model, request);
        let mut builder = client
            .post(self.chat_url())
            .header(CONTENT_TYPE, "application/json")
            .json(&body);
        if let Some(token) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = builder.send().map_err(|error| GatewayError::Transport {
            provider_id: self.config.id.clone(),
            message: error.to_string(),
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let message = response.text().unwrap_or_else(|_| status.to_string());
            return Err(GatewayError::Http {
                provider_id: self.config.id.clone(),
                message,
            });
        }
        let payload: VllmChatResponse =
            response
                .json()
                .map_err(|error| GatewayError::InvalidResponse {
                    provider_id: format!("{} ({error})", self.config.id),
                })?;
        payload.into_chat_response(&self.config.id)
    }
}

pub fn default_provider_config() -> ProviderConfig {
    ProviderConfig {
        id: "vllm-local".to_string(),
        label: "Local vLLM".to_string(),
        kind: ProviderKind::Vllm,
        enabled: true,
        model: "google/gemma-3-4b-it".to_string(),
        endpoint: EndpointConfig {
            base_url: "http://127.0.0.1:8000".to_string(),
            auth: EndpointAuth::BearerEnv {
                env_var: "PATINA_LLM_VLLM_API_KEY".to_string(),
            },
            request_timeout_secs: 120,
        },
        install: InstallLayoutRef {
            root: INSTALL_ROOT.to_string(),
            launch_script: LAUNCH_SCRIPT.to_string(),
            env_file: ENV_FILE.to_string(),
            config_example: CONFIG_EXAMPLE.to_string(),
            notes: NOTES_PATH.to_string(),
        },
    }
}

#[derive(Debug, Serialize)]
struct VllmChatRequest {
    model: String,
    messages: Vec<VllmMessage>,
    temperature: f32,
    max_tokens: u32,
}

impl VllmChatRequest {
    fn from_request(model: &str, request: &ChatRequest) -> Self {
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            if !system_prompt.trim().is_empty() {
                messages.push(VllmMessage {
                    role: "system".to_string(),
                    content: system_prompt.clone(),
                });
            }
        }
        messages.push(VllmMessage {
            role: "user".to_string(),
            content: request.user_prompt.clone(),
        });
        Self {
            model: model.to_string(),
            messages,
            temperature: request.temperature_milli as f32 / 1000.0,
            max_tokens: request.max_output_tokens,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct VllmMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct VllmChatResponse {
    model: String,
    choices: Vec<VllmChoice>,
}

impl VllmChatResponse {
    fn into_chat_response(self, provider_id: &str) -> Result<ChatResponse, GatewayError> {
        let Some(choice) = self.choices.into_iter().next() else {
            return Err(GatewayError::InvalidResponse {
                provider_id: provider_id.to_string(),
            });
        };
        Ok(ChatResponse {
            model: self.model,
            output_text: choice.message.content,
        })
    }
}

#[derive(Debug, Deserialize)]
struct VllmChoice {
    message: VllmMessage,
}

#[derive(Debug, Deserialize)]
struct VllmModelsResponse {
    data: Vec<VllmModelRecord>,
}

#[derive(Debug, Deserialize)]
struct VllmModelRecord {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::{
        default_provider_config, VllmChatRequest, VllmChatResponse, VllmGateway, VllmModelsResponse,
    };
    use crate::provider::{ChatRequest, ModelGateway};
    use serde_json::json;

    #[test]
    fn default_vllm_config_is_valid() {
        let gateway = VllmGateway::new(default_provider_config());
        gateway.validate().expect("vllm config should validate");
    }

    #[test]
    fn builds_openai_style_chat_payload_for_vllm() {
        let request = ChatRequest {
            system_prompt: Some("You are a scientific assistant.".to_string()),
            user_prompt: "Summarise this structure.".to_string(),
            temperature_milli: 250,
            max_output_tokens: 512,
        };
        let payload = VllmChatRequest::from_request("google/gemma-3-4b-it", &request);
        assert_eq!(payload.model, "google/gemma-3-4b-it");
        assert_eq!(payload.messages.len(), 2);
        assert_eq!(payload.messages[0].role, "system");
        assert_eq!(payload.messages[1].role, "user");
        assert_eq!(payload.max_tokens, 512);
    }

    #[test]
    fn parses_vllm_chat_response() {
        let payload = json!({
            "model": "google/gemma-3-4b-it",
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Structure summary."
                    }
                }
            ]
        });
        let parsed: VllmChatResponse = serde_json::from_value(payload).expect("parse");
        let response = parsed.into_chat_response("vllm-local").expect("convert");
        assert_eq!(response.model, "google/gemma-3-4b-it");
        assert_eq!(response.output_text, "Structure summary.");
    }

    #[test]
    fn parses_vllm_models_response() {
        let payload = json!({
            "data": [
                { "id": "google/gemma-3-4b-it" },
                { "id": "google/gemma-3-12b-it" }
            ]
        });
        let parsed: VllmModelsResponse = serde_json::from_value(payload).expect("parse");
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.data[0].id, "google/gemma-3-4b-it");
    }
}
