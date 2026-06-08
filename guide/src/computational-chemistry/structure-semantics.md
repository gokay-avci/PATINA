# 5.1 Structure Semantics

One of the most durable ideas in `patina` is that not every atomic structure should be treated as
the same kind of object.

That is why the workspace is moving toward a scientific-kernel view in which validated structures
are typed by dimensionality and periodicity instead of being passed around only as generic records.

## Why This Matters

If a 0D cluster, a 2D slab, and a 3D framework are all treated as the same shape everywhere, then:

- conversions become ambiguous
- scientific assumptions hide inside adapters
- duplicate logic and normalization can silently become wrong
- readers cannot tell whether a result is exact, inferred, or lossy

The stable long-term direction, reflected in the `patina-sci-kernel` planning documents, is to
distinguish between:

- transport records for workflows
- validated scientific structures
- codecs and file representations

## Working Vocabulary

The vocabulary that is likely to stay is:

- `Cluster0D`
- `Wire1D`
- `Slab2D`
- `Framework3D`
- `PeriodicAxes`
- `Lattice3`
- `CoordinateBasis`

Those names may move modules over time, but the distinction they express is architecturally and
scientifically important.

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

From the scientific side, this typing says:

- a cluster is finite and non-periodic
- a slab has physically meaningful periodicity only in selected axes
- a framework is genuinely periodic in three dimensions
- coordinate and lattice metadata are part of the scientific meaning, not mere storage detail

That distinction protects analyses such as normalization, collapse detection, and structure
comparison from accidentally assuming full periodicity when only partial periodicity is present.

{{#endtab}}
{{#tab name="Software Lens"}}

From the software side, this typing says:

- do not let every crate reinvent its own almost-equivalent structure model
- keep file codecs separate from scientific transforms
- represent lossy or inferred conversion steps explicitly

This is the cleanest way to prevent backend-specific assumptions from leaking into the shared
scientific core.

{{#endtab}}
{{#tab name="Bridge"}}

The bridge rule is:

- `Candidate` remains useful as a workflow DTO
- validated structure semantics belong in the scientific kernel
- conversions should report what was parsed exactly, what was inferred, and what may have been lost

That is the basis for a reporting system that can still be audited after future refactors.

{{#endtab}}
{{#endtabs}}

## Public Book Posture

The book should avoid promising a frozen internal module map here. What it should promise is the
scientific contract:

- dimensionality matters
- periodic axes matter
- conversion policy matters
- inferred structure metadata should not be smuggled in silently
