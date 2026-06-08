use anyhow::{anyhow, Context, Result};
use figment::{
    providers::{Env, Format, Json, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorkflowConfiguration {
    pub stage: StageConfiguration,
    pub input: StructureInputConfiguration,
    pub structure: StructureToolConfiguration,
    pub ga: GaWorkflowConfiguration,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StageConfiguration {
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StructureInputConfiguration {
    pub candidate_json: Option<PathBuf>,
    pub structure_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GaWorkflowConfiguration {
    pub base_candidate_json: Option<PathBuf>,
    pub cluster_seed_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StructureToolConfiguration {
    pub space_group: SpaceGroupConfiguration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SpaceGroupConfiguration {
    pub position_tolerance: f64,
    pub cell_tolerance: f64,
    pub output_json: bool,
}

impl Default for SpaceGroupConfiguration {
    fn default() -> Self {
        Self {
            position_tolerance: 1.0e-5,
            cell_tolerance: 1.0e-5,
            output_json: false,
        }
    }
}

pub(crate) fn load_workflow_configuration(
    config_path: Option<&Path>,
) -> Result<WorkflowConfiguration> {
    let mut figment = Figment::from(Serialized::defaults(WorkflowConfiguration::default()));

    if let Some(config_path) = config_path {
        figment = merge_config_file(figment, config_path)?;
    }

    merge_workspace_env(figment)
        .extract()
        .context("failed to resolve PATINA workflow configuration")
}

fn merge_workspace_env(figment: Figment) -> Figment {
    figment.merge(Env::prefixed("PATINA_").split("__"))
}

fn merge_config_file(figment: Figment, config_path: &Path) -> Result<Figment> {
    match config_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("toml") => Ok(figment.merge(Toml::file(config_path))),
        Some("json") => Ok(figment.merge(Json::file(config_path))),
        Some(extension) => Err(anyhow!(
            "unsupported config file extension `.{extension}` for `{}`; expected .toml or .json",
            config_path.display()
        )),
        None => Err(anyhow!(
            "configuration file `{}` has no extension; expected .toml or .json",
            config_path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::load_workflow_configuration;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    const WORKSPACE_ENV_TEST_KEYS: &[&str] = &[
        "PATINA_STAGE__DIR",
        "PATINA_STRUCTURE__SPACE_GROUP__OUTPUT_JSON",
    ];

    struct EnvSnapshot {
        values: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvSnapshot {
        fn capture(keys: &[&'static str]) -> Self {
            Self {
                values: keys
                    .iter()
                    .map(|key| (*key, std::env::var_os(key)))
                    .collect(),
            }
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            for (key, value) in &self.values {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }

    fn with_env_vars<T>(vars: &[(&'static str, &'static str)], f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let mut keys = WORKSPACE_ENV_TEST_KEYS.to_vec();
        for (key, _) in vars {
            if !keys.contains(key) {
                keys.push(key);
            }
        }
        let _snapshot = EnvSnapshot::capture(&keys);
        for key in &keys {
            std::env::remove_var(key);
        }
        for (key, value) in vars {
            std::env::set_var(key, value);
        }
        f()
    }

    #[test]
    fn loads_nested_toml_workflow_configuration() {
        with_env_vars(&[], || {
            let tempdir = tempfile::tempdir().expect("tempdir");
            let config_path = tempdir.path().join("patina.toml");
            fs::write(
                &config_path,
                r#"
                [stage]
                dir = "stage-a"

                [input]
                structure_path = "zif8.cif"

                [ga]
                cluster_seed_path = "seed.xyz"

                [structure.space_group]
                position_tolerance = 0.0002
                output_json = true
            "#,
            )
            .expect("write config");

            let config = load_workflow_configuration(Some(&config_path)).expect("load config");

            assert_eq!(config.stage.dir, Some(PathBuf::from("stage-a")));
            assert_eq!(config.input.structure_path, Some(PathBuf::from("zif8.cif")));
            assert_eq!(config.ga.cluster_seed_path, Some(PathBuf::from("seed.xyz")));
            assert_eq!(config.structure.space_group.position_tolerance, 0.0002);
            assert_eq!(config.structure.space_group.cell_tolerance, 1.0e-5);
            assert!(config.structure.space_group.output_json);
        });
    }

    #[test]
    fn patina_env_prefix_configures_workspace() {
        with_env_vars(
            &[
                ("PATINA_STAGE__DIR", "patina-stage"),
                ("PATINA_STRUCTURE__SPACE_GROUP__OUTPUT_JSON", "true"),
            ],
            || {
                let config = load_workflow_configuration(None).expect("load config");
                assert_eq!(config.stage.dir, Some(PathBuf::from("patina-stage")));
                assert!(config.structure.space_group.output_json);
            },
        );
    }
}
