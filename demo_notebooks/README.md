# Demo Notebooks

These notebook demos are designed for live teaching. The recommended approach is:

1. use Python only as an orchestration and visualization layer
2. call the real `patina-driver` workflow surface from the notebook
3. read the generated `manifest.json`, `raw/`, and `outputs/` artifacts back into Python

That keeps the demo honest. Students see the same shared workflow contracts that power the CLI
today and that the TUI/app can converge on later, instead of a separate notebook-only API.

## Why marimo here

`marimo` fits this repo well because:

- the notebooks are plain `.py` files and review cleanly in git
- VS Code can open and run them without introducing binary notebook churn
- the cells can mix shell orchestration, provenance inspection, tables, and 3D structure plots

The helper module [`common.py`](common.py) is plain Python, so the same functions can also be
imported from a Jupyter notebook later if you want that format for a class.

## Environment

The root workspace `uv` project now carries `marimo[recommended]`.

Typical setup:

```bash
uv sync
cargo build -p patina-driver
uv run marimo edit demo_notebooks/00_capabilities_overview.py
```

You can also run a notebook directly:

```bash
uv run marimo run demo_notebooks/01_cluster_perturbation_demo.py
```

## Suggested teaching order

1. [`00_capabilities_overview.py`](00_capabilities_overview.py)
   Show the super-app idea: shared workflow discovery, file contracts, scaffolds, and route clarity.
2. [`01_cluster_perturbation_demo.py`](01_cluster_perturbation_demo.py)
   Run a pure-Rust cluster workflow end to end and inspect provenance plus generated variants.
3. [`02_surface_generation_demo.py`](02_surface_generation_demo.py)
   Run a pure-Rust periodic workflow end to end and inspect the slab artifacts and manifest.

## Demo framing

The clean message for students is:

- `workflow list` shows the computational capabilities
- `workflow scaffold` shows the typed experiment contract
- `workflow run` launches the real workflow
- the manifest/provenance layer makes each run inspectable and reproducible

That communicates the "super app" better than only showing one frontend. The notebook becomes a
scientific narrative layer over the shared workflow kernel.
