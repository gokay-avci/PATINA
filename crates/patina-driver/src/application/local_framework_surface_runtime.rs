use crate::application::ports::FrameworkNormalizationPort;
use anyhow::{anyhow, Context, Result};
use patina_external::{BackendEvaluator, JanusMaceConfig, PersistentJanusMaceBackend};
use patina_raspa::{
    DensityGrid, DensityGridBinning, DensityGridNormalization, EnergyHistogram,
    EnergyHistogramSpec, GcmcConditions, GcmcEnergyEvaluator, GcmcEngine, GcmcMoveSchedule,
    GcmcRequest, GcmcResult, MoyoSymmetryAnalyzer, NumberHistogram, NumberHistogramSpec,
    PeriodicFramework, RigidGuestGcmcEngine, RigidGuestTemplate, SymmetryAnalyzer,
    SymmetryTolerance,
};
use patina_surface::{
    DefaultSurfaceGenerationEngine, HeuristicSurfacePolarityAnalyzer, SurfaceFace,
    SurfaceGenerationConfig, SurfaceGenerationEngine, SurfacePolarityAnalyzer,
};
use patina_types::Candidate;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use super::driver_support::{
    absolutize_path, candidate_from_xyz, read_candidate_json, EvalBackendKind, JanusModeSetting,
    JanusOptimizerSetting,
};
use super::workflow_provenance::{build_workflow_provenance, WorkflowProvenanceSpec};

#[derive(Debug, Clone)]
pub(crate) struct PeriodicStructureInput {
    pub candidate_json: Option<PathBuf>,
    pub structure_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct FrameworkSymmetryRunRequest {
    pub input: PeriodicStructureInput,
    pub run_dir: PathBuf,
    pub system: Option<String>,
    pub tolerance: SymmetryTolerance,
}

#[derive(Debug, Clone)]
pub(crate) struct FrameworkNormalizationRunRequest {
    pub input: PeriodicStructureInput,
    pub run_dir: PathBuf,
    pub system: Option<String>,
    pub tolerance: SymmetryTolerance,
    pub target: super::ports::FrameworkNormalizationTarget,
}

#[derive(Debug, Clone)]
pub(crate) struct FrameworkGcmcRunRequest {
    pub input: PeriodicStructureInput,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: Option<String>,
    pub guest: String,
    pub temperature_kelvin: f64,
    pub pressure_bar: f64,
    pub initialization_cycles: usize,
    pub production_cycles: usize,
    pub max_guest_count: usize,
    pub minimum_guest_host_distance: f64,
    pub minimum_guest_guest_distance: f64,
    pub density_dimensions: [usize; 3],
    pub density_binning: DensityGridBinning,
    pub density_normalization: DensityGridNormalization,
    pub energy_histogram_bins: usize,
    pub energy_histogram_range: (f64, f64),
    pub number_histogram_limits: (usize, usize),
    pub seed: u64,
    pub python_bin: Option<PathBuf>,
    pub janus_adapter_script: Option<PathBuf>,
    pub janus_arch: String,
    pub janus_model: String,
    pub janus_device: String,
    pub janus_dtype: String,
    pub janus_mode: JanusModeSetting,
    pub janus_fmax: f64,
    pub janus_steps: usize,
    pub janus_optimizer: JanusOptimizerSetting,
}

#[derive(Debug, Clone)]
pub(crate) struct SurfaceGenerationRunRequest {
    pub input: PeriodicStructureInput,
    pub run_dir: PathBuf,
    pub system: Option<String>,
    pub config: SurfaceGenerationConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct SurfacePolarityRunRequest {
    pub input: PeriodicStructureInput,
    pub run_dir: PathBuf,
    pub system: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SurfaceReconstructionRunRequest {
    pub input: PeriodicStructureInput,
    pub run_dir: PathBuf,
    pub workdir: PathBuf,
    pub system: Option<String>,
    pub generation_config: SurfaceGenerationConfig,
    pub target_face: SurfaceFace,
    pub movable_species: Vec<String>,
    pub site_filter: super::surface_reconstruction::SurfaceReconstructionSiteFilter,
    pub region_policy: super::surface_reconstruction::SurfaceReconstructionRegionPolicy,
    pub movable_layer_count: Option<usize>,
    pub layer_z_tolerance_angstrom: f64,
    pub move_family: super::surface_reconstruction::SurfaceReconstructionMoveFamily,
    pub steps: usize,
    pub temperature: f64,
    pub lateral_fractional_step: f64,
    pub outward_normal_step_angstrom: f64,
    pub inward_normal_step_angstrom: f64,
    pub evaluate_initial_state: bool,
    pub seed: u64,
    pub top_n_exports: usize,
    pub backend: super::local_sampling_runtime::LocalSamplingBackendConfig,
}

struct LocalSurfaceGenerationPort {
    engine: DefaultSurfaceGenerationEngine,
}

impl super::ports::SurfaceGenerationPort for LocalSurfaceGenerationPort {
    fn generate_surface(
        &self,
        request: &super::ports::SurfaceGenerationRequest,
    ) -> Result<patina_surface::SurfaceGenerationResult> {
        Ok(self
            .engine
            .generate_surface(&patina_surface::SurfaceGenerationRequest {
                parent: request.parent.clone(),
                config: request.config.clone(),
            })?)
    }
}

struct LocalSurfacePolarityPort {
    analyzer: HeuristicSurfacePolarityAnalyzer,
}

impl super::ports::SurfacePolarityPort for LocalSurfacePolarityPort {
    fn analyze_surface_polarity(
        &self,
        request: &super::ports::SurfacePolarityRequest,
    ) -> Result<patina_surface::SurfacePolarityReport> {
        Ok(self.analyzer.analyze_surface_polarity(&request.slab)?)
    }
}

struct LocalSurfaceReconstructionEvaluationPort<'a> {
    backend: &'a dyn BackendEvaluator,
}

impl super::ports::SamplingEvaluationPort for LocalSurfaceReconstructionEvaluationPort<'_> {
    fn evaluate(
        &self,
        request: &super::ports::SamplingEvaluationRequest,
    ) -> Result<patina_types::EvalResult> {
        request.validate_shape()?;
        fs::create_dir_all(&request.eval_dir).with_context(|| {
            format!(
                "failed to create surface reconstruction evaluation dir `{}`",
                request.eval_dir.display()
            )
        })?;
        self.backend
            .evaluate(&request.candidate, &request.eval_dir)
            .map_err(Into::into)
    }
}

struct LocalFrameworkSymmetryPort {
    analyzer: MoyoSymmetryAnalyzer,
}

impl super::ports::FrameworkSymmetryPort for LocalFrameworkSymmetryPort {
    fn analyze_framework_symmetry(
        &self,
        request: &super::ports::FrameworkSymmetryRequest,
    ) -> Result<patina_raspa::SymmetryAnalysis> {
        Ok(self
            .analyzer
            .analyze(&request.framework, request.tolerance)?)
    }
}

struct LocalFrameworkNormalizationPort {
    analyzer: MoyoSymmetryAnalyzer,
}

impl super::ports::FrameworkNormalizationPort for LocalFrameworkNormalizationPort {
    fn normalize_framework(
        &self,
        request: &super::ports::FrameworkNormalizationRequest,
    ) -> Result<patina_raspa::PeriodicFramework> {
        let analysis = self
            .analyzer
            .analyze(&request.framework, request.tolerance)?;
        match request.target {
            super::ports::FrameworkNormalizationTarget::Standardized => {
                analysis.standardized_framework.ok_or_else(|| {
                    anyhow!("symmetry analysis did not produce a standardized framework")
                })
            }
            super::ports::FrameworkNormalizationTarget::PrimitiveStandardized => {
                analysis.primitive_standardized_framework.ok_or_else(|| {
                    anyhow!("symmetry analysis did not produce a primitive standardized framework")
                })
            }
        }
    }
}

struct LocalJanusFrameworkEnergyEvaluator {
    backend: PersistentJanusMaceBackend,
    workdir: PathBuf,
    next_eval_index: Mutex<usize>,
}

impl GcmcEnergyEvaluator for LocalJanusFrameworkEnergyEvaluator {
    fn evaluate_configuration(
        &self,
        candidate: &Candidate,
    ) -> std::result::Result<patina_types::EvalResult, patina_raspa::RaspaInterfaceError> {
        let mut index = self.next_eval_index.lock().map_err(|_| {
            patina_raspa::RaspaInterfaceError::EnergyEvaluation(
                "framework GCMC evaluator mutex was poisoned".into(),
            )
        })?;
        let eval_dir = self.workdir.join(format!("eval_{:06}", *index));
        *index += 1;
        fs::create_dir_all(&eval_dir).map_err(|err| {
            patina_raspa::RaspaInterfaceError::EnergyEvaluation(format!(
                "failed to create framework GCMC eval dir `{}`: {err}",
                eval_dir.display()
            ))
        })?;
        self.backend
            .evaluate(candidate, &eval_dir)
            .map_err(|err| patina_raspa::RaspaInterfaceError::EnergyEvaluation(format!("{err}")))
    }
}

struct LocalFrameworkGcmcPort {
    engine: RigidGuestGcmcEngine<LocalJanusFrameworkEnergyEvaluator>,
}

impl super::ports::FrameworkGcmcPort for LocalFrameworkGcmcPort {
    fn run_framework_gcmc(&self, request: &GcmcRequest) -> Result<GcmcResult> {
        Ok(self.engine.run(request)?)
    }
}

struct LocalFrameworkSymmetryArtifactSink {
    run_dir: PathBuf,
    system: String,
    source_path: PathBuf,
}

impl super::ports::FrameworkSymmetryArtifactSink for LocalFrameworkSymmetryArtifactSink {
    fn persist_framework_symmetry_run(
        &self,
        execution: &super::framework_symmetry::FrameworkSymmetryExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        fs::create_dir_all(&raw_dir)
            .with_context(|| format!("failed to create raw dir `{}`", raw_dir.display()))?;
        fs::create_dir_all(&outputs_dir)
            .with_context(|| format!("failed to create outputs dir `{}`", outputs_dir.display()))?;

        let standardized_candidate = execution
            .analysis
            .standardized_framework
            .as_ref()
            .map(PeriodicFramework::to_candidate);
        let primitive_standardized_candidate = execution
            .analysis
            .primitive_standardized_framework
            .as_ref()
            .map(PeriodicFramework::to_candidate);

        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("framework_symmetry"),
                "mode": "framework_symmetry",
                "system": self.system,
                "engine": "Rust Framework Symmetry (Moyo)",
                "source_path": self.source_path,
                "artifacts": {
                    "source_candidate": "raw/source_candidate.json",
                    "framework": "raw/framework.json",
                    "symmetry_analysis": "raw/symmetry_analysis.json",
                    "standardized_framework": "outputs/standardized_framework.json",
                    "primitive_standardized_framework": "outputs/primitive_standardized_framework.json",
                    "standardized_candidate": "outputs/standardized_candidate.json",
                    "primitive_standardized_candidate": "outputs/primitive_standardized_candidate.json"
                },
                "summary": {
                    "hall_number": execution.analysis.hall_number,
                    "international_number": execution.analysis.international_number,
                    "hm_symbol": execution.analysis.hm_symbol,
                    "operation_count": execution.analysis.operations.len(),
                    "orbit_count": execution.analysis.orbits.len(),
                    "wyckoff_count": execution.analysis.wyckoff_letters.len()
                },
                "tolerance": execution.tolerance
            }))
            .context("failed to serialize framework symmetry manifest")?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;

        fs::write(
            raw_dir.join("source_candidate.json"),
            serde_json::to_string_pretty(&execution.source_candidate)
                .context("failed to serialize source candidate")?,
        )
        .with_context(|| {
            format!(
                "failed to write source_candidate.json in `{}`",
                raw_dir.display()
            )
        })?;
        fs::write(
            raw_dir.join("framework.json"),
            serde_json::to_string_pretty(&execution.framework)
                .context("failed to serialize periodic framework")?,
        )
        .with_context(|| format!("failed to write framework.json in `{}`", raw_dir.display()))?;
        fs::write(
            raw_dir.join("symmetry_analysis.json"),
            serde_json::to_string_pretty(&execution.analysis)
                .context("failed to serialize symmetry analysis")?,
        )
        .with_context(|| {
            format!(
                "failed to write symmetry_analysis.json in `{}`",
                raw_dir.display()
            )
        })?;

        if let Some(standardized_framework) = execution.analysis.standardized_framework.as_ref() {
            fs::write(
                outputs_dir.join("standardized_framework.json"),
                serde_json::to_string_pretty(standardized_framework)
                    .context("failed to serialize standardized framework")?,
            )
            .with_context(|| {
                format!(
                    "failed to write standardized_framework.json in `{}`",
                    outputs_dir.display()
                )
            })?;
        }
        if let Some(primitive_standardized_framework) =
            execution.analysis.primitive_standardized_framework.as_ref()
        {
            fs::write(
                outputs_dir.join("primitive_standardized_framework.json"),
                serde_json::to_string_pretty(primitive_standardized_framework)
                    .context("failed to serialize primitive standardized framework")?,
            )
            .with_context(|| {
                format!(
                    "failed to write primitive_standardized_framework.json in `{}`",
                    outputs_dir.display()
                )
            })?;
        }
        if let Some(candidate) = standardized_candidate.as_ref() {
            fs::write(
                outputs_dir.join("standardized_candidate.json"),
                serde_json::to_string_pretty(candidate)
                    .context("failed to serialize standardized candidate")?,
            )
            .with_context(|| {
                format!(
                    "failed to write standardized_candidate.json in `{}`",
                    outputs_dir.display()
                )
            })?;
        }
        if let Some(candidate) = primitive_standardized_candidate.as_ref() {
            fs::write(
                outputs_dir.join("primitive_standardized_candidate.json"),
                serde_json::to_string_pretty(candidate)
                    .context("failed to serialize primitive standardized candidate")?,
            )
            .with_context(|| {
                format!(
                    "failed to write primitive_standardized_candidate.json in `{}`",
                    outputs_dir.display()
                )
            })?;
        }

        Ok(())
    }
}

struct LocalFrameworkNormalizationArtifactSink {
    run_dir: PathBuf,
    system: String,
    source_path: PathBuf,
}

impl super::ports::FrameworkNormalizationArtifactSink for LocalFrameworkNormalizationArtifactSink {
    fn persist_framework_normalization_run(
        &self,
        execution: &super::framework_normalization::FrameworkNormalizationExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        fs::create_dir_all(&raw_dir)
            .with_context(|| format!("failed to create raw dir `{}`", raw_dir.display()))?;
        fs::create_dir_all(&outputs_dir)
            .with_context(|| format!("failed to create outputs dir `{}`", outputs_dir.display()))?;

        let normalized_candidate = execution.normalized_framework.to_candidate();
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("framework_normalization"),
                "mode": "framework_normalization",
                "system": self.system,
                "engine": "Rust Framework Normalization (Moyo)",
                "source_path": self.source_path,
                "target": execution.target,
                "artifacts": {
                    "source_candidate": "raw/source_candidate.json",
                    "framework": "raw/framework.json",
                    "normalized_framework": "outputs/normalized_framework.json",
                    "normalized_candidate": "outputs/normalized_candidate.json"
                },
                "tolerance": execution.tolerance
            }))
            .context("failed to serialize framework normalization manifest")?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;

        fs::write(
            raw_dir.join("source_candidate.json"),
            serde_json::to_string_pretty(&execution.source_candidate)
                .context("failed to serialize source candidate")?,
        )?;
        fs::write(
            raw_dir.join("framework.json"),
            serde_json::to_string_pretty(&execution.framework)
                .context("failed to serialize input framework")?,
        )?;
        fs::write(
            outputs_dir.join("normalized_framework.json"),
            serde_json::to_string_pretty(&execution.normalized_framework)
                .context("failed to serialize normalized framework")?,
        )?;
        fs::write(
            outputs_dir.join("normalized_candidate.json"),
            serde_json::to_string_pretty(&normalized_candidate)
                .context("failed to serialize normalized candidate")?,
        )?;
        Ok(())
    }
}

struct LocalFrameworkGcmcArtifactSink {
    run_dir: PathBuf,
    workdir: PathBuf,
    system: String,
    source_path: PathBuf,
}

impl super::ports::FrameworkGcmcArtifactSink for LocalFrameworkGcmcArtifactSink {
    fn persist_framework_gcmc_run(
        &self,
        execution: &super::framework_gcmc::FrameworkGcmcExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        let density_dir = self.run_dir.join("density_grids");
        let histogram_dir = self.run_dir.join("energy_histogram");
        let number_histogram_dir = self.run_dir.join("number_histogram");
        fs::create_dir_all(&raw_dir)?;
        fs::create_dir_all(&outputs_dir)?;
        fs::create_dir_all(&density_dir)?;
        fs::create_dir_all(&histogram_dir)?;
        fs::create_dir_all(&number_histogram_dir)?;

        let density_grid_path = execution.result.density_grid.as_ref().map(|_| {
            density_dir.join(format!(
                "grid_unitcell_component_{}.s0.cube",
                execution.request.guest_name
            ))
        });
        let energy_histogram_path = execution
            .result
            .energy_histogram
            .as_ref()
            .map(|_| histogram_dir.join("energy_histogram.s0.txt"));
        let number_histogram_path = execution
            .result
            .number_histogram
            .as_ref()
            .map(|_| number_histogram_dir.join("number_histogram.s0.txt"));

        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("framework_gcmc"),
                "workflow_id": "framework.gcmc",
                "mode": "framework_gcmc",
                "system": self.system,
                "engine": "Rust Framework GCMC (patina-raspa domain + Janus/MACE periodic backend)",
                "source_path": self.source_path,
                "backend": {
                    "force_evaluator": "janus_mace",
                    "ownership": "backend evaluation remains on the PATINA Rust boundary; RASPA3 native force fields are not used"
                },
                "provenance": serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "framework.gcmc",
                    workflow_owner: "framework_gcmc",
                    backend_id: Some("janus_mace"),
                    backend_mode: None,
                    lane_mode: None,
                    duplicate_policy_mode: None,
                    parallel_contract: None,
                    run_dir: Some(self.run_dir.as_path()),
                    workdir: Some(self.workdir.as_path()),
                    source_run_dir: None,
                    source_path: Some(self.source_path.as_path()),
                    candidate_json: None,
                    template_bundle: None,
                })?)?,
                "guest": execution.request.guest_name,
                "artifacts": {
                    "source_candidate": "raw/source_candidate.json",
                    "framework": "raw/framework.json",
                    "gcmc_request": "raw/gcmc_request.json",
                    "gcmc_result": "raw/gcmc_result.json",
                    "final_candidate": "outputs/final_candidate.json",
                    "production_trace": "outputs/production_trace.json",
                    "energy_histogram": energy_histogram_path.as_ref().map(|path| relativize_under(&self.run_dir, path)),
                    "number_histogram": number_histogram_path.as_ref().map(|path| relativize_under(&self.run_dir, path)),
                    "density_grid": density_grid_path.as_ref().map(|path| relativize_under(&self.run_dir, path))
                },
                "summary": execution.result.summary,
            }))
            .context("failed to serialize framework GCMC manifest")?,
        )?;

        fs::write(
            raw_dir.join("source_candidate.json"),
            serde_json::to_string_pretty(&execution.source_candidate)?,
        )?;
        fs::write(
            raw_dir.join("framework.json"),
            serde_json::to_string_pretty(&execution.framework)?,
        )?;
        fs::write(
            raw_dir.join("gcmc_request.json"),
            serde_json::to_string_pretty(&execution.request)?,
        )?;
        fs::write(
            raw_dir.join("gcmc_result.json"),
            serde_json::to_string_pretty(&execution.result)?,
        )?;
        fs::write(
            outputs_dir.join("final_candidate.json"),
            serde_json::to_string_pretty(&execution.result.final_candidate)?,
        )?;
        fs::write(
            outputs_dir.join("production_trace.json"),
            serde_json::to_string_pretty(&execution.result.production_trace)?,
        )?;

        if let (Some(grid), Some(path)) =
            (&execution.result.density_grid, density_grid_path.as_ref())
        {
            write_raspa_density_cube(path, &execution.framework, grid)?;
        }
        if let (Some(histogram), Some(path), Some(spec)) = (
            &execution.result.energy_histogram,
            energy_histogram_path.as_ref(),
            execution.request.energy_histogram.as_ref(),
        ) {
            write_raspa_energy_histogram(path, histogram, spec)?;
        }
        if let (Some(histogram), Some(path), Some(spec)) = (
            &execution.result.number_histogram,
            number_histogram_path.as_ref(),
            execution.request.number_histogram.as_ref(),
        ) {
            write_raspa_number_histogram(path, histogram, spec)?;
        }
        Ok(())
    }
}

struct LocalSurfaceGenerationArtifactSink {
    run_dir: PathBuf,
    system: String,
    source_path: PathBuf,
}

impl super::ports::SurfaceGenerationArtifactSink for LocalSurfaceGenerationArtifactSink {
    fn persist_surface_generation_run(
        &self,
        execution: &super::surface_generation::SurfaceGenerationExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        fs::create_dir_all(&raw_dir)
            .with_context(|| format!("failed to create raw dir `{}`", raw_dir.display()))?;
        fs::create_dir_all(&outputs_dir)
            .with_context(|| format!("failed to create outputs dir `{}`", outputs_dir.display()))?;

        let slab_candidate = execution.result.slab.to_candidate();
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("surface_generation"),
                "workflow_id": "framework.generate-surface",
                "mode": "surface_generation",
                "system": self.system,
                "engine": "Rust Surface Generation (patina-surface)",
                "source_path": self.source_path,
                "provenance": serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "framework.generate-surface",
                    workflow_owner: "framework_generate_surface",
                    backend_id: None,
                    backend_mode: None,
                    lane_mode: None,
                    duplicate_policy_mode: None,
                    parallel_contract: None,
                    run_dir: Some(self.run_dir.as_path()),
                    workdir: None,
                    source_run_dir: None,
                    source_path: Some(self.source_path.as_path()),
                    candidate_json: None,
                    template_bundle: None,
                })?)?,
                "miller": execution.config.miller,
                "artifacts": {
                    "source_candidate": "raw/source_candidate.json",
                    "surface_parent": "raw/surface_parent.json",
                    "surface_generation_result": "raw/surface_generation_result.json",
                    "slab": "outputs/slab.json",
                    "slab_candidate": "outputs/slab_candidate.json",
                    "slab_cif": "outputs/slab.cif",
                    "surface_diagnostics": "outputs/surface_diagnostics.json"
                },
                "summary": {
                    "slab_label": execution.result.slab.label,
                    "atom_count": execution.result.slab.atoms.len(),
                    "topology_safe_cut": execution.result.diagnostics.topology_safe_cut,
                    "broken_bond_estimate": execution.result.diagnostics.broken_bond_estimate,
                    "dedup_report": execution.result.diagnostics.dedup_report,
                    "chosen_cut_offset_angstrom": execution.result.diagnostics.chosen_cut_offset_angstrom,
                    "interplanar_spacing_angstrom": execution.result.diagnostics.interplanar_spacing_angstrom,
                    "layer_count": execution.result.diagnostics.layer_count,
                    "graph_diagnostics": execution.result.diagnostics.graph_diagnostics,
                    "surface_bond_summary": execution.result.diagnostics.surface_bond_summary
                },
                "config": execution.config
            }))
            .context("failed to serialize surface generation manifest")?,
        )?;
        fs::write(
            raw_dir.join("source_candidate.json"),
            serde_json::to_string_pretty(&execution.source_candidate)
                .context("failed to serialize source candidate")?,
        )?;
        fs::write(
            raw_dir.join("surface_parent.json"),
            serde_json::to_string_pretty(&execution.parent)
                .context("failed to serialize surface parent")?,
        )?;
        fs::write(
            raw_dir.join("surface_generation_result.json"),
            serde_json::to_string_pretty(&execution.result)
                .context("failed to serialize surface generation result")?,
        )?;
        fs::write(
            outputs_dir.join("slab.json"),
            serde_json::to_string_pretty(&execution.result.slab)
                .context("failed to serialize slab")?,
        )?;
        fs::write(
            outputs_dir.join("slab_candidate.json"),
            serde_json::to_string_pretty(&slab_candidate)
                .context("failed to serialize slab candidate")?,
        )?;
        fs::write(
            outputs_dir.join("surface_diagnostics.json"),
            serde_json::to_string_pretty(&execution.result.diagnostics)
                .context("failed to serialize surface diagnostics")?,
        )?;
        write_candidate_cif(&slab_candidate, &outputs_dir.join("slab.cif"))?;
        Ok(())
    }
}

fn write_candidate_cif(candidate: &Candidate, path: &Path) -> Result<()> {
    let lattice = candidate.lattice.ok_or_else(|| {
        anyhow!(
            "candidate `{}` is missing lattice for CIF export",
            candidate.label
        )
    })?;
    let periodic_count = candidate.periodic_axes.iter().filter(|axis| **axis).count();
    if periodic_count < 2 {
        return Err(anyhow!(
            "candidate `{}` is not periodic enough for CIF export",
            candidate.label
        ));
    }

    let a = lattice[0];
    let b = lattice[1];
    let c = lattice[2];
    let len_a = vector_norm(a);
    let len_b = vector_norm(b);
    let len_c = vector_norm(c);
    let alpha = angle_degrees(b, c);
    let beta = angle_degrees(a, c);
    let gamma = angle_degrees(a, b);

    let mut out = String::new();
    out.push_str("data_patina_surface\n");
    out.push_str("_audit_creation_method 'Generated by patina-driver surface workflow'\n");
    out.push_str(&format!("_cell_length_a {:.6}\n", len_a));
    out.push_str(&format!("_cell_length_b {:.6}\n", len_b));
    out.push_str(&format!("_cell_length_c {:.6}\n", len_c));
    out.push_str(&format!("_cell_angle_alpha {:.6}\n", alpha));
    out.push_str(&format!("_cell_angle_beta {:.6}\n", beta));
    out.push_str(&format!("_cell_angle_gamma {:.6}\n", gamma));
    out.push_str("loop_\n");
    out.push_str("_atom_site_type_symbol\n");
    out.push_str("_atom_site_fract_x\n");
    out.push_str("_atom_site_fract_y\n");
    out.push_str("_atom_site_fract_z\n");
    for (species, fractional) in candidate
        .species
        .iter()
        .zip(candidate.fractional_coords.iter())
    {
        out.push_str(&format!(
            "{} {:.8} {:.8} {:.8}\n",
            species, fractional[0], fractional[1], fractional[2]
        ));
    }

    fs::write(path, out).with_context(|| format!("failed to write CIF `{}`", path.display()))?;
    Ok(())
}

fn vector_norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn angle_degrees(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dot = left[0] * right[0] + left[1] * right[1] + left[2] * right[2];
    let denom = vector_norm(left) * vector_norm(right);
    if denom <= 1.0e-12 {
        90.0
    } else {
        (dot / denom).clamp(-1.0, 1.0).acos().to_degrees()
    }
}

fn sanitize_artifact_label(label: &str) -> String {
    let sanitized = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "candidate".to_string()
    } else {
        sanitized
    }
}

fn build_surface_reconstruction_markdown_report(
    system: &str,
    execution: &super::surface_reconstruction::SurfaceReconstructionExecution,
    ranked_structures: &[super::surface_reconstruction::SurfaceReconstructionEvaluatedStructure],
) -> String {
    let mut report = String::new();
    report.push_str("# Surface Reconstruction Report\n\n");
    report.push_str(&format!("System: `{system}`  \n"));
    report.push_str(&format!(
        "Target face: `{:?}`  \n",
        execution.summary.target_face
    ));
    report.push_str(&format!(
        "Miller index: `({} {} {})`  \n",
        execution.generation_config.miller.h,
        execution.generation_config.miller.k,
        execution.generation_config.miller.l
    ));
    report.push_str(&format!(
        "Generated slab atoms: `{}`  \n",
        execution.generation_result.slab.atoms.len()
    ));
    report.push_str(&format!(
        "Movable indices: `{}`  \n",
        execution
            .summary
            .movable_indices
            .iter()
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    report.push_str(&format!(
        "Site filter: `{:?}`  \n",
        execution.summary.site_filter
    ));
    report.push_str(&format!(
        "Region policy: `{:?}`  \n",
        execution.summary.region_policy
    ));
    report.push_str(&format!(
        "Movable layer count: `{:?}`  \n",
        execution.summary.movable_layer_count
    ));
    report.push_str(&format!(
        "Layer z tolerance (A): `{:.3}`  \n",
        execution.summary.layer_z_tolerance_angstrom
    ));
    report.push_str(&format!(
        "Move family: `{:?}`  \n",
        execution.summary.move_family
    ));
    report.push_str(&format!(
        "Accepted steps: `{}` / `{}`  \n\n",
        execution.summary.accepted_steps, execution.summary.steps
    ));

    if !execution.summary.warnings.is_empty() {
        report.push_str("## Workflow Warnings\n\n");
        for warning in &execution.summary.warnings {
            report.push_str(&format!("- {warning}\n"));
        }
        report.push('\n');
    }

    report.push_str("## Diagnostics\n\n");
    report.push_str(&format!(
        "- Topology-safe cut: `{:?}`\n",
        execution.generation_result.diagnostics.topology_safe_cut
    ));
    report.push_str(&format!(
        "- Broken-bond estimate: `{:?}`\n",
        execution.generation_result.diagnostics.broken_bond_estimate
    ));
    report.push_str(&format!(
        "- Chosen cut offset (A): `{:?}`\n",
        execution
            .generation_result
            .diagnostics
            .chosen_cut_offset_angstrom
    ));
    report.push_str(&format!(
        "- Interplanar spacing (A): `{:?}`\n",
        execution
            .generation_result
            .diagnostics
            .interplanar_spacing_angstrom
    ));
    report.push_str(&format!(
        "- Layer count: `{:?}`\n\n",
        execution.generation_result.diagnostics.layer_count
    ));

    report.push_str("## Ranked Structures\n\n");
    report.push_str("| Rank | Step | Accepted | Energy | Label |\n");
    report.push_str("| --- | --- | --- | --- | --- |\n");
    for (rank, structure) in ranked_structures.iter().take(10).enumerate() {
        let step = structure
            .step_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "initial".to_string());
        report.push_str(&format!(
            "| {} | {} | {} | {:.8} | `{}` |\n",
            rank + 1,
            step,
            structure.accepted,
            structure.evaluation.energy,
            structure.evaluation.label
        ));
    }

    if let Some(best) = execution.best_evaluation.as_ref() {
        report.push_str("\n## Best Structure\n\n");
        report.push_str(&format!("- Label: `{}`\n", best.label));
        report.push_str(&format!("- Energy: `{:.8}`\n", best.energy));
        report.push_str(&format!("- Converged: `{}`\n", best.converged));
    }

    if let Some(surface_bonds) = execution
        .generation_result
        .diagnostics
        .surface_bond_summary
        .as_ref()
    {
        report.push_str("\n## Surface Bond Sites\n\n");
        report.push_str(&format!(
            "- Top indices: `{}`\n",
            surface_bonds
                .top_indices
                .iter()
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        report.push_str(&format!(
            "- Bottom indices: `{}`\n",
            surface_bonds
                .bottom_indices
                .iter()
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        report.push_str(&format!(
            "- Dangling candidates: `{}`\n",
            surface_bonds.dangling_candidates.len()
        ));
    }

    report
}

struct LocalSurfacePolarityArtifactSink {
    run_dir: PathBuf,
    system: String,
    source_path: PathBuf,
}

struct LocalSurfaceReconstructionArtifactSink {
    run_dir: PathBuf,
    system: String,
    source_path: PathBuf,
    backend: EvalBackendKind,
    backend_metadata: Option<serde_json::Value>,
    top_n_exports: usize,
    parameters: serde_json::Value,
}

impl super::ports::SurfacePolarityArtifactSink for LocalSurfacePolarityArtifactSink {
    fn persist_surface_polarity_run(
        &self,
        execution: &super::surface_polarity::SurfacePolarityExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        fs::create_dir_all(&raw_dir)
            .with_context(|| format!("failed to create raw dir `{}`", raw_dir.display()))?;
        fs::create_dir_all(&outputs_dir)
            .with_context(|| format!("failed to create outputs dir `{}`", outputs_dir.display()))?;

        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("surface_polarity"),
                "mode": "surface_polarity",
                "system": self.system,
                "engine": "Rust Surface Polarity Analysis (patina-surface)",
                "source_path": self.source_path,
                "artifacts": {
                    "slab": "raw/slab.json",
                    "polarity_report": "outputs/polarity_report.json"
                },
                "summary": {
                    "slab_label": execution.slab.label,
                    "classification": execution.report.classification,
                    "residual_dipole_proxy_z": execution.report.residual_dipole_proxy_z
                }
            }))
            .context("failed to serialize surface polarity manifest")?,
        )?;
        fs::write(
            raw_dir.join("slab.json"),
            serde_json::to_string_pretty(&execution.slab).context("failed to serialize slab")?,
        )?;
        fs::write(
            outputs_dir.join("polarity_report.json"),
            serde_json::to_string_pretty(&execution.report)
                .context("failed to serialize polarity report")?,
        )?;
        Ok(())
    }
}

impl super::ports::SurfaceReconstructionArtifactSink for LocalSurfaceReconstructionArtifactSink {
    fn persist_surface_reconstruction_run(
        &self,
        execution: &super::surface_reconstruction::SurfaceReconstructionExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        let candidates_dir = outputs_dir.join("ranked_candidates");
        fs::create_dir_all(&raw_dir)
            .with_context(|| format!("failed to create raw dir `{}`", raw_dir.display()))?;
        fs::create_dir_all(&outputs_dir)
            .with_context(|| format!("failed to create outputs dir `{}`", outputs_dir.display()))?;
        fs::create_dir_all(&candidates_dir).with_context(|| {
            format!(
                "failed to create ranked-candidates dir `{}`",
                candidates_dir.display()
            )
        })?;

        let slab_candidate = execution.generation_result.slab.to_candidate();
        let mut ranked_structures = execution.evaluated_structures.clone();
        ranked_structures.sort_by(|left, right| {
            left.evaluation
                .energy
                .partial_cmp(&right.evaluation.energy)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let ranked_report = if ranked_structures.is_empty() {
            vec![serde_json::json!({
                "rank": 1,
                "step_index": serde_json::Value::Null,
                "moved_atom_index": serde_json::Value::Null,
                "species": serde_json::Value::Null,
                "accepted": false,
                "label": execution.final_candidate.label,
                "energy": serde_json::Value::Null,
                "converged": serde_json::Value::Null,
                "provenance": "fallback_final_candidate_no_successful_evaluations"
            })]
        } else {
            ranked_structures
                .iter()
                .enumerate()
                .map(|(rank, structure)| {
                    serde_json::json!({
                        "rank": rank + 1,
                        "step_index": structure.step_index,
                        "moved_atom_index": structure.moved_atom_index,
                        "species": structure.species,
                        "accepted": structure.accepted,
                        "label": structure.evaluation.label,
                        "energy": structure.evaluation.energy,
                        "converged": structure.evaluation.converged
                    })
                })
                .collect::<Vec<_>>()
        };

        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("surface_reconstruction"),
                "mode": "surface_reconstruction",
                "system": self.system,
                "engine": format!("Rust Surface Reconstruction ({})", self.backend.engine_label()),
                "source_path": self.source_path,
                "backend": self.backend,
                "backend_metadata": self.backend_metadata,
                "artifacts": {
                    "source_candidate": "raw/source_candidate.json",
                    "surface_parent": "raw/surface_parent.json",
                    "surface_generation_result": "raw/surface_generation_result.json",
                    "trace": "raw/trace.json",
                    "evaluated_structures": "raw/evaluated_structures.json",
                    "slab": "outputs/slab.json",
                    "slab_candidate": "outputs/slab_candidate.json",
                    "slab_cif": "outputs/slab.cif",
                    "final_candidate": "outputs/final_candidate.json",
                    "final_candidate_cif": "outputs/final_candidate.cif",
                    "best_candidate": "outputs/best_candidate.json",
                    "best_candidate_cif": "outputs/best_candidate.cif",
                    "ranked_structures": "outputs/ranked_structures.json",
                    "report_json": "outputs/reconstruction_report.json",
                    "report_md": "outputs/reconstruction_report.md",
                    "ranked_candidates": "outputs/ranked_candidates/"
                },
                "summary": {
                    "target_face": execution.summary.target_face,
                    "movable_species": execution.summary.movable_species,
                    "movable_indices": execution.summary.movable_indices,
                    "movable_sites": execution.summary.movable_sites,
                    "site_filter": execution.summary.site_filter,
                    "region_policy": execution.summary.region_policy,
                    "movable_layer_count": execution.summary.movable_layer_count,
                    "layer_z_tolerance_angstrom": execution.summary.layer_z_tolerance_angstrom,
                    "move_family": execution.summary.move_family,
                    "steps": execution.summary.steps,
                    "accepted_steps": execution.summary.accepted_steps,
                    "rejected_acceptance_steps": execution.summary.rejected_acceptance_steps,
                    "rejected_evaluation_steps": execution.summary.rejected_evaluation_steps,
                    "best_energy": execution.summary.best_energy,
                    "warnings": execution.summary.warnings,
                    "evaluated_structure_count": execution.evaluated_structures.len()
                },
                "generation_config": execution.generation_config,
                "parameters": self.parameters,
            }))
            .context("failed to serialize surface reconstruction manifest")?,
        )?;

        fs::write(
            raw_dir.join("source_candidate.json"),
            serde_json::to_string_pretty(&execution.source_candidate)
                .context("failed to serialize source candidate")?,
        )?;
        fs::write(
            raw_dir.join("surface_parent.json"),
            serde_json::to_string_pretty(&execution.parent)
                .context("failed to serialize surface parent")?,
        )?;
        fs::write(
            raw_dir.join("surface_generation_result.json"),
            serde_json::to_string_pretty(&execution.generation_result)
                .context("failed to serialize surface generation result")?,
        )?;
        fs::write(
            raw_dir.join("trace.json"),
            serde_json::to_string_pretty(&execution.trace)
                .context("failed to serialize surface reconstruction trace")?,
        )?;
        fs::write(
            raw_dir.join("evaluated_structures.json"),
            serde_json::to_string_pretty(&execution.evaluated_structures)
                .context("failed to serialize evaluated structures")?,
        )?;

        fs::write(
            outputs_dir.join("slab.json"),
            serde_json::to_string_pretty(&execution.generation_result.slab)
                .context("failed to serialize slab")?,
        )?;
        fs::write(
            outputs_dir.join("slab_candidate.json"),
            serde_json::to_string_pretty(&slab_candidate)
                .context("failed to serialize slab candidate")?,
        )?;
        write_candidate_cif(&slab_candidate, &outputs_dir.join("slab.cif"))?;
        fs::write(
            outputs_dir.join("final_candidate.json"),
            serde_json::to_string_pretty(&execution.final_candidate)
                .context("failed to serialize final candidate")?,
        )?;
        write_candidate_cif(
            &execution.final_candidate,
            &outputs_dir.join("final_candidate.cif"),
        )?;

        if let Some(best) = execution.best_evaluation.as_ref() {
            let best_candidate = Candidate::from(&best.structure);
            fs::write(
                outputs_dir.join("best_candidate.json"),
                serde_json::to_string_pretty(&best_candidate)
                    .context("failed to serialize best candidate")?,
            )?;
            write_candidate_cif(&best_candidate, &outputs_dir.join("best_candidate.cif"))?;
        }

        fs::write(
            outputs_dir.join("ranked_structures.json"),
            serde_json::to_string_pretty(&ranked_report)
                .context("failed to serialize ranked structures report")?,
        )?;
        for (rank, structure) in ranked_structures
            .iter()
            .take(self.top_n_exports.max(1))
            .enumerate()
        {
            let candidate = Candidate::from(&structure.evaluation.structure);
            let stem = format!(
                "rank_{:03}_{}",
                rank + 1,
                sanitize_artifact_label(&structure.evaluation.label)
            );
            fs::write(
                candidates_dir.join(format!("{stem}.json")),
                serde_json::to_string_pretty(&candidate)
                    .context("failed to serialize ranked candidate")?,
            )?;
            write_candidate_cif(&candidate, &candidates_dir.join(format!("{stem}.cif")))?;
        }
        if ranked_structures.is_empty() {
            fs::write(
                candidates_dir.join("rank_001_final_candidate.json"),
                serde_json::to_string_pretty(&execution.final_candidate)
                    .context("failed to serialize fallback ranked candidate")?,
            )?;
            write_candidate_cif(
                &execution.final_candidate,
                &candidates_dir.join("rank_001_final_candidate.cif"),
            )?;
        }

        let report_json = serde_json::json!({
            "system": self.system,
            "source_path": self.source_path,
            "backend": self.backend,
            "target_face": execution.summary.target_face,
            "movable_species": execution.summary.movable_species,
            "movable_indices": execution.summary.movable_indices,
            "movable_sites": execution.summary.movable_sites,
            "site_filter": execution.summary.site_filter,
            "region_policy": execution.summary.region_policy,
            "movable_layer_count": execution.summary.movable_layer_count,
            "layer_z_tolerance_angstrom": execution.summary.layer_z_tolerance_angstrom,
            "move_family": execution.summary.move_family,
            "step_statistics": {
                "steps": execution.summary.steps,
                "accepted_steps": execution.summary.accepted_steps,
                "rejected_acceptance_steps": execution.summary.rejected_acceptance_steps,
                "rejected_evaluation_steps": execution.summary.rejected_evaluation_steps
            },
            "warnings": execution.summary.warnings,
            "surface_diagnostics": execution.generation_result.diagnostics,
            "best_structure": execution.best_evaluation,
            "ranked_structures": ranked_report
        });
        fs::write(
            outputs_dir.join("reconstruction_report.json"),
            serde_json::to_string_pretty(&report_json)
                .context("failed to serialize reconstruction report json")?,
        )?;
        fs::write(
            outputs_dir.join("reconstruction_report.md"),
            build_surface_reconstruction_markdown_report(
                &self.system,
                execution,
                &ranked_structures,
            ),
        )
        .with_context(|| {
            format!(
                "failed to write reconstruction markdown report in `{}`",
                outputs_dir.display()
            )
        })?;

        Ok(())
    }
}

pub(crate) fn analyze_framework_symmetry(
    request: FrameworkSymmetryRunRequest,
) -> Result<super::framework_symmetry::FrameworkSymmetryExecution> {
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    let (candidate, source_path) = load_periodic_structure_input(&request.input)?;
    let system = request.system.unwrap_or_else(|| candidate.label.clone());
    let execution = super::framework_symmetry::run_framework_symmetry_workflow(
        candidate,
        request.tolerance,
        &LocalFrameworkSymmetryPort {
            analyzer: MoyoSymmetryAnalyzer,
        },
        &LocalFrameworkSymmetryArtifactSink {
            run_dir: request.run_dir,
            system,
            source_path,
        },
    )?;
    let normalization_port = LocalFrameworkNormalizationPort {
        analyzer: MoyoSymmetryAnalyzer,
    };
    let standardized_framework =
        normalization_port.normalize_framework(&super::ports::FrameworkNormalizationRequest {
            framework: execution.framework.clone(),
            tolerance: execution.tolerance,
            target: super::ports::FrameworkNormalizationTarget::Standardized,
        })?;
    let primitive_standardized_framework =
        normalization_port.normalize_framework(&super::ports::FrameworkNormalizationRequest {
            framework: execution.framework.clone(),
            tolerance: execution.tolerance,
            target: super::ports::FrameworkNormalizationTarget::PrimitiveStandardized,
        })?;
    if let Some(from_analysis) = execution.analysis.standardized_framework.as_ref() {
        anyhow::ensure!(
            *from_analysis == standardized_framework,
            "standardized framework drift between symmetry and normalization ports"
        );
    }
    if let Some(from_analysis) = execution.analysis.primitive_standardized_framework.as_ref() {
        anyhow::ensure!(
            *from_analysis == primitive_standardized_framework,
            "primitive standardized framework drift between symmetry and normalization ports"
        );
    }
    Ok(execution)
}

pub(crate) fn normalize_framework(
    request: FrameworkNormalizationRunRequest,
) -> Result<super::framework_normalization::FrameworkNormalizationExecution> {
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    let (candidate, source_path) = load_periodic_structure_input(&request.input)?;
    let system = request.system.unwrap_or_else(|| candidate.label.clone());
    super::framework_normalization::run_framework_normalization_workflow(
        candidate,
        request.tolerance,
        request.target,
        &LocalFrameworkNormalizationPort {
            analyzer: MoyoSymmetryAnalyzer,
        },
        &LocalFrameworkNormalizationArtifactSink {
            run_dir: request.run_dir,
            system,
            source_path,
        },
    )
}

pub(crate) fn run_framework_gcmc(
    request: FrameworkGcmcRunRequest,
) -> Result<super::framework_gcmc::FrameworkGcmcExecution> {
    anyhow::ensure!(
        request.janus_mode == JanusModeSetting::SinglePoint,
        "framework GCMC currently requires `--janus-mode single-point` so the Markov state remains well-defined"
    );
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;

    let (candidate, source_path) = load_periodic_structure_input(&request.input)?;
    let request_framework = PeriodicFramework::try_from_candidate(&candidate)?;
    let system = request.system.unwrap_or_else(|| candidate.label.clone());

    let guest_name = request.guest.trim().to_ascii_lowercase();
    let guest_template = match guest_name.as_str() {
        "h2" | "hydrogen" => RigidGuestTemplate::hydrogen(),
        other => {
            return Err(anyhow!(
                "unsupported guest `{other}`; the first milestone currently supports only `h2`"
            ));
        }
    };

    let python_bin = absolutize_path(
        &request
            .python_bin
            .unwrap_or_else(patina_external::default_janus_python_bin),
    )?;
    let adapter_script = absolutize_path(
        &request
            .janus_adapter_script
            .unwrap_or_else(patina_external::default_janus_adapter_script),
    )?;
    let backend = PersistentJanusMaceBackend::new(
        JanusMaceConfig {
            python_bin,
            adapter_script,
            arch: request.janus_arch.clone(),
            model: request.janus_model.clone(),
            device: request.janus_device.clone(),
            default_dtype: request.janus_dtype.clone(),
            mode: request.janus_mode.into(),
            optimizer: request.janus_optimizer.into(),
            fmax: request.janus_fmax,
            steps: request.janus_steps,
        },
        Some(Duration::from_secs(300)),
        request.workdir.join("janus_persistent_session"),
        true,
    )?;

    super::framework_gcmc::run_framework_gcmc_workflow(
        candidate,
        GcmcRequest {
            framework: request_framework,
            guest_name: guest_name.clone(),
            guest_template,
            blocks: 1,
            initialization_cycles: request.initialization_cycles,
            production_cycles: request.production_cycles,
            conditions: GcmcConditions {
                temperature_kelvin: request.temperature_kelvin,
                chemical_potential: None,
                fugacity_pascal: None,
                pressure_pascal: Some(request.pressure_bar * 1.0e5),
            },
            move_schedule: GcmcMoveSchedule::default(),
            random_seed: request.seed,
            max_guest_count: request.max_guest_count,
            minimum_guest_host_distance: request.minimum_guest_host_distance,
            minimum_guest_guest_distance: request.minimum_guest_guest_distance,
            density_grid: Some(patina_raspa::DensityGridSpec {
                dimensions: request.density_dimensions,
                sample_every: 1,
                write_every: request.production_cycles.max(1),
                normalization: request.density_normalization,
                binning: request.density_binning,
                pseudo_atom_channels: vec![guest_name.clone()],
            }),
            energy_histogram: Some(EnergyHistogramSpec {
                number_of_bins: request.energy_histogram_bins,
                range: request.energy_histogram_range,
                sample_every: 1,
                write_every: request.production_cycles.max(1),
            }),
            number_histogram: Some(NumberHistogramSpec {
                lower_limit: request.number_histogram_limits.0,
                upper_limit: request.number_histogram_limits.1,
                sample_every: 1,
                write_every: request.production_cycles.max(1),
            }),
        },
        &LocalFrameworkGcmcPort {
            engine: RigidGuestGcmcEngine::new(LocalJanusFrameworkEnergyEvaluator {
                backend,
                workdir: request.workdir.join("evaluations"),
                next_eval_index: Mutex::new(0),
            }),
        },
        &LocalFrameworkGcmcArtifactSink {
            run_dir: request.run_dir,
            workdir: request.workdir,
            system,
            source_path,
        },
    )
}

pub(crate) fn generate_surface(
    request: SurfaceGenerationRunRequest,
) -> Result<super::surface_generation::SurfaceGenerationExecution> {
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    let (candidate, source_path) = load_periodic_structure_input(&request.input)?;
    let system = request.system.unwrap_or_else(|| candidate.label.clone());
    super::surface_generation::run_surface_generation_workflow(
        candidate,
        request.config,
        &LocalSurfaceGenerationPort {
            engine: DefaultSurfaceGenerationEngine,
        },
        &LocalSurfaceGenerationArtifactSink {
            run_dir: request.run_dir,
            system,
            source_path,
        },
    )
}

pub(crate) fn run_surface_reconstruction(
    request: SurfaceReconstructionRunRequest,
) -> Result<super::surface_reconstruction::SurfaceReconstructionExecution> {
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    fs::create_dir_all(&request.workdir)
        .with_context(|| format!("failed to create workdir `{}`", request.workdir.display()))?;

    let (candidate, source_path) = load_periodic_structure_input(&request.input)?;
    let system = request.system.unwrap_or_else(|| candidate.label.clone());
    let mut generation_config = request.generation_config.clone();
    let mut runtime_warnings = Vec::<String>::new();
    if generation_config.slab_reduction.dedup_slab
        || generation_config.slab_reduction.reduce_slab_inplane
    {
        runtime_warnings.push(
            "surface reconstruction currently disables slab dedup/reduction because the present reduction path is not scientifically safe for periodic reconstruction runs".into(),
        );
        generation_config.slab_reduction.dedup_slab = false;
        generation_config.slab_reduction.reduce_slab_inplane = false;
    }
    let (backend, backend_metadata) = super::backend_runs::build_backend(
        request.backend.backend.into(),
        request.backend.timeout,
        request.backend.executable.clone(),
        request.backend.master_gin_template.clone(),
        request.backend.run_job_template.clone(),
        request.backend.atoms_in_template.clone(),
        request.backend.jobs_template.clone(),
        request.backend.python_bin.clone(),
        request.backend.janus_adapter_script.clone(),
        request.backend.janus_arch.clone(),
        request.backend.janus_model.clone(),
        request.backend.janus_device.clone(),
        request.backend.janus_dtype.clone(),
        request.backend.janus_mode.into(),
        request.backend.janus_optimizer.into(),
        request.backend.janus_fmax,
        request.backend.janus_steps,
    )?;

    super::surface_reconstruction::run_surface_reconstruction_workflow(
        &super::surface_reconstruction::SurfaceReconstructionWorkflowRequest {
            source_candidate: candidate,
            generation_config: generation_config.clone(),
            target_face: request.target_face,
            movable_species: request.movable_species.clone(),
            site_filter: request.site_filter,
            region_policy: request.region_policy,
            movable_layer_count: request.movable_layer_count,
            layer_z_tolerance_angstrom: request.layer_z_tolerance_angstrom,
            move_family: request.move_family,
            steps: request.steps,
            temperature: request.temperature,
            lateral_fractional_step: request.lateral_fractional_step,
            outward_normal_step_angstrom: request.outward_normal_step_angstrom,
            inward_normal_step_angstrom: request.inward_normal_step_angstrom,
            evaluate_initial_state: request.evaluate_initial_state,
            seed: request.seed,
            workdir: request.workdir.clone(),
        },
        &LocalSurfaceGenerationPort {
            engine: DefaultSurfaceGenerationEngine,
        },
        &LocalSurfaceReconstructionEvaluationPort {
            backend: backend.as_ref(),
        },
        &LocalSurfaceReconstructionArtifactSink {
            run_dir: request.run_dir,
            system,
            source_path,
            backend: request.backend.backend,
            backend_metadata,
            top_n_exports: request.top_n_exports,
            parameters: serde_json::json!({
                "runtime_warnings": runtime_warnings,
                "steps": request.steps,
                "temperature": request.temperature,
                "lateral_fractional_step": request.lateral_fractional_step,
                "site_filter": request.site_filter,
                "region_policy": request.region_policy,
                "movable_layer_count": request.movable_layer_count,
                "layer_z_tolerance_angstrom": request.layer_z_tolerance_angstrom,
                "move_family": request.move_family,
                "outward_normal_step_angstrom": request.outward_normal_step_angstrom,
                "inward_normal_step_angstrom": request.inward_normal_step_angstrom,
                "evaluate_initial_state": request.evaluate_initial_state,
                "seed": request.seed,
                "top_n_exports": request.top_n_exports
            }),
        },
    )
}

pub(crate) fn analyze_surface_polarity(
    request: SurfacePolarityRunRequest,
) -> Result<super::surface_polarity::SurfacePolarityExecution> {
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    let (candidate, source_path) = load_periodic_structure_input(&request.input)?;
    let slab = patina_surface::SurfaceSlab::try_from_candidate(&candidate)?;
    let system = request.system.unwrap_or_else(|| slab.label.clone());
    super::surface_polarity::run_surface_polarity_workflow(
        slab,
        &LocalSurfacePolarityPort {
            analyzer: HeuristicSurfacePolarityAnalyzer,
        },
        &LocalSurfacePolarityArtifactSink {
            run_dir: request.run_dir,
            system,
            source_path,
        },
    )
}

fn load_periodic_structure_input(input: &PeriodicStructureInput) -> Result<(Candidate, PathBuf)> {
    match (
        input.candidate_json.as_deref(),
        input.structure_path.as_deref(),
    ) {
        (Some(_), Some(_)) => Err(anyhow!(
            "provide exactly one of `--candidate-json` or `--structure-path`"
        )),
        (None, None) => Err(anyhow!(
            "one of `--candidate-json` or `--structure-path` is required"
        )),
        (Some(path), None) => {
            let path = absolutize_path(path)?;
            Ok((read_candidate_json(&path)?, path))
        }
        (None, Some(path)) => {
            let path = absolutize_path(path)?;
            Ok((candidate_from_xyz(&path)?, path))
        }
    }
}

fn relativize_under(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .ok()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn write_raspa_energy_histogram(
    path: &Path,
    histogram: &EnergyHistogram,
    spec: &EnergyHistogramSpec,
) -> Result<()> {
    const EV_TO_K: f64 = 11604.51812;
    let mut rendered = String::new();
    rendered.push_str("# energy_histogram, number of counts: synthetic_rust_owned\n");
    rendered.push_str("# column 1: energy [K]\n");
    rendered.push_str("# column 2: total energy histogram [-]\n");
    rendered.push_str("# column 3: total energy histogram error [-]\n");
    rendered.push_str("# column 4: VDW energy histogram [-]\n");
    rendered.push_str("# column 5: VDW energy histogram error [-]\n");
    rendered.push_str("# column 6: Coulombic energy histogram [-]\n");
    rendered.push_str("# column 7: Coulombic energy histogram error [-]\n");
    rendered.push_str("# column 8: Polarization energy histogram [-]\n");
    rendered.push_str("# column 9: Polarization energy histogram error [-]\n");
    for bin in 0..spec.number_of_bins {
        let energy_ev = spec.range.0
            + (bin as f64) * (spec.range.1 - spec.range.0) / spec.number_of_bins as f64;
        rendered.push_str(&format!(
            "{:.10} {:.10} 0.0 {:.10} 0.0 {:.10} 0.0 {:.10} 0.0\n",
            energy_ev * EV_TO_K,
            histogram.total[bin],
            histogram.vdw[bin],
            histogram.coulomb[bin],
            histogram.polarization[bin]
        ));
    }
    fs::write(path, rendered)?;
    Ok(())
}

fn write_raspa_number_histogram(
    path: &Path,
    histogram: &NumberHistogram,
    spec: &NumberHistogramSpec,
) -> Result<()> {
    let mut rendered = String::new();
    rendered.push_str("# number_histogram\n");
    rendered.push_str("# column 1: occupancy [-]\n");
    rendered.push_str("# column 2: probability [-]\n");
    for (offset, value) in histogram.per_component[0].iter().enumerate() {
        rendered.push_str(&format!("{} {:.10}\n", spec.lower_limit + offset, value));
    }
    fs::write(path, rendered)?;
    Ok(())
}

fn write_raspa_density_cube(
    path: &Path,
    framework: &PeriodicFramework,
    grid: &DensityGrid,
) -> Result<()> {
    let mut rendered = String::new();
    rendered.push_str("Cube density file\n");
    rendered.push_str("Written by Rust PATINA RASPA lane\n");
    rendered.push_str(&format!("{} 0.0 0.0 0.0\n", framework.atoms.len()));
    rendered.push_str(&format!(
        "-{} {:.10} {:.10} {:.10}\n",
        grid.dimensions[0],
        framework.lattice[0][0] / grid.dimensions[0] as f64,
        framework.lattice[0][1] / grid.dimensions[0] as f64,
        framework.lattice[0][2] / grid.dimensions[0] as f64
    ));
    rendered.push_str(&format!(
        "-{} {:.10} {:.10} {:.10}\n",
        grid.dimensions[1],
        framework.lattice[1][0] / grid.dimensions[1] as f64,
        framework.lattice[1][1] / grid.dimensions[1] as f64,
        framework.lattice[1][2] / grid.dimensions[1] as f64
    ));
    rendered.push_str(&format!(
        "-{} {:.10} {:.10} {:.10}\n",
        grid.dimensions[2],
        framework.lattice[2][0] / grid.dimensions[2] as f64,
        framework.lattice[2][1] / grid.dimensions[2] as f64,
        framework.lattice[2][2] / grid.dimensions[2] as f64
    ));
    for atom in &framework.atoms {
        let cart = [
            framework.lattice[0][0] * atom.fractional[0]
                + framework.lattice[1][0] * atom.fractional[1]
                + framework.lattice[2][0] * atom.fractional[2],
            framework.lattice[0][1] * atom.fractional[0]
                + framework.lattice[1][1] * atom.fractional[1]
                + framework.lattice[2][1] * atom.fractional[2],
            framework.lattice[0][2] * atom.fractional[0]
                + framework.lattice[1][2] * atom.fractional[1]
                + framework.lattice[2][2] * atom.fractional[2],
        ];
        rendered.push_str(&format!(
            "{} 0.0 {:.10} {:.10} {:.10}\n",
            atomic_number_from_symbol(&atom.species),
            cart[0],
            cart[1],
            cart[2]
        ));
    }
    for value in &grid.values {
        rendered.push_str(&format!("{:.10}\n", value));
    }
    fs::write(path, rendered)?;
    Ok(())
}

fn atomic_number_from_symbol(symbol: &str) -> u8 {
    match symbol.trim() {
        "H" => 1,
        "He" => 2,
        "Li" => 3,
        "Be" => 4,
        "B" => 5,
        "C" => 6,
        "N" => 7,
        "O" => 8,
        "F" => 9,
        "Ne" => 10,
        "Na" => 11,
        "Mg" => 12,
        "Al" => 13,
        "Si" => 14,
        "P" => 15,
        "S" => 16,
        "Cl" => 17,
        "Ar" => 18,
        "K" => 19,
        "Ca" => 20,
        "Ti" => 22,
        "Cr" => 24,
        "Mn" => 25,
        "Fe" => 26,
        "Co" => 27,
        "Ni" => 28,
        "Cu" => 29,
        "Zn" => 30,
        "Br" => 35,
        "Zr" => 40,
        "I" => 53,
        _ => 0,
    }
}
