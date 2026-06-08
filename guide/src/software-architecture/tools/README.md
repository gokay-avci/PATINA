# 6.6 Tooling Landscape

> [!NOTE]
> These chapters describe stable meaning and ownership boundaries. They are not blanket claims of
> full upstream parity.

<p class="lead">
  The external-tools landscape in `patina` should be explained as a set of explicit scientific and
  software boundaries: some lanes are Rust-owned ports of useful ideas, some are adapter surfaces
  around external runtimes, and some are deliberate inspiration targets rather than literal clones.
</p>

## Chapters

| Chapter | Stable Meaning |
| --- | --- |
| [ARVO-Derived Surface Geometry](arvo.html) | Future Rust-owned sphere-based area, volume, and surface-energy interpretation for cluster-like systems. |
| [Dreadnaut And Topology Identity](dreadnaut.html) | Exact graph-isomorphism lane for native-compatible hashkeys, duplicate control, and topology-aware reporting. |
| [Autoemulate Surrogate Runtime](autoemulate.html) | Advisory surrogate-learning lane with Rust orchestration and an explicit Python runtime boundary. |
| [Goedecker-Inspired Perturber Stack](perturber-stack.html) | Perturbation, environment fingerprints, and assignment-based structure distance without collapsing structure semantics into descriptors. |
| [RASPA3-Inspired Periodic Work](raspa-inspired.html) | Real Rust-owned periodic framework and adsorption lane, narrower than the full `RASPA3-main` capability surface. |
| [stk-Inspired Construction Kernel](stk-inspired.html) | Rust-native supramolecular construction kernel that keeps RDKit at an explicit adapter edge. |
| [SYVA Cluster Symmetry Lane](syva.html) | Rust-owned 0D cluster symmetry port with workflow wiring and explicit scope limits. |

The common design rule across all of these chapters is simple:

- keep stable scientific meaning visible
- keep external runtime and adapter boundaries explicit
- avoid documenting unstable file layout as if it were the public architecture
