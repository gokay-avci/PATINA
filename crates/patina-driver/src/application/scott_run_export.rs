use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ScottExportPaths {
    pub traces_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub structures_dir: PathBuf,
    pub ga_population_dir: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct ScottProcessSummary {
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub recovered_partial_run: bool,
}

pub fn copy_if_exists(source: &Path, target: &Path) -> Result<()> {
    if source.exists() {
        fs::copy(source, target).with_context(|| {
            format!(
                "failed to copy `{}` to `{}`",
                source.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

pub fn copy_structure_outputs(run_output_dir: &Path, structures_dir: &Path) -> Result<()> {
    copy_structure_outputs_inner(run_output_dir, run_output_dir, structures_dir)
}

pub fn prepare_scott_export_paths(run_dir: &Path) -> Result<ScottExportPaths> {
    let paths = ScottExportPaths {
        traces_dir: run_dir.join("traces"),
        raw_dir: run_dir.join("raw"),
        structures_dir: run_dir.join("outputs").join("structures"),
        ga_population_dir: run_dir.join("outputs").join("ga_population_snapshots"),
    };
    fs::create_dir_all(&paths.traces_dir)
        .with_context(|| format!("failed to create `{}`", paths.traces_dir.display()))?;
    fs::create_dir_all(&paths.raw_dir)
        .with_context(|| format!("failed to create `{}`", paths.raw_dir.display()))?;
    fs::create_dir_all(&paths.structures_dir)
        .with_context(|| format!("failed to create `{}`", paths.structures_dir.display()))?;
    fs::create_dir_all(&paths.ga_population_dir)
        .with_context(|| format!("failed to create `{}`", paths.ga_population_dir.display()))?;
    Ok(paths)
}

pub fn stage_scott_search_raw_outputs(
    request: &crate::ResolvedScottSearchRun,
    paths: &ScottExportPaths,
) -> Result<()> {
    for name in ["KLMC.log", "KLMC.err", "KLMC.out"] {
        copy_if_exists(&request.workdir.join(name), &paths.raw_dir.join(name))?;
    }
    for name in ["run.job", "Master.gin", "atoms.in"] {
        copy_if_exists(
            &request.workdir.join("data").join(name),
            &paths.raw_dir.join(name),
        )?;
    }
    for name in [
        "gulp_klmc.gin",
        "gulp_klmc.gout",
        "energy",
        "metropolis",
        "log",
        "log-mc",
        "logBest",
    ] {
        copy_if_exists(
            &request.workdir.join("run").join(name),
            &paths.raw_dir.join(name),
        )?;
    }
    for name in [
        "gulp_klmc.gin",
        "gulp_klmc.gout",
        "energy",
        "metropolis",
        "logBest",
    ] {
        copy_if_exists(
            &request.workdir.join("run").join("0").join(name),
            &paths.raw_dir.join(name),
        )?;
    }
    copy_if_exists(
        &request.workdir.join("run").join("A1_save.gout"),
        &paths.raw_dir.join("A1_save.gout"),
    )?;
    copy_if_exists(
        &request.workdir.join("run").join("0").join("A0_save.gout"),
        &paths.raw_dir.join("A0_save.gout"),
    )?;
    copy_if_exists(
        &request.workdir.join("run").join("0").join("A1_save.gout"),
        &paths.raw_dir.join("A1_save.gout"),
    )?;
    copy_structure_outputs(&request.workdir.join("run"), &paths.structures_dir)?;
    crate::application::scott_ga_artifacts::copy_ga_population_snapshots(
        &request.workdir.join("run"),
        &paths.ga_population_dir,
    )?;
    copy_if_exists(
        &request.workdir.join("GA-restart").join("statistics"),
        &paths.raw_dir.join("ga_restart_statistics"),
    )?;
    crate::application::scott_ga_artifacts::copy_ga_statistics(
        &request.workdir.join("run"),
        &paths.raw_dir,
    )?;
    copy_if_exists(
        &request.workdir.join("top_structures").join("energies"),
        &paths.raw_dir.join("top_structures_energies"),
    )?;
    copy_if_exists(
        &request.workdir.join("top_structures").join("statistics"),
        &paths.raw_dir.join("top_structures_statistics"),
    )?;
    copy_if_exists(
        &request.workdir.join("top_structures").join("hashkeys"),
        &paths.raw_dir.join("top_structures_hashkeys"),
    )?;
    copy_if_exists(
        &request.workdir.join("hashkey_audit_events.csv"),
        &paths.raw_dir.join("hashkey_audit_events.csv"),
    )?;
    copy_structure_outputs(
        &request.workdir.join("top_structures"),
        &paths.structures_dir,
    )?;
    Ok(())
}

pub fn write_scott_search_manifest(
    request: &crate::ResolvedScottSearchRun,
    patch_summary: &crate::RunJobPatchSummary,
    process: ScottProcessSummary,
) -> Result<()> {
    let provenance = infer_scott_search_provenance(request, patch_summary)?;
    let run_name = request
        .run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unnamed-run");
    let manifest = serde_json::json!({
        "run_name": run_name,
        "job_type": request.job_type.as_str(),
        "job_type_code": request.job_type.job_code(),
        "mode": request.trace_mode_label(),
        "system": request.system,
        "engine": if request.backend.evaluator_backend.as_deref() == Some("JANUS_EXTERNAL") {
            "SCOTT+JANUS_MACE"
        } else {
            "SCOTT+GULP"
        },
        "notes": "SCOTT-owned search exported by the Rust interface layer.",
        "artifact_model": {
            "structures": "Relaxed structure files outside SCOTT POP directories.",
            "ga_population_snapshots": "Files written under SCOTT POP directories, typically from writeGAPop / writePopulation paths.",
            "topology_nearness_report": "Rust-generated report of exact hashkey matches plus graph-summary nearness comparisons."
        },
        "provenance": provenance,
        "process": {
            "exit_code": process.exit_code,
            "signal": process.signal,
            "recovered_partial_run": process.recovered_partial_run
        },
        "walker_trace": "traces/walker_trace.csv",
        "mc_trace": "traces/mc_trace.csv",
        "generation_metrics": "traces/generation_metrics.csv",
        "janus_generation_metrics": "traces/janus_generation_metrics.csv",
        "hashkey_generation_metrics": "traces/hashkey_generation_metrics.csv",
        "structures_dir": "outputs/structures",
        "ga_population_snapshots_dir": "outputs/ga_population_snapshots",
        "topology_nearness_report": "raw/topology_nearness_report.json",
        "search_config": {
            "shared": request.shared,
            "backend": request.backend,
            "bh": request.bh,
            "ga": request.ga
        },
        "run_job_overrides": patch_summary
    });
    fs::write(
        request.run_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)
            .context("failed to serialize SCOTT search manifest")?,
    )
    .with_context(|| {
        format!(
            "failed to write manifest in `{}`",
            request.run_dir.display()
        )
    })?;
    Ok(())
}

pub fn infer_scott_search_provenance(
    request: &crate::ResolvedScottSearchRun,
    patch_summary: &crate::RunJobPatchSummary,
) -> Result<serde_json::Value> {
    let run_job_text =
        fs::read_to_string(request.workdir.join("data").join("run.job")).unwrap_or_default();
    let klmc_out = fs::read_to_string(request.workdir.join("KLMC.out")).unwrap_or_default();
    let klmc_log = fs::read_to_string(request.workdir.join("KLMC.log")).unwrap_or_default();

    let source_mode = match request.trace_mode {
        Some(crate::SearchMode::Ga) => infer_ga_source_mode(&klmc_out, &klmc_log),
        Some(crate::SearchMode::Bh) => infer_bh_source_mode(&klmc_out, &klmc_log),
        None => "native_entrypoint_only",
    };

    let seed_line = run_job_text
        .lines()
        .find(|line| line.trim_start().starts_with("SEED:"))
        .map(str::trim)
        .unwrap_or("SEED:unknown");

    Ok(serde_json::json!({
        "source_mode": source_mode,
        "seed_control": seed_line,
        "scott_bin": request.scott_bin,
        "data_dir": request.data_dir,
        "workdir": request.workdir,
        "config_path": request.config_path,
        "cli_overrides": request.cli_overrides,
        "override_sources": patch_summary.overrides,
        "copied_inputs": {
            "run_job": "raw/run.job",
            "master_gin": "raw/Master.gin",
            "atoms_in": "raw/atoms.in"
        }
    }))
}

fn infer_ga_source_mode(klmc_out: &str, klmc_log: &str) -> &'static str {
    if klmc_out.contains("Seed with previous population in") {
        "restart_population"
    } else if klmc_out.contains("Seed with a predefined structure in")
        || klmc_log.contains("Seed with a predefined structure in")
    {
        "predefined_seed_structures"
    } else if klmc_out.contains("Using random data for")
        || klmc_log.contains("Data ready for candidate")
    {
        "random_initial_population"
    } else {
        "unknown"
    }
}

fn infer_bh_source_mode(klmc_out: &str, klmc_log: &str) -> &'static str {
    if klmc_out.contains("Uploading previous cluster from")
        || klmc_out.contains("Seed with a predefined structure in")
    {
        "seeded_or_restart_structure"
    } else if klmc_out.contains("Using random data for")
        || klmc_log.contains("Data ready for candidate")
    {
        "random_start"
    } else {
        "unknown"
    }
}

fn copy_structure_outputs_inner(root: &Path, current: &Path, structures_dir: &Path) -> Result<()> {
    if !current.exists() {
        return Ok(());
    }

    for entry in
        fs::read_dir(current).with_context(|| format!("failed to read `{}`", current.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry inside `{}`",
                current.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("POP") {
                continue;
            }
            copy_structure_outputs_inner(root, &path, structures_dir)?;
            continue;
        }

        if !is_structure_snapshot(&path) {
            continue;
        }

        let Some(relative) = path.strip_prefix(root).ok() else {
            continue;
        };
        let target = structures_dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create `{}`", parent.display()))?;
        }
        fs::copy(&path, &target).with_context(|| {
            format!(
                "failed to copy structure `{}` to `{}`",
                path.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

fn is_structure_snapshot(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "xyz" | "car"))
        .unwrap_or(false)
}
