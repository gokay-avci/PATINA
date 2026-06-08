use crate::{
    ClusterPerturbationEngine, ClusterStructure, DefaultClusterPerturbationEngine,
    OverlapMatrixFingerprintEngine, PerturbationConfig, StructureFingerprintEngine,
};
use anyhow::{bail, Result};
use patina_dreadnaut::{
    build_dreadnaut_graph_text, build_graph, canonical_hashkey_from_graph_text,
    compute_hashkey_radius, graph_edit_distance, graph_summary, pair_cutoff_margins,
    resolve_dreadnaut_path, AtomSpec, GraphSummary,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutoffMarginSummary {
    pub min_abs_margin: f64,
    pub q10_abs_margin: f64,
    pub q50_abs_margin: f64,
    pub q90_abs_margin: f64,
    pub near_critical_pair_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologySweepBase {
    pub radius: f64,
    pub hashkey: String,
    pub graph_summary: GraphSummary,
    pub margin_summary: CutoffMarginSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerturbationTopologyObservation {
    pub sigma: f64,
    pub repeat: usize,
    pub fingerprint_distance: f64,
    pub drms: f64,
    pub hashkey_same: bool,
    pub edge_additions: usize,
    pub edge_deletions: usize,
    pub crossed_margin_pair_count: usize,
    pub min_abs_margin_base: f64,
    pub min_abs_margin_perturbed: f64,
    pub base_hashkey: String,
    pub perturbed_hashkey: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerturbationTopologySweep {
    pub base: TopologySweepBase,
    pub observations: Vec<PerturbationTopologyObservation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerturbationTopologySweepRequest {
    pub template_config: PerturbationConfig,
    pub sigmas: Vec<f64>,
    pub repeats_per_sigma: usize,
    pub atom_specs: Vec<AtomSpec>,
    pub radius_mode: String,
    pub radius_const: f64,
    pub include_p_orbitals: bool,
    pub margin_epsilon: f64,
    pub dreadnaut_path: Option<PathBuf>,
}

pub fn perturbation_topology_sweep(
    source: &ClusterStructure,
    request: &PerturbationTopologySweepRequest,
) -> Result<PerturbationTopologySweep> {
    source
        .validate()
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    if request.sigmas.is_empty() {
        bail!("topology sweep requires at least one sigma");
    }
    if request.repeats_per_sigma == 0 {
        bail!("topology sweep requires repeats_per_sigma > 0");
    }

    let dreadnaut_path = resolve_dreadnaut_path(request.dreadnaut_path.as_deref())?;
    let source_candidate = patina_types::Candidate::from(source);
    let base_radius = compute_hashkey_radius(
        &source_candidate,
        &request.atom_specs,
        &request.radius_mode,
        request.radius_const,
    )?;
    let base_graph = build_graph(&source_candidate, base_radius, &request.atom_specs);
    let base_graph_text =
        build_dreadnaut_graph_text(&source_candidate, base_radius, &request.atom_specs);
    let base_hashkey = canonical_hashkey_from_graph_text(&dreadnaut_path, &base_graph_text)?;
    let base_margins = pair_cutoff_margins(&source_candidate, base_radius);
    let base_margin_summary = summarize_margins(&base_margins, request.margin_epsilon);
    let base_summary = graph_summary(&base_graph);
    let fingerprint_engine = OverlapMatrixFingerprintEngine {
        include_p_orbitals: request.include_p_orbitals,
    };
    let base_fingerprint = fingerprint_engine
        .fingerprint(source)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    let mut observations = Vec::new();
    for (sigma_index, sigma) in request.sigmas.iter().copied().enumerate() {
        for repeat in 0..request.repeats_per_sigma {
            let mut config = request.template_config;
            config.sigma = sigma;
            config.seed = config
                .seed
                .map(|seed| seed ^ ((sigma_index as u64) << 32) ^ repeat as u64);
            let batch = DefaultClusterPerturbationEngine
                .generate(source, config, 1)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            let perturbed = batch.variants.into_iter().next().expect("single variant");
            let perturbed_fingerprint = fingerprint_engine
                .fingerprint(&perturbed)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            let fingerprint_distance =
                euclidean_distance(&base_fingerprint.values, &perturbed_fingerprint.values)?;

            let perturbed_candidate = patina_types::Candidate::from(&perturbed);
            let perturbed_radius = compute_hashkey_radius(
                &perturbed_candidate,
                &request.atom_specs,
                &request.radius_mode,
                request.radius_const,
            )?;
            let perturbed_graph =
                build_graph(&perturbed_candidate, perturbed_radius, &request.atom_specs);
            let perturbed_graph_text = build_dreadnaut_graph_text(
                &perturbed_candidate,
                perturbed_radius,
                &request.atom_specs,
            );
            let perturbed_hashkey =
                canonical_hashkey_from_graph_text(&dreadnaut_path, &perturbed_graph_text)?;
            let diff = graph_edit_distance(&base_graph, &perturbed_graph)?;
            let perturbed_margins = pair_cutoff_margins(&perturbed_candidate, perturbed_radius);
            let perturbed_margin_summary =
                summarize_margins(&perturbed_margins, request.margin_epsilon);
            let crossed_margin_pair_count = base_margins
                .iter()
                .zip(perturbed_margins.iter())
                .filter(|(base, perturbed)| base.is_edge != perturbed.is_edge)
                .count();

            observations.push(PerturbationTopologyObservation {
                sigma,
                repeat,
                fingerprint_distance,
                drms: distance_rms(source, &perturbed)?,
                hashkey_same: base_hashkey == perturbed_hashkey,
                edge_additions: diff.edge_additions.len(),
                edge_deletions: diff.edge_deletions.len(),
                crossed_margin_pair_count,
                min_abs_margin_base: base_margin_summary.min_abs_margin,
                min_abs_margin_perturbed: perturbed_margin_summary.min_abs_margin,
                base_hashkey: base_hashkey.clone(),
                perturbed_hashkey,
            });
        }
    }

    Ok(PerturbationTopologySweep {
        base: TopologySweepBase {
            radius: base_radius,
            hashkey: base_hashkey,
            graph_summary: base_summary,
            margin_summary: base_margin_summary,
        },
        observations,
    })
}

pub fn topology_sweep_csv(sweep: &PerturbationTopologySweep) -> String {
    let mut out = String::from(
        "sigma,repeat,fingerprint_distance,drms,hashkey_same,edge_additions,edge_deletions,crossed_margin_pair_count,min_abs_margin_base,min_abs_margin_perturbed,base_hashkey,perturbed_hashkey\n",
    );
    for obs in &sweep.observations {
        out.push_str(&format!(
            "{:.16e},{},{:.16e},{:.16e},{},{},{},{},{:.16e},{:.16e},{},{}\n",
            obs.sigma,
            obs.repeat,
            obs.fingerprint_distance,
            obs.drms,
            obs.hashkey_same,
            obs.edge_additions,
            obs.edge_deletions,
            obs.crossed_margin_pair_count,
            obs.min_abs_margin_base,
            obs.min_abs_margin_perturbed,
            obs.base_hashkey,
            obs.perturbed_hashkey
        ));
    }
    out
}

fn summarize_margins(
    margins: &[patina_dreadnaut::PairMargin],
    epsilon: f64,
) -> CutoffMarginSummary {
    let mut values = margins
        .iter()
        .map(|margin| margin.margin.abs())
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let quantile = |q: f64| -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let index = ((values.len() - 1) as f64 * q).round() as usize;
        values[index]
    };
    CutoffMarginSummary {
        min_abs_margin: values.first().copied().unwrap_or(0.0),
        q10_abs_margin: quantile(0.10),
        q50_abs_margin: quantile(0.50),
        q90_abs_margin: quantile(0.90),
        near_critical_pair_count: values.iter().filter(|value| **value <= epsilon).count(),
    }
}

fn euclidean_distance(left: &[f64], right: &[f64]) -> Result<f64> {
    if left.len() != right.len() {
        bail!(
            "fingerprint dimensionality mismatch: {} vs {}",
            left.len(),
            right.len()
        );
    }
    Ok(left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| {
            let delta = a - b;
            delta * delta
        })
        .sum::<f64>()
        .sqrt())
}

fn distance_rms(left: &ClusterStructure, right: &ClusterStructure) -> Result<f64> {
    if left.atoms.len() != right.atoms.len() {
        bail!(
            "atom count mismatch for dRMS: {} vs {}",
            left.atoms.len(),
            right.atoms.len()
        );
    }
    let mut count = 0usize;
    let mut acc = 0.0_f64;
    for i in 0..left.atoms.len() {
        let li = &left.atoms[i].cartesian;
        let ri = &right.atoms[i].cartesian;
        for j in (i + 1)..left.atoms.len() {
            let lj = &left.atoms[j].cartesian;
            let rj = &right.atoms[j].cartesian;
            let left_distance =
                ((li[0] - lj[0]).powi(2) + (li[1] - lj[1]).powi(2) + (li[2] - lj[2]).powi(2))
                    .sqrt();
            let right_distance =
                ((ri[0] - rj[0]).powi(2) + (ri[1] - rj[1]).powi(2) + (ri[2] - rj[2]).powi(2))
                    .sqrt();
            let delta = left_distance - right_distance;
            acc += delta * delta;
            count += 1;
        }
    }
    if count == 0 {
        Ok(0.0)
    } else {
        Ok((acc / count as f64).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        perturbation_topology_sweep, topology_sweep_csv, PerturbationTopologySweepRequest,
    };
    use crate::{ClusterAtom, ClusterStructure, PerturbationConfig};
    use patina_dreadnaut::AtomSpec;

    #[cfg(unix)]
    fn write_fake_dreadnaut(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("fake_dreadnaut.sh");
        std::fs::write(&path, "#!/bin/sh\ncat >/dev/null\nprintf '[1 2]\\n'\n")
            .expect("write fake dreadnaut");
        let mut permissions = std::fs::metadata(&path)
            .expect("fake dreadnaut metadata")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod fake dreadnaut");
        path
    }

    fn source() -> ClusterStructure {
        ClusterStructure {
            label: "cluster".into(),
            atoms: vec![
                ClusterAtom {
                    species: "Mg".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [1.5, 0.0, 0.0],
                },
            ],
        }
    }

    fn atom_specs() -> Vec<AtomSpec> {
        vec![
            AtomSpec {
                species: "O".into(),
                covalent_radius: 0.66,
                ionic_radius: 1.4,
            },
            AtomSpec {
                species: "Mg".into(),
                covalent_radius: 1.41,
                ionic_radius: 0.86,
            },
        ]
    }

    #[cfg(unix)]
    #[test]
    fn topology_sweep_reports_single_observation_for_single_sigma_repeat() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let sweep = perturbation_topology_sweep(
            &source(),
            &PerturbationTopologySweepRequest {
                template_config: PerturbationConfig {
                    max_displacement: Some(0.0),
                    seed: Some(11),
                    ..PerturbationConfig::default()
                },
                sigmas: vec![0.05],
                repeats_per_sigma: 1,
                atom_specs: atom_specs(),
                radius_mode: "IR".into(),
                radius_const: 0.4,
                include_p_orbitals: false,
                margin_epsilon: 0.1,
                dreadnaut_path: Some(write_fake_dreadnaut(tempdir.path())),
            },
        )
        .expect("sweep");
        assert_eq!(sweep.observations.len(), 1);
        assert!(sweep.observations[0].hashkey_same);
        assert_eq!(sweep.observations[0].edge_additions, 0);
        assert_eq!(sweep.observations[0].edge_deletions, 0);
        let csv = topology_sweep_csv(&sweep);
        assert!(csv.starts_with("sigma,repeat,"));
    }
}
