use crate::genetic_algorithm::native_run::{
    load_rust_ga_generation_states, load_rust_ga_run, RustNativeGaRunError,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NativeGmIterError {
    #[error(transparent)]
    NativeRun(#[from] RustNativeGaRunError),
    #[error("no generation state snapshots were found under `{0}`")]
    NoGenerationStates(PathBuf),
    #[error("final generation contains no converged member with a canonical hashkey")]
    NoFinalBestHashkey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeGmIterConfig {
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeGmIterReport {
    pub latest_generation: usize,
    pub first_matching_generation: usize,
    pub final_best_hashkey: String,
    pub final_best_energy: f64,
    pub final_best_source_label: String,
    pub final_best_relaxed_label: String,
    pub final_best_origin: String,
    pub scanned_generation_count: usize,
}

pub fn run_native_gm_iter_workflow(
    config: &NativeGmIterConfig,
) -> Result<NativeGmIterReport, NativeGmIterError> {
    let run = load_rust_ga_run(&config.run_dir)?;
    let snapshots = load_rust_ga_generation_states(&config.run_dir)?;
    if snapshots.is_empty() {
        return Err(NativeGmIterError::NoGenerationStates(
            config.run_dir.clone(),
        ));
    }

    let final_state = snapshots
        .last()
        .map(|snapshot| &snapshot.state)
        .ok_or_else(|| NativeGmIterError::NoGenerationStates(config.run_dir.clone()))?;
    let final_best = final_state
        .population
        .iter()
        .filter(|member| member.evaluation.converged)
        .filter_map(|member| {
            member
                .topology
                .canonical_hashkey
                .as_ref()
                .map(|hashkey| (member, hashkey))
        })
        .min_by(|(left, _), (right, _)| {
            left.evaluation
                .energy
                .partial_cmp(&right.evaluation.energy)
                .unwrap_or(std::cmp::Ordering::Greater)
        })
        .ok_or(NativeGmIterError::NoFinalBestHashkey)?;

    let first_matching_generation = snapshots
        .iter()
        .find(|snapshot| {
            snapshot.state.population.iter().any(|member| {
                member.topology.canonical_hashkey.as_deref() == Some(final_best.1.as_str())
            })
        })
        .map(|snapshot| snapshot.generation)
        .ok_or(NativeGmIterError::NoFinalBestHashkey)?;

    Ok(NativeGmIterReport {
        latest_generation: run.checkpoint.generation_state.generation,
        first_matching_generation,
        final_best_hashkey: final_best.1.clone(),
        final_best_energy: final_best.0.evaluation.energy,
        final_best_source_label: final_best.0.source.label.clone(),
        final_best_relaxed_label: final_best.0.evaluation.label.clone(),
        final_best_origin: final_best.0.origin.clone(),
        scanned_generation_count: snapshots.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::{run_native_gm_iter_workflow, NativeGmIterConfig};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn finds_first_generation_where_final_best_hashkey_appears() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        fs::create_dir_all(run_dir.join("raw")).expect("raw dir");
        fs::create_dir_all(run_dir.join("traces")).expect("traces dir");
        fs::write(
            run_dir.join("manifest.json"),
            r#"{
  "artifacts": {
    "generation_metrics": "traces/generation_metrics.csv",
    "controller_trace": "traces/controller_trace.csv",
    "latest_restart_checkpoint": "raw/rust_ga_checkpoint_latest.json"
  },
  "backend": "scott_runtime",
  "population_size": 2,
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
            "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,boundary_population_size,boundary_valid_population_size,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count\n0,initialize,1.0,2,2,0,2,-10.0,-9.5,-9.0,2,2,2,2,0,0,0,0,0\n1,evolve,1.0,2,2,0,2,-11.0,-10.5,-10.0,2,2,2,2,0,0,0,0,0\n",
        )
        .expect("write metrics");
        fs::write(
            run_dir.join("traces/controller_trace.csv"),
            "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices\n0,Initialize,2,2,2,0,0,0,0,0,-10.0,\n1,Selection,2,2,0,0,0,0,0,0,-11.0,0|1\n",
        )
        .expect("write trace");
        fs::write(
            run_dir.join("raw/rust_ga_checkpoint_latest.json"),
            r#"{
  "workflow_owner": "scott_staged_ga",
  "checkpoint_version": 1,
  "generation_completed": 1,
  "population_size": 2,
  "requested_generations": 2,
  "seed": 11,
  "system": "(MgO)2",
  "backend": "scott_runtime",
  "janus_mode": "gulp",
  "search_config": {
    "temperature": 625.0,
    "step_size": 0.5,
    "population_size": 2,
    "max_steps": 2,
    "seed": 11
  },
  "generation_state": {
    "generation": 1,
    "population": [],
    "elites": [],
    "repopulation": []
  }
}"#,
        )
        .expect("write checkpoint");
        fs::write(
            run_dir.join("raw/generation_0000_state.json"),
            r#"{
  "generation": 0,
  "population": [
    {
      "member_id": 0,
      "origin": "SEED",
      "occurrences": 1,
      "source": { "label": "seed_a", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] },
      "evaluation": { "label": "seed_a_relaxed", "energy": -10.0, "converged": true, "structure": { "label": "seed_a_relaxed", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] }, "backend_run_dir": null, "primary_output_path": null },
      "lineage": { "origin_label": "seed_a", "generation": 0, "step": null, "parent_labels": [], "attempt": 1 },
      "topology": { "canonical_hashkey": "hk-a", "source_hashkey": "hk-a", "relaxed_hashkey": "hk-a" }
    }
  ],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write g0");
        fs::write(
            run_dir.join("raw/generation_0001_state.json"),
            r#"{
  "generation": 1,
  "population": [
    {
      "member_id": 0,
      "origin": "MUTATE",
      "occurrences": 1,
      "source": { "label": "child_best", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] },
      "evaluation": { "label": "child_best_relaxed", "energy": -11.0, "converged": true, "structure": { "label": "child_best_relaxed", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] }, "backend_run_dir": null, "primary_output_path": null },
      "lineage": { "origin_label": "child_best", "generation": 1, "step": null, "parent_labels": ["seed_a"], "attempt": 1 },
      "topology": { "canonical_hashkey": "hk-a", "source_hashkey": "hk-a", "relaxed_hashkey": "hk-a" }
    }
  ],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write g1");

        let report = run_native_gm_iter_workflow(&NativeGmIterConfig {
            run_dir: run_dir.clone(),
        })
        .expect("native gm iter");

        assert_eq!(report.latest_generation, 1);
        assert_eq!(report.first_matching_generation, 0);
        assert_eq!(report.final_best_hashkey, "hk-a");
        assert_eq!(report.final_best_energy, -11.0);
    }
}
