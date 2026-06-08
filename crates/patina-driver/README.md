# patina-driver

`patina-driver` is the top-level workflow CLI for the PATINA Rust workspace.

It orchestrates Rust-owned workflows and routes into external evaluators, including Python-backed
lanes.

## Python Runtime Policy

`patina-driver` does not own a Python project of its own.

Instead, it consumes workspace-managed Python runtimes:

- `venvs/janus`
  used for Janus/MACE-backed evaluation and optimization lanes
- `venvs/autoemulate`
  used by the surrogate runtime through `patina-emulate`

## Default Assumptions

For Janus-backed workflows, the live default Python path is:

```text
venvs/janus/bin/python
```

For emulate-backed workflows, the driver passes the environment name `autoemulate` into the
`patina-emulate` runtime, which resolves it under the workspace-managed `venvs/` directory.

## Important Boundary

`patina-driver`:

- owns CLI flags and default runtime path policy
- does not own package installation for Janus or AutoEmulate
- should not create crate-local `.venv` directories

If a workflow needs a different Python executable, pass it explicitly through the relevant
`--python-bin` or emulate-environment options instead of mutating the crate layout.

## Workflow Discovery

The driver now exposes a shared workflow-contract surface:

```bash
cargo run -p patina-driver -- workflow list
cargo run -p patina-driver -- workflow describe ga.scott-staged
cargo run -p patina-driver -- workflow inputs framework.generate-surface
cargo run -p patina-driver -- workflow files ga.scott-staged
cargo run -p patina-driver -- workflow scaffold ga.scott-staged
cargo run -p patina-driver -- workflow validate-spec path/to/workflow.toml
cargo run -p patina-driver -- workflow resolve-spec path/to/workflow.toml
cargo run -p patina-driver -- workflow run path/to/workflow.toml
```

These commands are backed by a shared registry in `patina-types` so the driver and TUI can describe
the same workflow families, file contracts, and launch routes from one source of truth.

Current typed-spec coverage and the current shared-spec execution lane are the same today:

- `ga.scott-staged`
- `ga.janus-persistent`
- `sampling.basin-hopping`
- `sampling.energy-lid`
- `sampling.simulated-annealing`
- `structure.perturb-cluster`
- `framework.generate-surface`
- `framework.gcmc`

the shared execution plan now hands typed application requests directly into the runtime layer
instead of bouncing back through `clap` arg structs.
