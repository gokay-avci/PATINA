use csv::Trim;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RustNativeGaRunError {
    #[error("run directory `{0}` does not exist")]
    MissingRunDir(PathBuf),
    #[error("required GA artifact `{artifact}` is not listed in `{manifest_path}`")]
    MissingArtifactEntry {
        artifact: &'static str,
        manifest_path: PathBuf,
    },
    #[error("failed to read manifest `{path}`: {source}")]
    ReadManifest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse manifest `{path}`: {source}")]
    ParseManifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to read generation metrics `{path}`: {source}")]
    ReadGenerationMetrics {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse generation metrics `{path}`: {source}")]
    ParseGenerationMetrics { path: PathBuf, source: csv::Error },
    #[error("failed to read controller trace `{path}`: {source}")]
    ReadControllerTrace {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse controller trace `{path}`: {source}")]
    ParseControllerTrace { path: PathBuf, source: csv::Error },
    #[error("failed to read GA checkpoint `{path}`: {source}")]
    ReadCheckpoint {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse GA checkpoint `{path}`: {source}")]
    ParseCheckpoint {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to read generation state `{path}`: {source}")]
    ReadGenerationState {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse generation state `{path}`: {source}")]
    ParseGenerationState {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaSearchConfig {
    pub temperature: f64,
    pub step_size: f64,
    #[serde(default)]
    pub population_size: Option<usize>,
    #[serde(default)]
    pub max_steps: Option<usize>,
    #[serde(default)]
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaManifest {
    pub artifacts: BTreeMap<String, String>,
    pub backend: String,
    #[serde(default)]
    pub duplicate_policy_mode: Option<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
    #[serde(default)]
    pub lane_mode: Option<String>,
    #[serde(default)]
    pub operator_policy: serde_json::Value,
    #[serde(default)]
    pub parallel_contract: Option<String>,
    pub population_size: usize,
    pub requested_generations: usize,
    pub run_name: String,
    pub search_config: RustGaSearchConfig,
    pub system: String,
    pub workflow_owner: String,
    #[serde(default)]
    pub workflow_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaGenerationMetricRecord {
    pub generation: usize,
    pub phase: String,
    pub elapsed_secs: f64,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub converged_count: usize,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub boundary_population_size: usize,
    pub boundary_valid_population_size: usize,
    pub population_size: usize,
    pub valid_population_size: usize,
    pub duplicate_count: usize,
    pub duplicate_hashkey_count: usize,
    pub duplicate_pmoi_count: usize,
    pub duplicate_energy_tol_count: usize,
    pub repopulated_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaControllerTraceRecord {
    pub generation: usize,
    pub stage: String,
    pub population_size: usize,
    pub valid_population_size: usize,
    pub child_count: usize,
    pub duplicate_count: usize,
    pub duplicate_hashkey_count: usize,
    pub duplicate_pmoi_count: usize,
    pub duplicate_energy_tol_count: usize,
    pub repopulated_count: usize,
    pub best_energy: Option<f64>,
    #[serde(default)]
    pub selected_indices: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaCheckpoint {
    pub workflow_owner: String,
    pub checkpoint_version: u32,
    pub generation_completed: usize,
    pub population_size: usize,
    pub requested_generations: usize,
    #[serde(default)]
    pub seed: Option<u64>,
    pub system: String,
    pub backend: String,
    pub janus_mode: String,
    pub search_config: RustGaSearchConfig,
    #[serde(default)]
    pub operator_policy: serde_json::Value,
    pub generation_state: patina_types::GaGenerationState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaRunArtifacts {
    pub run_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: RustGaManifest,
    pub generation_metrics_path: PathBuf,
    pub generation_metrics: Vec<RustGaGenerationMetricRecord>,
    pub controller_trace_path: PathBuf,
    pub controller_trace: Vec<RustGaControllerTraceRecord>,
    pub checkpoint_path: PathBuf,
    pub checkpoint: RustGaCheckpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustGaGenerationStateSnapshot {
    pub generation: usize,
    pub path: PathBuf,
    pub state: patina_types::GaGenerationState,
}

impl RustGaControllerTraceRecord {
    pub fn selected_index_values(&self) -> Vec<usize> {
        self.selected_indices
            .as_deref()
            .map(|raw| {
                raw.split('|')
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| part.parse::<usize>().ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub fn load_rust_ga_run(run_dir: &Path) -> Result<RustGaRunArtifacts, RustNativeGaRunError> {
    if !run_dir.exists() {
        return Err(RustNativeGaRunError::MissingRunDir(run_dir.to_path_buf()));
    }

    let manifest_path = run_dir.join("manifest.json");
    let manifest_raw = fs::read_to_string(&manifest_path).map_err(|source| {
        RustNativeGaRunError::ReadManifest {
            path: manifest_path.clone(),
            source,
        }
    })?;
    let manifest: RustGaManifest = serde_json::from_str(&manifest_raw).map_err(|source| {
        RustNativeGaRunError::ParseManifest {
            path: manifest_path.clone(),
            source,
        }
    })?;

    let generation_metrics_path =
        resolve_artifact_path(run_dir, &manifest, &manifest_path, "generation_metrics")?;
    let controller_trace_path =
        resolve_artifact_path(run_dir, &manifest, &manifest_path, "controller_trace")?;
    let checkpoint_path = resolve_artifact_path(
        run_dir,
        &manifest,
        &manifest_path,
        "latest_restart_checkpoint",
    )?;

    let generation_metrics = read_csv_records(&generation_metrics_path).map_err(|source| {
        RustNativeGaRunError::ParseGenerationMetrics {
            path: generation_metrics_path.clone(),
            source,
        }
    })?;
    let controller_trace = read_csv_records(&controller_trace_path).map_err(|source| {
        RustNativeGaRunError::ParseControllerTrace {
            path: controller_trace_path.clone(),
            source,
        }
    })?;
    let checkpoint_raw = fs::read_to_string(&checkpoint_path).map_err(|source| {
        RustNativeGaRunError::ReadCheckpoint {
            path: checkpoint_path.clone(),
            source,
        }
    })?;
    let checkpoint: RustGaCheckpoint = serde_json::from_str(&checkpoint_raw).map_err(|source| {
        RustNativeGaRunError::ParseCheckpoint {
            path: checkpoint_path.clone(),
            source,
        }
    })?;

    Ok(RustGaRunArtifacts {
        run_dir: run_dir.to_path_buf(),
        manifest_path,
        manifest,
        generation_metrics_path,
        generation_metrics,
        controller_trace_path,
        controller_trace,
        checkpoint_path,
        checkpoint,
    })
}

pub fn load_rust_ga_generation_states(
    run_dir: &Path,
) -> Result<Vec<RustGaGenerationStateSnapshot>, RustNativeGaRunError> {
    if !run_dir.exists() {
        return Err(RustNativeGaRunError::MissingRunDir(run_dir.to_path_buf()));
    }

    let raw_dir = run_dir.join("raw");
    let mut snapshots = Vec::new();
    if !raw_dir.exists() {
        return Ok(snapshots);
    }

    for entry in
        fs::read_dir(&raw_dir).map_err(|source| RustNativeGaRunError::ReadGenerationState {
            path: raw_dir.clone(),
            source,
        })?
    {
        let entry = entry.map_err(|source| RustNativeGaRunError::ReadGenerationState {
            path: raw_dir.clone(),
            source,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(generation) = generation_from_state_filename(file_name) else {
            continue;
        };
        let raw = fs::read_to_string(&path).map_err(|source| {
            RustNativeGaRunError::ReadGenerationState {
                path: path.clone(),
                source,
            }
        })?;
        let state =
            serde_json::from_str::<patina_types::GaGenerationState>(&raw).map_err(|source| {
                RustNativeGaRunError::ParseGenerationState {
                    path: path.clone(),
                    source,
                }
            })?;
        snapshots.push(RustGaGenerationStateSnapshot {
            generation,
            path,
            state,
        });
    }

    snapshots.sort_by_key(|snapshot| snapshot.generation);
    Ok(snapshots)
}

fn resolve_artifact_path(
    run_dir: &Path,
    manifest: &RustGaManifest,
    manifest_path: &Path,
    artifact: &'static str,
) -> Result<PathBuf, RustNativeGaRunError> {
    let relative = manifest.artifacts.get(artifact).ok_or_else(|| {
        RustNativeGaRunError::MissingArtifactEntry {
            artifact,
            manifest_path: manifest_path.to_path_buf(),
        }
    })?;
    Ok(run_dir.join(relative))
}

fn read_csv_records<T>(path: &Path) -> Result<Vec<T>, csv::Error>
where
    T: for<'de> Deserialize<'de>,
{
    let file = fs::File::open(path).map_err(csv::Error::from)?;
    let mut reader = csv::ReaderBuilder::new().trim(Trim::All).from_reader(file);
    reader.deserialize().collect()
}

fn generation_from_state_filename(file_name: &str) -> Option<usize> {
    let suffix = file_name.strip_prefix("generation_")?;
    let generation = suffix.strip_suffix("_state.json")?;
    generation.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::{load_rust_ga_generation_states, load_rust_ga_run, RustGaControllerTraceRecord};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn loads_native_ga_artifacts_from_manifest_paths() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        fs::create_dir_all(run_dir.join("traces")).expect("traces dir");
        fs::create_dir_all(run_dir.join("raw")).expect("raw dir");

        fs::write(
            run_dir.join("manifest.json"),
            r#"{
  "artifacts": {
    "generation_metrics": "traces/generation_metrics.csv",
    "controller_trace": "traces/controller_trace.csv",
    "latest_restart_checkpoint": "raw/rust_ga_checkpoint_latest.json"
  },
  "backend": "scott_runtime",
  "population_size": 4,
  "requested_generations": 2,
  "run_name": "sample_run",
  "search_config": {
    "temperature": 625.0,
    "step_size": 0.5,
    "seed": 11
  },
  "system": "(MgO)2",
  "workflow_owner": "scott_staged_ga"
}"#,
        )
        .expect("write manifest");
        fs::write(
            run_dir.join("traces/generation_metrics.csv"),
            "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,boundary_population_size,boundary_valid_population_size,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count\n0,initialize,1.0,10,9,1,9,-10.0,-9.5,-9.0,4,4,4,4,1,1,0,0,0\n1,evolve,2.0,8,8,0,8,-10.5,-9.8,-9.2,4,4,4,4,2,1,1,0,1\n",
        )
        .expect("write generation metrics");
        fs::write(
            run_dir.join("traces/controller_trace.csv"),
            "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices\n0,Initialize,4,4,4,1,1,0,0,0,-10.0,\n1,Selection,4,4,0,0,0,0,0,0,-10.0,0|1|2|3\n",
        )
        .expect("write controller trace");
        fs::write(
            run_dir.join("raw/rust_ga_checkpoint_latest.json"),
            r#"{
  "workflow_owner": "scott_staged_ga",
  "checkpoint_version": 1,
  "generation_completed": 1,
  "population_size": 4,
  "requested_generations": 2,
  "seed": 11,
  "system": "(MgO)2",
  "backend": "scott_runtime",
  "janus_mode": "gulp",
  "search_config": {
    "temperature": 625.0,
    "step_size": 0.5,
    "population_size": 4,
    "max_steps": 2,
    "seed": 11
  },
  "generation_state": {
    "generation": 1,
    "population": [
      {
        "member_id": 0,
        "origin": "SEED",
        "occurrences": 1,
        "source": {
          "label": "seed",
          "species": ["Mg", "O"],
          "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
          "lattice": null,
          "periodic_axes": [false, false, false]
        },
        "evaluation": {
          "label": "seed_relaxed",
          "energy": -10.5,
          "converged": true,
          "structure": {
            "label": "seed_relaxed",
            "species": ["Mg", "O"],
            "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            "lattice": null,
            "periodic_axes": [false, false, false]
          },
          "backend_run_dir": null,
          "primary_output_path": null
        }
      }
    ],
    "elites": [],
    "repopulation": []
  }
}"#,
        )
        .expect("write checkpoint");

        let run = load_rust_ga_run(&run_dir).expect("load native run");
        assert_eq!(run.manifest.workflow_owner, "scott_staged_ga");
        assert_eq!(run.generation_metrics.len(), 2);
        assert_eq!(run.controller_trace.len(), 2);
        assert_eq!(run.checkpoint.generation_completed, 1);
        assert_eq!(run.checkpoint.generation_state.population.len(), 1);
    }

    #[test]
    fn parses_controller_selected_indices() {
        let record = RustGaControllerTraceRecord {
            generation: 3,
            stage: "Selection".to_string(),
            population_size: 4,
            valid_population_size: 4,
            child_count: 0,
            duplicate_count: 0,
            duplicate_hashkey_count: 0,
            duplicate_pmoi_count: 0,
            duplicate_energy_tol_count: 0,
            repopulated_count: 0,
            best_energy: Some(-1.0),
            selected_indices: Some("0|3|5".to_string()),
        };

        assert_eq!(record.selected_index_values(), vec![0, 3, 5]);
    }

    #[test]
    fn loads_generation_state_snapshots_from_raw_dir() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        fs::create_dir_all(run_dir.join("raw")).expect("raw dir");
        fs::write(
            run_dir.join("raw/generation_0000_state.json"),
            r#"{
  "generation": 0,
  "population": [],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write state 0");
        fs::write(
            run_dir.join("raw/generation_0002_state.json"),
            r#"{
  "generation": 2,
  "population": [],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write state 2");
        fs::write(
            run_dir.join("raw/generation_0001_boundary_state.json"),
            r#"{
  "generation": 1,
  "population": [],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write boundary state");

        let snapshots = load_rust_ga_generation_states(&run_dir).expect("load state snapshots");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].generation, 0);
        assert_eq!(snapshots[1].generation, 2);
    }
}
