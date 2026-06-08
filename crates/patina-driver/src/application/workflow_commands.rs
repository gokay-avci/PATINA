use anyhow::Result;

use super::workflow_cli::{
    load_workflow_run_spec, render_resolved_workflow_spec, render_workflow_description,
    render_workflow_files, render_workflow_inputs, render_workflow_registry, resolve_workflow,
    scaffold_workflow_spec, WorkflowArgs, WorkflowCommand,
};
use super::workflow_execution::WorkflowExecutionPlan;

pub(crate) fn handle_workflow_command(args: WorkflowArgs) -> Result<()> {
    match args.command {
        WorkflowCommand::List => println!("{}", render_workflow_registry()),
        WorkflowCommand::Describe(args) => {
            let workflow = resolve_workflow(&args.workflow)?;
            println!("{}", render_workflow_description(workflow));
        }
        WorkflowCommand::Inputs(args) => {
            let workflow = resolve_workflow(&args.workflow)?;
            println!("{}", render_workflow_inputs(workflow));
        }
        WorkflowCommand::Files(args) => {
            let workflow = resolve_workflow(&args.workflow)?;
            println!("{}", render_workflow_files(workflow));
        }
        WorkflowCommand::Scaffold(args) => {
            let rendered = scaffold_workflow_spec(&args.workflow, args.output.as_deref())?;
            if args.output.is_none() {
                println!("{rendered}");
            } else if let Some(output) = args.output {
                println!("wrote workflow scaffold to {output}");
            }
        }
        WorkflowCommand::ValidateSpec(args) => {
            let spec = load_workflow_run_spec(&args.spec)?;
            println!(
                "workflow spec ok: {} ({})",
                spec.workflow_id(),
                spec.definition().route
            );
        }
        WorkflowCommand::ResolveSpec(args) => {
            let spec = load_workflow_run_spec(&args.spec)?;
            println!("{}", render_resolved_workflow_spec(&spec)?);
        }
        WorkflowCommand::Run(args) => {
            let spec = load_workflow_run_spec(&args.spec)?;
            WorkflowExecutionPlan::from_spec(spec)?.execute()?;
        }
    }
    Ok(())
}
