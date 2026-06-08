use anyhow::{anyhow, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn write_backend_campaign_artifacts(
    run_dir: &Path,
    system: &str,
    candidate_paths: &[PathBuf],
    backend: crate::EvalBackendKind,
    runner_mode: crate::RunnerMode,
    atoms_in_template: Option<&Path>,
    result: &crate::application::report_types::CampaignResult,
) -> Result<()> {
    let traces_dir = run_dir.join("traces");
    let outputs_dir = run_dir.join("outputs");
    let raw_dir = run_dir.join("raw");
    let structures_root = outputs_dir.join("structures");
    fs::create_dir_all(&traces_dir).with_context(|| {
        format!(
            "failed to create traces directory `{}`",
            traces_dir.display()
        )
    })?;
    fs::create_dir_all(&outputs_dir).with_context(|| {
        format!(
            "failed to create outputs directory `{}`",
            outputs_dir.display()
        )
    })?;
    fs::create_dir_all(&raw_dir)
        .with_context(|| format!("failed to create raw directory `{}`", raw_dir.display()))?;
    fs::create_dir_all(&structures_root).with_context(|| {
        format!(
            "failed to create structures directory `{}`",
            structures_root.display()
        )
    })?;

    let candidate_inputs_dir = raw_dir.join("candidate_inputs");
    fs::create_dir_all(&candidate_inputs_dir).with_context(|| {
        format!(
            "failed to create candidate input directory `{}`",
            candidate_inputs_dir.display()
        )
    })?;
    for (index, path) in candidate_paths.iter().enumerate() {
        let target = candidate_inputs_dir.join(format!("candidate_{index:04}.json"));
        fs::copy(path, &target).with_context(|| {
            format!(
                "failed to copy candidate input `{}` to `{}`",
                path.display(),
                target.display()
            )
        })?;
    }

    let manifest = serde_json::json!({
        "run_name": run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("unnamed-campaign"),
        "workflow_owner": "rust_evaluator_campaign",
        "workflow_scope": "tracked evaluator campaign only; native SCOTT GA/BH remains separate",
        "generation_mode": "static_replay_of_base_candidates",
        "generation_propagation": "none",
        "system": system,
        "backend": backend,
        "runner_mode": runner_mode,
        "workers": result.summary.workers,
        "generations": result.summary.generations,
        "candidate_count": result.summary.candidate_count,
        "backend_metadata": result.summary.backend_metadata,
        "artifacts": {
            "generation_metrics": "traces/generation_metrics.csv",
            "campaign_summary": "raw/campaign_summary.json",
            "generation_responses": "raw/generation_XXXX_responses.json",
            "topology_profiles": "raw/topology_profiles.json",
            "candidate_inputs": "raw/candidate_inputs/",
            "structures": "outputs/structures/"
        }
    });
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).context("failed to serialize campaign manifest")?,
    )
    .with_context(|| {
        format!(
            "failed to write campaign manifest in `{}`",
            run_dir.display()
        )
    })?;

    fs::write(
        raw_dir.join("campaign_summary.json"),
        serde_json::to_string_pretty(result).context("failed to serialize campaign summary")?,
    )
    .with_context(|| {
        format!(
            "failed to write campaign_summary.json in `{}`",
            raw_dir.display()
        )
    })?;

    let atom_specs = atoms_in_template
        .map(crate::application::scott_topology_export::parse_atoms_file)
        .transpose()?;
    let mut topology_profile_rows = Vec::new();

    let metrics_path = traces_dir.join("generation_metrics.csv");
    let mut metrics = fs::File::create(&metrics_path)
        .with_context(|| format!("failed to create `{}`", metrics_path.display()))?;
    writeln!(
        metrics,
        "generation,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,unique_species_profiles,unique_edge_profiles,unique_coordination_profiles"
    )
    .context("failed to write campaign generation metrics header")?;

    for generation in 0..result.summary.generations {
        let generation_responses = result
            .summary
            .generation_telemetry
            .get(generation)
            .ok_or_else(|| anyhow!("missing generation telemetry for generation {generation}"))?;
        let responses_path = raw_dir.join(format!("generation_{generation:04}_responses.json"));
        let structures_dir = structures_root.join(format!("{generation:04}"));
        fs::create_dir_all(&structures_dir).with_context(|| {
            format!(
                "failed to create generation structures directory `{}`",
                structures_dir.display()
            )
        })?;
        let generation_rows = result
            .generation_responses
            .get(generation)
            .cloned()
            .ok_or_else(|| anyhow!("missing generation responses for generation {generation}"))?;

        fs::write(
            &responses_path,
            serde_json::to_string_pretty(&generation_rows)
                .context("failed to serialize generation responses")?,
        )
        .with_context(|| {
            format!(
                "failed to write generation responses in `{}`",
                responses_path.display()
            )
        })?;

        let mut species_profiles = BTreeMap::<String, usize>::new();
        let mut edge_profiles = BTreeMap::<String, usize>::new();
        let mut coordination_profiles = BTreeMap::<String, usize>::new();

        for response in &generation_rows {
            if let patina_types::WorkerOutcome::Success { result } = &response.outcome {
                let xyz_path = structures_dir.join(format!("{}.xyz", response.request_id));
                write_candidate_xyz_file(&result.relaxed_candidate, &xyz_path)?;
                if let Some(specs) = atom_specs.as_deref() {
                    let summary =
                        crate::application::scott_topology_export::build_structure_summary(
                            &xyz_path, specs, "IR", 0.4,
                        )?;
                    let species_key = serde_json::to_string(&summary.species_counts)
                        .context("failed to encode species profile")?;
                    let edge_key = serde_json::to_string(&summary.edge_counts_by_pair)
                        .context("failed to encode edge profile")?;
                    let coordination_key = serde_json::to_string(&summary.coordination_histograms)
                        .context("failed to encode coordination profile")?;
                    *species_profiles.entry(species_key).or_insert(0) += 1;
                    *edge_profiles.entry(edge_key).or_insert(0) += 1;
                    *coordination_profiles.entry(coordination_key).or_insert(0) += 1;
                    topology_profile_rows.push(serde_json::json!({
                        "generation": generation,
                        "request_id": response.request_id,
                        "energy": result.energy,
                        "species_profile": summary.species_counts,
                        "edge_profile": summary.edge_counts_by_pair,
                        "coordination_profile": summary.coordination_histograms,
                        "radius": summary.radius
                    }));
                }
            }
        }

        let artifact = result
            .generations
            .get(generation)
            .ok_or_else(|| anyhow!("missing campaign artifact for generation {generation}"))?;
        writeln!(
            metrics,
            "{},{:.6},{},{},{},{},{},{},{},{},{},{}",
            generation,
            generation_responses.elapsed_secs,
            generation_responses.request_count,
            generation_responses.success_count,
            generation_responses.failure_count,
            artifact.converged_count,
            format_optional_f64(artifact.best_energy),
            format_optional_f64(artifact.mean_energy),
            format_optional_f64(artifact.worst_energy),
            optional_usize_csv(species_profiles.len()),
            optional_usize_csv(edge_profiles.len()),
            optional_usize_csv(coordination_profiles.len()),
        )
        .context("failed to write campaign generation metrics row")?;
    }

    fs::write(
        raw_dir.join("topology_profiles.json"),
        serde_json::to_string_pretty(&topology_profile_rows)
            .context("failed to serialize topology profile rows")?,
    )
    .with_context(|| {
        format!(
            "failed to write topology_profiles.json in `{}`",
            raw_dir.display()
        )
    })?;

    Ok(())
}

fn write_candidate_xyz_file(candidate: &patina_types::Candidate, output_path: &Path) -> Result<()> {
    let mut rendered = String::new();
    rendered.push_str(&format!("{}\n", candidate.species.len()));
    rendered.push_str(&format!("label={}\n", candidate.label));
    for (species, coords) in candidate
        .species
        .iter()
        .zip(candidate.fractional_coords.iter())
    {
        rendered.push_str(&format!(
            "{} {:.10} {:.10} {:.10}\n",
            species, coords[0], coords[1], coords[2]
        ));
    }
    fs::write(output_path, rendered)
        .with_context(|| format!("failed to write `{}`", output_path.display()))
}

fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|inner| format!("{inner:.10}"))
        .unwrap_or_default()
}

fn optional_usize_csv(value: usize) -> String {
    if value == 0 {
        String::new()
    } else {
        value.to_string()
    }
}
