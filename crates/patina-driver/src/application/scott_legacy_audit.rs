use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct RunJobOverrideRecord {
    pub value: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunJobPatchSummary {
    pub overrides: BTreeMap<String, RunJobOverrideRecord>,
}

#[derive(Debug)]
struct TopStructureRow {
    label: String,
    energy: f64,
}

#[derive(Debug)]
pub struct JanusGenerationSummary {
    pub generation: usize,
    pub request_count: usize,
    pub converged_count: usize,
    pub nonconverged_count: usize,
    pub mean_energy: f64,
    pub best_energy: f64,
    pub mean_max_force: f64,
    pub max_max_force: f64,
    pub mean_steps: f64,
    pub max_steps: usize,
}

#[derive(Debug, Default, Clone)]
pub struct HashkeyGenerationSummary {
    pub generation: usize,
    pub duplicate_hashkey: usize,
    pub duplicate_pmoi: usize,
    pub duplicate_energy_tol: usize,
    pub initial_duplicate: usize,
    pub toplist_match: usize,
    pub blacklist_match: usize,
    pub undefined_hashkey: usize,
}

pub fn sanitize_csv_field(value: &str) -> String {
    value.replace(',', ";")
}

pub fn parse_top_structure_statistics(path: &Path) -> Result<Vec<(String, f64)>> {
    Ok(parse_top_structure_statistics_rows(path)?
        .into_iter()
        .map(|row| (row.label, row.energy))
        .collect())
}

pub fn parse_top_structure_energies(path: &Path) -> Result<Vec<(String, f64)>> {
    Ok(parse_top_structure_energy_rows(path)?
        .into_iter()
        .map(|row| (row.label, row.energy))
        .collect())
}

pub fn recover_scott_energy_snapshot(workdir: &Path) -> Result<Option<(String, f64)>> {
    let candidates = [
        ("A0", workdir.join("run").join("0").join("A0_save.gout")),
        ("A1", workdir.join("run").join("A1_save.gout")),
        ("A1", workdir.join("run").join("0").join("A1_save.gout")),
        ("gulp_klmc", workdir.join("run").join("gulp_klmc.gout")),
        (
            "gulp_klmc",
            workdir.join("run").join("0").join("gulp_klmc.gout"),
        ),
    ];

    for (label, path) in candidates {
        if !path.exists() {
            continue;
        }
        match patina_external::GotParser::parse_file(&path) {
            Ok(result) => return Ok(Some((label.to_string(), result.energy))),
            Err(_) => {
                if let Some(energy) = scan_final_energy_from_gout(&path)? {
                    return Ok(Some((label.to_string(), energy)));
                }
            }
        }
    }

    Ok(None)
}

pub fn export_janus_generation_traces(request: &crate::ResolvedScottSearchRun) -> Result<()> {
    if request.backend.evaluator_backend.as_deref() != Some("JANUS_EXTERNAL") {
        return Ok(());
    }

    let rows = collect_janus_generation_summaries(&request.workdir.join("run"))?;
    if rows.is_empty() {
        return Ok(());
    }

    let target = request
        .run_dir
        .join("traces")
        .join("janus_generation_metrics.csv");
    let mut file = fs::File::create(&target)
        .with_context(|| format!("failed to create `{}`", target.display()))?;
    writeln!(
        file,
        "generation,request_count,converged_count,nonconverged_count,mean_energy,best_energy,mean_max_force,max_max_force,mean_steps,max_steps"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{:.10},{:.10},{:.10},{:.10},{:.6},{}",
            row.generation,
            row.request_count,
            row.converged_count,
            row.nonconverged_count,
            row.mean_energy,
            row.best_energy,
            row.mean_max_force,
            row.max_max_force,
            row.mean_steps,
            row.max_steps
        )?;
    }
    Ok(())
}

pub fn export_hashkey_generation_traces(request: &crate::ResolvedScottSearchRun) -> Result<()> {
    let audit_path = request.workdir.join("hashkey_audit_events.csv");
    if !audit_path.exists() {
        return Ok(());
    }
    let rows = collect_hashkey_generation_summaries(&audit_path)?;
    if rows.is_empty() {
        return Ok(());
    }
    let target = request
        .run_dir
        .join("traces")
        .join("hashkey_generation_metrics.csv");
    let mut file = fs::File::create(&target)
        .with_context(|| format!("failed to create `{}`", target.display()))?;
    writeln!(
        file,
        "generation,duplicate_hashkey,duplicate_pmoi,duplicate_energy_tol,initial_duplicate,toplist_match,blacklist_match,undefined_hashkey"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{},{},{},{}",
            row.generation,
            row.duplicate_hashkey,
            row.duplicate_pmoi,
            row.duplicate_energy_tol,
            row.initial_duplicate,
            row.toplist_match,
            row.blacklist_match,
            row.undefined_hashkey
        )?;
    }
    Ok(())
}

fn parse_top_structure_statistics_rows(path: &Path) -> Result<Vec<TopStructureRow>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if idx == 0 || line.trim().is_empty() {
            continue;
        }
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        if parts[1].eq_ignore_ascii_case("x") {
            continue;
        }
        let Some(energy) = parts[2].parse::<f64>().ok() else {
            continue;
        };
        rows.push(TopStructureRow {
            label: parts[1].to_string(),
            energy,
        });
    }
    Ok(rows)
}

fn parse_top_structure_energy_rows(path: &Path) -> Result<Vec<TopStructureRow>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in content.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 {
            continue;
        }
        let Some(energy) = parts.last().and_then(|value| value.parse::<f64>().ok()) else {
            continue;
        };
        rows.push(TopStructureRow {
            label: parts[0].to_string(),
            energy,
        });
    }
    Ok(rows)
}

fn collect_janus_generation_summaries(run_dir: &Path) -> Result<Vec<JanusGenerationSummary>> {
    let mut per_generation: BTreeMap<usize, Vec<(f64, bool, f64, usize)>> = BTreeMap::new();
    if !run_dir.exists() {
        return Ok(Vec::new());
    }
    for generation_entry in
        fs::read_dir(run_dir).with_context(|| format!("failed to read `{}`", run_dir.display()))?
    {
        let generation_entry = generation_entry?;
        let generation_path = generation_entry.path();
        if !generation_path.is_dir() {
            continue;
        }
        let Some(generation_name) = generation_path.file_name().and_then(|name| name.to_str())
        else {
            continue;
        };
        let Ok(generation) = generation_name.parse::<usize>() else {
            continue;
        };
        for entry in fs::read_dir(&generation_path)
            .with_context(|| format!("failed to read `{}`", generation_path.display()))?
        {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.ends_with("_janus_summary.txt") {
                continue;
            }
            if let Some(row) = parse_janus_summary_file(&path)? {
                per_generation.entry(generation).or_default().push(row);
            }
        }
    }

    let mut out = Vec::new();
    for (generation, rows) in per_generation {
        if rows.is_empty() {
            continue;
        }
        let request_count = rows.len();
        let converged_count = rows
            .iter()
            .filter(|(_, converged, _, _)| *converged)
            .count();
        let nonconverged_count = request_count.saturating_sub(converged_count);
        let mean_energy =
            rows.iter().map(|(energy, _, _, _)| *energy).sum::<f64>() / request_count as f64;
        let best_energy = rows
            .iter()
            .map(|(energy, _, _, _)| *energy)
            .fold(f64::INFINITY, f64::min);
        let mean_max_force = rows
            .iter()
            .map(|(_, _, max_force, _)| *max_force)
            .sum::<f64>()
            / request_count as f64;
        let max_max_force = rows
            .iter()
            .map(|(_, _, max_force, _)| *max_force)
            .fold(f64::NEG_INFINITY, f64::max);
        let mean_steps = rows
            .iter()
            .map(|(_, _, _, n_steps)| *n_steps as f64)
            .sum::<f64>()
            / request_count as f64;
        let max_steps = rows
            .iter()
            .map(|(_, _, _, n_steps)| *n_steps)
            .max()
            .unwrap_or(0);
        out.push(JanusGenerationSummary {
            generation,
            request_count,
            converged_count,
            nonconverged_count,
            mean_energy,
            best_energy,
            mean_max_force,
            max_max_force,
            mean_steps,
            max_steps,
        });
    }
    Ok(out)
}

fn parse_janus_summary_file(path: &Path) -> Result<Option<(f64, bool, f64, usize)>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut energy = None;
    let mut converged = None;
    let mut max_force = None;
    let mut n_steps = None;
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "energy" => energy = value.trim().parse::<f64>().ok(),
            "converged" => converged = Some(value.trim().eq_ignore_ascii_case("true")),
            "max_force" => max_force = value.trim().parse::<f64>().ok(),
            "n_steps" => n_steps = value.trim().parse::<usize>().ok(),
            _ => {}
        }
    }
    Ok(match (energy, converged, max_force, n_steps) {
        (Some(energy), Some(converged), Some(max_force), Some(n_steps)) => {
            Some((energy, converged, max_force, n_steps))
        }
        _ => None,
    })
}

fn collect_hashkey_generation_summaries(path: &Path) -> Result<Vec<HashkeyGenerationSummary>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut by_generation: BTreeMap<usize, HashkeyGenerationSummary> = BTreeMap::new();
    for (idx, line) in content.lines().enumerate() {
        if idx == 0 || line.trim().is_empty() {
            continue;
        }
        let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let Some(generation) = parts[0].parse::<usize>().ok() else {
            continue;
        };
        let row = by_generation
            .entry(generation)
            .or_insert_with(|| HashkeyGenerationSummary {
                generation,
                ..HashkeyGenerationSummary::default()
            });
        match (parts[1], parts[2]) {
            ("duplicate", "HASHKEY") => row.duplicate_hashkey += 1,
            ("duplicate", "PMOI") => row.duplicate_pmoi += 1,
            ("duplicate", "ENERGY_TOL") => row.duplicate_energy_tol += 1,
            ("initial_duplicate", _) => row.initial_duplicate += 1,
            ("topology_match", "TOPLIST") => row.toplist_match += 1,
            ("topology_match", "BLACKLIST") => row.blacklist_match += 1,
            ("undefined_hashkey", _) => row.undefined_hashkey += 1,
            _ => {}
        }
    }
    Ok(by_generation.into_values().collect())
}

fn scan_final_energy_from_gout(path: &Path) -> Result<Option<f64>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Final energy =") {
            continue;
        }
        let Some(value) = trimmed
            .split('=')
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
        else {
            continue;
        };
        let Some(energy) = value.parse::<f64>().ok() else {
            continue;
        };
        return Ok(Some(energy));
    }
    Ok(None)
}
