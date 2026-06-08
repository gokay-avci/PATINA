# 6.6.6 stk-Inspired Construction Kernel

<p class="lead">
  `patina-stk` is the Rust-native supramolecular construction lane in `patina`, inspired by the useful
  kernel ideas in `stk` while remaining aligned with the workspace's hexagonal architecture and
  explicit Python-runtime boundaries.
</p>

## At A Glance

- Scientific role:
  Topology-guided assembly and construction.
- Current ownership:
  The crate owns a domain-first construction kernel and a dedicated Python runtime project intended
  for the workspace-managed `venvs/stk` environment.
- Status:
  Inspired by `stk`, not a literal port. The project preserves topology and construction ideas
  while leaving heavy external services and broad API parity out of the first campaign.

## Stable Idea

The lasting idea from `stk` is a construction kernel:

- topology graphs define assembly intent
- construction state tracks staged placement
- chemistry-toolkit services sit behind explicit ports
- lightweight optimization stays optional rather than fused into the domain

That idea survives even if the exact module layout changes later.

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

For chemistry readers, this lane is about making supramolecular construction and topology-aware
assembly understandable without having to read a Python-heavy object model first.

{{#endtab}}
{{#tab name="Software Lens"}}

For software readers, the crucial design rule is that RDKit and similar tools should remain at an
adapter edge. The core construction model should stay testable and Rust-owned.

{{#endtab}}
{{#tab name="Bridge"}}

The bridge is reuse. A typed construction kernel can support real scientific assembly workflows
without tying the rest of `patina` to one upstream package's full API surface.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- the crate boundary and current scope definition
- the Rust-first topology and construction direction
- the dedicated Python runtime project under `crates/patina-stk/python`

Intentionally not the first target:

- broad Python API parity
- reaction-factory parity
- evolutionary-algorithm parity
- MongoDB and other heavy external services
