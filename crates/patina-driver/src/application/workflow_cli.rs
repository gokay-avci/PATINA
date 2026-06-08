use anyhow::{anyhow, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use figment::{
    providers::{Env, Format, Json, Serialized, Toml},
    Figment,
};
use patina_types::{
    workflow_by_id, workflow_by_route, workflow_registry, WorkflowDefinition, WorkflowFileKind,
    WorkflowRunSpec,
};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Parser)]
pub(crate) struct WorkflowArgs {
    #[command(subcommand)]
    pub command: WorkflowCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkflowCommand {
    /// List the registered workflow definitions.
    List,
    /// Describe one workflow by shared id or driver route.
    Describe(WorkflowSelectorArgs),
    /// Print the CLI-facing scientific and runtime input contract for one workflow.
    Inputs(WorkflowSelectorArgs),
    /// Print the file and artifact contract for one workflow.
    Files(WorkflowSelectorArgs),
    /// Emit a typed workflow spec template for one supported workflow.
    Scaffold(WorkflowScaffoldArgs),
    /// Validate and summarize one workflow spec file.
    ValidateSpec(WorkflowSpecPathArgs),
    /// Resolve one workflow spec after defaults and environment overlays.
    ResolveSpec(WorkflowSpecPathArgs),
    /// Execute one workflow from a typed spec file.
    Run(WorkflowSpecPathArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct WorkflowSelectorArgs {
    /// Shared workflow id such as `ga.scott-monolithic` or route such as `run-ga scott-monolithic`.
    pub workflow: String,
}

#[derive(Debug, clap::Args)]
pub(crate) struct WorkflowScaffoldArgs {
    /// Shared workflow id to scaffold.
    pub workflow: String,
    /// Optional output path. When omitted, the scaffold is printed to stdout as TOML.
    #[arg(long)]
    pub output: Option<Utf8PathBuf>,
}

#[derive(Debug, clap::Args)]
pub(crate) struct WorkflowSpecPathArgs {
    /// Path to a `.toml` or `.json` workflow spec file.
    pub spec: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct WorkflowSpecSelector {
    workflow: String,
}

pub(crate) fn resolve_workflow(identifier: &str) -> Result<&'static WorkflowDefinition> {
    workflow_by_id(identifier)
        .or_else(|| workflow_by_route(identifier))
        .ok_or_else(|| {
            let known = workflow_registry()
                .iter()
                .map(|workflow| format!("{} ({})", workflow.id, workflow.route))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!(
                "unknown workflow `{identifier}`; use a shared id or route. Known workflows: {known}"
            )
        })
}

pub(crate) fn render_workflow_registry() -> String {
    let mut text = String::from("Registered workflows\n");
    for workflow in workflow_registry() {
        text.push_str(&format!(
            "\n- {name} [{id}]\n  route: {route}\n  family: {family}\n  context: {context}\n  summary: {summary}\n",
            name = workflow.name,
            id = workflow.id,
            route = workflow.route,
            family = workflow.family,
            context = workflow.context.label(),
            summary = workflow.summary,
        ));
    }
    text
}

pub(crate) fn render_workflow_description(workflow: &WorkflowDefinition) -> String {
    let owner = workflow.expected_owner.unwrap_or("n/a");
    let backend = workflow.expected_backend.unwrap_or("n/a");
    let mut text = format!(
        "{name} [{id}]\nroute: {route}\nfamily: {family}\ncontext: {context} ({context_detail})\nsummary: {summary}\nwhen to use: {when_to_use}\ninbound port: {inbound_port}\nadapter: {adapter}\nupstream contract: {upstream}\ndownstream contract: {downstream}\nexpected workflow_owner: {owner}\nexpected backend: {backend}\n",
        name = workflow.name,
        id = workflow.id,
        route = workflow.route,
        family = workflow.family,
        context = workflow.context.label(),
        context_detail = workflow.context.description(),
        summary = workflow.summary,
        when_to_use = workflow.when_to_use,
        inbound_port = workflow.inbound_port,
        adapter = workflow.adapter,
        upstream = workflow.upstream,
        downstream = workflow.downstream,
        owner = owner,
        backend = backend,
    );
    text.push_str("\nrequired inputs\n");
    for item in workflow.required_inputs {
        text.push_str(&format!("- {item}\n"));
    }
    text.push_str("\nkey switches\n");
    for item in workflow.key_switches {
        text.push_str(&format!("- {item}\n"));
    }
    text.push_str("\ntracked outputs\n");
    for item in workflow.outputs {
        text.push_str(&format!("- {item}\n"));
    }
    text
}

pub(crate) fn render_workflow_inputs(workflow: &WorkflowDefinition) -> String {
    let mut text = format!(
        "{name} [{id}]\ncontext: {context} ({context_detail})\nupstream contract: {upstream}\n",
        name = workflow.name,
        id = workflow.id,
        context = workflow.context.label(),
        context_detail = workflow.context.description(),
        upstream = workflow.upstream,
    );
    text.push_str("\nrequired inputs\n");
    for item in workflow.required_inputs {
        text.push_str(&format!("- {item}\n"));
    }
    text.push_str("\nkey switches\n");
    for item in workflow.key_switches {
        text.push_str(&format!("- {item}\n"));
    }
    text
}

pub(crate) fn render_workflow_files(workflow: &WorkflowDefinition) -> String {
    let mut text = format!(
        "{name} [{id}]\nroute: {route}\nfile contract\n",
        name = workflow.name,
        id = workflow.id,
        route = workflow.route,
    );
    for kind in [
        WorkflowFileKind::ScientificInput,
        WorkflowFileKind::RuntimeSupport,
        WorkflowFileKind::AdapterTemplate,
        WorkflowFileKind::GeneratedArtifact,
    ] {
        let matching = workflow
            .file_contracts
            .iter()
            .filter(|entry| entry.kind == kind)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        text.push_str(&format!("\n{}s\n", kind.label()));
        for entry in matching {
            let required = if entry.required {
                "required"
            } else {
                "optional"
            };
            text.push_str(&format!(
                "- {label}: {path} [{required}]\n  {detail}\n",
                label = entry.label,
                path = entry.path_pattern,
                required = required,
                detail = entry.detail,
            ));
        }
    }
    text
}

pub(crate) fn scaffold_workflow_spec(
    workflow_id: &str,
    output: Option<&Utf8Path>,
) -> Result<String> {
    let spec = WorkflowRunSpec::scaffold_for(workflow_id)?;
    let rendered =
        toml::to_string_pretty(&spec).context("failed to render workflow scaffold as TOML")?;
    if let Some(output) = output {
        fs::write(output, &rendered)
            .with_context(|| format!("failed to write workflow scaffold `{}`", output))?;
    }
    Ok(rendered)
}

pub(crate) fn load_workflow_run_spec(path: &Utf8Path) -> Result<WorkflowRunSpec> {
    let selector: WorkflowSpecSelector = file_figment(path)?
        .merge(Env::prefixed("PATINA_WORKFLOW__").split("__"))
        .extract()
        .with_context(|| {
            format!(
                "failed to resolve workflow selector from `{}` and `PATINA_WORKFLOW__*`",
                path
            )
        })?;
    let defaults = WorkflowRunSpec::default_for_resolution(&selector.workflow)?;
    let spec: WorkflowRunSpec = Figment::from(Serialized::defaults(defaults))
        .merge(file_figment(path)?)
        .merge(Env::prefixed("PATINA_WORKFLOW__").split("__"))
        .extract()
        .with_context(|| {
            format!(
                "failed to resolve workflow spec from `{}` and `PATINA_WORKFLOW__*`",
                path
            )
        })?;
    spec.validate()?;
    Ok(spec)
}

pub(crate) fn render_resolved_workflow_spec(spec: &WorkflowRunSpec) -> Result<String> {
    let rendered =
        toml::to_string_pretty(spec).context("failed to render resolved workflow spec as TOML")?;
    Ok(format!(
        "{}\n\n{}",
        render_workflow_description(spec.definition()),
        rendered
    ))
}

fn file_figment(path: &Utf8Path) -> Result<Figment> {
    match path.extension().map(|value| value.to_ascii_lowercase()) {
        Some(extension) if extension == "toml" => Ok(Figment::new().merge(Toml::file(path))),
        Some(extension) if extension == "json" => Ok(Figment::new().merge(Json::file(path))),
        Some(extension) => Err(anyhow!(
            "unsupported workflow spec extension `.{extension}` for `{path}`; expected .toml or .json"
        )),
        None => Err(anyhow!(
            "workflow spec `{path}` has no extension; expected .toml or .json"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{load_workflow_run_spec, scaffold_workflow_spec, WorkflowRunSpec};
    use camino::Utf8PathBuf;
    use std::fs;

    #[test]
    fn staged_ga_scaffold_renders_workflow_tag() {
        let rendered = scaffold_workflow_spec("ga.scott-monolithic", None).expect("scaffold");
        assert!(rendered.contains("workflow = \"ga.scott-monolithic\""));
        assert!(rendered.contains("[run]"));
    }

    #[test]
    fn load_resolves_defaults_for_partial_spec() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("surface.toml")).expect("utf8 path");
        fs::write(
            &path,
            r#"
workflow = "framework.generate-surface"

[run]
run_dir = "runs/active/generated_surface"

[input]
structure_path = "inputs/framework.cif"

[surface]
h = 1
k = 1
l = 0
"#,
        )
        .expect("write spec");

        let spec = load_workflow_run_spec(&path).expect("load spec");
        match spec {
            WorkflowRunSpec::GenerateSurface(spec) => {
                assert_eq!(spec.surface.thickness, 10.0);
                assert_eq!(spec.surface.cut_strategy, "topology-aware");
            }
            other => panic!("unexpected workflow spec: {other:?}"),
        }
    }

    #[test]
    fn load_resolves_defaults_for_energy_lid_spec() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("energy_lid.toml")).expect("utf8 path");
        fs::write(
            &path,
            r#"
workflow = "sampling.energy-lid"

[run]
run_dir = "runs/active/energy_lid"
workdir = "scratch/energy_lid"

[source]
source_run_dir = "inputs/source_ga_run"
"#,
        )
        .expect("write spec");

        let spec = load_workflow_run_spec(&path).expect("load spec");
        match spec {
            WorkflowRunSpec::EnergyLid(spec) => {
                assert_eq!(spec.source.top_n, 8);
                assert_eq!(spec.sampling.lid_levels, 8);
                assert_eq!(spec.janus.mode, "single-point");
            }
            other => panic!("unexpected workflow spec: {other:?}"),
        }
    }
}
