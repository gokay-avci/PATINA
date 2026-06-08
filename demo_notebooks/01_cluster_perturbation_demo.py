import marimo

__generated_with = "0.23.4"
app = marimo.App(width="wide", app_title="PATINA Cluster Perturbation Demo")


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
        cluster_input_path,
        cluster_perturbation_spec_text,
        load_cluster_candidate,
        manifest_summary_frame,
        plot_candidate,
        pretty_json,
        provenance_frame,
        read_json,
        reset_demo_run,
        run_driver,
        shell_command,
        write_text,
    )

    return (
        artifact_frame,
        build_driver,
        cluster_input_path,
        cluster_perturbation_spec_text,
        load_cluster_candidate,
        manifest_summary_frame,
        mo,
        plot_candidate,
        pretty_json,
        provenance_frame,
        read_json,
        reset_demo_run,
        run_driver,
        shell_command,
        write_text,
    )


@app.cell
def _(mo):
    mo.md(
        r"""
        # Cluster perturbation demo

        This is a good live notebook run because it is:

        - pure Rust
        - quick to execute
        - scientifically interpretable
        - provenance-rich
        """
    )
    return


@app.cell
def _(build_driver, cluster_input_path, cluster_perturbation_spec_text, reset_demo_run, write_text):
    build_driver()
    run_dir = reset_demo_run("cluster_perturbation_demo")
    spec_path = run_dir / "workflow.toml"
    spec_text = cluster_perturbation_spec_text(run_dir, cluster_input_path())
    write_text(spec_path, spec_text)
    return run_dir, spec_path, spec_text


@app.cell
def _(load_cluster_candidate):
    cluster_candidate = load_cluster_candidate()
    return (cluster_candidate,)


@app.cell
def _(cluster_candidate, plot_candidate):
    source_plot = plot_candidate(cluster_candidate, "Source cluster candidate")
    return (source_plot,)


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
    variant_candidates = read_json(run_dir / "outputs" / "variant_candidates.json")
    return manifest, variant_candidates


@app.cell
def _(artifact_frame, manifest, manifest_summary_frame, provenance_frame, run_dir):
    summary_frame = manifest_summary_frame(manifest)
    provenance_rows = provenance_frame(manifest)
    artifacts = artifact_frame(manifest, run_dir)
    return artifacts, provenance_rows, summary_frame


@app.cell
def _(plot_candidate, variant_candidates):
    first_variant_plot = plot_candidate(variant_candidates[0], "First generated variant")
    return (first_variant_plot,)


@app.cell
def _(cluster_candidate, mo):
    mo.md(
        f"## Source candidate\n\nSites: `{len(cluster_candidate['species'])}`; "
        "the workflow perturbs this structure and screens for duplicates."
    )
    return


@app.cell
def _(source_plot):
    source_plot
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
def _(first_variant_plot, mo):
    mo.md("## First generated variant")
    first_variant_plot
    return


if __name__ == "__main__":
    app.run()
