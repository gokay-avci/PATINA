use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RunManifest {
    #[serde(default)]
    pub run_name: Option<String>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub workflow_owner: Option<String>,
    #[serde(default)]
    pub workflow_scope: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub lane_mode: Option<String>,
    #[serde(default)]
    pub parallel_contract: Option<String>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub requested_generations: Option<usize>,
    #[serde(default)]
    pub population_size: Option<usize>,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    #[serde(default)]
    pub search_config: Option<Value>,
    #[serde(default)]
    pub operator_policy: Option<Value>,
    #[serde(default)]
    pub extra: Option<Value>,
    #[serde(default)]
    pub provenance: Option<Value>,
}
