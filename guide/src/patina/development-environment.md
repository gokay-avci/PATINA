# 6.3 Development Environment

The target setup story for `patina` is:

1. clone the repository
2. install Rust and `just`
3. bootstrap the managed environments with the root `Justfile`
4. supply only the external binaries that cannot be vendored, such as `GULP_BIN` or
   `PATINA_CRYSTAL_BIN`

Current bootstrap commands:

```bash
just setup-macos-native-deps   # macOS only; installs libomp for LightGBM/AutoEmulate
just build-nauty               # rebuild dreadnaut for this machine when provenance is stale
just setup-tools
just setup-janus
just setup-emulate
just setup-stk
cp .env.example .env       # optional; machine-local external executable paths
just doctor-external-paths
just doctor
```

The shorter managed-Python bootstrap is:

```bash
just setup-python
just doctor-python-envs
```

The local verification target is:

```bash
just verify-local
```

`verify-local` runs the machine doctors, `dreadnaut` smoke test, Python runtime import checks,
format check, `cargo check --workspace --all-targets`, and `cargo test --workspace --all-targets`.

On macOS, AutoEmulate currently pulls in LightGBM. The LightGBM wheel expects `libomp.dylib` in a
standard Homebrew or MacPorts location. Run `just setup-macos-native-deps` before the AutoEmulate
doctor if `doctor-system` reports `libomp` as missing.

The required public setup constraints are:

- no private absolute paths
- no assumptions about sibling repositories
- no requirement to manually discover hidden helper scripts
- operator-provided external binaries must be named explicitly and kept in environment variables

`GULP_BIN`, `PATINA_CRYSTAL_BIN`, and similar external-program variables remain acceptable externally
supplied variables. `just doctor-external-paths` also accepts CRYSTAL's own `CRYSTAL_BIN` when it
points at a directory containing `Pcrystal`. A hardcoded path to a personal desktop installation is
not. Local workstations can keep those paths in `.env`; `just` loads it automatically. HPC scripts
should export the same variables from the scheduler job environment or site profile instead of
depending on a developer's local `.env`.

> [!TIP]
> The book itself is part of the development environment story. If a new contributor cannot build
> the docs and understand the workspace from the docs, the setup is still too implicit.
