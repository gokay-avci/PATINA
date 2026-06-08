use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Terminal artifact kind expected from one CP2K geometry optimization run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cp2kArtifactKind {
    PrimaryOutput,
    RelaxedTrajectoryXyz,
    RestartInput,
}

/// Candidate path that may carry authoritative CP2K terminal information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cp2kArtifactCandidate {
    pub kind: Cp2kArtifactKind,
    pub path: PathBuf,
}

/// Typed contract for terminal CP2K output discovery and coarse completion checks.
///
/// This contract intentionally stops before `EvalResult` materialization. It defines:
/// - the authoritative main stdout file
/// - the expected geometry/restart filenames derived from `PROJECT`
/// - the convergence banner and energy-line shape used for later parsing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cp2kTerminalOutputContract {
    pub primary_output_name: String,
    pub stderr_name: String,
    pub project_name: Option<String>,
}

impl Cp2kTerminalOutputContract {
    pub const GEOMETRY_COMPLETED_BANNER: &'static str =
        "***                    GEOMETRY OPTIMIZATION COMPLETED                      ***";
    pub const TOTAL_ENERGY_PREFIX: &'static str = "Total Energy";

    pub fn from_input_text(
        input_text: &str,
        primary_output_name: impl Into<String>,
        stderr_name: impl Into<String>,
    ) -> Self {
        Self {
            primary_output_name: primary_output_name.into(),
            stderr_name: stderr_name.into(),
            project_name: parse_cp2k_project_name(input_text),
        }
    }

    pub fn from_input_file(
        input_path: &Path,
        primary_output_name: impl Into<String>,
        stderr_name: impl Into<String>,
    ) -> Result<Self, io::Error> {
        let input_text = fs::read_to_string(input_path)?;
        Ok(Self::from_input_text(
            &input_text,
            primary_output_name,
            stderr_name,
        ))
    }

    pub fn primary_output_path(&self, workdir: &Path) -> PathBuf {
        workdir.join(&self.primary_output_name)
    }

    pub fn stderr_path(&self, workdir: &Path) -> PathBuf {
        workdir.join(&self.stderr_name)
    }

    pub fn artifact_candidates(&self, workdir: &Path) -> Vec<Cp2kArtifactCandidate> {
        let mut candidates = vec![Cp2kArtifactCandidate {
            kind: Cp2kArtifactKind::PrimaryOutput,
            path: self.primary_output_path(workdir),
        }];

        if let Some(project_name) = &self.project_name {
            candidates.push(Cp2kArtifactCandidate {
                kind: Cp2kArtifactKind::RelaxedTrajectoryXyz,
                path: workdir.join(format!("{project_name}-pos-1.xyz")),
            });
            candidates.push(Cp2kArtifactCandidate {
                kind: Cp2kArtifactKind::RestartInput,
                path: workdir.join(format!("{project_name}-1.restart")),
            });
        }

        candidates
    }

    pub fn assess_stdout_text(&self, stdout_text: &str) -> Cp2kStdoutAssessment {
        let optimization_completed = stdout_text.contains(Self::GEOMETRY_COMPLETED_BANNER);
        let last_total_energy_hartree = stdout_text
            .lines()
            .filter_map(parse_total_energy_line_hartree)
            .next_back();

        let status = if optimization_completed {
            Cp2kStdoutStatus::GeometryOptimizationCompleted
        } else if last_total_energy_hartree.is_some() {
            Cp2kStdoutStatus::OptimizationIncomplete
        } else {
            Cp2kStdoutStatus::MissingAuthoritativeMarkers
        };

        Cp2kStdoutAssessment {
            status,
            optimization_completed,
            last_total_energy_hartree,
        }
    }
}

/// Coarse stdout assessment for later parser/reconciliation work.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cp2kStdoutStatus {
    GeometryOptimizationCompleted,
    OptimizationIncomplete,
    MissingAuthoritativeMarkers,
}

/// Minimal semantic extraction from the CP2K primary output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cp2kStdoutAssessment {
    pub status: Cp2kStdoutStatus,
    pub optimization_completed: bool,
    pub last_total_energy_hartree: Option<f64>,
}

pub fn parse_cp2k_project_name(input_text: &str) -> Option<String> {
    input_text.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("PROJECT ") {
            trimmed
                .split_whitespace()
                .nth(1)
                .map(str::to_string)
                .filter(|value| !value.is_empty())
        } else {
            None
        }
    })
}

fn parse_total_energy_line_hartree(line: &str) -> Option<f64> {
    let trimmed = line.trim();
    if !trimmed.starts_with(Cp2kTerminalOutputContract::TOTAL_ENERGY_PREFIX) {
        return None;
    }

    trimmed
        .split('=')
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_cp2k_project_name, Cp2kArtifactKind, Cp2kStdoutStatus, Cp2kTerminalOutputContract,
    };
    use std::path::Path;

    #[test]
    fn contract_extracts_project_name_from_input() {
        let contract = Cp2kTerminalOutputContract::from_input_text(
            "&GLOBAL\n  PROJECT H2O\n  RUN_TYPE GEO_OPT\n&END GLOBAL\n",
            "cp2k.out",
            "cp2k.stderr.log",
        );

        assert_eq!(contract.project_name.as_deref(), Some("H2O"));
    }

    #[test]
    fn contract_derives_authoritative_terminal_artifact_candidates() {
        let contract = Cp2kTerminalOutputContract::from_input_text(
            "&GLOBAL\n  PROJECT H2O\n&END GLOBAL\n",
            "cp2k.out",
            "cp2k.stderr.log",
        );

        let candidates = contract.artifact_candidates(Path::new("/runs/demo"));
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].kind, Cp2kArtifactKind::PrimaryOutput);
        assert_eq!(candidates[1].kind, Cp2kArtifactKind::RelaxedTrajectoryXyz);
        assert_eq!(candidates[1].path, Path::new("/runs/demo/H2O-pos-1.xyz"));
        assert_eq!(candidates[2].kind, Cp2kArtifactKind::RestartInput);
        assert_eq!(candidates[2].path, Path::new("/runs/demo/H2O-1.restart"));
    }

    #[test]
    fn stdout_assessment_detects_completed_geometry_optimization() {
        let contract = Cp2kTerminalOutputContract::from_input_text(
            "&GLOBAL\n  PROJECT H2O\n&END GLOBAL\n",
            "cp2k.out",
            "cp2k.stderr.log",
        );
        let stdout = r#"
 --------  Informations at step =     1 ------------
 Total Energy               =       -17.1643447508
 ---------------------------------------------------
 *******************************************************************************
 ***                    GEOMETRY OPTIMIZATION COMPLETED                      ***
 *******************************************************************************
 "#;

        let assessment = contract.assess_stdout_text(stdout);
        assert_eq!(
            assessment.status,
            Cp2kStdoutStatus::GeometryOptimizationCompleted
        );
        assert!(assessment.optimization_completed);
        assert_eq!(assessment.last_total_energy_hartree, Some(-17.1643447508));
    }

    #[test]
    fn stdout_assessment_detects_incomplete_but_salvageable_output() {
        let contract = Cp2kTerminalOutputContract::from_input_text(
            "&GLOBAL\n  PROJECT H2O\n&END GLOBAL\n",
            "cp2k.out",
            "cp2k.stderr.log",
        );
        let stdout = r#"
 --------  Informations at step =     1 ------------
 Total Energy               =       -17.1000000000
 "#;

        let assessment = contract.assess_stdout_text(stdout);
        assert_eq!(assessment.status, Cp2kStdoutStatus::OptimizationIncomplete);
        assert!(!assessment.optimization_completed);
        assert_eq!(assessment.last_total_energy_hartree, Some(-17.1));
    }

    #[test]
    fn project_name_parser_ignores_missing_project_keyword() {
        assert_eq!(
            parse_cp2k_project_name("&GLOBAL\n  RUN_TYPE GEO_OPT\n&END GLOBAL\n"),
            None
        );
    }
}
