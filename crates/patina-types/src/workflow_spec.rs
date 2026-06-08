use crate::workflow_catalog::{workflow_by_id, WorkflowDefinition};
use anyhow::{anyhow, bail, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "workflow")]
pub enum WorkflowRunSpec {
    #[serde(rename = "ga.scott-monolithic", alias = "ga.scott-staged")]
    ScottStagedGa(ScottStagedGaSpec),
    #[serde(rename = "ga.persistent-daemon", alias = "ga.janus-persistent")]
    JanusPersistentGa(JanusPersistentGaSpec),
    #[serde(rename = "sampling.basin-hopping")]
    BasinHopping(BasinHoppingSpec),
    #[serde(rename = "sampling.energy-lid")]
    EnergyLid(EnergyLidSpec),
    #[serde(rename = "sampling.simulated-annealing")]
    SimulatedAnnealing(SimulatedAnnealingSpec),
    #[serde(rename = "structure.perturb-cluster")]
    PerturbCluster(PerturbClusterSpec),
    #[serde(rename = "framework.generate-surface")]
    GenerateSurface(GenerateSurfaceSpec),
    #[serde(rename = "framework.gcmc")]
    FrameworkGcmc(FrameworkGcmcSpec),
}

impl WorkflowRunSpec {
    pub fn workflow_id(&self) -> &'static str {
        match self {
            Self::ScottStagedGa(_) => "ga.scott-monolithic",
            Self::JanusPersistentGa(_) => "ga.persistent-daemon",
            Self::BasinHopping(_) => "sampling.basin-hopping",
            Self::EnergyLid(_) => "sampling.energy-lid",
            Self::SimulatedAnnealing(_) => "sampling.simulated-annealing",
            Self::PerturbCluster(_) => "structure.perturb-cluster",
            Self::GenerateSurface(_) => "framework.generate-surface",
            Self::FrameworkGcmc(_) => "framework.gcmc",
        }
    }

    pub fn definition(&self) -> &'static WorkflowDefinition {
        workflow_by_id(self.workflow_id()).expect("workflow definition must exist")
    }

    pub fn default_for_resolution(workflow_id: &str) -> Result<Self> {
        match workflow_id {
            "ga.scott-monolithic" | "ga.scott-staged" => {
                Ok(Self::ScottStagedGa(ScottStagedGaSpec::default()))
            }
            "ga.persistent-daemon" | "ga.janus-persistent" => {
                Ok(Self::JanusPersistentGa(JanusPersistentGaSpec::default()))
            }
            "sampling.basin-hopping" => Ok(Self::BasinHopping(BasinHoppingSpec::default())),
            "sampling.energy-lid" => Ok(Self::EnergyLid(EnergyLidSpec::default())),
            "sampling.simulated-annealing" => {
                Ok(Self::SimulatedAnnealing(SimulatedAnnealingSpec::default()))
            }
            "structure.perturb-cluster" => Ok(Self::PerturbCluster(PerturbClusterSpec::default())),
            "framework.generate-surface" => {
                Ok(Self::GenerateSurface(GenerateSurfaceSpec::default()))
            }
            "framework.gcmc" => Ok(Self::FrameworkGcmc(FrameworkGcmcSpec::default())),
            other => Err(anyhow!(
                "workflow spec scaffolding is not implemented yet for `{other}`"
            )),
        }
    }

    pub fn scaffold_for(workflow_id: &str) -> Result<Self> {
        match workflow_id {
            "ga.scott-monolithic" | "ga.scott-staged" => {
                Ok(Self::ScottStagedGa(ScottStagedGaSpec::scaffold_example()))
            }
            "ga.persistent-daemon" | "ga.janus-persistent" => Ok(Self::JanusPersistentGa(
                JanusPersistentGaSpec::scaffold_example(),
            )),
            "sampling.basin-hopping" => {
                Ok(Self::BasinHopping(BasinHoppingSpec::scaffold_example()))
            }
            "sampling.energy-lid" => Ok(Self::EnergyLid(EnergyLidSpec::scaffold_example())),
            "sampling.simulated-annealing" => Ok(Self::SimulatedAnnealing(
                SimulatedAnnealingSpec::scaffold_example(),
            )),
            "structure.perturb-cluster" => {
                Ok(Self::PerturbCluster(PerturbClusterSpec::scaffold_example()))
            }
            "framework.generate-surface" => Ok(Self::GenerateSurface(
                GenerateSurfaceSpec::scaffold_example(),
            )),
            "framework.gcmc" => Ok(Self::FrameworkGcmc(FrameworkGcmcSpec::scaffold_example())),
            other => Err(anyhow!(
                "workflow spec scaffolding is not implemented yet for `{other}`"
            )),
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::ScottStagedGa(spec) => spec.validate(),
            Self::JanusPersistentGa(spec) => spec.validate(),
            Self::BasinHopping(spec) => spec.validate(),
            Self::EnergyLid(spec) => spec.validate(),
            Self::SimulatedAnnealing(spec) => spec.validate(),
            Self::PerturbCluster(spec) => spec.validate(),
            Self::GenerateSurface(spec) => spec.validate(),
            Self::FrameworkGcmc(spec) => spec.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScottStagedGaSpec {
    pub run: RunLocationSpec,
    pub seed: GaSeedSpec,
    pub ga: ScottGaControllerSpec,
    pub backend: StagedBackendTemplateSpec,
    pub routing: RuntimeRoutingSpec,
    pub janus: JanusRuntimeSpec,
    pub duplicate_policy: DuplicatePolicySpec,
}

impl Default for ScottStagedGaSpec {
    fn default() -> Self {
        Self {
            run: RunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/monolithic_ga_example"),
                workdir: Some(Utf8PathBuf::from("scratch/monolithic_ga_example")),
                system: Some("unknown-system".to_string()),
            },
            seed: GaSeedSpec::default(),
            ga: ScottGaControllerSpec::default(),
            backend: StagedBackendTemplateSpec::default(),
            routing: RuntimeRoutingSpec::default(),
            janus: JanusRuntimeSpec::default(),
            duplicate_policy: DuplicatePolicySpec::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JanusPersistentGaSpec {
    pub run: RunLocationSpec,
    pub seed: GaSeedSpec,
    pub ga: ScottGaControllerSpec,
    pub backend: PersistentJanusBackendSpec,
    pub janus: JanusRuntimeSpec,
    pub duplicate_policy: JanusDuplicatePolicySpec,
    pub operator_policy: JanusGaOperatorPolicySpec,
}

impl Default for JanusPersistentGaSpec {
    fn default() -> Self {
        Self {
            run: RunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/persistent_daemon_ga_example"),
                workdir: Some(Utf8PathBuf::from("scratch/persistent_daemon_ga_example")),
                system: Some("unknown-system".to_string()),
            },
            seed: GaSeedSpec::default(),
            ga: ScottGaControllerSpec::default(),
            backend: PersistentJanusBackendSpec::default(),
            janus: JanusRuntimeSpec::default(),
            duplicate_policy: JanusDuplicatePolicySpec::default(),
            operator_policy: JanusGaOperatorPolicySpec::default(),
        }
    }
}

impl JanusPersistentGaSpec {
    fn scaffold_example() -> Self {
        Self {
            seed: GaSeedSpec {
                stage_dir: None,
                base_candidate_json: Some(Utf8PathBuf::from("inputs/base_candidate.json")),
                base_candidate_path: None,
                resume_from_checkpoint: None,
            },
            backend: PersistentJanusBackendSpec {
                workers: Some(4),
                ..PersistentJanusBackendSpec::default()
            },
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<()> {
        validate_cluster_ga_seed_spec("ga.persistent-daemon", &self.run, &self.seed)?;
        if matches!(self.backend.workers, Some(0)) {
            bail!("ga.persistent-daemon requires `backend.workers` to be at least 1");
        }
        if self.duplicate_policy.mode != "external-native-hashkey" {
            bail!(
                "ga.persistent-daemon currently supports only `duplicate_policy.mode = \"external-native-hashkey\"`"
            );
        }
        Ok(())
    }
}

impl ScottStagedGaSpec {
    fn scaffold_example() -> Self {
        Self {
            seed: GaSeedSpec {
                stage_dir: None,
                base_candidate_json: Some(Utf8PathBuf::from("inputs/base_candidate.json")),
                base_candidate_path: None,
                resume_from_checkpoint: None,
            },
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<()> {
        if self.run.workdir.is_none() {
            bail!("ga.scott-monolithic requires `run.workdir`");
        }
        let seed_count = usize::from(self.seed.base_candidate_json.is_some())
            + usize::from(self.seed.base_candidate_path.is_some())
            + usize::from(self.seed.resume_from_checkpoint.is_some());
        if seed_count > 1 {
            bail!(
                "ga.scott-monolithic accepts only one of `seed.base_candidate_json`, `seed.base_candidate_path`, or `seed.resume_from_checkpoint`"
            );
        }
        if seed_count == 0 && self.seed.stage_dir.is_none() {
            bail!(
                "ga.scott-monolithic requires one explicit seed path or `seed.stage_dir` for stage inference"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasinHoppingSpec {
    pub run: RunLocationSpec,
    pub seed: CandidateSeedSpec,
    pub sampling: BasinHoppingSamplingSpec,
    pub backend: SamplingBackendSpec,
    pub janus: JanusRuntimeSpec,
}

impl Default for BasinHoppingSpec {
    fn default() -> Self {
        Self {
            run: RunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/basin_hopping_example"),
                workdir: Some(Utf8PathBuf::from("scratch/basin_hopping_example")),
                system: Some("unknown-system".to_string()),
            },
            seed: CandidateSeedSpec {
                candidate_json: Utf8PathBuf::from("inputs/cluster_seed.json"),
            },
            sampling: BasinHoppingSamplingSpec::default(),
            backend: SamplingBackendSpec::default(),
            janus: JanusRuntimeSpec::default(),
        }
    }
}

impl BasinHoppingSpec {
    fn scaffold_example() -> Self {
        Self::default()
    }

    fn validate(&self) -> Result<()> {
        if self.run.workdir.is_none() {
            bail!("sampling.basin-hopping requires `run.workdir`");
        }
        match self.sampling.bh_method.as_str() {
            "relax" | "fixed" => {}
            "oscillate" => {
                if self.sampling.n_high_temperature.is_none() {
                    bail!(
                        "sampling.basin-hopping requires `sampling.n_high_temperature` when `sampling.bh_method = \"oscillate\"`"
                    );
                }
                if self.sampling.n_low_temperature.is_none() {
                    bail!(
                        "sampling.basin-hopping requires `sampling.n_low_temperature` when `sampling.bh_method = \"oscillate\"`"
                    );
                }
            }
            other => bail!("sampling.basin-hopping method `{other}` is unsupported"),
        }
        match self.sampling.bh_accept.as_str() {
            "metropolis" | "quench" => {}
            other => bail!("sampling.basin-hopping acceptance `{other}` is unsupported"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnergyLidSpec {
    pub run: RunLocationSpec,
    pub source: SourceRunSelectionSpec,
    pub sampling: EnergyLidSamplingSpec,
    pub backend: SamplingBackendSpec,
    pub janus: JanusRuntimeSpec,
}

impl Default for EnergyLidSpec {
    fn default() -> Self {
        Self {
            run: RunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/energy_lid_example"),
                workdir: Some(Utf8PathBuf::from("scratch/energy_lid_example")),
                system: Some("unknown-system".to_string()),
            },
            source: SourceRunSelectionSpec::default(),
            sampling: EnergyLidSamplingSpec::default(),
            backend: SamplingBackendSpec::default(),
            janus: JanusRuntimeSpec {
                mode: "single-point".to_string(),
                ..JanusRuntimeSpec::default()
            },
        }
    }
}

impl EnergyLidSpec {
    fn scaffold_example() -> Self {
        Self::default()
    }

    fn validate(&self) -> Result<()> {
        validate_source_run_sampling_spec("sampling.energy-lid", &self.run)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatedAnnealingSpec {
    pub run: RunLocationSpec,
    pub source: SourceRunSelectionSpec,
    pub sampling: SimulatedAnnealingSamplingSpec,
    pub backend: SamplingBackendSpec,
    pub janus: JanusRuntimeSpec,
}

impl Default for SimulatedAnnealingSpec {
    fn default() -> Self {
        Self {
            run: RunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/simulated_annealing_example"),
                workdir: Some(Utf8PathBuf::from("scratch/simulated_annealing_example")),
                system: Some("unknown-system".to_string()),
            },
            source: SourceRunSelectionSpec::default(),
            sampling: SimulatedAnnealingSamplingSpec::default(),
            backend: SamplingBackendSpec::default(),
            janus: JanusRuntimeSpec {
                mode: "single-point".to_string(),
                ..JanusRuntimeSpec::default()
            },
        }
    }
}

impl SimulatedAnnealingSpec {
    fn scaffold_example() -> Self {
        Self::default()
    }

    fn validate(&self) -> Result<()> {
        validate_source_run_sampling_spec("sampling.simulated-annealing", &self.run)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerturbClusterSpec {
    pub run: SurfaceRunLocationSpec,
    pub seed: CandidateSeedSpec,
    pub perturbation: ClusterPerturbationControlSpec,
}

impl Default for PerturbClusterSpec {
    fn default() -> Self {
        Self {
            run: SurfaceRunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/perturb_cluster_example"),
                system: Some("cluster-example".to_string()),
            },
            seed: CandidateSeedSpec {
                candidate_json: Utf8PathBuf::from("inputs/cluster_seed.json"),
            },
            perturbation: ClusterPerturbationControlSpec::default(),
        }
    }
}

impl PerturbClusterSpec {
    fn scaffold_example() -> Self {
        Self::default()
    }

    fn validate(&self) -> Result<()> {
        if self.perturbation.count == 0 {
            bail!("structure.perturb-cluster requires `perturbation.count` to be at least 1");
        }
        if self.perturbation.sigma <= 0.0 {
            bail!("structure.perturb-cluster requires `perturbation.sigma` to be positive");
        }
        if self.perturbation.duplicate_threshold < 0.0 {
            bail!("structure.perturb-cluster requires `perturbation.duplicate_threshold` to be non-negative");
        }
        if self
            .perturbation
            .max_displacement
            .is_some_and(|value| value <= 0.0)
        {
            bail!("structure.perturb-cluster requires `perturbation.max_displacement` to be positive when provided");
        }
        if self
            .perturbation
            .validate_min_distance
            .is_some_and(|value| value <= 0.0)
        {
            bail!("structure.perturb-cluster requires `perturbation.validate_min_distance` to be positive when provided");
        }
        match self.perturbation.duplicate_screening_mode.as_str() {
            "global-overlap"
            | "global_overlap"
            | "environment-assignment"
            | "environment_assignment" => {}
            other => {
                bail!("structure.perturb-cluster duplicate screening mode `{other}` is unsupported")
            }
        }
        if self.perturbation.environment_width_cutoff <= 0.0 {
            bail!("structure.perturb-cluster requires `perturbation.environment_width_cutoff` to be positive");
        }
        if self.perturbation.environment_max_atoms_in_sphere == 0 {
            bail!("structure.perturb-cluster requires `perturbation.environment_max_atoms_in_sphere` to be at least 1");
        }
        if self.perturbation.environment_s_orbital_count == 0 {
            bail!("structure.perturb-cluster requires `perturbation.environment_s_orbital_count` to be at least 1");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenerateSurfaceSpec {
    pub run: SurfaceRunLocationSpec,
    pub input: PeriodicStructureInputSpec,
    pub surface: SurfaceCutSpec,
}

impl Default for GenerateSurfaceSpec {
    fn default() -> Self {
        Self {
            run: SurfaceRunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/surface_example"),
                system: Some("framework-example".to_string()),
            },
            input: PeriodicStructureInputSpec::default(),
            surface: SurfaceCutSpec::default(),
        }
    }
}

impl GenerateSurfaceSpec {
    fn scaffold_example() -> Self {
        Self {
            input: PeriodicStructureInputSpec {
                candidate_json: None,
                structure_path: Some(Utf8PathBuf::from("inputs/framework.cif")),
            },
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<()> {
        let path_count = usize::from(self.input.candidate_json.is_some())
            + usize::from(self.input.structure_path.is_some());
        if path_count != 1 {
            bail!(
                "framework.generate-surface requires exactly one of `input.candidate_json` or `input.structure_path`"
            );
        }
        match self.surface.cut_strategy.as_str() {
            "fixed-offset" | "fixed_offset" | "topology-aware" | "topology_aware" => {}
            other => bail!("framework.generate-surface cut strategy `{other}` is unsupported"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameworkGcmcSpec {
    pub run: RunLocationSpec,
    pub input: PeriodicStructureInputSpec,
    pub gcmc: FrameworkGcmcControlSpec,
    pub janus: JanusRuntimeSpec,
}

impl Default for FrameworkGcmcSpec {
    fn default() -> Self {
        Self {
            run: RunLocationSpec {
                run_dir: Utf8PathBuf::from("runs/active/framework_gcmc_example"),
                workdir: Some(Utf8PathBuf::from("scratch/framework_gcmc_example")),
                system: Some("framework-example".to_string()),
            },
            input: PeriodicStructureInputSpec {
                candidate_json: None,
                structure_path: Some(Utf8PathBuf::from("inputs/framework.cif")),
            },
            gcmc: FrameworkGcmcControlSpec::default(),
            janus: JanusRuntimeSpec {
                mode: "single-point".to_string(),
                ..JanusRuntimeSpec::default()
            },
        }
    }
}

impl FrameworkGcmcSpec {
    fn scaffold_example() -> Self {
        Self::default()
    }

    fn validate(&self) -> Result<()> {
        if self.run.workdir.is_none() {
            bail!("framework.gcmc requires `run.workdir`");
        }
        let path_count = usize::from(self.input.candidate_json.is_some())
            + usize::from(self.input.structure_path.is_some());
        if path_count != 1 {
            bail!("framework.gcmc requires exactly one of `input.candidate_json` or `input.structure_path`");
        }
        if !self.gcmc.guest.eq_ignore_ascii_case("h2") {
            bail!("framework.gcmc currently supports only `gcmc.guest = \"h2\"`");
        }
        if self.gcmc.temperature_kelvin <= 0.0 {
            bail!("framework.gcmc requires `gcmc.temperature_kelvin` to be positive");
        }
        if self.gcmc.pressure_bar <= 0.0 {
            bail!("framework.gcmc requires `gcmc.pressure_bar` to be positive");
        }
        if self.gcmc.initialization_cycles == 0 {
            bail!("framework.gcmc requires `gcmc.initialization_cycles` to be at least 1");
        }
        if self.gcmc.production_cycles == 0 {
            bail!("framework.gcmc requires `gcmc.production_cycles` to be at least 1");
        }
        if self.gcmc.max_guest_count == 0 {
            bail!("framework.gcmc requires `gcmc.max_guest_count` to be at least 1");
        }
        if self.gcmc.minimum_guest_host_distance <= 0.0 {
            bail!("framework.gcmc requires `gcmc.minimum_guest_host_distance` to be positive");
        }
        if self.gcmc.minimum_guest_guest_distance <= 0.0 {
            bail!("framework.gcmc requires `gcmc.minimum_guest_guest_distance` to be positive");
        }
        if self.gcmc.density_nx == 0 || self.gcmc.density_ny == 0 || self.gcmc.density_nz == 0 {
            bail!("framework.gcmc requires all density dimensions to be at least 1");
        }
        match self.gcmc.density_binning.as_str() {
            "standard" | "equitable" => {}
            other => bail!("framework.gcmc density binning `{other}` is unsupported"),
        }
        match self.gcmc.density_normalization.as_str() {
            "max" | "number-density" | "number_density" => {}
            other => bail!("framework.gcmc density normalization `{other}` is unsupported"),
        }
        if self.gcmc.energy_histogram_bins == 0 {
            bail!("framework.gcmc requires `gcmc.energy_histogram_bins` to be at least 1");
        }
        if self.gcmc.energy_histogram_min_ev >= self.gcmc.energy_histogram_max_ev {
            bail!("framework.gcmc requires `gcmc.energy_histogram_min_ev < gcmc.energy_histogram_max_ev`");
        }
        if self.gcmc.number_histogram_lower > self.gcmc.number_histogram_upper {
            bail!("framework.gcmc requires `gcmc.number_histogram_lower <= gcmc.number_histogram_upper`");
        }
        if self.janus.mode != "single-point" {
            bail!("framework.gcmc currently requires `janus.mode = \"single-point\"`");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunLocationSpec {
    pub run_dir: Utf8PathBuf,
    pub workdir: Option<Utf8PathBuf>,
    pub system: Option<String>,
}

impl Default for RunLocationSpec {
    fn default() -> Self {
        Self {
            run_dir: Utf8PathBuf::from("runs/active/example"),
            workdir: Some(Utf8PathBuf::from("scratch/example")),
            system: Some("unknown-system".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceRunLocationSpec {
    pub run_dir: Utf8PathBuf,
    pub system: Option<String>,
}

impl Default for SurfaceRunLocationSpec {
    fn default() -> Self {
        Self {
            run_dir: Utf8PathBuf::from("runs/active/surface_example"),
            system: Some("unknown-system".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GaSeedSpec {
    pub stage_dir: Option<Utf8PathBuf>,
    pub base_candidate_json: Option<Utf8PathBuf>,
    pub base_candidate_path: Option<Utf8PathBuf>,
    pub resume_from_checkpoint: Option<Utf8PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CandidateSeedSpec {
    pub candidate_json: Utf8PathBuf,
}

impl Default for CandidateSeedSpec {
    fn default() -> Self {
        Self {
            candidate_json: Utf8PathBuf::from("inputs/candidate.json"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceRunSelectionSpec {
    pub source_run_dir: Utf8PathBuf,
    pub top_n: usize,
}

impl Default for SourceRunSelectionSpec {
    fn default() -> Self {
        Self {
            source_run_dir: Utf8PathBuf::from("inputs/source_ga_run"),
            top_n: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PeriodicStructureInputSpec {
    pub candidate_json: Option<Utf8PathBuf>,
    pub structure_path: Option<Utf8PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScottGaControllerSpec {
    pub generations: usize,
    pub population: usize,
    pub keep_dirs: bool,
    pub seed: Option<u64>,
    pub temperature: f64,
    pub step_size: f64,
}

impl Default for ScottGaControllerSpec {
    fn default() -> Self {
        Self {
            generations: 10,
            population: 10,
            keep_dirs: false,
            seed: Some(11),
            temperature: 10.0,
            step_size: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StagedBackendTemplateSpec {
    pub scott_input_dir: Option<Utf8PathBuf>,
    pub executable: Option<Utf8PathBuf>,
    pub master_gin_template: Option<Utf8PathBuf>,
    pub run_job_template: Option<Utf8PathBuf>,
    pub atoms_in_template: Option<Utf8PathBuf>,
    pub timeout_secs: Option<u64>,
}

impl Default for StagedBackendTemplateSpec {
    fn default() -> Self {
        Self {
            scott_input_dir: Some(Utf8PathBuf::from("inputs/scott")),
            executable: None,
            master_gin_template: None,
            run_job_template: None,
            atoms_in_template: None,
            timeout_secs: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeRoutingSpec {
    pub default_backend: String,
    pub stage_backends: BTreeMap<u8, String>,
}

impl Default for RuntimeRoutingSpec {
    fn default() -> Self {
        Self {
            default_backend: "gulp".to_string(),
            stage_backends: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DuplicatePolicySpec {
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub pmoi_tolerance: f64,
    pub enable_pmoi: bool,
}

impl Default for DuplicatePolicySpec {
    fn default() -> Self {
        Self {
            use_dreadnaut_keys: true,
            hashkey_radius: "IR".to_string(),
            hashkey_radius_const: 0.4,
            pmoi_tolerance: 0.02,
            enable_pmoi: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplingBackendSpec {
    pub backend: String,
    pub timeout_secs: Option<u64>,
    pub executable: Option<Utf8PathBuf>,
    pub master_gin_template: Option<Utf8PathBuf>,
    pub run_job_template: Option<Utf8PathBuf>,
    pub atoms_in_template: Option<Utf8PathBuf>,
    pub jobs_template: Option<Utf8PathBuf>,
}

impl Default for SamplingBackendSpec {
    fn default() -> Self {
        Self {
            backend: "gulp".to_string(),
            timeout_secs: None,
            executable: None,
            master_gin_template: None,
            run_job_template: None,
            atoms_in_template: None,
            jobs_template: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JanusRuntimeSpec {
    pub python_bin: Option<Utf8PathBuf>,
    pub janus_adapter_script: Option<Utf8PathBuf>,
    pub arch: String,
    pub model: String,
    pub device: String,
    pub dtype: String,
    pub mode: String,
    pub optimizer: String,
    pub fmax: f64,
    pub steps: usize,
}

impl Default for JanusRuntimeSpec {
    fn default() -> Self {
        Self {
            python_bin: None,
            janus_adapter_script: None,
            arch: "mace_mp".to_string(),
            model: "small".to_string(),
            device: "cpu".to_string(),
            dtype: "float64".to_string(),
            mode: "local-opt".to_string(),
            optimizer: "abc-fire".to_string(),
            fmax: 0.1,
            steps: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasinHoppingSamplingSpec {
    pub bh_steps: usize,
    pub walkers: usize,
    pub temperature: f64,
    pub step_size: f64,
    pub bh_method: String,
    pub bh_accept: String,
    pub dynamic_threshold: Option<usize>,
    pub moveclass_threshold: Option<usize>,
    pub max_dynamic_step_multiplier: Option<f64>,
    pub n_high_temperature: Option<usize>,
    pub n_low_temperature: Option<usize>,
    pub prob_switch_atoms: Option<f64>,
    pub prob_switch_cations: Option<f64>,
    pub prob_mutate_cluster: Option<f64>,
    pub prob_twist_cluster: Option<f64>,
    pub prob_translate_cluster: Option<f64>,
    pub prob_rotate_cluster: Option<f64>,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
}

impl Default for BasinHoppingSamplingSpec {
    fn default() -> Self {
        Self {
            bh_steps: 40,
            walkers: 1,
            temperature: 10.0,
            step_size: 0.1,
            bh_method: "relax".to_string(),
            bh_accept: "metropolis".to_string(),
            dynamic_threshold: None,
            moveclass_threshold: None,
            max_dynamic_step_multiplier: None,
            n_high_temperature: None,
            n_low_temperature: None,
            prob_switch_atoms: None,
            prob_switch_cations: None,
            prob_mutate_cluster: None,
            prob_twist_cluster: None,
            prob_translate_cluster: None,
            prob_rotate_cluster: None,
            enforce_container: false,
            boundary: None,
            seed: 11,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnergyLidSamplingSpec {
    pub lid_levels: usize,
    pub lid_increment: f64,
    pub steps_per_lid: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub quench_steps: usize,
    pub runners_per_lid: usize,
    pub seed: u64,
}

impl Default for EnergyLidSamplingSpec {
    fn default() -> Self {
        Self {
            lid_levels: 8,
            lid_increment: 1.0,
            steps_per_lid: 20,
            step_size: 0.1,
            enforce_container: false,
            boundary: None,
            quench_steps: 10,
            runners_per_lid: 3,
            seed: 7,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatedAnnealingSamplingSpec {
    pub anneal_steps: usize,
    pub initial_temperature: f64,
    pub temperature_scale: f64,
    pub hold_steps: usize,
    pub quench_steps: usize,
    pub step_size: f64,
    pub enforce_container: bool,
    pub boundary: Option<f64>,
    pub seed: u64,
}

impl Default for SimulatedAnnealingSamplingSpec {
    fn default() -> Self {
        Self {
            anneal_steps: 40,
            initial_temperature: 25.0,
            temperature_scale: 0.8,
            hold_steps: 1,
            quench_steps: 10,
            step_size: 0.1,
            enforce_container: false,
            boundary: None,
            seed: 17,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceCutSpec {
    pub h: i32,
    pub k: i32,
    pub l: i32,
    pub thickness: f64,
    pub vacuum: f64,
    pub supercell_a: usize,
    pub supercell_b: usize,
    pub cut_strategy: String,
    pub cut_offset_fraction: Option<f64>,
    pub dedup_slab: bool,
    pub reduce_slab_inplane: bool,
    pub dedup_frac_tol: f64,
    pub dedup_wrap_z: bool,
    pub dedup_ignore_element: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterPerturbationControlSpec {
    pub count: usize,
    pub sigma: f64,
    pub max_displacement: Option<f64>,
    pub validate_min_distance: Option<f64>,
    pub duplicate_threshold: f64,
    pub duplicate_screening_mode: String,
    pub include_p_orbitals: bool,
    pub environment_width_cutoff: f64,
    pub environment_max_atoms_in_sphere: usize,
    pub environment_s_orbital_count: usize,
    pub environment_p_orbital_count: usize,
    pub seed: Option<u64>,
}

impl Default for ClusterPerturbationControlSpec {
    fn default() -> Self {
        Self {
            count: 8,
            sigma: 0.15,
            max_displacement: Some(0.3),
            validate_min_distance: None,
            duplicate_threshold: 1.0e-6,
            duplicate_screening_mode: "global-overlap".to_string(),
            include_p_orbitals: false,
            environment_width_cutoff: 5.0,
            environment_max_atoms_in_sphere: 100,
            environment_s_orbital_count: 1,
            environment_p_orbital_count: 1,
            seed: Some(11),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameworkGcmcControlSpec {
    pub guest: String,
    pub temperature_kelvin: f64,
    pub pressure_bar: f64,
    pub initialization_cycles: usize,
    pub production_cycles: usize,
    pub max_guest_count: usize,
    pub minimum_guest_host_distance: f64,
    pub minimum_guest_guest_distance: f64,
    pub density_nx: usize,
    pub density_ny: usize,
    pub density_nz: usize,
    pub density_binning: String,
    pub density_normalization: String,
    pub energy_histogram_bins: usize,
    pub energy_histogram_min_ev: f64,
    pub energy_histogram_max_ev: f64,
    pub number_histogram_lower: usize,
    pub number_histogram_upper: usize,
    pub seed: u64,
}

impl Default for FrameworkGcmcControlSpec {
    fn default() -> Self {
        Self {
            guest: "h2".to_string(),
            temperature_kelvin: 298.0,
            pressure_bar: 1.0,
            initialization_cycles: 200,
            production_cycles: 1000,
            max_guest_count: 32,
            minimum_guest_host_distance: 1.2,
            minimum_guest_guest_distance: 1.0,
            density_nx: 48,
            density_ny: 48,
            density_nz: 48,
            density_binning: "equitable".to_string(),
            density_normalization: "number-density".to_string(),
            energy_histogram_bins: 128,
            energy_histogram_min_ev: -5.0,
            energy_histogram_max_ev: 1.0,
            number_histogram_lower: 0,
            number_histogram_upper: 32,
            seed: 11,
        }
    }
}

impl Default for SurfaceCutSpec {
    fn default() -> Self {
        Self {
            h: 1,
            k: 0,
            l: 0,
            thickness: 10.0,
            vacuum: 15.0,
            supercell_a: 1,
            supercell_b: 1,
            cut_strategy: "topology-aware".to_string(),
            cut_offset_fraction: None,
            dedup_slab: false,
            reduce_slab_inplane: false,
            dedup_frac_tol: 2.0e-4,
            dedup_wrap_z: false,
            dedup_ignore_element: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistentJanusBackendSpec {
    pub workers: Option<usize>,
    pub keep_dirs: bool,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JanusDuplicatePolicySpec {
    pub mode: String,
    pub atoms_in_template: Option<Utf8PathBuf>,
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub pmoi_tolerance: f64,
    pub enable_pmoi: bool,
}

impl Default for JanusDuplicatePolicySpec {
    fn default() -> Self {
        Self {
            mode: "external-native-hashkey".to_string(),
            atoms_in_template: Some(Utf8PathBuf::from("KLMC3-main/KLMC/data/atoms.in")),
            use_dreadnaut_keys: true,
            hashkey_radius: "IR".to_string(),
            hashkey_radius_const: 0.4,
            pmoi_tolerance: 0.02,
            enable_pmoi: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct JanusGaOperatorPolicySpec {
    pub backend: Option<String>,
    pub pop_replacement_ratio: Option<f64>,
    pub reinsert_elites_ratio: Option<f64>,
    pub mutation_ratio: Option<f64>,
    pub mut_selfcross_ratio: Option<f64>,
    pub max_repop_attempts: Option<usize>,
    pub crossover_attempts: Option<usize>,
    pub tournament_size_min: Option<usize>,
    pub tournament_size_max: Option<usize>,
}

fn validate_source_run_sampling_spec(workflow_id: &str, run: &RunLocationSpec) -> Result<()> {
    if run.workdir.is_none() {
        bail!("{workflow_id} requires `run.workdir`");
    }
    Ok(())
}

fn validate_cluster_ga_seed_spec(
    workflow_id: &str,
    run: &RunLocationSpec,
    seed: &GaSeedSpec,
) -> Result<()> {
    if run.workdir.is_none() {
        bail!("{workflow_id} requires `run.workdir`");
    }
    let seed_count = usize::from(seed.base_candidate_json.is_some())
        + usize::from(seed.base_candidate_path.is_some())
        + usize::from(seed.resume_from_checkpoint.is_some());
    if seed_count > 1 {
        bail!(
            "{workflow_id} accepts only one of `seed.base_candidate_json`, `seed.base_candidate_path`, or `seed.resume_from_checkpoint`"
        );
    }
    if seed_count == 0 && seed.stage_dir.is_none() {
        bail!(
            "{workflow_id} requires one explicit seed path or `seed.stage_dir` for stage inference"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BasinHoppingSamplingSpec, GenerateSurfaceSpec, WorkflowRunSpec};
    use crate::workflow_catalog::workflow_by_id;

    const SUPPORTED_SHARED_SPECS: &[&str] = &[
        "ga.scott-monolithic",
        "ga.persistent-daemon",
        "sampling.basin-hopping",
        "sampling.energy-lid",
        "sampling.simulated-annealing",
        "structure.perturb-cluster",
        "framework.generate-surface",
        "framework.gcmc",
    ];

    #[test]
    fn supported_workflows_have_shared_resolution_specs() {
        for workflow_id in SUPPORTED_SHARED_SPECS {
            let workflow = workflow_by_id(workflow_id).expect("workflow in registry");
            let spec =
                WorkflowRunSpec::default_for_resolution(workflow.id).unwrap_or_else(|error| {
                    panic!("missing default spec for {}: {error}", workflow.id)
                });
            assert_eq!(spec.workflow_id(), workflow.id);
        }
    }

    #[test]
    fn supported_workflows_have_shared_scaffold_specs() {
        for workflow_id in SUPPORTED_SHARED_SPECS {
            let workflow = workflow_by_id(workflow_id).expect("workflow in registry");
            let spec = WorkflowRunSpec::scaffold_for(workflow.id).unwrap_or_else(|error| {
                panic!("missing scaffold spec for {}: {error}", workflow.id)
            });
            assert_eq!(spec.workflow_id(), workflow.id);
            spec.validate().unwrap_or_else(|error| {
                panic!("invalid scaffold spec for {}: {error}", workflow.id)
            });
        }
    }

    #[test]
    fn basin_hopping_accepts_oscillate_when_temperature_windows_are_present() {
        let mut spec = BasinHoppingSamplingSpec {
            bh_method: "oscillate".to_string(),
            n_high_temperature: Some(3),
            n_low_temperature: Some(2),
            ..BasinHoppingSamplingSpec::default()
        };
        assert_eq!(spec.bh_method, "oscillate");
        let run = WorkflowRunSpec::BasinHopping(super::BasinHoppingSpec {
            sampling: spec.clone(),
            ..super::BasinHoppingSpec::default()
        });
        run.validate().expect("oscillate spec should validate");

        spec.n_low_temperature = None;
        let invalid = WorkflowRunSpec::BasinHopping(super::BasinHoppingSpec {
            sampling: spec,
            ..super::BasinHoppingSpec::default()
        });
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn generate_surface_rejects_invalid_cut_strategy_during_validation() {
        let mut spec = GenerateSurfaceSpec::default();
        spec.input.candidate_json = Some("inputs/framework.json".into());
        spec.surface.cut_strategy = "shifted".to_string();
        assert!(spec.validate().is_err());
    }
}
