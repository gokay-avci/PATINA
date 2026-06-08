use crate::genetic_algorithm::native_run::{load_rust_ga_run, RustNativeGaRunError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GaEnergyHistogramConfig {
    pub run_dir: PathBuf,
    pub bin_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaEnergyHistogramBin {
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GaOriginPopulationSummary {
    pub origin: String,
    pub member_count: usize,
    pub occurrence_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaEnergyHistogramReport {
    pub workflow_owner: String,
    pub backend: String,
    pub run_dir: PathBuf,
    pub generation: usize,
    pub population_size: usize,
    pub converged_population_size: usize,
    pub unconverged_population_size: usize,
    pub min_energy: Option<f64>,
    pub max_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub bins: Vec<GaEnergyHistogramBin>,
    pub origins: Vec<GaOriginPopulationSummary>,
}

pub fn run_energy_histogram_workflow(
    config: &GaEnergyHistogramConfig,
) -> Result<GaEnergyHistogramReport, RustNativeGaRunError> {
    let run = load_rust_ga_run(&config.run_dir)?;
    let population = &run.checkpoint.generation_state.population;

    let mut energies = Vec::new();
    let mut origin_counts = BTreeMap::<String, (usize, usize)>::new();

    for member in population {
        let entry = origin_counts
            .entry(member.origin.clone())
            .or_insert((0usize, 0usize));
        entry.0 += 1;
        entry.1 += member.occurrences;

        if member.evaluation.converged {
            energies.push(member.evaluation.energy);
        }
    }

    let population_size = population.len();
    let converged_population_size = energies.len();
    let unconverged_population_size = population_size.saturating_sub(converged_population_size);
    let min_energy = energies.iter().copied().reduce(f64::min);
    let max_energy = energies.iter().copied().reduce(f64::max);
    let mean_energy = if energies.is_empty() {
        None
    } else {
        Some(energies.iter().sum::<f64>() / energies.len() as f64)
    };
    let bins = build_histogram_bins(&energies, config.bin_count.max(1));
    let origins = origin_counts
        .into_iter()
        .map(
            |(origin, (member_count, occurrence_count))| GaOriginPopulationSummary {
                origin,
                member_count,
                occurrence_count,
            },
        )
        .collect();

    Ok(GaEnergyHistogramReport {
        workflow_owner: run.manifest.workflow_owner,
        backend: run.manifest.backend,
        run_dir: config.run_dir.clone(),
        generation: run.checkpoint.generation_state.generation,
        population_size,
        converged_population_size,
        unconverged_population_size,
        min_energy,
        max_energy,
        mean_energy,
        bins,
        origins,
    })
}

fn build_histogram_bins(energies: &[f64], bin_count: usize) -> Vec<GaEnergyHistogramBin> {
    let Some(min_energy) = energies.iter().copied().reduce(f64::min) else {
        return Vec::new();
    };
    let Some(max_energy) = energies.iter().copied().reduce(f64::max) else {
        return Vec::new();
    };

    if energies.len() == 1 || (max_energy - min_energy).abs() <= f64::EPSILON {
        return vec![GaEnergyHistogramBin {
            lower_bound: min_energy,
            upper_bound: max_energy,
            count: energies.len(),
        }];
    }

    let width = (max_energy - min_energy) / bin_count as f64;
    let mut bins = (0..bin_count)
        .map(|index| GaEnergyHistogramBin {
            lower_bound: min_energy + width * index as f64,
            upper_bound: if index + 1 == bin_count {
                max_energy
            } else {
                min_energy + width * (index + 1) as f64
            },
            count: 0,
        })
        .collect::<Vec<_>>();

    for energy in energies {
        let mut index = ((energy - min_energy) / width).floor() as usize;
        if index >= bin_count {
            index = bin_count - 1;
        }
        bins[index].count += 1;
    }

    bins
}

#[cfg(test)]
mod tests {
    use super::{run_energy_histogram_workflow, GaEnergyHistogramConfig};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn computes_histogram_and_origin_breakdown_from_checkpoint_population() {
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
            "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,boundary_population_size,boundary_valid_population_size,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count\n0,initialize,1.0,10,9,1,3,-10.0,-9.5,-9.0,4,3,4,3,1,1,0,0,0\n",
        )
        .expect("write generation metrics");
        fs::write(
            run_dir.join("traces/controller_trace.csv"),
            "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices\n0,Initialize,4,3,4,1,1,0,0,0,-10.0,\n",
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
        "occurrences": 2,
        "source": {
          "label": "seed_0",
          "species": ["Mg", "O"],
          "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
          "lattice": null,
          "periodic_axes": [false, false, false]
        },
        "evaluation": {
          "label": "seed_0_relaxed",
          "energy": -10.0,
          "converged": true,
          "structure": {
            "label": "seed_0_relaxed",
            "species": ["Mg", "O"],
            "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            "lattice": null,
            "periodic_axes": [false, false, false]
          },
          "backend_run_dir": null,
          "primary_output_path": null
        }
      },
      {
        "member_id": 1,
        "origin": "CROSSO",
        "occurrences": 1,
        "source": {
          "label": "cross_0",
          "species": ["Mg", "O"],
          "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
          "lattice": null,
          "periodic_axes": [false, false, false]
        },
        "evaluation": {
          "label": "cross_0_relaxed",
          "energy": -9.8,
          "converged": true,
          "structure": {
            "label": "cross_0_relaxed",
            "species": ["Mg", "O"],
            "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            "lattice": null,
            "periodic_axes": [false, false, false]
          },
          "backend_run_dir": null,
          "primary_output_path": null
        }
      },
      {
        "member_id": 2,
        "origin": "MUTATE",
        "occurrences": 3,
        "source": {
          "label": "mut_0",
          "species": ["Mg", "O"],
          "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
          "lattice": null,
          "periodic_axes": [false, false, false]
        },
        "evaluation": {
          "label": "mut_0_relaxed",
          "energy": -9.4,
          "converged": true,
          "structure": {
            "label": "mut_0_relaxed",
            "species": ["Mg", "O"],
            "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            "lattice": null,
            "periodic_axes": [false, false, false]
          },
          "backend_run_dir": null,
          "primary_output_path": null
        }
      },
      {
        "member_id": 3,
        "origin": "REPOPR",
        "occurrences": 4,
        "source": {
          "label": "repop_0",
          "species": ["Mg", "O"],
          "fractional_coords": [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
          "lattice": null,
          "periodic_axes": [false, false, false]
        },
        "evaluation": {
          "label": "repop_0_relaxed",
          "energy": 1.7976931348623157e308,
          "converged": false,
          "structure": {
            "label": "repop_0_relaxed",
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

        let report = run_energy_histogram_workflow(&GaEnergyHistogramConfig {
            run_dir: run_dir.clone(),
            bin_count: 2,
        })
        .expect("energy histogram");

        assert_eq!(report.population_size, 4);
        assert_eq!(report.converged_population_size, 3);
        assert_eq!(report.unconverged_population_size, 1);
        assert_eq!(report.min_energy, Some(-10.0));
        assert_eq!(report.max_energy, Some(-9.4));
        assert_eq!(report.bins.len(), 2);
        assert_eq!(report.bins.iter().map(|bin| bin.count).sum::<usize>(), 3);
        assert_eq!(report.origins.len(), 4);
        assert_eq!(report.origins[0].origin, "CROSSO");
    }
}
