use super::driver_support::ensure_native_scott_search_candidate;
use super::duplicate_parity::{ExternalHashkeyDuplicatePolicy, RustJanusDuplicatePolicy};
use super::filter_taxonomy::FilterDescriptor;
use super::janus_operator_policy::{
    select_janus_ga_operator_policy, GaOperatorBackendProfile, JanusGaOperatorPolicy,
};
use crate::application::scott_topology_export::parse_atoms_file;
use crate::{absolutize_path, default_worker_count, enforce_external_initial_uniqueness};
use anyhow::{anyhow, Context, Result};
use patina_evaluator::ScottDuplicateReason;
use patina_external::{JanusMaceConfig, PersistentJanusMaceBackend};
use patina_runner::{PersistentWorkerPool, RunConfig};
use patina_search::{ScottDuplicateClassifier, ScottParityGeneticAlgorithm};
use patina_types::{Candidate, SearchConfig};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type RustJanusGaController = ScottParityGeneticAlgorithm<RustJanusDuplicatePolicy>;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum RustJanusLaneMode {
    StandaloneCapable,
}

impl RustJanusLaneMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandaloneCapable => "standalone_capable",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum RustJanusDuplicatePolicyMode {
    ExternalNativeHashkey,
    ClassifierOnly,
}

impl RustJanusDuplicatePolicyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExternalNativeHashkey => "external_native_hashkey",
            Self::ClassifierOnly => "classifier_only",
        }
    }
}

/// Controller-local bootstrap consumed by the GA workflow service.
pub struct RustJanusGaControllerBootstrap {
    pub search_cfg: SearchConfig,
    pub controller: RustJanusGaController,
    pub start_state: RustJanusGaStartState,
}

/// Scientific duplicate/evidence policy metadata attached to one GA lane.
pub struct RustJanusGaEvidencePolicy {
    pub duplicate_policy_mode: RustJanusDuplicatePolicyMode,
    pub duplicate_filter_stack: Vec<FilterDescriptor>,
    pub duplicate_trace: SharedScottDuplicateTrace,
}

/// Lane-level metadata that should not be bundled into controller bootstrap state.
pub struct RustJanusGaLaneMetadata {
    pub operator_policy: JanusGaOperatorPolicy,
    pub lane_mode: RustJanusLaneMode,
}

/// Full GA setup assembled by the driver before workflow execution.
///
/// This struct intentionally separates controller bootstrap from lane metadata and
/// duplicate/evidence policy so higher-level orchestration can reuse the scientific
/// metadata without forcing workflow services to depend on unrelated setup concerns.
pub struct RustJanusGaCoreSetup {
    pub controller_bootstrap: RustJanusGaControllerBootstrap,
    pub evidence_policy: RustJanusGaEvidencePolicy,
    pub lane_metadata: RustJanusGaLaneMetadata,
}

pub struct RustJanusGaSetup {
    pub core: RustJanusGaCoreSetup,
    pub python_bin: PathBuf,
    pub adapter_script: PathBuf,
    pub pool: PersistentWorkerPool,
}

/// Application-level bootstrap for the Rust-owned GA lane.
///
/// This is intentionally CLI-agnostic so the driver can translate command-line
/// arguments in `main.rs` without leaking that shape into the application layer.
#[derive(Debug, Clone)]
pub struct RustJanusGaCoreRequest {
    pub base_candidate: Option<Candidate>,
    pub base_candidate_source_path: Option<PathBuf>,
    pub workdir: PathBuf,
    pub run_dir: PathBuf,
    pub resume_from_checkpoint: Option<PathBuf>,
    pub requested_generations: usize,
    pub population_size: usize,
    pub seed: Option<u64>,
    pub temperature: f64,
    pub step_size: f64,
    pub operator_policy_backend: Option<String>,
    pub janus_mode: crate::DriverJanusMode,
    pub atoms_in_template: Option<PathBuf>,
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub pmoi_tolerance: f64,
    pub enable_pmoi: bool,
    pub operator_overrides: RustJanusGaOperatorOverrides,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RustJanusGaOperatorOverrides {
    pub pop_replacement_ratio: Option<f64>,
    pub reinsert_elites_ratio: Option<f64>,
    pub max_repop_attempts: Option<usize>,
    pub mutation_ratio: Option<f64>,
    pub mut_selfcross_ratio: Option<f64>,
    pub crossover_attempts: Option<usize>,
    pub tournament_size_min: Option<usize>,
    pub tournament_size_max: Option<usize>,
}

/// Backend/runtime setup for the persistent Janus evaluation adapter.
#[derive(Debug, Clone)]
pub struct RustJanusGaBackendSetupRequest {
    pub workdir: PathBuf,
    pub workers: Option<usize>,
    pub keep_dirs: bool,
    pub timeout_secs: Option<u64>,
    pub python_bin: Option<PathBuf>,
    pub janus_adapter_script: Option<PathBuf>,
    pub janus_arch: String,
    pub janus_model: String,
    pub janus_device: String,
    pub janus_dtype: String,
    pub janus_mode: crate::DriverJanusMode,
    pub janus_optimizer: crate::DriverJanusOptimizer,
    pub janus_fmax: f64,
    pub janus_steps: usize,
}

pub enum RustJanusGaStartState {
    Fresh {
        initial_candidates: Vec<Candidate>,
    },
    Resume {
        completed_generation: usize,
        population: Vec<patina_search::ScottGaMember>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ScottGaDuplicateTraceRecord {
    pub left_source_label: String,
    pub right_source_label: String,
    pub left_relaxed_label: String,
    pub right_relaxed_label: String,
    pub left_energy: f64,
    pub right_energy: f64,
    pub left_hashkey: Option<String>,
    pub right_hashkey: Option<String>,
    pub exact_hashkey_match: Option<bool>,
    pub reason: ScottDuplicateReason,
}

pub type SharedScottDuplicateTrace = Arc<Mutex<Vec<ScottGaDuplicateTraceRecord>>>;

pub fn snapshot_duplicate_trace(
    trace: &SharedScottDuplicateTrace,
) -> Vec<ScottGaDuplicateTraceRecord> {
    match trace.lock() {
        Ok(records) => records.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

pub fn prepare_rust_janus_ga_core(
    request: &RustJanusGaCoreRequest,
) -> Result<RustJanusGaCoreSetup> {
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;

    let resume_checkpoint = request
        .resume_from_checkpoint
        .as_deref()
        .map(crate::application::rust_janus_ga_checkpoint::load_checkpoint)
        .transpose()?;
    let (base_candidate, seed, search_cfg, operator_policy, start_state): (
        Candidate,
        u64,
        SearchConfig,
        JanusGaOperatorPolicy,
        RustJanusGaStartState,
    ) = if let Some(checkpoint) = resume_checkpoint {
        if request.requested_generations <= checkpoint.generation_completed {
            return Err(anyhow!(
                "`--ga-generations` ({}) must be greater than checkpoint generation {}",
                request.requested_generations,
                checkpoint.generation_completed
            ));
        }
        let base_candidate = checkpoint
            .generation_state
            .population
            .first()
            .map(|member| Candidate::from(&member.evaluation.structure))
            .ok_or_else(|| anyhow!("resume checkpoint contains no population members"))?;
        let seed = checkpoint.seed.unwrap_or(1);
        let search_cfg = SearchConfig {
            temperature: checkpoint.search_config.temperature,
            step_size: checkpoint.search_config.step_size,
            population_size: checkpoint.search_config.population_size,
            max_steps: request.requested_generations,
            seed: checkpoint.search_config.seed.or(Some(seed)),
        };
        ensure_native_scott_search_candidate(&base_candidate, "Rust Janus GA resume")?;
        let population =
            crate::application::rust_janus_ga_checkpoint::checkpoint_population(&checkpoint)?;
        (
            base_candidate,
            seed,
            search_cfg,
            checkpoint.operator_policy.clone(),
            RustJanusGaStartState::Resume {
                completed_generation: checkpoint.generation_completed,
                population,
            },
        )
    } else {
        if request.requested_generations == 0 {
            return Err(anyhow!("`--ga-generations` must be at least 1"));
        }
        if request.population_size < 2 {
            return Err(anyhow!("`--population` must be at least 2"));
        }
        let base_candidate = request.base_candidate.clone().ok_or_else(|| {
            anyhow!("base candidate input is required unless resuming from a checkpoint")
        })?;
        ensure_native_scott_search_candidate(&base_candidate, "Rust Janus GA setup")?;
        let seed = request.seed.unwrap_or(1);
        let search_cfg = SearchConfig {
            temperature: request.temperature,
            step_size: request.step_size,
            population_size: request.population_size,
            max_steps: request.requested_generations,
            seed: Some(seed),
        };
        let mut operator_policy = select_janus_ga_operator_policy(
            match request.operator_policy_backend.as_deref() {
                Some("gulp") => GaOperatorBackendProfile::Gulp,
                _ => GaOperatorBackendProfile::JanusMace,
            },
            request.janus_mode,
            request.step_size,
        );
        apply_operator_overrides(&mut operator_policy, &request.operator_overrides);
        (
            base_candidate,
            seed,
            search_cfg,
            operator_policy,
            RustJanusGaStartState::Fresh {
                initial_candidates: Vec::new(),
            },
        )
    };

    let duplicate_classifier = if request.enable_pmoi {
        ScottDuplicateClassifier::pmoi_energy_fallback_with_pmoi_tolerance(request.pmoi_tolerance)
    } else {
        ScottDuplicateClassifier::default()
    };
    let duplicate_trace: SharedScottDuplicateTrace = Arc::new(Mutex::new(Vec::new()));
    let (duplicate_policy_mode, duplicate_policy, atom_specs, hkg_path) =
        if request.use_dreadnaut_keys {
            let atom_specs = request
                .atoms_in_template
                .as_ref()
                .map(|atoms_in_template| {
                    absolutize_path(atoms_in_template)
                        .and_then(|atoms_in_template| parse_atoms_file(&atoms_in_template))
                })
                .transpose()?;
            let hkg_path = super::driver_support::verify_hashkey_dreadnaut_adapter(
                &patina_dreadnaut::bundled_dreadnaut_path(),
                "Rust Janus GA external-native-hashkey preflight",
            )?;
            (
                RustJanusDuplicatePolicyMode::ExternalNativeHashkey,
                RustJanusDuplicatePolicy::External(ExternalHashkeyDuplicatePolicy::new(
                    duplicate_classifier.clone(),
                    atom_specs.clone(),
                    request.hashkey_radius.clone(),
                    request.hashkey_radius_const,
                    hkg_path.clone(),
                    request.workdir.join("hashkey_runtime"),
                    duplicate_trace.clone(),
                )?),
                atom_specs,
                Some(hkg_path),
            )
        } else {
            (
                RustJanusDuplicatePolicyMode::ClassifierOnly,
                RustJanusDuplicatePolicy::Classifier(duplicate_classifier),
                None,
                None,
            )
        };
    let duplicate_filter_stack =
        super::filter_taxonomy::rust_janus_filter_stack(duplicate_policy_mode);
    let mut controller = ScottParityGeneticAlgorithm::with_configs(
        base_candidate.clone(),
        seed,
        operator_policy.to_search_config(),
        duplicate_policy,
    );
    let start_state = match start_state {
        RustJanusGaStartState::Fresh { .. } => {
            let mut initial_candidates = controller.initialize_population(&search_cfg);
            if let Some(hkg_path) = hkg_path.as_deref() {
                enforce_external_initial_uniqueness(
                    &mut controller,
                    &mut initial_candidates,
                    atom_specs.as_deref(),
                    &request.hashkey_radius,
                    request.hashkey_radius_const,
                    hkg_path,
                    &request.workdir.join("hashkey_runtime_initial"),
                )?;
            }
            RustJanusGaStartState::Fresh { initial_candidates }
        }
        RustJanusGaStartState::Resume {
            completed_generation,
            population,
        } => {
            controller.configure_for_run(&search_cfg);
            RustJanusGaStartState::Resume {
                completed_generation,
                population,
            }
        }
    };

    Ok(RustJanusGaCoreSetup {
        controller_bootstrap: RustJanusGaControllerBootstrap {
            search_cfg,
            controller,
            start_state,
        },
        evidence_policy: RustJanusGaEvidencePolicy {
            duplicate_policy_mode,
            duplicate_filter_stack,
            duplicate_trace,
        },
        lane_metadata: RustJanusGaLaneMetadata {
            operator_policy,
            lane_mode: RustJanusLaneMode::StandaloneCapable,
        },
    })
}

fn apply_operator_overrides(
    policy: &mut JanusGaOperatorPolicy,
    overrides: &RustJanusGaOperatorOverrides,
) {
    if let Some(value) = overrides.pop_replacement_ratio {
        policy.operator_config.pop_replacement_ratio = value.clamp(0.0, 1.0);
    }
    if let Some(value) = overrides.reinsert_elites_ratio {
        policy.operator_config.reinsert_elites_ratio = value.clamp(0.0, 1.0);
    }
    if let Some(value) = overrides.max_repop_attempts {
        policy.operator_config.max_repop_attempts = value.max(1);
    }
    if let Some(value) = overrides.mutation_ratio {
        policy.operator_config.mutation_ratio = value.clamp(0.0, 1.0);
    }
    if let Some(value) = overrides.mut_selfcross_ratio {
        policy.operator_config.mut_selfcross_ratio = value.clamp(0.0, 1.0);
    }
    if let Some(value) = overrides.crossover_attempts {
        policy.operator_config.crossover_attempts = value.max(1);
    }
    if let Some(value) = overrides.tournament_size_min {
        policy.operator_config.tournament_size_min = value.max(1);
    }
    if let Some(value) = overrides.tournament_size_max {
        policy.operator_config.tournament_size_max = value.max(1);
    }
    if policy.operator_config.tournament_size_min > policy.operator_config.tournament_size_max {
        std::mem::swap(
            &mut policy.operator_config.tournament_size_min,
            &mut policy.operator_config.tournament_size_max,
        );
    }
    if overrides != &RustJanusGaOperatorOverrides::default() {
        policy.profile_name = format!("{}_overridden", policy.profile_name);
        policy.rationale.push(
            "operator settings were overridden explicitly for this run, so controller-side selection and replacement pressure no longer match the built-in backend profile defaults".to_string(),
        );
    }
}

pub fn prepare_rust_janus_ga_setup(
    core_request: &RustJanusGaCoreRequest,
    backend_request: &RustJanusGaBackendSetupRequest,
) -> Result<RustJanusGaSetup> {
    let core = prepare_rust_janus_ga_core(core_request)?;
    let python_bin = absolutize_path(
        &backend_request
            .python_bin
            .clone()
            .unwrap_or_else(patina_external::default_janus_python_bin),
    )?;
    let adapter_script = absolutize_path(
        &backend_request
            .janus_adapter_script
            .clone()
            .unwrap_or_else(patina_external::default_janus_adapter_script),
    )?;
    let timeout = backend_request.timeout_secs.map(Duration::from_secs);
    let janus_config = JanusMaceConfig {
        python_bin: python_bin.clone(),
        adapter_script: adapter_script.clone(),
        arch: backend_request.janus_arch.clone(),
        model: backend_request.janus_model.clone(),
        device: backend_request.janus_device.clone(),
        default_dtype: backend_request.janus_dtype.clone(),
        mode: backend_request.janus_mode.into(),
        optimizer: backend_request.janus_optimizer.into(),
        fmax: backend_request.janus_fmax,
        steps: backend_request.janus_steps,
    };
    let backend = PersistentJanusMaceBackend::new(
        janus_config,
        timeout,
        backend_request.workdir.join("janus_persistent_session"),
        backend_request.keep_dirs,
    )?;
    let pool = PersistentWorkerPool::new(RunConfig {
        backend: Arc::new(backend),
        workdir: backend_request.workdir.join("worker_pool"),
        n_workers: backend_request.workers.unwrap_or_else(default_worker_count),
        keep_dirs: backend_request.keep_dirs,
    })?;

    Ok(RustJanusGaSetup {
        core,
        python_bin,
        adapter_script,
        pool,
    })
}

#[cfg(test)]
mod tests {
    use super::{apply_operator_overrides, RustJanusGaOperatorOverrides};
    use crate::application::janus_operator_policy::{
        select_janus_ga_operator_policy, GaOperatorBackendProfile,
    };
    use crate::DriverJanusMode;

    #[test]
    fn operator_overrides_replace_controller_policy_values() {
        let mut policy = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::Gulp,
            DriverJanusMode::LocalOpt,
            0.24,
        );
        apply_operator_overrides(
            &mut policy,
            &RustJanusGaOperatorOverrides {
                pop_replacement_ratio: Some(0.8),
                reinsert_elites_ratio: Some(0.2),
                max_repop_attempts: Some(48),
                mutation_ratio: Some(0.9),
                mut_selfcross_ratio: Some(0.1),
                crossover_attempts: Some(6),
                tournament_size_min: Some(2),
                tournament_size_max: Some(3),
            },
        );

        assert_eq!(policy.operator_config.pop_replacement_ratio, 0.8);
        assert_eq!(policy.operator_config.reinsert_elites_ratio, 0.2);
        assert_eq!(policy.operator_config.max_repop_attempts, 48);
        assert_eq!(policy.operator_config.mutation_ratio, 0.9);
        assert_eq!(policy.operator_config.mut_selfcross_ratio, 0.1);
        assert_eq!(policy.operator_config.crossover_attempts, 6);
        assert_eq!(policy.operator_config.tournament_size_min, 2);
        assert_eq!(policy.operator_config.tournament_size_max, 3);
        assert!(policy.profile_name.ends_with("_overridden"));
    }
}
