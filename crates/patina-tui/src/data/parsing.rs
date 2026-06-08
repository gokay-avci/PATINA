use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::domain::bh::WalkerTraceRow;
use crate::domain::ga::{
    ControllerTraceRow, GenerationMetricRow, GenerationStateFile, GenerationSummaryRow,
};
use crate::domain::run_manifest::RunManifest;
use crate::error::TuiError;

pub fn parse_manifest(path: &Path) -> Result<RunManifest> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest `{}`", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse `{}`", path.display()))?;
    if !value.is_object() {
        return Err(TuiError::InvalidManifest {
            path: path.to_path_buf(),
        }
        .into());
    }
    serde_json::from_value(value).with_context(|| {
        format!(
            "failed to deserialize manifest into TUI read model `{}`",
            path.display()
        )
    })
}

pub fn parse_generation_metrics(path: &Path) -> Result<Vec<GenerationMetricRow>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read generation metrics `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let parts = split_csv_row(line);
        if parts.len() < 17 {
            continue;
        }
        rows.push(GenerationMetricRow {
            generation: parse_usize(parts.first().copied()),
            phase: parts.get(1).copied().unwrap_or_default().to_string(),
            elapsed_secs: parse_f64(parts.get(2).copied()).unwrap_or(0.0),
            request_count: parse_usize(parts.get(3).copied()),
            success_count: parse_usize(parts.get(4).copied()),
            failure_count: parse_usize(parts.get(5).copied()),
            converged_count: parse_usize(parts.get(6).copied()),
            best_energy: parse_f64(parts.get(7).copied()),
            mean_energy: parse_f64(parts.get(8).copied()),
            worst_energy: parse_f64(parts.get(9).copied()),
            population_size: parse_optional_usize(parts.get(10).copied()),
            valid_population_size: parse_optional_usize(parts.get(11).copied()),
            duplicate_count: parse_optional_usize(parts.get(12).copied()),
            repopulated_count: parse_optional_usize(parts.get(16).copied()),
        });
    }
    Ok(rows)
}

pub fn parse_controller_trace(path: &Path) -> Result<Vec<ControllerTraceRow>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read controller trace `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let parts = split_csv_row(line);
        if parts.len() < 12 {
            continue;
        }
        rows.push(ControllerTraceRow {
            generation: parse_usize(parts.first().copied()),
            stage: parts.get(1).copied().unwrap_or_default().to_string(),
            population_size: parse_usize(parts.get(2).copied()),
            valid_population_size: parse_usize(parts.get(3).copied()),
            child_count: parse_usize(parts.get(4).copied()),
            duplicate_count: parse_usize(parts.get(5).copied()),
            duplicate_hashkey_count: parse_usize(parts.get(6).copied()),
            duplicate_pmoi_count: parse_usize(parts.get(7).copied()),
            duplicate_energy_tol_count: parse_usize(parts.get(8).copied()),
            repopulated_count: parse_usize(parts.get(9).copied()),
            best_energy: parse_f64(parts.get(10).copied()),
            selected_indices: parts
                .get(11)
                .copied()
                .unwrap_or_default()
                .split('|')
                .filter_map(|item| item.parse::<usize>().ok())
                .collect(),
        });
    }
    Ok(rows)
}

pub fn parse_walker_trace(path: &Path) -> Result<Vec<WalkerTraceRow>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read walker trace `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let parts = split_csv_row(line);
        if parts.len() < 9 {
            continue;
        }
        rows.push(WalkerTraceRow {
            step: parse_usize(parts.first().copied()),
            walker_id: parts.get(1).copied().unwrap_or_default().to_string(),
            accepted: parts.get(2).copied().unwrap_or_default().to_string(),
            energy: parse_f64(parts.get(3).copied()),
            best_energy: parse_f64(parts.get(4).copied()),
            temperature: parse_f64(parts.get(5).copied()),
            step_size: parse_f64(parts.get(6).copied()),
            move_class: parts.get(7).copied().unwrap_or_default().to_string(),
            label: parts.get(8).copied().unwrap_or_default().to_string(),
        });
    }
    Ok(rows)
}

pub fn parse_generation_state(path: &Path) -> Result<GenerationStateFile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read generation state `{}`", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse generation state `{}`", path.display()))
}

pub fn parse_generation_summary(path: &Path) -> Result<GenerationSummaryRow> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read generation summary `{}`", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse generation summary `{}`", path.display()))
}

fn split_csv_row(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

fn parse_usize(value: Option<&str>) -> usize {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .and_then(|item| item.parse::<usize>().ok())
        .unwrap_or(0)
}

fn parse_optional_usize(value: Option<&str>) -> Option<usize> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .and_then(|item| item.parse::<usize>().ok())
}

fn parse_f64(value: Option<&str>) -> Option<f64> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .and_then(|item| item.parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::{parse_controller_trace, parse_generation_metrics};
    use std::fs;

    #[test]
    fn parses_generation_metrics_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("generation_metrics.csv");
        fs::write(
            &path,
            "generation,phase,elapsed_secs,request_count,success_count,failure_count,converged_count,best_energy,mean_energy,worst_energy,population_size,valid_population_size,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count\n0,initialize,1.0,20,18,2,18,-10.0,-9.0,-8.0,12,10,3,0,2,1,2\n",
        )
        .expect("write metrics");

        let rows = parse_generation_metrics(&path).expect("parse metrics");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].generation, 0);
        assert_eq!(rows[0].phase, "initialize");
        assert_eq!(rows[0].duplicate_count, Some(3));
    }

    #[test]
    fn parses_controller_trace_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("controller_trace.csv");
        fs::write(
            &path,
            "generation,stage,population_size,valid_population_size,child_count,duplicate_count,duplicate_hashkey_count,duplicate_pmoi_count,duplicate_energy_tol_count,repopulated_count,best_energy,selected_indices\n1,Selection,20,18,0,0,0,0,0,0,-12.0,0|1|5\n",
        )
        .expect("write controller trace");

        let rows = parse_controller_trace(&path).expect("parse controller trace");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].generation, 1);
        assert_eq!(rows[0].selected_indices, vec![0, 1, 5]);
    }
}
