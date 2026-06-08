# 6.6.1 ARVO-Derived Surface Geometry

<p class="lead">
  `patina-arvo` is the planned Rust-owned lane for sphere-based surface and volume analysis of
  cluster-like structures, with the eventual goal of making surface reporting a typed scientific
  capability rather than an opaque side utility.
</p>

## At A Glance

- Scientific role:
  Surface area, enclosed volume, and surface energy for cluster-like systems.
- Current ownership:
  Typed domain models already exist. The current crate owns `Sphere`, `SphereCloud`,
  `SurfaceGeometrySummary`, and surface-energy summaries, with validation rules already present in
  Rust.
- Status:
  The public API skeleton exists, but the actual sphere-union geometry port from `arvo_c` is not
  yet complete.

## Stable Idea

The stable value of this lane is not the exact current file layout. It is the scientific contract:

- represent a structure as a validated sphere cloud
- compute area and volume from that geometry
- derive surface-energy quantities from explicit energy and area inputs

That is a good fit for `patina` because it keeps a common reporting need inside typed Rust data
models.

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

For materials chemistry, this lane is about turning shape intuition into explicit observables:

- how much surface is exposed
- how that surface changes between candidate motifs
- how excess energy scales against area rather than only atom count

{{#endtab}}
{{#tab name="Software Lens"}}

For software architecture, the key rule is that surface analysis should become a reusable kernel,
not a hidden post-processing script with ad hoc assumptions.

{{#endtab}}
{{#tab name="Bridge"}}

The bridge is scientific reporting discipline. A surface-energy number is only trustworthy when the
geometry assumptions that produced the area are explicit.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- sphere-cloud validation
- typed surface-energy calculation from explicit inputs
- public domain types that can support later geometry work

Still deferred:

- the full ARVO sphere geometry kernel
- workflow-wide integration across all intended `patina-tools` surfaces
- any claim that `patina-arvo` is already a full replacement for legacy surface utilities
