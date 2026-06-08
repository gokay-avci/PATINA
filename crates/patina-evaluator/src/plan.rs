use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::stages::{FinalStageFailurePolicy, StageSelection, StageSelectionError};

/// The two primary evaluator modes the Rust Scott port is expected to own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottBackendMode {
    Gulp,
    JanusMace,
}

/// Intended use of the Scott evaluator procedure within a higher-level workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottProcedureIntent {
    SingleEvaluation,
    ProductionRun,
    BasinHopping,
    GeneticAlgorithm,
    SolidSolutions,
    ScanSurface,
    SimulatedAnnealing,
    EnergyLid,
    HybridGaProduction,
}

/// Cell-mode expectations for the staged evaluator rendering path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottLatticeMode {
    Cluster,
    Periodic3d,
    PartialPeriodic,
}

/// Static settings for the Scott evaluator procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottEvaluatorSettings {
    pub backend_mode: ScottBackendMode,
    pub procedure_intent: ScottProcedureIntent,
    pub lattice_mode: ScottLatticeMode,
    pub max_relaxation_attempts: usize,
    pub gnorm_tolerance: f64,
    pub final_stage_failure_policy: FinalStageFailurePolicy,
    pub retrieve_relaxed_geometry: bool,
    pub keep_stage_artifacts: bool,
}

impl Default for ScottEvaluatorSettings {
    fn default() -> Self {
        Self {
            backend_mode: ScottBackendMode::Gulp,
            procedure_intent: ScottProcedureIntent::SingleEvaluation,
            lattice_mode: ScottLatticeMode::Cluster,
            max_relaxation_attempts: 1,
            gnorm_tolerance: 1.0e-4,
            final_stage_failure_policy: FinalStageFailurePolicy::KeepPreviousAccepted,
            retrieve_relaxed_geometry: true,
            keep_stage_artifacts: true,
        }
    }
}

/// User-visible evaluator plan bound to a template and working layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottEvaluatorPlan {
    pub master_template: Utf8PathBuf,
    pub atoms_in: Option<Utf8PathBuf>,
    pub work_root: Utf8PathBuf,
    pub settings: ScottEvaluatorSettings,
}

/// Fully specified procedure plan including selected stages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottProcedurePlan {
    pub evaluator: ScottEvaluatorPlan,
    pub stages: StageSelection,
}

/// Fortran-shaped staged evaluator inputs before they are normalized into Rust plans.
///
/// This is the direct landing zone for future `run.job` parsing and any other import path
/// that still speaks in native Scott arrays and scalar flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeScottProcedureConfig {
    pub engine_codes: Vec<char>,
    pub refine_thresholds: Vec<Option<f64>>,
    pub energy_min_thresholds: Vec<Option<f64>>,
    pub energy_max_thresholds: Vec<Option<f64>>,
    pub max_relaxation_attempts: usize,
    pub gnorm_tolerance: f64,
    pub only_2nd_energy: bool,
    pub retrieve_relaxed_geometry: bool,
    pub keep_stage_artifacts: bool,
}

#[derive(Debug, Error)]
pub enum NativeScottProcedureParseError {
    #[error("failed to read run.job from `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("missing required run.job key `{0}`")]
    MissingKey(&'static str),
    #[error("run.job key `{key}` has invalid integer value `{value}`")]
    InvalidInteger { key: String, value: String },
    #[error("run.job key `{key}` has invalid float value `{value}`")]
    InvalidFloat { key: String, value: String },
    #[error("run.job key `{key}` has invalid logical value `{value}`")]
    InvalidBoolean { key: String, value: String },
    #[error("run.job key `N_DEF_ENERGY` did not contain any evaluator definitions")]
    EmptyEnergyDefinition,
    #[error("run.job key `N_DEF_ENERGY` contains unknown evaluator token `{0}`")]
    UnknownEnergyDefinition(String),
}

impl NativeScottProcedureConfig {
    pub fn from_run_job_path(path: &Utf8Path) -> Result<Self, NativeScottProcedureParseError> {
        let text =
            fs::read_to_string(path).map_err(|source| NativeScottProcedureParseError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_run_job_text(&text)
    }

    pub fn from_run_job_text(text: &str) -> Result<Self, NativeScottProcedureParseError> {
        let entries = parse_run_job_entries(text);
        let engine_codes = parse_engine_codes(
            lookup_run_job_value(&entries, "N_DEF_ENERGY")
                .ok_or(NativeScottProcedureParseError::MissingKey("N_DEF_ENERGY"))?,
        )?;
        let stage_count = engine_codes.len();

        Ok(Self {
            engine_codes,
            refine_thresholds: parse_stage_thresholds(&entries, stage_count, "R_REFINE_THRESHOLD")?,
            energy_min_thresholds: parse_stage_thresholds(
                &entries,
                stage_count,
                "R_MIN_CLUSTER_ENERGY",
            )?,
            energy_max_thresholds: parse_stage_thresholds(
                &entries,
                stage_count,
                "R_MAX_CLUSTER_ENERGY",
            )?,
            max_relaxation_attempts: parse_optional_usize(&entries, "N_RELAXATION_ATTEMPTS")?
                .unwrap_or(1),
            gnorm_tolerance: parse_optional_f64(&entries, "GNORM")?
                .or(parse_optional_f64(&entries, "R_GNORM_TOL")?)
                .unwrap_or(1.0e-4),
            only_2nd_energy: parse_optional_bool(&entries, "ONLY_2ND_ENERGY")?
                .or(parse_optional_bool(&entries, "L_ONLY_2ND_ENERGY")?)
                .unwrap_or(false),
            retrieve_relaxed_geometry: true,
            keep_stage_artifacts: true,
        })
    }

    pub fn into_stage_selection(self) -> Result<StageSelection, StageSelectionError> {
        StageSelection::from_native_scott(
            &self.engine_codes,
            &self.refine_thresholds,
            &self.energy_min_thresholds,
            &self.energy_max_thresholds,
            self.only_2nd_energy,
        )
    }

    pub fn into_settings(
        self,
        backend_mode: ScottBackendMode,
        procedure_intent: ScottProcedureIntent,
        lattice_mode: ScottLatticeMode,
    ) -> ScottEvaluatorSettings {
        ScottEvaluatorSettings {
            backend_mode,
            procedure_intent,
            lattice_mode,
            max_relaxation_attempts: self.max_relaxation_attempts,
            gnorm_tolerance: self.gnorm_tolerance,
            final_stage_failure_policy: FinalStageFailurePolicy::from_only_2nd_energy(
                self.only_2nd_energy,
            ),
            retrieve_relaxed_geometry: self.retrieve_relaxed_geometry,
            keep_stage_artifacts: self.keep_stage_artifacts,
        }
    }

    pub fn into_procedure_plan(
        self,
        backend_mode: ScottBackendMode,
        procedure_intent: ScottProcedureIntent,
        lattice_mode: ScottLatticeMode,
        master_template: Utf8PathBuf,
        atoms_in: Option<Utf8PathBuf>,
        work_root: Utf8PathBuf,
    ) -> Result<ScottProcedurePlan, StageSelectionError> {
        let settings = self
            .clone()
            .into_settings(backend_mode, procedure_intent, lattice_mode);
        let stages = self.into_stage_selection()?;

        Ok(ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template,
                atoms_in,
                work_root,
                settings,
            },
            stages,
        })
    }
}

fn parse_run_job_entries(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (lhs, rhs) = trimmed.split_once(':')?;
            Some((lhs.trim().to_ascii_uppercase(), rhs.trim().to_string()))
        })
        .collect()
}

fn parse_engine_codes(value: &str) -> Result<Vec<char>, NativeScottProcedureParseError> {
    let mut codes = Vec::new();
    for token in value
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(strip_job_quotes)
        .filter(|token| !token.is_empty())
    {
        let code = match token.to_ascii_uppercase().as_str() {
            "G" | "GULP" => 'G',
            "A" | "AIMS" => 'A',
            "R" | "CRYSTAL" | "CRYSTAL23" => 'R',
            "V" | "VASP" => 'V',
            "N" | "NW" | "NWCHEM" => 'N',
            "D" | "DMOL" | "DMOL3" => 'D',
            "X" | "NONE" => 'X',
            other => {
                return Err(NativeScottProcedureParseError::UnknownEnergyDefinition(
                    other.to_string(),
                ))
            }
        };
        codes.push(code);
    }

    if codes.is_empty() {
        Err(NativeScottProcedureParseError::EmptyEnergyDefinition)
    } else {
        Ok(codes)
    }
}

fn parse_stage_thresholds(
    entries: &[(String, String)],
    stage_count: usize,
    base_key: &str,
) -> Result<Vec<Option<f64>>, NativeScottProcedureParseError> {
    let mut values = vec![None; stage_count];

    if let Some(raw) = lookup_run_job_value(entries, base_key) {
        let parsed = parse_float_key(base_key, raw)?;
        values.fill(Some(parsed));
    }

    for stage in 1..=stage_count {
        let key = format!("{base_key}_{stage}");
        if let Some(raw) = lookup_run_job_value(entries, &key) {
            values[stage - 1] = Some(parse_float_key(&key, raw)?);
        }
    }

    Ok(values)
}

fn parse_optional_usize(
    entries: &[(String, String)],
    key: &str,
) -> Result<Option<usize>, NativeScottProcedureParseError> {
    lookup_run_job_value(entries, key)
        .map(|value| {
            strip_job_quotes(value).parse::<usize>().map_err(|_| {
                NativeScottProcedureParseError::InvalidInteger {
                    key: key.to_string(),
                    value: value.to_string(),
                }
            })
        })
        .transpose()
}

fn parse_optional_f64(
    entries: &[(String, String)],
    key: &str,
) -> Result<Option<f64>, NativeScottProcedureParseError> {
    lookup_run_job_value(entries, key)
        .map(|value| parse_float_key(key, value))
        .transpose()
}

fn parse_optional_bool(
    entries: &[(String, String)],
    key: &str,
) -> Result<Option<bool>, NativeScottProcedureParseError> {
    lookup_run_job_value(entries, key)
        .map(|value| parse_bool_key(key, value))
        .transpose()
}

fn parse_float_key(key: &str, value: &str) -> Result<f64, NativeScottProcedureParseError> {
    strip_job_quotes(value).parse::<f64>().map_err(|_| {
        NativeScottProcedureParseError::InvalidFloat {
            key: key.to_string(),
            value: value.to_string(),
        }
    })
}

fn parse_bool_key(key: &str, value: &str) -> Result<bool, NativeScottProcedureParseError> {
    match strip_job_quotes(value).to_ascii_uppercase().as_str() {
        ".TRUE." | "TRUE" | "T" | "1" => Ok(true),
        ".FALSE." | "FALSE" | "F" | "0" => Ok(false),
        _ => Err(NativeScottProcedureParseError::InvalidBoolean {
            key: key.to_string(),
            value: value.to_string(),
        }),
    }
}

fn lookup_run_job_value<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find_map(|(lhs, rhs)| (lhs == key).then_some(rhs.as_str()))
}

fn strip_job_quotes(value: &str) -> &str {
    value.trim().trim_matches('\'').trim_matches('"')
}

#[cfg(test)]
mod tests {
    use super::{
        NativeScottProcedureConfig, NativeScottProcedureParseError, ScottBackendMode,
        ScottLatticeMode, ScottProcedureIntent,
    };
    use crate::{FinalStageFailurePolicy, StageEngine};

    fn native_config() -> NativeScottProcedureConfig {
        NativeScottProcedureConfig {
            engine_codes: vec!['G', 'A'],
            refine_thresholds: vec![Some(-10.0), None],
            energy_min_thresholds: vec![Some(-100.0), Some(-50.0)],
            energy_max_thresholds: vec![Some(0.0), Some(-1.0)],
            max_relaxation_attempts: 3,
            gnorm_tolerance: 1.0e-5,
            only_2nd_energy: true,
            retrieve_relaxed_geometry: true,
            keep_stage_artifacts: false,
        }
    }

    #[test]
    fn native_config_maps_to_typed_settings() {
        let settings = native_config().into_settings(
            ScottBackendMode::Gulp,
            ScottProcedureIntent::ProductionRun,
            ScottLatticeMode::Cluster,
        );

        assert_eq!(settings.max_relaxation_attempts, 3);
        assert_eq!(settings.gnorm_tolerance, 1.0e-5);
        assert_eq!(
            settings.final_stage_failure_policy,
            FinalStageFailurePolicy::RejectCandidate
        );
        assert!(settings.retrieve_relaxed_geometry);
        assert!(!settings.keep_stage_artifacts);
    }

    #[test]
    fn native_config_maps_to_full_procedure_plan() {
        let plan = native_config()
            .into_procedure_plan(
                ScottBackendMode::Gulp,
                ScottProcedureIntent::ProductionRun,
                ScottLatticeMode::Cluster,
                "/tmp/Master.gin".into(),
                Some("/tmp/atoms.in".into()),
                "/tmp/work".into(),
            )
            .unwrap();

        assert_eq!(plan.stages.stages.len(), 2);
        assert_eq!(plan.stages.stages[0].engine, StageEngine::Gulp);
        assert_eq!(plan.stages.stages[1].engine, StageEngine::Aims);
        assert_eq!(
            plan.evaluator.settings.final_stage_failure_policy,
            FinalStageFailurePolicy::RejectCandidate
        );
    }

    #[test]
    fn parses_run_job_with_broadcast_and_indexed_thresholds() {
        let config = NativeScottProcedureConfig::from_run_job_text(
            r#"
            JOB_TYPE:2
            N_DEF_ENERGY:GULP, AIMS, VASP
            N_RELAXATION_ATTEMPTS:4
            GNORM:0.0025
            ONLY_2ND_ENERGY:.TRUE.
            R_MIN_CLUSTER_ENERGY:-100.0
            R_MIN_CLUSTER_ENERGY_2:-80.0
            R_MAX_CLUSTER_ENERGY_3:-1.0
            R_REFINE_THRESHOLD:-10.0
            R_REFINE_THRESHOLD_2:-25.0
            "#,
        )
        .unwrap();

        assert_eq!(config.engine_codes, vec!['G', 'A', 'V']);
        assert_eq!(
            config.refine_thresholds,
            vec![Some(-10.0), Some(-25.0), Some(-10.0)]
        );
        assert_eq!(
            config.energy_min_thresholds,
            vec![Some(-100.0), Some(-80.0), Some(-100.0)]
        );
        assert_eq!(config.energy_max_thresholds, vec![None, None, Some(-1.0)]);
        assert_eq!(config.max_relaxation_attempts, 4);
        assert_eq!(config.gnorm_tolerance, 0.0025);
        assert!(config.only_2nd_energy);
        assert!(config.retrieve_relaxed_geometry);
        assert!(config.keep_stage_artifacts);
    }

    #[test]
    fn parses_native_run_job_synonyms_and_defaults() {
        let config = NativeScottProcedureConfig::from_run_job_text(
            r#"
            N_DEF_ENERGY:GULP, CRYSTAL, R, CRYSTAL23
            R_GNORM_TOL:0.01
            L_ONLY_2ND_ENERGY:.FALSE.
            "#,
        )
        .unwrap();

        assert_eq!(config.engine_codes, vec!['G', 'R', 'R', 'R']);
        assert_eq!(config.max_relaxation_attempts, 1);
        assert_eq!(config.gnorm_tolerance, 0.01);
        assert!(!config.only_2nd_energy);
    }

    #[test]
    fn rejects_unknown_energy_definition_token() {
        let error =
            NativeScottProcedureConfig::from_run_job_text("N_DEF_ENERGY:GULP,FOO").unwrap_err();

        assert!(matches!(
            error,
            NativeScottProcedureParseError::UnknownEnergyDefinition(ref value)
                if value == "FOO"
        ));
    }
}
