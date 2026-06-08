# patina-test

Cross-workspace verification crate for PATINA.

Current role:

- host integration-style tests that should exercise production code paths from outside individual
  implementation modules
- accumulate architecture, invariant, oracle, regression, and parity checks in one visible place
- reduce overreliance on large `#[cfg(test)]` blocks embedded inside production crates

Important current limitation:

- `patina-driver` is only partially exposed here. A minimal library seam is currently opened for the
  framework, production, and hybrid workflow modules so that cross-crate tests can start without a
  broad driver refactor.
- many driver workflows are still binary-only from the perspective of `patina-test`; those remain
  migration candidates rather than fully externalized tests.

Near-term intended layout:

- `tests/framework_workflows.rs` for the first externalized workflow-service specification tests
- `tests/production_workflow.rs` for production-stage workflow semantics and topology/precheck
  behavior
- `tests/hybrid_workflow.rs` for hybrid seed selection, acquisition-guided promotion, and enforced
  crossover semantics
- `tests/surface_workflows.rs` for surface-generation and surface-polarity workflow semantics
- `tests/hashkey_parity.rs` for legacy/native parity checks
- future categories should grow toward:
  `architecture/`, `integration/`, `oracle/`, `metamorphic/`, `regression/`, `adversarial/`
  as separate test files or submodule trees

## As-Ga native hashkey parity

Fixtures live under:

- `to_integrate_project/clusters /As-Ga`

Expected native hashkeys belong in:

- `crates/patina-test/fixtures/native_hashkeys/as_ga_expected_hashkeys.txt`

Format:

```text
<fixture.xyz> <native_hashkey>
```

Example:

```text
0acf2e63-4f36-4743-af06-b2e5f73c5746.xyz 1_2_3_4
```

Commands:

```bash
# Fast generic crate checks
cargo test -p patina-test

# Run the legacy fixture-dependent unit checks explicitly
cargo test -p patina-test --lib -- --ignored --nocapture

# Export the native-style dreadnaut graphs for the As-Ga fixtures
cargo run -p patina-test --bin export_as_ga_graphs

# Generate observed Rust/native-compatible hashkeys for the As-Ga fixtures
cargo run -p patina-test --bin hashkey_report

# Run the strict parity assertion once the expected manifest and dreadnaut path are ready
cargo test -p patina-test as_ga_native_hashkeys_match_expected_manifest -- --ignored --nocapture
```
