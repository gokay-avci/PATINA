use super::bh_continuity;
use anyhow::{Context, Result};
use patina_external::GotParser;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BhStepDecision {
    pub step: usize,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BhLogReason {
    pub step: usize,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct BhTraceRecovery {
    pub decisions: Vec<BhStepDecision>,
    pub reasons: Vec<BhLogReason>,
    pub energy_rows: BTreeMap<String, f64>,
    pub recovered_a1: Option<f64>,
    pub recovered_a2: Option<f64>,
}

pub fn recover_bh_trace(workdir: &Path) -> Result<BhTraceRecovery> {
    Ok(BhTraceRecovery {
        decisions: parse_bh_step_decisions(workdir)?,
        reasons: parse_bh_log_reasons(workdir)?,
        energy_rows: parse_bh_energy_rows(workdir)?,
        recovered_a1: recover_bh_saved_gout_energy(workdir, "A1")?,
        recovered_a2: recover_bh_saved_gout_energy(workdir, "A2")?,
    })
}

pub fn write_bh_walker_state_artifact(
    workdir: &Path,
    raw_dir: &Path,
    decisions: &[BhStepDecision],
) -> Result<()> {
    let Some(state) = recover_bh_walker_state(workdir, decisions)? else {
        return Ok(());
    };
    fs::create_dir_all(raw_dir)
        .with_context(|| format!("failed to create `{}`", raw_dir.display()))?;
    fs::write(
        raw_dir.join("bh_walker_state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;
    Ok(())
}

fn recover_bh_walker_state(
    workdir: &Path,
    decisions: &[BhStepDecision],
) -> Result<Option<patina_types::BhWalkerState>> {
    let step = decisions
        .iter()
        .map(|decision| decision.step)
        .max()
        .unwrap_or(0);
    let candidates = [
        workdir.join("run").join("A2_save.gout"),
        workdir.join("run").join("A1_save.gout"),
        workdir.join("run").join("0").join("A1_save.gout"),
        workdir.join("run").join("gulp_klmc.gout"),
        workdir.join("run").join("0").join("gulp_klmc.gout"),
    ];

    let mut parseable = Vec::new();
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let Ok(result) = GotParser::parse_file(&path) else {
            continue;
        };
        parseable.push((path, result.energy));
    }

    let Some((current_path, _)) = parseable.first() else {
        return Ok(None);
    };
    let best_path = parseable
        .iter()
        .min_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Greater)
        })
        .map(|(path, _)| path.as_path());
    let restart_snapshot_path = [
        workdir.join("restart").join("walker.can"),
        workdir.join("run").join("restart").join("walker.can"),
        workdir.join("run").join("walker.can"),
        workdir.join("walker.can"),
    ]
    .into_iter()
    .find(|path| path.exists());
    let state = bh_continuity::load_bh_walker_state_from_restart_paths(
        "0",
        "recovered_bh_walker",
        step,
        current_path,
        best_path,
        restart_snapshot_path.as_deref(),
    )?;
    Ok(Some(state))
}

fn parse_bh_step_decisions(workdir: &Path) -> Result<Vec<BhStepDecision>> {
    let klmc_out_path = workdir.join("KLMC.out");
    if !klmc_out_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&klmc_out_path)
        .with_context(|| format!("failed to read `{}`", klmc_out_path.display()))?;
    let mut decisions = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(step) = parse_step_from_banner(trimmed, "Rejected step,") {
            decisions.push(BhStepDecision {
                step,
                accepted: false,
            });
        } else if let Some(step) = parse_step_from_banner(trimmed, "Accepted downhill step,") {
            decisions.push(BhStepDecision {
                step,
                accepted: true,
            });
        } else if let Some(step) = parse_step_from_banner(trimmed, "Accepted step,") {
            decisions.push(BhStepDecision {
                step,
                accepted: true,
            });
        }
    }
    Ok(decisions)
}

fn parse_step_from_banner(line: &str, prefix: &str) -> Option<usize> {
    let tail = line.split(prefix).nth(1)?.trim();
    let digits = tail
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<usize>().ok()
    }
}

fn parse_bh_log_reasons(workdir: &Path) -> Result<Vec<BhLogReason>> {
    let log_path = workdir.join("run").join("log");
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&log_path)
        .with_context(|| format!("failed to read `{}`", log_path.display()))?;
    let mut rows = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Step") {
            continue;
        }
        let Some((step_part, reason_part)) = trimmed.split_once(':') else {
            continue;
        };
        let step_digits = step_part
            .chars()
            .filter(|ch| ch.is_ascii_digit())
            .collect::<String>();
        let Some(step) = step_digits.parse::<usize>().ok() else {
            continue;
        };
        rows.push(BhLogReason {
            step,
            reason: reason_part.trim().to_string(),
        });
    }
    Ok(rows)
}

fn parse_bh_energy_rows(workdir: &Path) -> Result<BTreeMap<String, f64>> {
    let mut found = BTreeMap::new();
    for candidate in [
        workdir.join("run").join("energy"),
        workdir.join("run").join("0").join("energy"),
    ] {
        if !candidate.exists() {
            continue;
        }
        let content = fs::read_to_string(&candidate)
            .with_context(|| format!("failed to read `{}`", candidate.display()))?;
        for (idx, line) in content.lines().enumerate() {
            if idx == 0 || line.trim().is_empty() {
                continue;
            }
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 2 {
                continue;
            }
            let Some(energy) = parts[1].parse::<f64>().ok() else {
                continue;
            };
            found.insert(parts[0].to_string(), energy);
        }
        break;
    }
    Ok(found)
}

fn recover_bh_saved_gout_energy(workdir: &Path, label: &str) -> Result<Option<f64>> {
    let path = workdir.join("run").join(format!("{label}_save.gout"));
    if !path.exists() {
        return Ok(None);
    }
    match GotParser::parse_file(&path) {
        Ok(result) => Ok(Some(result.energy)),
        Err(_) => scan_final_energy_from_gout(&path),
    }
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

#[cfg(test)]
mod tests {
    use super::{
        recover_bh_trace, recover_bh_walker_state, write_bh_walker_state_artifact, BhLogReason,
        BhStepDecision,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn materialize_preserved_bh_restart_fixture(root: &Path) {
        let restart_dir = root.join("restart");
        let run_dir = root.join("run");
        fs::create_dir_all(&restart_dir).expect("restart dir");
        fs::create_dir_all(&run_dir).expect("run dir");
        let files = [
            (
                root.join("KLMC.out"),
                include_str!("fixtures/bh/native_restart_workdir/KLMC.out"),
            ),
            (
                run_dir.join("log"),
                include_str!("fixtures/bh/native_restart_workdir/run/log"),
            ),
            (
                run_dir.join("energy"),
                include_str!("fixtures/bh/native_restart_workdir/run/energy"),
            ),
            (
                run_dir.join("A2_save.gout"),
                include_str!("fixtures/bh/native_restart_workdir/run/A2_save.gout"),
            ),
            (
                run_dir.join("A1_save.gout"),
                include_str!("fixtures/bh/native_restart_workdir/run/A1_save.gout"),
            ),
            (
                restart_dir.join("walker.can"),
                include_str!("fixtures/bh/native_restart_workdir/restart/walker.can"),
            ),
        ];

        for (path, content) in files {
            fs::write(path, content).expect("write preserved fixture file");
        }
    }

    #[test]
    fn preserved_bh_fixture_recovers_trace_and_walker_state_artifact() {
        let temp = tempdir().expect("tempdir");
        materialize_preserved_bh_restart_fixture(temp.path());
        let restart_path = temp.path().join("restart").join("walker.can");

        let trace = recover_bh_trace(temp.path()).expect("recover trace");
        assert_eq!(
            trace.decisions,
            vec![
                BhStepDecision {
                    step: 1,
                    accepted: false,
                },
                BhStepDecision {
                    step: 2,
                    accepted: true,
                },
                BhStepDecision {
                    step: 3,
                    accepted: true,
                },
            ]
        );
        assert_eq!(
            trace.reasons,
            vec![
                BhLogReason {
                    step: 1,
                    reason: "rejected by metropolis gate".into(),
                },
                BhLogReason {
                    step: 2,
                    reason: "accepted downhill move".into(),
                },
                BhLogReason {
                    step: 3,
                    reason: "accepted after matched relaxation comparison".into(),
                },
            ]
        );
        assert_eq!(trace.energy_rows.get("A2"), Some(&-1.25));
        assert_eq!(trace.energy_rows.get("A1"), Some(&-1.50));
        assert_eq!(trace.recovered_a2, Some(-1.25));
        assert_eq!(trace.recovered_a1, Some(-1.50));

        let state = recover_bh_walker_state(temp.path(), &trace.decisions)
            .expect("recover state")
            .expect("state");

        assert_eq!(state.step, 3);
        assert_eq!(state.restart.origin_label, "walker");
        assert_eq!(
            state.restart.source_output_path.as_deref(),
            Some(restart_path.to_string_lossy().as_ref())
        );
        let restart_equivalence = state
            .restart_equivalence
            .as_ref()
            .expect("restart equivalence");
        assert!(restart_equivalence.matches_current);
        assert!(!restart_equivalence.matches_best);

        let raw_dir = temp.path().join("raw");
        write_bh_walker_state_artifact(temp.path(), &raw_dir, &trace.decisions)
            .expect("write walker state artifact");
        let persisted: patina_types::BhWalkerState = serde_json::from_str(
            &fs::read_to_string(raw_dir.join("bh_walker_state.json"))
                .expect("read bh walker state artifact"),
        )
        .expect("deserialize bh walker state artifact");
        assert_eq!(persisted, state);
    }
}
