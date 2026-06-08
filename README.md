# PATINA

PATINA (  is a Rust workspace for atomistic topology, inference, and networked assembly. It provides a modular foundation for structure search, simulation orchestration, scientific workflows, and extensible computational materials tooling.

the name PATINA comes from: 

For a concise public overview, start with the [PATINA Project Summary](PROJECT_SUMMARY.md). The
README below is the working entry point for setup, crate groups, and local tooling.

The main purpose is simple: keep the scientific logic, workflow orchestration, external program
interfaces, and documentation in one place, but with clearer boundaries.
The project is in a alpha phase, so this repository should be read as a working research software
workspace rather than a finished library.

At the moment PATINA is focused on:

- global optimisation workflows for clusters, frameworks, and surfaces
- reusable Rust kernels for structure handling, topology, symmetry, and search logic
- clean adapter boundaries for external tools such as GULP, Janus/MACE, CP2K, RASPA-like workflows,
  Dreadnaut/nauty, and Python-backed specialist runtimes
- documentation that explains both the chemistry and the software architecture

## What This Repo Contains

The top level is a Cargo workspace. Most of the work lives under `crates/`, with the reader-facing
documentation under `guide/`.

Important top-level files:

- `Cargo.toml` defines the Rust workspace and shared dependencies.
- `Cargo.lock` pins the current Rust dependency graph.
- `Justfile` contains the common local commands for checks, docs, Python environments, and native
  helper builds.
- `guide/` contains the mdBook documentation.
- `crates/` contains the Rust crates.

The workspace is split by responsibility. That split matters more than the exact crate list, because
some names will probably still change as the system design gets cleaner.

## The Main Crate Groups

Core scientific and data crates:

- `patina-sci-kernel`
  Structure semantics, geometry, conversion, topology helpers, and shared scientific logic.
- `patina-types`
  Shared transport records used between workflow layers.
- `patina-perturber`
  Perturbation, fingerprint, assignment-distance, and parity-facing structure operations.
- `patina-surface`
  Surface generation, analysis, graph, and reduction work.

Topology, symmetry, and construction:

- `patina-dreadnaut`
  Dreadnaut/nauty-facing graph and canonical-labelling work.
- `patina-topology`
  Topology generation, validation, signatures, and export paths.
- `patina-syva`
  Rust-owned symmetry work for 0D cluster-style systems.
- `patina-arvo`
  ARVO-derived geometry lane.
- `patina-stk`
  A Rust-side construction kernel inspired by stk concepts.

Workflow and runtime orchestration:

- `patina-driver`
  The main CLI and top-level workflow composition layer.
- `patina-search`
  Search controllers and SCOTT-shaped workflow logic.
- `patina-runner`
  Runtime dispatch and worker-pool handling.
- `patina-runtime`
  Runtime protocol and local execution bridge.
- `patina-evaluator`
  Evaluation procedure planning and status handling.
- `patina-ulab`
  Site/runtime coordination, scheduler-facing logic, and HPC-style workspace handling.

External and specialist lanes:

- `patina-external`
  Adapters for external programs and their input/output contracts.
- `patina-raspa`
  Periodic framework RASPA like patterns and adsorption facing experiments.
- `patina-emulate`
  Surrogate/emulator runtime work, with a Python package under the crate.
- `patina-llm`
  Optional LLM provider infrastructure.

Interfaces and support:

- `patina-tui`
  Operator-facing terminal interface.
- `patina-app`
  Desktop/app-facing experiment.
- `patina-tools`
  Analysis, plotting, legacy XYZ handling, and GA helper commands.
- `patina-test`
  Workspace-level fixtures and parity tests.

## Getting Started

You need a normal Rust toolchain first:

```bash
rustup default stable
cargo --version
```

The project also uses `just` for common commands:

```bash
just --list
```

For a first local check, start here:

```bash
just doctor-system
cargo check --workspace --all-targets
```

If you want the full local verification path, use:

```bash
just verify-local
```

That command is stricter. It expects more of the local environment to exist, including native helper
builds and Python environments. If you are only reading the code or doing normal Rust edits, start
with `cargo check` first.

## Native And External Tooling

PATINA has adapters for external scientific tools, but the repo should not pretend those tools are
magically installed.

Useful environment variables are listed with:

```bash
just env-help
```

Common examples:

- `GULP_BIN=/abs/path/to/gulp`
- `PATINA_CRYSTAL_BIN=/abs/path/to/Pcrystal`
- `CRYSTAL_INSTALL_GUIDE=/abs/path/to/CRYSTAL23_how_to_install.pdf`
- `JANUS_PYTHON_BIN=/abs/path/to/python`
- `PATINA_DREADNAUT_PATH=/abs/path/to/dreadnaut`
- `PATINA_SITE_PROFILE=macbook-pro|dgx-spark|young|archer2`

For machine-local external program paths, copy `.env.example` to `.env` and edit the executable
paths for that machine. `just` loads `.env` automatically, while HPC job scripts can still export
the same variables explicitly. Check the current external executable setup with:

```bash
just doctor-external-paths
just env-export
```

The bundled nauty source lives under `crates/patina-dreadnaut/nauty25r9`. Build the local binary for
your machine with:

```bash
just build-nauty
just doctor-nauty
```

The binary itself is not meant to be committed. Build it locally.

## Python Runtimes

Some lanes use Python because the surrounding scientific tools are Python-native. Those environments
are managed outside the committed source tree.

The main setup commands are:

```bash
just setup-tools
just setup-janus
just setup-emulate
just setup-stk
```

Or, if you want all of them:

```bash
just setup-python
```

The generated environments live under `venvs/` and are ignored by Git.

## Young Bootstrap

For a first build on Young, keep the `uv` cache and Cargo target tree in your own scratch space,
bootstrap the Python runtimes from a login node, then build the Rust driver in release mode:
Example for a HPC workflow (YOUNG): 
```bash
module unload -f compilers mpi gcc-libs || true
module load beta-modules
module load gcc-libs/10.2.0
module load python/3.9.6

cd /path/to/patina-checkout
cp .env.example .env
export PATINA_SITE_PROFILE=young
export UV_CACHE_DIR="$PWD/.uv-cache"
export CARGO_TARGET_DIR="${HOME}/Scratch/UCL/patina/cargo_target"

just setup-tools
just setup-janus
just build-nauty
just doctor-system
just doctor-python-envs
just doctor-nauty
cargo build --release -p patina-driver
```

If the Janus/MACE weights are stored on Young rather than fetched by model name, launch the TiO2
campaign helper with an explicit local model file:

```bash
uv run --no-project python scripts/tio2_janus_campaign.py write-inputs \
  --janus-model-path /path/to/janus.model
uv run --no-project python scripts/tio2_janus_campaign.py run \
  --janus-model-path /path/to/janus.model
```

## Documentation

The guide is an mdBook under `guide/`. It is the best place to understand the project beyond the
crate list.

Build or serve it with:

```bash
just docs-build
just docs-serve
```

The guide is organised around:

- scientific foundations
- computational chemistry concepts
- software architecture
- runtime boundaries
- legacy KLMC3 context

If you are new to the project, read:

1. `guide/src/introduction.md`
2. `guide/src/patina/README.md`
3. `guide/src/software-architecture/workspace-map.md`
4. `guide/src/software-architecture/runtime-boundaries.md`

## Development Notes

A clean software appraoch is adopted for : 

- keep scientific semantics inward
- keep orchestration around those semantics
- keep external programs at adapter boundaries
- keep generated data out of Git
- keep private campaign notes out of the public repo

That is the whole point of this rewrite. PATINA should be easier to audit, easier to test, and easier
to explain than the older workflow, even while it still carries some parity and migration work.

## Current State

This is an active research software workspace. Some crates are already reasonably reusable. Some are
still integration-heavy. Some exist because old behaviour has to be understood before it can be
replaced cleanly.

The codebase should be judged by whether it makes the scientific workflow more explicit and more
reproducible. That is the direction of travel.
