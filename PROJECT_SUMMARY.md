# PATINA Project Summary

PATINA is a Rust-first research software workspace for reproducible atomistic structure search
across clusters, frameworks, and surfaces.

The project rebuilds useful ideas from older KLMC/SCOTT workflows with a clearer public shape:
typed scientific state in Rust, explicit workflow orchestration, external simulation programs
behind adapter boundaries, and generated campaign material kept out of Git.

## At A Glance

- Domain: computational chemistry, materials modelling, and atomistic global optimisation.
- Core stack: Rust workspace with optional Python runtimes for tools that are Python-native.
- Main entry point: `crates/patina-driver`.
- Documentation: `guide/` contains the public mdBook source.
- Public contract: source, fixtures, and docs are committed; local runs and private state are not.

## Why It Exists

Atomistic search projects often become hard to reproduce because search logic, external program
calls, scratch files, local environments, and campaign notes drift into the same layer. PATINA keeps
those concerns separate.

The target is an auditable research platform:

- scientific structures, topology, geometry, and search state are represented explicitly
- workflow status and provenance are handled as first-class data
- external tools are called through adapters instead of leaking through the core logic
- local paths, build products, and generated runs are excluded from the public repository
- documentation explains both the chemistry and the software architecture

## What It Does Today

PATINA currently provides:

- Rust crates for structure semantics, geometry, topology, symmetry, perturbation, and surfaces
- workflow crates for search orchestration, runtime dispatch, evaluation planning, and site profiles
- adapter lanes for tools such as GULP, Janus/MACE, CP2K, CRYSTAL, RASPA-style workflows, and nauty
- parity-facing code for understanding older KLMC/SCOTT behavior where that history still matters
- an mdBook guide for scientific background, architecture, tooling, and runtime boundaries

## Repository Map

| Area | Purpose | Entry Points |
| --- | --- | --- |
| Core science | Structure, geometry, topology, perturbation, and shared records | `crates/patina-sci-kernel`, `crates/patina-types`, `crates/patina-perturber`, `crates/patina-surface` |
| Topology and construction | Graph identity, topology generation, symmetry, ARVO-derived geometry, and stk-inspired construction | `crates/patina-dreadnaut`, `crates/patina-topology`, `crates/patina-syva`, `crates/patina-arvo`, `crates/patina-stk` |
| Workflow and runtime | CLI orchestration, search controllers, workers, runtime bridge, evaluation plans, and HPC-style site handling | `crates/patina-driver`, `crates/patina-search`, `crates/patina-runner`, `crates/patina-runtime`, `crates/patina-evaluator`, `crates/patina-ulab` |
| External and specialist lanes | External scientific programs, adsorption/framework experiments, emulators, and optional LLM infrastructure | `crates/patina-external`, `crates/patina-raspa`, `crates/patina-emulate`, `crates/patina-llm` |
| Interfaces and support | Terminal UI, app experiment, analysis tools, and workspace-level fixtures | `crates/patina-tui`, `crates/patina-app`, `crates/patina-tools`, `crates/patina-test` |
| Documentation | Public guide, architecture notes, and reader-facing project map | `README.md`, `guide/` |

## GitHub-Ready Surface

The committed repository is intended to be source-first:

- build output, virtual environments, caches, generated runs, scratch data, archives, screenshots,
  and private notes are ignored
- `.env.example` files describe local configuration without committing machine-specific secrets
- external executable paths are configured through environment variables
- native helper binaries are rebuilt locally instead of committed
- setup examples use placeholder or checkout-relative paths so the project can move directories

This keeps the public checkout reviewable while still allowing machine-specific research runs to
exist locally.

## Quick Start

For a first source check:

```bash
git clone <repo-url> patina
cd patina
cargo check --workspace --all-targets
```

Common project commands are collected in `Justfile`:

```bash
just --list
just doctor-system
just docs-build
```

External chemistry tools are optional until you run workflows that need them. Configure local paths
through `.env` using `.env.example` as the template.

## Current Quality Bar

PATINA is not presented as a small finished library. It is an active research software workspace,
with reusable kernels, integration-heavy workflow lanes, and parity work that is being retired as
the Rust-owned model becomes clearer.

The public quality bar is:

- a source checkout should not depend on the original local directory name
- the Rust workspace should compile without generated local artifacts
- secrets and private campaign material should stay outside Git
- external program assumptions should be visible at adapter boundaries
- documentation should explain why the code is shaped the way it is

## Best Entry Points

Start with:

1. [`README.md`](README.md) for setup, crate groups, and local tooling.
2. [`guide/src/README.md`](guide/src/README.md) for the public documentation book.
3. [`guide/src/software-architecture/workspace-map.md`](guide/src/software-architecture/workspace-map.md)
   for the architecture map.
4. [`guide/src/software-architecture/runtime-boundaries.md`](guide/src/software-architecture/runtime-boundaries.md)
   for generated data and campaign boundaries.
5. [`crates/patina-driver`](crates/patina-driver) for the main workflow entry point.
