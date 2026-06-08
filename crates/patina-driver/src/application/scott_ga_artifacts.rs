use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GaGenerationSummary {
    pub generation: usize,
    pub best_energy: f64,
    pub mean_energy: f64,
    pub worst_energy: f64,
    pub n_converged: usize,
    pub n_failed: usize,
    pub population_size: usize,
    pub best_cluster_id: String,
    pub best_origin: String,
    pub unique_relaxed_hashkeys: usize,
    pub unique_origins: usize,
}

pub fn collect_ga_statistics_files(run_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if !run_dir.exists() {
        return Ok(found);
    }
    collect_ga_statistics_files_recursive(run_dir, &mut found)?;
    found.sort();
    Ok(found)
}

pub fn summarize_ga_statistics(path: &Path) -> Result<Option<GaGenerationSummary>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut energy_values = Vec::new();
    let mut origins = BTreeSet::new();
    let mut relaxed_hashkeys = BTreeSet::new();
    let mut population_size = 0usize;
    let mut n_failed = 0usize;
    let mut generation = ga_generation_from_filename(path);
    let mut best_energy = f64::INFINITY;
    let mut best_cluster_id = String::new();
    let mut best_origin = String::new();

    for (idx, line) in content.lines().enumerate() {
        if idx == 0 || line.trim().is_empty() {
            continue;
        }
        population_size += 1;
        let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 17 {
            continue;
        }
        let edefn = parts[3].parse::<i32>().ok();
        let status = parts[4];
        let status_code = status.parse::<i32>().ok();
        let cluster_id = parts[1].to_string();
        let relaxed_hashkey = parts[2].to_string();
        let origin = parts[14].to_string();
        if generation.is_none() {
            if let Ok(gen_no) = parts[15].parse::<usize>() {
                generation = Some(gen_no);
            }
        }
        if !origin.is_empty() {
            origins.insert(origin.clone());
        }
        let failed = matches!(edefn, Some(0))
            || matches!(status_code, Some(0))
            || status.eq_ignore_ascii_case("failed");
        if failed {
            n_failed += 1;
            continue;
        }
        if !relaxed_hashkey.is_empty() && !relaxed_hashkey.eq_ignore_ascii_case("UNDEFINED_HASHKEY")
        {
            relaxed_hashkeys.insert(relaxed_hashkey);
        }
        match parts[5].parse::<f64>() {
            Ok(energy) => {
                if energy < best_energy {
                    best_energy = energy;
                    best_cluster_id = cluster_id;
                    best_origin = origin;
                }
                energy_values.push(energy)
            }
            Err(_) => continue,
        }
    }

    if energy_values.is_empty() {
        return Ok(None);
    }

    let worst_energy = energy_values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let mean_energy = energy_values.iter().sum::<f64>() / energy_values.len() as f64;

    Ok(Some(GaGenerationSummary {
        generation: generation.unwrap_or(0),
        best_energy,
        mean_energy,
        worst_energy,
        n_converged: population_size.saturating_sub(n_failed),
        n_failed,
        population_size,
        best_cluster_id,
        best_origin,
        unique_relaxed_hashkeys: relaxed_hashkeys.len(),
        unique_origins: origins.len(),
    }))
}

pub fn copy_ga_statistics(run_dir: &Path, raw_dir: &Path) -> Result<()> {
    for path in collect_ga_statistics_files(run_dir)? {
        let Some(file_name) = path.file_name() else {
            continue;
        };
        fs::copy(&path, raw_dir.join(file_name)).with_context(|| {
            format!(
                "failed to copy GA statistics `{}` into `{}`",
                path.display(),
                raw_dir.display()
            )
        })?;
    }
    Ok(())
}

pub fn copy_ga_population_snapshots(run_output_dir: &Path, snapshots_dir: &Path) -> Result<()> {
    copy_ga_population_snapshots_inner(run_output_dir, run_output_dir, snapshots_dir, false)
}

fn collect_ga_statistics_files_recursive(dir: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read `{}`", dir.display()))? {
        let entry = entry.with_context(|| {
            format!("failed to read directory entry inside `{}`", dir.display())
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_ga_statistics_files_recursive(&path, found)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("gaStatistics") && name.ends_with(".csv") {
            found.push(path);
        }
    }
    Ok(())
}

fn copy_ga_population_snapshots_inner(
    root: &Path,
    current: &Path,
    snapshots_dir: &Path,
    inside_pop: bool,
) -> Result<()> {
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
            let next_inside_pop =
                inside_pop || path.file_name().and_then(|name| name.to_str()) == Some("POP");
            copy_ga_population_snapshots_inner(root, &path, snapshots_dir, next_inside_pop)?;
            continue;
        }

        if !inside_pop || !is_structure_snapshot(&path) {
            continue;
        }

        let Some(relative) = path.strip_prefix(root).ok() else {
            continue;
        };
        let target = snapshots_dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create `{}`", parent.display()))?;
        }
        fs::copy(&path, &target).with_context(|| {
            format!(
                "failed to copy GA population snapshot `{}` to `{}`",
                path.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

fn ga_generation_from_filename(path: &Path) -> Option<usize> {
    let stem = path.file_stem()?.to_str()?;
    let suffix = stem.strip_prefix("gaStatistics")?;
    suffix.parse::<usize>().ok()
}

fn is_structure_snapshot(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "xyz" | "car"))
        .unwrap_or(false)
}
