# patina Notebook Companion

This JupyterLite companion is for interactive inspection inside the browser.

What works well here:

- inspecting committed example data
- Plotly-driven exploratory plotting in a notebook cell
- lightweight scientific explanation and experimentation

What does **not** work directly here:

- running `cargo run -p patina-tools ...` against the native workspace
- calling arbitrary local executables from the browser kernel
- using the full Rust workspace as if the notebook had shell access

So the current split is deliberate:

- `mdbook-cmdrun` remains the authoritative build-time path for real crate execution
- JupyterLite is the browser-side notebook path for inspection and lightweight analysis

If we later want real Rust logic in-browser, the correct next step is a small wasm-exported kernel
or a dedicated remote execution service, not pretending JupyterLite can launch native Cargo
commands.
