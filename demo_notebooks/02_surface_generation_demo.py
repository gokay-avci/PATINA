import marimo

__generated_with = "0.23.4"
app = marimo.App(width="wide", app_title="PATINA Surface Generation Demo")


@app.cell
def _():
    import sys
    from pathlib import Path

    root = Path(__file__).resolve().parents[1]
    if str(root) not in sys.path:
        sys.path.insert(0, str(root))

    import marimo as mo

    from demo_notebooks.common import (
        artifact_frame,
        build_driver,
        framework_input_path,
        load_framework_candidate,
        manifest_summary_frame,
        plot_candidate,
        pretty_json,
        provenance_frame,
        read_json,
        reset_demo_run,
        run_driver,
        shell_command,
        surface_generation_spec_text,
        write_text,
    )

    return (
        artifact_frame,
        build_driver,
        framework_input_path,
        load_framework_candidate,
        manifest_summary_frame,
        mo,
        plot_candidate,
        pretty_json,
        provenance_frame,
        read_json,
        reset_demo_run,
        run_driver,
        shell_command,
        surface_generation_spec_text,
        write_text,
    )


@app.cell
def _(mo):
    mo.md(
        r"""
        # Surface generation demo

        This is the periodic pure-Rust notebook example:

        - start from a periodic framework candidate
        - cut a surface slab
        - inspect the resulting slab artifacts and provenance
        """
    )
    return


@app.cell
def _(build_driver, framework_input_path, reset_demo_run, surface_generation_spec_text, write_text):
    build_driver()
    run_dir = reset_demo_run("surface_generation_demo")
    spec_path = run_dir / "workflow.toml"
    spec_text = surface_generation_spec_text(run_dir, framework_input_path())
    write_text(spec_path, spec_text)
    return run_dir, spec_path, spec_text


@app.cell
def _(load_framework_candidate):
    framework_candidate = load_framework_candidate()
    return (framework_candidate,)


@app.cell
def _(framework_candidate, plot_candidate):
    parent_plot = plot_candidate(framework_candidate, "Periodic parent framework")
    return (parent_plot,)


@app.cell
def _(mo, shell_command, spec_path):
    mo.md(
        "## Workflow launch command\n\n```bash\n"
        + shell_command("workflow", "run", spec_path.as_posix())
        + "\n```"
    )
    return


@app.cell
def _(mo, spec_text):
    mo.md("## Workflow spec\n\n```toml\n" + spec_text + "\n```")
    return


@app.cell
def _(run_driver, spec_path):
    validate_output = run_driver("workflow", "validate-spec", spec_path.as_posix()).stdout
    resolved_spec = run_driver("workflow", "resolve-spec", spec_path.as_posix()).stdout
    run_summary = run_driver("workflow", "run", spec_path.as_posix()).json()
    return resolved_spec, run_summary, validate_output


@app.cell
def _(mo, validate_output):
    mo.md("## Validation\n\n```text\n" + validate_output + "\n```")
    return


@app.cell
def _(mo, resolved_spec):
    mo.md("## Resolved spec\n\n```toml\n" + resolved_spec + "\n```")
    return


@app.cell
def _(mo, pretty_json, run_summary):
    mo.md("## Run summary\n\n```json\n" + pretty_json(run_summary) + "\n```")
    return


@app.cell
def _(read_json, run_dir):
    manifest = read_json(run_dir / "manifest.json")
    slab_candidate = read_json(run_dir / "outputs" / "slab_candidate.json")
    return manifest, slab_candidate


@app.cell
def _(artifact_frame, manifest, manifest_summary_frame, provenance_frame, run_dir):
    summary_frame = manifest_summary_frame(manifest)
    provenance_rows = provenance_frame(manifest)
    artifacts = artifact_frame(manifest, run_dir)
    return artifacts, provenance_rows, summary_frame


@app.cell
def _(plot_candidate, slab_candidate):
    slab_plot = plot_candidate(slab_candidate, "Generated slab candidate")
    return (slab_plot,)


@app.cell
def _(framework_candidate, mo):
    mo.md(
        f"## Parent framework\n\nSites: `{len(framework_candidate['species'])}`; "
        "the workflow converts this periodic candidate into a slab plus diagnostics."
    )
    return


@app.cell
def _(parent_plot):
    parent_plot
    return


@app.cell
def _(summary_frame):
    summary_frame
    return


@app.cell
def _(provenance_rows):
    provenance_rows
    return


@app.cell
def _(artifacts):
    artifacts
    return


@app.cell
def _(mo):
    mo.md("## Generated slab")
    return


@app.cell
def _(slab_plot):
    slab_plot
    return


if __name__ == "__main__":
    app.run()
