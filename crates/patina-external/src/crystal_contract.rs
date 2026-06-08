use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const HARTREE_TO_EV: f64 = 27.211_386_245_988;

/// High-level CRYSTAL run family inferred from adapter intent or later input-deck parsing.
///
/// This is deliberately broader than the current single-point / optimization path because
/// CRYSTAL outputs can also carry property, phonon, band-structure, and density-of-states results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrystalRunIntent {
    Unknown,
    SinglePointEnergy,
    GeometryOptimization,
    Properties,
    Phonon,
    BandStructure,
    DensityOfStates,
}

/// Artifact kind that may carry CRYSTAL terminal evidence or reusable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrystalArtifactKind {
    PrimaryOutput,
    ErrorOutput,
    PropertiesOutput,
    AuxiliaryOutput,
    Wavefunction,
    Restart,
}

/// Candidate path that may carry authoritative CRYSTAL terminal information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrystalArtifactCandidate {
    pub kind: CrystalArtifactKind,
    pub path: PathBuf,
}

/// Typed contract for CRYSTAL terminal output discovery and coarse completion checks.
///
/// The contract intentionally stops before `EvalResult` materialization. It records the likely
/// output artifacts and extracts only low-risk observables from stdout. Later adapters can layer
/// full parsers for geometry, gradients, band structures, phonons, elastic constants, charges, and
/// other CRYSTAL property surfaces without tightening this terminal-evidence boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrystalTerminalOutputContract {
    pub primary_output_name: String,
    pub stderr_name: String,
    pub properties_output_name: Option<String>,
    pub auxiliary_output_names: Vec<String>,
    pub run_intent: CrystalRunIntent,
}

impl CrystalTerminalOutputContract {
    pub fn new(primary_output_name: impl Into<String>, stderr_name: impl Into<String>) -> Self {
        Self {
            primary_output_name: primary_output_name.into(),
            stderr_name: stderr_name.into(),
            properties_output_name: None,
            auxiliary_output_names: Vec::new(),
            run_intent: CrystalRunIntent::Unknown,
        }
    }

    pub fn with_run_intent(mut self, run_intent: CrystalRunIntent) -> Self {
        self.run_intent = run_intent;
        self
    }

    pub fn with_properties_output_name(mut self, output_name: impl Into<String>) -> Self {
        self.properties_output_name = Some(output_name.into());
        self
    }

    pub fn with_auxiliary_output_name(mut self, output_name: impl Into<String>) -> Self {
        self.auxiliary_output_names.push(output_name.into());
        self
    }

    pub fn primary_output_path(&self, workdir: &Path) -> PathBuf {
        workdir.join(&self.primary_output_name)
    }

    pub fn stderr_path(&self, workdir: &Path) -> PathBuf {
        workdir.join(&self.stderr_name)
    }

    pub fn artifact_candidates(&self, workdir: &Path) -> Vec<CrystalArtifactCandidate> {
        let mut candidates = vec![
            CrystalArtifactCandidate {
                kind: CrystalArtifactKind::PrimaryOutput,
                path: self.primary_output_path(workdir),
            },
            CrystalArtifactCandidate {
                kind: CrystalArtifactKind::ErrorOutput,
                path: self.stderr_path(workdir),
            },
        ];

        if let Some(output_name) = &self.properties_output_name {
            candidates.push(CrystalArtifactCandidate {
                kind: CrystalArtifactKind::PropertiesOutput,
                path: workdir.join(output_name),
            });
        }

        candidates.extend(self.auxiliary_output_names.iter().map(|output_name| {
            CrystalArtifactCandidate {
                kind: CrystalArtifactKind::AuxiliaryOutput,
                path: workdir.join(output_name),
            }
        }));

        candidates
    }

    pub fn assess_stdout_text(&self, stdout_text: &str) -> CrystalStdoutAssessment {
        let observables = CrystalParsedObservables::from_stdout_text(stdout_text);
        let status = if !observables.error_markers.is_empty() {
            CrystalStdoutStatus::ErrorLike
        } else if observables.optimization_converged_seen {
            CrystalStdoutStatus::OptimizationConverged
        } else if observables.normal_termination_seen {
            CrystalStdoutStatus::TerminatedNormally
        } else if observables.total_energy_hartree.is_some()
            || observables.total_energy_ev.is_some()
        {
            CrystalStdoutStatus::EnergyOnly
        } else {
            CrystalStdoutStatus::MissingAuthoritativeMarkers
        };

        CrystalStdoutAssessment {
            status,
            run_intent: self.run_intent,
            observables,
        }
    }
}

/// Coarse stdout assessment for later parser/reconciliation work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrystalStdoutStatus {
    TerminatedNormally,
    OptimizationConverged,
    EnergyOnly,
    ErrorLike,
    MissingAuthoritativeMarkers,
}

/// Minimal semantic extraction from the CRYSTAL primary output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrystalParsedObservables {
    pub total_energy_hartree: Option<f64>,
    pub total_energy_ev: Option<f64>,
    pub normal_termination_seen: bool,
    pub optimization_converged_seen: bool,
    pub error_markers: Vec<String>,
}

impl CrystalParsedObservables {
    pub fn from_stdout_text(stdout_text: &str) -> Self {
        let mut total_energy_hartree = None;
        let mut total_energy_ev = None;
        let mut error_markers = Vec::new();

        for line in stdout_text.lines() {
            let lower = line.to_ascii_lowercase();
            if let Some(value) = parse_total_energy_line(&lower, line) {
                match value.unit {
                    CrystalEnergyUnit::Hartree => {
                        total_energy_hartree = Some(value.value);
                        total_energy_ev = Some(value.value * HARTREE_TO_EV);
                    }
                    CrystalEnergyUnit::Ev => {
                        total_energy_ev = Some(value.value);
                        total_energy_hartree = Some(value.value / HARTREE_TO_EV);
                    }
                }
            }

            if is_error_marker(&lower) && error_markers.len() < 8 {
                let marker = line.trim();
                if !marker.is_empty() && !error_markers.iter().any(|seen| seen == marker) {
                    error_markers.push(marker.to_string());
                }
            }
        }

        Self {
            total_energy_hartree,
            total_energy_ev,
            normal_termination_seen: normal_termination_seen(stdout_text),
            optimization_converged_seen: optimization_converged_seen(stdout_text),
            error_markers,
        }
    }
}

/// Coarse stdout assessment plus extracted observables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrystalStdoutAssessment {
    pub status: CrystalStdoutStatus,
    pub run_intent: CrystalRunIntent,
    pub observables: CrystalParsedObservables,
}

impl CrystalStdoutAssessment {
    pub fn result_mapping_decision(&self) -> CrystalResultMappingDecision {
        let has_energy = self.observables.total_energy_hartree.is_some()
            || self.observables.total_energy_ev.is_some();
        let mut notes = Vec::new();

        let status = match self.status {
            CrystalStdoutStatus::ErrorLike => {
                notes.push("error markers dominate any partial observables".into());
                CrystalResultMappingStatus::FailedTerminalEvidence
            }
            CrystalStdoutStatus::MissingAuthoritativeMarkers => {
                notes.push(
                    "no authoritative CRYSTAL completion or energy evidence was found".into(),
                );
                CrystalResultMappingStatus::InsufficientEvidence
            }
            CrystalStdoutStatus::EnergyOnly => {
                notes
                    .push("energy-only evidence is salvageable but not terminal completion".into());
                CrystalResultMappingStatus::SalvageableObservation
            }
            CrystalStdoutStatus::TerminatedNormally
            | CrystalStdoutStatus::OptimizationConverged
                if has_energy =>
            {
                match self.run_intent {
                    CrystalRunIntent::GeometryOptimization => {
                        notes.push("optimized geometry must be fixture-validated before EvalResult materialization".into());
                    }
                    CrystalRunIntent::Properties
                    | CrystalRunIntent::Phonon
                    | CrystalRunIntent::BandStructure
                    | CrystalRunIntent::DensityOfStates => {
                        notes.push("property-bearing CRYSTAL output should preserve sidecar result artifacts".into());
                    }
                    CrystalRunIntent::Unknown => {
                        notes.push(
                            "run intent is unknown; only scalar energy mapping is eligible".into(),
                        );
                    }
                    CrystalRunIntent::SinglePointEnergy => {}
                }
                CrystalResultMappingStatus::CandidateForMaterialization
            }
            CrystalStdoutStatus::TerminatedNormally
            | CrystalStdoutStatus::OptimizationConverged => {
                notes.push("completion marker was found but no total energy was extracted".into());
                CrystalResultMappingStatus::InsufficientEvidence
            }
        };

        CrystalResultMappingDecision {
            status,
            run_intent: self.run_intent,
            total_energy_hartree: self.observables.total_energy_hartree,
            total_energy_ev: self.observables.total_energy_ev,
            notes,
        }
    }
}

/// Conservative result-mapping status between terminal parsing and `EvalResult` materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrystalResultMappingStatus {
    CandidateForMaterialization,
    SalvageableObservation,
    FailedTerminalEvidence,
    InsufficientEvidence,
}

/// Result-mapping decision that recovery code can store without fabricating a scientific result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrystalResultMappingDecision {
    pub status: CrystalResultMappingStatus,
    pub run_intent: CrystalRunIntent,
    pub total_energy_hartree: Option<f64>,
    pub total_energy_ev: Option<f64>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CrystalEnergyValue {
    value: f64,
    unit: CrystalEnergyUnit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CrystalEnergyUnit {
    Hartree,
    Ev,
}

fn parse_total_energy_line(lower: &str, line: &str) -> Option<CrystalEnergyValue> {
    if !(lower.contains("total") && lower.contains("energy")) {
        return None;
    }
    if lower.contains("threshold") || lower.contains("tolerance") || lower.contains("convergence") {
        return None;
    }

    let value = line
        .split_whitespace()
        .filter_map(parse_float_token)
        .rfind(|value| value.is_finite())?;

    let unit = if lower.contains(" ev") || lower.ends_with("ev") || lower.contains("(ev)") {
        CrystalEnergyUnit::Ev
    } else {
        CrystalEnergyUnit::Hartree
    };

    Some(CrystalEnergyValue { value, unit })
}

fn parse_float_token(token: &str) -> Option<f64> {
    let cleaned = token.trim_matches(|character: char| {
        !character.is_ascii_digit()
            && character != '+'
            && character != '-'
            && character != '.'
            && character != 'e'
            && character != 'E'
    });
    cleaned.parse::<f64>().ok()
}

fn normal_termination_seen(stdout_text: &str) -> bool {
    let lower = stdout_text.to_ascii_lowercase();
    lower.contains("normal termination")
        || lower.contains("crystal run ended")
        || lower.contains("crystal calculation ended")
        || (lower.contains("ended") && lower.contains("crystal") && !lower.contains("abnormal"))
}

fn optimization_converged_seen(stdout_text: &str) -> bool {
    let lower = stdout_text.to_ascii_lowercase();
    lower.contains("opt end")
        || (lower.contains("optimization") && lower.contains("converged"))
        || (lower.contains("optimisation") && lower.contains("converged"))
        || (lower.contains("convergence") && lower.contains("reached"))
}

fn is_error_marker(lower: &str) -> bool {
    lower.contains(" error")
        || lower.starts_with("error")
        || lower.contains("abnormal")
        || lower.contains(" failed")
        || lower.contains("failure")
        || lower.contains("not converged")
}

#[cfg(test)]
mod tests {
    use super::{
        CrystalArtifactKind, CrystalResultMappingStatus, CrystalRunIntent, CrystalStdoutStatus,
        CrystalTerminalOutputContract,
    };
    use std::path::Path;

    #[test]
    fn contract_derives_terminal_artifact_candidates_without_assuming_fixed_unit_files() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err")
            .with_run_intent(CrystalRunIntent::Properties)
            .with_properties_output_name("properties.out")
            .with_auxiliary_output_name("fort.9");

        let candidates = contract.artifact_candidates(Path::new("/runs/crystal"));

        assert_eq!(candidates.len(), 4);
        assert_eq!(candidates[0].kind, CrystalArtifactKind::PrimaryOutput);
        assert_eq!(candidates[0].path, Path::new("/runs/crystal/crystal.out"));
        assert_eq!(candidates[1].kind, CrystalArtifactKind::ErrorOutput);
        assert_eq!(candidates[2].kind, CrystalArtifactKind::PropertiesOutput);
        assert_eq!(candidates[3].kind, CrystalArtifactKind::AuxiliaryOutput);
        assert_eq!(contract.run_intent, CrystalRunIntent::Properties);
    }

    #[test]
    fn stdout_assessment_detects_normal_termination_and_energy() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err");
        let stdout = r#"
 TOTAL ENERGY(DFT)(AU)     -75.123456789
 CRYSTAL RUN ENDED NORMALLY
 "#;

        let assessment = contract.assess_stdout_text(stdout);

        assert_eq!(assessment.status, CrystalStdoutStatus::TerminatedNormally);
        assert_eq!(
            assessment.result_mapping_decision().status,
            CrystalResultMappingStatus::CandidateForMaterialization
        );
        assert_eq!(
            assessment.observables.total_energy_hartree,
            Some(-75.123456789)
        );
        assert!(assessment.observables.total_energy_ev.is_some());
        assert!(assessment.observables.normal_termination_seen);
    }

    #[test]
    fn stdout_assessment_detects_optimization_convergence() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err");
        let stdout = r#"
 TOTAL ENERGY = -75.0 HARTREE
 OPT END - GEOMETRY OPTIMIZATION CONVERGED
 "#;

        let assessment = contract.assess_stdout_text(stdout);

        assert_eq!(
            assessment.status,
            CrystalStdoutStatus::OptimizationConverged
        );
        assert_eq!(
            assessment.result_mapping_decision().status,
            CrystalResultMappingStatus::CandidateForMaterialization
        );
        assert!(assessment.observables.optimization_converged_seen);
        assert_eq!(assessment.observables.total_energy_hartree, Some(-75.0));
    }

    #[test]
    fn stdout_assessment_allows_energy_only_salvage_without_claiming_completion() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err");
        let stdout = r#"
 SCF CYCLE 12
 TOTAL ENERGY = -42.25
 "#;

        let assessment = contract.assess_stdout_text(stdout);

        assert_eq!(assessment.status, CrystalStdoutStatus::EnergyOnly);
        assert_eq!(
            assessment.result_mapping_decision().status,
            CrystalResultMappingStatus::SalvageableObservation
        );
        assert_eq!(assessment.observables.total_energy_hartree, Some(-42.25));
        assert!(!assessment.observables.normal_termination_seen);
    }

    #[test]
    fn stdout_assessment_lets_errors_dominate_over_partial_energy() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err");
        let stdout = r#"
 TOTAL ENERGY = -42.25
 ERROR **** SCF FAILED
 "#;

        let assessment = contract.assess_stdout_text(stdout);

        assert_eq!(assessment.status, CrystalStdoutStatus::ErrorLike);
        assert_eq!(
            assessment.result_mapping_decision().status,
            CrystalResultMappingStatus::FailedTerminalEvidence
        );
        assert_eq!(assessment.observables.total_energy_hartree, Some(-42.25));
        assert_eq!(assessment.observables.error_markers.len(), 1);
    }

    #[test]
    fn stdout_energy_parser_ignores_threshold_lines() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err");
        let stdout = "TOTAL ENERGY THRESHOLD 1.0E-7\n";

        let assessment = contract.assess_stdout_text(stdout);

        assert_eq!(
            assessment.status,
            CrystalStdoutStatus::MissingAuthoritativeMarkers
        );
        assert_eq!(
            assessment.result_mapping_decision().status,
            CrystalResultMappingStatus::InsufficientEvidence
        );
        assert_eq!(assessment.observables.total_energy_hartree, None);
    }

    #[test]
    fn result_mapping_notes_preserve_future_property_surfaces() {
        let contract = CrystalTerminalOutputContract::new("crystal.out", "crystal.err")
            .with_run_intent(CrystalRunIntent::BandStructure);
        let stdout = r#"
 TOTAL ENERGY(DFT)(AU)     -75.123456789
 CRYSTAL RUN ENDED NORMALLY
 "#;

        let decision = contract
            .assess_stdout_text(stdout)
            .result_mapping_decision();

        assert_eq!(
            decision.status,
            CrystalResultMappingStatus::CandidateForMaterialization
        );
        assert_eq!(decision.run_intent, CrystalRunIntent::BandStructure);
        assert!(decision
            .notes
            .iter()
            .any(|note| note.contains("property-bearing")));
    }
}
