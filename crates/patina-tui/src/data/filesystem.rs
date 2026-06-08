use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::data::datasource::RunDataSource;
use crate::data::parsing::{
    parse_controller_trace, parse_generation_metrics, parse_generation_state,
    parse_generation_summary, parse_manifest, parse_walker_trace,
};
use crate::domain::artifacts::{RunSnapshot, WorkflowKind};
use crate::domain::ga::{ArtifactIndex, GenerationStore};
use crate::error::TuiError;

#[derive(Debug, Default)]
pub struct FilesystemRunDataSource;

impl RunDataSource for FilesystemRunDataSource {
    fn load_run(&self, run_dir: &Path) -> Result<RunSnapshot> {
        if !run_dir.exists() {
            return Err(TuiError::MissingRunDir {
                path: run_dir.to_path_buf(),
            }
            .into());
        }

        let manifest_path = run_dir.join("manifest.json");
        let manifest = parse_manifest(&manifest_path)?;
        let generation_metrics =
            parse_generation_metrics(&run_dir.join("traces/generation_metrics.csv"))?;
        let controller_trace =
            parse_controller_trace(&run_dir.join("traces/controller_trace.csv"))?;
        let walker_trace = parse_walker_trace(&run_dir.join("traces/walker_trace.csv"))?;
        let generations = load_generations(run_dir)?;
        let artifacts = ArtifactIndex {
            manifest_path,
            raw_files: collect_relative_files(&run_dir.join("raw"), run_dir),
            output_files: collect_relative_files(&run_dir.join("outputs"), run_dir),
        };

        let workflow_kind = if !generation_metrics.is_empty() || !generations.states.is_empty() {
            WorkflowKind::Ga
        } else if !walker_trace.is_empty() {
            WorkflowKind::Bh
        } else {
            WorkflowKind::Unknown
        };

        Ok(RunSnapshot {
            run_dir: run_dir.to_path_buf(),
            loaded_from_run_dir: true,
            manifest,
            workflow_kind,
            generation_metrics,
            controller_trace,
            walker_trace,
            generations,
            artifacts,
        })
    }
}

fn load_generations(run_dir: &Path) -> Result<GenerationStore> {
    let raw_dir = run_dir.join("raw");
    let mut store = GenerationStore::default();
    if !raw_dir.exists() {
        return Ok(store);
    }

    for entry in fs::read_dir(&raw_dir)
        .with_context(|| format!("failed to read raw artifact dir `{}`", raw_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        if let Some(generation) = generation_index(name, "generation_", "_state.json") {
            let state = parse_generation_state(&path).with_context(|| {
                format!("failed to parse generation state `{}`", path.display())
            })?;
            store.states.insert(generation, state);
            continue;
        }

        if let Some(generation) = generation_index(name, "generation_", "_summary.json") {
            let summary = parse_generation_summary(&path).with_context(|| {
                format!("failed to parse generation summary `{}`", path.display())
            })?;
            store.summaries.insert(generation, summary);
        }
    }

    Ok(store)
}

fn generation_index(name: &str, prefix: &str, suffix: &str) -> Option<usize> {
    let stripped = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    stripped.parse::<usize>().ok()
}

fn collect_relative_files(root: &Path, base: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !root.exists() {
        return files;
    }
    visit_files(root, &mut |path| {
        if let Ok(relative) = path.strip_prefix(base) {
            files.push(relative.to_path_buf());
        }
    });
    files.sort();
    files
}

fn visit_files(root: &Path, callback: &mut dyn FnMut(&Path)) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit_files(&path, callback);
            } else if path.is_file() {
                callback(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::data::datasource::RunDataSource;
    use crate::domain::artifacts::WorkflowKind;

    use super::FilesystemRunDataSource;

    #[test]
    fn loads_minimal_ga_run_from_manifest_and_metrics() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path();
        fs::create_dir(run_dir.join("traces")).expect("create traces");
        fs::create_dir(run_dir.join("raw")).expect("create raw");
        fs::create_dir(run_dir.join("outputs")).expect("create outputs");

        fs::write(
            run_dir.join("manifest.json"),
            r#"{
              "run_name": "test_run",
              "workflow_owner": "scott_staged_ga",
              "backend": "scott_runtime",
              "lane_mode": "standalone_capable",
              "parallel_contract": "staged_scott_runtime_generation_dispatch",
              "system": "(MgO)8",
              "requested_generations": 4,
              "population_size": 10,
              "artifacts": {
                "generation_metrics": "traces/generation_metrics.csv"
              }
            }"#,
        )
        .expect("write manifest");
        fs::write(
            run_dir.join("traces").join("generation_metrics.csv"),
            "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count\n0,initialize,1.0,10,10,0,10,-8.0,-7.5,-7.0,10,10,0,0,0,0,0\n",
        )
        .expect("write metrics");

        let snapshot = FilesystemRunDataSource.load_run(run_dir).expect("load run");
        assert_eq!(snapshot.workflow_kind, WorkflowKind::Ga);
        assert_eq!(snapshot.generation_metrics.len(), 1);
        assert_eq!(
            snapshot.manifest.parallel_contract.as_deref(),
            Some("staged_scott_runtime_generation_dispatch")
        );
        assert_eq!(
            snapshot.artifacts.manifest_path,
            run_dir.join("manifest.json")
        );
    }
}
