use crate::config::{ProviderConfig, ProviderKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    pub default_model: String,
    pub install_root: String,
    pub hosted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub temperature_milli: u16,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub model: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub provider_label: String,
    pub model_count: usize,
    pub model_ids: Vec<String>,
}

pub trait ModelGateway {
    fn metadata(&self) -> ProviderMetadata;
    fn validate(&self) -> Result<(), GatewayError>;
    fn probe(&self) -> Result<ProbeResult, GatewayError>;
    fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, GatewayError>;
}

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("provider `{provider_id}` is disabled")]
    ProviderDisabled { provider_id: String },
    #[error("provider `{provider_id}` has an empty model id")]
    EmptyModel { provider_id: String },
    #[error("provider `{provider_id}` has an empty base url")]
    EmptyBaseUrl { provider_id: String },
    #[error("provider `{provider_id}` requires bearer auth from environment variable `{env_var}`")]
    MissingBearerEnv {
        provider_id: String,
        env_var: String,
    },
    #[error("provider `{provider_id}` returned an HTTP error: {message}")]
    Http {
        provider_id: String,
        message: String,
    },
    #[error("provider `{provider_id}` request failed: {message}")]
    Transport {
        provider_id: String,
        message: String,
    },
    #[error("provider `{provider_id}` returned an unsupported response shape")]
    InvalidResponse { provider_id: String },
}

pub fn validate_provider_config(config: &ProviderConfig) -> Result<(), GatewayError> {
    if !config.enabled {
        return Err(GatewayError::ProviderDisabled {
            provider_id: config.id.clone(),
        });
    }
    if config.model.trim().is_empty() {
        return Err(GatewayError::EmptyModel {
            provider_id: config.id.clone(),
        });
    }
    if config.endpoint.base_url.trim().is_empty() {
        return Err(GatewayError::EmptyBaseUrl {
            provider_id: config.id.clone(),
        });
    }
    Ok(())
}
