use crate::domain::artifacts::RunSnapshot;
use camino::Utf8PathBuf;
use patina_types::{WorkflowDefinition, WorkflowRunSpec};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchDraftSection {
    Scientific,
    Runtime,
    Adapter,
}

impl LaunchDraftSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scientific => "scientific",
            Self::Runtime => "runtime",
            Self::Adapter => "adapter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchDraftField {
    pub key: String,
    pub value: String,
    pub section: LaunchDraftSection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchDraft {
    pub workflow_id: &'static str,
    pub spec: WorkflowRunSpec,
    pub fields: Vec<LaunchDraftField>,
    pub validation_error: Option<String>,
}

impl LaunchDraft {
    pub fn for_workflow(workflow: &'static WorkflowDefinition, snapshot: &RunSnapshot) -> Self {
        let mut spec = WorkflowRunSpec::scaffold_for(workflow.id).unwrap_or_else(|_| {
            WorkflowRunSpec::default_for_resolution(workflow.id)
                .expect("registered workflow must have a default spec")
        });
        overlay_snapshot_context(&mut spec, snapshot);
        let mut draft = Self {
            workflow_id: workflow.id,
            fields: fields_from_spec(&spec),
            spec,
            validation_error: None,
        };
        draft.try_sync_spec_from_fields();
        draft
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn selected_field(&self, index: usize) -> Option<&LaunchDraftField> {
        self.fields.get(index)
    }

    pub fn push_char(&mut self, index: usize, ch: char) {
        if let Some(field) = self.fields.get_mut(index) {
            field.value.push(ch);
            self.try_sync_spec_from_fields();
        }
    }

    pub fn pop_char(&mut self, index: usize) {
        if let Some(field) = self.fields.get_mut(index) {
            field.value.pop();
            self.try_sync_spec_from_fields();
        }
    }

    pub fn render_command_preview(&self, workflow: &WorkflowDefinition) -> String {
        let mut lines = vec![format!("cargo run -p patina-driver -- {}", workflow.route)];
        for field in &self.fields {
            if field.value.trim().is_empty() {
                continue;
            }
            lines.push(format!("  --{} {}", field.key, field.value));
        }
        lines.join("\n")
    }

    pub fn render_field_lines(&self, selected: usize, editing: bool) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_section: Option<LaunchDraftSection> = None;
        for (index, field) in self.fields.iter().enumerate() {
            if current_section != Some(field.section) {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                current_section = Some(field.section);
                lines.push(field.section.label().to_string());
            }
            let marker = if index == selected {
                if editing {
                    ">"
                } else {
                    "*"
                }
            } else {
                " "
            };
            lines.push(format!("{} --{} = {}", marker, field.key, field.value));
        }
        lines.push(String::new());
        lines.push(match &self.validation_error {
            Some(error) => format!("spec status: invalid ({error})"),
            None => "spec status: valid shared workflow spec".to_string(),
        });
        lines
    }

    fn try_sync_spec_from_fields(&mut self) {
        match rebuild_spec_from_fields(self.workflow_id, &self.fields) {
            Ok(spec) => {
                self.spec = spec;
                self.validation_error = None;
            }
            Err(error) => {
                self.validation_error = Some(error);
            }
        }
    }
}

fn overlay_snapshot_context(spec: &mut WorkflowRunSpec, snapshot: &RunSnapshot) {
    let system = snapshot
        .manifest
        .system
        .clone()
        .unwrap_or_else(|| "unknown-system".to_string());
    match spec {
        WorkflowRunSpec::ScottStagedGa(spec) => {
            spec.run.system = Some(system);
        }
        WorkflowRunSpec::JanusPersistentGa(spec) => {
            spec.run.system = Some(system);
        }
        WorkflowRunSpec::BasinHopping(spec) => {
            spec.run.system = Some(system);
        }
        WorkflowRunSpec::EnergyLid(spec) => {
            spec.run.system = Some(system);
            if snapshot.has_loaded_run() {
                spec.source.source_run_dir = snapshot.run_dir.to_string_lossy().to_string().into();
            }
        }
        WorkflowRunSpec::SimulatedAnnealing(spec) => {
            spec.run.system = Some(system);
            if snapshot.has_loaded_run() {
                spec.source.source_run_dir = snapshot.run_dir.to_string_lossy().to_string().into();
            }
        }
        WorkflowRunSpec::PerturbCluster(spec) => {
            spec.run.system = Some(system);
        }
        WorkflowRunSpec::GenerateSurface(spec) => {
            spec.run.system = Some(system);
        }
        WorkflowRunSpec::FrameworkGcmc(spec) => {
            spec.run.system = Some(system);
        }
    }
}

fn fields_from_spec(spec: &WorkflowRunSpec) -> Vec<LaunchDraftField> {
    let mut fields = Vec::new();
    match spec {
        WorkflowRunSpec::ScottStagedGa(spec) => {
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "workdir",
                spec.run
                    .workdir
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "base-candidate-json",
                spec.seed
                    .base_candidate_json
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "ga-generations",
                &spec.ga.generations.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "population",
                &spec.ga.population.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "runtime-default-backend",
                &spec.routing.default_backend,
                LaunchDraftSection::Adapter,
            );
        }
        WorkflowRunSpec::JanusPersistentGa(spec) => {
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "workdir",
                spec.run
                    .workdir
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "base-candidate-json",
                spec.seed
                    .base_candidate_json
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "ga-generations",
                &spec.ga.generations.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "population",
                &spec.ga.population.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "workers",
                &spec
                    .backend
                    .workers
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "janus-model",
                &spec.janus.model,
                LaunchDraftSection::Adapter,
            );
            push_field(
                &mut fields,
                "duplicate-policy-mode",
                &spec.duplicate_policy.mode,
                LaunchDraftSection::Adapter,
            );
        }
        WorkflowRunSpec::BasinHopping(spec) => {
            push_field(
                &mut fields,
                "candidate-json",
                spec.seed.candidate_json.as_str(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "workdir",
                spec.run
                    .workdir
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "backend",
                &spec.backend.backend,
                LaunchDraftSection::Adapter,
            );
            push_field(
                &mut fields,
                "bh-steps",
                &spec.sampling.bh_steps.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "walkers",
                &spec.sampling.walkers.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "temperature",
                &spec.sampling.temperature.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "step-size",
                &spec.sampling.step_size.to_string(),
                LaunchDraftSection::Scientific,
            );
        }
        WorkflowRunSpec::EnergyLid(spec) => {
            push_field(
                &mut fields,
                "source-run-dir",
                spec.source.source_run_dir.as_str(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "workdir",
                spec.run
                    .workdir
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "top-n",
                &spec.source.top_n.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "backend",
                &spec.backend.backend,
                LaunchDraftSection::Adapter,
            );
            push_field(
                &mut fields,
                "lid-levels",
                &spec.sampling.lid_levels.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "lid-increment",
                &spec.sampling.lid_increment.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "steps-per-lid",
                &spec.sampling.steps_per_lid.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "runners-per-lid",
                &spec.sampling.runners_per_lid.to_string(),
                LaunchDraftSection::Scientific,
            );
        }
        WorkflowRunSpec::SimulatedAnnealing(spec) => {
            push_field(
                &mut fields,
                "source-run-dir",
                spec.source.source_run_dir.as_str(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "workdir",
                spec.run
                    .workdir
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "top-n",
                &spec.source.top_n.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "backend",
                &spec.backend.backend,
                LaunchDraftSection::Adapter,
            );
            push_field(
                &mut fields,
                "anneal-steps",
                &spec.sampling.anneal_steps.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "initial-temperature",
                &spec.sampling.initial_temperature.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "temperature-scale",
                &spec.sampling.temperature_scale.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "quench-steps",
                &spec.sampling.quench_steps.to_string(),
                LaunchDraftSection::Scientific,
            );
        }
        WorkflowRunSpec::GenerateSurface(spec) => {
            push_field(
                &mut fields,
                "structure-path",
                spec.input
                    .structure_path
                    .as_ref()
                    .map(|value| value.as_str())
                    .or_else(|| {
                        spec.input
                            .candidate_json
                            .as_ref()
                            .map(|value| value.as_str())
                    })
                    .unwrap_or(""),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "h",
                &spec.surface.h.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "k",
                &spec.surface.k.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "l",
                &spec.surface.l.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "thickness",
                &spec.surface.thickness.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "vacuum",
                &spec.surface.vacuum.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "cut-strategy",
                &spec.surface.cut_strategy,
                LaunchDraftSection::Adapter,
            );
        }
        WorkflowRunSpec::PerturbCluster(spec) => {
            push_field(
                &mut fields,
                "candidate-json",
                spec.seed.candidate_json.as_str(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "count",
                &spec.perturbation.count.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "sigma",
                &spec.perturbation.sigma.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "duplicate-screening-mode",
                &spec.perturbation.duplicate_screening_mode,
                LaunchDraftSection::Adapter,
            );
        }
        WorkflowRunSpec::FrameworkGcmc(spec) => {
            push_field(
                &mut fields,
                "structure-path",
                spec.input
                    .structure_path
                    .as_ref()
                    .map(|value| value.as_str())
                    .or_else(|| {
                        spec.input
                            .candidate_json
                            .as_ref()
                            .map(|value| value.as_str())
                    })
                    .unwrap_or(""),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "run-dir",
                spec.run.run_dir.as_str(),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "workdir",
                spec.run
                    .workdir
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "system",
                spec.run.system.as_deref().unwrap_or(""),
                LaunchDraftSection::Runtime,
            );
            push_field(
                &mut fields,
                "guest",
                &spec.gcmc.guest,
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "temperature-kelvin",
                &spec.gcmc.temperature_kelvin.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "pressure-bar",
                &spec.gcmc.pressure_bar.to_string(),
                LaunchDraftSection::Scientific,
            );
            push_field(
                &mut fields,
                "janus-model",
                &spec.janus.model,
                LaunchDraftSection::Adapter,
            );
        }
    }
    fields
}

fn rebuild_spec_from_fields(
    workflow_id: &str,
    fields: &[LaunchDraftField],
) -> Result<WorkflowRunSpec, String> {
    let values = fields
        .iter()
        .map(|field| (field.key.as_str(), field.value.as_str()))
        .collect::<BTreeMap<_, _>>();
    match workflow_id {
        "ga.scott-staged" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::ScottStagedGa(spec) => spec,
                _ => unreachable!(),
            };
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.workdir = Some(required_path(&values, "workdir")?);
            spec.run.system = optional_string(&values, "system");
            spec.seed.base_candidate_json = optional_path(&values, "base-candidate-json");
            spec.ga.generations = required_parse(&values, "ga-generations")?;
            spec.ga.population = required_parse(&values, "population")?;
            spec.routing.default_backend = required_string(&values, "runtime-default-backend")?;
            let spec = WorkflowRunSpec::ScottStagedGa(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "ga.janus-persistent" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::JanusPersistentGa(spec) => spec,
                _ => unreachable!(),
            };
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.workdir = Some(required_path(&values, "workdir")?);
            spec.run.system = optional_string(&values, "system");
            spec.seed.base_candidate_json = optional_path(&values, "base-candidate-json");
            spec.ga.generations = required_parse(&values, "ga-generations")?;
            spec.ga.population = required_parse(&values, "population")?;
            spec.backend.workers = optional_parse(&values, "workers")?;
            spec.janus.model = required_string(&values, "janus-model")?;
            spec.duplicate_policy.mode = required_string(&values, "duplicate-policy-mode")?;
            let spec = WorkflowRunSpec::JanusPersistentGa(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "sampling.basin-hopping" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::BasinHopping(spec) => spec,
                _ => unreachable!(),
            };
            spec.seed.candidate_json = required_path(&values, "candidate-json")?;
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.workdir = Some(required_path(&values, "workdir")?);
            spec.run.system = optional_string(&values, "system");
            spec.backend.backend = required_string(&values, "backend")?;
            spec.sampling.bh_steps = required_parse(&values, "bh-steps")?;
            spec.sampling.walkers = required_parse(&values, "walkers")?;
            spec.sampling.temperature = required_parse(&values, "temperature")?;
            spec.sampling.step_size = required_parse(&values, "step-size")?;
            let spec = WorkflowRunSpec::BasinHopping(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "sampling.energy-lid" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::EnergyLid(spec) => spec,
                _ => unreachable!(),
            };
            spec.source.source_run_dir = required_path(&values, "source-run-dir")?;
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.workdir = Some(required_path(&values, "workdir")?);
            spec.run.system = optional_string(&values, "system");
            spec.source.top_n = required_parse(&values, "top-n")?;
            spec.backend.backend = required_string(&values, "backend")?;
            spec.sampling.lid_levels = required_parse(&values, "lid-levels")?;
            spec.sampling.lid_increment = required_parse(&values, "lid-increment")?;
            spec.sampling.steps_per_lid = required_parse(&values, "steps-per-lid")?;
            spec.sampling.runners_per_lid = required_parse(&values, "runners-per-lid")?;
            let spec = WorkflowRunSpec::EnergyLid(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "sampling.simulated-annealing" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::SimulatedAnnealing(spec) => spec,
                _ => unreachable!(),
            };
            spec.source.source_run_dir = required_path(&values, "source-run-dir")?;
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.workdir = Some(required_path(&values, "workdir")?);
            spec.run.system = optional_string(&values, "system");
            spec.source.top_n = required_parse(&values, "top-n")?;
            spec.backend.backend = required_string(&values, "backend")?;
            spec.sampling.anneal_steps = required_parse(&values, "anneal-steps")?;
            spec.sampling.initial_temperature = required_parse(&values, "initial-temperature")?;
            spec.sampling.temperature_scale = required_parse(&values, "temperature-scale")?;
            spec.sampling.quench_steps = required_parse(&values, "quench-steps")?;
            let spec = WorkflowRunSpec::SimulatedAnnealing(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "framework.generate-surface" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::GenerateSurface(spec) => spec,
                _ => unreachable!(),
            };
            spec.input.structure_path = Some(required_path(&values, "structure-path")?);
            spec.input.candidate_json = None;
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.system = optional_string(&values, "system");
            spec.surface.h = required_parse(&values, "h")?;
            spec.surface.k = required_parse(&values, "k")?;
            spec.surface.l = required_parse(&values, "l")?;
            spec.surface.thickness = required_parse(&values, "thickness")?;
            spec.surface.vacuum = required_parse(&values, "vacuum")?;
            spec.surface.cut_strategy = required_string(&values, "cut-strategy")?;
            let spec = WorkflowRunSpec::GenerateSurface(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "structure.perturb-cluster" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::PerturbCluster(spec) => spec,
                _ => unreachable!(),
            };
            spec.seed.candidate_json = required_path(&values, "candidate-json")?;
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.system = optional_string(&values, "system");
            spec.perturbation.count = required_parse(&values, "count")?;
            spec.perturbation.sigma = required_parse(&values, "sigma")?;
            spec.perturbation.duplicate_screening_mode =
                required_string(&values, "duplicate-screening-mode")?;
            let spec = WorkflowRunSpec::PerturbCluster(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        "framework.gcmc" => {
            let mut spec = match WorkflowRunSpec::scaffold_for(workflow_id)
                .map_err(|error| error.to_string())?
            {
                WorkflowRunSpec::FrameworkGcmc(spec) => spec,
                _ => unreachable!(),
            };
            spec.input.structure_path = Some(required_path(&values, "structure-path")?);
            spec.input.candidate_json = None;
            spec.run.run_dir = required_path(&values, "run-dir")?;
            spec.run.workdir = Some(required_path(&values, "workdir")?);
            spec.run.system = optional_string(&values, "system");
            spec.gcmc.guest = required_string(&values, "guest")?;
            spec.gcmc.temperature_kelvin = required_parse(&values, "temperature-kelvin")?;
            spec.gcmc.pressure_bar = required_parse(&values, "pressure-bar")?;
            spec.janus.model = required_string(&values, "janus-model")?;
            let spec = WorkflowRunSpec::FrameworkGcmc(spec);
            spec.validate().map_err(|error| error.to_string())?;
            Ok(spec)
        }
        other => Err(format!(
            "shared launch draft does not support workflow `{other}` yet"
        )),
    }
}

fn push_field(
    fields: &mut Vec<LaunchDraftField>,
    key: &str,
    value: &str,
    section: LaunchDraftSection,
) {
    fields.push(LaunchDraftField {
        key: key.to_string(),
        value: value.to_string(),
        section,
    });
}

fn required_string(values: &BTreeMap<&str, &str>, key: &str) -> Result<String, String> {
    let value = values.get(key).copied().unwrap_or_default().trim();
    if value.is_empty() {
        return Err(format!("`--{key}` is required"));
    }
    Ok(value.to_string())
}

fn optional_string(values: &BTreeMap<&str, &str>, key: &str) -> Option<String> {
    let value = values.get(key).copied().unwrap_or_default().trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn required_path(values: &BTreeMap<&str, &str>, key: &str) -> Result<Utf8PathBuf, String> {
    required_string(values, key).map(Into::into)
}

fn optional_path(values: &BTreeMap<&str, &str>, key: &str) -> Option<Utf8PathBuf> {
    optional_string(values, key).map(Into::into)
}

fn required_parse<T>(values: &BTreeMap<&str, &str>, key: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = required_string(values, key)?;
    value
        .parse::<T>()
        .map_err(|error| format!("invalid value for `--{key}`: {error}"))
}

fn optional_parse<T>(values: &BTreeMap<&str, &str>, key: &str) -> Result<Option<T>, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match optional_string(values, key) {
        Some(value) => value
            .parse::<T>()
            .map(Some)
            .map_err(|error| format!("invalid value for `--{key}`: {error}")),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::LaunchDraft;
    use crate::domain::artifacts::{RunSnapshot, WorkflowKind};
    use crate::domain::ga::{ArtifactIndex, GenerationStore};
    use crate::domain::run_manifest::RunManifest;
    use patina_types::{workflow_by_id, WorkflowRunSpec};
    use std::path::PathBuf;

    fn snapshot() -> RunSnapshot {
        RunSnapshot {
            run_dir: PathBuf::from("runs/active/example"),
            loaded_from_run_dir: true,
            manifest: RunManifest {
                system: Some("mgo24".to_string()),
                ..RunManifest::default()
            },
            workflow_kind: WorkflowKind::Ga,
            generation_metrics: Vec::new(),
            controller_trace: Vec::new(),
            walker_trace: Vec::new(),
            generations: GenerationStore::default(),
            artifacts: ArtifactIndex {
                manifest_path: PathBuf::from("runs/active/example/manifest.json"),
                raw_files: Vec::new(),
                output_files: Vec::new(),
            },
        }
    }

    #[test]
    fn draft_uses_current_run_for_follow_on_workflow() {
        let workflow = workflow_by_id("sampling.energy-lid").expect("workflow");
        let draft = LaunchDraft::for_workflow(workflow, &snapshot());
        let preview = draft.render_command_preview(workflow);
        assert!(preview.contains("--source-run-dir runs/active/example"));
        assert!(preview.contains("--lid-levels 8"));
        assert!(draft.validation_error.is_none());
        match &draft.spec {
            WorkflowRunSpec::EnergyLid(spec) => {
                assert_eq!(spec.source.top_n, 8);
            }
            other => panic!("unexpected spec: {other:?}"),
        }
    }
}
