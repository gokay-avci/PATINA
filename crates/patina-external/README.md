# patina-external

`patina-external` is the adapter boundary for external DFT and force-evaluation programs.

It should own:

- input deck rendering for GULP, CP2K, CRYSTAL, AIMS, VASP, NWChem, DMOL, and later external ML adapters
- local executable, local script, scheduler-submitted, and batch/remote launch mechanics
- output parsing back into typed evaluation results and artifacts
- external-code status such as submitted, converged, non-converged, exported-only, failed, or timed out

It should not own:

- solid-solution, GA, BH, annealing, energy-lid, or scan-surface workflow rules
- Scott staged-evaluator state transitions and retry policy
- domain kernels or stochastic move semantics
- Figment provider composition
- site-profile launch provenance or scheduler-submission budgets

## Python Runtime Boundary

`patina-external` can execute Python-backed adapters, but it does not own a Python package or a
crate-local virtual environment.

Current policy:

- Janus adapter execution should usually use `venvs/janus/bin/python`
- the supported adapter script lives under `crates/patina-external/python/janus_mace_adapter.py`
- environment installation and naming policy are owned at the workspace / driver layer
- the root `Justfile` is the supported bootstrap surface for the Janus runtime
- `GulpExternalAdapter` is now the first concrete adapter that supports local and scheduler-submitted launch semantics through the newer external ports
- `Cp2kExternalAdapter` is now the first CP2K lane:
  it stages inputs, supports export-only and scheduler-submitted execution, exposes a typed
  terminal output contract for project-aware artifact discovery, and still intentionally stops
  short of terminal scientific parsing until parser/result mapping is implemented on top of that
  contract
- `CrystalExternalAdapter` is the first CRYSTAL lane:
  it stages a template input plus structure sidecar, supports export-only and scheduler-submitted
  execution, and exposes `CrystalTerminalOutputContract` for broad run intents and coarse stdout
  assessment. `CrystalResultMappingDecision` keeps terminal parsing separate from `EvalResult`
  materialization: normal completion plus energy is only a candidate, energy-only output is
  salvageable observation, and error markers remain failed evidence. Terminal local execution is
  still blocked until representative CRYSTAL output fixtures are mapped without losing property,
  phonon, band-structure, or density-of-states extensibility.

This crate should stay focused on adapter invocation and parsing, not Python environment
management.
