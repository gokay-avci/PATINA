import marimo

__generated_with = "0.23.4"
app = marimo.App(width="wide", app_title="PATINA Capabilities Overview")


@app.cell
def _():
    import sys
    from pathlib import Path

    root = Path(__file__).resolve().parents[1]
    if str(root) not in sys.path:
        sys.path.insert(0, str(root))

    import marimo as mo
    import pandas as pd

    from demo_notebooks.common import build_driver, shell_command, workflow_text

    return build_driver, mo, pd, shell_command, workflow_text


@app.cell
def _(mo):
    mo.md(
        r"""
        # PATINA super-app overview

        This notebook is the high-level demo for students.

        It shows:

        - the shared workflow registry
        - the typed workflow contract surface
        - the idea that one kernel can be launched from different frontends

        The notebook is not using a separate notebook-only API. It calls the real
        `patina-driver` workflow layer and inspects the same artifacts the app and TUI will use.
        """
    )
    return


@app.cell
def _(build_driver):
    build_result = build_driver()
    return (build_result,)


@app.cell
def _(mo):
    capability_rows = [
        {
            "workflow_id": "structure.perturb-cluster",
            "kind": "Pure Rust",
            "student_demo_value": "Fast local run plus duplicate analysis",
        },
        {
            "workflow_id": "framework.generate-surface",
            "kind": "Pure Rust",
            "student_demo_value": "Periodic input to slab generation with artifacts",
        },
        {
            "workflow_id": "ga.scott-staged",
            "kind": "Hybrid runtime",
            "student_demo_value": "Best shown via scaffold, provenance, and architecture discussion",
        },
        {
            "workflow_id": "sampling.energy-lid",
            "kind": "Backend-evaluated sampling",
            "student_demo_value": "Good for explaining follow-on workflows from prior runs",
        },
        {
            "workflow_id": "framework.gcmc",
            "kind": "Backend-heavy periodic workflow",
            "student_demo_value": "Good contract/provenance demo; live run depends on Janus setup",
        },
    ]
    capabilities = mo.md("## What we can communicate well in a notebook")
    return capabilities, capability_rows


@app.cell
def _(pd, capability_rows):
    capability_frame = pd.DataFrame(capability_rows)
    return (capability_frame,)


@app.cell
def _(workflow_text):
    registry = workflow_text("list")
    surface_description = workflow_text("describe", "framework.generate-surface")
    gcmc_files = workflow_text("files", "framework.gcmc")
    perturb_scaffold = workflow_text("scaffold", "structure.perturb-cluster")
    return gcmc_files, perturb_scaffold, registry, surface_description


@app.cell
def _(shell_command):
    commands = [
        shell_command("workflow", "list"),
        shell_command("workflow", "describe", "framework.generate-surface"),
        shell_command("workflow", "files", "framework.gcmc"),
        shell_command("workflow", "scaffold", "structure.perturb-cluster"),
    ]
    return (commands,)


@app.cell
def _(build_result, mo):
    mo.md(
        f"""
        ## Driver readiness

        Build command used:

        ```bash
        cargo build -p patina-driver
        ```

        Exit code: `{build_result.returncode}`
        """
    )
    return


@app.cell
def _(capabilities):
    capabilities
    return


@app.cell
def _(capability_frame):
    capability_frame
    return


@app.cell
def _(commands, mo):
    mo.md(
        "## Exact CLI surface shown here\n\n```bash\n" + "\n".join(commands) + "\n```"
    )
    return


@app.cell
def _(mo, registry):
    mo.md("## Shared workflow registry\n\n```text\n" + registry + "\n```")
    return


@app.cell
def _(mo, surface_description):
    mo.md("## Example workflow description\n\n```text\n" + surface_description + "\n```")
    return


@app.cell
def _(mo, gcmc_files):
    mo.md("## Example file contract\n\n```text\n" + gcmc_files + "\n```")
    return


@app.cell
def _(mo, perturb_scaffold):
    mo.md("## Example typed workflow scaffold\n\n```toml\n" + perturb_scaffold + "\n```")
    return


@app.cell
def _(mo):
    mo.md(
        r"""
        ## Teaching message

        A good way to present PATINA to materials-science students is:

        - start with *what workflows exist*
        - show *what inputs a workflow expects*
        - run one or two small pure-Rust workflows live
        - inspect the resulting manifest and provenance

        That makes the "super app" story concrete: one computational core, several frontends,
        and reproducible scientific artifacts.
        """
    )
    return


if __name__ == "__main__":
    app.run()
