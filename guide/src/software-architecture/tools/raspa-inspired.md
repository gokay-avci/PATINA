# 6.6.5 RASPA3-Inspired Periodic Work

<p class="lead">
  `patina-raspa` is the periodic-framework and adsorption side-science lane in `patina`: a real
  Rust-owned domain crate inspired by `RASPA3-main`, but intentionally narrower than a full clone
  of the broader upstream runtime.
</p>

## At A Glance

- Scientific role:
  Frameworks, adsorption, and porous-material analysis.
- Current ownership:
  The crate already owns periodic framework types, symmetry analysis, density-grid and histogram
  surfaces, and GCMC-facing contracts.
- Status:
  This is useful Rust-owned periodic science, but it should not be documented as if it were already
  the whole `RASPA3-main` simulation stack.

## Stable Idea

The durable concept is to import the high-value deterministic periodic-science kernels into Rust
without swallowing every runtime concern at once.

That means `patina-raspa` is best understood as:

- a domain layer for framework science
- a place for periodic symmetry and property kernels
- a future home for deeper adsorption and porous-material logic

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

For the materials audience, this lane matters because periodic frameworks need tools that are not
the same as 0D cluster search: symmetry, pore-like property summaries, adsorption observables, and
ensemble-aware reasoning.

{{#endtab}}
{{#tab name="Software Lens"}}

For the software audience, the important point is scope discipline:

- keep framework science in `patina-raspa`
- keep general runtime and backend policy at explicit boundaries
- avoid presenting a partial import as if it already owned every upstream capability

{{#endtab}}
{{#tab name="Bridge"}}

The bridge is long-lived periodic reporting. Framework science benefits from the same explicit
ownership and provenance discipline as cluster science.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- periodic framework representation
- symmetry analysis
- density-grid and histogram property engines
- GCMC-facing request and result contracts

Deferred:

- a broader forcefield or adapter layer
- full runtime breadth comparable to the full `RASPA3-main` stack
- casual claims of SCOTT workflow parity from this lane alone
