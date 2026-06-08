# 6.5 External Software And Runtimes

Some of the most durable boundaries in `patina` are not between Rust crates, but between Rust and
the external software ecosystem it orchestrates.

The code layout may move. These external boundaries are likely to stay.

The dedicated tool-by-tool chapter set lives under [Tooling Landscape](tools/README.md).

## Principle

External programs should be presented as explicit evaluator or toolkit boundaries, not as hidden
global assumptions scattered through the codebase.

That is why `patina-external` exists as an adapter boundary and why Python environments are managed at
the workspace level rather than inside arbitrary crates.

## External Programs Likely To Remain Relevant

The current stable external-software picture includes:

- `GULP`
  the strongest established external evaluator lane in the current workspace
- `CRYSTAL`
  an external DFT lane that should use the same explicit executable-path contract as other local or
  scheduler-backed engines
- `Janus/MACE`
  the active Python-backed ML evaluation lane
- `AIMS`, `VASP`, `NWChem`, `DMol`
  named external-code targets in the adapter architecture, even where execution is not yet equally
  mature
- `RDKit`
  a chemistry-toolkit boundary for the `patina-stk` lane rather than part of the Rust domain crate
- `SYVA`
  preserved through a Rust-owned parity port rather than as a continuing runtime dependency
- `Dreadnaut`
  still an identity/topology adapter when native-compatible hashkey semantics matter

## Runtime Policy

The workspace rule is intentionally simple:

- managed Python environments live under `venvs/`
- crates should not create private `.venv` directories
- environment bootstrap is a workspace concern
- adapter crates own invocation contracts, not environment installation policy
- local executable paths are supplied through `.env` or exported scheduler variables, then checked
  with `just doctor-external-paths`

That leads to the current practical split:

- `venvs/janus` for Janus/MACE-backed paths
- `venvs/autoemulate` for the surrogate runtime
- `venvs/stk` for the RDKit-backed `patina-stk` runtime

## Guide Script Examples

The committed `guide/scripts/` folder now carries a few concrete operator-facing examples for the
`GULP` lane.

- `run_local_staged_gulp_demo.sh`
  a tiny fake-backend sanity check for the staged Scott runtime contract
- `run_local_mgo25_staged_gulp_exploration.sh`
  the better "real lane" local example when a reader wants an actual `GULP`-backed staged GA run
- `run_young_gulp_campaign_shard.qsub.sh`
  the scheduler-facing `GULP` example for the Young campaign lane

That split is intentional: the smallest demo proves the adapter contract, while the MgO and Young
scripts show how a real `GULP` evaluator lane is wired into local and HPC-oriented workflows.

## Why This Boundary Is Stable

Even if the workspace is refactored later, these commitments are worth preserving:

- external programs remain outside the scientific core
- deck generation and output parsing remain adapter responsibilities
- domain semantics do not become identical with one backend's file format
- environment management remains explicit and reproducible

{{#tabs global="lens"}}
{{#tab name="Software Lens"}}

The software benefit is clean dependency flow:

- core crates stay testable without external executables
- backends can be swapped or extended without rewriting the domain
- Python-heavy chemistry tooling does not leak into Rust-first kernel types

{{#endtab}}
{{#tab name="Materials Lens"}}

The scientific benefit is auditability:

- readers can tell when an energy came from GULP, Janus, or another backend
- backend-specific quirks are easier to localize
- provenance records can state which evaluator produced which result

{{#endtab}}
{{#tab name="Bridge"}}

The bridge rule is:

- external software is essential to many workflows
- external software should still appear as an explicit edge, not the hidden definition of the core

That is the only sustainable way to support both long-lived scientific reporting and future
engineering refactors.

{{#endtab}}
{{#endtabs}}
