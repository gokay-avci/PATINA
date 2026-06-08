# Notebook Companion

This directory contains the JupyterLite companion for the `patina` documentation.

Use the workspace `just` commands rather than trying to remember the raw `uv` and `jupyter lite`
commands:

- `just docs-notebook-bootstrap`
- `just docs-notebook-build`
- `just docs-notebook-serve`

What these do:

- `docs-notebook-bootstrap` installs the notebook toolchain into `venvs/notebook`
- `docs-notebook-build` builds the static JupyterLite site into `guide/notebook/_site`
- `docs-notebook-serve` serves that built notebook site locally

Important limitation:

- the notebook is for in-browser inspection and experimentation
- it does **not** run native `cargo run -p ...` commands from the local workspace
- authoritative crate execution still belongs to the mdBook build-time `cmdrun` path

Open it with `just docs-notebook-serve`. Do not rely on directly opening
`guide/notebook/_site/index.html` from the filesystem.
