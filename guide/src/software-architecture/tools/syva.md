# 6.6.7 SYVA Cluster Symmetry Lane

<p class="lead">
  `patina-syva` is the Rust-owned 0D cluster symmetry lane in `patina`, ported from the useful core of
  `Analysis-Toolkit-master/SYVA` and already exposed through workflow tooling.
</p>

## At A Glance

- Scientific role:
  Point groups, symmetry operations, and symmetrized geometry.
- Current ownership:
  The crate owns input parsing, preprocessing, symmetry search, classification, operation
  summaries, and subgroup-driven symmetrized geometry over the bundled SYVA fixture corpus.
- Status:
  The lane is already wired into `patina-tools cluster-symmetry`, while broader replacement across
  every old call site remains deliberately deferred.

## Stable Idea

The durable scientific value is a Rust-owned symmetry analysis path for clusters:

- parse and normalize representative inputs
- classify point-group and framework-group behavior
- summarize operations and subgroup structure
- produce symmetrized geometry where the current Rust path supports it

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

For chemistry readers, this lane matters because cluster symmetry is not just decoration. It helps
explain motif families, degeneracy, subgroup relations, and the meaning of small distortions.

{{#endtab}}
{{#tab name="Software Lens"}}

For software readers, the important result is ownership: a useful symmetry kernel now lives in Rust
with bundled fixtures and explicit scope, rather than only behind an opaque legacy executable.

{{#endtab}}
{{#tab name="Bridge"}}

The bridge is auditability. Symmetry assignments are far easier to trust when the fixture base,
scope limits, and transformation summaries are explicit.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- 0D cluster symmetry path
- bundled upstream fixture coverage and parity-oriented tests
- workflow exposure through `patina-tools cluster-symmetry`

Explicitly kept elsewhere:

- periodic framework symmetry, which belongs to the `patina-raspa` lane
- legacy reporting/export surfaces that are not required for the current Rust-owned analysis path

## Live Command Example

This book can embed build-time CLI output with `mdbook-cmdrun`. The block below is generated during
`mdbook build` by running `patina-tools cluster-symmetry` on a small water geometry shipped with the
book.

> [!NOTE]
> If the toolchain is configured correctly, the next section appears automatically during book
> generation and shows a real `patina-tools cluster-symmetry` report.

<!-- cmdrun --strict bash ../../../scripts/render-syva-water-example.sh -->
