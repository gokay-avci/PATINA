use patina_evaluator::{BackendAttemptReport, StageBackendStatus, StageIndex};
use patina_external::EvalError;
use patina_types::EvalResult;
use std::fs;
use std::path::Path;

/// Minimal context needed to turn one backend call into a Scott attempt report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostAttemptContext {
    pub stage: StageIndex,
    pub attempt: usize,
    pub primary_output_path: Option<String>,
}

/// Maps the current external-adapter result surface into a Scott backend-attempt report.
///
/// This is the first concrete bridge between the working backend adapters and the
/// Scott-native procedure kernel.
pub fn ghost_result_to_attempt_report(
    ctx: GhostAttemptContext,
    outcome: Result<EvalResult, EvalError>,
) -> BackendAttemptReport {
    match outcome {
        Ok(result) => BackendAttemptReport {
            stage: ctx.stage,
            attempt: ctx.attempt,
            backend_status: if result.converged {
                StageBackendStatus::Converged
            } else {
                StageBackendStatus::ConvergedWithGradientWarning
            },
            failure_detail: None,
            energy: Some(result.energy),
            gnorm: None,
            relaxed_label: Some(result.relaxed_candidate.label.clone()),
            primary_output_path: ctx.primary_output_path,
            result: Some(result),
        },
        Err(error) => map_eval_error(ctx, error),
    }
}

fn map_eval_error(ctx: GhostAttemptContext, error: EvalError) -> BackendAttemptReport {
    match error {
        EvalError::NotConverged {
            energy,
            n_steps,
            partial_result,
        } => {
            let recovered = recovered_gulp_cycle_gnorm(&ctx.primary_output_path);
            BackendAttemptReport {
                stage: ctx.stage,
                attempt: ctx.attempt,
                backend_status: recovered
                    .map(|_| StageBackendStatus::ConvergedWithGradientWarning)
                    .unwrap_or(StageBackendStatus::RequiresMoreCycles),
                failure_detail: Some(match recovered {
                    Some((recovered_steps, gnorm)) => format!(
                        "backend did not emit a convergence banner; recovered final cycle state from output (n_steps={}, gnorm={gnorm:.6}, energy={energy})",
                        recovered_steps.max(n_steps)
                    ),
                    None => format!(
                        "backend did not converge after {n_steps} steps (energy={energy})"
                    ),
                }),
                energy: Some(energy),
                gnorm: recovered.map(|(_, gnorm)| gnorm),
                relaxed_label: partial_result
                    .as_ref()
                    .map(|result| result.relaxed_candidate.label.clone()),
                primary_output_path: ctx.primary_output_path,
                result: partial_result.map(|result| *result),
            }
        }
        EvalError::ParseFailed { line, reason } => BackendAttemptReport {
            stage: ctx.stage,
            attempt: ctx.attempt,
            backend_status: StageBackendStatus::InvalidEnergy,
            failure_detail: Some(format!(
                "backend output parse failed at line {line}: {reason}"
            )),
            energy: None,
            gnorm: None,
            relaxed_label: None,
            primary_output_path: ctx.primary_output_path,
            result: None,
        },
        EvalError::ProcessFailed { exit_code, stderr } => BackendAttemptReport {
            stage: ctx.stage,
            attempt: ctx.attempt,
            backend_status: StageBackendStatus::Crashed,
            failure_detail: Some(format!(
                "backend process failed (exit_code={exit_code:?}): {stderr}"
            )),
            energy: None,
            gnorm: None,
            relaxed_label: None,
            primary_output_path: ctx.primary_output_path,
            result: None,
        },
        EvalError::Timeout { elapsed } => BackendAttemptReport {
            stage: ctx.stage,
            attempt: ctx.attempt,
            backend_status: StageBackendStatus::Crashed,
            failure_detail: Some(format!("backend timed out after {elapsed:?}")),
            energy: None,
            gnorm: None,
            relaxed_label: None,
            primary_output_path: ctx.primary_output_path,
            result: None,
        },
        EvalError::IoError(error) => BackendAttemptReport {
            stage: ctx.stage,
            attempt: ctx.attempt,
            backend_status: StageBackendStatus::Crashed,
            failure_detail: Some(format!("backend I/O failure: {error}")),
            energy: None,
            gnorm: None,
            relaxed_label: None,
            primary_output_path: ctx.primary_output_path,
            result: None,
        },
        EvalError::TemplateInvalid { path, reason } => BackendAttemptReport {
            stage: ctx.stage,
            attempt: ctx.attempt,
            backend_status: StageBackendStatus::Crashed,
            failure_detail: Some(format!(
                "invalid gin template `{}`: {reason}",
                path.display()
            )),
            energy: None,
            gnorm: None,
            relaxed_label: None,
            primary_output_path: ctx.primary_output_path,
            result: None,
        },
    }
}

fn recovered_gulp_cycle_gnorm(primary_output_path: &Option<String>) -> Option<(usize, f64)> {
    let path = Path::new(primary_output_path.as_deref()?);
    let content = fs::read_to_string(path).ok()?;
    content.lines().rev().find_map(parse_gulp_cycle_line)
}

fn parse_gulp_cycle_line(line: &str) -> Option<(usize, f64)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("Cycle:") {
        return None;
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let cycle_index = tokens.iter().position(|token| *token == "Cycle:")?;
    let gnorm_index = tokens.iter().position(|token| *token == "Gnorm:")?;
    let n_steps = tokens.get(cycle_index + 1)?.parse::<usize>().ok()?;
    let gnorm = tokens.get(gnorm_index + 1)?.parse::<f64>().ok()?;
    Some((n_steps, gnorm))
}

#[cfg(test)]
mod tests {
    use super::{ghost_result_to_attempt_report, GhostAttemptContext};
    use patina_evaluator::{StageBackendStatus, StageIndex};
    use patina_external::EvalError;
    use patina_types::{Candidate, EvalResult};
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;

    fn ctx() -> GhostAttemptContext {
        GhostAttemptContext {
            stage: StageIndex(1),
            attempt: 2,
            primary_output_path: Some("stage_01_attempt_02/gulp_klmc.gout".into()),
        }
    }

    fn result() -> EvalResult {
        EvalResult {
            energy: -10.5,
            forces: vec![[0.0, 0.0, 0.0]],
            relaxed_candidate: Candidate {
                species: vec!["Mg".into()],
                fractional_coords: vec![[0.0, 0.0, 0.0]],
                lattice: None,
                periodic_axes: [false, false, false],
                label: "mg1".into(),
            },
            converged: true,
            wall_time: Duration::from_secs(1),
        }
    }

    #[test]
    fn successful_eval_maps_to_converged_attempt() {
        let report = ghost_result_to_attempt_report(ctx(), Ok(result()));
        assert_eq!(report.backend_status, StageBackendStatus::Converged);
        assert_eq!(report.energy, Some(-10.5));
        assert_eq!(report.relaxed_label.as_deref(), Some("mg1"));
        assert_eq!(
            report.primary_output_path.as_deref(),
            Some("stage_01_attempt_02/gulp_klmc.gout")
        );
        assert!(report.result.is_some());
        assert_eq!(report.failure_detail, None);
    }

    #[test]
    fn not_converged_maps_to_requires_more_cycles() {
        let report = ghost_result_to_attempt_report(
            ctx(),
            Err(EvalError::NotConverged {
                energy: -2.0,
                n_steps: 100,
                partial_result: Some(Box::new(result())),
            }),
        );
        assert_eq!(
            report.backend_status,
            StageBackendStatus::RequiresMoreCycles
        );
        assert_eq!(report.energy, Some(-2.0));
        assert_eq!(report.relaxed_label.as_deref(), Some("mg1"));
        assert!(report.result.is_some());
        assert!(report
            .failure_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("backend did not converge")));
    }

    #[test]
    fn not_converged_with_recoverable_gulp_cycle_maps_to_gradient_warning() {
        let temp = tempdir().expect("tempdir");
        let gout_path = temp.path().join("gulp_klmc.gout");
        fs::write(
            &gout_path,
            "\
  Cycle:   1000 Energy:      -950.197510  Gnorm:      0.005974  CPU:    0.427
  Job Finished
",
        )
        .expect("write gout");

        let report = ghost_result_to_attempt_report(
            GhostAttemptContext {
                stage: StageIndex(1),
                attempt: 1,
                primary_output_path: Some(gout_path.display().to_string()),
            },
            Err(EvalError::NotConverged {
                energy: -950.19751011,
                n_steps: 0,
                partial_result: Some(Box::new(result())),
            }),
        );

        assert_eq!(
            report.backend_status,
            StageBackendStatus::ConvergedWithGradientWarning
        );
        assert_eq!(report.gnorm, Some(0.005974));
        assert!(report
            .failure_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("recovered final cycle state")));
    }

    #[test]
    fn parse_fail_maps_to_invalid_energy() {
        let report = ghost_result_to_attempt_report(
            ctx(),
            Err(EvalError::ParseFailed {
                line: 7,
                reason: "bad gout".into(),
            }),
        );
        assert_eq!(report.backend_status, StageBackendStatus::InvalidEnergy);
        assert!(report
            .failure_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("parse failed")));
    }

    #[test]
    fn process_fail_maps_to_crashed() {
        let report = ghost_result_to_attempt_report(
            ctx(),
            Err(EvalError::ProcessFailed {
                exit_code: Some(1),
                stderr: "boom".into(),
            }),
        );
        assert_eq!(report.backend_status, StageBackendStatus::Crashed);
        assert!(report
            .failure_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("exit_code=Some(1)")));
    }
}
