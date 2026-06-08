# `patina-syva`

Pure Rust ownership of the useful 0D cluster symmetry path from
`Analysis-Toolkit-master/SYVA`.

## Scope

`patina-syva` currently owns the Rust-facing SYVA parity lane for:

- fixture input parsing and output-summary parsing
- atomic-number normalization and subset-aware preprocessing
- symmetry-element search and tolerance-window scanning
- point-group and framework-group classification
- operation summaries, permutations, subgroup selection, and subgroup optimization
- subgroup-driven symmetrized geometry for the current Rust-supported 0D path

The crate is already wired into the workflow-facing `patina-tools cluster-symmetry`
command. Broader cross-surface replacement inside other PATINA workflows remains
deferred until the active parity lane needs it.

## Bundled Upstream Fixtures

The upstream fixture manifest is exposed from Rust via
`patina_syva::bundled_fixture_manifest()`.

The bundled fixture set currently tracks every fixture under
`Analysis-Toolkit-master/SYVA/test/input` plus the corresponding output-only
variants under `.../output`:

| Fixture | Title | Current purpose | Output variants |
| --- | --- | --- | --- |
| `C60` | `Bucky Ball` | Icosahedral families, optimization, polyhedral subgroup coverage | none |
| `CO` | `CO` | Linear classification on the minimal two-atom case | none |
| `CO2` | `CO2` | Linear full-group handling, equivalence classes, `Dih` families | none |
| `CaTHF6` | `[Ca(THF)6]2+` | Subset-aware preprocessing and high-symmetry framework grouping | `CaTHF6_subset` |
| `H2O2` | `H2O2` | Proper-rotation-axis search on a small non-linear system | none |
| `N4S4` | `N4S4` | Improper rotations and dihedral-family optimization | none |
| `benzene` | `Benzene` | Normalization parity, planarity, and parser baseline coverage | `benzene_tol` |
| `cubane` | `Cubane` | Cubic symmetry, operation summaries, subgroup selection | none |
| `neopentane` | `Neopentane` | Improper-rotation families and tetrahedral subgroup coverage | none |
| `propyne` | `Propyne` | Principal-axis cyclic families and subgroup optimization | none |
| `tcpropmethane` | `Tetracyclopropylmethane` | Subset-aware output parsing and an upstream edge case | `tcpropmethane_subset` |

The manifest is tested against the checked-in upstream `input` and `output`
directories so it stays synchronized with the repository snapshot.

## Current Limitations

- This is a 0D cluster symmetry crate. Periodic framework symmetry belongs to
  the `patina-raspa` lane, not this crate.
- The active parity lane is based on the upstream SYVA fixture corpus bundled in
  this repository. It is evidence-backed for those representative cases, not a
  blanket claim over every possible molecular input.
- The current workflow exposure is centered on `patina-tools cluster-symmetry`.
  Full replacement of every older 0D point-group call site is still deferred.

## Explicitly Deferred SYVA Features

The current Rust port intentionally does not claim full ownership of the
optional legacy reporting/export surface from upstream SYVA:

- `redrep.f`-style reporting layers
- `gaussian.f`-style export/report helpers
- any attempt to mirror the original Fortran text UI as a compatibility surface

Those pieces are provenance aids, not prerequisites for the current Rust-owned
analysis path.
