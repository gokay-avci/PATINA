use crate::genetic_algorithm::native_run::{load_rust_ga_run, RustNativeGaRunError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GaEnergyEvolutionConfig {
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaEnergyEvolutionPoint {
    pub generation: usize,
    pub phase: String,
    pub elapsed_secs: f64,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub duplicate_count: usize,
    pub repopulated_count: usize,
    pub best_energy_improvement: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaEnergyEvolutionReport {
    pub workflow_owner: String,
    pub backend: String,
    pub run_dir: PathBuf,
    pub generation_count: usize,
    pub total_request_count: usize,
    pub total_failure_count: usize,
    pub global_best_energy: Option<f64>,
    pub global_best_generation: Option<usize>,
    pub final_best_energy: Option<f64>,
    pub final_mean_energy: Option<f64>,
    pub final_worst_energy: Option<f64>,
    pub points: Vec<GaEnergyEvolutionPoint>,
}

pub fn run_energy_evolution_workflow(
    config: &GaEnergyEvolutionConfig,
) -> Result<GaEnergyEvolutionReport, RustNativeGaRunError> {
    let run = load_rust_ga_run(&config.run_dir)?;

    let mut points = Vec::with_capacity(run.generation_metrics.len());
    let mut previous_best = None::<f64>;
    let mut global_best = None::<(usize, f64)>;
    let mut total_request_count = 0usize;
    let mut total_failure_count = 0usize;

    for row in &run.generation_metrics {
        let best_energy_improvement = match (previous_best, row.best_energy) {
            (Some(previous), Some(current)) if current < previous => Some(previous - current),
            _ => None,
        };

        if let Some(current_best) = row.best_energy {
            previous_best = Some(previous_best.map_or(current_best, |best| best.min(current_best)));
            match global_best {
                Some((_, best_energy)) if current_best >= best_energy => {}
                _ => global_best = Some((row.generation, current_best)),
            }
        }

        total_request_count += row.request_count;
        total_failure_count += row.failure_count;

        points.push(GaEnergyEvolutionPoint {
            generation: row.generation,
            phase: row.phase.clone(),
            elapsed_secs: row.elapsed_secs,
            request_count: row.request_count,
            success_count: row.success_count,
            failure_count: row.failure_count,
            best_energy: row.best_energy,
            mean_energy: row.mean_energy,
            worst_energy: row.worst_energy,
            duplicate_count: row.duplicate_count,
            repopulated_count: row.repopulated_count,
            best_energy_improvement,
        });
    }

    let last = run.generation_metrics.last();

    Ok(GaEnergyEvolutionReport {
        workflow_owner: run.manifest.workflow_owner,
        backend: run.manifest.backend,
        run_dir: config.run_dir.clone(),
        generation_count: run.generation_metrics.len(),
        total_request_count,
        total_failure_count,
        global_best_energy: global_best.map(|(_, energy)| energy),
        global_best_generation: global_best.map(|(generation, _)| generation),
        final_best_energy: last.and_then(|row| row.best_energy),
        final_mean_energy: last.and_then(|row| row.mean_energy),
        final_worst_energy: last.and_then(|row| row.worst_energy),
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::{run_energy_evolution_workflow, GaEnergyEvolutionConfig};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn computes_best_energy_evolution_from_native_metrics() {
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
  "population_size": 3,
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
            "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,boundary_population_size,boundary_valid_population_size,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count\n0,initialize,1.0,10,9,1,9,-10.0,-9.5,-9.0,3,3,3,3,1,1,0,0,0\n1,evolve,2.0,8,8,0,8,-10.7,-9.9,-9.1,3,3,3,3,2,1,1,0,1\n",
        )
        .expect("write generation metrics");
        fs::write(
            run_dir.join("traces/controller_trace.csv"),
            "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices\n0,Initialize,3,3,3,1,1,0,0,0,-10.0,\n",
        )
        .expect("write controller trace");
        fs::write(
            run_dir.join("raw/rust_ga_checkpoint_latest.json"),
            r#"{
  "workflow_owner": "scott_staged_ga",
  "checkpoint_version": 1,
  "generation_completed": 1,
  "population_size": 3,
  "requested_generations": 2,
  "seed": 11,
  "system": "(MgO)2",
  "backend": "scott_runtime",
  "janus_mode": "gulp",
  "search_config": {
    "temperature": 625.0,
    "step_size": 0.5,
    "population_size": 3,
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

        let report = run_energy_evolution_workflow(&GaEnergyEvolutionConfig {
            run_dir: run_dir.clone(),
        })
        .expect("energy evolution");

        assert_eq!(report.generation_count, 2);
        assert_eq!(report.global_best_generation, Some(1));
        assert_eq!(report.global_best_energy, Some(-10.7));
        assert_eq!(report.total_failure_count, 1);
        let improvement = report.points[1]
            .best_energy_improvement
            .expect("generation 1 improvement");
        assert!((improvement - 0.7).abs() < 1.0e-9);
    }
}
